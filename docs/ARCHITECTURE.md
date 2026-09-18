# Uwah Architecture

## 1. Overview

**Uwah** is a self-hosted, high-performance WhatsApp gateway written in Rust. It decouples client-facing applications from the underlying WhatsApp multi-device binary protocol using [`whatsapp-rust`](https://github.com/oxidezap/whatsapp-rust), Axum 0.8, and SQLite (SQLx).

---

## 2. High-Level Architecture

```
                      ┌────────────────────────────┐
                      │    HTTP Clients / Browsers │
                      └─────────────┬──────────────┘
                                    │
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│                              Axum 0.8                                  │
│  ┌─────────────────────────┬───────────────────┬────────────────────┐  │
│  │   Protected REST API    │   Maud + HTMX     │     Swagger UI     │  │
│  │  (/sessions, /chats...) │ (/sessions/:id/   │   (/swagger?key=)  │  │
│  │  (Bearer Auth Token)    │      scan)        │   (OpenAPI 3.1)    │  │
│  └────────────┬────────────┴─────────┬─────────┴──────────┬─────────┘  │
│               │                      │                    │            │
│               └──────────────────────┼────────────────────┘            │
│                                      ▼                                 │
│                         ┌─────────────────────────┐                    │
│                         │        AppState         │                    │
│                         │ - &'static Config       │                    │
│                         │ - SqlitePool (uwah.db)  │                    │
│                         │ - Arc<SessionManager>   │                    │
│                         └────────────┬────────────┘                    │
└──────────────────────────────────────┼─────────────────────────────────┘
                                       │
                                       ▼
┌────────────────────────────────────────────────────────────────────────┐
│                        SessionManager Layer                            │
│           (Registry of Active WhatsApp Bots: HashMap<ID, Arc<Client>>)  │
│                                                                        │
│   ┌──────────────────────────┐        ┌──────────────────────────┐     │
│   │    WhatsApp Session A    │        │    WhatsApp Session B    │     │
│   │   - Bot / Client Worker  │        │   - Bot / Client Worker  │     │
│   │   - Background Tokio Task│        │   - Background Tokio Task│     │
│   │   - on_message handler   │        │   - on_message handler   │     │
│   │   - on_qr_code handler   │        │   - on_qr_code handler   │     │
│   │   - on_connected handler │        │   - on_connected handler │     │
│   └────────────┬─────────────┘        └────────────┬─────────────┘     │
└────────────────┼───────────────────────────────────┼───────────────────┘
                 │                                   │
                 ▼                                   ▼
      ┌─────────────────────┐             ┌─────────────────────┐
      │  sessions/{id_a}.db │             │  sessions/{id_b}.db │
      │ (Signal Keys Store) │             │ (Signal Keys Store) │
      └─────────────────────┘             └─────────────────────┘
                 │                                   │
                 └─────────────────┬─────────────────┘
                                   ▼
                         ┌───────────────────┐
                         │      uwah.db      │
                         │(Gateway App State)│
                         └───────────────────┘
```

---

## 3. Dual-Database Storage Design

To guarantee cryptographic integrity and complete isolation between sessions, **Uwah strictly enforces a dual-database architecture**:

### A. Application State Database (`uwah.db`)
- **Engine**: SQLite managed by SQLx (`sqlx::SqlitePool`).
- **Configuration**: Configured via `uwah.toml` under `[database] dir`.
- **Concurrency**: Operates in **WAL mode** (`PRAGMA journal_mode = WAL`), `synchronous = NORMAL`, `foreign_keys = ON`, and **`busy_timeout = 30s`**.
- **Contents**:
  - `sessions`: Session registry (status, phone number, push name, active QR payload).
  - `chats`: Conversation threads (direct, group, newsletter).
  - `messages`: Inbound and outbound message records, status (`sent`, `received`), timestamps, reply context.
  - `contacts`: Address book, LID-to-Phone mappings, verified names.
  - `group_participants`: Membership tracking and admin permissions.
  - `calls`: Audio/video call history.
  - `media_cache`: Local file cache deduplication.

### B. Session Protocol Stores (`sessions/{id}.db`)
- **Engine**: SQLite managed internally by `whatsapp-rust-sqlite-storage` (Diesel + r2d2).
- **Configuration**: Root directory configured via `uwah.toml` under `[sessions] dir`.
- **Purpose**: Exclusively holds low-level WhatsApp protocol state: Signal prekeys, identity keys, sender keys, Noise handshake secrets, and app state sync keys.
- **Isolation**: Each connected WhatsApp number receives its own dedicated `.db` file. The application code never queries or mutates this file directly.

---

## 4. Concurrency & Lifecycle Management

1. **Static Configuration**:
   Loaded once at boot using `OnceLock<Config>`. Exposed to all handlers via `&'static Config` (zero heap allocations per request).
2. **Background Session Worker**:
   When a session is created or accessed, `SessionManager` boots an async `Bot`. The `BotHandle` is spawned on a dedicated Tokio background task (`tokio::spawn(async move { bot_handle.await })`), keeping the Noise socket and event listener alive across HTTP request boundaries.
3. **Inbound Event Processing**:
   The `on_message` hook intercepts real-time incoming messages, automatically ensures the corresponding chat stub exists in `uwah.db` (preventing foreign key violations), extracts payload text, and persists the record idempotently.
4. **Outbound Messaging Pipeline**:
   The REST API retrieves the `Arc<Client>` from `SessionManager`, builds a WhatsApp `wa::Message` (with `ContextInfo` when `quoted_message_id` is present), and dispatches asynchronously through WhatsApp's encrypted channel.

