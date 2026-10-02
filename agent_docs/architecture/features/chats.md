# Conversas e mensagens — estado atual

> Os handlers atuais estão em `src/api/http/chats.rs` e `src/api/http/messages.rs`. Use `docs/API.md` como contrato; notas sobre handlers vazios são históricas.

## Objetivo

Documentar chats e mensagens modelados no repositório.

## Chats

`src/models/chats.rs::Chat` representa uma conversa por (`session_id`, `jid`), com nome, `kind`, unread count, arquivamento, pin, mute, última mensagem e metadados de assunto. A migration aceita por convenção `direct`, `group`, `newsletter` e `broadcast`.

## Mensagens

`src/models/messages.rs::Message` contém id composto com sessão, chat, remetente, citação, direção, tipo, corpo, dados de mídia, status de entrega, flags de edição/exclusão e timestamp. Os valores `message_type` e `delivery_status` são strings; o SQL apenas os comenta.

## Operações e eventos

Os repositórios e handlers atuais cobrem leitura, criação, exclusão, envio, recebimento, histórico, eventos e status de entrega em diferentes fluxos. Para o contrato de integração, consulte `docs/API.md`; para detalhes de implementação, consulte `src/api/http/chats.rs`, `src/api/http/messages.rs` e `src/events/handlers/messages.rs`.

## Estado

Chats e mensagens são estruturas de persistência parcialmente preparadas; a integração funcional depende da implementação do adaptador `whatsapp-rust`, APIs e materialização de eventos.
