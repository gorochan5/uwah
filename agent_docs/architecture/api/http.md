# API HTTP/REST

## Objetivo

Documentar a superfície HTTP suportada pelo código real.

## Estado atual

Não há rotas implementadas. `src/api/http/*.rs` e `src/routes/*.rs` estão vazios; `src/api/mod.rs` apenas declara módulos privados e `src/routes/mod.rs` não contém router. Axum está declarado em `Cargo.toml`, e `src/main.rs` contém apenas uma tentativa incompleta de `axum::serve`.

As tabelas de endpoints do README são intenção de produto, não contrato executável. Assim, não há parâmetros, JSON, códigos HTTP, middleware, health check ou tratamento de erros que possam ser confirmados.

## Rotas planejadas no README

O README menciona `/health`, operações de `/sessions`, leitura de chats, mensagens, contatos, participantes de grupos, histórico de chamadas e mídia. Os métodos marcados com 🚧 são explicitamente planejados. Nenhum deles possui handler no repositório.

## Segurança

Embora o README exija `Authorization: Bearer`, `src/security/mod.rs` está vazio e não há middleware. Consulte [../../security/security.md](../../security/security.md).

## Pendências

Implementar router, estado compartilhado, validação de path/body, mapeamento de erros para HTTP, autenticação e contratos JSON. Não usar esta página como especificação de integração até esses itens existirem.
