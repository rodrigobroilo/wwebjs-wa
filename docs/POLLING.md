# Polling

Poll WhatsApp for new messages using the `wa` CLI.

## Prerequisites

- `wa` binary built from `wa-cli/` (`cargo build --release`)
- SQLite database initialized (auto-created on first run from schema file)

## Setup

```bash
export WWEBJS_URL="http://localhost:3000"
export WWEBJS_API_KEY="your-api-key"
export DB_PATH="./whatsapp.db"
export WA_SCHEMA_PATH="./wa-cli/config/wa-schema.sql"

# Optional
export BOT_PHONE="15551234567"     # Your bot's phone (filters out own messages)
export BOT_NAME="My Bot"           # Bot display name (for mention detection)
export BOT_PREFIX="[Bot] "         # Prefix added to outgoing messages
```

## Basic polling

```bash
# Poll for new messages (default limit: 100)
wa poll

# Poll with custom limit
wa poll --limit 50
```

### Exit codes

| Code | Meaning |
|------|---------|
| 0 | No new actionable messages |
| 1 | New messages found — JSON lines on stdout |
| 2 | Error (API unreachable, DB failure, etc.) |

### Output format

When exit code is 1, each line is a JSON object:

```json
{"from":"5511999999999@c.us","to":"15551234567@c.us","author":"5511999999999@c.us","body":"Hello!","timestamp":1711234567,"messageId":"false_5511999999999@c.us_ABC123","hasMedia":false,"isGroup":false,"isForwarded":false,"chatName":"John","contactName":"John","quotedMessageId":null}
```

## Cron setup

### Every 60 seconds

```bash
# crontab -e
* * * * * cd /path/to/wwebjs-wa && source .env && wa poll >> /var/log/wa-poll.log 2>&1
```

### Every 30 seconds

```bash
* * * * * cd /path/to/wwebjs-wa && source .env && wa poll >> /var/log/wa-poll.log 2>&1
* * * * * sleep 30 && cd /path/to/wwebjs-wa && source .env && wa poll >> /var/log/wa-poll.log 2>&1
```

## What polling does

1. Fetches messages from `/messages?limit=N` endpoint
2. Checks each message against `seen_message_ids` table (atomic INSERT OR IGNORE)
3. Logs ALL messages (seen and unseen) to `messages` table with full metadata
4. For directed messages (mentions bot name/phone): sends 👀 reaction via `/react/:messageId`
5. Outputs unseen, non-bot messages as JSON lines on stdout
6. Skips messages from the bot itself (`BOT_PHONE` / `from_me` detection)

## Dedup guarantees

- **Atomic seen-marking**: Uses `INSERT OR IGNORE` + `rows_affected` check. Two concurrent pollers cannot both claim the same message.
- **Reaction dedup**: Uses `reactions_sent` table with atomic INSERT before API call. Rolls back on API failure.
- **No duplicate output**: Only messages that were newly marked as seen appear in stdout.

## Syncing contacts

```bash
# Sync contacts and groups from WhatsApp to local DB
wa sync-contacts
```

This fetches all contacts and groups from the server and upserts them into the `contacts` and `groups` tables. Changes are tracked in the `change_log` table.

## Full automation example

```bash
#!/bin/bash
# poll-and-handle.sh — Poll, process media, handle messages
source .env

OUTPUT=$(wa poll 2>/dev/null)
EXIT=$?

if [ $EXIT -eq 1 ]; then
    echo "$OUTPUT" | while IFS= read -r line; do
        msg_id=$(echo "$line" | jq -r '.messageId')
        has_media=$(echo "$line" | jq -r '.hasMedia')
        body=$(echo "$line" | jq -r '.body')
        from=$(echo "$line" | jq -r '.from')

        # Process media if present
        if [ "$has_media" = "true" ]; then
            wa process-media "$msg_id"
        fi

        # Send reply
        wa send --to "$from" -m "Got your message!" --quoted "$msg_id"
    done
fi
```

## Monitoring

Check poll health by looking at the `seen_message_ids` table:

```bash
sqlite3 "$DB_PATH" "SELECT COUNT(*), MAX(seen_at) FROM seen_message_ids;"
```
