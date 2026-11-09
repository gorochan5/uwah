# Database Access Patterns

The application database is accessed through SQLx pools and the modules under `src/repository/`. HTTP and gRPC handlers use these repositories for gateway records; WhatsApp protocol stores are owned by the WhatsApp library and accessed through its client and storage abstractions.

Several message, contact, and media operations use batch queries or transactions. Webhook events may be enqueued in the same transaction as the state change that produced them. The exact query behavior is defined in repository code and migrations.

See [Database and migrations](../../../docs/DATABASE.md) for persistence boundaries, migration caveats, and SQLite settings.
