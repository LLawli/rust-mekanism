# Changelog

Formato baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/).

## [Unreleased]

### Added

- O ar que o Heat Generator lê passa a ser o do original: `Biome.getTemperature(pos, seaLevel)` com o ajuste por altura (acima de `seaLevel + 17` a temperatura cai com a altura e com um ruído
  por posição de semente fixa) e o modificador `frozen` dos oceanos congelados, em vez da temperatura base do bioma; o bioma de cada bloco vem do `BiomeManager.getBiome` (o quadrado mais próximo com
  sorteio) e o nível do mar da dimensão (63, 32, 0 e -63 num mundo plano). O ar de cada face e o do bloco ficam guardados enquanto o tile vive (`CachedAmbientTemperature`): um bioma que muda
  depois não muda o gerador até o chunk descarregar e carregar. A regra está na `machinery_basis` (`biome_temperature`, `heat::AmbientCache`), igual ao bit contra cinco rastros do Java
  (`heat_ambient_{table,height,cache,game_desert,game_frozen}`: 2.880 temperaturas de bioma, 1.800 posições do modificador `frozen` e 12 geradores de 160 ticks), e em jogo os geradores de seis
  alturas, de 6 a 300, dão o calor e a energia do Java tick a tick.
- O mundo simulado (`sim::chunks`) tem as notificações de vizinhança do `setBlock`: a flag 1 avisa os seis vizinhos em `NeighborUpdater.UPDATE_ORDER`, `updateNeighbourForOutputSignal` manda o
  `onNeighborChange` do NeoForge em `Level.NEIGHBOR_UPDATE_LIST`, o bloco posto por jogador roda `setPlacedBy`, e um tile sai do chunk e volta do NBT (`detach_block_entity`, `attach_block_entity`).
  `sim::neighbor_replay` repete os rastros `neighbors_*` do Java, tick a tick e no meio do tick, e `machinery_basis_core::neighbors` guarda as duas ordens, as flags e o sinal que um aviso deixa
  para o tile.
- O Heat Generator tem os slots de armadura e de mão secundária (`armorSideBar(-20, 11, 0)`, `GeneratorsContainerTypes.java:29`): quatro `ArmorSlot` em
  (-20, 11 + 18 i), a cabeça em cima, e o `OffhandSlot` em (-20, 83), sobre o suporte `GuiSideHolder.create(this, -26, 6, 98, true, true, TAB_ARMOR_SLOTS)`
  (`holder_left`). São os slots 40 a 44 do menu do servidor, que o motor verificado do Horizon já sabe tratar: o capacete entra no slot da cabeça e é recusado no das
  botas, uma pilha de 2 capacetes entra 1, a mão aceita qualquer item, a peça com maldição do vínculo não sai em sobrevivência (clique nem shift-clique), e o clique
  com o cursor equipa pelo `onEquipItem` do motor (o evento de jogo, medido com um sensor de sculk, e o som, que sai da mesma chamada) e o shift-clique não. O shift-clique segue o `MekanismContainer.quickMoveStack`: do inventário vai ao gerador
  (combustível primeiro, mesmo da mão), depois à armadura (só slots vazios) e ao outro grupo; da armadura ou da mão vai à barra de atalhos e ao inventário. A regra
  está na `machinery_basis` (`armor_slots` e `armor_bar`: o registro por tipo de menu, a geometria, o mapa dos índices e os retângulos de elementos da tela) e o
  Generators só declara a barra e as duas abas e desenha o suporte. Nove injeções cruas e cinco módulos novos na `machinery_basis` (veja
  `docs/injecoes-horizon.md`, "A barra de armadura e mão").
- O Heat Generator tem a aba de calor (`GuiHeatTab`, em (-26, 109)): o tooltip mostra `Temp`, `Transferred` e `Dissipated` (`lastTransferLoss` e
  `lastEnvironmentLoss`, que `TileEntityHeatGenerator.simulate` guarda e o contêiner sincroniza como dois `SyncableDouble`, aqui oito data slots) e a
  unidade; o clique esquerdo avança e o direito volta a unidade de temperatura do cliente (`MekanismConfig.local.tempUnit`: K, C, R, F e STP, nessa ordem,
  gravada em `config/mekanism/local.toml`), e o sprite da aba segue a unidade (`tab/heat_info_<k|c|r|f|stp>`, os cinco PNG que faltavam). As conversões
  (`UnitDisplayUtils.TemperatureUnit`, `MekanismUtils.getTemperatureDisplay`, `getDisplayBase` de femto a yotta) estão na `machinery_basis`
  (`temperature`, `units`, `data_slots::split_f64`), conferidas contra o rastro do Java `temperature_units.jsonl` (230 combinações de display e 25 listas
  de tooltip, repetidas por `heat_tab.rs`) e `heat_generator_*` (as duas perdas, tick a tick, bit a bit). O clique é o hook `screen_mouse_button_pressed_pre`
  (sem som e sem pacote, como o `GuiHeatTab`). `lastTransferLoss` vale zero: o port não tem bloco com a capability de calor para a troca pela base.
- O Heat Generator tem a aba e a janela de upgrades (o gerador aceita só o de silêncio, `AttributeUpgradeSupport.MUFFLING_ONLY`): dois slots virtuais no
  menu (entrada e saída, com a regra do `UpgradeInventorySlot`), o `TileComponentUpgrade` refeito do NBT a cada tick (`upgrade::rebuild`,
  `UpgradeTransient` para o `upgradeTicks` e o `canCheckUpgrades`, que o Java não salva), a remoção por pacote, o shift-clique e o
  `ItemUpgrade.useOn`. Tudo o que não é do gerador foi para a `machinery_basis` (leitura e escrita do `component_upgrade`, os slots virtuais, o `useOn`, o
  hook `window_viewers`), e as máquinas do Mekanism usam o mesmo código. Seis rastros do Java (`heat_generator_upgrades`, `_upgrade_slot`,
  `_upgrade_slot_quick_move`, `_upgrade_window_install`, `_upgrade_remove_by_packet`, `_container_tracking`), repetidos por `upgrade_window.rs`.
  O som do gerador não existe no port, então o silêncio só conta a contagem; os upgrades instalados não vão no item solto ao quebrar (como nas máquinas).
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
- O comparador de telas ganhou tetos medidos, e não tem mais limite fixo para o mundo: a barra de químico e o medidor de lava valem só até o que o lote mediu
  (um nível por canal e os pixels do pior ciclo), sem o "pior custo do próprio ciclo"; o ícone da lista de upgrades em guiScale 3 só vale se o mesmo estado deu o
  ícone idêntico em guiScale 2; o mundo atrás da tela tem o limite do nível de luz do bloco (0, 8 ou 13) e, dentro dele, sai no relatório como `NE` (não
  explicado, H30), não como `ok`; as partículas da máquina acesa saem como diferença conhecida e contada (`Machine.java:23`), com a causa. Os controles da luz
  (`minecraft:light` de nível 4, 8, 12 e 15, com e sem tela) e as opções gráficas do original aplicadas ao Horizon (`OPCOES_DO_ORIGINAL`, conferidas com
  `options-get`) fazem parte do roteiro. O medidor de lava dos casos de tela se explica pelo ciclo do mesmo nível, com o cursor ou sem ele (`generator_lava_ciclo`,
  `generator_fase_lava_1000`, `generator_fase_lava_hover`): 0 pixel e 0 níveis, e esses casos têm a energia cheia, para a lava não baixar (a lava queimando segue no caso
  `generator_queima` e no rastro `heat_generator_lava_run`). Os slots de armadura do Heat Generator seguem `DIF`, com a causa "não portado" no relatório. As partículas do
  caso anterior de um caso aceso também são diferença conhecida. 58 mutações do comparador, todas pegas por testes (`telas-tools/mut_py6.py`).
- Injeção crua `container_dim` (`machinery_basis`): o fundo escurecido das telas de contêiner é o degradê do original (`0xC0101010` a `0xD0101010`), no
  lugar do retângulo fixo do Horizon (H22 em `docs/injecoes-horizon.md`).

- O Heat Generator tem o sinal do comparador, como no original (`Generator` adiciona `Attributes.COMPARATOR`; `TileEntityHeatGenerator.getRedstoneLevel` lê o tanque de lava,
  `ContainerType.FLUID.getRedstoneSignalFromContainer`, e só o tanque marca o comparador como sujo): o nível (0 a 15, `Mth.lerpDiscrete`) é guardado em `current_redstone`, relido no fim
  do tick em que o tanque mudou, e os vizinhos são avisados dentro do tick. Dois hooks novos em `mekanismgenerators` (`has_analog_output` e `analog_output`).
  Rastro `comparator_levels_heat_generator` (lava de 0 a 10.000), repetido tick a tick por `comparator_levels.rs`.

### Changed

- A tela das cinco máquinas (as quatro de progresso e o Heat Generator) recebe os dados do servidor como o `MekanismContainer` do original: um pacote por jogador com só o que mudou
  (`PacketUpdateContainer`), lido por um espelho do menu no cliente, no lugar dos data slots de 16 bits. A janela de upgrades acrescenta os dados dela só ao menu de quem a abriu, o
  atraso de um tick dos upgrades na janela acabou (o servidor lê o valor vivo), e o cliente de outro processo resolve o químico do tanque pelo registro de recursos que o servidor
  manda. Um espectador com a tela de uma máquina aberta passa a contar como jogador olhando (`playersUsing`), e a máquina refaz a checagem de erros da receita enquanto ele olha.
  `lastEnergyReceived` passa a contar a energia que entra pela capability em jogo (injeção `energy_store`), e o Heat Generator guarda `redstoneLastTick` no fim do tick. Uma injeção
  crua nova (`energy_store`), e o hook `mod_container_viewers` deixou de ser usado (`docs/injecoes-horizon.md`).
- O bloco aceso das quatro máquinas emite luz 8, como `Attributes.ACTIVE_LIGHT` (`Machine.java:26`), e não 13 do gabarito `minecraft:furnace`; o Heat Generator
  segue em 13 (`ACTIVE_MELT_LIGHT`) e apagado emite 0. Injeção crua `active_light` (`properties.light` é um inteiro por bloco, H18).

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

- A tela de uma máquina (as quatro e o Heat Generator) aberta num mundo reaberto antes do primeiro tick dela mostra os valores salvos (energia, progresso da receita, tanque, calor, upgrades e modo de redstone) em vez
  dos padrão, como no original, que lê o tile direto (`MekanismContainer.broadcastChanges` e `sendAllDataToRemote` leem o tile que o `loadAdditional` já carregou). O objeto vivo do port nasce no primeiro tick; até lá o menu
  lê um objeto feito do NBT (`machinery_basis_core::menu_source`), e um objeto vivo velho não vale para um tile novo na mesma posição. Rastros `{chamber,infuser,heat_generator}_trackers_reloaded` do Java
  (`TrackerTraceTest`), repetidos por `tracker_trace.rs` dos dois mods, e a cena `reabrir=1` de `CenaTelas` lado a lado em jogo.
- O pacote do menu leva o `ResourceType` da pilha (item, fluido ou químico, antes do id), como o `ResourceStackPropertyData` do Java, e o espelho só aceita a pilha do tipo do dado. Sem mudança que o jogador veja.
- O aviso do comparador de uma máquina sai dentro do tick dela, como no original (`TileEntityMekanism.tickServer`, `:645-652`: `notifyComparatorChange` roda ali), e não
  depois de todos os block entities (`level_tick_post`). Uma máquina vizinha que tica depois já recebe o `onNeighborChange` no mesmo tick (e conserta o `redstone` velho nele); a
  que tica antes o recebe no tick seguinte. Antes, a máquina também avisava os vizinhos no primeiro tick mesmo sem mudar de nível, o que o original não faz. A injeção
  `level_tick_post` saiu. Rastros `comparator_order` (as duas ordens de ticker, com um comparador lendo cada máquina, a saída a cada tick) e `comparator_first_tick`,
  repetidos por `comparator_trace.rs`, que também repete o valor analógico das quatro máquinas vazias, com itens e cheias (`comparator_levels_*`).
- Uma máquina posta por um jogador já tem a capacidade do item (1.000, ou a que os upgrades dão) no tick da colocação (`setPlacedBy` -> `applyImplicitComponents` ->
  `recalculateUpgrades`), e não só no primeiro tick dela: um Heat Generator ou um cabo que encha a máquina nesse tick esvazia tudo de uma vez, e antes o port a deixava com os
  quatro ticks de consumo do bloco (20). Rastro `placed_fed_*` (o gerador cheio encostado, nos dois momentos do tick), repetido por `placed_fed.rs`.
- O `redstone` das quatro máquinas e do Heat Generator só muda quando o jogo avisa o bloco, como no original (`updatePower`, `TileEntityMekanism.java:1109`): `BlockTile.neighborChanged`
  (um vizinho que mudou com a flag 1 do `setBlock`), `onNeighborChange` do NeoForge (uma máquina vizinha que mudou o nível do comparador) e `onAdded` (o bloco posto com um
  item). O port lia o sinal dos seis vizinhos a cada tick da máquina, então uma mudança que não avisa (`/setblock ... strict`, `setBlock` sem a flag 1) ligava ou desligava
  a máquina na hora, e no original o `redstone` fica velho até o próximo aviso (um bloco posto ao lado, uma alavanca vizinha); uma máquina posta por `/setblock` ao lado de um
  sinal ligado também nasce com `redstone = false`, e só a posta com um item o lê. O Heat Generator ignorava o controle de redstone: agora `canFunction()`
  (`TileEntityHeatGenerator.java:140`) segue o modo e o `redstone`, e os dois vão ao NBT. O hook `neighbor_changed` anota o aviso e o fim da operação no mundo (`Game::flush_spill`,
  injeção crua, ficha H34 em `docs/injecoes-horizon.md`) lê o sinal. Rastros do Java `neighbors_*` (`NeighborTraceTest`, quinze arquivos), repetidos tick a tick no simulador
  (`neighbors_trace.rs` no Mekanism e no Generators) e em jogo (`h2_vizinhos.py`). O `onNeighborChange` do NeoForge dos contêineres do vanilla (um baú, um funil) sai de `Game::container_changed` (segunda injeção crua, ficha H35); fica de fora o dos block entities do vanilla sem contêiner (uma placa, um sino) e o de `Level.removeBlockEntity`.

- O overlay de nível >= 0,98 da barra de energia das quatro máquinas agora aparece: com o erro de energia ainda aceso (a máquina pausada, que não recalculou) e a energia que
  chegou de fora do tick dela (um gerador encostado, ou a energia posta por comando), a barra cheia ganha o aviso listrado por cima do conteúdo e a aba de avisos aparece, como
  no original (`GuiBar.java:70`). O original sincroniza os data slots da tela no `broadcastChanges`, no `ServerPlayer.tick` do tick seguinte, com o valor vivo de cada um, e o port
  os escrevia no tick da máquina, antes de a energia chegar; agora os escreve no fim do tick do servidor (`server_tick`, fase `ServerTickEnd`), e a energia posta por `data merge`
  não zera mais a barra até o tick seguinte. Sem injeção crua nova (um hook do catálogo com estrutura privada, ficha H33 em `docs/injecoes-horizon.md`).
- Clicar numa aba lateral da tela de uma máquina (a de calor e a de energia do Heat Generator, a de upgrades e a janela dela) ou num slot fora da imagem com um item no
  cursor não solta mais o item: `GuiMekanism.hasClickedOutside` só chama de "fora" o que não está sobre um elemento ativo da tela, e o NeoForge nunca deixa um slot sob
  o mouse ser "fora". O Horizon olhava só o retângulo da imagem de 176 por 166 e soltava a pilha. As abas de energia, de avisos e de experiência das quatro máquinas
  do Mekanism entram nessa conta também (2026-11-07): a de avisos só com algum aviso e o botão de 18 por 18 da de xp só com o upgrade instalado, como no original; o
  clique direito sobre uma aba também não solta, e a borda do suporte da aba de xp (fora do botão) solta, como lá. Conferido ponto a ponto contra o cliente oficial
  (`dev/referencia/rastros/fora.jsonl`) e em jogo. O registro dos elementos de tela saiu da barra de armadura para `machinery_basis_core::screen_elements`, com condição por
  data slots (veja `docs/machinery-basis.md`). Segurança, redstone, livro de receitas e configuração ainda não existem no port, então o clique nelas segue soltando o item.
- O roteiro de comparação de telas do port virava o jogador para a máquina (`interact` põe inclinação 20 e giro -9), e o mundo atrás da tela era outra vista
  que a do original (a máquina ficava atrás do painel): o roteiro volta a `look 0 0` depois de abrir a tela, e aplica as entradas da luz do original
  (`time set 6000`, `weather clear`).
- O tooltip de uma linha da lista da janela de upgrades quebra a descrição em 170 pixels e usa o espaçamento de um `Tooltip.create`, como o original
  (`GuiUpgradeScrollList.updateTooltip`): antes saía numa linha só, mais larga que a caixa do original, e com a folga de título de um tooltip de item.
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
