# Media Processing

Process WhatsApp media (images, audio, video) using the `wa` CLI.

## Prerequisites

- `wa` binary built from `wa-cli/` (`cargo build --release`)
- Groq API key (for audio transcription via Whisper and image description via Llama)

## Setup

```bash
# Set environment variables
export GROQ_API_KEY="your-groq-api-key"
export WWEBJS_URL="http://localhost:3000"
export WWEBJS_API_KEY="your-api-key"
export DB_PATH="./whatsapp.db"
export WA_SCHEMA_PATH="./wa-cli/config/wa-schema.sql"
```

## Usage

### Process any media message

```bash
# Auto-detects type: image → vision description, audio → transcription
wa process-media MESSAGE_ID

# Specify language for audio transcription (ISO 639-1)
wa process-media MESSAGE_ID --lang pt    # Portuguese
wa process-media MESSAGE_ID --lang en    # English
wa process-media MESSAGE_ID --lang es    # Spanish
```

### What it does

1. **Downloads** media from WhatsApp via `/media/:messageId` endpoint
2. **Detects type** from MIME type (image, audio, video)
3. **For images**: Sends to Groq Llama 4 Scout for visual description
4. **For audio**: Converts to supported format (if needed via ffmpeg), sends to Groq Whisper for transcription
5. **Stores results** in both `messages` table (ai_description, transcript) and `media_log` table
6. **Cleans up** temporary files

### Supported formats

| Type | Formats | Processing |
|------|---------|-----------|
| Images | JPEG, PNG, WebP, GIF | Groq Llama vision → description |
| Audio | OGG/Opus, MP3, WAV, M4A, AAC | Groq Whisper → transcription |
| Video | MP4, 3GP | Not processed (too large for API) |

### Dedup protection

The command checks `media_log` before processing. If the message was already processed, it exits with code 3 (`DEDUP_SKIP`).

## Automation

### Process media from poll output

```bash
#!/bin/bash
source .env

# Poll for new messages
OUTPUT=$(wa poll 2>/dev/null)
if [ $? -eq 1 ]; then
    echo "$OUTPUT" | while IFS= read -r line; do
        has_media=$(echo "$line" | jq -r '.hasMedia')
        msg_id=$(echo "$line" | jq -r '.messageId')
        if [ "$has_media" = "true" ]; then
            wa process-media "$msg_id"
        fi
    done
fi
```

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success — media processed and stored |
| 1 | Error — API failure, download failed, etc. |
| 2 | Error — DB failure, parse error |
| 3 | DEDUP_SKIP — already processed |
