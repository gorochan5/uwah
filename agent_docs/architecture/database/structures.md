# Estruturas de dados de persistência

## Objetivo

Relacionar modelos Rust e tabelas declaradas.

## Modelos

Todos os modelos em `src/models` derivam `Serialize`, `Deserialize`, `Debug`, `Clone` e `sqlx::FromRow` (com variações de imports não utilizados).

| Rust | Arquivo | Correspondência |
|---|---|---|
| `Session` | `src/models/sessions.rs` | `sessions`; status e QR são `String`/`Option<String>` |
| `Chat` | `src/models/chats.rs` | `chats`; flags bool, contadores i64, timestamps opcionais |
| `Message` | `src/models/messages.rs` | `messages`; `media_key` vira `Option<Vec<u8>>` |
| `Contact` | `src/models/contacts.rs` | `contacts`; `devices` permanece string |
| `GroupParticipant` | `src/models/group_participants.rs` | `group_participants` |
| `Call` | `src/models/calls.rs` | `calls` |
| `MediaCache` | `src/models/media_cache.rs` | `media_cache` |

Não há DTOs de API, enums Rust para estados, conversões de domínio, entidades transitórias ou estruturas de sessão em memória. Os estados descritos no SQL/README são strings sem validação de domínio.

## Serialização

`serde` permite serialização potencial para JSON, mas nenhum handler usa esses modelos. `sqlx::query_as!` faz aliases explícitos para timestamps UTC e nulabilidade. A migration usa `TIMESTAMP`, enquanto o modelo usa `chrono::DateTime<Utc>`.

## Limitações

Os modelos não representam todas as regras relacionais e alguns repositórios tentam inserir somente identificadores em colunas `NOT NULL`, portanto não constituem ainda uma camada persistente funcional.
