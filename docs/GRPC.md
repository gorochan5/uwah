# gRPC API Reference

Uwah serves gRPC over HTTP/2 on `service.port + 1`. With the default HTTP port of `6767`, gRPC listens on `127.0.0.1:6768` when `service.host` is `127.0.0.1`. The listener uses cleartext HTTP/2; it does not terminate TLS.

Protocol Buffer definitions are maintained in [`proto/uwah/v1/`](../proto/uwah/v1/). The server enables gRPC Server Reflection v1.

## Authentication and transport

All application services require gRPC metadata:

```text
authorization: Bearer <service-token>
```

The token is `[service].auth`, which may be configured as plaintext or as its SHA-256 hex digest. The reflection service is registered without the application-service authentication interceptor; restrict network access to the gRPC listener accordingly.

The built-in listener does not provide TLS. Use a trusted proxy or secure tunnel for remote connections. Ensure the proxy supports HTTP/2 gRPC traffic.

## Discovery

Install `grpcurl` and inspect the server without a local copy of the `.proto` files:

```sh
grpcurl -plaintext 127.0.0.1:6768 list
grpcurl -plaintext 127.0.0.1:6768 describe uwah.v1.SessionService
```

To call an authenticated method:

```sh
grpcurl -plaintext \
  -H 'authorization: Bearer YOUR_SERVICE_TOKEN' \
  -d '{}' \
  127.0.0.1:6768 uwah.v1.SessionService/ListSessions
```

## Services and methods

| Service | Methods |
| --- | --- |
| `HealthService` | `Check`, `Watch`, `CheckLiveness`, `CheckReadiness`, `CheckStartup`, `ExportMetrics` |
| `EventStreamService` | `StreamEvents` (server stream) |
| `SessionService` | `ListSessions`, `CreateSession`, `GetSession`, `DeleteSession`, `RefreshQr`, `RestartSession`, `LogoutSession`, `RequestPairingCode` |
| `ChatService` | `SetGlobalPresence`, `SetChatPresence`, `ListChats`, `GetChat`, `DeleteChat`, `MarkAsRead`, `MuteChat`, `PinChat`, `ArchiveChat`, `SetSubject`, `SetDescription`, `SetEphemeralTimer`, `SetGroupAnnouncement`, `SetGroupLock`, `GetInviteLink`, `LeaveGroup` |
| `GroupService` | `CreateGroup`, `JoinGroup` |
| `GroupParticipantService` | `ListParticipants`, `AddParticipants`, `RemoveParticipant`, `PromoteParticipants`, `DemoteParticipants` |
| `MessageService` | `ListMessages`, `SendMessage`, `SendLocation`, `SendContact`, `SendMedia`, `EditMessage`, `DeleteMessage`, `ReactMessage`, `RevokeMessage` |
| `ContactService` | `ListContacts`, `UpsertContact`, `SyncContacts`, `CheckContacts`, `GetContact`, `DeleteContact`, `GetProfilePicture`, `BlockContact`, `UnblockContact` |
| `CallService` | `ListCalls`, `GetCall`, `DeleteCall`, `RejectCall` |
| `MediaService` | `DownloadMedia` (server stream), `UploadMedia` (client stream), `UploadMediaUnary` |
| `WebhookDlqService` | `ListDlq`, `GetDlq`, `ReplayDlq`, `ReplayAllDlq`, `DeleteDlq` |

REST and gRPC functionality overlaps, but methods and request/response schemas are not guaranteed to have one-to-one parity. The `.proto` files are authoritative for field names, optionality, and wire types.

## Examples

### Health and readiness

```sh
grpcurl -plaintext \
  -H 'authorization: Bearer YOUR_SERVICE_TOKEN' \
  -d '{}' \
  127.0.0.1:6768 uwah.v1.HealthService/CheckReadiness
```

### Create a session

```sh
grpcurl -plaintext \
  -H 'authorization: Bearer YOUR_SERVICE_TOKEN' \
  -d '{"id":"primary"}' \
  127.0.0.1:6768 uwah.v1.SessionService/CreateSession
```

Creating a session may connect to WhatsApp and write protocol-store data.

### Stream events

```sh
# All sessions
grpcurl -plaintext \
  -H 'authorization: Bearer YOUR_SERVICE_TOKEN' \
  -d '{}' \
  127.0.0.1:6768 uwah.v1.EventStreamService/StreamEvents

# One session
grpcurl -plaintext \
  -H 'authorization: Bearer YOUR_SERVICE_TOKEN' \
  -d '{"session_id":"primary"}' \
  127.0.0.1:6768 uwah.v1.EventStreamService/StreamEvents
```

The event stream is live and in-memory; it does not replay events missed while disconnected.

### Send a message

```sh
grpcurl -plaintext \
  -H 'authorization: Bearer YOUR_SERVICE_TOKEN' \
  -d '{"session_id":"primary","chat_jid":"15551234567@s.whatsapp.net","body":"Hello from gRPC"}' \
  127.0.0.1:6768 uwah.v1.MessageService/SendMessage
```

Use the field names defined by [`messages.proto`](../proto/uwah/v1/messages.proto). Other examples and the full REST route list are in the [REST API reference](API.md).

## Error mapping

RPC handlers return standard gRPC status codes. Common mappings include:

| Status | Typical cause |
| --- | --- |
| `UNAUTHENTICATED` | Missing, malformed, or invalid Bearer metadata. |
| `INVALID_ARGUMENT` | Invalid request fields or identifiers. |
| `NOT_FOUND` | Requested session or entity does not exist. |
| `PERMISSION_DENIED` | A request is rejected by a permission or media-path boundary. |
| `INTERNAL` | Database, storage, or unexpected server failure. |

The precise status is method-specific. Consult the service implementation and `.proto` contract when handling less common errors.

## Operational notes

- Protect the gRPC port at the host firewall or ingress layer; reflection reveals service and message schemas.
- Keep the HTTP and gRPC listeners on trusted interfaces unless a secured proxy is in front of them.
- Do not log Bearer metadata or include production credentials in shell history.
- Media upload/download and event streaming use gRPC streaming where defined by the protocol.
