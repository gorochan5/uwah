# Segurança — estado atual

## Objetivo

Registrar a proteção real (e não apenas a intenção descrita no README) das interfaces do UWah.

## Configuração

`Service.auth` é carregado de `uwah.toml`, validado como não vazio e usado tanto pelos middlewares HTTP quanto pelo interceptor de autenticação gRPC. O valor pode ser um token plaintext ou o digest SHA-256 hexadecimal correspondente.

## Autenticação e autorização

`src/security/mod.rs` implementa parsing de Bearer, comparação em tempo constante após pré-hash SHA-256 (`constant_time_eq_str`), validação de IDs de sessão, assinatura e verificação HMAC-SHA256 de webhooks e middleware HTTP.

- **Rotas REST protegidas:** Exigem `Authorization: Bearer <token>`. Swagger e scan aceitam query tokens para uso interativo no navegador.
- **Serviços gRPC (todos os 11 serviços Tonic):** Exigem metadata `authorization: Bearer <TOKEN>`, validado via `grpc_auth_interceptor` em `src/api/grpc/mod.rs`. Rejeições retornam `Status::unauthenticated("unauthenticated")`. O serviço de introspecção `grpc.reflection.v1.ServerReflection` é exposto para discovery de esquemas.
- **Proteção de Mídia e Sistema de Arquivos:** `MediaService` sanitiza e valida caminhos canônicos impedindo ataques de travessia de diretório (Path Traversal), retornando `Status::permission_denied` em tentativas de violação.

## Transporte, segredos e logs

`src/main.rs` cria listeners TCP HTTP e gRPC sem TLS nativo. Em ambientes de produção, a terminação TLS deve ser provida externamente (via reverse proxy, ingress ou túnel seguro).

Logs de autenticação e interceptor gRPC não registram tokens, credenciais ou valores completos de cabeçalhos de autorização.

## Lacunas prioritárias

Prioridades remanescentes: rotação/expiração dinâmica de tokens, RBAC multi-tenant / escopo granular por sessão, e TLS nativo opcional. Consulte [api/http.md](../architecture/api/http.md) e [api/grpc.md](../architecture/api/grpc.md).
