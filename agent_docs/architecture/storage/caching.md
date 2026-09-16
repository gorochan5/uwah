# Cache

## Objetivo

Documentar o cache de mídia e separar schema de comportamento implementado.

## Estado atual

Não há implementação de cache em memória ou em disco. `src/models/media_cache.rs`, `src/repository/media_cache.rs` e a tabela `media_cache` apenas descrevem metadados: SHA-256, caminho, MIME, tamanho, contagem de referências e timestamps.

O repositório oferece leituras por hash, criação e exclusão esboçadas. Não há escrita de arquivo na pasta configurada em `[media]`, expiração, invalidação, atualização de `last_accessed_at`, ajuste de `ref_count`, locks ou reconciliação com mensagens.

## Consistência e pendências

Como o código não conecta mídia ao motor WhatsApp nem a handlers, nenhuma política de consistência pode ser afirmada. É necessário definir atomicidade entre arquivo e metadado, garbage collection, colisões de hash, concorrência e exposição HTTP antes de considerar o recurso implementado.
