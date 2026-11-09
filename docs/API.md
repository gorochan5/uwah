# Uwah WhatsApp Gateway — Complete REST & Real-Time API Reference

## 1. Authentication & Security Model

All API endpoints (except public health probes `/health*`, `/metrics`, and the visual scan portal) require authentication via `Authorization: Bearer <TOKEN>`.

### Authentication Rules
1. **Bearer Header (Recommended)**:
   ```http
   Authorization: Bearer <AUTH_TOKEN>
   ```
2. **Query Parameter (Restricted to Web UI / Scan Portal)**:
   ```http
   GET /swagger?key=<AUTH_TOKEN>
   GET /sessions/{id}/scan?token=<AUTH_TOKEN>
   ```

---

## 2. Interactive Swagger UI & OpenAPI 3.1 Spec

- **Swagger UI**: `http://127.0.0.1:6767/swagger`
- **OpenAPI 3.1 JSON Schema**: `http://127.0.0.1:6767/api-docs/openapi.json`

---

## 3. Real-Time Streaming & Transports

### Server-Sent Events (SSE)
- `GET /events` — Global multi-session real-time event stream (`text/event-stream`).
- `GET /sessions/{id}/events` — Session-scoped real-time event stream.
- Includes automatic 15-second keep-alive heartbeats (`: ping\n\n`).

### Full-Duplex WebSockets (`uwah.v1+json`)
- `GET /ws` — Global bidirectional WebSocket.
- `GET /sessions/{id}/ws` — Session-scoped bidirectional WebSocket.

---

## 4. Endpoints Index & Category Matrix

### 4.1 Observability, SRE & Health Probes
| Method | Route | Description | Status Code |
| :--- | :--- | :--- | :--- |
| `GET` | `/health` | General database and gateway connectivity check | `200 OK` / `503 Service Unavailable` |
| `GET` | `/health/live` | Zero-overhead event-loop liveness probe (K8s) | `200 OK` |
| `GET` | `/health/ready` | Deep readiness check (DB pool, storage volume, active workers) | `200 OK` / `503 Service Unavailable` |
| `GET` | `/health/startup` | Startup probe validating database migration readiness | `200 OK` / `503 Service Unavailable` |
| `GET` | `/metrics` | Prometheus SRE metrics exporter (`text/plain; version=0.0.4`) | `200 OK` |

---

### 4.2 Multi-Session Lifecycle Management
| Method | Route | Description | Status Code |
| :--- | :--- | :--- | :--- |
| `GET` | `/sessions` | List all configured sessions and their status | `200 OK` |
| `POST` | `/sessions` | Create and initialize a new WhatsApp bot session | `201 Created` |
| `GET` | `/sessions/{id}` | Get details of a specific session | `200 OK` / `404 Not Found` |
| `DELETE` | `/sessions/{id}` | Unregister and cleanly purge session and `.db` files | `204 No Content` |
| `POST` | `/sessions/{id}/qr` | Request a fresh QR code payload | `200 OK` |
| `POST` | `/sessions/{id}/restart` | Restart bot worker and reconnect socket | `200 OK` |
| `POST` | `/sessions/{id}/logout` | Unlink WhatsApp account from session | `200 OK` |
| `POST` | `/sessions/{id}/pairing-code` | Request an 8-character phone pairing code | `200 OK` |
| `GET` | `/sessions/{id}/scan` | Standalone Maud + HTMX vector QR pairing web page | `200 OK` |
| `GET` | `/sessions/{id}/qr-frame` | HTMX polling fragment for pairing screen | `200 OK` / `286 Stopped` |

---

### 4.3 Presence & Global Availability
| Method | Route | Description |
| :--- | :--- | :--- |
| `POST` | `/sessions/{id}/presence` | Set session online (`available: true`) or offline (`available: false`) |
| `POST` | `/sessions/{id}/chats/{jid}/presence` | Send typing indicator (`composing`, `recording`, `paused`) |

---

### 4.4 Chats & Conversation Threads
| Method | Route | Description |
| :--- | :--- | :--- |
| `GET` | `/sessions/{id}/chats` | List chats with limit/offset pagination |
| `GET` | `/sessions/{id}/chats/{jid}` | Get details for a specific chat thread |
| `DELETE` | `/sessions/{id}/chats/{jid}` | Delete chat thread from local database |
| `POST` | `/sessions/{id}/chats/{jid}/read` | Mark conversation as read / unread and send read receipt |
| `POST` | `/sessions/{id}/chats/{jid}/mute` | Mute or unmute chat with optional duration |
| `POST` | `/sessions/{id}/chats/{jid}/pin` | Pin or unpin chat in conversation list |
| `POST` | `/sessions/{id}/chats/{jid}/archive` | Archive or unarchive chat |
| `PATCH` | `/sessions/{id}/chats/{jid}/subject` | Set group/chat title subject |
| `PATCH` | `/sessions/{id}/chats/{jid}/description` | Set group/chat description |
| `PUT` | `/sessions/{id}/chats/{jid}/settings/ephemeral` | Configure disappearing messages timer (0, 86400, 604800, 7776000s) |
| `PUT` | `/sessions/{id}/chats/{jid}/settings/announcement` | Restrict sending messages to admins only |
| `PUT` | `/sessions/{id}/chats/{jid}/settings/lock` | Restrict modifying group info to admins only |
| `GET` | `/sessions/{id}/chats/{jid}/invite-code` | Retrieve or reset group invite link |
| `POST` | `/sessions/{id}/chats/{jid}/leave` | Cause the bot to leave the group |

---

### 4.5 Group Administration Plane
| Method | Route | Description |
| :--- | :--- | :--- |
| `POST` | `/sessions/{id}/groups` | Create a new WhatsApp group |
| `POST` | `/sessions/{id}/groups/join` | Join group via invite link or code |
| `GET` | `/sessions/{id}/chats/{jid}/participants` | List participants of a group |
| `POST` | `/sessions/{id}/chats/{jid}/participants` | Add participants to a group |
| `DELETE` | `/sessions/{id}/chats/{jid}/participants/{user_jid}` | Remove a participant from a group |
| `POST` | `/sessions/{id}/chats/{jid}/participants/promote` | Promote participants to group administrator |
| `POST` | `/sessions/{id}/chats/{jid}/participants/demote` | Demote administrators to regular members |

---

### 4.6 Messages Transmission & History
| Method | Route | Description |
| :--- | :--- | :--- |
| `GET` | `/sessions/{id}/chats/{jid}/messages` | Keyset-paginated message history |
| `POST` | `/sessions/{id}/chats/{jid}/messages` | Send text or reply quote message |
| `POST` | `/sessions/{id}/chats/{jid}/messages/location` | Send geographic coordinates location card |
| `POST` | `/sessions/{id}/chats/{jid}/messages/contact` | Send vCard contact business card |
| `POST` | `/sessions/{id}/chats/{jid}/messages/media` | Upload and transmit media (image, audio, video, document, sticker) |
| `PATCH` | `/sessions/{id}/chats/{jid}/messages/{msg_id}` | Edit text message within WhatsApp 15-minute window |
| `POST` | `/sessions/{id}/chats/{jid}/messages/{msg_id}/reaction` | React to message with emoji (or remove reaction) |
| `POST` | `/sessions/{id}/chats/{jid}/messages/{msg_id}/revoke` | Revoke / delete message for everyone |
| `DELETE` | `/sessions/{id}/chats/{jid}/messages/{msg_id}` | Delete message record from local storage |

---

### 4.7 Contacts & Address Book
| Method | Route | Description |
| :--- | :--- | :--- |
| `GET` | `/sessions/{id}/contacts` | List synchronized contacts |
| `POST` | `/sessions/{id}/contacts` | Upsert contact information |
| `POST` | `/sessions/{id}/contacts/sync` | Trigger on-demand contact synchronization |
| `POST` | `/sessions/{id}/contacts/check` | Check if phone numbers exist on WhatsApp |
| `GET` | `/sessions/{id}/contacts/{jid}` | Get contact profile by JID |
| `DELETE` | `/sessions/{id}/contacts/{jid}` | Delete contact record |
| `GET` | `/sessions/{id}/contacts/{jid}/profile-picture` | Fetch high-resolution avatar URL |
| `POST` | `/sessions/{id}/contacts/{jid}/block` | Block contact |
| `POST` | `/sessions/{id}/contacts/{jid}/unblock` | Unblock contact |

---

### 4.8 Media Cache & Downloads
| Method | Route | Description |
| :--- | :--- | :--- |
| `GET` | `/sessions/{id}/media/{sha256}` | Download decrypted binary media file with HTTP caching (`ETag`, `304`) |
| `POST` | `/sessions/{id}/media/upload` | Pre-upload media binary to server cache |

---

### 4.9 Voice & Video Call Signaling
| Method | Route | Description |
| :--- | :--- | :--- |
| `GET` | `/sessions/{id}/calls` | List call history records |
| `GET` | `/sessions/{id}/calls/{call_id}` | Get call details |
| `DELETE` | `/sessions/{id}/calls/{call_id}` | Delete call record |
| `POST` | `/sessions/{id}/calls/{call_id}/reject` | Reject inbound call offer on protocol level and dispatch webhook |

---

### 4.10 Webhook Dead-Letter Queue (DLQ)
| Method | Route | Description |
| :--- | :--- | :--- |
| `GET` | `/webhooks/dlq` | List all dead-lettered webhook events |
| `GET` | `/webhooks/dlq/{id}` | Inspect quarantined dead-letter entry |
| `POST` | `/webhooks/dlq/{id}/replay` | Requeue single quarantined webhook for retry |
| `POST` | `/webhooks/dlq/replay-all` | Requeue all dead-lettered events |
| `DELETE` | `/webhooks/dlq/{id}` | Purge quarantined event |

---

## 5. gRPC Microservices Interconnect API

The gateway also provides a gRPC API on `service.port + 1` (default: `6768`). Its surface overlaps with REST, but feature support is not guaranteed to match one-to-one. See [the gRPC reference](GRPC.md) for the available services and methods.

For the comprehensive gRPC reference guide, error mappings, proto schemas, and `grpcurl` examples, see **[`docs/GRPC.md`](GRPC.md)**.

### 5.1 gRPC Server Reflection
The server enables **gRPC Server Reflection Protocol v1** (`tonic-reflection`), allowing dynamic service discovery and inspection without requiring local `.proto` files:
```bash
grpcurl -plaintext 127.0.0.1:6768 list
grpcurl -plaintext 127.0.0.1:6768 describe uwah.v1.SessionService
```

### 5.2 gRPC Authentication
All gRPC RPC methods require metadata authentication using the same secret configured in `uwah.toml` under `[service].auth` (supporting plaintext or SHA-256 hex digest):

- **Metadata Header**: `authorization: Bearer <TOKEN>`
- **Error Behavior**: Missing, invalid, empty, or non-`Bearer ` metadata immediately returns gRPC status `Status::unauthenticated("unauthenticated")`.
- **Transport Security**: gRPC does not terminate TLS internally; tokens should be transported over an encrypted channel (TLS reverse proxy / ingress) or a secure internal private network.

### 5.3 Services Index (11 Tonic Services)
1. **`uwah.v1.HealthService`**: Health probes (`Check`, `Watch`, `CheckLiveness`, `CheckReadiness`, `CheckStartup`) and `ExportMetrics`.
2. **`uwah.v1.EventStreamService`**: Real-time event streaming (`StreamEvents`) with optional session filtering.
3. **`uwah.v1.SessionService`**: Session lifecycle (`ListSessions`, `CreateSession`, `GetSession`, `DeleteSession`, `RefreshQr`, `RestartSession`, `LogoutSession`, `RequestPairingCode`).
4. **`uwah.v1.ChatService`**: Chat presence, conversations, pins, mutes, archives, ephemeral settings, group metadata, and invite links.
5. **`uwah.v1.GroupService`**: Group creation (`CreateGroup`) and joining (`JoinGroup`).
6. **`uwah.v1.GroupParticipantService`**: Participant operations (`ListParticipants`, `AddParticipants`, `RemoveParticipant`, `PromoteParticipants`, `DemoteParticipants`).
7. **`uwah.v1.MessageService`**: Messages transmission (`SendMessage`, `SendLocation`, `SendContact`, `SendMedia`, `EditMessage`, `DeleteMessage`, `ReactMessage`, `RevokeMessage`, `ListMessages`).
8. **`uwah.v1.ContactService`**: Address book (`ListContacts`, `UpsertContact`, `SyncContacts`, `CheckContacts`, `GetContact`, `DeleteContact`, `GetProfilePicture`, `BlockContact`, `UnblockContact`).
9. **`uwah.v1.CallService`**: Call logs and signaling (`ListCalls`, `GetCall`, `DeleteCall`, `RejectCall`).
10. **`uwah.v1.MediaService`**: Chunked media downloads (`DownloadMedia`), chunked streaming uploads (`UploadMedia`), and unary uploads (`UploadMediaUnary`).
11. **`uwah.v1.WebhookDlqService`**: Dead-letter queue inspection and replays (`ListDlq`, `GetDlq`, `ReplayDlq`, `ReplayAllDlq`, `DeleteDlq`).


