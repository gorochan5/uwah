# API gRPC — estado atual

Os contratos Protobuf estão distribuídos em `proto/uwah/v1/*.proto` (10 contratos) e implementados em `src/api/grpc/*.rs`. O servidor é inicializado por `src/api/grpc/mod.rs` na porta `service.port + 1` (default: 6768) com 100% de paridade com a API REST.

## Tonic Server Reflection

O servidor expõe o **gRPC Server Reflection Protocol v1** (`tonic-reflection`), permitindo introspecção dinâmica de esquemas e serviços através de ferramentas CLI (`grpcurl`), Postman gRPC e extensões de IDE sem necessidade de fornecimento manual dos arquivos `.proto`.

## Serviços disponíveis (11 Tonic Services)

1. **`HealthService`**:
   - `Check`: Verificação pontual de conectividade com SQLite (`SERVING` / `NOT_SERVING`).
   - `Watch`: Streaming periódico (5s) de status do gateway.
   - `CheckLiveness`: Probe de liveness (K8s).
   - `CheckReadiness`: Probe profunda de readiness (banco de dados e sessões ativas).
   - `CheckStartup`: Probe de startup (validação de migrações).
   - `ExportMetrics`: Exportador de métricas Prometheus textuais.
2. **`EventStreamService`**:
   - `StreamEvents`: Streaming HTTP/2 multiplexado em tempo real de eventos da plataforma, com suporte a filtro opcional por `session_id`.
3. **`SessionService`**:
   - `ListSessions`, `CreateSession`, `GetSession`, `DeleteSession`, `RefreshQr`, `RestartSession`, `LogoutSession`, `RequestPairingCode`.
4. **`ChatService`**:
   - `SetGlobalPresence`, `SetChatPresence`, `ListChats`, `GetChat`, `DeleteChat`, `MarkAsRead`, `MuteChat`, `PinChat`, `ArchiveChat`, `SetSubject`, `SetDescription`, `SetEphemeralTimer`, `SetGroupAnnouncement`, `SetGroupLock`, `GetInviteLink`, `LeaveGroup`.
5. **`GroupService`**:
   - `CreateGroup`, `JoinGroup`.
6. **`GroupParticipantService`**:
   - `ListParticipants`, `AddParticipants`, `RemoveParticipant`, `PromoteParticipants`, `DemoteParticipants`.
7. **`MessageService`**:
   - `ListMessages`, `SendMessage`, `SendLocation`, `SendContact`, `SendMedia`, `EditMessage`, `DeleteMessage`, `ReactMessage`, `RevokeMessage`.
8. **`ContactService`**:
   - `ListContacts`, `UpsertContact`, `SyncContacts`, `CheckContacts`, `GetContact`, `DeleteContact`, `GetProfilePicture`, `BlockContact`, `UnblockContact`.
9. **`CallService`**:
   - `ListCalls`, `GetCall`, `DeleteCall`, `RejectCall`.
10. **`MediaService`**:
    - `DownloadMedia`: Streaming de download em chunks de 64KB com suporte a range e proteção contra path traversal.
    - `UploadMedia`: Streaming de upload contínuo em chunks com cálculo de SHA-256 em tempo real.
    - `UploadMediaUnary`: Upload direto em payload binário unário.
11. **`WebhookDlqService`**:
    - `ListDlq`, `GetDlq`, `ReplayDlq`, `ReplayAllDlq`, `DeleteDlq`.

## Autenticação

O servidor gRPC está protegido por um interceptor Tonic (`grpc_auth_interceptor`) em cada serviço, que valida as credenciais usando o mesmo segredo configurado em `[service].auth` de `uwah.toml` (suportando token plaintext ou digest SHA-256 hexadecimal com comparação em tempo constante via `subtle::ConstantTimeEq`).

- **Header esperado:** `authorization: Bearer <TOKEN>` via metadata gRPC.
- **Comportamento de erro:** Retorna `Status::unauthenticated("unauthenticated")` caso o header esteja ausente, o prefixo não seja `Bearer `, o token seja vazio ou a validação falhe.
- **Segurança de Transporte:** TLS não é configurado diretamente no processo; deve ser provido por terminação TLS externa (reverse proxy / ingress) ou em rede interna confiável.

Para documentação de referência completa com exemplos `grpcurl`, consulte [docs/GRPC.md](../../../docs/GRPC.md).
Para a operação REST e eventos HTTP, consulte [docs/API.md](../../../docs/API.md).
