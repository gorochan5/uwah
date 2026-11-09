# gRPC API

The Tonic server listens on `service.port + 1` (default `6768`). Protocol Buffer definitions are in `proto/uwah/v1/`; generated Rust code is included by the gRPC module. Server Reflection v1 is enabled.

All Tonic services require `authorization: Bearer <TOKEN>` metadata validated against `[service].auth`. The reflection service supports discovery. The listener uses cleartext HTTP/2 unless TLS is terminated externally.

The server exposes health/metrics, sessions, chats, groups, participants, messages, contacts, calls, media, event streaming, and webhook DLQ services. See [the complete gRPC reference](../../../docs/GRPC.md).
