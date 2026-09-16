# API gRPC

## Objetivo

Registrar o contrato gRPC existente e suas lacunas.

## Estado atual

Não há arquivos `.proto` em `proto/`, embora `Cargo.toml` declare `tonic`, `prost` e `tonic-build`. `build.rs` contém somente `fn main() {}`; portanto, nenhum contrato é compilado e nenhum servidor Tonic é iniciado. Os arquivos `src/api/grpc/*.rs` são vazios.

Não existem serviços, RPCs, mensagens, enums, streaming, códigos de erro, interceptors ou autenticação gRPC verificáveis. O streaming de eventos citado no README é planejado.

## Relação arquitetural prevista

O layout sugere que implementações em `src/api/grpc` seriam adaptadores para uma camada de aplicação/gerenciador de sessões, mas essa camada não existe. Não é possível fornecer exemplos de cliente ou endpoints reais.

## Pendências

Definir `.proto`, geração em `build.rs`, endereço/servidor Tonic, modelo de eventos, semântica de cancelamento e autenticação por metadata. Ver também [security.md](../security/security.md) e [workflow.md](../workflow.md).
