# Changelog

Formato baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/).

## [Unreleased]

### Changed

- As telas das quatro máquinas e do Heat Generator seguem os elementos do Java, em vez de aproximações: a barra de energia é `bar/base`
  (nove partes, repetidas) com `bar/vertical_power` em mosaico e `calculateSize` (pelo menos um pixel); a barra de químico do
  Infuser desenha o sprite animado do químico, multiplicado pelo `tint` do arquivo e não pela cor de representação; os slots têm a
  moldura do papel que a configuração de lados dá a eles (entrada vermelha, saída azul, extra amarelo, energia verde) e o raio por
  cima do slot de energia; o rótulo "Inventory" aparece, e título e rótulo usam `TEXT_TITLE` sem sombra; o fundo e as molduras usam o
  `gui.scaling` dos `.mcmeta` do original; a seta de progresso se enche por `ticksRequired`, que muda com upgrades de velocidade; o
  botão Dump também responde ao clique direito. Os tooltips das barras e do medidor ficam por cima ou por baixo do widget, e dizem
  `<energia>/<capacidade>` em FE (`1.5 kFE/8 kFE`) e `<nome>: <quantidade> mB` ou `Empty`, em vez de `kJ` e `N / 1000 mB`.
- A tela do Heat Generator tem o medidor largo de lava (`GuiFluidGauge`) e a barra de energia do original, e os slots no lugar certo
  (um pixel de diferença). O painel de texto com energia, combustível e temperatura, que não existe no original, saiu.
- Os data slots das telas ganharam `ticksRequired` (as quatro máquinas) e, no Infuser, o `tint`, o sprite e o id do químico:
  `data_slots` passa de 7 para 8 e de 10 para 28.

- O sprite do químico do Infuser vem do atlas de blocos, como no original: a tira fica em `textures/mek_chemical` com o
  `assets/minecraft/atlases/blocks.json` dele, e o HUD do Horizon a anima, sem desenho próprio.
- O tooltip de widget (barra, medidor, aba de avisos) usa os sprites `tooltip/background` e `tooltip/frame` em nove partes, com os cantos
  do 26.x, e não o desenho de retângulos do Horizon.
- O realce da frente do slot sob o cursor, nas telas do Mekanism e do Generators, usa o sprite de trás, como o original
  (`VirtualSlotContainerScreen.extractSlotHighlightFront` chama o `Back`). Uma injeção em `draw_slot_highlight`.

### Added

- `dev/referencia/cena_telas.py`, `comparar_lote.py` e `testes/CenaTelas.java`: os mesmos estados de tela (vazia, em operação, barra
  cheia e a meio, aviso, tanque, hover) montados no original e no port, e a comparação deles por elemento.
- `dev/referencia/comparar_tela.py`: compara dois prints da tela de uma máquina, no original e no port, com o mesmo `guiScale`.
  Recorta a tela, tira da conta as abas que o port não tem e conta os pixels diferentes por elemento, com uma imagem de diferença. O
  alvo é zero. Testes em `dev/referencia/test_comparar_tela.py`, que a `scripts/ci.sh` roda.
- Avisos na tela das quatro máquinas, como no `GuiWarningTab`: a aba amarela à esquerda (x = -26, y = 109) aparece quando há aviso e
  o tooltip dela lista `Issues:` em `EnumColor.YELLOW` e uma linha por aviso, cortada em 170 pixels, posicionado pelo
  `MenuTooltipPositioner`. A barra de energia, a seta de progresso e os slots de entrada e de saída mostram o aviso do erro de cada um
  (`warning_background` e `warning` em mosaico, o sprite `warning` da seta e do slot). A tela recebe a máscara de erros e o redstone
  (`controlType`, `redstone`, `redstoneLastTick`) em dois data slots a mais. O desenho do progresso segue o `GuiProgress`
  (pelo menos um pixel, e a moldura de um pixel da barra). Não há aba de redstone na tela ainda, então o aviso dela não aparece no
  elemento, só na lista da aba. A tela e as regras conferidas com os rastros do original e, em imagem, com o cliente oficial.
- Químicos e upgrades ficam nos caminhos e no formato do original: um datapack feito para o Mekanism vale aqui.
- Upgrades das máquinas do Mekanism: o item `mekanism:upgrade` com o componente `mekanism:upgrade_type`, os tipos de velocidade,
  energia, abafamento e contenção de experiência (os que a tag `default_machine` lista) com os máximos e as tags originais, as receitas
  de velocidade, energia e contenção de experiência, as texturas e os nomes. Shift e clique direito com o upgrade na mão instala nas
  quatro máquinas, como o `ItemUpgrade.useOn`. Tempo, consumo e capacidade seguem o que está instalado, também depois de reabrir o
  mundo. Os slots de entrada e de saída e a instalação em 20 ticks existem e estão testados, mas ainda não aparecem na tela.
- `upgrade.rs` na `machinery_basis`: o `TileComponentUpgrade` (`tickServer`, `addUpgrades`, `removeUpgrade`, `serialize` e
  `deserialize`), os dois `UpgradeInventorySlot` e os tipos e tags de upgrade lidos de dados. Conferido tick a tick com o rastro
  `chamber_upgrade_slot` do original, e com um teste de salvar e reabrir com upgrades instalados e nos slots.
- A capacidade de energia de uma máquina é a da instância, e não a do bloco: uma injeção em `native_energy_at` responde com a que os
  upgrades dão. A tela passa a ler a capacidade dos data slots.
- As máquinas do Mekanism dão sinal a comparadores, pelo quanto os slots estão cheios.
- Metallurgic Infuser na base de máquina: tanque de químico com diário e listener (`ChemicalTank`), `TwoInputCachedRecipe`
  de item e químico, `fillTankOrConvert` e a `ProgressMachine`, que as máquinas elétricas já usavam e agora serve também a
  máquina com tanque. Conferido tick a tick com o rastro do Infuser original (`infuser_run`), com teste de salvar e reabrir.
  O `MEKTRACE` ganha `tank` e `infusion`. Fora de escopo: `ItemStackConstantChemicalToObjectCachedRecipe` e o `usedSoFar`
  que ela muda, porque as 19 receitas de infusão têm `per_tick_usage: false`. A tag `used_so_far` é lida e gravada como no Java.
- O ejetor respeita a regra de cada slot do vizinho: pela lateral, um forno só recebe combustível.
- Base de máquina da `machinery_basis` portada do Mekanism como objetos vivos: transações
  com diários que se registram sozinhos, contêineres compartilhados, `CachedRecipe` com closures, monitor de receita, tile e
  o ejetor de itens numa transação que cobre a máquina e o baú, conferidos tick a tick com a Enrichment Chamber original.
- Enrichment Chamber, Crusher, Energized Smelter e Metallurgic Infuser ligados à base nova no jogo, com `MEKTRACE` e
  `MEKBENCH` atrás de `MEDIR`. Ejetor de itens (`/data merge block x y z {mekanism_eject:1}`, saída à direita), redstone, comparador e o
  NBT que o Java persiste (redstone, modo, estado ativo, atraso, nível do comparador, progresso, receitas usadas).
- Regras de hopper da `machinery_basis`: a máquina entrega ao hopper abaixo e recebe do hopper que aponta para ela,
  respeitando as faces configuradas, a 1 item por 8 ticks.
- Recursos tipados declarados por dados e tanques da `machinery_basis`.
- Bundle da `machinery_basis`: tanques salvos no NBT, registro de recursos, distribuição de energia, regra de hoppers e
  ejetor de itens.
- Heat Generator jogável, com tela, e minério de ósmio gerado no mundo.
- Metallurgic Infuser jogável, com tela, as receitas de infusão originais e os químicos de infusão, e os itens do aço, do
  circuito básico e da liga infundida.
- Steel Casing, Enrichment Chamber, Crusher e Energized Smelter jogáveis, com as receitas de enriquecimento e trituração
  originais, os pós, os itens enriquecidos e os circuitos e ligas que eles destravam.
- Universal Cable nos quatro tiers: leva energia dos geradores às máquinas, dividindo por igual entre quem consome.
- Rede de energia na `machinery_basis`, com a travessia das ligações e a divisão justa.
- Máquinas de progresso na `machinery_basis`: receitas de item e recurso, conversão de item em recurso, tags de recursos e
  hoppers que alcançam slots diferentes por face.
- Lava dentro do bloco na `machinery_basis`: um bloco com a propriedade `fluid_logged` recebe lava com shift + balde, e ela
  corre como lava real, ilumina e fica no lugar quando o bloco quebra.

### Changed

- O tanque do Metallurgic Infuser deixa o formato `machinery_basis.tanks` e passa ao do Java: `chemical_tanks`, uma lista de
  `{container, stored: {type, amount}}`, sem a lista quando está vazio. O que já estava salvo no formato anterior é levado ao
  novo no primeiro tick do bloco. O Heat Generator segue no formato antigo.
- O Infuser ganha o ejetor de itens (`mekanism_eject`), o controle de redstone, o comparador e o atraso para apagar da base
  nova, no lugar do progresso próprio de antes. `dump()` esvazia o tanque como `dumpOrClearContents`.

- O `ItemResource` da lib ganha `variant`, o que distingue itens de mesmo id (os componentes), e o `InventorySlot` ganha o
  `validator`, o item no `canInsert` e `Access::ManualOnly`.

### Removed

- `ProgressMachine::set_upgrades`, que punha os totais direto: os upgrades entram pelo componente.
- `machine::step` e `run_machine`, que só o Infuser ainda usava, e a conversão de item em tanque do código antigo.
