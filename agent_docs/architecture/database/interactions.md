# Database Interactions

`src/database/mod.rs` loads the configured SQLite URL, enables foreign keys, WAL mode, `synchronous=NORMAL`, a 30-second busy timeout, and the configured cache pragmas. SQLx applies embedded migrations during startup. The pool allows up to five connections.

Repositories in `src/repository/` encapsulate application queries. Inbound message handling can persist message state and enqueue webhook events transactionally. The periodic maintenance worker performs a passive WAL checkpoint and incremental vacuum.

Do not access the database directly during repository work. Consult [the database guide](../../../docs/DATABASE.md) and the migration files.
