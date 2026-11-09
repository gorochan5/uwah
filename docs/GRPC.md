# Uwah WhatsApp Gateway — Complete gRPC API Reference

The **Uwah WhatsApp Gateway** provides a gRPC API on port `service.port + 1` (default: `6768`). Built with [Tonic](https://github.com/hyperium/tonic) and [Prost](https://github.com/tokio-rs/prost), it exposes corresponding services over HTTP/2. Its feature set overlaps with REST, SSE, and WebSockets but is not guaranteed to match them one-to-one.

---

## 1. Architecture & Protocol Fundamentals

### 1.1 gRPC Server Reflection vs Swagger UI

| Capability | REST / OpenAPI (Swagger UI) | gRPC / Protocol Buffers (Tonic Reflection) |
| :--- | :--- | :--- |
| **Port / Endpoint** | `http://127.0.0.1:6767/swagger` | `127.0.0.1:6768` (HTTP/2 cleartext / h2c) |
| **Discovery Mechanism** | OpenAPI 3.1 JSON schema (`/api-docs/openapi.json`) | gRPC Server Reflection Protocol v1 (`grpc.reflection.v1.ServerReflection`) |
| **Client Introspection** | Web browser, Postman, curl, OpenAPI codegen | `grpcurl`, BloomRPC, Postman (gRPC), Buf, IDE plugins |
| **Payload Encoding** | Textual JSON (`application/json`) | Binary Protocol Buffers (`application/grpc+proto`) |
| **Multiplexing** | HTTP/1.1 keep-alive or HTTP/2 | Native HTTP/2 multiplexed streams over a single TCP connection |
| **Streaming Style** | SSE (`text/event-stream`) & WebSockets (`uwah.v1+json`) | Bidirectional, server-streaming, and client-streaming gRPC |

Server Reflection allows CLI tools (e.g. `grpcurl`) and developer tooling to inspect and invoke any service or RPC method without manually compiling or supplying local `.proto` files.

---

## 2. Authentication & Transport Security

### 2.1 Metadata Authentication
Every gRPC request (except Server Reflection metadata discovery) must include a valid authorization token in the request metadata:

- **Metadata Header**: `authorization: Bearer <TOKEN>`
- **Validation**: Constant-time verification using `subtle::ConstantTimeEq` against the configured `[service].auth` secret in `uwah.toml` (supports both plaintext secret string and SHA-256 hex digest).
- **Error Behavior**: Missing header, malformed prefix (not starting with `Bearer `), empty token, or invalid hash immediately returns `Status::unauthenticated("unauthenticated")`.

### 2.2 TLS & Production Deployment
The gateway binds gRPC on standard TCP (`0.0.0.0:6768` or `127.0.0.1:6768`) without terminating TLS inside the Rust process. In production environments:
1. Terminate TLS externally via an API Gateway / Ingress Controller / Reverse Proxy (e.g., Traefik, NGINX, Envoy, Caddy) configured for HTTP/2 (`h2` / `h2c`).
2. Alternatively, communicate over an isolated VPC / internal service mesh (e.g., Kubernetes cluster network, WireGuard overlay).

---

## 3. Error Codes & Status Mappings

All RPC failures map deterministically to standard gRPC canonical status codes:

| gRPC Status Code | Numeric | Trigger Condition / Typical Cause |
| :--- | :--- | :--- |
| `UNAUTHENTICATED` | 16 | Missing `authorization` metadata, absent `Bearer ` prefix, empty token, or invalid authentication key. |
| `NOT_FOUND` | 5 | Specified `session_id`, `jid`, `message_id`, `call_id`, `dlq_id`, or media hash (`sha256`) does not exist. |
| `INVALID_ARGUMENT` | 3 | Malformed SHA-256 hash (not 64 hex characters), empty payload, invalid JID format, file size exceeding `max_file_size`. |
| `PERMISSION_DENIED` | 7 | Path traversal attack detected attempting to escape the configured media cache storage directory. |
| `OUT_OF_RANGE` | 11 | Requested `offset` parameter in media download exceeds total file size on disk. |
| `INTERNAL` | 13 | Unhandled database query error, filesystem IO failure, or internal runtime error. |

---

## 4. HTTP ↔ gRPC Equivalence Matrix

| REST / Real-Time Route | gRPC Service & RPC Method | Stream Type | Description |
| :--- | :--- | :--- | :--- |
| `GET /health` | `uwah.v1.HealthService/Check` | Unary | Point-in-time health & DB connectivity check |
| `GET /health/live` | `uwah.v1.HealthService/CheckLiveness` | Unary | Kubernetes liveness probe (event-loop check) |
| `GET /health/ready` | `uwah.v1.HealthService/CheckReadiness` | Unary | Deep readiness probe (DB pool & active session count) |
| `GET /health/startup` | `uwah.v1.HealthService/CheckStartup` | Unary | Database migration readiness probe |
| `GET /metrics` | `uwah.v1.HealthService/ExportMetrics` | Unary | Prometheus plain-text metrics export |
| `GET /events` / `GET /sessions/:id/events` | `uwah.v1.EventStreamService/StreamEvents` | Server Stream | Real-time gateway event streaming with optional filtering |
| `GET /sessions` | `uwah.v1.SessionService/ListSessions` | Unary | List all registered sessions |
| `POST /sessions` | `uwah.v1.SessionService/CreateSession` | Unary | Create a new session instance |
| `GET /sessions/:id` | `uwah.v1.SessionService/GetSession` | Unary | Fetch session details |
| `DELETE /sessions/:id` | `uwah.v1.SessionService/DeleteSession` | Unary | Purge and unregister session |
| `POST /sessions/:id/qr` | `uwah.v1.SessionService/RefreshQr` | Unary | Refresh active QR code payload |
| `POST /sessions/:id/restart` | `uwah.v1.SessionService/RestartSession` | Unary | Reconnect bot worker |
| `POST /sessions/:id/logout` | `uwah.v1.SessionService/LogoutSession` | Unary | Unlink WhatsApp account |
| `POST /sessions/:id/pairing-code` | `uwah.v1.SessionService/RequestPairingCode` | Unary | Request 8-character phone pairing code |
| `POST /sessions/:id/presence` | `uwah.v1.ChatService/SetGlobalPresence` | Unary | Set session global online/offline status |
| `POST /sessions/:id/chats/:jid/presence` | `uwah.v1.ChatService/SetChatPresence` | Unary | Send typing / recording indicators |
| `GET /sessions/:id/chats` | `uwah.v1.ChatService/ListChats` | Unary | Paginated list of conversations |
| `GET /sessions/:id/chats/:jid` | `uwah.v1.ChatService/GetChat` | Unary | Get single chat conversation record |
| `DELETE /sessions/:id/chats/:jid` | `uwah.v1.ChatService/DeleteChat` | Unary | Delete chat conversation record |
| `POST /sessions/:id/chats/:jid/read` | `uwah.v1.ChatService/MarkAsRead` | Unary | Send read receipts and mark read |
| `POST /sessions/:id/chats/:jid/mute` | `uwah.v1.ChatService/MuteChat` | Unary | Mute/unmute chat notifications |
| `POST /sessions/:id/chats/:jid/pin` | `uwah.v1.ChatService/PinChat` | Unary | Pin/unpin chat thread |
| `POST /sessions/:id/chats/:jid/archive` | `uwah.v1.ChatService/ArchiveChat` | Unary | Archive/unarchive chat |
| `PATCH /sessions/:id/chats/:jid/subject` | `uwah.v1.ChatService/SetSubject` | Unary | Set chat / group title subject |
| `PATCH /sessions/:id/chats/:jid/description`| `uwah.v1.ChatService/SetDescription` | Unary | Set group description |
| `PUT /sessions/:id/chats/:jid/settings/ephemeral`| `uwah.v1.ChatService/SetEphemeralTimer`| Unary | Configure disappearing messages timer |
| `PUT /sessions/:id/chats/:jid/settings/announcement`| `uwah.v1.ChatService/SetGroupAnnouncement`| Unary | Restrict messages to admins only |
| `PUT /sessions/:id/chats/:jid/settings/lock`| `uwah.v1.ChatService/SetGroupLock` | Unary | Restrict group info edits to admins |
| `GET /sessions/:id/chats/:jid/invite-code` | `uwah.v1.ChatService/GetInviteLink` | Unary | Fetch or revoke group invite code |
| `POST /sessions/:id/chats/:jid/leave` | `uwah.v1.ChatService/LeaveGroup` | Unary | Bot leaves group |
| `POST /sessions/:id/groups` | `uwah.v1.GroupService/CreateGroup` | Unary | Create new WhatsApp group |
| `POST /sessions/:id/groups/join` | `uwah.v1.GroupService/JoinGroup` | Unary | Join group via invite code |
| `GET /sessions/:id/chats/:jid/participants`| `uwah.v1.GroupParticipantService/ListParticipants`| Unary | List group participant members |
| `POST /sessions/:id/chats/:jid/participants`| `uwah.v1.GroupParticipantService/AddParticipants`| Unary | Add participants to group |
| `DELETE /sessions/:id/chats/:jid/participants/:ujid`| `uwah.v1.GroupParticipantService/RemoveParticipant`| Unary | Remove participant from group |
| `POST /sessions/:id/chats/:jid/participants/promote`| `uwah.v1.GroupParticipantService/PromoteParticipants`| Unary | Promote members to admin |
| `POST /sessions/:id/chats/:jid/participants/demote`| `uwah.v1.GroupParticipantService/DemoteParticipants`| Unary | Demote admins to member |
| `GET /sessions/:id/chats/:jid/messages` | `uwah.v1.MessageService/ListMessages` | Unary | Keyset-paginated message history |
| `POST /sessions/:id/chats/:jid/messages` | `uwah.v1.MessageService/SendMessage` | Unary | Send text message / quoted reply |
| `POST /sessions/:id/chats/:jid/messages/location`| `uwah.v1.MessageService/SendLocation` | Unary | Send location pin message |
| `POST /sessions/:id/chats/:jid/messages/contact` | `uwah.v1.MessageService/SendContact` | Unary | Send vCard contact card |
| `POST /sessions/:id/chats/:jid/messages/media` | `uwah.v1.MessageService/SendMedia` | Unary | Transmit media message |
| `PATCH /sessions/:id/chats/:jid/messages/:mid` | `uwah.v1.MessageService/EditMessage` | Unary | Edit message within 15min window |
| `DELETE /sessions/:id/chats/:jid/messages/:mid`| `uwah.v1.MessageService/DeleteMessage` | Unary | Delete message record from database |
| `POST /sessions/:id/chats/:jid/messages/:mid/reaction`| `uwah.v1.MessageService/ReactMessage`| Unary | React with emoji or remove reaction |
| `POST /sessions/:id/chats/:jid/messages/:mid/revoke`| `uwah.v1.MessageService/RevokeMessage`| Unary | Revoke / delete message for everyone |
| `GET /sessions/:id/contacts` | `uwah.v1.ContactService/ListContacts` | Unary | List synced contacts |
| `POST /sessions/:id/contacts` | `uwah.v1.ContactService/UpsertContact` | Unary | Insert or update contact |
| `POST /sessions/:id/contacts/sync` | `uwah.v1.ContactService/SyncContacts` | Unary | Batch sync address book |
| `POST /sessions/:id/contacts/check` | `uwah.v1.ContactService/CheckContacts` | Unary | Verify WhatsApp registration |
| `GET /sessions/:id/contacts/:jid` | `uwah.v1.ContactService/GetContact` | Unary | Get single contact profile |
| `DELETE /sessions/:id/contacts/:jid` | `uwah.v1.ContactService/DeleteContact` | Unary | Delete contact record |
| `GET /sessions/:id/contacts/:jid/profile-picture`| `uwah.v1.ContactService/GetProfilePicture`| Unary | Fetch profile picture URL |
| `POST /sessions/:id/contacts/:jid/block` | `uwah.v1.ContactService/BlockContact` | Unary | Block contact |
| `POST /sessions/:id/contacts/:jid/unblock`| `uwah.v1.ContactService/UnblockContact` | Unary | Unblock contact |
| `GET /sessions/:id/calls` | `uwah.v1.CallService/ListCalls` | Unary | List call logs |
| `GET /sessions/:id/calls/:cid` | `uwah.v1.CallService/GetCall` | Unary | Get call details |
| `DELETE /sessions/:id/calls/:cid` | `uwah.v1.CallService/DeleteCall` | Unary | Delete call record |
| `POST /sessions/:id/calls/:cid/reject` | `uwah.v1.CallService/RejectCall` | Unary | Reject incoming call |
| `GET /sessions/:id/media/:sha256` | `uwah.v1.MediaService/DownloadMedia` | Server Stream | Stream binary media chunks (64KB chunks) |
| `POST /sessions/:id/media/upload` (multipart) | `uwah.v1.MediaService/UploadMedia` | Client Stream | Stream upload media chunks |
| `POST /sessions/:id/media/upload` (raw) | `uwah.v1.MediaService/UploadMediaUnary`| Unary | Direct single-message binary upload |
| `GET /webhooks/dlq` | `uwah.v1.WebhookDlqService/ListDlq` | Unary | List dead-lettered webhooks |
| `GET /webhooks/dlq/:id` | `uwah.v1.WebhookDlqService/GetDlq` | Unary | Get specific DLQ entry |
| `POST /webhooks/dlq/:id/replay` | `uwah.v1.WebhookDlqService/ReplayDlq` | Unary | Replay single quarantined webhook |
| `POST /webhooks/dlq/replay-all` | `uwah.v1.WebhookDlqService/ReplayAllDlq`| Unary | Replay all dead-lettered events |
| `DELETE /webhooks/dlq/:id` | `uwah.v1.WebhookDlqService/DeleteDlq` | Unary | Purge dead-letter entry |

---

## 5. Streaming Guides

### 5.1 Real-Time Event Streaming (`StreamEvents`)
The `EventStreamService.StreamEvents` RPC delivers an asynchronous stream of `GatewayEventEnvelope` messages generated across the gateway.

- **Filtering**: Set `session_id` to receive events for a specific session, or leave unset to receive global events across all active sessions.
- **Heartbeat & Lag Handling**: The stream automatically handles client lag by skipping dropped frames and logging warnings if a client slows down, preserving gateway memory.

```protobuf
message GatewayEventEnvelope {
  string event_id = 1;
  string session_id = 2;
  string event_type = 3;
  int64 timestamp = 4;
  string payload_json = 5;
}
```

### 5.2 Chunked Media Streaming (`DownloadMedia` & `UploadMedia`)
For high efficiency and low memory consumption when transferring large binary payloads (audio, video, documents, images):

1. **`DownloadMedia` (Server Streaming)**:
   - Streams the requested file in consecutive `64KB` chunks (`MediaChunk`).
   - Supports range requests via `offset` and `length`.
   - Validates paths against traversal attacks and updates `last_accessed_at` in the database.

2. **`UploadMedia` (Client Streaming)**:
   - Client sends an initial `UploadMediaMetadata` envelope followed by sequential byte chunks.
   - Computes SHA-256 continuously on the fly.
   - Enforces `max_file_size` limits during upload and stores deduplicated files by hash.

3. **`UploadMediaUnary` (Unary Direct Upload)**:
   - For small files or simplified clients, uploads binary data in a single request.

---

## 6. Practical Examples with `grpcurl`

Below are executable `grpcurl` commands. Replace `SECRET_TOKEN` with your configured auth token.

### 6.1 Server Reflection & Discovery
```bash
# List all exposed gRPC services
grpcurl -plaintext 127.0.0.1:6768 list

# Describe a specific service contract
grpcurl -plaintext 127.0.0.1:6768 describe uwah.v1.SessionService

# Describe a message schema
grpcurl -plaintext 127.0.0.1:6768 describe uwah.v1.MessageItem
```

### 6.2 Health Probes & Metrics
```bash
# Check point-in-time health
grpcurl -plaintext -H "authorization: Bearer SECRET_TOKEN" \
  -d '{"service": ""}' \
  127.0.0.1:6768 uwah.v1.HealthService/Check

# Kubernetes liveness probe
grpcurl -plaintext -H "authorization: Bearer SECRET_TOKEN" \
  -d '{}' \
  127.0.0.1:6768 uwah.v1.HealthService/CheckLiveness

# Prometheus metrics export
grpcurl -plaintext -H "authorization: Bearer SECRET_TOKEN" \
  -d '{}' \
  127.0.0.1:6768 uwah.v1.HealthService/ExportMetrics
```

### 6.3 Multi-Session Management
```bash
# List all sessions
grpcurl -plaintext -H "authorization: Bearer SECRET_TOKEN" \
  -d '{}' \
  127.0.0.1:6768 uwah.v1.SessionService/ListSessions

# Create a new session
grpcurl -plaintext -H "authorization: Bearer SECRET_TOKEN" \
  -d '{"id": "bot-marketing"}' \
  127.0.0.1:6768 uwah.v1.SessionService/CreateSession

# Request phone pairing code
grpcurl -plaintext -H "authorization: Bearer SECRET_TOKEN" \
  -d '{"id": "bot-marketing", "phone_number": "5511999998888"}' \
  127.0.0.1:6768 uwah.v1.SessionService/RequestPairingCode

# Restart session worker
grpcurl -plaintext -H "authorization: Bearer SECRET_TOKEN" \
  -d '{"id": "bot-marketing"}' \
  127.0.0.1:6768 uwah.v1.SessionService/RestartSession
```

### 6.4 Real-Time Event Subscription (Streaming)
```bash
# Stream events for all sessions
grpcurl -plaintext -H "authorization: Bearer SECRET_TOKEN" \
  -d '{}' \
  127.0.0.1:6768 uwah.v1.EventStreamService/StreamEvents

# Stream events filtered by session ID
grpcurl -plaintext -H "authorization: Bearer SECRET_TOKEN" \
  -d '{"session_id": "bot-marketing"}' \
  127.0.0.1:6768 uwah.v1.EventStreamService/StreamEvents
```

### 6.5 Messaging & Chat Operations
```bash
# Send a text message
grpcurl -plaintext -H "authorization: Bearer SECRET_TOKEN" \
  -d '{
    "session_id": "bot-marketing",
    "chat_jid": "5511988887777@s.whatsapp.net",
    "body": "Hello from gRPC!"
  }' \
  127.0.0.1:6768 uwah.v1.MessageService/SendMessage

# Send an emoji reaction
grpcurl -plaintext -H "authorization: Bearer SECRET_TOKEN" \
  -d '{
    "session_id": "bot-marketing",
    "chat_jid": "5511988887777@s.whatsapp.net",
    "message_id": "3EB0123456789ABCDEF",
    "emoji": "🚀"
  }' \
  127.0.0.1:6768 uwah.v1.MessageService/ReactMessage

# Send a location pin
grpcurl -plaintext -H "authorization: Bearer SECRET_TOKEN" \
  -d '{
    "session_id": "bot-marketing",
    "chat_jid": "5511988887777@s.whatsapp.net",
    "latitude": -23.55052,
    "longitude": -46.633308,
  "name": "Central Park",
  "address": "New York, NY"
  }' \
  127.0.0.1:6768 uwah.v1.MessageService/SendLocation

# Set chat composing presence (typing indicator)
grpcurl -plaintext -H "authorization: Bearer SECRET_TOKEN" \
  -d '{
    "session_id": "bot-marketing",
    "jid": "5511988887777@s.whatsapp.net",
    "state": "composing"
  }' \
  127.0.0.1:6768 uwah.v1.ChatService/SetChatPresence
```

### 6.6 Group Management
```bash
# Create a new group
grpcurl -plaintext -H "authorization: Bearer SECRET_TOKEN" \
  -d '{
    "session_id": "bot-marketing",
    "subject": "Developers Group",
    "participants": ["5511988887777@s.whatsapp.net", "5511977776666@s.whatsapp.net"]
  }' \
  127.0.0.1:6768 uwah.v1.GroupService/CreateGroup

# List group participants
grpcurl -plaintext -H "authorization: Bearer SECRET_TOKEN" \
  -d '{
    "session_id": "bot-marketing",
    "jid": "120363023456789012@g.us"
  }' \
  127.0.0.1:6768 uwah.v1.GroupParticipantService/ListParticipants
```

### 6.7 Contacts & Address Book
```bash
# Verify if phone numbers exist on WhatsApp
grpcurl -plaintext -H "authorization: Bearer SECRET_TOKEN" \
  -d '{
    "session_id": "bot-marketing",
    "contacts": ["+5511988887777", "+5511900000000"]
  }' \
  127.0.0.1:6768 uwah.v1.ContactService/CheckContacts
```

### 6.8 Media Streaming
```bash
# Download media chunks via server streaming
grpcurl -plaintext -H "authorization: Bearer SECRET_TOKEN" \
  -d '{
    "session_id": "bot-marketing",
    "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
  }' \
  127.0.0.1:6768 uwah.v1.MediaService/DownloadMedia
```

### 6.9 Webhook DLQ Management
```bash
# List quarantined dead-letter webhooks
grpcurl -plaintext -H "authorization: Bearer SECRET_TOKEN" \
  -d '{}' \
  127.0.0.1:6768 uwah.v1.WebhookDlqService/ListDlq

# Replay all failed webhooks
grpcurl -plaintext -H "authorization: Bearer SECRET_TOKEN" \
  -d '{}' \
  127.0.0.1:6768 uwah.v1.WebhookDlqService/ReplayAllDlq
```
