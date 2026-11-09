# Architecture

Uwah is a single-process gateway that connects client applications to WhatsApp through the `whatsapp-rust` library. Axum serves the HTTP API and pairing pages; Tonic serves the gRPC API. SQLx manages application data in SQLite, while the WhatsApp library owns each session's protocol store.

## System overview

```text
HTTP clients ── REST / SSE / WebSocket ──┐
                                         ├── AppState ── repositories ── SQLite application DB
gRPC clients ── Tonic services ──────────┘       │
                                                 ├── SessionManager ── WhatsApp workers
                                                 ├── Event broadcast bus
                                                 └── Outbox worker ── configured webhook receiver

Browser ── QR pairing page ── SessionManager
Each WhatsApp worker ── isolated protocol store under [sessions].dir
```

The gateway and session protocol stores are distinct persistence domains:

- **Application database:** configured by `[database].dir`; stores session metadata, chats, messages, contacts, calls, media-cache metadata, and webhook outbox/DLQ data.
- **Protocol stores:** configured by `[sessions].dir`; one library-managed store per WhatsApp session contains credentials and cryptographic protocol state. Application repositories do not query these stores.

See [Database and migrations](DATABASE.md) for the schema contract and caveats.

## Process lifecycle

At startup, `main` initializes logging, loads and validates `uwah.toml`, opens the SQLite pool, and applies embedded SQLx migrations. It creates shared application state and restores persisted sessions marked `CONNECTED`, subject to a concurrency limit. It then starts the outbox and database-maintenance tasks, starts gRPC on `service.port + 1`, binds the HTTP listener, and serves requests.

On HTTP shutdown, the process cancels and joins the gRPC, outbox, and maintenance tasks, stops active WhatsApp workers, and closes the application database pool. Shutdown is coordinated, but no fixed deadline is currently enforced.

## Session lifecycle

`SessionManager` tracks workers in memory and deduplicates concurrent startup attempts. A worker owns a WhatsApp client, cancellation token, and task handle. Stopping a session cancels the worker and disconnects the client; unexpected worker completion removes its active entry. Session metadata remains in the application database independently of in-memory worker state.

Creating or restarting a session can establish network connections to WhatsApp and create or modify protocol-store files. These operations should be treated as external side effects.

## Request and event flow

Protected HTTP routes pass through Bearer-token middleware. gRPC services use a Tonic interceptor with the same configured service credential. Handlers validate request data, use a running session client for WhatsApp operations, and use repository modules for application persistence.

Inbound WhatsApp events are dispatched to domain handlers. Handlers may update the application database, publish a `GatewayEvent`, and enqueue a webhook event. The in-process event bus feeds SSE, WebSocket, and gRPC streams. This broadcast bus is bounded and non-durable; consumers can miss events while disconnected or when they fall behind the buffer.

## Events and webhooks

Live event transports:

- HTTP Server-Sent Events: `/events` and `/sessions/{id}/events`.
- HTTP WebSockets: `/ws` and `/sessions/{id}/ws`.
- gRPC server stream: `uwah.v1.EventStreamService/StreamEvents`.

Webhook delivery follows a separate persistent path. Events are written to the outbox and dispatched by a background worker. Failed attempts are scheduled for retry; permanently rejected or exhausted events move to the DLQ. Successful outbox rows are deleted. Delivery is at-least-once, so webhook consumers should deduplicate by event ID. See [Webhook delivery](WEBHOOKS.md).

## Network interfaces

The HTTP listener uses `[service].host` and `[service].port`; the gRPC listener uses the next port. REST and gRPC do not terminate TLS internally. Place both behind a trusted TLS-capable proxy or use a secure tunnel when remote access is required.

The HTTP server exposes health probes and metrics, authenticated API routes, the scan portal, and Swagger UI. The gRPC server exposes the services defined in `proto/uwah/v1/` and enables Server Reflection v1. REST and gRPC feature parity is not guaranteed.

## Operational endpoints

- `/health`: database connectivity.
- `/health/live`: process liveness.
- `/health/ready`: database and media-directory accessibility, plus an informational active-session count.
- `/health/startup`: application schema availability.
- `/metrics`: basic Prometheus text metrics.

The active-session count does not determine readiness. See [REST API](API.md) and [gRPC API](GRPC.md) for protocol details.
