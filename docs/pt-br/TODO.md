# PaintAtlas — Próximos passos

Esta é a fila ativa, não o histórico do projeto. O piso inicial de validação é
**CPU dual-core, 4 GB de RAM e Intel de 4ª geração ou AMD equivalente, ou mais
recente**. A compatibilidade de GPU depende dos drivers e APIs disponíveis.

## Prioridades

1. [ ] **Validar Zoom e HUD de pincel**: testar mouse e caneta, atalhos
   configuráveis, zoom temporário e sessões de desenho; registrar regressões.
2. [ ] **Concluir a auditoria de memória**: revisar alocações restantes,
   especialmente em bibliotecas de filtros e recursos do backend. Registrar
   limites e medições em [`MEMORY_AUDIT.md`](reference/MEMORY_AUDIT.md).
3. [ ] **Medir no hardware mínimo**: RSS, picos de CPU e residência de GPU em
   Windows/Linux, com canvas grande, várias camadas, undo/redo, filtros e
   tablet/caneta. A máquina atual não substitui esse teste.
4. [ ] **Implementar recuperação/salvamento automático** conforme
   [`AUTOSAVE_RECOVERY.md`](reference/AUTOSAVE_RECOVERY.md), incluindo falhas de
   disco e recuperação sem sobrescrever projetos válidos.
5. [ ] **Definir um orçamento global de memória** após as medições do item 3,
   considerando documentos, histórico, workers, composites e renderer.

## Depois das prioridades

- Planejar armazenamento em tiles e evolução do histórico com base em benchmarks.
- Precisão múltipla e mudanças de renderer permanecem adiadas. Seguir os
  contratos em [`HIGH_BIT_DEPTH.md`](reference/HIGH_BIT_DEPTH.md) e no guia de
  [arquitetura](reference/ARCHITECTURE.md).
- Histórico detalhado: [`RELIABILITY_TODO_2026-10-26.md`](archive/RELIABILITY_TODO_2026-10-26.md).

## Validação de mudanças

Executar `cargo test --all-targets --locked`, `cargo fmt --all -- --check` e
`git diff --check`. Em mudanças de memória, registrar buffers simultâneos,
estimativas de pico e limites das bibliotecas/renderer.
