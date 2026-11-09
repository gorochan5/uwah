# REST API Reference

The HTTP API is served on `[service].host:[service].port` (default port `6767`). The live OpenAPI document and Swagger UI are generated from the API implementation.

- Swagger UI: `/swagger`
- OpenAPI JSON: `/api-docs/openapi.json`

Use the generated OpenAPI document as the source of truth for request schemas and response models. This page summarizes the routes and operational behavior.

## Security and authentication

Protected routes require:

```http
Authorization: Bearer <service-token>
```

The service token is configured as `[service].auth`. It may contain the token directly or its SHA-256 hexadecimal digest. Health routes and `/metrics` do not require authentication. The scan portal and Swagger UI support browser-oriented query credentials; the scan page accepts `?token=`, and Swagger accepts `?key=` or `?token=`. Query tokens may be recorded in browser history, access logs, and telemetry. Do not expose them in shared links.

The HTTP server does not terminate TLS. Use a trusted reverse proxy or secure tunnel for traffic over untrusted networks.

## Health and metrics

| Method | Path | Authentication | Behavior |
| --- | --- | --- | --- |
| `GET` | `/health` | Public | Checks database connectivity. |
| `GET` | `/health/live` | Public | Confirms that the HTTP process is responsive. |
| `GET` | `/health/ready` | Public | Checks database connectivity and media-directory accessibility; reports active sessions. |
| `GET` | `/health/startup` | Public | Checks that the application database schema is initialized. |
| `GET` | `/metrics` | Public | Exports Prometheus text metrics. |

The readiness response is successful when database and storage checks pass. The active-session count is informational and does not determine readiness.

## Session lifecycle and pairing

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/sessions` | List stored sessions. |
| `POST` | `/sessions` | Create a session and attempt to start its WhatsApp worker. |
| `GET` | `/sessions/{id}` | Retrieve a session. |
| `DELETE` | `/sessions/{id}` | Stop and remove a session and its protocol-store files. |
| `POST` | `/sessions/{id}/qr` | Update the session QR payload. |
| `POST` | `/sessions/{id}/restart` | Restart the session worker. |
| `POST` | `/sessions/{id}/logout` | Log the session out. |
| `POST` | `/sessions/{id}/pairing-code` | Request a phone-number pairing code. |
| `GET` | `/sessions/{id}/scan` | Render the browser pairing page. |
| `GET` | `/sessions/{id}/qr-frame` | Render the HTMX polling fragment; returns status `286` when connected. |

Session identifiers must contain 1–64 ASCII letters, digits, underscores, or hyphens and cannot be Windows reserved device names. Creating a session can contact WhatsApp and create persistent protocol state.

## Chats, presence, and groups

| Method | Path | Description |
| --- | --- | --- |
| `POST` | `/sessions/{id}/presence` | Set global session availability. |
| `GET` | `/sessions/{id}/chats` | List chats. |
| `GET`, `DELETE` | `/sessions/{id}/chats/{jid}` | Retrieve or delete a chat record. |
| `POST` | `/sessions/{id}/chats/{jid}/read` | Mark a chat as read. |
| `POST` | `/sessions/{id}/chats/{jid}/mute` | Mute or unmute a chat. |
| `POST` | `/sessions/{id}/chats/{jid}/pin` | Pin or unpin a chat. |
| `POST` | `/sessions/{id}/chats/{jid}/archive` | Archive or unarchive a chat. |
| `POST` | `/sessions/{id}/chats/{jid}/presence` | Set chat state such as composing or recording. |
| `PATCH` | `/sessions/{id}/chats/{jid}/subject` | Update a group subject. |
| `PATCH` | `/sessions/{id}/chats/{jid}/description` | Update a group description. |
| `PUT` | `/sessions/{id}/chats/{jid}/settings/ephemeral` | Set the disappearing-message timer. |
| `PUT` | `/sessions/{id}/chats/{jid}/settings/announcement` | Set group announcement mode. |
| `PUT` | `/sessions/{id}/chats/{jid}/settings/lock` | Set group edit restrictions. |
| `GET` | `/sessions/{id}/chats/{jid}/invite-code` | Retrieve or reset a group invite link. |
| `POST` | `/sessions/{id}/chats/{jid}/leave` | Leave a group. |
| `POST` | `/sessions/{id}/groups` | Create a group. |
| `POST` | `/sessions/{id}/groups/join` | Join a group using an invite code or link. |
| `GET`, `POST` | `/sessions/{id}/chats/{jid}/participants` | List or add group participants. |
| `DELETE` | `/sessions/{id}/chats/{jid}/participants/{user_jid}` | Remove a participant. |
| `POST` | `/sessions/{id}/chats/{jid}/participants/promote` | Promote participants. |
| `POST` | `/sessions/{id}/chats/{jid}/participants/demote` | Demote participants. |

## Messages

| Method | Path | Description |
| --- | --- | --- |
| `GET`, `POST` | `/sessions/{id}/chats/{jid}/messages` | List message history or send a text message. |
| `POST` | `/sessions/{id}/chats/{jid}/messages/location` | Send a location. |
| `POST` | `/sessions/{id}/chats/{jid}/messages/contact` | Send a contact card. |
| `POST` | `/sessions/{id}/chats/{jid}/messages/media` | Send media. |
| `POST` | `/sessions/{id}/chats/{jid}/messages/sticker` | Send a sticker. |
| `POST` | `/sessions/{id}/chats/{jid}/messages/sticker-pack` | Send a sticker pack. |
| `PATCH` | `/sessions/{id}/chats/{jid}/messages/{msg_id}` | Edit a message. |
| `DELETE` | `/sessions/{id}/chats/{jid}/messages/{msg_id}` | Delete the local message record. |
| `POST` | `/sessions/{id}/chats/{jid}/messages/{msg_id}/reaction` | Add or remove a reaction. |
| `POST` | `/sessions/{id}/chats/{jid}/messages/{msg_id}/revoke` | Revoke a message for participants, subject to WhatsApp restrictions. |

Text-message requests accept a `body`; quoted replies can include `quoted_message_id`. Message sending requires an active session and may be subject to WhatsApp-side restrictions. A successful API response cannot guarantee delivery to the recipient.

## Contacts and calls

| Method | Path | Description |
| --- | --- | --- |
| `GET`, `POST` | `/sessions/{id}/contacts` | List or upsert contacts. |
| `POST` | `/sessions/{id}/contacts/sync` | Synchronize contacts from the session. |
| `POST` | `/sessions/{id}/contacts/check` | Check phone numbers through WhatsApp. |
| `GET`, `DELETE` | `/sessions/{id}/contacts/{jid}` | Retrieve or delete a contact. |
| `GET` | `/sessions/{id}/contacts/{jid}/profile-picture` | Retrieve a cached or remote profile picture. |
| `POST` | `/sessions/{id}/contacts/{jid}/block` | Block a contact. |
| `POST` | `/sessions/{id}/contacts/{jid}/unblock` | Unblock a contact. |
| `GET` | `/sessions/{id}/calls` | List call records. |
| `GET`, `DELETE` | `/sessions/{id}/calls/{call_id}` | Retrieve or delete a call record. |
| `POST` | `/sessions/{id}/calls/{call_id}/reject` | Reject an incoming call. |

## Media

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/sessions/{id}/media/{sha256}` | Retrieve cached media. Supports byte ranges. |
| `POST` | `/sessions/{id}/media/upload` | Upload media into the local cache. |

Media is subject to `[media].max_file_size`. Cache retention and concurrency settings are configurable in `uwah.toml`. The server validates media paths against the configured cache root.

## Events and streaming

| Method | Path | Transport |
| --- | --- | --- |
| `GET` | `/events` | Server-Sent Events (global stream) |
| `GET` | `/sessions/{id}/events` | Server-Sent Events (session-filtered stream) |
| `GET` | `/ws` | WebSocket (global stream) |
| `GET` | `/sessions/{id}/ws` | WebSocket (session-filtered stream) |

These routes are protected by the shared Bearer token. Events use an in-process broadcast channel; they are not a durable event log, and disconnected consumers may miss events. SSE streams send periodic keep-alive comments. WebSocket clients receive JSON event envelopes. See [Architecture](ARCHITECTURE.md#events-and-webhooks) for delivery distinctions.

## Webhook dead-letter queue

All routes require authentication.

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/webhooks/dlq` | List quarantined webhook events. |
| `GET` | `/webhooks/dlq/{id}` | Retrieve one quarantined event. |
| `POST` | `/webhooks/dlq/{id}/replay` | Requeue an event for delivery. |
| `POST` | `/webhooks/dlq/replay-all` | Requeue all eligible events. |
| `DELETE` | `/webhooks/dlq/{id}` | Permanently delete a DLQ entry. |

Delivery signatures, retry behavior, and operational guidance are documented in [Webhooks](WEBHOOKS.md).

## gRPC

The gRPC API listens on `service.port + 1`. It has overlapping, but not identical, functionality. See the [gRPC reference](GRPC.md) and the source contracts in [`proto/uwah/v1/`](../proto/uwah/v1/).
