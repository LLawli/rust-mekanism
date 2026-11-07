# Changelog

Formato baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/).

## [Unreleased]

### Added

- O clique do cursor, a tecla numérica e o arrasto num menu de máquina respeitam as regras do slot, como o `InventoryContainerSlot.mayPlace`
  do original (`isValidForInsertion(item, MANUAL)`): terra não entra mais no slot de infusão do Infuser, nem o item que não é receita na entrada,
  nem nada na saída, e o combustível do Heat Generator é o que queima. A regra olha o tanque e a entrada, então o mod a deixa pronta antes de cada
  clique (`machinery_basis_core::slot_rules`, uma regra por dono) e o crate `[api]` da `machinery_basis` responde ao hook `menu_slot_rules` do
  Horizon com ela. `rules::slot_accepts` é a regra que o hopper, o shift-click e o cursor partilham. Rastro do Java `slot_click_<máquina>.jsonl`
  (`SlotClickTraceTest`, as quatro máquinas e o Heat Generator, 15 itens, quatro cliques, seis estados no Infuser), repetido por `slot_click.rs`
  e, em jogo, por `h2_cursor.py`. Uma injeção crua nova, `menu_click_before`, no Generators (veja `docs/injecoes-horizon.md`).
- Rastros do Java da capacidade de energia, do consumo e do tempo de operação de uma máquina posta por um jogador, pelo gabarito e lida do disco
  (`placed_<máquina>.jsonl`, `placed_run_*`; `PlacedTraceTest`), repetidos por `placed_capacity.rs` e `placed_run.rs`.
- Rastro do Java para a chegada dos itens no Metallurgic Infuser (`infuser_arrival.jsonl`, `ArrivalTraceTest`): com o tanque vazio, o item de entrada de uma
  receita e o item que vira o químico dela chegam como um jogador os põe, em três ordens (a entrada primeiro, a conversão primeiro,
  os dois no mesmo tick), em 13 pares (bio, fungi, redstone, diamond, gold e refined_obsidian). `infuser_arrival_trace.rs` o repete
  tick a tick com as receitas e as conversões lidas dos arquivos do port e confere quanto cada slot aceitou; `h2_infuser_arrival.py`
  o repete no jogo. O port segue o original nas três ordens: o slot de infusão aceita o item de conversão com a terra na entrada e o
  tanque vazio, converte no tick seguinte e a receita anda. O que parecia um defeito (terra na entrada e `bio_fuel` na infusão
  "ao mesmo tempo" não converte) vinha de como o cenário foi montado: `container X Y Z item n` põe no primeiro slot livre, que no
  Infuser é o de infusão, e `data merge ... Items` fora da ordem dos slots perde itens.
- A cena da argila no rastro `identity_heat_generator` (`clay_ball_player_fuel_empty`): um item que não queima, com o slot de
  combustível vazio, fica no inventário. Sem ela o combustível que aceita tudo passava no teste.
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
- Um item é o item e os componentes dele (`ItemResource` do NeoForge, `ItemStack.isSameItemSameComponents`), e não só o id: o slot,
  o hopper, o shift-clique e o ejetor só juntam pilhas do mesmo recurso. `clay` com nome A não empilha com `clay` com nome B, com
  `clay` sem nome nem com dados diferentes, e o hopper segue para o item seguinte que o slot aceita. A mecânica está na
  `machinery_basis` (`Components`, `ItemResource.components`, `room_for`, `hopper::first_taker`), sem nenhum id do Mekanism, e vale
  nas quatro máquinas e no Heat Generator. O tipo do upgrade passou a ser um componente comum (`mekanism:upgrade_type`) e o NBT dos
  slots de upgrade grava o `components` inteiro. Rastros `identity_<máquina>.jsonl` do Java (hopper de cima, tick a tick, e
  shift-clique nos dois sentidos), repetidos pelos testes e em jogo.
- O ejetor alcança a composteira, como o `ComposterWrapper` do NeoForge: pela face de cima dela (a máquina ejetando para baixo) o item
  compostável entra de um em um, sorteia as camadas uma vez por tentativa, recusa se o sorteio dá 0 sem gastar o item, e sobe o nível
  em um, com o tick agendado ao chegar a 7 e o evento de enchimento. Pela face de baixo e pelas laterais nada entra, e a lateral, que é
  a saída padrão do item, nunca chega a uma composteira. Mecânica em `machinery_basis_core::composter`; o sorteio e a subida de nível
  vêm do módulo `composter-wrapper` (H29). Rastro `composter_enrichment_chamber.jsonl` do Java, repetido tick a tick nos testes e
  em jogo. Enquanto a configuração de lados não existe, a chave `mekanism_eject_face` (`/data merge block ... {mekanism_eject_face:"down"}`)
  escolhe a face de saída do ejetor, como `mekanism_eject` o liga.
- O shift-clique do Heat Generator segue o `MekanismContainer.quickMoveStack`, como nas quatro máquinas: o item que o slot de combustível
  recusa vai para a barra de atalhos e não para o slot de energia, e o combustível só junta com o do mesmo item e componentes. Rastro
  `identity_heat_generator.jsonl` do Java (`quick_*`), repetido pelos testes e em jogo. Uma injeção nova, `menu_quick_move` de
  `mekanismgenerators`; o slot de energia (`EnergyInventorySlot.drain`) não aceita nada porque nenhum item portado tem energia.
- Aba de XP (`GuiXpContainmentTab`) nas quatro máquinas: aparece com o upgrade `xp_containment` instalado, na lateral direita (176, 90), com o
  suporte tingido de `0xFF80FF20`, o botão de 18 por 18, o ícone `tab/xp_containment`, o foco do botão e o tooltip "Collect all stored
  experience.". O clique esquerdo coleta a experiência guardada (`COLLECT_XP`: as bolas nascem no jogador, as receitas guardadas zeram),
  e só com o upgrade. A janela de upgrades por cima do botão o esconde do mouse. O servidor manda `hasXpContainment` por um data slot a
  mais. A mecânica genérica (uma aba inset: geometria, fundo do botão, clique, foco) está em `machinery_basis_core::side_tab` e o desenho
  em `machinery_basis::inset_tab`, sem id do Mekanism. Rastro `xp_tab` do cliente oficial, repetido por `xp_tab.rs`, e pixel contra o
  oficial em 21 estados (veja `dev/referencia/README.md`).
- O nome da máquina (`CustomName`) guarda o componente inteiro: cor, negrito, itálico e filhos passam do item ao tile, ao NBT salvo, ao
  título da tela (desenhado com o estilo e centrado pela largura dele) e ao item solto. `TileName<C>` não lê o componente. Rastro
  `chamber_name_styled`, repetido por `name_trace.rs`.
- O Heat Generator tem nome, como o original (`nameable`): o item o dá ao tile, o título da tela o desenha com o estilo e o item solto o leva.
  A lógica foi para a `machinery_basis` (`name_placed`, `name_loot`, `name_container_opened`, `name_destroying`, `screen_title`) e serve
  aos dois mods; o Generators ganhou os hooks `place_block`, `loot_drops`, `container_opened` e `destroy_block`. Rastro
  `heat_generator_name_styled`.
- Aba de energia (`GuiEnergyTab`) nas quatro máquinas e no Heat Generator, em (-26, 137), com o tooltip `Using: N/t`, `Needed: N` e `Unit: FE`
  (nas máquinas) ou `Producing: N/t` e `Unit: FE` (no gerador). O consumo por tick vai ao cliente em dois data slots novos. A mecânica está em
  `machinery_basis_core` (`side_tab::InfoTab`, `units::energy_info_lines`), sem id do Mekanism.
- A experiência guardada também sai quando uma explosão leva a máquina (`preRemoveSideEffects` roda em toda remoção), pelo hook
  `explosion_detonate`. Rastro `chamber_explosion`, repetido por `effects_trace.rs`.
- Coletar a experiência (a aba de XP) e tirar o resultado desbloqueiam as receitas usadas no livro do jogador que agiu e andam o critério
  `recipe_crafted` (`ServerPlayer.awardRecipes` e `triggerRecipeCrafted`), como o `awardUsedRecipesAndPopExperience` do original.
- O comparador de telas conta como elementos do mod a aba de energia, a de calor, os slots de armadura e a aba de upgrades do Heat Generator,
  que antes ficavam escondidos no mundo atrás da tela, e dá tolerância de 16 níveis por canal só ao tooltip.

### Changed

- A capacidade de energia de uma máquina depende de como ela chegou ao mundo, como no original: 1.000 (1.600 no Crusher) se um jogador a põe
  (`applyImplicitComponents` recalcula os upgrades), 20 (32) se vem de `setblock` ou de uma estrutura, e 20 de novo se o tile é lido do disco sem upgrade
  nem item no slot de upgrade (o `component_upgrade` não existe no NBT, e `ITileComponent.read` só chama `deserialize` com o filho). Com upgrades a
  capacidade volta do disco. Antes toda máquina nascia com 1.000. Uma máquina que o port deixa de andar por 5 segundos e volta mantém a capacidade que tinha (o tile do Java continua o mesmo; só conferido pelo código).
  A `redstone` do slot de energia converte onde a capacidade chega a 10.000 (velocidade 8 na Chamber, no Smelter e no Infuser; 7 e 8 no Crusher), e o
  `redstone_block` nunca; o texto antigo dizia que nunca convertia e que a capacidade ia a 1.000.
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

- O item solto de uma máquina quebrada pelo jogador leva o nome dela: o Horizon tira o tile do mundo antes de rolar a tabela de loot, e o
  nome é guardado em `destroy_block`, antes disso. Medido em jogo: sem isso o `copy_name` não achava o tile e o item saía sem nome.
- O hopper levava para a entrada da máquina o item do mesmo id mesmo com componentes diferentes (somava `clay` de nome B à pilha de
  `clay` de nome A), e o slot da lib e o shift-clique tratavam o mesmo id como o mesmo item. Agora comparam o item e os componentes.
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
