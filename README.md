# Uwah

Uwah is a self-hosted WhatsApp gateway written in Rust. It manages multiple WhatsApp sessions and exposes REST and gRPC APIs, live event streams, a browser-based QR pairing page, and optional webhooks.

> **Project status:** actively developed. API and protocol behavior can change between releases.

## Capabilities

- Manage independent WhatsApp sessions, each with its own protocol store.
- Send and receive messages, media, contacts, group operations, presence updates, and call actions through HTTP or gRPC APIs.
- Stream gateway events over Server-Sent Events, WebSockets, or gRPC.
- Deliver configured webhook events through a persistent outbox with retry and dead-letter handling.
- Cache media by SHA-256 and expose health probes and Prometheus-style metrics.

## Requirements

- A stable Rust toolchain with Cargo (the project uses Rust edition 2024).
- A writable local directory for the application database, session protocol stores, media cache, and logs.
- Network access from the host to WhatsApp and, if configured, to the webhook receiver.

## Configure

Create `uwah.toml` in the repository root. Use absolute paths for persistent data directories in production.

```toml
[database]
dir = "./data/uwah.db"

[service]
host = "127.0.0.1"
port = 6767
# Use a long, randomly generated secret. Do not commit this file.
auth = "replace-with-a-long-random-secret"

[media]
dir = "./data/media"
cache_ttl_seconds = 604800
max_file_size = 104857600
max_concurrent_downloads = 3

[sessions]
dir = "./data/sessions"

[filter]
contacts = true
groups = true
channels = true
transmission = true

# Optional. Omit this section to disable webhook delivery.
[webhook]
enabled = false
# url = "https://hooks.example.com/uwah"
# secret = "replace-with-a-separate-webhook-signing-secret"
```

`service.port` is the HTTP port. The gRPC server listens on the next port (`port + 1`). Both listeners use cleartext transport; terminate TLS at a trusted reverse proxy when traffic crosses an untrusted network. The `auth` value can be the token itself or its SHA-256 hex digest.

The `.gitignore` excludes local `.env` files, common private-key formats, local databases, logs, and build output. Keep credentials and WhatsApp session stores out of version control.

## Run locally

```sh
cargo run
```

On startup, Uwah reads `uwah.toml`, opens the configured SQLite database, applies embedded SQLx migrations, restores eligible sessions, and starts the HTTP and gRPC servers. The process must be able to write to all configured data directories.

To check or lint the source without starting the gateway:

```sh
cargo check --all-targets
cargo clippy --all-targets
```

Unit and isolated HTTP integration tests run with:

```sh
cargo test --all-targets
```

GitHub Actions runs these checks and tests on pushes and pull requests. Integration tests use a temporary SQLite database and do not connect to WhatsApp.

## First session

The protected HTTP API uses a Bearer token. Create a session, then open its scan page and link it from WhatsApp's **Linked devices** screen.

```sh
curl -X POST http://127.0.0.1:6767/sessions \
  -H 'Authorization: Bearer replace-with-a-long-random-secret' \
  -H 'Content-Type: application/json' \
  -d '{"id":"primary"}'
```

Open `http://127.0.0.1:6767/sessions/primary/scan?token=replace-with-a-long-random-secret` in a browser. Query-string credentials can be recorded in browser history, proxy logs, and telemetry. Restrict access to this page and avoid sharing the URL.

## Send a message

```sh
curl -X POST \
  http://127.0.0.1:6767/sessions/primary/chats/15551234567%40s.whatsapp.net/messages \
  -H 'Authorization: Bearer replace-with-a-long-random-secret' \
  -H 'Content-Type: application/json' \
  -d '{"body":"Hello from Uwah"}'
```

Use the recipient's WhatsApp JID in the path. For a quoted reply, include `quoted_message_id` in the JSON body. See the [REST API reference](docs/API.md) for endpoint details and payloads.

## Interfaces

| Interface | Default address | Authentication |
| --- | --- | --- |
| REST API | `http://127.0.0.1:6767` | `Authorization: Bearer <token>` on protected routes |
| Swagger UI | `http://127.0.0.1:6767/swagger` | Browser token via `?key=` or `?token=`; API requests still require Bearer auth |
| gRPC | `127.0.0.1:6768` | `authorization: Bearer <token>` metadata |
| QR pairing page | `/sessions/{id}/scan` | Browser query token or Bearer authorization |

Health probes and `/metrics` are public HTTP routes. The QR page and Swagger use browser-oriented authentication. See the [security guidance](docs/API.md#security-and-authentication) before exposing the service.

## Documentation

- [REST API](docs/API.md)
- [gRPC API](docs/GRPC.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Database and migrations](docs/DATABASE.md)
- [Webhook delivery and DLQ](docs/WEBHOOKS.md)
- [Roadmap](docs/ROADMAP.md)

The `proto/uwah/v1/` directory contains the versioned Protocol Buffer contracts. The `migrations/` directory is the authoritative application database schema history.

## License

No license file or package license is currently present in this repository. Contact the maintainers before redistributing the project.
