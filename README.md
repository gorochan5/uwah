# uwah

> **Unified WhatsApp Hub** — a self-hosted WhatsApp gateway that manages multiple sessions and exposes a dual REST + gRPC API.

[![Rust](https://img.shields.io/badge/rust-2024%20edition-orange?logo=rust)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE)
[![Status: Experimental](https://img.shields.io/badge/status-experimental-yellow)]()

---

## Overview

**uwah** wraps [`whatsapp-rust`](https://github.com/oxidezap/whatsapp-rust) and turns it into a production-ready service.
It lets you connect **multiple WhatsApp accounts** (sessions) and interact with all of them through a single, unified API — either REST/HTTP or gRPC with real-time streaming.

**Key traits:**

- 🔗 **Multi-session** — manage many WhatsApp numbers at once
- ⚡ **Dual API** — REST for CRUD operations, gRPC for streaming events (messages, calls, QR codes, status updates)
- 🗄️ **SQLite persistence** — messages, chats, contacts, calls, groups and media cache stored locally
- 🔒 **Token auth** — SHA-256 bearer token configured via `uwah.toml`
- 🦀 **Pure Rust** — built on Tokio, Axum and Tonic

---

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                     uwah                            │
│                                                     │
│  ┌─────────────┐         ┌───────────────────────┐  │
│  │  REST / HTTP│         │  gRPC (Tonic)         │  │
│  │   (Axum)   │         │  + server-side stream  │  │
│  └──────┬──────┘         └──────────┬────────────┘  │
│         │                           │               │
│         └─────────────┬─────────────┘               │
│                       ▼                             │
│              ┌─────────────────┐                    │
│              │  Session Manager│                    │
│              └────────┬────────┘                    │
│                       │                             │
│              ┌────────▼────────┐                    │
│              │  whatsapp-rust  │                    │
│              └────────┬────────┘                    │
│                       │                             │
│              ┌────────▼────────┐                    │
│              │   SQLite (sqlx) │                    │
│              └─────────────────┘                    │
└─────────────────────────────────────────────────────┘
                        │
                        ▼
               WhatsApp Network
```

---

## Configuration

uwah is configured via a `uwah.toml` file:

```toml
[database]
# Path to the SQLite database file
dir = "/path/to/uwah.db"

[service]
host = "localhost"
port = 6767
# SHA-256 of your auth token
auth = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"

[media]
# Directory where media files will be cached on disk
dir = "/path/to/media"
```

---

## REST API

> **Note:** The REST API is under active development. Endpoints marked with 🚧 are planned but not yet implemented.

All requests require an `Authorization: Bearer <token>` header.
Base URL: `http://<host>:<port>`

### Sessions

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/sessions` | List all sessions |
| `POST` | `/sessions` | Create a new session |
| `GET` | `/sessions/:id` | Get session details + QR code payload |
| `PATCH` | `/sessions/:id` | Rename a session |
| `DELETE` | `/sessions/:id` | Delete a session |

### Chats

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/sessions/:id/chats` | List all chats for a session |
| `GET` | `/sessions/:id/chats/:jid` | Get a specific chat |

### Messages

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/sessions/:id/chats/:jid/messages` | List messages in a chat |
| `POST` | `/sessions/:id/chats/:jid/messages` 🚧 | Send a message |
| `DELETE` | `/sessions/:id/chats/:jid/messages/:msg_id` 🚧 | Delete a message |

### Contacts

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/sessions/:id/contacts` | List contacts |
| `GET` | `/sessions/:id/contacts/:jid` | Get contact details |

### Groups

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/sessions/:id/chats/:jid/participants` | List group participants |

### Calls

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/sessions/:id/calls` | List call history |

### Media

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/sessions/:id/media/:sha256` 🚧 | Retrieve a cached media file |

### Health

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/health` | Service health check |

---

## gRPC & Streaming

uwah exposes a gRPC server (powered by [Tonic](https://github.com/hyperium/tonic)) for **real-time event streaming**.

Instead of polling the REST API, consumers can open a single gRPC stream per session and receive events as they happen:

- 📩 Incoming & outgoing messages
- 📞 Call events (`offered`, `accepted`, `ended`, `missed`)
- 🔄 QR code refreshes (during session setup)
- 👥 Group membership changes
- ✅ Message delivery status updates (`sent` → `delivered` → `read`)

> The `.proto` schema is currently being defined. It will live in the `proto/` directory.

---

## Data Model

uwah persists the following entities in SQLite:

| Entity | Key fields |
|--------|-----------|
| `Session` | `id`, `phone_number`, `push_name`, `status` (`QRCODE` / `OK`) |
| `Chat` | `jid`, `kind` (`direct` / `group` / `newsletter` / `broadcast`) |
| `Message` | `id`, `message_type`, `body`, `delivery_status`, `is_edited`, `is_deleted` |
| `Contact` | `jid`, `push_name`, `is_business` |
| `GroupParticipant` | `user_jid`, `role` (`member` / `admin` / `superadmin`) |
| `Call` | `id`, `is_video`, `status` (`offered` / `accepted` / `ended` / `missed`) |
| `MediaCache` | `sha256`, `disk_path`, `mime_type`, `ref_count` |

---

## License

[MIT](./LICENSE) © uwah contributors
