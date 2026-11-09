# Storage Structures

Uwah maintains two distinct SQLite-backed persistence domains:

1. The configured application database stores gateway metadata and operational records through SQLx.
2. The configured sessions directory contains one WhatsApp protocol store per session, managed by `whatsapp-rust`.

Keep both domains private, writable by the service account, and outside source control. Back up the application database and matching protocol stores together. Use the [database guide](../../../docs/DATABASE.md) for migration and backup considerations.
