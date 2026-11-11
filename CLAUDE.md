# rust-mekanism

## Objetivo: paridade 1:1

Este projeto é um port do Mekanism e do Mekanism Generators: o comportamento tem de ser idêntico ao do mod original, branch
26.3, em lógica, valores, visual, som, telas e interação.

- Nada se decide por simplicidade ou desempenho. Antes de escrever, leia o Java e reproduza o que ele faz, inclusive as
  otimizações, que são parte do comportamento.
- Não existe versão simplificada nem diferença aceitável por conveniência. Medir serve para confirmar paridade, não para
  escolher outro caminho.
- 1:1 inclui os bugs. Um comportamento do original que parece erro (uma pausa que não volta, um modo que cai em outro, um
  arredondamento torto, uma ordem estranha no tick) é portado igual, com teste de referência contra o Java, e não corrigido.
  Registre-o em "Comportamento do original que parece bug e fica", em [docs/plano-paridade.md](docs/plano-paridade.md), com a
  linha do Java, para ninguém "consertar" depois.
- Se o Horizon não permitir algo, tente primeiro uma injeção no jogo. Se nem isso der, pare e pergunte ao usuário. Nunca
  troque por uma aproximação sem autorização, e nunca diga que algo é igual ao original sem ter comparado com o código dele.
- Toda injeção crua (`target = ...`, `[[module]]`, ou hook que dependa de estrutura privada) criada, alterada ou removida atualiza
  [docs/injecoes-horizon.md](docs/injecoes-horizon.md) no mesmo change, com o hook que a substituiria. O documento é a base da issue
  que será aberta no Horizon só ao final do port: ele tem de estar sempre em dia, e a contagem dele tem de bater com os
  `horizon-mixin.toml`.
- O plano e o inventário do que ainda diverge estão em [docs/plano-paridade.md](docs/plano-paridade.md).

## Plano em dia

[docs/plano-paridade.md](docs/plano-paridade.md) reflete sempre o estado real, e é atualizado no mesmo change que o altera.

- O change que porta, corrige ou descobre algo atualiza o inventário: tira, ou marca como feito com a data, o que deixou de divergir;
  acrescenta a divergência nova com a causa; e corrige a linha que ficou velha. Uma linha que diz "sem X" quando X já existe é um erro do plano.
- O trabalho não segue a ordem das fases: parte de uma fase posterior pode sair antes, junto de outra frente (upgrades, abas, luz, som, telas).
  Quando isso acontecer, registre na fase a que a parte pertence o que já está feito e o que falta dela, para a fase não ser refeita nem dada como
  intocada.
- O estado de cada fase (aberta, em andamento, fechada, com a data) fica no começo dela. Uma fase só fecha quando tudo dela cumpre "Pronto significa".
- Decisões do usuário sobre o plano entram em "Decisões", com a data e o motivo.
- Antes de integrar uma frente, releia as linhas do inventário que ela toca e confira cada uma contra o código.

## Testes

O máximo possível roda sem entrar no jogo e sem ninguém interagir. Prefira um teste que emula o jogo tick a tick e compara os
estados, com as regras em código puro e um mundo simulado por trás. A referência dos valores é o Java do original. Teste
manual em mundo fica para o que só o jogo mostra, como visual e som.

## Builds e jogo

Vale para toda sessão e todo subagente, e o prompt de quem compila tem de repetir esta regra.

- No máximo 1 build por vez no notebook. No desktop há 2 vagas no total: um jogo ocupa uma, e um build (8 núcleos e 8 GB) ocupa as duas.
  Jogo só no desktop, nos containers de teste `horizon` e `horizon2`, um cliente por container. (O usuário liberou o desktop em 2026-11-11;
  os limites voltam a descer quando ele disser.)
- O jogo é sempre headless: `gamescope --backend headless -W 1920 -H 1080 -w 1920 -h 1080`, sem `-f`. Nunca `--backend wayland` nem nada que abra
  janela no desktop, que o usuário está usando. Prints e vídeo saem pelo `horizon-ctl`. Nunca mate um gamescope que não seja do teste.
- O jogo de teste também é mudo: todo cliente, do port e do original, manda o áudio para um sink nulo do PipeWire (criado antes, removido ao fim)
  ou roda com o som desligado. Nunca pelas caixas do usuário.
- Notebook: todo `cargo`, clippy e `scripts/ci.sh` passa por `/mnt/dados/builds/rust-mekanism-ws/lim <comando>`, que faz fila com `flock` e
  roda num escopo do systemd com teto de memória e CPU. Nunca chame `cargo` fora dele.
- Desktop: até 8 núcleos e 8 GB por build. Use `~/.cache/rust-mekanism/lim-build <comando>`, que pega as 2 vagas e põe `CARGO_BUILD_JOBS=8`,
  com `--max-workers=8` no gradle. O jogo ocupa uma vaga: antes de abri-lo, rode em background `~/.cache/rust-mekanism/lim-jogo <nome>`, espere
  `jogo-<nome>.ativo` ganhar o número da vaga, e apague esse arquivo ao fechar o jogo. O container nasce com 6 CPUs e 12 GB: antes do
  `native-build`, rode `docker update --cpus 8 --memory 8g --memory-swap 8g <container>`, e antes do jogo
  `docker update --cpus 4 --memory 6g --memory-swap 6g <container>`.
- Um só cache Rust por máquina: no notebook, `lim` aponta todos os workspaces para o `target` do projeto; no desktop, `horizon` e `horizon2`
  compartilham `native/target` e `cargo` por bind mount. Nunca crie outro `target` nem copie esses diretórios.
- Build de debug é apagado quando deixa de ser usado: ao integrar um workspace, remova o workspace e sua pasta de rascunho; a cada `native-build`,
  apague o build de teste anterior, mantendo só o atual e o `native-f319701-76be8ef43ab8`. Ao fim do teste, apague também backups e mundos de teste.
- Se um dos dois scripts sumir, recrie antes de compilar: o conteúdo está em `_rules/um-build-e-um-jogo-por-vez.md`, no ai-memory.

## Onde o código fica

Uma mecânica vai para a lib `machinery_basis` quando outros mods puderem reaproveitá-la ou quando vários blocos diferentes
do Mekanism a usarem: o que se repete dentro do mod provavelmente serve a outros mods. No mod fica só o que é daquele bloco:
valores, receitas, texturas e o layout da tela.

O critério completo está em [docs/machinery-basis.md](docs/machinery-basis.md#regra-de-escopo). A lib continua genérica: ela
não conhece nenhum id, químico ou máquina do Mekanism, que entram como dados ou parâmetros.
