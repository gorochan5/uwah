# Runtime Workflow

## Startup

`src/main.rs` initializes logging, loads and validates `uwah.toml`, opens the configured SQLite pool, and applies embedded migrations. It builds shared application state, restores eligible connected sessions, starts the webhook outbox and database-maintenance tasks, starts gRPC on `service.port + 1`, and then serves HTTP.

## Requests and events

Protected HTTP and gRPC requests are authenticated with the configured service token. Handlers validate request data, call the WhatsApp client when an operation requires it, and persist gateway state through repository modules. Incoming WhatsApp events are handled by domain handlers and may be broadcast to live event consumers or stored for webhook delivery.

The broadcast bus is in-memory and bounded. Webhook delivery is persistent and at-least-once; receivers should deduplicate events.

## Shutdown

After HTTP graceful shutdown, the process cancels and joins gRPC, outbox, and maintenance tasks, stops active session workers, and closes the database pool. There is no fixed shutdown deadline.

See [Architecture](../../docs/ARCHITECTURE.md) and [Webhook delivery](../../docs/WEBHOOKS.md).
