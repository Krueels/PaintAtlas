# Documentos com precisão de cor múltipla

## Escopo

O objetivo é selecionar por documento canais RGBA inteiros de **8 bits**, inteiros
de **16 bits** ou ponto flutuante de **32 bits**, preservando a precisão em todo o
fluxo de edição. Todas as camadas usam a precisão escolhida. Valores float além
da faixa de exibição exigem espaço de cor de trabalho, função de transferência,
política de alfa e transformação de exibição definidos. CMYK e conversão ICC não
estão incluídos.

## Situação atual

- Pixels de camada, traços, seleções e máscaras usam buffers RGBA de bytes.
- Composição, pintura, preenchimento, alpha lock, ajustes e filtros operam em
  canais de 8 bits.
- Uploads do canvas e composição fallback na CPU usam pixels de 8 bits.
- `.patlas` v1–v7 armazena pixels em bytes; versões anteriores precisam continuar
  sendo lidas.
- Importação converte para RGBA8 e exportações raster usam caminhos de 8 bits.

Uma alteração somente no armazenamento não basta: todos os caminhos de edição
precisam preservar a precisão ou recusar a operação antes de modificar dados.

## Restrições de projeto

1. Definir representação explícita para RGBA8, RGBA16 e RGBA32 float. Cada pixel
   ocupa 4, 8 ou 16 bytes, respectivamente.
2. Definir espaço de cor, transferência, faixa HDR e regras de alfa antes de
   implementar composição ou pintura em alta precisão.
3. Manter documentos de 8 bits eficientes; prévias 8-bit de GPU são caches
   descartáveis, nunca a fonte de verdade.
4. Não quantizar silenciosamente operações não suportadas.
5. Recalcular limites de documentos, histórico, importação e buffers temporários
   em bytes alocados, incluindo buffers simultâneos.
6. Criar nova versão `.patlas` com precisão explícita, manter leitura de v1–v7
   como RGBA8 e testar promoção/conversão de versões antigas.
7. Começar com importação/exportação sem perdas em formatos que suportem a
   precisão. Redução para formato inferior deve ser explícita e não alterar o
   documento.

## Etapas e critérios

Definir primeiro o contrato de cor e buffers; depois atualizar serialização,
operações centrais, histórico/seleção, filtros, importação/exportação e UI.
Testar round-trip, undo/redo exato, cancelamento, arquivos antigos, valores HDR
quando aplicável e picos de memória. Cada recurso deve preservar a precisão
selecionada ou recusar claramente antes de mutar dados. Não anunciar suporte
completo antes da validação ponta a ponta.
