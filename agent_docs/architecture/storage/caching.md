# Cache

## Objetivo

Documentar o cache de mídia e separar schema de comportamento implementado.

## Estado atual

Há cache de mídia em disco, exposto por `GET /sessions/{id}/media/{sha256}` e `POST /sessions/{id}/media/upload`. `src/models/media_cache.rs`, `src/repository/media_cache.rs` e a tabela `media_cache` mantêm SHA-256, caminho, MIME, tamanho, contagem de referências e timestamps.

O repositório oferece leituras, criação, atualização de acesso e exclusão/pruning de entradas não referenciadas; o handler de mídia grava e transmite arquivos. A configuração de TTL e tamanho máximo está em `[media]`. O readiness check ainda usa o caminho fixo `data/media_cache`, uma limitação conhecida.

## Consistência e pendências

Como o código não conecta mídia ao motor WhatsApp nem a handlers, nenhuma política de consistência pode ser afirmada. É necessário definir atomicidade entre arquivo e metadado, garbage collection, colisões de hash, concorrência e exposição HTTP antes de considerar o recurso implementado.
