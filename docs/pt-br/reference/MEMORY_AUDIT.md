# Auditoria de memória e notas de benchmark

Este documento apoia a fila em [`TODO.md`](../TODO.md); não é um orçamento de
memória do processo. As estimativas de CPU não incluem toda a capacidade do
alocador, intermediários de bibliotecas, staging do renderer ou residência de
GPU. Amostragens de working set podem perder picos curtos.

## Picos e limites conhecidos

- Workers de geometria clonam documentos; entrada, saída e documento original
  podem coexistir. O limite de 256 MiB é por documento/saída, não um limite de
  pico do processo.
- Redimensionamento que não seja nearest usa intermediários RGBA32F de 16 bytes
  por texel; algumas alocações internas do crate `image` não são fallíveis pela
  API usada.
- Workers de imagem compartilham limite de admissão de oito jobs. Estimativas de
  bytes são heurísticas, não limites do alocador. Jobs cancelados ocupam o slot
  até terminar, e processos distintos não são coordenados.
- `Document::MAX_BYTES` é 256 MiB para pixels do documento e caches de grupos.
  Um canvas RGBA8 4096² ocupa 64 MiB, sem contar histórico, workers, composites,
  staging ou GPU.
- Uploads de camada/grupo, composites de efeitos e prévias de seleção/colagem
  possuem reservas explícitas fallíveis na CPU. Alocações de textura do egui,
  backend e driver permanecem fora desse controle.
- O preenchimento por balde descobre a região antes de alterar pixels; em
  4096², o mapa visitado pode ocupar 16 MiB e a fila de índices até 64 MiB.
- Buffers principais de blur, median, morfologia e mosaico usam reservas
  fallíveis. Ainda faltam auditar alocações internas de bibliotecas externas,
  callers legados de rebuild de grupos e recursos de renderer/backend.

## Medições locais registradas

Máquina de referência: Ryzen 5 5600X, 32 GiB RAM, GeForce RTX 3050 e Windows —
acima do hardware mínimo.

| Cenário release | Resultado observado |
|---|---:|
| Gaussian blur, 3000², raio 24 | 120–138 ms |
| 30 traços, 3000², snapshots completos | 736–768 ms |
| 30 traços, 3000², histórico incremental | 18,1–18,3 ms |
| Preenchimento de região 3000² | 201 ms |
| Working set amostrado do teste de preenchimento | 49,9 MiB |
| Working set amostrado dos testes de blur/histórico | 321,9 MiB |

São amostras de working set de processos de teste separados, não picos de RSS
garantidos; os testes não criam as texturas do renderer do aplicativo. Residência
de GPU e medições no hardware mínimo continuam pendentes.

Benchmarks manuais:

```text
cargo test --release --locked document::tests::benchmark_bucket_fill_3000 -- --ignored --nocapture
cargo test --release --locked -- --ignored --nocapture
```

## Trabalho de segurança já realizado

Há caminhos limitados/fallíveis para construção de documentos, geometria,
seleções, serialização, leitura de projetos, admissão de workers, exportação,
preenchimento e buffers principais de filtros. Consulte os testes para garantias
detalhadas. O orçamento global do processo e as alocações externas permanecem
em aberto.
