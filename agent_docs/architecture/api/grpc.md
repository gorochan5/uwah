# gRPC API

The Tonic server listens on `service.port + 1` (default `6768`). Protocol Buffer contracts are stored in `proto/uwah/v1/`, and Server Reflection v1 is enabled.

Application services require `authorization: Bearer <TOKEN>` metadata. Reflection is registered separately and does not use the application-service interceptor. The listener uses cleartext HTTP/2; terminate TLS at a trusted proxy when required.

The services cover health and metrics, event streaming, sessions, chats, groups, participants, messages, contacts, calls, media, and webhook DLQ operations. REST and gRPC parity is not guaranteed. See the [gRPC reference](../../../docs/GRPC.md).
