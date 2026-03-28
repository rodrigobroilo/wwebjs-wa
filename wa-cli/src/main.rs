use base64::Engine;
use chrono::Utc;
use clap::{Parser, Subcommand};
use reqwest::blocking::Client;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::process;

#[derive(Parser)]
#[command(
    name = "wa",
    version = "1.0.0",
    about = "WhatsApp CLI — send messages, media, reactions, search history, process media, sync contacts, and poll for new messages. All via the wwebjs-wa API.",
    after_help = "EXAMPLES:\n  wa send --to 5511999999999 -m \"Hello!\"\n  wa send --group \"Family Chat\" -m \"Good morning!\"\n  wa send-media --to 5511999999999 --file photo.jpg --mimetype image/jpeg\n  wa react --message-id ABC123 --reaction 👍\n  wa search -q \"flight\" --limit 10\n  wa process-media MSG_ID_123\n  wa sync-contacts\n  wa poll\n\nEXIT CODES:\n  0  Success (message sent, poll found nothing, etc.)\n  1  Error (send failed, API unreachable, etc.)\n  2  Error (parse failure, DB error, etc.)\n  3  DEDUP_SKIP — message was already sent/replied/reacted. Not an error, just a no-op.\n     Callers should treat exit 3 as success (the work was already done).\n\nENVIRONMENT:\n  WWEBJS_URL              wwebjs-wa server URL (required)\n  WWEBJS_API_KEY          API key for authentication\n  CF_ACCESS_CLIENT_ID     Cloudflare Access client ID\n  CF_ACCESS_CLIENT_SECRET Cloudflare Access client secret\n  DB_PATH                 SQLite database path\n  BOT_PREFIX              Optional prefix for sent messages (default: none)\n  BOT_PHONE               Bot's phone number (for poll directed-at-bot detection)\n  BOT_NAME                Bot's display name (for poll mention detection)\n  GROQ_API_KEY            Groq API key (for process-media transcription/vision)"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Send a text message to a person or group
    Send {
        /// Phone number (e.g. 5511999999999)
        #[arg(long)]
        to: Option<String>,
        /// Group name (exact match)
        #[arg(long)]
        group: Option<String>,
        /// Message text
        #[arg(long, short)]
        message: String,
        /// Reply to a specific message ID
        #[arg(long)]
        quoted_id: Option<String>,
    },
    /// Send media (image, document, audio, video) to a person or group
    SendMedia {
        /// Phone number
        #[arg(long)]
        to: Option<String>,
        /// Group name
        #[arg(long)]
        group: Option<String>,
        /// Path to the file to send
        #[arg(long)]
        file: String,
        /// MIME type (e.g. image/jpeg, application/pdf)
        #[arg(long)]
        mimetype: String,
        /// Optional caption text
        #[arg(long)]
        caption: Option<String>,
        /// Send as document attachment instead of inline media
        #[arg(long)]
        as_document: bool,
    },
    /// React to a message with an emoji
    React {
        /// Message ID to react to
        #[arg(long)]
        message_id: String,
        /// Emoji reaction (e.g. 👍 ❤️ 😂)
        #[arg(long)]
        reaction: String,
    },
    /// Search message history in the local database
    Search {
        /// Search query (matches message body)
        #[arg(long, short)]
        query: String,
        /// Max results to return
        #[arg(long, default_value = "20")]
        limit: usize,
        /// Filter by sender phone or name
        #[arg(long)]
        from: Option<String>,
    },
    /// Log a sent message to the database (used by external scripts)
    Log {
        /// Message ID from the API response
        #[arg(long)]
        message_id: String,
        /// Recipient phone or group name
        #[arg(long)]
        to: String,
        /// Message body text
        #[arg(long)]
        body: String,
        /// Message type: text, media, reaction
        #[arg(long, default_value = "text")]
        msg_type: String,
    },
    /// Download and process media: transcribe audio (Whisper), describe images (Llama 4), store in DB
    ProcessMedia {
        /// Message ID containing the media
        message_id: String,
        /// Language for audio transcription (ISO 639-1)
        #[arg(long, default_value = "pt")]
        lang: String,
    },
    /// Sync contacts and groups from the wwebjs-wa server to the local database
    SyncContacts,
    /// Poll for new messages, deduplicate, react to directed messages, output actionable JSON lines
    #[command(after_help = "EXIT CODES:\n  0  No new actionable messages\n  1  New actionable messages found (output as JSON lines)\n  2  Error (API unreachable, parse failure, etc.)")]
    Poll {
        /// Max messages to fetch from server buffer
        #[arg(long, default_value = "50")]
        limit: u32,
    },
}

struct ApiConfig {
    base_url: String,
    headers: Vec<(String, String)>,
    client: Client,
    db_path: String,
    bot_prefix: String,
}

impl ApiConfig {
    fn from_env() -> Self {
        let base_url = env::var("WWEBJS_URL").unwrap_or_else(|_| "https://localhost:3000".into());
        let api_key = env::var("WWEBJS_API_KEY").unwrap_or_default();
        let cf_id = env::var("CF_ACCESS_CLIENT_ID").unwrap_or_default();
        let cf_secret = env::var("CF_ACCESS_CLIENT_SECRET").unwrap_or_default();
        let user_agent = env::var("USER_AGENT").unwrap_or_else(|_| {
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36".into()
        });
        let db_path = env::var("DB_PATH").unwrap_or_else(|_| {
            format!(
                "{}/workspace/cron-state/whatsapp.db",
                env::var("HOME").unwrap_or_default()
            )
        });
        let bot_prefix =
            env::var("BOT_PREFIX").unwrap_or_default();

        let headers = vec![
            ("X-Api-Key".into(), api_key),
            ("CF-Access-Client-Id".into(), cf_id),
            ("CF-Access-Client-Secret".into(), cf_secret),
            ("User-Agent".into(), user_agent.clone()),
            ("Content-Type".into(), "application/json".into()),
        ];

        let client = Client::builder()
            .user_agent(&user_agent)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            base_url,
            headers,
            client,
            db_path,
            bot_prefix,
        }
    }

    fn open_db(&self) -> Result<Connection, String> {
        let conn = Connection::open(&self.db_path)
            .map_err(|e| format!("Failed to open DB: {}", e))?;
        // WAL mode for concurrent access (multiple wa subcommands in parallel)
        let _ = conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;");

        // Apply schema from external SQL file — REQUIRED
        let schema_path = env::var("WA_SCHEMA_PATH").unwrap_or_else(|_| {
            let home = env::var("HOME").unwrap_or_default();
            let candidates = [
                format!("{}/workspace/config/wa-schema.sql", home),
                "config/wa-schema.sql".to_string(),
                "wa-schema.sql".to_string(),
            ];
            for p in &candidates {
                if std::path::Path::new(p).exists() {
                    return p.clone();
                }
            }
            String::new()
        });

        if schema_path.is_empty() {
            return Err(
                "DB schema file not found! Searched: $WA_SCHEMA_PATH, ~/workspace/config/wa-schema.sql, ./config/wa-schema.sql, ./wa-schema.sql. \
                 Fix: Set WA_SCHEMA_PATH in your .env file."
                .to_string()
            );
        }

        let sql = fs::read_to_string(&schema_path).map_err(|e| {
            format!("Schema file '{}' unreadable: {}. Fix: Check permissions.", schema_path, e)
        })?;

        conn.execute_batch(&sql).map_err(|e| {
            format!("Schema file '{}' has invalid SQL: {}. Fix: Check syntax.", schema_path, e)
        })?;

        Ok(conn)
    }

    /// Open DB or exit with clear error. Use this in all commands that need DB.
    fn require_db(&self) -> Connection {
        match self.open_db() {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!("❌ {}", e);
                process::exit(2);
            }
        }
    }

    fn post(&self, path: &str, body: &impl Serialize) -> Result<String, String> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.client.post(&url).json(body);
        for (k, v) in &self.headers {
            req = req.header(k.as_str(), v.as_str());
        }
        match req.send() {
            Ok(resp) => {
                let status = resp.status();
                let text = resp.text().unwrap_or_default();
                if status.is_success() {
                    Ok(text)
                } else {
                    Err(format!("HTTP {}: {}", status, text))
                }
            }
            Err(e) => Err(format!("Request failed: {}", e)),
        }
    }

    fn get(&self, path: &str) -> Result<String, String> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.client.get(&url);
        for (k, v) in &self.headers {
            req = req.header(k.as_str(), v.as_str());
        }
        match req.send() {
            Ok(resp) => {
                let status = resp.status();
                let text = resp.text().unwrap_or_default();
                if status.is_success() {
                    Ok(text)
                } else {
                    Err(format!("HTTP {}: {}", status, text))
                }
            }
            Err(e) => Err(format!("Request failed: {}", e)),
        }
    }

    fn log_sent(&self, msg_id: &str, to: &str, body: &str, msg_type: &str, quoted_id: Option<&str>, media_mime: Option<&str>, media_file: Option<&str>) {
        let conn = self.require_db();
        let now = Utc::now().to_rfc3339();
        // Determine if target is a group (contains @g.us or is a name with spaces)
        let (to_phone, to_group) = if to.contains("@g.us") || to.contains(" ") {
            ("".to_string(), to.to_string())
        } else {
            (to.to_string(), "".to_string())
        };
        if let Err(e) = conn.execute(
            "INSERT INTO sent_messages (id, to_phone, to_group, body, timestamp, platform, created_at, quoted_msg_id, message_type, media_mimetype, media_filename)
             VALUES (?1, ?2, ?3, ?4, ?5, 'whatsapp', ?6, ?7, ?8, ?9, ?10)",
            params![msg_id, to_phone, to_group, body, now, now, quoted_id.unwrap_or(""), msg_type, media_mime.unwrap_or(""), media_file.unwrap_or("")],
        ) {
            eprintln!("❌ Failed to log sent message to DB: {}", e);
        }
    }
}

fn cmd_send(config: &ApiConfig, to: Option<String>, group: Option<String>, message: String, quoted_id: Option<String>) {
    // DEDUP: If replying to a specific message, check if we already replied to it
    if let Some(ref qid) = quoted_id {
        if !qid.is_empty() {
            let conn = config.require_db();
            let already_replied: bool = conn.query_row(
                "SELECT 1 FROM sent_messages WHERE quoted_msg_id = ?1 LIMIT 1",
                params![qid],
                |_| Ok(true),
            ).unwrap_or(false);
            if already_replied {
                println!("DEDUP_SKIP: Already replied to message {}", qid);
                process::exit(3);
            }
        }
    }

    let prefixed = format!("{}{}", config.bot_prefix, message);

    // DEDUP: Check if we sent a very similar message to the same target in the last 60 seconds
    {
        let check_target = if let Some(g) = &group { g.clone() } else if let Some(t) = &to { t.clone() } else { String::new() };
        let conn = config.require_db();
        let recent_dup: bool = conn.query_row(
            "SELECT 1 FROM sent_messages WHERE to_phone = ?1 AND body = ?2 AND timestamp > datetime('now', '-60 seconds') LIMIT 1",
            params![check_target, prefixed],
            |_| Ok(true),
        ).unwrap_or(false);
        if recent_dup {
            println!("DEDUP_SKIP: Identical message already sent to {} in last 60s", check_target);
            process::exit(3);
        }
    }

    let (endpoint, target) = if let Some(g) = &group {
        ("/send-group", g.clone())
    } else if let Some(t) = &to {
        ("/send", t.clone())
    } else {
        eprintln!("Error: --to or --group required");
        process::exit(1);
    };

    let mut body = serde_json::json!({ "message": prefixed });
    if group.is_some() {
        body["group_name"] = serde_json::Value::String(target.clone());
    } else {
        body["to"] = serde_json::Value::String(target.clone());
    }
    if let Some(qid) = &quoted_id {
        body["quotedMessageId"] = serde_json::Value::String(qid.clone());
    }

    match config.post(endpoint, &body) {
        Ok(resp) => {
            // Try to extract message ID from response
            let msg_id = serde_json::from_str::<serde_json::Value>(&resp)
                .ok()
                .and_then(|v| v["id"].as_str().map(String::from))
                .unwrap_or_default();
            config.log_sent(&msg_id, &target, &prefixed, "text", quoted_id.as_deref(), None, None);
            println!("✅ Sent to {}", target);
        }
        Err(e) => {
            eprintln!("❌ Send failed: {}", e);
            process::exit(1);
        }
    }
}

fn cmd_send_media(config: &ApiConfig, to: Option<String>, group: Option<String>, file: String, mimetype: String, caption: Option<String>, as_document: bool) {
    let file_data = match fs::read(&file) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to read file {}: {}", file, e);
            process::exit(1);
        }
    };

    let b64 = base64::engine::general_purpose::STANDARD.encode(&file_data);
    let filename = std::path::Path::new(&file)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".into());

    let cap = caption.as_deref().map(|c| format!("{}{}", config.bot_prefix, c));

    // DEDUP: Check if identical media+caption was sent to same target in last 60 seconds
    {
        let check_target = if let Some(g) = &group { g.clone() } else if let Some(t) = &to { t.clone() } else { String::new() };
        let check_body = cap.as_deref().unwrap_or("");
        let conn = config.require_db();
        let recent_dup: bool = conn.query_row(
            "SELECT 1 FROM sent_messages WHERE to_phone = ?1 AND media_filename = ?2 AND timestamp > datetime('now', '-60 seconds') LIMIT 1",
            params![check_target, filename],
            |_| Ok(true),
        ).unwrap_or(false);
        if recent_dup {
            println!("DEDUP_SKIP: Same media already sent to {} in last 60s", check_target);
            process::exit(3);
        }
        let _ = check_body; // suppress unused warning
    }

    let (endpoint, target) = if let Some(g) = &group {
        ("/send-media", g.clone())
    } else if let Some(t) = &to {
        ("/send-media", t.clone())
    } else {
        eprintln!("Error: --to or --group required");
        process::exit(1);
    };

    let mut body = serde_json::json!({
        "data": b64,
        "mimetype": mimetype,
        "filename": filename,
    });

    if let Some(c) = &cap {
        body["caption"] = serde_json::Value::String(c.clone());
    }
    if as_document {
        body["sendAsDocument"] = serde_json::Value::Bool(true);
    }
    if group.is_some() {
        body["group_name"] = serde_json::Value::String(target.clone());
    } else {
        body["to"] = serde_json::Value::String(target.clone());
    }

    match config.post(endpoint, &body) {
        Ok(resp) => {
            let msg_id = serde_json::from_str::<serde_json::Value>(&resp)
                .ok()
                .and_then(|v| v["id"].as_str().map(String::from))
                .unwrap_or_default();
            config.log_sent(&msg_id, &target, cap.as_deref().unwrap_or(""), "media", None, Some(&mimetype), Some(&filename));
            println!("✅ Media sent to {}", target);
        }
        Err(e) => {
            eprintln!("❌ Send media failed: {}", e);
            process::exit(1);
        }
    }
}

fn cmd_react(config: &ApiConfig, message_id: String, reaction: String) {
    // DEDUP: Check if we already reacted to this message
    let conn = config.require_db();
    let already: bool = conn.query_row(
        "SELECT 1 FROM reactions_sent WHERE message_id = ?1",
        params![message_id],
        |_| Ok(true),
    ).unwrap_or(false);
    if already {
        println!("DEDUP_SKIP: Already reacted to message {}", message_id);
        process::exit(3);
    }

    let body = serde_json::json!({
        "messageId": message_id,
        "emoji": reaction,
    });

    match config.post(&format!("/react/{}", message_id), &body) {
        Ok(_) => {
            // Log reaction to DB
            let conn = config.require_db();
            let now = Utc::now().to_rfc3339();
            let _ = conn.execute(
                "INSERT OR IGNORE INTO reactions_sent (message_id, reaction, timestamp) VALUES (?1, ?2, ?3)",
                params![message_id, reaction, now],
            );
            println!("✅ Reacted {} to {}", reaction, message_id);
        }
        Err(e) => {
            eprintln!("❌ React failed: {}", e);
            process::exit(1);
        }
    }
}

fn cmd_search(config: &ApiConfig, query: String, limit: usize, from: Option<String>) {
    let conn = match config.open_db() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to open DB: {}", e);
            process::exit(1);
        }
    };

    let pattern = format!("%{}%", query);
    let from_pattern = from.map(|f| format!("%{}%", f));

    let mut rows: Vec<(String, String, String, String, String, bool, String)> = Vec::new();

    if let Some(ref fp) = from_pattern {
        let sql = "SELECT message_id, from_number, from_name, body, timestamp, is_group, group_name
         FROM messages WHERE body LIKE ?1 AND (from_number LIKE ?2 OR from_name LIKE ?2)
         ORDER BY timestamp DESC LIMIT ?3";
        if let Ok(mut stmt) = conn.prepare(sql) {
            let mapped = stmt.query_map(params![pattern, fp, limit], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?))
            });
            if let Ok(iter) = mapped {
                for r in iter.flatten() {
                    rows.push(r);
                }
            }
        }
    } else {
        let sql = "SELECT message_id, from_number, from_name, body, timestamp, is_group, group_name
         FROM messages WHERE body LIKE ?1
         ORDER BY timestamp DESC LIMIT ?2";
        if let Ok(mut stmt) = conn.prepare(sql) {
            let mapped = stmt.query_map(params![pattern, limit], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?))
            });
            if let Ok(iter) = mapped {
                for r in iter.flatten() {
                    rows.push(r);
                }
            }
        }
    }

    if rows.is_empty() {
        println!("No messages found matching \'{}\'", query);
        return;
    }

    for (_msg_id, from_num, from_name, body, ts, is_group, group) in &rows {
        let loc = if *is_group { format!("[{}]", group) } else { String::new() };
        let display_body: String = if body.len() > 100 { format!("{}...", &body[..100]) } else { body.clone() };
        println!("{} {} {} {}: {}", ts, loc, from_name, from_num, display_body);
    }
    println!("\n{} result(s)", rows.len());
}

fn cmd_log(config: &ApiConfig, message_id: String, to: String, body: String, msg_type: String) {
    config.log_sent(&message_id, &to, &body, &msg_type, None, None, None);
    println!("✅ Logged message {}", message_id);
}

fn cmd_process_media(config: &ApiConfig, message_id: String, lang: String) {
    let groq_key = env::var("GROQ_API_KEY").unwrap_or_default();
    if groq_key.is_empty() {
        eprintln!("❌ GROQ_API_KEY not set");
        process::exit(1);
    }

    // Download media from wwebjs
    let media_resp = match config.get(&format!("/media/{}", message_id)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("❌ Failed to download media: {}", e);
            process::exit(1);
        }
    };

    let media: serde_json::Value = match serde_json::from_str(&media_resp) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("❌ Failed to parse media response: {}", e);
            process::exit(1);
        }
    };

    if media.get("error").is_some() {
        eprintln!("❌ Media error: {}", media["error"]);
        process::exit(1);
    }

    let mimetype = media["mimetype"].as_str().unwrap_or("");
    let b64_data = media["data"].as_str().unwrap_or("");
    let filename = media["filename"].as_str().unwrap_or("");
    let size = media["size"].as_u64().unwrap_or(0);

    let (description, transcript) = if mimetype.starts_with("image/") {
        // Image → Llama 4 Scout vision
        let desc = describe_image_groq(&config.client, &groq_key, b64_data, mimetype);
        (desc, None)
    } else if mimetype.starts_with("audio/") || mimetype.contains("ogg") {
        // Audio → Whisper transcription
        let text = transcribe_audio_groq(&config.client, &groq_key, b64_data, mimetype, &lang);
        let desc = format!("🎤 Voice note: \"{}\"", text);
        (desc, Some(text))
    } else if mimetype.starts_with("video/") {
        // Video → just log metadata (no ffmpeg in Rust)
        (format!("🎥 Video: {} ({})", filename, format_size(size)), None)
    } else {
        (format!("📎 Document: {} ({})", filename, mimetype), None)
    };

    // Update DB
    let conn = config.require_db();
    {
        let now = Utc::now().to_rfc3339();
        let _ = conn.execute(
            "UPDATE messages SET ai_description = ?1, transcript = ?2, media_mimetype = ?3, media_filename = ?4, media_size = ?5, media_description = ?6 WHERE id = ?7",
            params![description, transcript.as_deref().unwrap_or(""), mimetype, filename, size as i64, description, message_id],
        );
        let _ = conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS media_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                message_id TEXT, mimetype TEXT, filename TEXT, size_bytes INTEGER,
                ai_description TEXT, transcription TEXT, timestamp TEXT, created_at TEXT
            );"
        );
        let _ = conn.execute(
            "INSERT INTO media_log (message_id, mimetype, filename, size_bytes, ai_description, transcription, timestamp, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![message_id, mimetype, filename, size as i64, description, transcript.as_deref().unwrap_or(""), now, now],
        );
    }

    let result = serde_json::json!({
        "message_id": message_id,
        "mimetype": mimetype,
        "filename": filename,
        "size": size,
        "description": description,
        "transcript": transcript,
    });
    println!("{}", serde_json::to_string_pretty(&result).unwrap_or_default());
}

fn describe_image_groq(client: &Client, api_key: &str, b64_data: &str, mimetype: &str) -> String {
    let data_url = format!("data:{};base64,{}", mimetype, b64_data);
    let payload = serde_json::json!({
        "model": "meta-llama/llama-4-scout-17b-16e-instruct",
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": "Describe this image concisely in 1-2 sentences. Include key details: people, objects, text, location, mood. If there's text in the image, include it verbatim in quotes."},
                {"type": "image_url", "image_url": {"url": data_url}}
            ]
        }],
        "max_tokens": 200
    });

    match client.post("https://api.groq.com/openai/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
    {
        Ok(resp) => {
            let data: serde_json::Value = resp.json().unwrap_or_default();
            if data.get("error").is_some() {
                format!("[Vision error: {}]", data["error"]["message"].as_str().unwrap_or("unknown"))
            } else {
                data["choices"][0]["message"]["content"].as_str().unwrap_or("[No description]").trim().to_string()
            }
        }
        Err(e) => format!("[Vision request failed: {}]", e),
    }
}

fn transcribe_audio_groq(client: &Client, api_key: &str, b64_data: &str, mimetype: &str, lang: &str) -> String {
    let decoded = match base64::engine::general_purpose::STANDARD.decode(b64_data) {
        Ok(d) => d,
        Err(e) => return format!("[Decode error: {}]", e),
    };

    let ext = if mimetype.contains("ogg") { "ogg" } else if mimetype.contains("mp3") || mimetype.contains("mpeg") { "mp3" } else { "wav" };
    let fname = format!("audio.{}", ext);

    let form = reqwest::blocking::multipart::Form::new()
        .text("model", "whisper-large-v3")
        .text("language", lang.to_string())
        .text("response_format", "json")
        .part("file", reqwest::blocking::multipart::Part::bytes(decoded)
            .file_name(fname)
            .mime_str(mimetype).unwrap_or_else(|_| reqwest::blocking::multipart::Part::bytes(vec![]).mime_str("audio/ogg").unwrap()));

    match client.post("https://api.groq.com/openai/v1/audio/transcriptions")
        .header("Authorization", format!("Bearer {}", api_key))
        .multipart(form)
        .send()
    {
        Ok(resp) => {
            let data: serde_json::Value = resp.json().unwrap_or_default();
            data["text"].as_str().unwrap_or("[Transcription failed]").trim().to_string()
        }
        Err(e) => format!("[Transcription request failed: {}]", e),
    }
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 { format!("{} B", bytes) }
    else if bytes < 1024 * 1024 { format!("{:.1} KB", bytes as f64 / 1024.0) }
    else { format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0)) }
}

fn cmd_sync_contacts(config: &ApiConfig) {
    // Fetch all chats
    let chats_resp = match config.get("/chats") {
        Ok(r) => r,
        Err(e) => {
            eprintln!("❌ Failed to fetch chats: {}", e);
            process::exit(1);
        }
    };

    let chats_data: serde_json::Value = match serde_json::from_str(&chats_resp) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("❌ Failed to parse chats: {}", e);
            process::exit(1);
        }
    };

    let chats = if let Some(arr) = chats_data.as_array() {
        arr.clone()
    } else if let Some(arr) = chats_data["chats"].as_array() {
        arr.clone()
    } else {
        eprintln!("❌ Unexpected chats format");
        process::exit(1);
    };

    let conn = match config.open_db() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("❌ Failed to open DB: {}", e);
            process::exit(1);
        }
    };

    // Ensure tables exist
    let _ = conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS contacts (
            phone TEXT PRIMARY KEY, name TEXT, about TEXT, is_business INTEGER DEFAULT 0,
            updated_at TEXT
        );
        CREATE TABLE IF NOT EXISTS groups (
            id TEXT PRIMARY KEY, name TEXT, description TEXT, participant_count INTEGER,
            participants TEXT, admins TEXT, updated_at TEXT
        );
        CREATE TABLE IF NOT EXISTS change_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            entity_type TEXT, entity_id TEXT, field TEXT,
            old_value TEXT, new_value TEXT, changed_at TEXT
        );"
    );

    let now = Utc::now().to_rfc3339();
    let mut contact_count = 0u32;
    let mut group_count = 0u32;
    let mut changes = 0u32;

    for chat in &chats {
        let chat_id = chat["id"].as_str().unwrap_or("");
        let is_group = chat["isGroup"].as_bool().unwrap_or(false);

        if is_group {
            let gid = chat_id.replace("@g.us", "");
            if let Ok(resp) = config.get(&format!("/group/{}", gid)) {
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(&resp) {
                    let gdata = if data.get("group").is_some() { &data["group"] } else { &data };
                    let name = gdata["name"].as_str().unwrap_or("");
                    let desc = gdata["description"].as_str().or_else(|| gdata["desc"].as_str()).unwrap_or("");
                    let participants = gdata["participants"].as_array().map(|a| a.len() as i64).unwrap_or(0);
                    let parts_json = serde_json::to_string(&gdata["participants"]).unwrap_or_else(|_| "[]".into());

                    // Build admins list
                    let admins: Vec<&serde_json::Value> = gdata["participants"].as_array()
                        .map(|arr| arr.iter().filter(|p| p["isAdmin"].as_bool().unwrap_or(false) || p["isSuperAdmin"].as_bool().unwrap_or(false)).collect())
                        .unwrap_or_default();
                    let admins_json = serde_json::to_string(&admins).unwrap_or_else(|_| "[]".into());

                    // Check for existing
                    let existing: Option<String> = conn.query_row(
                        "SELECT name FROM groups WHERE id = ?1", params![chat_id], |row| row.get(0)
                    ).ok();

                    if let Some(old_name) = &existing {
                        if old_name != name && !name.is_empty() {
                            let _ = conn.execute(
                                "INSERT INTO change_log (entity_type, entity_id, field, old_value, new_value, changed_at) VALUES ('group', ?1, 'name', ?2, ?3, ?4)",
                                params![chat_id, old_name, name, now],
                            );
                            changes += 1;
                        }
                        let _ = conn.execute(
                            "UPDATE groups SET name=?1, description=?2, participant_count=?3, participants=?4, admins=?5, updated_at=?6 WHERE id=?7",
                            params![name, desc, participants, parts_json, admins_json, now, chat_id],
                        );
                    } else {
                        let _ = conn.execute(
                            "INSERT INTO groups (id, name, description, participant_count, participants, admins, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                            params![chat_id, name, desc, participants, parts_json, admins_json, now],
                        );
                        changes += 1;
                    }
                    group_count += 1;
                }
            }
        } else {
            let phone = chat_id.split('@').next().unwrap_or("");
            if phone.len() >= 8 && phone.chars().all(|c| c.is_ascii_digit()) {
                if let Ok(resp) = config.get(&format!("/contact/{}", phone)) {
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(&resp) {
                        let cdata = if data.get("contact").is_some() { &data["contact"] } else { &data };
                        let name = cdata["name"].as_str()
                            .or_else(|| cdata["pushname"].as_str())
                            .or_else(|| cdata["shortName"].as_str())
                            .unwrap_or("");
                        let about = cdata["about"].as_str().unwrap_or("");
                        let is_biz = if cdata["isBusiness"].as_bool().unwrap_or(false) { 1i32 } else { 0 };

                        let existing: Option<String> = conn.query_row(
                            "SELECT name FROM contacts WHERE phone = ?1", params![phone], |row| row.get(0)
                        ).ok();

                        if let Some(old_name) = &existing {
                            if old_name != name && !name.is_empty() && !old_name.is_empty() {
                                let _ = conn.execute(
                                    "INSERT INTO change_log (entity_type, entity_id, field, old_value, new_value, changed_at) VALUES ('contact', ?1, 'name', ?2, ?3, ?4)",
                                    params![phone, old_name, name, now],
                                );
                                changes += 1;
                            }
                            let _ = conn.execute(
                                "UPDATE contacts SET name=?1, about=?2, is_business=?3, updated_at=?4 WHERE phone=?5",
                                params![if name.is_empty() { old_name.as_str() } else { name }, about, is_biz, now, phone],
                            );
                        } else {
                            let _ = conn.execute(
                                "INSERT INTO contacts (phone, name, about, is_business, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                                params![phone, name, about, is_biz, now],
                            );
                            changes += 1;
                        }
                        contact_count += 1;
                    }
                }
            }
        }
    }

    println!("✅ Synced {} contacts, {} groups ({} changes)", contact_count, group_count, changes);
}

// --- Poll types and function ---

#[derive(Deserialize)]
struct PollApiResponse {
    #[serde(default)]
    messages: Vec<PollMessage>,
}

#[derive(Deserialize)]
struct PollMessage {
    #[serde(default)]
    id: String,
    #[serde(default)]
    from: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    timestamp: u64,
    #[serde(default, rename = "type")]
    msg_type: String,
    #[serde(default, rename = "hasMedia")]
    has_media: bool,
    #[serde(default, rename = "isGroup")]
    is_group: bool,
    #[serde(default, rename = "chatName")]
    chat_name: String,
    #[serde(default, rename = "contactName")]
    contact_name: String,
    #[serde(default, rename = "mentionedIds")]
    mentioned_ids: Vec<String>,
}

#[derive(Serialize)]
struct PollOutput {
    from: String,
    author: String,
    body: String,
    timestamp: u64,
    #[serde(rename = "messageId")]
    message_id: String,
    #[serde(rename = "hasMedia")]
    has_media: bool,
    #[serde(rename = "isGroup")]
    is_group: bool,
    #[serde(rename = "chatName")]
    chat_name: String,
    #[serde(rename = "contactName")]
    contact_name: String,
}

fn cmd_poll(config: &ApiConfig, limit: u32) {
    let bot_phone = env::var("BOT_PHONE").unwrap_or_default();
    let bot_name = env::var("BOT_NAME").unwrap_or_default();

    // Fetch messages
    let resp_text = match config.get(&format!("/messages?limit={}", limit)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to fetch messages: {}", e);
            process::exit(2);
        }
    };

    let api_resp: PollApiResponse = match serde_json::from_str(&resp_text) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Failed to parse response: {}", e);
            process::exit(2);
        }
    };

    let conn = match config.open_db() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to open DB: {}", e);
            process::exit(2);
        }
    };

    let _ = conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS seen_message_ids (
            message_id TEXT PRIMARY KEY,
            seen_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            chat_id TEXT,
            chat_name TEXT,
            sender TEXT,
            content TEXT,
            message_type TEXT,
            timestamp TEXT,
            is_from_me INTEGER DEFAULT 0,
            created_at TEXT,
            phone TEXT,
            chat_type TEXT,
            quoted_msg_id TEXT,
            is_group INTEGER DEFAULT 0,
            has_media INTEGER DEFAULT 0,
            contact_name TEXT,
            mentioned_ids TEXT
        );"
    );

    let mut actionable = Vec::new();

    for msg in &api_resp.messages {
        if msg.id.is_empty() { continue; }

        // Skip already seen
        let seen: bool = conn.query_row(
            "SELECT 1 FROM seen_message_ids WHERE message_id = ?1",
            params![msg.id], |_| Ok(true)
        ).unwrap_or(false);
        if seen { continue; }

        // Mark as seen
        let now = Utc::now().to_rfc3339();
        let _ = conn.execute(
            "INSERT OR IGNORE INTO seen_message_ids (message_id, seen_at) VALUES (?1, ?2)",
            params![msg.id, now],
        );

        // Log ALL incoming messages to messages table
        let is_from_me = if msg.id.starts_with("true_") || msg.author == "me" { 1 } else { 0 };
        let chat_type = if msg.is_group { "group" } else { "dm" };
        let mentioned = serde_json::to_string(&msg.mentioned_ids).unwrap_or_default();
        let _ = conn.execute(
            "INSERT OR IGNORE INTO messages (id, chat_id, chat_name, sender, content, message_type, timestamp, is_from_me, created_at, chat_type, is_group, has_media, contact_name, mentioned_ids)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                msg.id, msg.from, &msg.chat_name,
                if msg.contact_name.is_empty() { &msg.from } else { &msg.contact_name },
                msg.body, msg.msg_type, msg.timestamp.to_string(),
                is_from_me, now, chat_type, msg.is_group as i32,
                msg.has_media as i32, &msg.contact_name,
                mentioned
            ],
        );

        // Skip fromMe
        if msg.id.starts_with("true_") || msg.author == "me" { continue; }

        // Skip non-chat types
        let valid = ["chat", "image", "video", "audio", "ptt", "document", "sticker"];
        if !valid.contains(&msg.msg_type.as_str()) { continue; }

        // Skip bot's own messages (from bot phone number)
        if !bot_phone.is_empty() && msg.from.contains(&bot_phone) { continue; }

        // Send 👀 reaction if directed at bot
        let directed = if !msg.is_group {
            true
        } else {
            let body_lower = msg.body.to_lowercase();
            msg.mentioned_ids.iter().any(|m| !bot_phone.is_empty() && m.contains(&bot_phone))
                || (!bot_phone.is_empty() && msg.body.contains(&bot_phone))
                || (!bot_name.is_empty() && body_lower.contains(&bot_name.to_lowercase()))
        };

        if directed {
            // Check if we already reacted before sending
            let _ = conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS reactions_sent (
                    message_id TEXT NOT NULL,
                    reaction TEXT NOT NULL,
                    timestamp TEXT NOT NULL,
                    PRIMARY KEY (message_id, reaction)
                );"
            );
            let already_reacted: bool = conn.query_row(
                "SELECT 1 FROM reactions_sent WHERE message_id = ?1",
                params![msg.id],
                |_| Ok(true),
            ).unwrap_or(false);
            if !already_reacted {
                let payload = serde_json::json!({"emoji": "👀"});
                if config.post(&format!("/react/{}", msg.id), &payload).is_ok() {
                    let react_now = Utc::now().to_rfc3339();
                    let _ = conn.execute(
                        "INSERT OR IGNORE INTO reactions_sent (message_id, reaction, timestamp) VALUES (?1, ?2, ?3)",
                        params![msg.id, "👀", react_now],
                    );
                }
            }
        }

        actionable.push(PollOutput {
            from: msg.from.clone(),
            author: msg.author.clone(),
            body: msg.body.clone(),
            timestamp: msg.timestamp,
            message_id: msg.id.clone(),
            has_media: msg.has_media,
            is_group: msg.is_group,
            chat_name: msg.chat_name.clone(),
            contact_name: msg.contact_name.clone(),
        });
    }

    for msg in &actionable {
        if let Ok(json) = serde_json::to_string(msg) {
            println!("{}", json);
        }
    }

    // Exit code: 0 = nothing new, 1 = has actionable messages
    if !actionable.is_empty() {
        process::exit(1);
    }
}

fn main() {
    let cli = Cli::parse();
    let config = ApiConfig::from_env();

    match cli.command {
        Commands::Send { to, group, message, quoted_id } => {
            cmd_send(&config, to, group, message, quoted_id);
        }
        Commands::SendMedia { to, group, file, mimetype, caption, as_document } => {
            cmd_send_media(&config, to, group, file, mimetype, caption, as_document);
        }
        Commands::React { message_id, reaction } => {
            cmd_react(&config, message_id, reaction);
        }
        Commands::Search { query, limit, from } => {
            cmd_search(&config, query, limit, from);
        }
        Commands::Log { message_id, to, body, msg_type } => {
            cmd_log(&config, message_id, to, body, msg_type);
        }
        Commands::ProcessMedia { message_id, lang } => {
            cmd_process_media(&config, message_id, lang);
        }
        Commands::SyncContacts => {
            cmd_sync_contacts(&config);
        }
        Commands::Poll { limit } => {
            cmd_poll(&config, limit);
        }
    }
}
