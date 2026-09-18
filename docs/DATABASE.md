# Uwah Database Schema & Concurrency Guide

## 1. Overview

Uwah maintains two strictly isolated database tiers:
1. **`uwah.db`**: Gateway business state and relational data model (SQLx + SQLite).
2. **`sessions/{id}.db`**: Cryptographic Signal protocol store (whatsapp-rust internal engine).

---

## 2. Gateway Database Schema (`uwah.db`)

### `sessions`
Tracks active and historical WhatsApp sessions.
| Column | Type | Constraints | Description |
|---|---|---|---|
| `id` | TEXT | PRIMARY KEY | Unique identifier for the session |
| `phone_number` | TEXT | NULLABLE | Authenticated phone number (e.g. `5519998005388`) |
| `push_name` | TEXT | NULLABLE | WhatsApp profile display name |
| `status` | TEXT | NOT NULL DEFAULT `'QRCODE'` | `QRCODE`, `CONNECTED`, `LOGGED_OUT` |
| `qr_payload` | TEXT | NULLABLE | Raw WhatsApp pairing string |
| `created_at` | TIMESTAMP | NOT NULL | Record creation time |
| `updated_at` | TIMESTAMP | NOT NULL | Last status/qr update |

---

### `chats`
Conversation threads (1:1 direct chats, group chats, newsletters).
| Column | Type | Constraints | Description |
|---|---|---|---|
| `session_id` | TEXT | PK, FK -> `sessions(id)` | Owning session |
| `jid` | TEXT | PK | WhatsApp Chat JID (`...s.whatsapp.net` or `...@g.us`) |
| `name` | TEXT | NULLABLE | Contact display name or group title |
| `kind` | TEXT | NOT NULL DEFAULT `'direct'` | `'direct'`, `'group'`, `'newsletter'`, `'broadcast'` |
| `unread_count` | INTEGER | NOT NULL DEFAULT 0 | Unread message counter |
| `is_archived` | BOOLEAN | NOT NULL DEFAULT 0 | Archived state |
| `is_pinned` | BOOLEAN | NOT NULL DEFAULT 0 | Pinned state |
| `last_message_at` | INTEGER | NULLABLE | Epoch timestamp of latest message |
| `created_at` | TIMESTAMP | NOT NULL | Creation timestamp |
| `updated_at` | TIMESTAMP | NOT NULL | Update timestamp |

---

### `messages`
Inbound and outbound message records.
| Column | Type | Constraints | Description |
|---|---|---|---|
| `id` | TEXT | PK | WhatsApp message/stanza ID |
| `session_id` | TEXT | PK, FK -> `sessions(id)` | Owning session |
| `chat_jid` | TEXT | NOT NULL, FK -> `chats(session_id, jid)` | Target chat |
| `sender_jid` | TEXT | NOT NULL | Sender JID |
| `quoted_message_id`| TEXT | NULLABLE | Stanza ID of quoted message if reply |
| `is_from_me` | BOOLEAN | NOT NULL | True if sent by gateway |
| `message_type` | TEXT | NOT NULL | `'text'`, `'image'`, `'audio'`, etc. |
| `body` | TEXT | NULLABLE | Text content or media caption |
| `delivery_status` | TEXT | NOT NULL | `'pending'`, `'sent'`, `'delivered'`, `'read'`, `'received'` |
| `is_edited` | BOOLEAN | NOT NULL DEFAULT 0 | True if message edit was applied |
| `is_deleted` | BOOLEAN | NOT NULL DEFAULT 0 | True if revoked |
| `timestamp` | TIMESTAMP | NOT NULL | Message creation timestamp |

---

### `contacts`
Contact book synchronized from WhatsApp servers.
| Column | Type | Constraints | Description |
|---|---|---|---|
| `session_id` | TEXT | PK, FK -> `sessions(id)` | Owning session |
| `jid` | TEXT | PK | Contact Phone Number JID |
| `lid_jid` | TEXT | NULLABLE | Linked Identity JID |
| `phone_number` | TEXT | NULLABLE | Clean digits format |
| `push_name` | TEXT | NULLABLE | Name chosen by user |
| `is_business` | BOOLEAN | NOT NULL DEFAULT 0 | True if WhatsApp Business account |

---

### `group_participants`
Group membership records.
| Column | Type | Constraints | Description |
|---|---|---|---|
| `session_id` | TEXT | PK | Owning session |
| `group_jid` | TEXT | PK, FK -> `chats(session_id, jid)` | Target group chat |
| `user_jid` | TEXT | PK | Member user JID |
| `role` | TEXT | NOT NULL DEFAULT `'member'` | `'member'`, `'admin'`, `'superadmin'` |
| `joined_at` | TIMESTAMP | NOT NULL | Join timestamp |

---

### `calls`
Voice and video call history.
| Column | Type | Constraints | Description |
|---|---|---|---|
| `session_id` | TEXT | PK, FK -> `sessions(id)` | Owning session |
| `id` | TEXT | PK | Call identifier |
| `chat_jid` | TEXT | NOT NULL | Peer or group JID |
| `caller_jid` | TEXT | NOT NULL | Caller user JID |
| `is_video` | BOOLEAN | NOT NULL | Call format |
| `status` | TEXT | NOT NULL | `'ringing'`, `'accepted'`, `'missed'`, `'rejected'` |

---

## 3. Concurrency & Reliability Tuning

SQLite allows multiple concurrent readers in WAL mode, but only **one single writer at any instant**.

To prevent `SQLITE_BUSY` crashes:
1. **`busy_timeout(30s)`**: Configured on `SqliteConnectOptions`. Any transaction waiting for the write lock will sleep and retry for up to 30 seconds rather than aborting.
2. **`journal_mode = WAL`**: Enables concurrent readers during active background writes.
3. **`synchronous = Normal`**: Maximizes write throughput while preserving ACID crash resilience.
4. **Idempotency**: All mutations use `ON CONFLICT (...) DO UPDATE` or `ON CONFLICT (...) DO NOTHING` to guarantee that replays or network retries never panic.

