# Media Cache

Downloaded media is stored under `[media].dir` and indexed by its SHA-256 hash in the application database. The cache tracks file size, MIME type, reference count, and last-access time. HTTP media retrieval supports byte ranges; upload size and concurrent downloads are configurable.

Expired and unreferenced entries are eligible for pruning according to repository logic. The readiness probe checks that the configured media directory can be created or accessed. Protect cached content and ensure the service account has appropriate filesystem permissions.

See the [REST API](../../../docs/API.md) and [database guide](../../../docs/DATABASE.md).
