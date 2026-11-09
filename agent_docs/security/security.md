# Security Model

## Credentials

`[service].auth` is required and is shared by protected REST and gRPC APIs. Credentials may be configured as plaintext or as the corresponding SHA-256 digest. Comparisons use the security module's constant-time verification.

- REST uses `Authorization: Bearer <TOKEN>`.
- Tonic services use gRPC metadata `authorization: Bearer <TOKEN>`.
- Swagger and the QR scan portal support query tokens for browser use. Query credentials may be exposed through logs, history, and telemetry; avoid sharing or externally exposing such URLs.

## Transport and payloads

The HTTP and gRPC listeners do not terminate TLS. Use a trusted TLS reverse proxy or secure tunnel on untrusted networks. Webhook payloads can be HMAC-SHA256 signed. Media services validate canonical paths to prevent traversal.

See [the API security model](../../docs/API.md) and [the gRPC guide](../../docs/GRPC.md).
