# Changelog

Formato baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/).

## [Unreleased]

### Added

- A rede de energia está na lib `machinery_basis_core::transmission`, em código puro e conferida contra o original, sem uso no jogo ainda (Fase 3, frente `rede-energia`, decisões 112 e 113): o buffer somado da rede, a capacidade por tier, o `currentScale` e o evento da escala, a fusão de energia e de escala, o corte do buffer, as shares dos cabos na quebra e na descarga do chunk, o NBT da energia, o dado de upgrade e o `pullFromAcceptors` do `UniversalCable`, e as regras de `BufferedTransmitter`. Nove cenários do `EnergyNetworkTraceTest` em duas execuções (`rastros/energy_*.jsonl`), com a energia de cada cabo, rede e máquina igual tick a tick. A Enrichment Chamber posta por `setBlock` não expunha a capability de energia no `gameTest` porque os lados dela nascem `NONE`: a configuração do item (`applyComponentsFromItemStack`) a expõe.
- A distribuição genérica do Mekanism está na lib `machinery_basis_core::distribution`, em código puro e conferida contra o original (frente `rede-distribuicao`, Fase 3; sem uso novo no jogo): `SplitInfo`, `Target` sobre as transações do NeoForge (`sendPossible`, `shiftNeeded`, `sendRemainingSplit`, a ordem e o resto, o mesmo handler listado duas vezes mantendo o lugar da primeira vez em `needed`), `EmitUtils.sendToAcceptors` (long e int), os alvos de energia, de recurso (fluido, químico) e de save (contêiner e transmissor) e `EnergyUtils.emit` (lista e contêiner). O rastro `distribution.jsonl` (`DistributionTraceTest`, 730 casos: 286 de alvo, 72 do int, 85 de `emit` de lista, 191 de save e 95 de fluido, com cada chamada de `insert` na ordem) repete com zero diferenças. O ejetor do cubo de energia passa a usar o módulo no lugar de um `SplitInfo` e um `emit` privados, sem mudar o resultado (`energy_cube_trace`, 9 cenários, 0 diferenças).
- A conexão por lado de um transmissor está na lib `machinery_basis_core::transmission`, em código puro e conferida contra o original (frente `rede-conexao`, Fase 3; ainda sem uso no jogo, o cabo e o cano seguem como estavam): `ConnectionType` (a ordem `NORMAL`, `PUSH`, `PULL`, `NONE`, o ciclo do Configurator com `getNext`, `BY_ID` que cai em `NORMAL`) e `TransmissionType` (`BY_ID` que dá a volta), os bytes `currentTransmitterConnections` e `currentAcceptorConnections` com `refreshConnections` nas duas formas, `refreshAcceptorConnections`, o redstone reativo (`isRedstoneActivated`, `recheckRedstone`), `canConnect`, `canConnectMutual`, `isValidTransmitter`, o `getConnectionType` visto (um transmissor do outro lado mostra `NORMAL` mesmo com o tipo cru `PUSH` ou `PULL`), `onNeighborBlockChange`, `onModeChange`, o clique agachado do Configurator (`TileEntityTransmitter.onSneakRightClick`, com o lado do braço que o olhar toca), o clique do redstone reativo, `canExtract` e `canInsert` por lado, `read`, `write`, `writeReducedUpdatedTag` e `handleUpdateTag` com as chaves do original, e o `AcceptorCache` (a capability por lado guardada até ser invalidada, o ouvinte que suja o lado). O mundo entra por um trait (`TransmitterWorld`) e o que a rede e os vizinhos teriam de fazer sai como lista ordenada de efeitos. Rastros do `TransmitterConnectionTraceTest` (`transmitter_*.jsonl`: 280 vizinhos, 204 cliques do Configurator, 12 cenários de redstone, 30 NBT) repetidos por `tests/transmitter_connection_reference.rs` e `tests/transmitter_basics_reference.rs`, com zero diferenças.
- A topologia das redes de transmissores está na `machinery_basis/core` (`transmission/`), sem uso no jogo ainda (Fase 3, frente `rede-registro`, decisões 101 e 102): o `TransmitterNetworkRegistry` (`onTick` na ordem do original, `trackTransmitter`, `invalidateTransmitter`, `registerOrphanTransmitter`, `onTicketLevelChange`), o `OrphanPathFinder` (busca em largura, fusão por `createNetworkByMerging`), a topologia da `DynamicNetwork` (`commit`, `invalidate`, `adoptTransmittersAndAcceptorsFrom`), o `NetworkAcceptorCache` e o `CompatibleTransmitterValidator`, genéricos para energia, fluido, químico, item e calor. Conferida tick a tick contra o original em onze cenários e duas execuções (`rastros/network_*.jsonl`), inclusive a ordem de iteração dos conjuntos do fastutil, que depende de hashes sorteados a cada execução.
- Os eventos do jogo de que a rede de transmissores da Fase 3 vive, entregues na ordem do original (decisão 107, H131): `machinery_basis` avisa quem registrou um tipo de block entity de `clearRemoved`, `onAdded`, `onChunkUnloaded`, `setRemoved`, do ticker, do vizinho que mudou, da capability de um vizinho invalidada, do ticket de um chunk e do `ServerTickEvent.Post` (com `runsNormally`), conferidos contra um rastro do Java (`TransmitterEventsTraceTest`) e atrás de uma sonda desligada; o cabo de energia segue como antes.
- O item solto parado na célula com lava de um bloco com colisão (o Heat Generator com `fluid_logged=lava`) não pega fogo, e a água não o apaga, como no original (decisão 100, H123 e H124): o item cuja caixa está dentro de uma colisão fica com `noPhysics` e `Entity.isAffectedByBlocks` é falso, então nenhum efeito de bloco (a lava que acende e fere, a água que apaga, o cacto) age sobre ele; enquanto o `moveTowardsClosestSpace` não o põe para fora ele vive com o fogo que trazia descendo 1 por tick, e no tick em que sai com a lava ainda nele ela o acende. No cabo, sem colisão no vão, o item morre no segundo tick, como antes. O `environment_tick` também passou a seguir o original no `isInLava` falso do primeiro tick (o item em chamas na lava morre no primeiro tick) e no `baseTick` antes da água que apaga (o item em chamas na água perde 1 de vida). As injeções `item_no_physics` e `item_effects` e o módulo `item-effects` (`machinery_basis`: 168 injeções, 78 cruas, 37 módulos), contra o rastro novo `fluido_item_parado.jsonl` (`FluidoItemParadoTraceTest.java`, 14 casos de 12 ticks com o `noPhysics`, o fogo e a vida) e 11 mutações, e em jogo (`h2_fluido_no_bloco.py parado`, 26 casos).
- O dispenser com um balde contra um bloco com `fluid_logged` faz o que o original faz (decisão 99, H110): o balde de lava, de água e de peixe enche o cabo, o cano, o cubo de energia, o Heat Generator e as máquinas que o declaram, quando estão vazios (o balde fica vazio no dispenser, o fluido entra no bloco e agenda o tick); o balde vazio tira o fluido de dentro e o balde cheio vai para o slot livre do dispenser; o que não dá (bloco com fluido, o Heat Generator com o balde de água cheio, neve em pó) solta o item. Antes o dispenser do Horizon trocava o cabo pelo fluido e não alcançava o resto. O helper `dispense_item` da `machinery_basis` decide por `machinery_basis_core::fluid_logged::dispense_bucket`, conferido contra 90 casos do original (`rastros/dispenser_bucket.jsonl`, `DispenserBucketTraceTest.java`), sem injeção nova.
- O Heat Generator ganha o valor `water` em `fluid_logged` e a regra de colocação do original (`facing` pelo jogador e `fluid_logged` pelo fluido do lugar): 24 estados, como no original (antes 16, só `empty` e `lava`).
- O mob e o item solto dentro de um bloco com `fluid_logged` sentem a água e a lava, como no original (decisão 88, H3): o porco perde o ar na água (300 a 241 em 59 ticks), queima e morre na lava (no tick 21) e o item de pedra é destruído pela lava, tick a tick como o rastro do original. A injeção `logged_fluid_facts` (HEAD cancelável de `gameplay_tables`, mais o módulo `logged-fluid-facts`) dá a fonte de água ou de lava às fatos que o mob e o item leem.

- O cubo de energia (os cinco tiers) e o Heat Generator expõem o contêiner de itens por instância e por face, como as quatro máquinas (decisão 94, H2): o hopper, o dropper, o dispenser e o ejetor de outra máquina agem pelas faces do cubo (a configuração de lados, com a regra dos slots de carga e de descarga, que olha a energia guardada e a do item) e do gerador (combustível em todas as faces menos a direita, energia só na direita, `COOKING_FUEL` ou balde de lava na entrada, e só sai o que não é combustível nem balde de lava). O Heat Generator aceita no slot de energia os itens com handler de energia (o Energy Tablet, o equipamento e os cubos), como o `EnergyInventorySlot.drain`. O hook `container_accepts` passou do `mekanism` para a `machinery_basis` e a contagem de injeções segue em 165. Rastro novo do Java (`dropper_energy_cube_basic`, `dropper_energy_cube_creative` e `dropper_heat_generator`: 180 fases de dropper e de dispenser e 72 de saída por bloco) repetido por `dropper_cube_trace.rs` e pelo `dropper_trace.rs` do Generators.

- As quatro máquinas configuráveis (Enrichment Chamber, Crusher, Energized Smelter e Metallurgic Infuser) expõem o contêiner de itens por instância e por face (opção B da decisão 45, H2): o hopper, o dropper e o ejetor de outra máquina agem pelas faces que a configuração de lados dá, e o item que a face ou o slot recusa fica com quem o deu (o dropper o guarda, nada cai no chão), como no original; o dispenser segue só soltando o item. Três injeções cruas (`container_instance` e `container_take` da `machinery_basis`, `container_accepts` do `mekanism`) no lugar da emulação no tick da máquina, que saiu: o ritmo é o do hopper do Horizon, 1 item por 8 ticks. Conferido sem o jogo contra o original (as 810 configurações de capability, as fases de hopper, as sondas dos slots, e as 120 fases de dropper e as 120 de dispenser que o Java gravou em `dropper_*.jsonl`); em jogo, 16 casos iguais e o ritmo medido. O cubo de energia e o Heat Generator seguem pela emulação (decisão 94).

- A água e a lava dentro de um bloco com `fluid_logged` são fluido de verdade, como no original (decisão 21, H3): o mesher as desenha no vão, o fluxo para fora é o do original tick a tick (oito blocos, 71 de 80 conferências; o alarme industrial, que o original derruba, é a diferença), o jogador no vão cai a -0,005 na água e a -0,02 na lava e perde vida, e o balde de água ou de lava enche o bloco (sem agachar) e o esvazia, como `BucketItem.use` (decisão 89; o cabo 24 de 27 casos iguais). Duas injeções cruas novas, `logged_fluid_begin` e `logged_fluid_view`, e o módulo `logged-fluid-view`; o hook `block_entity_submit` da `machinery_basis` e o cubo de lava da block entity saem. O mob e o item solto no vão ainda não sentem o fluido (decisão 88, H3). Testado contra o rastro do Java (`FluidoNoBlocoTraceTest`) e em jogo (`h2_fluido_no_bloco.py`).
- Os 22 transmissores (os quatro cabos, os tubos, os condutores, os canos, os transportadores, o restritivo e o desviador) são desenhados pelo OBJ do original, `transmitter_small.obj.mek` e `transmitter_large.obj.mek`, parte por parte (o lado ligado mostra a parte `NORMAL`, o outro a `NONE`), em vez do centro e do braço em JSON: o item, a árvore de modelos, o OBJ e o `.mtl` são cópias byte a byte do original, e o blockstate é um `multipart` de 12 entradas no lugar do `mekanism:special/transmitter` que o Horizon recusa (decisão 26 (b)). Duas injeções cruas (`obj_material_parse` e `obj_material_quads`, H16) leem o `Kd` (0,8) e o `neoforge_TintIndex` do `.mtl`, que o Horizon perdia. Conferido sem o jogo (o blockstate e as 24 partes do OBJ de cada transmissor contra o original, a conta do `Kd` contra o NeoForge, e as 264 partes cozidas no `horizon-assets` do pin); em jogo, contra o original, os pixels diferentes dos transmissores caíram de 1,5 % para 0,59 % (pequenos) e de 3,1 % para 1,12 % (grandes); o que sobra é o `hideContiguousJoin` (Fase 4).

- A nota do bloco de nota toca na categoria de som `record`, como o `NoteBlock` do vanilla (`NoteBlock.java:162`, decisão 92, H117): a injeção `note_sound_source` (HEAD cancelável de `Audio::play_seeded`) toca os eventos `block.note_block.*`, de qualquer bloco de nota, na categoria `record` em vez de `block`. Com `block` em 0 e `record` em 1 a nota toca, como no original; o vídeo do bloco de nota foi regravado só com `record`.
- O Metallurgic Infuser e o Heat Generator (gabarito `minecraft:furnace`) deixam de ser um cubo cheio para o jogo (decisão 93): `isSolidRender` falso, amortecimento de luz 0, a luz do céu passa e `canOcclude` verdadeiro, como no original, e as cinco máquinas do forno (com o Crusher, a Enrichment Chamber e o Energized Smelter) têm a reação ao pistão `IMMOVEABLE`. A tabela `FURNACE`, gerada do original em jogo, é aplicada pelas injeções de linha que já existiam.
- A forma de oclusão real dos blocos de máquina e dos cabos e transmissores (decisão 22, H51): a injeção `occlusion_shapes` (`machinery_basis`, HEAD cancelável de `tables`) escreve em `occlusion.nbt` a forma de seleção de cada estado (o `getOcclusionShape` do `Block`) e o centro dos 22 cabos e transmissores (`BlockTransmitter.getOcclusionShape`). Só o bloco `noOcclusion` do original leva `occludes: false`; a linha de jogo diz o `canOcclude` do original, e o Metallurgic Infuser e o Heat Generator o recuperam. Conferido estado a estado contra o original (`rastros/blocos_oclusao.jsonl`, `testes/OclusaoTraceTest.java`) e em jogo: a faixa clara sob o laser, o QIO Exporter, o Logistical Sorter e o Quantum Entangloporter não volta.
- O corte de face entre o vidro estrutural, o vidro do reator e a matriz de foco do laser (decisão 30, H51): as injeções `glass_ids` e `glass_skip` e o módulo `glass-skip` (`mekanismgenerators`) seguem o `skipRendering` de `BlockTileGlass`, `BlockReactorGlass` e `BlockLaserFocusMatrix` (sete pares, conferidos contra os 42.436 pares do original); a linha de 1 a 2 pixels em volta da matriz some.
- O balde cheio não derrama num cabo ou transmissor (`BlockMekanism.canBeReplaced(state, fluid)`, decisão 86, H110): o hook `fill_bucket`. `isPathfindable` e o fluxo de fluido já eram iguais (decisão 87).
- Os vídeos de teste em jogo saem com o som do jogo (`dev/referencia/gravar-jogo.sh rec ... <container>`), gravado num sink nulo próprio do PipeWire e juntado ao vídeo, sem nada nas caixas do usuário; `audio_nulo.sh seguro` vigia que o stream só vai para sinks de teste. O vídeo do bloco de nota sobre o sal e o bio fuel (decisão 17) foi regravado com som.
- A entidade dentro de um fluido de mod age como no original (decisão 18, H47 e H106): o jogador e o mob que não voa ficam parados, sem gravidade nem nado (o 26.3 não move uma entidade num fluido que não é água nem lava), o ar cai 1 por tick, a queda anda por 0,5 a cada tick em vez de zerar, não há respingo, o fogo não apaga, e a correnteza soma 0,014 por tick ao mob também. Oito injeções cruas `fluid_entity_*` (`player_travel`, `Mob::travel`, `Mob::update_fluids`, `breathing::breathe` e os métodos do `EntityFluidTickWorld`), que substituem a `fire_before`; testado contra o rastro `fluids_entity.jsonl` (`fluid_entity_reference.rs`, `fluid_entity_module.rs`) e em jogo (`h2_fluido_entidade.py`, `h2_fluido_entidade_video.py`).

- O bloco de nota em cima do sal e do bloco de bio fuel toca o instrumento do original (`SNARE` e `BANJO`, `MekanismBlocks.java:980-982`) e não mais `HARP`, o do gabarito `lapis_block`: a injeção `note_instrument` (RETURN de `J::rename`, H44) escreve o instrumento na linha do estado dos dois blocos. Conferido em jogo pelo estado do bloco de nota e pelo som que o jogo toca, ao pôr o bloco de nota por jogador e ao trocar a base por baixo; o vanilla (lápis, areia, feno) segue igual.
- O núcleo do item do Energy Cube com energia (`RenderEnergyCubeItem`): translúcido, na cor do tier com o alfa da energia guardada, girando e subindo e descendo com o `gameTime` na barra de atalhos, na mão, no chão e no quadro, por uma camada a mais no item resolvido (injeção crua `energy_cube_item_core`, H99). Contra o original, a mão com energia difere em 1 pixel ou menos (era 128). O alfa do núcleo do cubo posto passou a ser o `floor` do `ARGB.as8BitChannel` (0,5 dava 128, agora 127, como o original).
- O item do Energy Cube (os cinco tiers) mostra a porta nos seis lados e o LED aceso no lado que sai (a frente; os seis no criativo), como o `RenderEnergyCubeItem` com a configuração padrão do item: na mão sem energia 0 pixel de diferença contra o original e 49 na barra de atalhos (antes 23.000 e 2.310). O roteiro de comparação do cubo (`h2_blocos.py`) passou a limitar a energia do criativo cheio a `Long.MAX_VALUE`, e `h2_cubo_item.py` repete o cubo posto de novo e o núcleo sob `tick freeze`.
- A comparação em pixel do Energy Cube contra o original (`cena_telas.py`, `CenaTelas.java`, `comparar_lote.py`, `comparar_cubo.py`, `cenas/cubo.json`): a tela em guiScale 2 e 3 (vazio, parcial, cheio, ultimate, criativo, o medidor com o cursor, os slots, a aba de energia, as janelas de lados e de transporte) deu 0 pixel nos elementos, o medidor animado pelo ciclo de 32 quadros (40 de 40 prints iguais), e o modelo no mundo e o item ficaram medidos (o item na barra e na mão divergia: sem portas, decisão 74, H99, corrigido na frente `cubo-item`; o núcleo sob `tick freeze` não se mexe, o pedido H100 saiu).

- A tela do cubo de energia (`GuiEnergyCube`), aberta no clique nos cinco cubos: o medidor largo de energia com o tooltip, os slots de descarga (`MINUS`) e de carga (`PLUS`) com a regra de `EnergyInventorySlot`, o painel de armadura à direita, a aba de energia (`Input` e `Max Output`) e as abas e janelas de lados e de transporte (a janela de upgrades só existe nos blocos com upgrades). O menu abre pelo hook `right_click_block` (o gabarito `minecraft:beacon` fazia o bloco de nota de cima tocar `hat` em vez de `harp`, decisão 68).
- Os cinco cubos de energia (`TileEntityEnergyCube`, básico, avançado, elite, ultimate e criativo), conferidos tick a tick contra o original (`rastros/energy_cube_*.jsonl`, nove cenários, nenhuma diferença) e em jogo: a energia em `long` por instância com a capacidade e a taxa de `tiers.toml`, o limite de taxa só para o ejetor e os slots (quem vem de fora não tem teto), o criativo que aceita e entrega sem guardar (o valor exato em `energy_amount`), os slots de descarga (enche do item que entrega, converte `redstone` e o bloco dela) e de carga (`mekanism:energy` do item), o ejetor de energia para os vizinhos com a divisão do `SplitInfo` pela configuração de lados `ENERGY` e `ITEM` por instância, o `lastEnergyReceived`, o comparador, o item posto e solto com a energia e a configuração, o Configurator e o cartão, e as duas pilhas do cubo criativo na aba criativa. O desenho (as portas, o LED aceso de cada lado e o núcleo translúcido que gira, pelo `CubeSideState` que o servidor manda) sai da block entity, sobre o modelo estático. A tela, a forma por instância e o desenho do item ficam para depois (ver o plano).
- A forma de seleção do cubo de energia por instância (`BlockEnergyCube.getShape`): o contorno e o raio de seleção seguem o quadro, girado pelo `facing`, e as placas dos lados habilitados, conferidos contra o rastro do original (384 casos: seis `facing` e as 64 combinações de lados). A colisão segue a forma por estado (H92).
- Os efeitos de tela do Configurator e dos itens de modo, conferidos pixel a pixel contra o original em jogo: a sobreposição colorida da face que o olhar toca (o sprite do tipo de transmissão na cor do `DataType` a 60 %, girada nas faces de cima e de baixo; o contorno padrão some), o texto do modo sobre a barra por 5 segundos (`MekanismStatusOverlay`), o texto `Mode: ...` do HUD (`MekanismHUD`, com `hud.enabled`, `hud.scale` e `hud.reverse` no `Mekanism/client.toml`) e o menu radial (`GuiRadialSelector`: o anel, a fatia atual, a fatia sob o mouse, a rolagem, o clique e o `removed`; `accessibility.whiteRadialText` e `accessibility.allowModeScroll`). A tecla `N` com o Configurator só abre o menu radial e não troca mais o modo, como no original (a rolagem agachado troca). A regra das dicas de slot e de medidor com o Configurator no cursor está pronta (`configurator_slot_tooltip`, `configurator_gauge_tooltip`). Seis hooks (`gui_layer_post`, `screen_render_post`, `screen_key_pressed_pre`, `screen_mouse_button_pressed_pre`, `screen_mouse_scrolled_pre`, `block_highlight`) e uma injeção crua (`world_overlay`, H87, decisão 62).

- A energia guardada no item (`ContainerType.ENERGY`, o componente `mekanism:energy`, um `long`): o Energy Tablet, o Atomic Disassembler, o arco elétrico, o Portable Teleporter, o Free Runners (e o blindado), o Meka-Tool, as quatro peças do MekaSuit, o Robit, os cinco cubos de energia e as células de indução guardam energia no item, com a capacidade e a taxa de carga de `gear.toml` e `tiers.toml` (arquivos novos e chaves novas, com os nomes do original), a barra de energia na cor de `energyColor` de `client.toml` (só nos itens que o original dá barra, e não na pilha de mais de um item), a linha `Stored energy:` do tooltip (energia e capacidade) e, na aba criativa e nas abas do vanilla, o item vazio e o cheio (o cheio com a capacidade de `gear.toml` e `tiers.toml` de agora). A Enrichment Chamber, o Crusher, o Energized Smelter, o Metallurgic Infuser e o Heat Generator levam a energia ao serem quebrados (também pelo Configurator) e a recebem ao serem repostos, limitada à capacidade do tile. A receita `mekanism:mek_data` soma a energia das entradas e a põe no resultado até a capacidade dele (injeções `mek_data_energy_grid` e `mek_data_energy_use`, H79). O Configurator não guarda energia no 26.3. O carregamento pelo slot de energia das máquinas fica para a Fase 5.
- As abas de configuração de itens e de transporte, à esquerda da tela das quatro máquinas, e as janelas que elas abrem (`GuiSideConfigurationTab`, `GuiTransporterConfigTab`, `GuiSideConfiguration`, `GuiTransporterConfig`): a de lados mostra um botão por lado, colorido pelo `DataType` e com o bloco vizinho, o ejetor, o botão de limpar os lados (shift para todos os tipos) e as abas dos tipos (itens, energia, químico); a de transporte, o `Strict Input`, a cor de saída e a cor de entrada de cada lado. Os cliques mandam ao servidor o que os pacotes do original fazem (`PacketSideData`, `PacketEjectConfiguration`, `PacketBatchConfiguration`, `PacketEjectColor`, `PacketInputColor`), e a máquina obedece (os hoppers e o ejetor seguem o lado). As janelas se empilham (a de baixo fica desfocada), guardam a posição e o fixar, e o clique nas abas com um item no cursor não solta mais o item.
- O tipo de receita de mesa `mekanism:mek_data` (`MekanismShapedRecipe`) e as 86 receitas da datagen que o usam (85 do Mekanism e a do Wind Generator): os depósitos, tanques de fluido e de químico, cubos de energia e células de indução acima do básico, as fábricas, as máquinas que levam outra máquina como entrada, os drives do QIO e 12 itens de equipamento (`atomic_disassembler`, `electric_bow`, `flamethrower`, `jetpack`...). O Horizon manda o tipo de mod para uma tabela que a mesa não lê; a injeção `mek_data_alias` (H79) o lê como `crafting_shaped`, e os arquivos ficam os do original. Em jogo as 86 dão o item certo na mesa. O que a `assemble` do original copia da entrada (energia, tanques, upgrades, segurança) espera o item guardar o dado; a regra de energia e de segurança está pronta em `machinery_basis_core::recipe_data`, conferida contra um rastro do Java. O original não copia a configuração de lados nem o ejetor, e o port também não. Entram as tags `c:strings` e `c:tools/igniter` do NeoForge, que as receitas do arco elétrico e do lança-chamas citam.
- O Configurator (`ItemConfigurator`) e o cartão de configuração (`ItemConfigurationCard`), com a função do original, conferidos clique a clique contra o original em jogo (`rastros/configurator_*.jsonl`). Os oito modos
  (`configurate_items`, `_fluids`, `_chemicals`, `_energy`, `_heat`, `empty`, `rotate`, `wrench`) ficam no componente `mekanism:configurator_mode` (o modo padrão não deixa remendo), com o tooltip e o texto de cada um. Sem agachar, o clique abre a tela da máquina
  (o bloco age antes do item) e só gira em `rotate` (o bloco gira no sentido horário e a tela não abre). Agachado: em configurar, avança o `DataType` da face e avisa (`toggle_mode`, com as cores do tipo); em `empty`, solta os
  itens dos slots pela face; em `rotate`, vira para a face oposta; em `wrench`, desmonta a máquina para o inventário com a configuração. A pedra e o ar respondem `SUCCESS` ao `rotate`, como no original. A tecla `N` (troca o modo, para trás
  agachado) e a rolagem agachado trocam o modo pelo pacote `mekanism:mode`. O cartão, agachado, copia a configuração, o ejetor e o modo de redstone da máquina (e só o modo de redstone do Heat Generator) para o componente
  `mekanism:configuration_data`; sem agachar, cola em outra máquina do mesmo tipo (`set`) ou recusa (`unequal`), sem abrir a tela; agachado no ar, esvazia (`cleared`). O modelo do cartão acende com dados (`mekanism:config_card`).
  Com o Configurator na mão, o contorno e a mira de um transmissor mostram só a caixa que o olhar toca (injeção `context_outline`, H71). A sobreposição colorida da face, o texto do modo sobre a barra, o menu radial e a conquista ficam para depois (ver o plano, decisão 48).
- A configuração de lados por instância das quatro máquinas (`TileComponentConfig`, `ConfigInfo`, `DataType`, `TransmissionType`, `ISlotInfo`, `RelativeSide`, `AttachedSideConfig`, `TileEntityConfigurableMachine`) e o ejetor guiado por ela
  (`TileComponentEjector`: o modo de ejetar, os lados de saída, as cores do filtro, `strictInput`, o atraso de meio segundo). Cada lado nasce em `NONE`, e só o item que põe a máquina aplica o padrão dele
  (`AttachedSideConfig.ELECTRIC_MACHINE` e `ADVANCED_MACHINE`): uma máquina posta por `setblock` não aceita nem entrega por face nenhuma, como no original. A configuração é salva no NBT do tile com as chaves do original
  (`component_config`: `eject<n>` e `config<n>`; `component_ejector`) e no item pelos componentes `mekanism:side_config` e `mekanism:ejector`, então sobrevive a quebrar e repor e a sair e voltar ao mundo; os hoppers vizinhos
  seguem o `DataType` de cada face pela frente de agora (o giro do bloco os acompanha), e a capability de energia responde por instância, pelas faces da configuração (injeção `energy_capacity`, H1), então cabo e gerador só entregam pelas faces em
  `INPUT`. O par `writeConfigurationData`/`setConfigurationData` está pronto para o Configurator e o cartão de configuração. A tela de lados, o Configurator e as abas de configuração são de outras frentes.
- O brilho de encantamento no equipamento vestido (`stack.hasFoil()`): o MekaSuit leva o brilho da armadura (`armorCutoutNoCullGlint`, a textura `enchanted_glint_armor` em escala 0,16, em toda peça, opaca, brilhante e translúcida) e o jetpack, o tanque de
  mergulho e as Free Runners o brilho do item (`itemCutoutGlint`, `enchanted_glint_item` em escala 8), no jogador, no armor stand e nos mobs. O original amostra a textura do brilho com a UV0 do quad no atlas de itens, então onde cada sprite cai
  no atlas decide que parte da peça brilha: o port usa o retângulo de cada sprite no atlas do original (`rastros/armor_atlas.json`). Os tipos de desenho e as escalas saem do texto do Java e do Minecraft (`rastros/armor_gear.json`, `foil`).
- O sombreamento de vértice do NeoForge (`EnhancedBlockModelLighter`, `enhancedLighting`, ligado por padrão) nos blocos de mod: o quad que não é de face inteira (um elemento girado, como as abas da caixa de papelão aberta) é sombreado pela normal, e os de
  face inteira recebem os consertos da luz e dos cantos. O algoritmo é um módulo de código puro (`machinery_basis/enhanced_lighting.rs`) conferido quad a quad contra o NeoForge em 1.044 quads de 15 blocos (`rastros/lighting_enhanced.jsonl`),
  aplicado pela injeção `enhanced_ao` (H70); os blocos do vanilla ficam com o do vanilla (decisão 35).
- O conteúdo do Mekanism Generators, sem função (`GeneratorsBlocks`, `GeneratorsItems`, `GeneratorsCreativeTabs`): os 23 blocos que o port ainda não tinha (`solar_generator`,
  `advanced_solar_generator`, `wind_generator`, `bio_generator`, `gas_burning_generator`, os sete da turbina, o `reactor_glass`, os cinco da fissão e os cinco da fusão) e os três itens (`solar_panel`,
  `hohlraum`, `turbine_blade`), cada um com o que o original dá: as propriedades de registro e de estado (`active`, `facing`, `fluid_logged`, `mode`) e os 148 estados do original, a forma
  de seleção e de colisão de cada estado (o Wind Generator e a Control Rod Assembly passam de 16 caixas, pela injeção `shape_limit`), a luz do estado ativo (13 no Bio-Generator e no
  Gas-Burning Generator, 8 no Wind Generator) e a da lava, a reação ao pistão e a oclusão (pela injeção `block_rows`, com a tabela gerada do original em jogo), o posicionamento (`facing`
  pela guinada e `fluid_logged` pelo fluido do lugar), o estado de bloco, a definição do item, 36 modelos (com o `neoforge:obj` do Wind Generator e o `neoforge:composite` dele), 53 texturas,
  o nome e a descrição, a loot table, as tags de bloco e de item (as de estrutura da turbina e dos reatores, que também entram em `mekanism:structures`, e as do vanilla) e 24 receitas da
  datagen (as de mesa e a de infusão do `hohlraum`, com o id do original; a do Heat Generator passa a ser `generator/heat`). O Generators ganha a aba criativa "Mekanism: Generators" (os
  itens, os 24 blocos e os 4 baldes na ordem de registro) e acrescenta os geradores aos blocos funcionais, os blocos com comparador à redstone, os baldes às ferramentas e o hohlraum e o
  painel solar aos ingredientes do vanilla (injeção `creative_tabs`, H43). O vidro do reator e a matriz de foco do laser usam o gabarito do vidro.
- O `transform` de modelo do NeoForge (`translation`, `rotation`, `scale`, `origin` e `matrix`, na raiz de um modelo e em cada filho de um `neoforge:composite`), que o Horizon não lê: duas injeções cruas
  (`model_transform_decode` e `model_transform_bake`, H67) e um módulo juntam a transformação à rotação do blockstate como o `composeRootTransformIntoModelState` do original (o mais perto da folha vence, o filho soma
  a sua por dentro da do `composite`). O painel do Advanced Solar Generator sobe o bloco que o original o sobe, e a torre do Wind Generator vem do `.obj` do original, sem a translação nos vértices. A conta é
  conferida contra um rastro das classes do NeoForge (`ModelTransformTraceTest`).
- Os 62 blocos de `MekanismBlocks` que faltavam, sem função, com tudo o que o original dá a eles (`BlockType`, `BlockBin`, `BlockFluidTank`, `BlockEnergyCube`, `BlockBasicMultiblock`, `BlockLargeTransmitter`...): os depósitos, os tanques de fluido e de químico e os cubos de energia por tier (os quatro e o criativo), as células e os provedores de indução, a carcaça e a porta da matriz, as partes da caldeira, do tanque dinâmico, do SPS e do evaporador térmico, o vidro estrutural, a bobina supercarregada, o elemento de superaquecimento, o dispersor de pressão, os 18 transmissores (canos, tubos, transportadores e condutores por tier, o restritivo e o desviador) e o bloco de apoio. Cada um tem o gabarito `minecraft:lodestone` com a dureza, a resistência, a cor do mapa e o som do original (o vidro estrutural, o `minecraft:glass`), os estados e as formas dele (os cubos de energia passam de 16 caixas e usam a injeção `shape_limit`), a luz do estado ativo, a reação ao pistão e a oclusão pela tabela de linhas, o posicionamento, o estado de bloco, o item, os modelos, as texturas, a loot table, o nome, a descrição, as tags (`mekanism:structures/*`, `transmitters/*`, `bins`...), a raridade épica do SPS e da bobina e as 45 receitas de mesa que ficaram completas (as 40 dos blocos e as do Meka-Tool e do MekaSuit, que esperavam a célula de indução; entram as tags `c:bars` do NeoForge). Os transmissores são o bloco que conecta do Horizon, com o centro de 6 ou 8 pixels, a tag da família e o `fluid_logged` do original, no nível do cabo de energia, que ganha a cor do mapa, o som de pedra, a reação ao pistão e o `fluid_logged`. O cubo de energia, o tanque de fluido e os transmissores são escritos como modelos comuns (o Horizon não carrega o carregador, o desenhista de item nem o OBJ do original) e o vidro do tanque de fluido sai com a cor de cada tier. Os cinco tanques de fluido entram na aba de ferramentas, antes dos baldes. As tags `mekanism:structures` e `structures/common` (de bloco e de item) levam, como opcionais, as entradas do Generators que a datagen dele acrescenta (o Horizon não deixa o Generators escrever em `data/mekanism/tags`), o que completa as receitas do conjunto de combustível da fissão e do controlador do reator.
- Os 22 fluidos do Mekanism e do Mekanism Generators (`MekanismFluids`, `GeneratorsFluids`, `FluidDeferredRegister`), sem função de máquina: o bloco de líquido de cada um (com o nível 0 a 15 do `LiquidBlock`), o balde (com a camada de líquido tingida e a pilha de 1, o de pasta nutritiva bebível, com a fome e a saturação das chaves `nutritionalPasteMBPerFood` e `nutritionalPasteSaturation` de `general.toml`), o nome, os modelos, as texturas com a cor, as tags `c:buckets/*` dos 22 baldes, `minecraft:replaceable` com os blocos e as tags de entidade `c:creepers` e `c:endermen`; os 18 baldes do Mekanism entram na aba criativa do Mekanism e no fim da aba de ferramentas do vanilla. O fluido corre como o original: o `FlowingFluid` do Minecraft 26.3 está portado e bate tick a tick com o Java em 13 cenários (um piso, uma queda, um buraco, degraus, obstáculos, a fonte retirada, duas fontes, um teto, uma coluna e a água do vanilla ao lado). O balde cheio esvazia, o vazio pega a fonte, e o dispensador esvazia o balde; a água e a lava do vanilla só trocam um fluido de mod vindas de cima, e as entidades se movem nele como na água (o jogador em chamas não apaga nele, como o original). O Horizon não tem fluido de mod no motor, então o desenho é um cubo por célula (decisão 18, pedidos H47 e H48).
- Os itens e blocos de craft e de recurso do original, sem função, com tudo o que ele dá a eles (`MekanismItems`, `MekanismBlocks`, `BlockResourceInfo`, `OreType`): 74 itens (os `clump_`,
  `crystal_`, `dirty_dust_` e `shard_` de cada metal, os pós, lingotes, pepitas e brutos de estanho, chumbo e urânio, o bronze e o glowstone e obsidiano refinados, `enriched_tin`,
  `fluorite_gem`, `salt`, `sawdust`, `substrate`, `dye_base`, os `hdpe_*`, os `pellet_*`, `yellow_cake_uranium`, `reprocessed_fissile_fragment`, `teleportation_core`,
  `electrolytic_core`, `module_base`, `module` e os quatro `qio_drive_*`) e 25 blocos (os 16 blocos de recurso, o sal e o bio fuel, os minérios de estanho, urânio, fluorita e chumbo na
  pedra e no deepslate, e o quadro do teleporter), cada um com modelo, estado de bloco, textura, nome, loot table (minério com fortuna; a fluorita de 2 a 4 gemas antes dela),
  dureza, resistência, ferramenta, cor do mapa, luz, som, raridade, resistência ao fogo, tempo de queima (carvão vegetal 16.000 e bio fuel 1.000), cor do nome (no tooltip), material de
  acabamento (os 12 materiais do original, com a paleta e a tag `#minecraft:trim_materials`) e as tags de bloco e de item (`mineable/pickaxe`, `needs_stone_tool`, `beacon_base_blocks`...).
  Os itens e o `steel_casing` que o port já tinha ganham o que faltava (a raridade das ligas, a resistência ao fogo do obsidiano refinado, o `.mcmeta` animado de três texturas e o gabarito
  sem o material de acabamento do ferro). Os arquivos copiados do original são conferidos por hash contra a datagen dele (`rastros/craft_datagen.json`).
- As tags `c:` da datagen que citam esses itens e blocos (84 de item e de bloco; 114 das 173) e as tags `minecraft:` e `mekanism:` do mesmo grupo, mais `c:ender_pearls`, `c:buckets/water` e
  `c:rods/wooden` do NeoForge, e as 99 receitas de mesa e de forno do original que ficaram completas (blocos, lingotes e pepitas, o forno e o alto-forno de cada minério, as peças
  `hdpe`, o núcleo eletrolítico, o quadro do teleporter e três receitas de upgrade). As seis receitas do forno do ósmio escritas à mão saíram: o original tem as dele em `processing/osmium/ingot`.
- Estanho, urânio, fluorita e chumbo geram no mundo como o ósmio (`OreType`: tamanho e descarte, quantidade e altura dos padrões do `WorldConfig`), e o sal gera em discos no fundo da água
  (`ResizableDiskFeature`: raio de 2 a 3, meia-altura 1, duas por chunk). A fluorita dá de 1 a 4 de experiência ao ser quebrada (`BlockOre.getExpDrop`, injeção `fluorite_experience`, H42).
- A aba criativa "Mekanism" (`MekanismCreativeTabs`), com os 143 itens e blocos que o port tem na ordem de registro do original e as oito pilhas de upgrade por tipo; o Mekanism também acrescenta
  os itens ao fim das abas de construção, natureza, funcionais, redstone, ferramentas e ingredientes do vanilla (`addToExistingTabs`), pela injeção `creative_tabs` (H43).
- Os 84 blocos de máquina e blocos avulsos com tile do original, sem função (`MekanismBlocks`): 48 máquinas e blocos avulsos (`antiprotonic_nucleosynthesizer`, `chemical_*`, `combiner`,
  `digital_miner`, `dimensional_stabilizer`, `electric_pump`, `electrolytic_separator`, `fluidic_plenisher`, `formulaic_assemblicator`, `fuelwood_heater`, `isotopic_centrifuge`, `laser` e o
  amplificador e o raio trator, `logistical_sorter`, `modification_station`, `nutritional_liquifier`, `oredictionificator`, `osmium_compressor`, `painting_machine`, `pigment_*`,
  `precision_sawmill`, `pressurized_reaction_chamber`, `purification_chamber`, os cinco `qio_*`, `quantum_entangloporter`, `resistive_heater`, `rotary_condensentrator`, `security_desk`,
  `seismic_vibrator`, `solar_neutron_activator`, `teleporter`, `chargepad`, `industrial_alarm`, `radioactive_waste_barrel`, `personal_barrel`, `personal_chest` e `cardboard_box`) e as 36
  fábricas, nos quatro tiers. Cada bloco tem o que o original dá a ele: dureza, resistência, ferramenta, cor do mapa e som; os estados do original (`facing`, `active`, `fluid_logged`,
  `open`, `storage`), com os mesmos nomes (`/setblock mekanism:laser[facing=up,active=true]` vale nos dois jogos); a forma de seleção e a de colisão de cada estado (a união das caixas do
  original: o limite de 16 caixas por forma do Horizon sai pela injeção `shape_limit`, H50); a luz do estado ativo (8, 13 ou 15), a da lava (15) e a fixa do Teleporter; a reação ao pistão e a
  oclusão do original (a tabela `block_rows_table.rs`, pela injeção `active_light`, H49); o estado de bloco, a definição do item, 173 modelos e 253 texturas (e 12 `.mcmeta` animados) copiados do original, a loot
  table, o nome, a chave `description.*`, as tags de bloco e de item, as 31 receitas de mesa (com as tags `c:` do NeoForge `buckets/empty`, `chests/wooden`, `barrels/wooden`, `storage_blocks/diamond` e `clumps/resin`, iguais às do jar), e o lugar na aba criativa do Mekanism e nas de blocos funcionais, redstone e
  ferramentas do vanilla. O original em jogo (`BlocosTraceTest`) é a referência: `rastros/blocos.jsonl`, `blocos_formas.jsonl` e `blocos_colocacao.jsonl`, que `tests/blocos_registry.rs`
  e `tests/blocos_content.rs` conferem estado a estado.
  O chão sob as máquinas de forma menor que o cubo aparece como no original (`properties.occludes: false` nos 67 blocos de `isSolidRender` falso: antes a face de cima do chão sumia e o céu aparecia numa faixa sob o `laser`, o `qio_exporter`,
  o `logistical_sorter` e o `quantum_entangloporter`); em jogo, os 84 ids, 405 estados, 67 posições e a luz de 159 estados batem com o original, e os prints lado a lado ficam em 0,16 a 0,42 % de pixels diferentes nos blocos apagados.
- O bloco posto como o original o põe (`machinery_basis/placement`): o `facing` vem da guinada do jogador e, nos blocos que viram para cima e para baixo, da inclinação (`AttributeStateFacing`,
  `PLAYER_LOCATION`), ou da face clicada (`SELECTED_FACE`), e o `fluid_logged` do fluido que estava no lugar (`BlockStateHelper.getStateForPlacement`). Confere com o original a cada guinada de
  -720 a 720 graus, a cada inclinação de -90 a 90 e a cada face (`machinery_basis/core/tests/placement_reference.rs`) e em jogo (64 casos).
- Os 37 itens de equipamento do original, sem função (`MekanismItems`): `atomic_disassembler`, `canteen`, `configuration_card`, `configurator`, `crafting_formula`, `dictionary`, `dosimeter`, `electric_bow`, `energy_tablet`,
  `flamethrower`, `free_runners` e a blindada, `gauge_dropper`, `geiger_counter`, os quatro `hazmat_*`, `hdpe_elytra`, `jetpack` e o blindado, os quatro `mekasuit_*`, `meka_tool`, `network_reader`,
  `portable_qio_dashboard`, `portable_teleporter`, `robit`, `scuba_mask`, `scuba_tank`, `seismic_reader` e os quatro instaladores de tier. Cada um tem o que o original dá como conteúdo: raridade, pilha, resistência
  ao fogo, durabilidade e reparo do élitro de HDPE (648, planador, `damage_on_hurt` falso), `Equippable` com o slot e o ativo, os atributos de armadura do MekaSuit (os números do netherite), das botas e do jetpack
  blindados, `mob_visibility` do MekaSuit, encantável, recarga de uso, cor do nome do Configurator e dos instaladores (no tooltip), 59 modelos (inclusive os `neoforge:composite` e o `neoforge:obj` da Meka-Tool) e
  texturas, os ativos de equipamento `hazmat` e `hdpe_elytra` (o traje e o élitro vestidos saem como no original), nomes, 23 tags (`head_armor` e as outras de armadura, `c:tools/*`, `module_containers/*`, `enchantable/*`)
  e 19 receitas de mesa (o traje de hazmat, o conta-gotas, o instalador de tier, o configurador, a canteen, a máscara de mergulho, o élitro de HDPE e outras). As abas de ferramentas e de combate do vanilla ganham os
  itens do Mekanism, e a aba "Mekanism" fica com 180 entradas na ordem do original. Os arquivos copiados e as propriedades são conferidos contra o Java e a datagen (`rastros/equipment_items.json`,
  `tests/equipment_content.rs`). O uso (energia, modos, tanques, módulos, ferramenta e arma, o desenho do Robit) fica para as fases de cada um.
  O arquivo do item do Configurator, que escolhe o modelo por um componente do mod, carrega por uma injeção nova (`item_component_type`, H5).
- O MekaSuit, o jetpack (e o blindado), as Free Runners (e as blindadas) e o tanque de mergulho são desenhados no corpo, como no original (`client/render/armor`: `MekaSuitArmor`,
  `ChestArmor`, `FreeRunnerArmor`): no jogador (em terceira pessoa, na tela do inventário e o braço em primeira pessoa), nos outros jogadores, nos mobs de modelo humanoide (testado no husk)
  e no armor stand, na pose das partes do corpo (agachar, nadar, voar, o balanço das pernas), com o bebê escalado como o original (`scaleBaby`). O traje é o OBJ do original
  (`mekasuit.obj`, 126 objetos) em 22 modelos avulsos, um por grupo que `MekaSuitArmor.createQuads` monta (peça, posição do corpo, brilho, as peças `excl_` e `shared_`), com os LEDs
  (`_led`) sem luz do mundo; o corpo, os braços, as pernas e a cabeça do jogador somem enquanto o traje os cobre, e a capa some com o peito do MekaSuit, o jetpack e o tanque. A mecânica
  é genérica (`machinery_basis_core::worn_gear`, uma tabela de itens com os modelos, as partes e os passos de cada um; `glue`: o desenho) e o Mekanism só dá a tabela
  (`mekanism_rules::worn_gear`). Seis injeções cruas novas e dois módulos (H55). Em teste: os quads de cada peça e de cada combinação de peças vestidas conferem com o
  `MekaSuitArmor.createQuads` rodado no cliente do original (`rastros/armor_groups.jsonl`, 72 linhas), e os números de pose com o texto do Java (`rastros/armor_gear.json`).
- As tags `c:dyes/black`, `c:dyes/orange`, `c:glass_panes` e `c:glass_panes/colorless` do NeoForge (o Horizon não tem tag `c:`), que as receitas do traje de hazmat e do conta-gotas citam (decisão 19).
- As quatro máquinas acesas soltam a fumaça e o pó de redstone na face da frente (`Machine.java:23`), e o Heat Generator aceso a fumaça e a chama pela de trás (`GeneratorsBlockTypes.java:69`), como o `BlockTile.animateTick` do original: o cliente sorteia as posições em volta do jogador a cada tick (667 a 16 blocos e 667 a 32, no
  `ClientLevel.animateTick` do Horizon) e cada bloco aceso sorteado solta uma partícula por entrada, girada pelo `facing`. A chave `machineEffects` (`rendering.particle` do `config/Mekanism/client.toml`, ligada por padrão) as desliga. Uma injeção nova na `machinery_basis` (`animate_tick`, mais um módulo que guarda os animadores dos mods) leva o sorteio do Horizon até o
  bloco de mod; as regras (`machinery_basis_core::active_fx`) repetem um rastro do original com semente fixa, bit a bit para a fumaça e o pó, e em jogo a taxa por segundo fica a menos de 2 desvios da esperança do sorteio nos dois jogos (3,0 por segundo de cada tipo a 3 blocos).
- O `bio_fuel` queima: o item tem o componente `minecraft:cooking_fuel` do original (`cookingFuel(COOKING_TIME_BIO_FUEL)`, `MekanismItems.java:182`), com o provider de 100 ticks (50 num bloco de cozimento rápido) escrito por extenso no JSON do item, igual
  ao arquivo que a datagen do original escreve. O forno, o alto-forno e o slot de combustível do Heat Generator o aceitam e o queimam: o forno e o alto-forno dão os 469 e 269 ticks do rastro do Java, e o Heat Generator converte cada item em 5 mB de
  lava (`1000 * 100 / 20000`), tick a tick como o original (`furnace_bio_fuel`, `blast_furnace_bio_fuel`, `heat_generator_bio_fuel_run`).
- A mensagem de receita incompleta chega ao chat (`IncompleteRecipeScanner`): em vermelho, `[Mekanism] Broken tags in Mekanism recipes detected, please check server logs for details. You will be missing some recipes and machines may not accept expected inputs.`, a
  todo jogador do servidor no `/reload` e a quem entra no mundo (o anfitrião inclusive), quando a varredura acha uma receita incompleta (um ingrediente de tag sem item). As 40 receitas que hoje ficam incompletas só porque o port ainda não tem o conteúdo delas (estanho,
  urânio, chumbo, fluorita, bronze, pós sujos) não contam (decisão 13 do plano), então a mensagem só sai com um data pack que esvazia uma tag. O log `Incomplete recipe detected` passa a sair na carga de dados, uma vez, e não no primeiro uso da máquina; uma tag de
  recurso (químico) sem membro também conta.
- As receitas de data pack das máquinas (`enriching`, `crushing`, `metallurgic_infusing`, a conversão de químico e a de energia) aceitam o ingrediente `neoforge:components` (`DataComponentIngredient`: `items`, `components` e `strict`). O Horizon só entrega ao mod o ingrediente em texto ou lista e descartava a receita sem aviso; o mod lê o arquivo e porta o `test` do NeoForge (valor igual, `!tipo` para o componente ausente, `strict` para a pilha sem componente fora do patch) sobre os componentes padrão do item e o patch da pilha, e o índice de receitas o trata como o original (exato com `strict`, complexo sem). `machinery_basis_core::components_ingredient`, repetido contra um rastro do Java de 285 pares. O Smelter também: a receita de cozimento do jogo com `neoforge:components` (que o Horizon descartava inteira, sem as tags resolvidas) passa por uma injeção crua em `Ingredient::parse_object` (e o hook `ingredient`, para resolver `#tag`), e as receitas de tipo próprio leem `compound`, `intersection` e `difference` em volta dele.
- As receitas de data pack das máquinas (`enriching`, `crushing`, `metallurgic_infusing`, a conversão de químico e a de energia) aceitam o ingrediente `neoforge:components` (`DataComponentIngredient`: `items`, `components` e `strict`). O Horizon só entrega ao mod o ingrediente em texto ou lista e descartava a receita sem aviso; o mod lê o arquivo e porta o `test` do NeoForge (valor igual, `!tipo` para o componente ausente, `strict` para a pilha sem componente fora do patch) sobre os componentes padrão do item e o patch da pilha, e o índice de receitas o trata como o original (exato com `strict`, complexo sem). `machinery_basis_core::components_ingredient`, repetido contra um rastro do Java de 285 pares. Ficam de fora o `compound`, o `intersection` e o `difference` em arquivos de receita e as receitas de cozimento do Smelter com esses ingredientes, que o Horizon descarta antes do mod.
- A configuração de servidor do Mekanism e do Mekanism Generators, nos arquivos, nomes, padrões, limites e comentários do original (`MekanismConfig`, `MekanismGeneratorsConfig`), só nas chaves das mecânicas já portadas. Cada arquivo é `Type.SYNCED` no
  original e o Horizon o lê como `Server`: `config/Mekanism/general.toml` (`blockDeactivationDelay`, 0 a 1200, padrão 60; `maxUpgradeMultiplier`, mínimo 1, padrão 10), `machine-usage.toml` (o consumo da Enrichment Chamber, do Crusher, do Metallurgic Infuser e do Energized
  Smelter), `tiers.toml` (a capacidade dos quatro cabos de energia), `generators.toml` (`heat_generator`: `heatGeneration`, `heatGenerationLava`, `heatGenerationNether`, `tankCapacity` e `heatGenerationFluidRate`) e `generator-storage.toml` (`heatGenerator`, a capacidade do buffer). O mundo pode trocar um deles
  com uma cópia na pasta `serverconfig/` (no NeoForge 26.3 a pasta se chama `syncedconfig/`). Um valor fora do limite vai para o limite, um de outro tipo, ou que o validador recusa, vai para o padrão, e o arquivo é escrito de novo com os comentários, guardando a cópia de antes em `<nome>-1.toml.bak` (até cinco), como o NeoForge:
  conferido contra os mesmos arquivos errados de propósito, corrigidos pelo original (`rastros/config_correcao`). O jogo relê o arquivo quando muda. O `maxUpgradeMultiplier` e o `blockDeactivationDelay` valem no uso seguinte; o consumo e as capacidades, nas máquinas e nos blocos novos. Com `maxUpgradeMultiplier` acima de 200 uma máquina com velocidade
  alta faz mais de uma operação por tick, o que o padrão nunca dá. Uma chave que o arquivo do mod de verdade tem e o port não lê fica no arquivo (o NeoForge as apagaria). A regra genérica (leitura, correção, limites inteiros e longos, validador entre chaves, sub-seções, raiz, o texto do NightConfig e a cópia `.bak`) está na `machinery_basis`
  (`client_config`). Três hooks do catálogo (`register_configs`, `config_loading`, `config_reloading`) passam a valer também para o Mekanism Generators. Em jogo (desktop, headless e mudo): os seis arquivos saem com o texto do original, um valor errado escrito com o jogo aberto volta ao limite com a cópia `.bak`, `enrichmentChamber = 20` gasta 20 por tick, `heatGenerationFluidRate = 250` queima 250 mB por tick, e com `maxUpgradeMultiplier = 1000` o Metallurgic Infuser com 8 de velocidade repete
  o rastro do original de cinco operações por tick.
- O som das máquinas lê o arquivo de configuração de cliente do original, `config/Mekanism/client.toml` (`MekanismConfigHelper.registerConfig`; o mesmo arquivo onde as janelas do Mekanism e do Generators guardam a posição), criado com o texto que o NightConfig escreve
  (`ClientConfig.java:74-81`: seção `sounds`, `baseVolume` de 0 a 10 com padrão 1 e `enableMachine` ligado, com os mesmos comentários, `# Default:` e `# Range:`, mais as posições das janelas). `baseVolume` multiplica o volume de cada
  som no momento em que ele nasce (um som que já toca não muda) e `enableMachine` desligado impede um som novo (um que já toca segue até o jogador sair do alcance). O jogo relê o arquivo
  quando ele muda; um valor fora do limite vai para o limite, um de outro tipo ou ausente vai para o padrão, e o arquivo é escrito de novo com o valor corrigido (as chaves e seções que o port não lê ficam), como o `ModConfigSpec`. A regra
  genérica está na `machinery_basis` (`client_config`); o Mekanism só dá os nomes. Três hooks do catálogo novos (`register_configs`,
  `config_loading`, `config_reloading`, API 397).
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

- O Configurator em `ROTATE` e o cartão de configuração agem antes de a tela do bloco abrir pelo resultado do clique, como o original (`TileEntityMekanism.openGui` devolve `TRY_WITH_EMPTY_HAND`): uma injeção crua em `Game::use_hand` (o hook `right_click_block` saiu) gira o bloco ou passa a vez ao `useOn` do cartão (`SUCCESS_SERVER`, ou `PASS` sem dados), balança a mão para todo jogador, inclusive o convidado de LAN, e dispara `ITEM_USED_ON_BLOCK`; uma segunda, no cliente do convidado (`predict_item_on_block`), prevê o `useOn` do cartão, e o descarte do `use` duplicado (`card_just_used`) saiu. Agachado de mãos vazias a tela das máquinas e do cubo de energia não abre (`openGui` passa), e o menu do cubo não balança a mão nem marca `use_failed`. `mekanism_rules::tile_use` está conferida contra os rastros `configurator_click_*.jsonl` e `configurator_card.jsonl` (decisões 47 e 68, H72).
- Os comandos `mekanism_eject` e `mekanism_eject_face` saem: o ejetor liga e escolhe os lados pelo NBT `component_config` do tile (por exemplo
  `/data merge block x y z {component_config:{eject3:1b,config3:[I;0,0,4,0,0,0]}}`, a direita em `OUTPUT`), que é o que o original grava. `FaceSlots` e `Sides` deixam de ser declarados à mão por bloco: saem do `DataType` de cada lado.
- A mensagem de receita incompleta (decisão 13) vale como no original: com o conteúdo de craft e de recurso completo, as 40 receitas que só faltavam por conteúdo (`CONTENT_GAPS`) viraram 0 e a
  lista saiu; qualquer receita incompleta dos tipos do Mekanism que o port lê levanta a mensagem. As 8 receitas que o Horizon recusava por item desconhecido também deixaram de existir.
- O arquivo de cliente e o de configuração local passam da pasta `config/mekanism/` para a do original, `config/Mekanism/` (o M maiúsculo, `MekanismConfigHelper.CONFIG_DIR`): `client.toml` e `local.toml`. O original não migra nada, então o port também não: quem tinha as posições
  das janelas, o volume dos sons ou a unidade de temperatura em `config/mekanism/` começa dos padrões (os arquivos velhos ficam onde estão, sem uso, e podem ser copiados à mão). As posições das janelas, do Mekanism e do Heat Generator, estão no mesmo arquivo e passam a ter o texto do
  NightConfig do original: o bloco `[window]` com o comentário e uma tabela `[window.<nome>]` por janela, recuada, com um comentário em cada chave (a janela de upgrades é `[window.upgrade]`). O arquivo novo já nasce com `sounds` e `window.upgrade`. Uma correção do arquivo guarda as chaves e as seções que o port não lê, em vez de apagá-las.

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

### Removed

- A emulação do hopper (`tick_hoppers`, `eject_items`, `machinery_basis_core::hopper` e `Sides::of_slots`): o Horizon move os itens pelo contêiner da instância, no ritmo do próprio hopper.

### Fixed

- O balde cheio só deixa de derramar quando o bloco que recebe o fluido é o transmissor (decisão 86, H116): ao lado de uma laje ou escada que se enche de água, um cabo não impede mais de enchê-la, como no original. A injeção `bucket_target` (RETURN de `Game::bucket_target`) anota o bloco que recebe, e o hook `fill_bucket` recusa só esse.
- A cor do nome de item com `setNameColor` (o Configurator, os circuitos de controle, os instaladores de tier e os pellets) saía a 19 a 37 % de opacidade, e as linhas do tooltip do upgrade saíam com as cores trocadas (o `u32` da linha de tooltip é ARGB e o glue mandava RGBA): a cor agora vai no estilo do texto.
- A caixa de papelão aberta (e os outros blocos de elemento girado: o barril radioativo, a mesa de segurança, o vibrador sísmico, o ativador de nêutrons, a estação de modificação, o entangloporter, o minerador digital) tem o sombreamento do original: a aba da
  esquerda quase preta e a da direita mais clara, antes marrom escuro e mais escura (os pixels diferentes do original na tampa da caixa caem de 217 para menos de 1).
- O cabo de energia pega o `fluid_logged` pela água, lava ou ar do lugar ao ser posto por um jogador (`state_for_placement` devolvia o estado dos lados antes de olhar a regra de colocação), como os outros blocos.
- O índice de componentes exatos guarda só a primeira receita `strict` de cada item, como `ComponentSensitiveInputCache.addComponentInputCache` do original (a segunda cai num mapa local e se perde).
- Duas receitas complexas (ingrediente `neoforge:components` sem `strict`, por exemplo) que casam a mesma entrada seguem a ordem de um `HashSet` com o hash de identidade da chave do id, como o `HashSet<RecipeHolder>` do original, e não a ordem da lista: o vencedor é sorteado entre as que casam, fixo durante a execução. Decisão 11 do plano.
- Uma receita que o mod recusa (campo faltando ou inválido, item ou químico desconhecido, tag sem membros, tipo de ingrediente que o Horizon não entrega) agora aparece no log com o motivo, no texto do original (`Couldn't parse data file '<id>': ...`, `Incomplete recipe detected: <id>` e `Empty tag: <tag>`), em vez de sumir.
- A parte fracionária da experiência que as máquinas soltam (`AbstractFurnaceBlockEntity.createExperience`) sai de um gerador uniforme: o `java.util.Random` de 48 bits que o `Math.random()` do Java usa (`machinery_basis_core::java_random`, conferido contra o JDK 21), semeado a cada execução. A fração do relógio de antes andava poucos nanossegundos entre dois sorteios seguidos, então a chance do ponto a mais não era a fração da receita.
- Um upgrade na mão entra no slot da janela de upgrades pelo clique do cursor (esquerdo, direito, tecla numérica e arrasto), como no original (`UpgradeInventorySlot.input`): a regra de slot do clique,
  de 2026-11-07, só conhecia a entrada, a energia e a infusão e vetava os dois slots da janela, então o upgrade ficava no cursor (o shift-clique passava por outro caminho). A entrada aceita o tipo que a
  máquina suporta (energia, silêncio, velocidade e xp nas quatro, só o silêncio no Heat Generator) e a saída não aceita nada. Medido em jogo: 36 casos por máquina nas quatro e 20 no gerador, sem diferença
  contra o rastro `slot_click_upgrade_<máquina>`.
- O upgrade na mão, com shift, sobre uma máquina (`ItemUpgrade.useOn`) instala desde o primeiro uso, como no original: antes só instalava depois de a tela daquele tipo de máquina abrir uma vez no jogo
  (o registro da janela de upgrades era feito na criação da tela), então o Crusher, o Energized Smelter, o Infuser e o Heat Generator recém-postos não aceitavam o item. O registro passou para o hook `block_entity`.
- O som das máquinas confere o alcance pelos pés do jogador (`SoundHandler.isClientPlayerInRange`, `Minecraft.player.position()`), como o original, e não pela câmera do último quadro: com os pés a 15,5 blocos
  do bloco (e a câmera a 17,1) o som começa, e a 16,5 não. A atenuação do som, essa sim, segue a câmera, como no vanilla.
- O upgrade de silêncio que sai pela janela de upgrades não avisa mais o cliente na hora (`TileComponentUpgrade.removeUpgrade` não manda o pacote): o som que ele calou segue mudo com a máquina acesa, e o
  `lit` que muda leva a contagem verdadeira, como no original. Com `enableMachineSounds` desligado, o som que já tocava segue tocando quando a máquina apaga ou o bloco sai, e o bloco novo no mesmo lugar o
  adota, também como no original (cenários `silencio_tocando_tirado`, `maquinas_off_depois` e `maquinas_off_ligando` do rastro `som_medido.jsonl`).
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
