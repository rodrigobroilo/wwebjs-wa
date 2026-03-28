# wwebjs-wa

> ⚠️ **Unofficial project.** This is an independent, community-built wrapper around WhatsApp Web. It is **not** affiliated with, endorsed by, or connected to WhatsApp or Meta in any way. Use at your own risk — see [Disclaimer](#disclaimer).

A lightweight, self-hosted WhatsApp Web API server built on [whatsapp-web.js](https://github.com/pedroslopez/whatsapp-web.js). Designed for AI agents, home automation, and chatbots.

Run it as a Docker container on any machine (NAS, Raspberry Pi, VPS) and get a full REST API for WhatsApp — send messages, react, forward, create polls, manage contacts and groups, and more.

## Table of Contents

- [Features](#features)
- [Quick Start](#quick-start)
- [Configuration](#configuration)
- [API Reference](#api-reference)
- [Endpoint Details](#endpoint-details)
- [Response Examples](#response-examples)
- [AI Agent Integration](#ai-agent-integration)
- [Rust CLI Client](#rust-cli-client)
- [Exposing with Cloudflare Tunnel (Optional)](#exposing-with-cloudflare-tunnel-optional)
- [Persistent Data](#persistent-data)
- [Security](#security)
- [Running on a Synology NAS](#running-on-a-synology-nas)
- [Limitations & Known Issues](#limitations--known-issues)
- [Development](#development)
- [Troubleshooting](#troubleshooting)
- [Third-Party Software](#third-party-software)
- [Privacy & Compliance](#privacy--compliance)
- [Disclaimer](#disclaimer)
- [License](#license)

## Features

- 📱 **20 REST API endpoints** — messaging, media, reactions, polls, contacts, groups
- 🐳 **Docker-ready** — single container, persistent sessions via volume
- 🔑 **API key authentication** — simple header-based auth
- 🧵 **Thread support** — reply to specific messages with `quotedMessageId`
- 📸 **Media support** — send and receive images, videos, documents, audio
- 👍 **Reactions** — emoji reactions on any message
- 📊 **Polls** — create polls in group chats
- 🔍 **Search** — search messages across chats
- 📌 **Pin/Unpin** — pin important messages
- 🗑️ **Delete** — delete sent messages
- ↩️ **Forward** — forward messages between chats
- 👤 **Contacts & Groups** — get contact info, group participants, admins
- 🌐 **Cloudflare Tunnel compatible** — expose securely without port forwarding

## Quick Start

### Prerequisites

- Docker and Docker Compose (v2+)
- A phone with WhatsApp installed (for QR code pairing)
- ~256 MB RAM, ~500 MB disk (Chromium included in container)
- Tested on: x86_64 (Intel/AMD), ARM64 (Raspberry Pi 4, Apple Silicon)

### 1. Clone and configure

```bash
git clone https://github.com/yourusername/wwebjs-wa.git
cd wwebjs-wa
cp .env.example .env
# Edit .env — set a strong API_KEY (do NOT use the default)
```

### 2. Build and run

```bash
docker compose up -d --build
```

### 3. Pair with WhatsApp

Open `http://localhost:3100/qr?key=YOUR_API_KEY` in your browser and scan the QR code with WhatsApp on your phone.

### 4. Verify connection

```bash
curl -H "X-Api-Key: YOUR_API_KEY" http://localhost:3100/health
```

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | `3100` | Server port |
| `API_KEY` | `change-me` | **Required.** Set a strong, unique key. Sent via `X-Api-Key` header. |

### docker-compose.yml

```yaml
version: "3"
services:
  wwebjs:
    build: .
    container_name: wwebjs-wa
    restart: unless-stopped
    ports:
      - "3100:3100"
    volumes:
      - /volume1/docker/wwebjs/data:/data
    environment:
      - PORT=3100
      - API_KEY=your-secret-api-key
```

> **Note:** Replace `/volume1/docker/wwebjs/data` with any host path for persistent session storage. On Synology NAS, `/volume1/docker/wwebjs/data` is a common choice. Replace `your-secret-api-key` with a strong, unique key.

### Number & ID Formats

The API accepts these formats for phone numbers and chat IDs:

| Field | Format | Example |
|-------|--------|---------|
| `to` | Raw number (country code + number, no `+`) | `15551234567` |
| `chatId` | WhatsApp chat ID | `15551234567@c.us` |
| `group_name` | Case-insensitive substring match | `"My Group"` |
| `:groupId` | WhatsApp group ID | `120363012345678901@g.us` |
| `:messageId` | Serialized message ID | `true_15551234567@c.us_3EB0ABC123` |

> **Note:** For `group_name`, the API matches the first group whose name contains the given substring (case-insensitive). To avoid ambiguity with similar names, prefer using group IDs directly via `chatId`.

## API Reference

All endpoints require the `X-Api-Key` header. Prefer headers over query parameters for security.

### Connection

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/health` | Connection status, message count, uptime |
| `GET` | `/qr` | QR code page for WhatsApp Web pairing |
| `POST` | `/restart` | Reconnect the client (async — poll `/health` until ready) |
| `POST` | `/logout` | Clear session, invalidate volume auth, `/qr` available after |

### Messaging

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/messages` | Recent messages. Params: `limit` (default 50), `since` (unix timestamp), `chat` (chat ID) |
| `POST` | `/send` | Send text to a phone number |
| `POST` | `/send-group` | Send text to a group (by name or ID) |
| `POST` | `/send-media` | Send image/video/doc/audio (base64) |
| `POST` | `/forward/:messageId` | Forward a message to another chat |
| `DELETE` | `/message/:messageId` | Delete a message. `?everyone=true` deletes for all. |

### Interactions

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/react/:messageId` | React with an emoji (send `""` to remove) |
| `POST` | `/mark-read` | Mark a chat as read |
| `POST` | `/pin/:messageId` | Pin (`duration` in seconds) or unpin (`duration: 0`) a message |
| `POST` | `/poll` | Create a poll in a group |

### Discovery

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/chats` | List all chats (up to 30, sorted by recent activity) |
| `GET` | `/history/:chatId` | Fetch messages from a chat. Param: `limit` (default 50) |
| `GET` | `/search` | Search messages. Params: `query` (required), `chatId`, `limit` (default 20). Substring match. |
| `GET` | `/contact/:number` | Contact info: name, about, business status, profile pic URL |
| `GET` | `/group/:groupId` | Group info: name, description, participants with admin status |
| `GET` | `/media/:messageId` | Download media as base64 JSON (may be large for video) |

### HTTP Status Codes

| Code | Meaning |
|------|---------|
| `200` | Success |
| `400` | Bad request (missing/invalid parameters) |
| `401` | Unauthorized (missing or wrong API key) |
| `404` | Not found (message, group, or contact not found) |
| `500` | Server error |
| `503` | Client not connected (need to pair via QR) |

All error responses return `{"error": "description"}`.

## Endpoint Details

### POST /send

```json
{
  "to": "15551234567",
  "message": "Hello!",
  "quotedMessageId": "optional_message_id_for_threading"
}
```

**Response:** `{"success": true, "id": "serialized_message_id"}`

### POST /send-group

```json
{
  "group_name": "My Group",
  "message": "Hello group!",
  "quotedMessageId": "optional_message_id_for_threading"
}
```

**Response:** `{"success": true, "id": "serialized_message_id", "group": "My Group"}`

### POST /send-media

```json
{
  "to": "15551234567",
  "data": "base64_encoded_file_data",
  "mimetype": "image/jpeg",
  "filename": "photo.jpg",
  "caption": "Check this out!",
  "quotedMessageId": "optional_message_id_for_threading"
}
```

For groups, use `group_name` instead of `to`.

**Response:** `{"success": true, "id": "serialized_message_id"}`

### POST /react/:messageId

```json
{
  "emoji": "👍"
}
```

Send `{"emoji": ""}` to remove a reaction.

**Response:** `{"success": true}`

### POST /forward/:messageId

```json
{
  "to": "15551234567"
}
```

Or forward to a group: `{"group_name": "My Group"}`

**Response:** `{"success": true}`

### POST /mark-read

```json
{
  "chatId": "15551234567@c.us"
}
```

**Response:** `{"success": true}`

### POST /poll

```json
{
  "group_name": "My Group",
  "title": "What should we watch?",
  "options": ["Movie A", "Movie B", "Movie C"],
  "allowMultiple": false
}
```

You can also use `chatId` instead of `group_name`.

**Response:** `{"success": true, "id": "serialized_message_id"}`

### POST /pin/:messageId

```json
{
  "duration": 604800
}
```

Duration in seconds (default: 7 days). Send `{"duration": 0}` to unpin.

**Response:** `{"success": true, "action": "pinned", "duration": 604800}`

### DELETE /message/:messageId?everyone=true

Set `everyone=true` to delete for all participants, or omit for "delete for me" only.

**Response:** `{"success": true, "deletedForEveryone": true}`

## Response Examples

### GET /health

```json
{
  "status": "connected",
  "hasQR": false,
  "info": {
    "pushname": "My Bot",
    "wid": "15551234567@c.us"
  },
  "messageCount": 142,
  "uptime": 86400
}
```

### GET /messages?limit=2

```json
{
  "count": 2,
  "messages": [
    {
      "id": "false_15551234567@c.us_3EB0ABC123",
      "from": "15559876543@c.us",
      "to": "15551234567@c.us",
      "author": "15559876543@c.us",
      "body": "Hello!",
      "timestamp": 1710000000,
      "type": "chat",
      "hasMedia": false,
      "isGroup": false,
      "chatName": "John Doe",
      "contactName": "John",
      "isForwarded": false,
      "mentionedIds": [],
      "quotedMessageId": null
    }
  ]
}
```

### GET /contact/15551234567

```json
{
  "id": "15551234567@c.us",
  "name": "John Doe",
  "pushname": "John",
  "number": "15551234567",
  "isBlocked": false,
  "isBusiness": false,
  "about": "Hey there! I am using WhatsApp.",
  "profilePicUrl": "https://..."
}
```

### GET /group/120363012345678901@g.us

```json
{
  "id": "120363012345678901@g.us",
  "name": "My Group",
  "description": "A group for friends",
  "createdAt": 1700000000,
  "participantCount": 5,
  "participants": [
    {"id": "15551234567@c.us", "isAdmin": true, "isSuperAdmin": true},
    {"id": "15559876543@c.us", "isAdmin": false, "isSuperAdmin": false}
  ]
}
```

## AI Agent Integration

This server is designed to be controlled by AI agents (Claude, GPT, OpenClaw, custom bots, etc.).

### Architecture

**Local setup** (agent + wwebjs-wa on the same machine or network):
```
┌─────────────┐     localhost / LAN     ┌─────────────┐
│   AI Agent   │ ──────────────────────▶│  wwebjs-wa   │
│ (OpenClaw,   │     http://localhost    │  (Docker)    │
│  Claude, GPT)│     port 3100          │              │
└─────────────┘                         └──────┬───────┘
                                               │
                                        WhatsApp Web
                                               │
                                        ┌──────▼───────┐
                                        │   WhatsApp   │
                                        │   Servers    │
                                        └──────────────┘
```

**Remote setup** (agent in the cloud, wwebjs-wa at home/NAS):
```
┌─────────────┐     HTTPS      ┌──────────────────┐     localhost     ┌─────────────┐
│   AI Agent   │ ──────────────▶│ Cloudflare Tunnel │ ────────────────▶│  wwebjs-wa   │
│ (cloud-hosted│   CF-Access    │   (recommended)   │    port 3100     │  (Docker)    │
│  or SaaS)    │   + API key    │                   │                  │  at home/NAS │
└─────────────┘                └──────────────────┘                  └──────┬───────┘
                                                                           │
                                                                    WhatsApp Web
                                                                           │
                                                                    ┌──────▼───────┐
                                                                    │   WhatsApp   │
                                                                    │   Servers    │
                                                                    └──────────────┘
```

### Recommended Polling Pattern

1. **Poll `/messages`** every 60 seconds with `?since=LAST_TIMESTAMP`
2. **Track seen message IDs** in a local database to avoid processing duplicates
3. **React** to acknowledge messages directed at your bot
4. **Reply with `quotedMessageId`** to maintain conversation threads
5. **Download media** via `/media/:messageId` for image/audio processing
6. **Update `since`** after processing each batch (not per-message)

### Example: AI Agent Message Loop

Using the Rust CLI with cron:

```bash
# In your .env file:
export WWEBJS_URL="http://localhost:3100"
export WWEBJS_API_KEY="your-api-key"
export DB_PATH="/path/to/whatsapp.db"

# Crontab entry — poll every 60 seconds:
* * * * * source /path/to/.env && /path/to/wa poll
```

The `wa poll` command:
1. Fetches new messages from the server
2. Deduplicates against the local SQLite database
3. Sends 👀 reaction on messages directed at the bot
4. Outputs new actionable messages as JSON (exit code 1)
5. Your AI agent reads the JSON output and decides how to respond:

```bash
# Send a reply with threading
wa send --to 5511999999999 -m "Got it!" --quoted MSG_ID_123

# React to acknowledge
wa react --message-id MSG_ID_123 --reaction "👍"

# Send an image
wa send-media --to 5511999999999 --file result.png --mimetype image/png --caption "Here you go"
```

## Exposing with Cloudflare Tunnel (Optional)

### When do you need this?

| Setup | Cloudflare Tunnel? |
|-------|-------------------|
| AI agent + wwebjs-wa on **the same machine** (localhost) | ❌ Not needed — use `http://localhost:3100` |
| AI agent + wwebjs-wa on **the same local network** | ❌ Not needed — use `http://192.168.x.x:3100` |
| AI agent in **the cloud** (VPS, SaaS) + wwebjs-wa at **home/NAS** | ✅ **Recommended** — secure remote access |
| AI agent on **your phone/laptop** + wwebjs-wa at **home/NAS** | ✅ **Recommended** — access from anywhere |

If your AI agent (OpenClaw, Claude, ChatGPT, etc.) runs **remotely** and your wwebjs-wa runs at **home or on a NAS**, Cloudflare Tunnel lets the agent reach your WhatsApp server securely — no port forwarding, no exposing your home IP.

Use [Cloudflare Tunnel](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/) to expose it securely without opening ports.

### Setup

1. Install `cloudflared`:

```bash
# Debian/Ubuntu
curl -L https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64.deb -o cloudflared.deb
sudo dpkg -i cloudflared.deb

# macOS
brew install cloudflare/cloudflare/cloudflared
```

2. Authenticate, create tunnel, and configure:

```bash
cloudflared tunnel login
cloudflared tunnel create wwebjs-wa
```

3. Configure (`~/.cloudflared/config.yml`):

```yaml
tunnel: YOUR_TUNNEL_ID
credentials-file: /path/to/YOUR_TUNNEL_ID.json

ingress:
  - hostname: wwebjs.yourdomain.com   # Replace with your domain
    service: http://localhost:3100
  - service: http_status:404
```

4. Route DNS and run:

```bash
cloudflared tunnel route dns wwebjs-wa wwebjs.yourdomain.com
cloudflared tunnel run wwebjs-wa
```

### Securing with Cloudflare Access (Recommended)

Add a [Cloudflare Access](https://developers.cloudflare.com/cloudflare-one/policies/access/) application to require authentication:

1. Go to **Cloudflare Zero Trust** → **Access** → **Applications**
2. Create a **Self-hosted** app for your tunnel hostname
3. Add a **Service Auth** policy using **Service Tokens**
4. Include CF-Access headers on every request:

```bash
curl -H "X-Api-Key: YOUR_API_KEY" \
     -H "CF-Access-Client-Id: YOUR_CF_CLIENT_ID" \
     -H "CF-Access-Client-Secret: YOUR_CF_CLIENT_SECRET" \
     https://wwebjs.yourdomain.com/health
```

## Persistent Data

WhatsApp session data is stored in the `/data` directory inside the container, mapped to a host path via `docker-compose.yml`.

| Action | Effect |
|--------|--------|
| Rebuild container (`docker compose up --build`) | Session preserved ✅ |
| Stop/restart container | Session preserved ✅ |
| Delete host data directory | Session lost — re-pair via QR ❌ |

**Backup:** Copy your host data directory (e.g., `/volume1/docker/wwebjs/data`) or use `docker cp wwebjs-wa:/data ./backup`.

## Security

⚠️ **Important security considerations:**

- **Never use the default API key** (`change-me`) in production
- **Use strong, unique API keys** — generate with `openssl rand -hex 32`
- **Prefer `X-Api-Key` header** over query parameters (avoid keys in server logs/URLs)
- **Never expose port 3100 directly** to the internet — use Cloudflare Tunnel or a reverse proxy with HTTPS
- **Protect the session volume** — it contains your WhatsApp auth tokens
- **Rotate API keys** periodically
- **Cloudflare Access** adds a second layer of authentication for remote access

## Running on a Synology NAS

This container runs well on Synology NAS (DS918+, DS920+, DS923+, etc.):

1. Enable **Container Manager** (Docker) in DSM Package Center
2. SSH into your NAS and clone the repo (or upload via File Station)
3. Build and run: `docker compose up -d --build`
4. The WhatsApp session persists in the Docker volume across DSM updates and restarts

> Tested on DS918+ (Intel Celeron J3455) with ~150 MB RAM usage.

## Limitations & Known Issues

- **Unofficial API** — WhatsApp may block or restrict accounts that violate their Terms of Service. Use responsibly.
- **Single session** — one phone number per container instance. Run multiple containers for multiple numbers.
- **In-memory message buffer** — the server keeps the last 1,000 messages in memory. For persistent message storage, use an external database with the polling pattern.
- **QR re-pairing** — WhatsApp Web sessions may expire. Monitor `/health` and re-pair if disconnected.
- **Rate limits** — WhatsApp may rate-limit sending. No built-in rate limiting in the server.
- **Large media** — `/media/:messageId` returns base64-encoded data in JSON. Very large files may cause timeouts or high memory usage.
- **Group name collisions** — `group_name` uses substring matching. Use group IDs for precision.

## Rust CLI Client

This repo includes a **Rust CLI client** (`wa`) — a single binary with all the tools you need to interact with the server. No Python, no pip, no dependencies — just one 4 MB binary.

### Build

```bash
cd wa-cli
cargo build --release
# Binary at target/release/wa
```

> **Prerequisites:** [Rust](https://rustup.rs/) (1.70+). The build is fully self-contained (statically linked TLS, bundled SQLite).

### Subcommands

| Command | Description |
|---------|-------------|
| `wa send` | Send a text message to a phone or group |
| `wa send-media` | Send image, document, video, or audio |
| `wa react` | React to a message with an emoji |
| `wa search` | Search messages in the local SQLite database |
| `wa log` | Log a sent message to the database |
| `wa process-media` | Download media, transcribe audio (Groq Whisper), describe images (Groq Llama) |
| `wa sync-contacts` | Sync contacts and groups from server to local database |
| `wa poll` | Poll for new messages (designed for cron, every 60s) |
| `wa init-db` | Initialize the SQLite database schema (16 tables) |

### Quick start

```bash
# Set environment variables (or use a .env file)
export WWEBJS_URL="http://localhost:3100"
export WWEBJS_API_KEY="your-api-key"
export DB_PATH="./whatsapp.db"

# Optional: for Cloudflare Tunnel
export CF_ACCESS_CLIENT_ID="your-cf-id"
export CF_ACCESS_CLIENT_SECRET="your-cf-secret"

# Optional: for media AI processing (free Groq account)
export GROQ_API_KEY="your-groq-key"

# Optional: prefix for all sent messages
export BOT_PREFIX=""

# Initialize database
./wa init-db

# Send a message
./wa send --to 5511999999999 -m "Hello from Rust!"

# Send to a group
./wa send --group "My Group" -m "Good morning!"

# Send an image with caption
./wa send-media --to 5511999999999 --file photo.jpg --mimetype image/jpeg --caption "Check this out"

# React to a message
./wa react --message-id "true_5511999999999@c.us_ABC123" --reaction "👍"

# Search messages
./wa search -q "meeting" --limit 20

# Poll for new messages (run via cron every 60s)
./wa poll

# Process media (transcribe audio / describe images)
./wa process-media MESSAGE_ID

# Sync contacts and groups to local DB
./wa sync-contacts
```

### Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `WWEBJS_URL` | Yes | Server URL (e.g. `http://localhost:3100`) |
| `WWEBJS_API_KEY` | Yes | API key for authentication |
| `DB_PATH` | Yes | Path to SQLite database file |
| `CF_ACCESS_CLIENT_ID` | No | Cloudflare Access client ID |
| `CF_ACCESS_CLIENT_SECRET` | No | Cloudflare Access client secret |
| `GROQ_API_KEY` | No | Groq API key (for process-media transcription/vision) |
| `BOT_PREFIX` | No | Prefix for sent messages (default: none) |
| `BOT_PHONE` | No | Bot phone number (poll self-message filtering) |
| `BOT_NAME` | No | Bot display name (poll mention detection) |

### Cron setup (automatic polling)

```bash
# Poll every 60 seconds
* * * * * source /path/to/.env && /path/to/wa poll
```

Exit codes: `0` = no new messages, `1` = new actionable messages (JSON output on stdout).

### Help

Every subcommand has detailed `--help`:

```bash
wa --help              # Overview
wa send --help         # Send options
wa poll --help         # Polling options
wa search --help       # Search filters
wa process-media --help
```

### Documentation

| Document | Contents |
|----------|----------|
| [`docs/SQLITE.md`](docs/SQLITE.md) | Full database schema (16 tables), ER diagram, common queries |
| [`docs/MEDIA-PROCESSING.md`](docs/MEDIA-PROCESSING.md) | Image description + audio transcription via Groq (free) |
| [`docs/POLLING.md`](docs/POLLING.md) | Cron-based polling, deduplication, scheduling guide |


## Development

### Run locally (without Docker)

```bash
# Install dependencies
npm install

# Install Chromium (needed by whatsapp-web.js/Puppeteer)
# macOS: brew install chromium
# Ubuntu: sudo apt install chromium-browser

# Set environment variables
export PORT=3100
export API_KEY=your-dev-key

# Start
node server.js
```

### Project structure

```
├── server.js              # Main server (Express + whatsapp-web.js)
├── Dockerfile             # Container definition
├── docker-compose.yml     # Compose config
├── package.json           # Node.js dependencies
├── .env.example           # Environment variable template
├── .gitignore
├── LICENSE                # MIT License + disclaimer
├── wa-cli/                # Rust CLI client
│   ├── Cargo.toml         # Rust dependencies
│   └── src/main.rs        # Full source (~900 lines)
└── docs/                  # Detailed guides
    ├── SQLITE.md          # Database schema & queries
    ├── MEDIA-PROCESSING.md # Groq AI setup guide
    └── POLLING.md         # Polling & dedup guide
```

## Troubleshooting

| Issue | Solution |
|-------|----------|
| QR code not appearing | Wait 10–20 seconds after container start, then refresh `/qr` |
| Client disconnects frequently | Ensure stable internet; avoid scanning QR on multiple devices |
| Messages not appearing | Check `/health` — client may need `/restart` |
| Media download fails | Large files may time out; retry the request |
| Group not found | Use the full group ID instead of `group_name` to avoid substring ambiguity |
| `503 Not connected` | Client needs pairing — visit `/qr` to scan |
| High memory usage | Reduce `MAX_MESSAGES` in `server.js` (default: 1000) |

## Third-Party Software

This project uses the following open-source libraries and services:

| Package | License | Purpose |
|---------|---------|---------|
| [whatsapp-web.js](https://github.com/pedroslopez/whatsapp-web.js) | Apache-2.0 | WhatsApp Web client library |
| [Express](https://expressjs.com/) | MIT | HTTP server framework |
| [qrcode](https://github.com/soldair/node-qrcode) | MIT | QR code generation for pairing |
| [Puppeteer](https://pptr.dev/) / Chromium | Apache-2.0 | Headless browser for WhatsApp Web |
| [reqwest](https://docs.rs/reqwest) (Rust) | MIT/Apache-2.0 | HTTP client for CLI |
| [clap](https://docs.rs/clap) (Rust) | MIT/Apache-2.0 | CLI argument parsing |
| [rusqlite](https://docs.rs/rusqlite) (Rust) | MIT | SQLite bindings for CLI |
| [Groq](https://groq.com) (optional) | Proprietary (free tier) | AI inference for media processing |
| [Whisper](https://github.com/openai/whisper) (via Groq) | MIT | Speech recognition model |
| [Llama 4 Scout](https://ai.meta.com/llama/) (via Groq) | Llama License | Vision/multimodal model |
| [ffmpeg](https://ffmpeg.org/) (optional) | LGPL/GPL | Video frame extraction |

Full dependency list in [`package.json`](package.json) and [`wa-cli/Cargo.toml`](wa-cli/Cargo.toml).

## Privacy & Compliance

**You are responsible for how you use this software.** Consider the following:

- **Message storage** — The polling scripts store all received messages locally in SQLite. Ensure you have consent from participants where required by local law.
- **Contact data** — Contact sync stores phone numbers, names, and profile info. Handle according to applicable data protection regulations (GDPR, CCPA, etc.).
- **Third-party AI processing** — If using Groq for media processing, images and audio are sent to Groq's servers for AI inference. Review [Groq's privacy policy](https://groq.com/privacy-policy/).
- **WhatsApp Terms of Service** — Automated use of WhatsApp may violate their ToS. Accounts may be restricted or banned. This project does not encourage violation of any terms.
- **No warranty** — This software is provided as-is with no guarantee of availability, accuracy, or compliance.

## Disclaimer

This project is **not affiliated with, endorsed by, or connected to WhatsApp, Meta, or any of their subsidiaries**. "WhatsApp" is a registered trademark of WhatsApp LLC.

This is an unofficial, community-built tool that interacts with WhatsApp Web. The authors and contributors:

- **Do not recommend** using this software in violation of WhatsApp's Terms of Service
- **Accept no liability** for account bans, data loss, legal consequences, or any other damages
- **Make no guarantees** about the software's reliability, security, or fitness for any purpose
- **Are not responsible** for how you use this software or what you do with the data it collects

**Use entirely at your own risk.** By using this software, you acknowledge that you understand these risks and accept full responsibility.

## License

MIT License — see [LICENSE](LICENSE) for details.

## Acknowledgments

Built with [whatsapp-web.js](https://github.com/pedroslopez/whatsapp-web.js) by [Pedro Lopez](https://github.com/pedroslopez) — an unofficial WhatsApp Web API library.

Media processing powered by [Groq](https://groq.com) (free tier), using [Whisper](https://github.com/openai/whisper) (OpenAI) for speech recognition and [Llama 4 Scout](https://ai.meta.com/llama/) (Meta) for vision.
