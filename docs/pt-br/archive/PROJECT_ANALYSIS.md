# PaintAtlas: avaliação técnica e comparação com Clip Studio Paint

Este relatório é uma avaliação histórica de 23 de outubro de 2026, anterior a
vários recursos atuais. Não é benchmark nem validação física de mesas
digitalizadoras. Consulte o README para o estado atual.

## Diagnóstico histórico

Na época, PaintAtlas era um protótipo raster funcional, com cerca de 2.074 linhas
Rust em cinco arquivos. Permitia desenhar, organizar camadas básicas, desfazer
ações e salvar projetos, mas ainda não tinha o motor de traços, arquitetura de
documentos nem variedade de operações de um editor maduro.

As prioridades recomendadas eram entrada confiável de caneta, qualidade de
traço, memória/histórico escaláveis, seleção/transformação e fluxo completo de
camadas. O objetivo sugerido era um fluxo de ilustração: esboçar, finalizar
linhas, corrigir proporções, preencher em camada separada, sombrear com recorte e
exportar. Vetores avançados, quadrinhos multipágina, animação e 3D ampliariam o
escopo.

## Fundamentos identificados

- Rust com eframe/egui e OpenGL para prévias.
- Uma textura por camada e uploads parciais para regiões alteradas.
- Pixels RGBA, pincel circular, borracha, formas, preenchimento e histórico.
- Camadas com opacidade, visibilidade, ordenação, duplicação e remoção.
- Zoom centrado no cursor, pan e formato nativo versionado.
- Backends de pressão usando XInput2 e evdev no Linux.

## Limitações observadas

### Entrada de caneta

Pressão e posição eram obtidas por caminhos diferentes: a pressão consultava o
estado atual do dispositivo e as posições vinham de eventos egui. Um único valor
de pressão podia ser aplicado a vários pontos do frame, perdendo amostras e
dependendo da taxa de atualização. A recomendação era sincronizar posição,
pressão, timestamp, contato, proximidade, inclinação e identidade do dispositivo,
além de validar com hardware real.

### Histórico, memória e renderização

O histórico então clonava o documento inteiro por checkpoint. Uma camada RGBA
4096×4096 ocupa 64 MiB antes de texturas e buffers temporários. Recomendou-se
armazenamento esparso em tiles e histórico incremental. O pincel rasterizava na
CPU e tinha poucos parâmetros; mudanças de estado podiam recriar texturas. A
proposta era distinguir alterações de pixels, estrutura e metadados, manter IDs
estáveis e usar prévias temporárias.

Também faltavam dinâmica configurável, estabilização, espaçamento controlável e
semântica explícita de fluxo/opacidade.

### Interface, formatos e recuperação

A interface concentrava muitas responsabilidades em poucos módulos, tinha
painéis rígidos e atalhos não configuráveis. O formato nativo não tinha
migrações, recuperação nem salvamento automático. Recomendou-se separar comandos
dos widgets, permitir configurar atalhos e projetar snapshots que nunca
sobrescrevessem o projeto original.

## Prioridades propostas

As áreas sugeridas incluíam entrada confiável; armazenamento/histórico
escaláveis; seleção, transformação, alpha lock e clipping; preenchimento baseado
em lineart de referência; presets persistentes; workspace configurável;
recuperação; e, posteriormente, vetores, ferramentas para quadrinhos e formatos
de intercâmbio.

Uma arquitetura sugerida separava backends de entrada, fila de amostras, motor de
traços, documento tiled, UI/comandos, renderer/cache GPU, histórico e arquivos.
Recomendava-se definir contratos de tiles, seleção e comandos para evitar
implementações duplicadas.

## Validação histórica

Na data do relatório, 16 testes cobriam matemática de pixels, arquivos, regiões
sujas e eventos simulados. Isso não validava pressão de driver, mapeamento
multi-monitor, dial real ou latência ponta a ponta. Faltavam sessões de entrada
gravadas, testes visuais do renderer, matriz de hardware e cenários de
recuperação. Essas observações são históricas e podem não descrever o código
atual.
