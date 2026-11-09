# Database Schema

The application database is SQLite, managed by SQLx. The authoritative schema is the ordered SQL migration set under `migrations/`; do not infer schema objects from roadmap text alone.

The initial migration creates sessions, chats, messages, contacts, group participants, calls, and media cache. The performance migration adds indexes. Three historical migration files are empty in this checkout. A later additive migration creates outbox and DLQ objects when absent. The empty session-cascade and group-metadata migrations remain under review.

See [the database guide](../../../docs/DATABASE.md) for table and pragma details.
