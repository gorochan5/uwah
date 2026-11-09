# UWAH Architecture Vision

UWAH is a self-hosted WhatsApp gateway written in Rust. It exposes HTTP/REST and gRPC interfaces, manages WhatsApp sessions through `whatsapp-rust`, persists application data in SQLite, and provides event streams and optional webhooks.

## Scope

The gateway manages sessions, translates API operations, and stores local application data. It is not a CRM or a distributed database. SQLite is the default storage engine; any alternative requires a compatibility prototype and a concrete deployment need.

## Current components

- Axum REST API and browser pairing portal.
- Tonic gRPC services with bearer-token authentication.
- `SessionManager` with supervised bot workers and cancellation.
- SQLite application database and separate per-session protocol stores.
- Broadcast events, SSE, WebSockets, gRPC streaming, and an optional transactional webhook outbox/DLQ.

See [the architecture guide](../../docs/ARCHITECTURE.md), [the API reference](../../docs/API.md), and [the roadmap](../../docs/ROADMAP.md) for current details.
