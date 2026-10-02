# uwah

> **Unified WhatsApp Hub** — a self-hosted, high-performance WhatsApp gateway that manages multiple sessions and exposes an interactive REST API, dynamic web portal, and gRPC interface.

[![Rust](https://img.shields.io/badge/rust-2024%20edition-orange?logo=rust)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE)
[![Swagger](https://img.shields.io/badge/swagger-OpenAPI%203.1-brightgreen)](http://127.0.0.1:6767/swagger?key=123)
[![Status: Active](https://img.shields.io/badge/status-active%20development-blue)]()

---

## 🚀 Key Highlights

- 🔗 **Multi-Session Isolation** — Each WhatsApp account runs its own background bot worker with isolated Signal protocol storage (`sessions/{id}.db`).
- ⚡ **Instant Messaging & Quotes** — Real-time outbound message dispatch and inbound message interception with full **quoted reply** (`quoted_message_id`) context support.
- 📱 **QR Portal** — Vector SVG QR rendering via **Maud + HTMX** (`/sessions/:id/scan?token=123`). The portal validates its browser token separately from the protected REST routes.
- 📖 **Interactive Swagger UI** — Built-in OpenAPI 3.1 interface at `/swagger` powered by `utoipa` and `utoipa-swagger-ui`; browser access may use `?key=` or `?token=`.
- 🗄️ **Hardened SQLite Concurrency** — Application database (`uwah.db`) operating in WAL mode with `busy_timeout(30s)` and idempotent upserts to eliminate lock contention.
- 🔒 **HTTP Security** — Protected REST routes require `Authorization: Bearer <token>`. Swagger and the scan portal additionally support browser query tokens, with plaintext or SHA-256 digest matching.

---

## 🏛️ System Architecture

```
                      ┌────────────────────────────┐
                      │    HTTP Clients / Browsers │
                      └─────────────┬──────────────┘
                                    │
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│                              Axum 0.8                                  │
│  ┌─────────────────────────┬───────────────────┬────────────────────┐  │
│  │   Protected REST API    │   Maud + HTMX     │     Swagger UI     │  │
│  │  (/sessions, /chats...) │ (/sessions/:id/   │   (/swagger?key=)  │  │
│  │  (Bearer Auth Token)    │      scan)        │   (OpenAPI 3.1)    │  │
│  └────────────┬────────────┴─────────┬─────────┴──────────┬─────────┘  │
│               │                      │                    │            │
│               └──────────────────────┼────────────────────┘            │
│                                      ▼                                 │
│                         ┌─────────────────────────┐                    │
│                         │        AppState         │                    │
│                         │ - &'static Config       │                    │
│                         │ - SqlitePool (uwah.db)  │                    │
│                         │ - Arc<SessionManager>   │                    │
│                         └────────────┬────────────┘                    │
└──────────────────────────────────────┼─────────────────────────────────┘
                                       │
                                       ▼
┌────────────────────────────────────────────────────────────────────────┐
│                        SessionManager Layer                            │
│           (Registry of Active WhatsApp Bots: HashMap<ID, Arc<Client>>)  │
│                                                                        │
│   ┌──────────────────────────┐        ┌──────────────────────────┐     │
│   │    WhatsApp Session A    │        │    WhatsApp Session B    │     │
│   │   - Bot / Client Worker  │        │   - Bot / Client Worker  │     │
│   │   - Background Tokio Task│        │   - Background Tokio Task│     │
│   │   - on_message handler   │        │   - on_message handler   │     │
│   │   - on_qr_code handler   │        │   - on_qr_code handler   │     │
│   │   - on_connected handler │        │   - on_connected handler │     │
│   └────────────┬─────────────┘        └────────────┬─────────────┘     │
└────────────────┼───────────────────────────────────┼───────────────────┘
                 │                                   │
                 ▼                                   ▼
      ┌─────────────────────┐             ┌─────────────────────┐
      │  sessions/{id_a}.db │             │  sessions/{id_b}.db │
      │ (Signal Keys Store) │             │ (Signal Keys Store) │
      └─────────────────────┘             └─────────────────────┘
                 │                                   │
                 └─────────────────┬─────────────────┘
                                   ▼
                         ┌───────────────────┐
                         │      uwah.db      │
                         │(Gateway App State)│
                         └───────────────────┘
```

Detailed technical breakdown available in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

---

## ⚙️ Configuration (`uwah.toml`)

Uwah is configured through a clean `uwah.toml` file at the root:

```toml
[database]
# Path to the application state SQLite database
dir = "d:/projects/rust/uwah/uwah.db"

[service]
host = "localhost"
port = 6767
# Plaintext token (e.g. "123") OR SHA-256 hex digest
auth = "123"

[media]
# Directory where downloaded media files are cached
dir = "d:/projects/rust/uwah/media"

[sessions]
# Directory where per-session Signal stores are maintained
dir = "d:/projects/rust/uwah/sessions"
```

---

## 📖 Swagger UI

Access the interactive API explorer directly in your browser:

👉 **`http://127.0.0.1:6767/swagger?key=123`**

You can inspect models, test request parameters, and execute live endpoints directly from the UI.

---

## 📱 Pairing a Device (Maud + HTMX QR Scanner)

1. Create a session via REST:
   ```bash
   curl -X POST http://127.0.0.1:6767/sessions \
     -H "Authorization: Bearer 123" \
     -H "Content-Type: application/json" \
     -d '{"id": "minha-sessao"}'
   ```
2. Open the reactive scan portal in your browser:
   👉 **`http://127.0.0.1:6767/sessions/minha-sessao/scan?token=123`**
3. Scan the QR code using your WhatsApp phone app (**Linked Devices** $\rightarrow$ **Link a Device**).
4. The page immediately transitions to a green status card (*"Session Connected"*), and the gateway is ready to send and receive messages!

---

## 💬 Sending Messages

Send a real-time message to any phone number or group:

```bash
curl -X POST http://127.0.0.1:6767/sessions/minha-sessao/chats/5519998005388/messages \
  -H "Authorization: Bearer 123" \
  -H "Content-Type: application/json" \
  -d '{
    "body": "Olá mundo via Uwah Gateway!"
  }'
```

### Quoting / Replying to a Message

To quote an existing message, pass `quoted_message_id`:

```bash
curl -X POST http://127.0.0.1:6767/sessions/minha-sessao/chats/5519998005388/messages \
  -H "Authorization: Bearer 123" \
  -H "Content-Type: application/json" \
  -d '{
    "body": "Essa é uma resposta citando a mensagem anterior!",
    "quoted_message_id": "3EB0C44298FC1C14"
  }'
```

---

## 📚 Documentation Index

For in-depth guides, consult the dedicated documentation files:
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — System internals, dual-database design, and event loop lifecycles.
- [docs/API.md](docs/API.md) — Complete REST endpoint reference and request/response specifications.
- [docs/DATABASE.md](docs/DATABASE.md) — Schema definitions, indexing, WAL configuration, and SQLite concurrency tuning.
- [docs/WEBHOOKS.md](docs/WEBHOOKS.md) — Webhook configuration, signatures, retries, outbox and DLQ operations.

### Current limitations

- The gRPC services currently expose health checks and event streaming; no gRPC authentication interceptor is configured.
- The readiness probe currently checks the fixed `data/media_cache` directory rather than `[media].dir`.
- The REST and gRPC listeners are plain TCP. TLS must be provided by an external proxy or added to the service.

---

## 📄 License

[MIT](./LICENSE) © uwah contributors
