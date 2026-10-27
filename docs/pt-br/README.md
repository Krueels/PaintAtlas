# Documentação do PaintAtlas — Português brasileiro

Esta pasta reúne as versões em português brasileiro. A documentação canônica do
projeto é mantida em inglês.

## Documentação atual

- [Próximos passos](TODO.md): fila ativa de trabalho.
- [Auditoria de memória](reference/MEMORY_AUDIT.md): limites conhecidos e medições.
- [Arquitetura](reference/ARCHITECTURE.md): organização do código-fonte.
- [Recuperação e salvamento automático](reference/AUTOSAVE_RECOVERY.md): contrato
  proposto; o recurso ainda não está implementado.
- [Catálogo de filtros](reference/FILTER_CATALOG.md): filtros, diferenças e
  recursos adiados.
- [Precisão de cor](reference/HIGH_BIT_DEPTH.md): plano para documentos de 8,
  16 e 32 bits.

## Arquivo

- [Análise técnica do projeto](archive/PROJECT_ANALYSIS.md): avaliação histórica
  de uma versão anterior; alguns detalhes podem estar desatualizados.
- [Checklist histórico de confiabilidade](archive/RELIABILITY_TODO_2026-10-26.md):
  auditoria e registro das tarefas daquela etapa.

Consulte também o [índice canônico em inglês](../README.md).

## Réguas, QuickShape e referências

- **Réguas e simetria:** a ferramenta Régua (`U`) cria réguas vinculadas às camadas, com simetria de 2 a 16 traços, guias paralelas/radiais e perspectiva de 1, 2 ou 3 pontos. Os projetos `.patlas` v8 preservam essas réguas; versões v1–v7 continuam abrindo normalmente.
- **QuickShape:** desenhe com pincel ou borracha e mantenha a caneta parada no final. O traço se transforma suavemente em uma forma reconhecida. Arraste para ajustar, use `Shift` para formas regulares e edite as alças depois de soltar. As opções ficam em Configurações → Desenho.
- **Referências:** em Janela → Referências, vincule várias imagens locais e alterne entre elas. Use o scroll para zoom e arraste para mover. Somente os caminhos e a posição de cada referência são salvos; imagens não são incorporadas aos projetos. Remover ou limpar a lista exige confirmação. O menu permite recarregar o original ou localizar um arquivo movido.

Os nomes antigos `compilar.sh` e `gerar-appimage.sh` continuam disponíveis como aliases dos scripts em inglês.
