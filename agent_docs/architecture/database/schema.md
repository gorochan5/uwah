# Schema do banco

## Objetivo

Descrever o schema declarado em `migrations/20260909173149_init.sql`.

## Visão geral

O banco atual é SQLite via SQLx. As migrations criam as tabelas de gateway, índices e estruturas de outbox/DLQ/metadados. A execução ocorre automaticamente em `src/database/mod.rs`; consulte `docs/DATABASE.md` para o resumo atual.

| Tabela | Chave primária | Relações principais |
|---|---|---|
| `sessions` | `id` | raiz das entidades por sessão |
| `chats` | (`session_id`, `jid`) | `sessions`; referenciada por mensagens/grupos |
| `messages` | (`id`, `session_id`) | sessão, chat e mídia |
| `contacts` | (`session_id`, `jid`) | sessão |
| `group_participants` | (`session_id`, `group_jid`, `user_jid`) | chat de grupo |
| `calls` | (`id`, `session_id`) | sessão |
| `media_cache` | `sha256` | mensagens apontam para ela |

## Colunas

| Tabela | Colunas declaradas |
|---|---|
| `sessions` | `id TEXT NOT NULL`, `phone_number TEXT`, `push_name TEXT`, `status TEXT NOT NULL DEFAULT 'QRCODE'`, `qr_payload TEXT`, `created_at/updated_at TIMESTAMP NOT NULL` |
| `chats` | `session_id`, `jid` obrigatórios; `name`, `kind DEFAULT 'direct'`, contadores/flags, `muted_until`, `last_message_at`, assunto e timestamps |
| `messages` | identificadores, remetente, citação, flags, `message_type`, corpo, URLs/chaves/hash de mídia, `delivery_status`, edição/exclusão e `timestamp` |
| `contacts` | JIDs, telefone, nomes/status/foto, negócio, username, devices e nome verificado |
| `group_participants` | sessão, grupo, usuário, `role DEFAULT 'member'`, `joined_at` |
| `calls` | sessão, id, chat, caller, flags vídeo/grupo, status, duração e timestamps |
| `media_cache` | hash, caminho, MIME, tamanho, referências e timestamps |

Foreign keys usam `ON DELETE CASCADE` para dados dependentes da sessão/chat e `SET NULL` para `messages.media_sha256`. A migration não declara CHECK constraints; valores como status e kind são convenções em comentários.

## Inicialização

`src/database/mod.rs` habilita foreign keys e WAL, executa as migrations e usa até cinco conexões. A manutenção periódica faz checkpoint WAL e vacuum incremental.
