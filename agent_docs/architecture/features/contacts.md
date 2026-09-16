# Contatos

## Objetivo

Descrever a representação e o acesso atualmente disponíveis.

`src/models/contacts.rs::Contact` é identificado por (`session_id`, `jid`) e inclui `lid_jid`, telefone, nomes, status, foto, flag de negócio, username, devices e nome verificado. `src/repository/contacts.rs` contém leituras globais e por sessão, além de create/delete esboçados.

Não há sincronização com `whatsapp-rust`, consulta externa, eventos, handler HTTP/gRPC ou DTO. A criação fornece somente JID e sessão, deixando colunas obrigatórias sem valor. Portanto, o suporte é apenas modelo/schema e CRUD incompleto.
