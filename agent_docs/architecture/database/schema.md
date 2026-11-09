# Database Schema

The gateway application database is SQLite, managed by SQLx. The authoritative schema is the ordered migration set in `migrations/`.

The checked-in migrations define session, chat, message, contact, group-participant, call, and media-cache structures, as well as indexes and webhook outbox/DLQ structures. Some historical migration files are empty in this checkout, so documentation or model definitions alone cannot establish what exists in a newly initialized database. Review the full migration sequence before depending on session-cascade or group-metadata behavior.

See [Database and migrations](../../../docs/DATABASE.md) for the current details.
