# SQLite Database

The `wa` CLI uses SQLite to track all WhatsApp messages, sent messages, reactions, contacts, groups, and media.

## Setup

The database is created automatically on first run. The schema is loaded from a config file:

```bash
export DB_PATH="./whatsapp.db"
export WA_SCHEMA_PATH="./wa-cli/config/wa-schema.sql"

# Any wa command will create the DB and tables if they don't exist
wa poll --limit 1
```

## Tables

### messages
All incoming messages (received via poll).

| Column | Type | Description |
|--------|------|-------------|
| id | TEXT PK | WhatsApp message ID |
| chat_id | TEXT | Chat/group JID |
| chat_name | TEXT | Chat display name |
| sender | TEXT | Sender JID |
| content | TEXT | Message body |
| message_type | TEXT | Type: chat, image, ptt, video, etc. |
| timestamp | TEXT | Unix timestamp |
| is_from_me | INTEGER | 1 if sent by bot |
| phone | TEXT | Sender phone (extracted from JID) |
| chat_type | TEXT | "dm" or "group" |
| quoted_msg_id | TEXT | ID of quoted/replied message |
| is_forwarded | INTEGER | 1 if forwarded |
| is_group | INTEGER | 1 if from a group |
| has_media | INTEGER | 1 if has downloadable media |
| contact_name | TEXT | Contact display name |
| contact_number | TEXT | Contact phone number |
| mentioned_ids | TEXT | JSON array of mentioned JIDs |
| ai_description | TEXT | AI-generated image description |
| transcript | TEXT | Audio transcription |
| media_mimetype | TEXT | Media MIME type |
| media_filename | TEXT | Media filename |
| media_size | INTEGER | Media size in bytes |

### sent_messages
All outgoing messages (sent via `wa send` / `wa send-media`).

| Column | Type | Description |
|--------|------|-------------|
| id | TEXT | Message ID from API response |
| to_phone | TEXT | Recipient phone or group JID |
| to_group | TEXT | Group name (if group message) |
| body | TEXT | Message content |
| timestamp | TEXT | Send timestamp |
| platform | TEXT | Always "whatsapp" |
| quoted_msg_id | TEXT | ID of quoted message (for threading) |
| message_type | TEXT | "text" or "media" |
| media_mimetype | TEXT | Media MIME type |
| media_filename | TEXT | Media filename |

### seen_message_ids
Dedup table — tracks which messages have been processed by poll.

| Column | Type | Description |
|--------|------|-------------|
| message_id | TEXT PK | Message ID |
| seen_at | TEXT | When first seen |

### reactions_sent
Tracks emoji reactions sent to prevent duplicates.

| Column | Type | Description |
|--------|------|-------------|
| message_id | TEXT | Message reacted to |
| reaction | TEXT | Emoji sent |
| timestamp | TEXT | When reacted |

### contacts
Synced from WhatsApp via `wa sync-contacts`.

| Column | Type | Description |
|--------|------|-------------|
| id | TEXT PK | Contact ID |
| phone | TEXT | Phone number |
| name | TEXT | Display name |
| about | TEXT | Status/about text |
| is_business | INTEGER | 1 if business account |

### groups
Synced from WhatsApp via `wa sync-contacts`.

| Column | Type | Description |
|--------|------|-------------|
| id | TEXT PK | Group JID |
| name | TEXT | Group name |
| description | TEXT | Group description |
| participant_count | INTEGER | Number of members |
| participants | TEXT | JSON array of participant JIDs |
| admins | TEXT | JSON array of admin JIDs |

### media_log
Tracks processed media (transcriptions, descriptions).

| Column | Type | Description |
|--------|------|-------------|
| id | INTEGER PK | Auto-increment |
| message_id | TEXT | Source message ID |
| mimetype | TEXT | Media MIME type |
| filename | TEXT | Media filename |
| size | INTEGER | Size in bytes |
| description | TEXT | AI description (images) |
| transcript | TEXT | Transcription (audio) |

### dedup_locks
Atomic locks to prevent race conditions across concurrent processes.

| Column | Type | Description |
|--------|------|-------------|
| lock_key | TEXT PK | Lock identifier |
| created_at | TEXT | When acquired (auto-expires after 5 min) |

### change_log
Tracks changes to contacts and groups (name changes, etc.).

| Column | Type | Description |
|--------|------|-------------|
| entity_type | TEXT | "contact" or "group" |
| entity_id | TEXT | Phone or group ID |
| field | TEXT | Changed field name |
| old_value | TEXT | Previous value |
| new_value | TEXT | New value |
| changed_at | TEXT | Timestamp |

## Architecture

```
wwebjs-wa (Docker)  ──── REST API ────▶  wa poll (cron, every 60s)
     │                                        │
     │                                        ├─ Log to messages table
     │                                        ├─ React 👀 to directed messages
     │                                        └─ Output actionable JSON lines
     │
     │                                   wa sync-contacts (hourly)
     │                                        │
     │                                        ├─ Upsert contacts table
     │                                        └─ Upsert groups table
     │
     ▼
  wa send ────────────▶ Log to sent_messages
  wa send-media ──────▶ Log to sent_messages
  wa react ───────────▶ Log to reactions_sent
  wa process-media ───▶ Update messages + media_log
```

## Concurrency

- **WAL mode** enabled on every connection for concurrent read/write
- **5-second busy timeout** prevents lock errors under load
- **Atomic dedup** via `INSERT OR IGNORE` + `rows_affected` check
- **dedup_locks table** prevents race conditions on send/react operations
- Safe to run multiple `wa poll` instances concurrently
