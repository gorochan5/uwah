# Storage Structures

UWAH uses two storage tiers:

- `uwah.db` stores gateway application data through SQLx and SQLite.
- `sessions/{id}.db` stores WhatsApp protocol and cryptographic state through the library's SQLite backend.

Application repositories own gateway queries. The application must not directly inspect or mutate per-session protocol databases. Schema changes belong in versioned SQL migrations; preserve checksums of historical migrations that may already have been applied.

See [the database guide](../../../docs/DATABASE.md).
