# Conversas e mensagens

## Objetivo

Documentar chats e mensagens modelados no repositório.

## Chats

`src/models/chats.rs::Chat` representa uma conversa por (`session_id`, `jid`), com nome, `kind`, unread count, arquivamento, pin, mute, última mensagem e metadados de assunto. A migration aceita por convenção `direct`, `group`, `newsletter` e `broadcast`.

## Mensagens

`src/models/messages.rs::Message` contém id composto com sessão, chat, remetente, citação, direção, tipo, corpo, dados de mídia, status de entrega, flags de edição/exclusão e timestamp. Os valores `message_type` e `delivery_status` são strings; o SQL apenas os comenta.

## Operações e eventos

Os repositórios esboçam leitura/criação/exclusão, mas as criações não preenchem campos obrigatórios e não há envio, recebimento, histórico conectado a WhatsApp, eventos ou status de entrega implementados. Os handlers HTTP/gRPC estão vazios.

## Estado

Chats e mensagens são estruturas de persistência parcialmente preparadas; a integração funcional depende da implementação do adaptador `whatsapp-rust`, APIs e materialização de eventos.
