# Chamadas de voz e vídeo — estado atual

> Histórico e rejeição de chamadas possuem handlers em `src/api/http/calls.rs`. Consulte `docs/API.md`; este arquivo contém notas históricas de implementação.

## Objetivo

Distinguir a capacidade declarada da implementação real.

`src/models/calls.rs::Call` e a tabela `calls` representam histórico com caller, chat, vídeo, chamada em grupo, status, duração e timestamps. A migration comenta `offered`, `accepted`, `rejected`, `ended` e `missed`; são strings, não estados tipados.

`src/repository/calls.rs` esboça leitura, criação e exclusão, porém a criação preenche somente id/sessão, incompatível com outros campos obrigatórios. Não há handlers, eventos, sinalização, mídia de voz/vídeo ou uso das capacidades VoIP de `whatsapp-rust` no UWah.

**Estado:** não suportado pelo serviço atual; qualquer suporte futuro exige integração explícita, estados, ciclo de vida e limites de mídia.
