# Security Model

## Authentication

`[service].auth` is the shared credential for protected REST and gRPC application routes. It can be configured as plaintext or as the SHA-256 hex digest of the token. Comparisons use constant-time verification.

- REST: `Authorization: Bearer <TOKEN>`.
- gRPC: `authorization: Bearer <TOKEN>` metadata.
- Swagger and the QR pairing page support browser query tokens. Query credentials can appear in browser history, proxy logs, and telemetry; avoid sharing those URLs.
- Health probes, `/metrics`, and gRPC reflection are not protected by the application Bearer middleware. Restrict access at the network boundary where needed.

## Transport and data

The HTTP and gRPC listeners do not terminate TLS. Use a trusted TLS reverse proxy or secure tunnel for untrusted networks. Webhook requests can be signed with HMAC-SHA256; the receiver is responsible for enforcing any signature timestamp age window.

Media paths are checked against the configured cache root. Application data and WhatsApp protocol stores may contain sensitive personal and cryptographic information. Limit filesystem permissions and keep them out of source control.

See the [REST security guidance](../../docs/API.md#security-and-authentication), [gRPC reference](../../docs/GRPC.md#authentication-and-transport), and [webhook guide](../../docs/WEBHOOKS.md).
