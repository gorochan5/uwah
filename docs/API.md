# Uwah REST API Specification

## 1. Authentication

All REST endpoints (except `/health` and the scan portal) require authentication.

### Methods
1. **Header Authentication (Standard REST)**:
   ```http
   Authorization: Bearer <TOKEN>
   ```
2. **Query Parameter (Browser / Webhook / Swagger)**:
   ```http
   GET /swagger?key=<TOKEN>
   GET /sessions/:id/scan?token=<TOKEN>
   ```
3. **Configuration Match (`uwah.toml`)**:
   `verify_token` supports:
   - Plaintext match (e.g. `auth = "123"` in `uwah.toml`)
   - SHA-256 hex digest match (e.g. `auth = "a665a45920422f9d417e4867efdc4fb8a04a1f3fff1fa07e998e86f7f7a27ae3"`)

---

## 2. Interactive Swagger UI

Interactive OpenAPI 3.1 documentation is served directly by the gateway:

- **URL**: `http://127.0.0.1:6767/swagger`
- **Protected Access**: Open `http://127.0.0.1:6767/swagger?key=123` (or supply the `Authorization: Bearer 123` header).
- **OpenAPI JSON Schema**: `/api-docs/openapi.json`

---

## 3. Real-Time QR Scanner Portal (Maud + HTMX)

For connecting a physical phone by scanning the QR code:

- **Portal Page**: `GET /sessions/:id/scan?token=123`
  - Zero image dependencies: renders clean vector SVG QR codes directly in browser.
  - Inlines HTMX for reactive background polling without page reloads.
  - Automatically stops polling upon connection confirmation (HTTP status `286`).
- **Dynamic Fragment**: `GET /sessions/:id/qr-frame?token=123`
  - Polled every 2 seconds by HTMX.

---

## 4. Endpoints Reference

### Health
| Method | Route | Description | Auth Required |
|---|---|---|---|
| `GET` | `/health` | Service health status | No |

---

### Sessions
| Method | Route | Description |
|---|---|---|
| `GET` | `/sessions` | List all sessions |
| `POST` | `/sessions` | Create and initialize a new WhatsApp session |
| `GET` | `/sessions/:id` | Fetch session metadata and QR status |
| `POST` | `/sessions/:id/qr` | Manually trigger a QR code refresh |
| `DELETE` | `/sessions/:id` | Disconnect client and remove session from DB |

#### `POST /sessions` Example Request
```json
{
  "id": "my-session"
}
```

---

### Messages
| Method | Route | Description |
|---|---|---|
| `GET` | `/sessions/:id/chats/:jid/messages` | List message history for a chat |
| `POST` | `/sessions/:id/chats/:jid/messages` | Send an outbound message (text or quote reply) |
| `DELETE` | `/sessions/:id/chats/:jid/messages/:msg_id` | Delete a message record from local DB |

#### `POST /sessions/:id/chats/:jid/messages`
**URL Params**:
- `id`: Session ID (e.g. `teste`)
- `jid`: Recipient JID (e.g. `5519998005388` or `5519998005388@s.whatsapp.net` or `12036302@g.us`)

**Plain Text Send**:
```json
{
  "body": "Hello from Uwah WhatsApp Gateway!"
}
```

**Quoted Reply Send**:
```json
{
  "body": "Replying to your previous message!",
  "quoted_message_id": "3EB0C44298FC1C14"
}
```

---

### Chats
| Method | Route | Description |
|---|---|---|
| `GET` | `/sessions/:id/chats` | List all conversation threads |
| `GET` | `/sessions/:id/chats/:jid` | Retrieve chat details by JID |
| `DELETE` | `/sessions/:id/chats/:jid` | Delete a chat record |

---

### Contacts
| Method | Route | Description |
|---|---|---|
| `GET` | `/sessions/:id/contacts` | List contacts |
| `POST` | `/sessions/:id/contacts` | Upsert contact info |
| `POST` | `/sessions/:id/contacts/sync` | Trigger contact synchronization with WhatsApp |
| `GET` | `/sessions/:id/contacts/:jid` | Get contact details |
| `DELETE` | `/sessions/:id/contacts/:jid` | Delete contact |

---

### Group Participants
| Method | Route | Description |
|---|---|---|
| `GET` | `/sessions/:id/chats/:jid/participants` | List members of a group |
| `DELETE` | `/sessions/:id/chats/:jid/participants/:user_jid` | Remove participant from group registry |

---

### Calls
| Method | Route | Description |
|---|---|---|
| `GET` | `/sessions/:id/calls` | List call history |
| `GET` | `/sessions/:id/calls/:call_id` | Get single call details |
| `DELETE` | `/sessions/:id/calls/:call_id` | Delete call entry |

---

### Media
| Method | Route | Description |
|---|---|---|
| `GET` | `/sessions/:id/media/:sha256` | Stream or download cached media file |

