# Projeto de salvamento automático e recuperação

Este documento especifica o contrato desejado antes da implementação. Enquanto
o aplicativo está aberto, os documentos em memória são a fonte de verdade.
Arquivos de recuperação são snapshots descartáveis: não são a única cópia do
projeto e nunca substituem o `.patlas` original.

## Dados recuperados

- Criar snapshot de todas as abas, inclusive inativas, registrando ID da sessão,
  ordem, caminho original (se houver) e estado de modificação.
- Salvar cada documento usando o serializador `.patlas`; manter o manifesto da
  sessão separado dos documentos.
- Restaurar pixels, camadas, grupos, efeitos e metadados. Como o formato atual
  não salva undo/redo, seleção ativa, transformações flutuantes ou visualização,
  a aba recuperada começa sem esses estados e marcada como modificada.
- Recuperar como documento separado e não salvo. Nunca gravar sobre o caminho
  original sem uma ação explícita do usuário.

## Publicação transacional

1. Gravar uma geração completa nova em diretório exclusivo e reabrir cada
   snapshot para validá-lo.
2. Gravar o manifesto por último, com versão, ID da geração, documentos e ordem
   das abas. Validar que os caminhos permaneçam dentro da raiz de recuperação.
3. Sincronizar os arquivos e publicar por rename atômico no mesmo volume,
   preservando a geração válida anterior até a publicação concluir.
4. Em falha de serialização, falta de espaço ou rename, preservar a geração
   anterior, informar o erro e não marcar o documento como salvo.

## Ciclo de vida e retenção

- Usar debounce e snapshots periódicos para documentos modificados; não salvar a
  cada evento do ponteiro. Capturar apenas estados consistentes, após concluir
  ou cancelar gestos transitórios.
- Na inicialização, validar a geração mais recente e perguntar se deve ser
  recuperada. Não restaurar nem descartar em silêncio; preservar dados inválidos
  para diagnóstico e oferecer uma geração anterior válida, se houver.
- Manter ao menos uma geração comprovadamente válida. Limitar retenção por
  quantidade e espaço sem apagar a única recuperação quando um novo snapshot
  falhar.
- Ao encerrar normalmente, remover recuperação somente depois que todas as abas
  modificadas forem salvas ou descartadas explicitamente.
- Impedir que instâncias simultâneas publiquem ou limpem a mesma sessão.

## Requisitos antes de implementar

- Escolher APIs multiplataforma para diretório de dados e locks.
- Validar versão, tamanho, quantidade e caminhos do manifesto antes de alocar ou
  ler conteúdo.
- Testar múltiplas abas, snapshots inválidos, interrupção de publicação,
  falhas de escrita/disco cheio, retenção e cancelamento pelo usuário. Falhas
  não podem modificar o projeto original nem a geração anterior.
- Só adicionar a interface de recuperação após os testes de persistência e
  validação passarem.

## Limites da garantia

O salvamento automático não substitui salvamentos explícitos e não pode
recuperar alterações posteriores ao último snapshot concluído. Até existir
publicação transacional, recuperação validada na inicialização e testes de
falhas de disco, o PaintAtlas não oferece recuperação de falhas.
