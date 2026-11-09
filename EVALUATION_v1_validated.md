# Validated UWAH Evaluation

This document validates the findings in [`EVALUATION_v1.md`](EVALUATION_v1.md) against the repository and records the resulting reliability priorities. It is a static code and documentation review; it does not claim database inspection or runtime verification.

## Assessment

The project already has REST and gRPC APIs, session workers, an in-process event bus, SSE and WebSocket streams, a webhook outbox/DLQ, media caching, basic Prometheus metrics, and separate SQLite stores for gateway data and WhatsApp protocol state.

The most valuable near-term work is operational reliability. Product features such as scheduled messages, templates, polls, rich interactive messages, SDKs, and distributed deployment remain deferred until a concrete need exists. SQLite remains the default local store. Turso may be evaluated through a compatibility prototype before adoption.

## Findings and implementation status

| Finding | Validation | Current status |
| --- | --- | --- |
| Auto-boot tasks were detached | `auto_boot_sessions` originally spawned per-session tasks without awaiting them. | Fixed: auto-boot uses a `JoinSet` and is awaited before the HTTP server starts. |
| Session worker exit left stale `Running` state | The bot task completion was only logged. | Fixed: unexpected completion removes the active entry; explicit stop cancels and joins the worker. |
| Global shutdown did not await daemon tasks | Outbox, maintenance, and gRPC handles were discarded. | Fixed: shutdown cancels and awaits these handles before closing the pool. |
| Readiness used a hard-coded media path | It created `data/media_cache` regardless of configuration. | Fixed: it uses `config.media.dir`. |
| WebSocket peer task could outlive its partner | The first task to finish left the other detached. | Fixed: a shared cancellation token stops both tasks and the handler joins them. |
| Upload temp names could collide | Names used milliseconds and process ID, and `File::create` could truncate. | Fixed: HTTP and streaming gRPC uploads use UUID names and exclusive file creation. |
| Outbox migration file was empty | The historical migration has zero bytes. | Partially addressed: a new additive migration creates missing outbox/DLQ tables and indexes without changing historical checksums. The empty migration files remain. |
| Session-cascade and group-metadata migration files are empty | Both files have zero bytes. | Unresolved: review the intended schema and migration history before repairing. No database was inspected. |

## Remaining priorities

### P0 — Finish reliability foundations

- Review clean-install and upgrade migration behavior. Preserve checksums of migrations that may already have been applied. Resolve the empty session-cascade and group-metadata migration history against the intended schema.
- Add a bounded shutdown deadline and log tasks that fail to drain before it expires.
- Record worker exit reasons and make observed lifecycle state available to operations.

### P1 — Improve supportability when needed

- Add outbox pending count, oldest pending age, retry count, and DLQ metrics.
- Add session diagnostics only for values backed by direct observation, such as worker status, last event time, and reconnect count.
- Replace QR portal query-token propagation with a short-lived, narrowly scoped browser credential or session mechanism.
- Add idempotency to the most-used send routes if client retries are a real source of duplicate messages. Define key scope, retention, and `pending`/`sent`/`unknown` outcomes. Do not promise exactly-once delivery across the WhatsApp network boundary.

### P2 — Require a concrete deployment or client need

- Durable event replay requires a retained event log, stable cursor, retention policy, and authorization model. The webhook outbox is not an event log: successful deliveries are deleted.
- Scoped API credentials and administrative audit events are most valuable when mutually untrusted clients share one gateway.
- Choose a small operator dashboard or CLI based on actual operational workflows.
- Prototype Turso compatibility with current SQLx queries, migrations, transaction requirements, and local development before changing the storage decision.

### Deferred

Scheduled and recurring messages, templates, polls, rich interactive messages, live location, community/channel management, transcription, official SDKs, PostgreSQL, distributed coordination, and Kubernetes are deferred. Revisit them when users request them and after validating library/protocol support or a concrete scaling requirement.

## Conclusion

The original evaluation correctly identified worker supervision, orderly shutdown, readiness, and migration history as high-value areas. The runtime fixes listed above are now in place. Migration history remains the principal unresolved foundation; new product capabilities should wait until it is understood and the operational behavior is documented accurately.
