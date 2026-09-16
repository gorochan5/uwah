# Grupos

## Objetivo

Registrar o suporte efetivamente identificado para grupos.

Grupos não têm entidade própria: são `Chat` com `kind = "group"`, identificados pelo JID. Participantes usam `GroupParticipant` em `src/models/group_participants.rs`, com sessão, `group_jid`, `user_jid`, `role` e `joined_at`; a migration prevê `member`, `admin` e `superadmin` por convenção.

`src/repository/group_participants.rs` esboça leitura/criação/exclusão, mas `get_one`, `create` e `delete` operam apenas com `session_id` e não refletem a chave composta completa. Não existem metadados sincronizados, operações de gerenciamento, eventos, API ou integração WhatsApp implementados.
