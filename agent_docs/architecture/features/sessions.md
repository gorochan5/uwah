# WhatsApp Sessions

Session metadata is stored in the gateway database. Each session's WhatsApp protocol state is isolated in its own library-managed SQLite store. `SessionManager` deduplicates concurrent startup, retains the client and worker handle, and uses a cancellation token for stop.

Unexpected bot-worker completion removes the session from the in-memory active registry. Explicit stop disconnects the client and joins its worker. Startup restores persisted sessions marked `CONNECTED`, with a concurrency limit.

See [the API reference](../../../docs/API.md) and [the architecture guide](../../../docs/ARCHITECTURE.md).
