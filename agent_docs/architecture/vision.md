# Visão arquitetural do UWah

## Objetivo

Registrar a finalidade e o estado real do Unified WhatsApp API Hub (UWah).

## Visão geral

O UWah é concebido como um serviço Rust que encapsula `whatsapp-rust` e oferece uma fronteira única para consumidores externos, especialmente o Kronos BackOffice. O README descreve duas interfaces pretendidas: REST/HTTP com Axum e gRPC com Tonic, além de sessões múltiplas, persistência SQLite e eventos em tempo real.

No estado do repositório, essa visão ainda não está conectada em uma aplicação funcional: `src/main.rs` não compila, os módulos `src/api`, `src/routes` e `src/security` estão vazios e não há arquivo `.proto` em `proto/`.

## Problema e limites

A abstração pretende esconder detalhes do protocolo WhatsApp e oferecer contratos estáveis para sistemas de negócio. A responsabilidade do UWah seria administrar sessões, traduzir operações de mensagens/contatos/grupos/chamadas e materializar dados locais. Não há código que demonstre ainda integração com Kronos, um gerenciador de sessões ou processamento de eventos.

O projeto não é, atualmente, um CRM, um banco de dados distribuído, um provedor de WhatsApp independente nem uma API pública pronta para produção. `whatsapp-rust = 0.7.0` aparece no `Cargo.toml`, mas não é referenciado pelo código-fonte existente.

## Componentes e dependências

* Tokio fornece o runtime pretendido.
* Axum e Tonic são dependências declaradas para HTTP e gRPC.
* SQLx/SQLite sustentam o schema e repositórios esboçados.
* `uwah.toml` descreve banco, serviço, mídia e um valor `service.auth`.
* A rede WhatsApp e `whatsapp-rust` são dependências externas planejadas.

## Diferenciais e evolução

Multi-sessão, API dupla e separação entre modelos/repositórios/interfaces são objetivos arquiteturais documentados no README e refletidos na árvore de diretórios. Streaming gRPC, cache de mídia e eventos são propostas do README, não capacidades verificáveis na implementação atual.

## Estado atual

O schema inicial e os modelos Rust existem parcialmente; acesso SQL CRUD foi esboçado. Inicialização, APIs, autenticação, integração WhatsApp, eventos, cache em disco e migrações executadas permanecem pendentes.

Consulte [workflow.md](workflow.md), [api/http.md](api/http.md), [api/grpc.md](api/grpc.md) e [database/schema.md](database/schema.md).
