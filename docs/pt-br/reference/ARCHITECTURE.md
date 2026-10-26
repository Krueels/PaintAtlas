# Arquitetura do projeto

PaintAtlas é um aplicativo Rust. O código-fonte é organizado por
responsabilidade; `main.rs` declara os módulos para manter imports estáveis como
`crate::document`.

## Mapa do código-fonte

| Diretório | Responsabilidade |
|---|---|
| `src/ui/` | Estrutura do app, painéis, diálogos e fluxos de interface. |
| `src/document/` | Documentos, camadas, grupos, seleções e histórico undo/redo. |
| `src/brush/` | Pontas de pincel, pintura e processamento de traços/pressão. |
| `src/image/` | Operações de imagem e filtros. |
| `src/graphics/` | Shaders de GPU e auxiliares de composição. |
| `src/input/` | Backends de entrada de tablet e caneta. |
| `src/io/` | Área de transferência e importação por arrastar e soltar. |
| `src/settings/` | Preferências, atalhos, presets, tema, workspace e localização. |
| `assets/` | Ícones, fontes, artes e pontas de pincel incluídas no app. |
| `vendor/` | Dependências de terceiros modificadas; preservar sua estrutura. |

## Convenções

- Coloque módulos no diretório correspondente e registre módulos de nível
  superior em `src/main.rs`.
- Mantenha detalhes de interface em `src/ui/`; operações reutilizáveis de
  documento, pincel, imagem, entrada e gráficos devem ficar fora dos widgets.
- Mantenha código específico de plataforma em `src/input/`.
- Campos novos de preferências persistidas devem ter defaults de desserialização
  para manter compatibilidade com configurações existentes.
- Traduções visíveis ao usuário ficam em `src/settings/i18n.rs`, com inglês como
  fallback e chaves estáveis.
- O renderer pode ser OpenGL (`glow`) ou Vulkan via `wgpu`. Efeitos personalizados
  que dependem de OpenGL devem continuar opcionais.

## Adicionar funcionalidades

Separe operações de pixels/documentos da interface sempre que possível. Adicione
testes focados junto da implementação e atualize este mapa quando surgir uma
nova área de nível superior no código.
