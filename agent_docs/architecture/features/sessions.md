# WhatsApp Sessions

Session metadata is stored in the gateway application database. Each session's WhatsApp authentication and protocol state is kept in a separate library-managed store under `[sessions].dir`.

`SessionManager` deduplicates concurrent startup, tracks active clients and worker tasks, and coordinates stop and disconnect operations. At startup, Uwah attempts to restore persisted sessions marked `CONNECTED`, subject to a concurrency limit. Session creation and reconnection require WhatsApp network access.

See the [REST API](../../../docs/API.md), [architecture guide](../../../docs/ARCHITECTURE.md), and [database guide](../../../docs/DATABASE.md).
