# Registro histórico: TODO de confiabilidade (2026-10-26)

Resumo em português do checklist e das notas consolidados na fila ativa
[`TODO.md`](../TODO.md). Este arquivo é histórico; itens podem ter sido
superados.

## Objetivo e regras

Construir um aplicativo confiável para sessões longas e projetos grandes,
inclusive em hardware modesto. O documento é a fonte de verdade; recursos de
renderer/GPU são aceleração descartável.

### Hardware mínimo de referência

- Usar inicialmente **4 GB de RAM, CPU dual-core e Intel de 4ª geração ou AMD
  equivalente e mais recente** como referência de validação.
- Considerar GPUs NVIDIA GeForce GTX série 1000 ou mais recentes, GPUs dedicadas
  AMD/Intel e GPUs integradas compatíveis, condicionadas aos drivers e APIs.
- Medições em hardware mais potente não substituem testes no perfil mínimo.
- Fazer mudanças focadas, estimar picos e preservar edições em falhas de
  alocação, cancelamento, salvamento ou renderer.
- Evitar reescrita ampla do renderer sem medições que identifiquem gargalo
  distinto de CPU, cópias de memória e uploads.

## Trabalho registrado como concluído

- Construtores fallíveis de documentos e validação de dimensões em criação,
  importação, arrastar/soltar e colagem.
- Limites de histórico e alertas localizados quando alterações não podem ser
  registradas por restrição de memória.
- Redução de buffers temporários em blur esparso, geometria, máscaras e
  redimensionamento; reservas fallíveis nos caminhos controlados pelo app.
- Aplicação de resultados de worker movendo documentos em vez de clonar pixels
  novamente; testes de ownership e undo/redo.
- Auditoria inicial de pressupostos 8-bit e contrato para precisão múltipla.
- Operações de seleção/camada e snapshots de exportação com tratamento de falha
  de alocação antes de mutações quando possível.
- Salvamento `.patlas` com buffer único fallível e substituição atômica; validação
  de truncamento e preservação do destino anterior.
- Estimativas de memória para documentos, histórico, seleções e workers, com
  ressalvas sobre GPU e buffers omitidos.
- Limite compartilhado de oito workers de imagem; jobs cancelados continuam
  ocupando vaga até terminar.
- Benchmarks release de blur, histórico e preenchimento em máquina moderna.

## Auditoria de picos: conclusões

- Workers de geometria podem manter documento vivo, clone de entrada e resultados
  simultaneamente. O limite de 256 MiB por documento não limita o pico do processo.
- O resampler `image` para filtros non-nearest cria buffers float de 16 bytes por
  texel; algumas alocações internas da biblioteca não são fallíveis pela API.
- Máscaras, composites, uploads, prévias, exportações e texturas podem se
  sobrepor. A estimativa por aba é parcial e não inclui toda capacidade de Vec,
  alocações externas, driver ou residência de GPU.
- Buffers principais de blur, median, morfologia e mosaico têm reservas
  fallíveis; ainda faltam auditar alocações internas de bibliotecas, callers
  legados de rebuild de grupos e recursos do renderer/backend.
- Em canvas 4096², o mapa de preenchimento pode ocupar 16 MiB e a fila de índices
  até 64 MiB.

## Benchmarks locais registrados

Ryzen 5 5600X, 32 GiB RAM, GeForce RTX 3050, Windows — acima do perfil mínimo.

| Cenário release | Resultado observado |
|---|---:|
| Gaussian blur, 3000², raio 24 | 120–138 ms |
| 30 traços, snapshots completos | 736–768 ms |
| 30 traços, histórico incremental | 18,1–18,3 ms |
| Preenchimento de região 3000² | 201 ms |
| Working set amostrado: preenchimento | 49,9 MiB |
| Working set amostrado: blur + histórico | 321,9 MiB |

São amostras de processos de teste, não picos de RSS garantidos e não medem
residência de GPU. Hardware mínimo e residência GPU continuam pendentes.

## Fila histórica de prioridades

1. **P0 — Memória e preservação:** concluir auditoria, definir limites baseados
   em medições, manter operações canceláveis e verificar integridade do
   documento/histórico em falhas.
2. **P1 — Recuperação:** implementar autosave transacional, retenção e recuperação
   controlada pelo usuário. O contrato está em
   [`AUTOSAVE_RECOVERY.md`](../reference/AUTOSAVE_RECOVERY.md); o recurso ainda
   não está implementado.
3. **P2 — Fluxo profissional:** planejar tiles/histórico incremental com
   profiling; validar pressão, seleção, transformação e exportação.
4. **P3 — Precisão múltipla, adiada:** RGBA8, RGBA16 e float32 em todo o fluxo,
   conforme [`HIGH_BIT_DEPTH.md`](../reference/HIGH_BIT_DEPTH.md), sem quantização
   silenciosa.
5. **P4 — Renderer, adiado:** medir CPU, composição, upload, efeitos e apresentação
   antes de escolher mudanças de backend.

## Checklist de validação

- Executar `cargo test --all-targets --locked`, `cargo fmt --all -- --check` e
  `git diff --check` após mudanças.
- Validar Windows/Linux quando houver código compartilhado ou específico de
  plataforma.
- Em mudanças de memória, documentar buffers simultâneos, estimativa de pico e
  alocações que bibliotecas/renderer ainda controlam.
