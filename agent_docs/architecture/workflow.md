# Runtime Workflow

## Startup

`src/main.rs` initializes logging and configuration, opens the SQLite pool and applies migrations, creates shared application state, and awaits connected-session auto-boot. It then starts the outbox and maintenance workers, starts gRPC on `service.port + 1`, and serves the Axum HTTP API.

## Requests and events

HTTP and gRPC handlers validate requests and bearer credentials, obtain a session client when required, call the WhatsApp library, and persist application data through repositories. Incoming events are dispatched to handlers and published to configured event transports. Webhook delivery is asynchronous and at-least-once.

## Shutdown

After HTTP graceful shutdown, the process cancels and awaits gRPC, outbox, and maintenance tasks. It then stops active sessions, joins their workers, and closes the database pool. Auto-boot has already completed before the HTTP server starts.

See [the architecture guide](../../docs/ARCHITECTURE.md) and [webhook operations](../../docs/WEBHOOKS.md).
