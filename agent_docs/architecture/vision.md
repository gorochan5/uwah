# Visão arquitetural do UWah — estado atual

> Este documento foi revisado em 2026-10-01. Para a referência operacional atual, consulte `README.md` e `docs/`.

## Objetivo

Registrar a finalidade e o estado real do Unified WhatsApp API Hub (UWah).

## Visão geral

O UWah é um serviço Rust que encapsula `whatsapp-rust` e oferece uma fronteira REST/HTTP com Axum, uma interface gRPC limitada a health/event streaming, sessões múltiplas, persistência SQLite e eventos em tempo real.

O estado atual conecta essa visão em uma aplicação com inicialização, router HTTP, SessionManager, eventos, migrations e servidor gRPC. Os contratos `.proto` estão em `proto/`; as limitações remanescentes estão registradas em `docs/ARCHITECTURE.md`.

## Problema e limites

A abstração esconde detalhes do protocolo WhatsApp e oferece contratos para sistemas de negócio. A responsabilidade do UWah é administrar sessões, traduzir operações de mensagens/contatos/grupos/chamadas e materializar dados locais. Não há integração específica com Kronos documentada no código.

O projeto não é um CRM, um banco de dados distribuído nem uma API pública pronta para produção. A integração com `whatsapp-rust = 0.7.0` é usada pelo `SessionManager` para os workers de sessão.

## Componentes e dependências

* Tokio fornece o runtime pretendido.
* Axum e Tonic são dependências declaradas para HTTP e gRPC.
* SQLx/SQLite sustentam o schema e repositórios esboçados.
* `uwah.toml` descreve banco, serviço, mídia e um valor `service.auth`.
* A rede WhatsApp e `whatsapp-rust` são dependências usadas pelo SessionManager.

## Diferenciais e evolução

Multi-sessão, API REST, streaming gRPC de eventos, cache de mídia, eventos SSE/WebSocket e outbox de webhooks estão representados na implementação atual. A autenticação gRPC e TLS permanecem ausentes.

## Estado atual

O schema, os modelos, os repositórios CRUD, as APIs, a autenticação HTTP, a integração de sessões, os eventos, o cache em disco e a execução automática de migrations estão implementados em diferentes níveis de completude. A fonte autoritativa do schema é `migrations/`.

Consulte [workflow.md](workflow.md), [api/http.md](api/http.md), [api/grpc.md](api/grpc.md) e [database/schema.md](database/schema.md).
