# Database and Migrations

Uwah uses SQLite for gateway application data. SQLx opens the database at `[database].dir`, enables foreign keys, uses WAL mode, and applies the ordered migrations embedded from `migrations/` during startup. The migration files are the authoritative schema history; this guide describes the current checked-in files and does not replace migration review.

## Persistence domains

### Application database

The configured application database stores data used by the gateway API:

| Table | Purpose |
| --- | --- |
| `sessions` | Session identity, connection status, QR payload, and profile metadata. |
| `chats` | Per-session chat metadata and state. |
| `messages` | Per-session message history and delivery metadata. |
| `contacts` | Contact details and identifier mappings. |
| `group_participants` | Participant membership and role data. |
| `calls` | Call records. |
| `media_cache` | Media hashes, disk paths, sizes, and access metadata. |
| `outbox_events` | Pending webhook deliveries and retry state. |
| `webhook_dlq` | Events quarantined after permanent failure or retry exhaustion. |
| `group_metadata` | Group-level metadata where created by the active schema. |

Most application entities are scoped by `session_id`. Refer to the SQL migrations for exact columns, constraints, and indexes.

### WhatsApp protocol stores

The WhatsApp library maintains a separate protocol store for each session under `[sessions].dir`. These files hold authentication and cryptographic protocol state. Uwah treats them as library-owned data and does not use its SQLx repositories to query them.

Back up the application database and protocol stores together if you need a recoverable installation. Protect both as sensitive data.

## Migration history and clean-install caveats

Migrations are ordered by their timestamped filenames and applied by SQLx. Historical files `20261001000000_cascade_session_rename.sql` and `20261001020000_group_metadata.sql` are empty in this checkout. The outbox repair migration (`20261109000000_repair_outbox_schema.sql`) creates outbox and DLQ objects if they are missing, and the performance migration defines indexes.

Because some historical migrations are empty, the schema a fresh database receives may not match every design statement in older documents. Review the complete migration sequence before relying on cascade behavior or `group_metadata` being present. Do not edit a migration that may already have been applied: SQLx records checksums. Add a new migration for schema changes.

## SQLite settings

Pool initialization in `src/database/mod.rs` configures:

- Foreign-key enforcement.
- Write-ahead logging (`WAL`) and `synchronous = NORMAL`.
- A 30-second busy timeout.
- A 64 MiB SQLite page cache, a 256 MiB memory-map limit, and in-memory temporary storage.
- Incremental auto-vacuum mode.

The process runs a passive WAL checkpoint and an incremental vacuum approximately every 30 minutes. These settings do not make SQLite a multi-writer database; workloads still serialize writes. Keep the database on a reliable local filesystem and monitor free disk space.

## Backup and restore

Stop the gateway or use a SQLite-aware backup method that accounts for WAL files. Copying only the main database file while the gateway is active may omit committed data still present in the WAL. Preserve file permissions and back up the matching session protocol-store directory. Test restoration on a separate location before relying on a backup.

Do not place production data, credentials, or session stores in source control. The repository `.gitignore` covers the default local database and common local artifacts; custom paths must be protected by the operator.
