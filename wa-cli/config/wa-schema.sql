-- WhatsApp DB Schema
-- Used by wa CLI binary to create/validate tables.
-- Edit this file to change the schema without recompiling.

CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    chat_id TEXT,
    chat_name TEXT,
    sender TEXT,
    content TEXT,
    message_type TEXT,
    media_type TEXT,
    media_description TEXT,
    timestamp TEXT,
    is_from_me INTEGER DEFAULT 0,
    created_at TEXT,
    phone TEXT,
    chat_type TEXT,
    quoted_msg_id TEXT,
    status TEXT,
    replied_with_id TEXT,
    replied_at TEXT,
    transcript TEXT,
    is_forwarded INTEGER DEFAULT 0,
    is_group INTEGER DEFAULT 0,
    mentioned_ids TEXT,
    has_media INTEGER DEFAULT 0,
    media_mimetype TEXT,
    media_filename TEXT,
    media_size INTEGER,
    media_url TEXT,
    contact_name TEXT,
    contact_number TEXT,
    ai_description TEXT
);

CREATE TABLE IF NOT EXISTS sent_messages (
    id TEXT,
    to_phone TEXT,
    to_group TEXT,
    body TEXT,
    timestamp TEXT,
    platform TEXT DEFAULT 'whatsapp',
    created_at TEXT,
    quoted_msg_id TEXT,
    chat_id TEXT,
    chat_name TEXT,
    message_type TEXT DEFAULT 'text',
    media_mimetype TEXT,
    media_filename TEXT,
    media_size INTEGER,
    reply_to_msg_id TEXT
);

CREATE TABLE IF NOT EXISTS seen_message_ids (
    message_id TEXT PRIMARY KEY,
    seen_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS reactions_sent (
    message_id TEXT NOT NULL,
    reaction TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    PRIMARY KEY (message_id, reaction)
);

CREATE TABLE IF NOT EXISTS contacts (
    id TEXT PRIMARY KEY,
    phone TEXT,
    name TEXT,
    short_name TEXT,
    push_name TEXT,
    is_business INTEGER DEFAULT 0,
    is_group INTEGER DEFAULT 0,
    updated_at TEXT
);

CREATE TABLE IF NOT EXISTS groups (
    id TEXT PRIMARY KEY,
    name TEXT,
    description TEXT,
    participant_count INTEGER,
    updated_at TEXT
);

CREATE TABLE IF NOT EXISTS media_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id TEXT,
    mimetype TEXT,
    filename TEXT,
    size INTEGER,
    description TEXT,
    transcript TEXT,
    created_at TEXT,
    updated_at TEXT
);

CREATE TABLE IF NOT EXISTS download_requests (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    requester TEXT,
    title TEXT,
    tmdb_url TEXT,
    media_type TEXT,
    status TEXT DEFAULT 'pending',
    message_id TEXT,
    group_id TEXT,
    created_at TEXT,
    updated_at TEXT
);
