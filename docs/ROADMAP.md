# Roadmap

This roadmap prioritizes reliability and supportability over adding endpoints without a demonstrated need. Items reflect the current code and known migration caveats; they are not release commitments.

## Design principles

- Keep SQLite as the default local application store until a concrete deployment requirement justifies another backend.
- Maintain one active WhatsApp worker per session.
- Keep network and database awaits outside shared state locks.
- Preserve the webhook outbox and document its at-least-once delivery semantics.
- Bound and observe background work, and coordinate shutdown explicitly.
- Keep project-authored documentation in English and aligned with implementation.

## Current baseline

- REST and gRPC application APIs share a configured Bearer credential.
- Session protocol stores are isolated from the gateway application database.
- Live events are available through the in-process broadcast bus, SSE, WebSockets, and gRPC streaming.
- Optional webhook delivery uses a persistent outbox and dead-letter queue.
- Health endpoints and basic Prometheus text metrics are available.
- GitHub Actions checks, lints, and runs unit and isolated HTTP integration tests on pushes and pull requests.

## Priority 0: close reliability gaps

- Review clean-install and upgrade behavior for the empty historical session-cascade and group-metadata migrations. Preserve checksums of migrations that may already have been applied.
- Add a shutdown deadline and report tasks that fail to drain before it expires.
- Record worker exit reasons and expose only lifecycle diagnostics backed by observed state.
- Expand integration coverage to authenticated database-backed session and repository flows using isolated test state.

## Priority 1: improve operations when usage warrants it

- Add outbox depth, oldest-pending age, retry, and DLQ metrics.
- Add session diagnostics for observed worker state, last event time, and reconnect count.
- Replace QR query-token propagation with a short-lived, narrowly scoped browser credential.
- Add send idempotency if client retries cause duplicate messages. Define key scope, retention, and uncertain delivery outcomes; do not promise exactly-once delivery across WhatsApp.

## Priority 2: require a concrete product or deployment need

- Add durable event replay only if consumers need recovery after disconnects. This requires an event log, stable cursor, retention policy, and authorization model; the webhook outbox is not a replay log.
- Consider scoped API credentials and administrative audit events when mutually untrusted clients share a gateway.
- Choose an operator dashboard or CLI based on actual support workflows.
- Prototype Turso compatibility against SQL queries, migrations, transactions, and local development before changing the storage decision.

## Deferred

Scheduled messages, templates, polls, richer interactive messages, live location, community and channel management, transcription, official SDKs, PostgreSQL, distributed coordination, and Kubernetes are deferred until there is a validated need and confirmed support in `whatsapp-rust` or the WhatsApp protocol.

## Documentation maintenance

- Treat `agent_docs/whatsapp-rust.md` as a vendored upstream reference snapshot. Preserve its wording and refresh it only from its upstream source.
- Keep API, gRPC, and schema references aligned with route handlers, Protobuf contracts, and versioned migrations.
- Do not mark roadmap items complete until implementation and documentation agree.
