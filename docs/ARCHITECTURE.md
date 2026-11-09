# Uwah Architecture

## 1. Overview

**Uwah** is a self-hosted, high-performance WhatsApp gateway written in Rust. It decouples client-facing applications from the underlying WhatsApp multi-device binary protocol using [`whatsapp-rust`](https://github.com/oxidezap/whatsapp-rust), Axum 0.8, and SQLite (SQLx).

---

## 2. High-Level Architecture

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

---

## 3. Dual-Database Storage Design

To guarantee cryptographic integrity and complete isolation between sessions, **Uwah strictly enforces a dual-database architecture**:

### A. Application State Database (`uwah.db`)
- **Engine**: SQLite managed by SQLx (`sqlx::SqlitePool`).
- **Configuration**: Configured via `uwah.toml` under `[database] dir`.
- **Concurrency**: Operates in **WAL mode** (`PRAGMA journal_mode = WAL`), `synchronous = NORMAL`, `foreign_keys = ON`, and **`busy_timeout = 30s`**.
- **Contents**:
  - `sessions`: Session registry (status, phone number, push name, active QR payload).
  - `chats`: Conversation threads (direct, group, newsletter).
  - `messages`: Inbound and outbound message records, status (`sent`, `received`), timestamps, reply context.
  - `contacts`: Address book, LID-to-Phone mappings, verified names.
  - `group_participants`: Membership tracking and admin permissions.
  - `calls`: Audio/video call history.
  - `media_cache`: Local file cache deduplication.

### B. Session Protocol Stores (`sessions/{id}.db`)
- **Engine**: SQLite managed internally by `whatsapp-rust-sqlite-storage` (Diesel + r2d2).
- **Configuration**: Root directory configured via `uwah.toml` under `[sessions] dir`.
- **Purpose**: Exclusively holds low-level WhatsApp protocol state: Signal prekeys, identity keys, sender keys, Noise handshake secrets, and app state sync keys.
- **Isolation**: Each connected WhatsApp number receives its own dedicated `.db` file. The application code never queries or mutates this file directly.

---

## 4. Concurrency & Lifecycle Management

1. **Static Configuration**:
   Loaded once at boot using `OnceLock<Config>`. Exposed to all handlers via `&'static Config` (zero heap allocations per request).
2. **Background Session Worker**:
   `SessionManager` boots an async `Bot` and retains a supervised worker handle with a cancellation token. Unexpected worker termination removes the session from the active registry. Explicit stop cancels and joins the worker.
3. **Inbound Event Processing**:
   The registered event callback dispatches incoming WhatsApp events to the event handlers. Message handlers persist message and chat state and enqueue webhook events transactionally where supported. Exact extraction and persistence behavior is implemented by the event handlers and repositories.
4. **Outbound Messaging Pipeline**:
   The REST API retrieves the `Arc<Client>` from `SessionManager`, builds a WhatsApp `wa::Message` (with `ContextInfo` when `quoted_message_id` is present), and dispatches asynchronously through WhatsApp's encrypted channel.

## 5. Event and delivery paths

Gateway events are published to an in-process broadcast bus and can be consumed through:

- HTTP SSE at `/events` or `/sessions/{id}/events`;
- HTTP WebSocket at `/ws` or `/sessions/{id}/ws`;
- gRPC `EventStreamService.StreamEvents`, optionally filtered by `session_id`.

Configured webhooks are delivered through a transactional outbox worker. Failed deliveries are retried and eventually moved to `webhook_dlq`, which is operable through the protected `/webhooks/dlq` endpoints as well as the gRPC `WebhookDlqService`. See [WEBHOOKS.md](WEBHOOKS.md).

## 6. gRPC Microservices Subsystem

Uwah exposes a gRPC endpoint on `service.port + 1` (default `6768`) across 11 Tonic services. Its surface overlaps with REST but is not guaranteed to have complete one-to-one feature parity:
- **Service Discovery**: Enabled via gRPC Server Reflection Protocol v1 (`tonic-reflection`), allowing dynamic inspection with `grpcurl` or GUI clients.
- **Service Modules**: `HealthService`, `EventStreamService`, `SessionService`, `ChatService`, `GroupService`, `GroupParticipantService`, `MessageService`, `ContactService`, `CallService`, `MediaService`, and `WebhookDlqService`.
- **Media & Event Streaming**: Native HTTP/2 chunked streaming for both file downloads (`64KB` chunks) and upload pipelines, as well as real-time event broadcasting.
- See detailed protocol specs in [docs/GRPC.md](GRPC.md).

## 7. Operational boundaries

REST protected routes require a Bearer token. Swagger and the scan portal have browser-specific query-token handling. The gRPC server is protected by a Tonic metadata interceptor requiring `authorization: Bearer <token>` validated against `[service].auth` in constant time (returning `unauthenticated` on any failure). Neither HTTP nor gRPC listeners configure TLS internally (TLS termination should be handled by a reverse proxy or ingress).

The startup probe confirms that the `sessions` table can be queried. The readiness probe checks database connectivity and the configured media directory. It also reports active session count; that count does not determine readiness.


