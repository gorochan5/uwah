# Architecture Overview

Uwah is a single-process WhatsApp gateway written in Rust. It manages WhatsApp sessions and exposes REST, gRPC, and live event-streaming interfaces. Application data is stored in SQLite; WhatsApp protocol state is maintained separately by `whatsapp-rust` for each session.

## Product boundaries

Uwah provides a gateway and local persistence layer. It is not a CRM, a distributed database, or a durable event replay service. SQLite remains the default application store. Alternative storage or distributed components require a demonstrated deployment need and a compatibility prototype.

## Core components

- Axum HTTP API, browser-based QR pairing page, SSE, and WebSockets.
- Tonic gRPC services defined under `proto/uwah/v1/`.
- `SessionManager` for worker startup, deduplication, and shutdown.
- SQLx-managed application database and per-session library-managed protocol stores.
- In-process event broadcast and optional persistent webhook outbox/DLQ.

See [Architecture](../../docs/ARCHITECTURE.md), [REST API](../../docs/API.md), and the [roadmap](../../docs/ROADMAP.md).
