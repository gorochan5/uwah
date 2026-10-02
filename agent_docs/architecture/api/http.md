# API HTTP/REST — estado atual

Este documento aponta para a referência operacional em [docs/API.md](../../../docs/API.md). O router real está em `src/api/http/mod.rs` e cobre health, sessões, chats, mensagens, contatos, grupos, chamadas, mídia, SSE, WebSocket e DLQ.

Rotas protegidas exigem `Authorization: Bearer <token>`. Swagger possui autenticação própria para navegador e o portal de scan valida query tokens. O contrato detalhado de métodos, rotas e limitações fica em `docs/API.md`.
