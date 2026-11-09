# UWAH Roadmap

This roadmap follows the validated findings in [EVALUATION_v1_validated.md](../EVALUATION_v1_validated.md). It focuses on operational reliability. Product expansion is deferred until there is a concrete use case.

## Project principles

- Keep SQLite as the default local storage engine. Evaluate Turso only through a compatibility prototype before changing deployment guidance.
- Keep one active WhatsApp worker per session.
- Keep locks out of network and database awaits.
- Preserve the transactional outbox for webhook delivery and document its at-least-once semantics.
- Prefer observable, bounded background tasks and deterministic shutdown.
- Keep the user-facing project documentation in English.

## Current baseline

- REST/HTTP and gRPC APIs share a configured bearer secret.
- Sessions use isolated WhatsApp protocol stores; application data uses the configured SQLite database.
- Events are available through in-process broadcast, SSE, WebSockets, gRPC streams, and an optional webhook outbox/DLQ.
- `/metrics` exports basic Prometheus counters and gauges.
- Readiness checks database connectivity and the configured media directory.
- Background service and session tasks are joined or drained during orderly shutdown.
- Upload temporary files use exclusive creation and unique names.
- `20261109000000_repair_outbox_schema.sql` additively creates missing outbox/DLQ tables and indexes. Historical zero-byte migration files remain unchanged to preserve SQLx checksums.

## Priority 0 — Reliability foundations

- [x] Supervise session worker termination and remove dead workers from the active registry.
- [x] Join session workers during explicit stop and process shutdown.
- [x] Await auto-boot completion and supervise outbox dispatch tasks.
- [x] Cancel and join both sides of WebSocket connections.
- [x] Check the configured media directory in readiness.
- [x] Create upload temporary files exclusively with unique names.
- [x] Add a forward repair migration for missing outbox/DLQ schema objects.
- [ ] Review migration history and clean-install behavior without changing checksums of migrations that may already be recorded. Resolve the empty session-cascade and group-metadata migration history based on the intended schema contract.
- [ ] Add a bounded shutdown deadline and report tasks that fail to drain before it expires.
- [ ] Ensure worker exit reasons and session lifecycle transitions are visible in logs and health diagnostics.

## Priority 1 — Reliability driven by usage

- [ ] Add idempotency to the message-send routes most used by clients. Define key scope, retention, and `pending`/`sent`/`unknown` outcomes. Do not claim exactly-once delivery across the WhatsApp network boundary.
- [ ] Add outbox observability: pending count, oldest pending age, retries, and DLQ volume.
- [ ] Add session diagnostics only for values backed by observed state, such as worker status, last event time, and reconnect count.
- [ ] Replace QR portal query-token propagation with a short-lived, narrowly scoped browser credential or session mechanism.

## Priority 2 — Only when requirements justify it

- [ ] Add durable event replay only if consumers need recovery after disconnects. This requires a retained event log, stable cursor, retention policy, and authorization model; the webhook outbox is not that log.
- [ ] Add scoped API credentials and administrative audit events if the gateway is shared by mutually untrusted clients.
- [ ] Choose either a small operator dashboard or a CLI based on the actual operational workflow.
- [ ] Prototype Turso compatibility with the current SQLx queries, migrations, transaction requirements, and local-development workflow before adopting it.

## Deferred product work

Scheduled and recurring messages, templates, polls, richer interactive messages, live location, community/channel management, transcription, and official SDKs are deferred. Revisit them when users request them and after confirming support in `whatsapp-rust` and the WhatsApp protocol.

Distributed clusters, PostgreSQL, Redis/NATS, and Kubernetes are also deferred. The current single-process SQLite architecture does not establish a need for their operational cost.

## Documentation maintenance

- English is the official language for project-authored documentation, comments, and examples.
- Keep API and schema references aligned with the handlers, models, and versioned migrations.
- Treat `agent_docs/whatsapp-rust.md` as an upstream reference snapshot; preserve its source wording and update it only when refreshing that upstream snapshot.
- Do not mark roadmap items complete until the code and documented behavior agree.
