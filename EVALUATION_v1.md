# UWAH Evaluation — Original Recommendations (English Translation)

> **Historical review:** This document preserves the original evaluation and its recommendations. Some findings were later corrected or superseded. For current implementation status and priorities, see [`EVALUATION_v1_validated.md`](EVALUATION_v1_validated.md) and the [roadmap](docs/ROADMAP.md).

This document is an English translation of the original assessment; it is not a description of the current implementation.

## Overall assessment

UWAH has grown beyond a thin library wrapper into a communication gateway with its own architecture. Strong existing choices include separate gateway and per-session cryptographic storage, a session manager, REST and gRPC APIs, broadcast/SSE/WebSocket event delivery, a transactional webhook outbox and DLQ, SHA-256 media deduplication, architecture documentation, OpenAPI, and Protobuf contracts.

Several capabilities previously thought missing are already present: session reconnection, startup recovery, gRPC authentication, Prometheus metrics, and rate limiting. The next maturity step is operational guarantees rather than simply adding more WhatsApp endpoints.

## Security and exposure

### Granular authentication

The current shared `service.auth` secret can be suitable for a personal installation but limits multi-client use. Possible future additions include revocable API keys, scopes such as `sessions:read` and `messages:write`, session-bound credentials, expiration/rotation, and administrative audit events.

### TLS and QR portal credentials

The HTTP and gRPC listeners do not provide native TLS. Local use may be acceptable; externally exposed deployments should use a reverse proxy, secure tunnel, or native TLS.

The QR portal accepts a token in its query string and carries it into HTMX polling URLs. Query credentials can appear in logs, browser history, and observability systems. A short-lived, narrowly scoped pairing credential would reduce exposure.

## Event reliability

The in-process `tokio::sync::broadcast` bus is not persistent; disconnected consumers may miss events. A recovery endpoint such as `GET /events?after=<cursor>` would require a retained event log, stable sequence/cursor, retention rules, and optionally per-consumer acknowledgement. The existing webhook outbox should remain for low-latency delivery, but it is not itself a replay log because successful webhook rows are deleted.

## Message management

Potential features include scheduled and recurring messages, reusable templates, and send idempotency. Idempotency is useful when a client loses the HTTP response and retries, but the WhatsApp network call and local database write cannot form one atomic transaction. The API must represent uncertain outcomes and must not promise exactly-once delivery.

## WhatsApp ecosystem extensions

Possible extensions include polls, product/catalog messages, live location, communities, interactive messages, newsletter/channel management, incremental contact sync, group change history, disappearing-message improvements, complete vCard parsing, and optional audio transcription. These depend on `whatsapp-rust` and protocol support; they are not necessarily simple endpoint additions.

## Operations and observability

The current `/metrics` endpoint exposes basic message, webhook, and session metrics. Potential additions include message latency/delivery status, consumer lag, outbox backlog and oldest-event age, reconnect counts, media-cache size, and HTTP request counts. A broader instrumentation library should be adopted only if histograms and label management justify it.

A small administrative dashboard could show session state, pending QR codes, message activity, webhook failures, DLQ entries, storage use, and recent logs. A session diagnostics endpoint could expose worker/socket state, last event/message times, reconnect count, and storage accessibility, provided each field is directly observed.

## Findings from the original code review

The original review identified these items for validation:

- `src/main.rs`: auto-boot spawned tasks and returned without awaiting them.
- `src/api/session_manager.rs`: bot-worker completion was not fully supervised, allowing a stale `Running` state.
- `src/api/http/health.rs`: readiness used a hard-coded media path instead of the configured directory.
- `src/api/http/ws.rs`: one WebSocket task could outlive the other.
- `src/events/outbox_worker.rs`: shutdown did not await dispatch tasks; retry jitter was deterministic.
- `src/api/http/media.rs`: temporary upload names could collide.
- QR scan credentials could travel in query strings.
- Three October 2026 migration files were empty and needed review.

See the validated evaluation for current implementation status and remaining work.

## SDKs and command-line tools

~~Rust SDK — Priority 1: a native client with shared types, authentication, and stream reconnection.~~

~~Python SDK — Priority 2: integrations, automation, and processing applications.~~

~~TypeScript SDK — Priority 3: browser, Node.js, and external-tool integrations.~~

A CLI could support session listing/creation/restart/diagnostics, message sending, DLQ replay, and server status. It may be useful for operations but is not urgent while the APIs and docs cover required workflows.

## Original proposed priority order

The original evaluation proposed worker supervision, deterministic shutdown, send idempotency, and readiness fixes as P0; persistent event replay, granular permissions, scheduled messages, hierarchical rate limiting, and session diagnostics as P1; and a dashboard, polls/interactivity, and alternate storage as P2. It proposed distributed clustering at P3.

~~P2: Rust, Python, and TypeScript SDKs.~~ Not currently needed.

PostgreSQL, Redis, NATS, and Kubernetes should not be introduced without a demonstrated scaling or deployment need. The original author preferred SQLite and considered Turso as a possible future alternative.
