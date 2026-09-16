# Segurança

## Objetivo

Registrar a proteção real (e não apenas a intenção descrita no README) das interfaces do UWah.

## Configuração

`src/config/mod.rs::Service` possui `auth: String` e valida somente que não seja vazio. `uwah.toml` contém um valor apresentado como SHA-256 do token original. Porém `config::init()` procura `kronos_pdv.toml`, não `uwah.toml`, e o código não usa `Service.auth` para validar requisições.

## Autenticação e autorização

`src/security/mod.rs` está vazio. Não existe parsing de `Authorization`, validação Bearer, hash SHA-256, comparação em tempo constante, expiração, rotação, múltiplos tokens, middleware HTTP, interceptor gRPC ou autorização por sessão. Também não há rotas implementadas, portanto não é possível classificar health check ou exceções.

O README exige `Authorization: Bearer <token>`, mas isso é requisito planejado/documental, não comportamento verificável. SHA-256 também não é, sozinho, um esquema completo de gestão de credenciais.

## Transporte, segredos e logs

`src/main.rs` tenta criar um listener TCP HTTP sem TLS; não há servidor gRPC funcional nem configuração TLS. TLS deverá ser provido externamente ou implementado explicitamente. O arquivo `uwah.toml` versionado contém um hash e comentário com token de exemplo; configuração/segredo devem ser retirados do controle de versão e protegidos operacionalmente.

Há risco de exposição: `repository::sessions::auth` registra push name, status e QR payload; `refresh_qr` também registra o QR. `src/log/mod.rs` escreve `uwah.log`. Os logs devem ser redigidos antes de produção.

## Lacunas prioritárias

Definir fonte de configuração correta, validação Bearer compartilhada entre HTTP/gRPC, comparação constante, política de rotação/expiração, escopo por sessão, respostas sem vazamento e TLS. Consulte [api/http.md](../architecture/api/http.md) e [api/grpc.md](../architecture/api/grpc.md).
