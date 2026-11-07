# Changelog

Formato baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/).

## [Unreleased]

### Added

- O slot de energia converte `redstone` (10.000) e `redstone_block` (90.000) em energia, como `EnergyInventorySlot.fillContainerOrConvert`
  do original: um item por tick, antes da receita, e só se o contêiner aceita tudo o que a receita dá. As duas receitas
  `energy_conversion` são as da datagen, copiadas byte a byte. A mecânica está na `machinery_basis` (`energy_conversion`:
  `EnergyConversion`, o índice do tipo e `fill_container_or_convert`), sem nenhum id do Mekanism. Nas quatro máquinas portadas o
  buffer tem 20 (no máximo 1.000 com upgrades), então a energia não sobe, igual ao original: o item entra e fica. Rastros
  `chamber_energy_conversion`, `chamber_energy_conversion_running` e `infuser_energy_conversion` do Java, repetidos tick a tick.
- A ordem das receitas de cada tipo foi medida no original (`recipe_order.jsonl`) e é a do `RecipeMap`: caminho do arquivo e depois
  namespace, e não ordem de hash. O Horizon já a entrega assim; `recipe_registration_order.rs` confere as oito listas, o conteúdo do
  port e a receita escolhida para o único item que o original casa com duas receitas (`pitcher_pod`).
- Tags `c:` da datagen do original que o conteúdo já pode usar, copiadas byte a byte: `c:alloys`, `c:circuits` e
  `c:circuits/{advanced,elite,ultimate}` (item) e `c:ores/osmium` (bloco). As outras 143 esperam os itens, blocos e fluidos que
  citam. `c_tags.rs` confere as cópias e que nenhuma tag `c:` cita o que o Horizon descartaria.
- Som das quatro máquinas em operação (`TileEntityMekanism.updateSound` e `SoundHandler.TileTickableSound`): o `sounds.json` e os `.ogg`
  do original, o laço que liga com a máquina e para com ela (com o atraso de `lit`), o volume vezes o fator do upgrade de silêncio
  conferido a cada 20 a 39 ticks, a distância (`attenuation_distance` 16, crescendo com o volume acima de 1), o tempo congelado que
  cala o som, e o som de Natal de 25 de dezembro. A regra está em `machinery_basis_core::sound`, sem id do Mekanism; o jogo entra por
  um módulo (`tile-sound`, H19) e a contagem de upgrades de silêncio vai ao cliente por pacote, como o `sendUpdatePacket` faz.
- Upgrade de silêncio (`muffling`): receita (`recipe/upgrade/muffling.json`) e a tag `mekanism:muffling_center`, do original. Com o
  máximo (1) a máquina não toca; o efeito segue o `getVolumeFactor` e o `isFullyMuffled`, repetidos tick a tick contra o rastro
  `chamber_muffling`.
- Upgrade de contenção de experiência (`xp_containment`): `xpContainmentChanged`, `hadContainment` salvo (`has_containment`),
  `awardUsedRecipesAndPopExperience` ao tirar o resultado, a experiência guardada que cai ao quebrar a máquina ou ao tirar o
  upgrade (a que o Java perde quando o upgrade já saiu), e a experiência da última receita sem o upgrade, como o forno. Rastros
  `chamber_xp_containment`, `chamber_xp_vanilla` e `chamber_xp_broken`, repetidos pelo `effects_trace`.
- Nome da máquina (`TileEntityMekanism.customName`): o item colocado com nome o dá ao tile (`CustomName`, salvo), o item solto leva o nome (`copy_name`), e o servidor manda o nome a quem abre a tela, que o mostra como título. A regra está em `machinery_basis_core::name`, repetida contra o rastro `chamber_name`.
- Tooltip do item de upgrade (tipo, máximo instalado e descrição, nas cores do original), conferido linha a linha contra o rastro
  `upgrade_tooltip`.

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

### Fixed

- O hopper pelas costas de uma máquina (`DataType.ENERGY`, `InventorySlotInfo(true, true, ..)`) põe no slot de energia o que tem
  receita `energy_conversion` (a `redstone`), como o original; antes as costas não alcançavam slot nenhum. O shift-click de um
  `redstone` ou `redstone_block` com o menu aberto também o aceita no slot de energia, pela mesma regra
  (`InventoryContainerSlot.mayPlace` chama `isValidForInsertion(.., MANUAL)`).
- Um hopper embaixo do Metallurgic Infuser tira do slot de infusão o que o tanque não aceita nem converte (`DataType.EXTRA` da base,
  `InventorySlotInfo(true, true, ..)`, e o `canExtract` de `ChemicalInventorySlot.fillOrConvert`: `!external || !canFillOrConvert`):
  um graveto, ou o bio fuel com o tanque cheio de outro químico. O que o tanque converte fica. Nas outras três máquinas a base é
  `DataType.INPUT`, que não deixa tirar, então o hopper de baixo não leva nada (nem a entrada nem a saída, que fica à direita), como
  no original. Rastro do Java (`slots_*.jsonl`, `SlotsTraceTest.java`) e `slot_rules.rs` conferem as seis faces das quatro máquinas.
- Os slots de infusão e de energia recusam, para quem está de fora (`AutomationType.EXTERNAL`) ou à mão (`MANUAL`), o item que não
  convertem, e entregam a quem está de fora só o que não convertem (`fillOrConvert` de `ChemicalInventorySlot` e de
  `EnergyInventorySlot`: tiram com `!external || !canFillOrConvert(item)` e inserem com `internal || canFillOrConvert(item)`). A lib usava
  `Any`. As 56 perguntas do slot de infusão e as 7 do de energia, por máquina, batem com o original.
- A tag `c:crops/melon` lista `minecraft:melon`, como a do NeoForge, e não a fatia: com a fatia, o Crusher achava duas receitas para
  `melon_slice` (`crushing/biofuel/melon` e `crushing/biofuel/melon_slice`) e dava 6 de bio fuel em vez de 1. O original acha só a da
  fatia (`crusher_melon.jsonl`), e em jogo o port agora dá 1.

- O Heat Generator perde calor para o ar de cada face com a temperatura do bloco vizinho, como `simulateEnvironment` (antes usava a do
  próprio bloco nas seis), e a troca de calor menor que `HeatAPI.EPSILON` é ignorada com o valor do original (`1e-6`, antes `1e-5`).
  Os rastros do Java (`heat_generator_*`) e `heat_trace.rs` confirmam o calor, a energia e a lava tick a tick. Conferido em jogo com
  `fillbiome` em três vizinhos: o `MEKTRACE` leu 262,5, 280 e 330 K nas faces de baixo, sul e leste.
- O bloco do Heat Generator segue aceso por `blockDeactivationDelay` (60 ticks) depois que o gerador para, como `setActive(false)` do
  `TileEntityMekanism`: o desligamento sem atraso armado apaga na hora, e ligar é sempre instantâneo. `active_state` e `update_delay`
  são salvos como no Java. `heat_trace.rs` compara o `block_active` dos três rastros tick a tick.
- O botão Dump do Infuser esvazia o tanque na tela na hora, mesmo com o servidor parado (`tick freeze`): o pacote `DUMP_BUTTON` do
  original age quando chega e o menu sincroniza o tanque no mesmo instante. Antes a tela só mudava no tick seguinte do bloco.
- A mistura dos quadros de sprites animados e interpolados nas telas (químicos do Infuser, lava do medidor) arredonda os empates para
  baixo, como o `animate_sprite_interpolate.fsh` na GPU: injeção `sprite_ties` em `HudRenderer::animate_sprites`. Medido contra o
  original, prints do ciclo com um estado idêntico no port, antes e depois: `base` 18 e 40 de 40, `bio` 10 e 16, `fungi` 6 e 21, lava 40 e 40
  (ver `docs/injecoes-horizon.md`, H26).
- A receita que cada máquina escolhe segue o índice de receitas do Mekanism (`InputRecipeCache`), não a ordem da lista: com mais de
  uma receita para o mesmo item, vale a que o mapa por item devolve, e as de ingrediente complexo vêm depois. A Enrichment Chamber, o
  Crusher e o Energized Smelter só pegam uma receita quando a pilha tem a quantidade que ela pede (antes olhavam só o tipo). O slot
  de entrada e o tanque do Infuser, e o hopper vizinho, usam as regras do original (`containsRecipe`, `containsRecipeAB`/`A`,
  `containsRecipeBA`/`B`). O slot de infusão só aceita o item cuja conversão o tanque aceita (`canFillOrConvert`), e a conversão
  escolhida é a do índice. O Infuser usa a receita de uso constante quando a receita tem `per_tick_usage`; antes a receita era
  descartada com um aviso. Os arquivos do original não a usam, então o jogo normal não muda.
- O slot de entrada da Enrichment Chamber, do Crusher, do Energized Smelter e do Metallurgic Infuser não deixa mais o hopper ou o
  cano tirar o item (`InputInventorySlot` extrai com `notExternal()`); o jogador e a máquina continuam tirando.
- Cogumelos e fungos viram `fungi` no slot de infusão do Infuser: o `neoforge:or` dentro de outro `neoforge:or` perdia os filhos e a
  receita `fungi/from_mushrooms` sumia. E `bio_fuel` vira `bio`: faltava a tag `c:fuels/bio`.
- O convidado de LAN ou de servidor dedicado vê a janela de upgrades completa: os tipos e as tags de upgrade vão do servidor ao cliente
  por `native_net_send_to_player` quando o jogador entra e a cada `/reload`. Antes a janela aparecia vazia, sem lista nem "Supported:".
- O progresso de instalação da janela de upgrades mostra o valor de um tick atrás, como o cliente do original.

### Added

- O `MEKTRACE` do Heat Generator (`trace_line`, ligado por `MEDIR` como o das máquinas), com os campos dos rastros `heat_generator_*`;
  `heat_trace.rs` confere que a linha dele é igual ao que o replay calcula. `cena_telas.py` ganha `generator_queima` (100 passos
  queimando a lava) e `CenaTelas.java` registra a energia, a lava, o calor e o tanque do Infuser no log (`cena:`), para comparar o
  servidor do original com o `MEKTRACE` do port. Conferido: energia 20, lava 0 e temperatura 300,3102685960236 iguais nos dois
  depois de 100 passos, e 0 pixel de diferença na barra de energia.
- `machinery_basis`: `ItemStackConstantChemicalToObjectCachedRecipe` e o `usedSoFar` que ela muda (receita que gasta o químico a cada
  tick), os handlers de entrada e de saída de fluido e de químico, o contêiner de recurso grande genérico (`ChemicalTank` e `FluidTank`),
  `fillTankFromSlot` para item com capability de químico e o `ChemicalAttributeValidator`. Conferido tick a tick com o Osmium
  Compressor original (`compressor_run`). Nenhuma máquina do jogo usa ainda: o comportamento visível é o mesmo.
- A aba de upgrades e a janela dela nas quatro máquinas (Enrichment Chamber, Crusher, Energized Smelter e Metallurgic Infuser), como
  `GuiUpgradeWindowTab` e `GuiUpgradeWindow`: a aba na lateral direita; a janela arrastável pela faixa de cima, com os botões de fechar e
  de fixar, a lista dos upgrades instalados com barra de rolagem, a faixa "Supported:", a tela interna com nome, quantidade e efeito, os
  progressos de instalação e de remoção, o botão Uninstall (shift remove todos) e os dois slots de upgrade. Esc fecha a janela que não
  está fixa; a posição e o estado fixo persistem em `config/mekanism/client.toml`. Os slots de upgrade ficam no fim do menu, o
  shift-clique segue o `quickMoveStack` (o upgrade só entra com a janela aberta, e sem ela vai para a barra de atalhos), e o servidor
  guarda a janela escolhida por jogador. Os textos `gui.mekanism.upgrades*`, os sprites e os quatro tipos de upgrade que faltavam (anchor,
  chemical, filter, stone_generator) têm o texto, a textura e o modelo do original. `data_slots` passa de 8 para 17 e de 28 para 37, e
  `slots` de 3 para 5 e de 4 para 6.
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
