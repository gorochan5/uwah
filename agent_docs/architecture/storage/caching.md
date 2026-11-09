# Media Cache

Media metadata is stored in the application database and binary files are stored under `[media].dir`. Files are addressed by SHA-256 digest and reused by cache-aware upload/download paths. The configured size limit is enforced for uploads.

HTTP and streaming gRPC upload paths create unique temporary files exclusively before streaming bytes to disk. Keep temporary and final files within the configured media directory and preserve canonical-path checks on downloads.

See [the database guide](../../../docs/DATABASE.md) and [the API reference](../../../docs/API.md).
