# Interações com o banco

## Objetivo

Descrever o acesso SQL efetivamente presente.

## Conexões e inicialização

`src/database/mod.rs::init` cria `SqliteConnectOptions`, habilita foreign keys, WAL, sincronização `NORMAL`, criação do arquivo e um `SqlitePool` com `max_connections(5)`. O pool é guardado em `OnceLock`; `get()` falha com panic se `init()` não tiver sido chamado. Não há transações nem execução de migrations.

## Repositórios

`src/repository/{sessions,chats,messages,contacts,calls,group_participants,media_cache}.rs` usam `crate::database::get()`, `query_as!` para leituras e `query!` para escritas. Há `get_all`, buscas pontuais e, na maioria, `create`/`delete`. Sessões também têm `auth`, `refresh_qr` e `rename`.

As leituras filtradas por sessão existem em alguns métodos, mas mensagens, por exemplo, buscam por `id` sem `session_id`; participantes têm assinatura e filtro incompletos. Criações inserem apenas a chave, apesar de várias colunas serem `NOT NULL`, e não há atualização de chats, mensagens, contatos ou chamadas completas.

## Erros e consistência

As operações retornam `anyhow::Result` e adicionam contexto textual; não há mapeamento para API, retry, idempotência, transação ou estratégia de concorrência além do pool SQLite/WAL. Logs de repositório incluem identificadores e `sessions::auth`/`refresh_qr` registram dados sensíveis, inclusive QR payload.

## Estado

O acesso é um esqueleto não integrado ao `main`. Persistência de sessões, mensagens, eventos e mídia não está comprovadamente conectada ao motor WhatsApp. Consulte [schema.md](schema.md) e [../workflow.md](../workflow.md).
