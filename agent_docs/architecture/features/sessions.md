# Sessões WhatsApp — estado atual

> A API atual está em `docs/API.md`; o ciclo de vida é implementado por `src/api/http/sessions.rs` e `src/api/session_manager.rs`. Notas históricas abaixo devem ser lidas nesse contexto.

## Objetivo

Registrar o modelo de sessão e o nível de implementação.

## Modelo

`src/models/sessions.rs::Session` contém `id`, telefone, push name, `status`, `qr_payload` e timestamps. A tabela `sessions` usa `QRCODE` como default e o repositório `sessions::auth` força o status `OK` e limpa o QR.

## Operações declaradas

`src/repository/sessions.rs` esboça `get_all`, `get_one`, `create`, `delete`, `auth`, `refresh_qr` e `rename`. Isso representa persistência de metadados, não uma sessão conectada: não há `whatsapp-rust::Client`, armazenamento de credenciais, QR gerado, reconexão, isolamento em memória ou eventos.

## Ciclo de vida

Os estados são strings e não há enum nem máquina de estados. `QRCODE` e `OK` aparecem no schema/README; transições, expiração e encerramento não são implementados. A autenticação chamada `auth` apenas executa UPDATE no banco.

## Limitações

Não existe API ou fluxo operacional que crie a sessão, autentique no WhatsApp ou restaure credenciais. QR payload é armazenado em texto e também registrado em log pelo repositório; isso deve ser revisto antes de uso real.
