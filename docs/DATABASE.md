# Uwah Database Schema, Relational Integrity & Concurrency Architecture

## 1. Dual-Tier Storage Isolation

Uwah strictly separates business state from cryptographic stores:
1. **`uwah.db` (Relational Gateway Database)**: Central application data, contacts, chat histories, messages, outbox events, and group metadata (SQLx + SQLite WAL mode).
2. **`sessions/{id}.db` (Signal Cryptographic Stores)**: Per-session Noise keys, prekeys, and Double Ratchet state managed by `whatsapp-rust`.

---

## 2. Gateway Database Schema (`uwah.db`)

### 2.1 `sessions`
Tracks WhatsApp multi-session states and lifecycles.
| Column | Type | Constraints | Description |
| :--- | :--- | :--- | :--- |
| `id` | TEXT | PRIMARY KEY | Unique sanitized session ID (e.g. `primary_bot`) |
| `phone_number` | TEXT | NULLABLE | Authenticated user phone number |
| `push_name` | TEXT | NULLABLE | Profile display push name |
| `status` | TEXT | NOT NULL DEFAULT `'QRCODE'` | `QRCODE`, `PAIRING_CODE`, `CONNECTED`, `LOGGED_OUT`, `CONFLICT` |
| `qr_payload` | TEXT | NULLABLE | Vector QR pairing payload |
| `created_at` | TIMESTAMP | NOT NULL | Record creation timestamp |
| `updated_at` | TIMESTAMP | NOT NULL | Last update timestamp |

---

### 2.2 `outbox_events` (Transactional Outbox)
Enables reliable at-least-once webhook event delivery.
| Column | Type | Constraints | Description |
| :--- | :--- | :--- | :--- |
| `id` | TEXT | PRIMARY KEY | UUIDv7 monotonic event identifier |
| `session_id` | TEXT | NOT NULL, FK -> `sessions(id)` ON DELETE CASCADE | Owning session ID |
| `event_type` | TEXT | NOT NULL | Topic name (e.g. `message.received`, `session.connected`) |
| `aggregate_id` | TEXT | NOT NULL | Aggregate partition key (e.g. `chat_jid`) for FIFO ordering |
| `payload` | TEXT | NOT NULL | Serialized JSON event envelope |
| `status` | TEXT | NOT NULL DEFAULT `'pending'` | `'pending'`, `'dispatched'`, `'failed'` |
| `retry_count` | INTEGER | NOT NULL DEFAULT 0 | Retries consumed (max 10 before DLQ) |
| `next_retry_at` | INTEGER | NOT NULL DEFAULT 0 | Epoch millisecond schedule timestamp |
| `lease_until` | INTEGER | NOT NULL DEFAULT 0 | Lock lease expiry timestamp |
| `last_error` | TEXT | NULLABLE | Last HTTP/transport error message |
| `created_at` | INTEGER | NOT NULL | Epoch millisecond creation timestamp |

Indexes:
- `idx_outbox_poll` on `(status, next_retry_at)`
- `idx_outbox_aggregate_seq` on `(session_id, aggregate_id, created_at)`

---

### 2.3 `webhook_dlq` (Dead-Letter Queue)
Quarantine table for exhausted or permanently rejected webhook events.
| Column | Type | Constraints | Description |
| :--- | :--- | :--- | :--- |
| `id` | TEXT | PRIMARY KEY | UUIDv7 event identifier |
| `session_id` | TEXT | NOT NULL | Originating session ID |
| `event_type` | TEXT | NOT NULL | Event topic name |
| `payload` | TEXT | NOT NULL | Original JSON event body |
| `error_message` | TEXT | NOT NULL | Reason for DLQ quarantine |
| `retry_count` | INTEGER | NOT NULL | Number of retries attempted before DLQ |
| `failed_at` | TIMESTAMP | NOT NULL DEFAULT CURRENT_TIMESTAMP | Timestamp when moved to DLQ |

---

### 2.4 `group_metadata`
Stores WhatsApp group settings and permissions.
| Column | Type | Constraints | Description |
| :--- | :--- | :--- | :--- |
| `session_id` | TEXT | PK, FK -> `sessions(id)` ON DELETE CASCADE | Owning session |
| `group_jid` | TEXT | PK | WhatsApp Group JID (`...@g.us`) |
| `description` | TEXT | NULLABLE | Group description body |
| `description_id` | TEXT | NULLABLE | Optimistic concurrency token for descriptions |
| `is_locked` | BOOLEAN | NOT NULL DEFAULT 0 | True if only admins can edit group metadata |
| `is_announcement`| BOOLEAN | NOT NULL DEFAULT 0 | True if only admins can send messages |
| `ephemeral_expiration`| INTEGER | NULLABLE | Disappearing message expiration timer (seconds) |
| `membership_approval` | BOOLEAN | NOT NULL DEFAULT 0 | True if admin approval is required to join |
| `member_add_mode` | TEXT | NULLABLE | Who can add members |
| `member_link_mode`| TEXT | NULLABLE | Who can share invite links |
| `creator_jid` | TEXT | NULLABLE | Group creator JID |
| `parent_group_jid` | TEXT | NULLABLE | Parent community group JID if subgroup |
| `size` | INTEGER | NULLABLE | Total group participant count |
| `updated_at` | TIMESTAMP | NOT NULL DEFAULT CURRENT_TIMESTAMP | Last synchronized timestamp |

---

### 2.5 `media_cache`
Stores metadata and deduplicated SHA-256 binary references.
| Column | Type | Constraints | Description |
| :--- | :--- | :--- | :--- |
| `sha256` | TEXT | PRIMARY KEY | Plaintext binary file SHA-256 hex digest |
| `disk_path` | TEXT | NOT NULL | Path of the cached file |
| `mime_type` | TEXT | NOT NULL | MIME type (e.g. `image/jpeg`, `audio/ogg`) |
| `file_size`| INTEGER | NOT NULL | Binary file size on disk |
| `ref_count` | INTEGER | NOT NULL DEFAULT 1 | Number of logical references |
| `last_accessed_at` | TIMESTAMP | NOT NULL DEFAULT CURRENT_TIMESTAMP | Last cache access used by pruning |
| `created_at` | TIMESTAMP | NOT NULL DEFAULT CURRENT_TIMESTAMP | Ingestion timestamp |

### 2.6 Other gateway tables

The initial migration also creates `chats`, `messages`, `contacts`, `group_participants` and `calls`. Their composite keys are session-scoped: for example, chats use `(session_id, jid)` and messages/calls use `(id, session_id)`. Foreign keys cascade session deletion; message media references use `ON DELETE SET NULL`.

Later migrations add performance indexes, session-key cascade behavior, `outbox_events`, `webhook_dlq` and `group_metadata`. The authoritative schema is the SQL under `migrations/`; this document summarizes the current contract and should be updated whenever a migration changes it.

---

## 3. SQLite Engine Pragma Tuning

Configured during pool initialization in [`src/database/mod.rs`](file:///r:/projects/rust/uwah/src/database/mod.rs):
- `PRAGMA journal_mode = WAL;` (Concurrent readers and single writer without read locks)
- `PRAGMA synchronous = NORMAL;` (High throughput durability balance)
- `PRAGMA cache_size = -64000;` (64MB RAM page cache)
- `PRAGMA mmap_size = 268435456;` (256MB memory-mapped I/O)
- `PRAGMA temp_store = MEMORY;`
- `PRAGMA auto_vacuum = INCREMENTAL;`
- `PRAGMA busy_timeout = 30000;` (30-second contention backoff window)

Migrations are applied automatically during startup with `sqlx::migrate!("./migrations")`. A background maintenance task runs a passive WAL checkpoint and incremental vacuum every 30 minutes.
