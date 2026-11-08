# Plano de paridade 1:1

O objetivo é comportamento idêntico ao Mekanism e ao Mekanism Generators, branch 26.3 (`b2d04e166`, em
`/mnt/dados/caches/rust-mekanism-ref/mekanism`). O que já existe foi feito sob a decisão antiga de simplificar, e por isso é
ponto de partida, não entrega. Cada fase refaz sua parte a partir do Java.

## Regras de trabalho

1. **Java primeiro.** Leia a classe inteira e as que ela chama. Anote cada comportamento, inclusive ordem de execução no tick.
2. **Teste de referência antes do código.** Valores e sequências saem do Java, como `matches_the_reference_curve` do Heat
   Generator e o ejetor comparado tick a tick.
3. **Nomes e estrutura espelham o original** onde der: `Transmitter`, `DynamicNetwork`, `ConfigInfo`, `DataType`. Assim a
   comparação é linha a linha.
4. **Otimizações do original são portadas**, não trocadas por outras. Nada de cache por tempo.
5. **Bloqueio do Horizon:** tente injeção no jogo; se não der, registre na tabela abaixo e pergunte ao usuário. Cada injeção crua
   fica em [injecoes-horizon.md](injecoes-horizon.md), com o hook oficial que a eliminaria.
6. **"Igual ao original" só se diz depois de comparar.** Modelo, textura e som: o mesmo arquivo, carregado do mesmo jeito.
7. **Testes sem o jogo primeiro:** a lógica roda num mundo simulado, tick a tick, comparando estados, sem ninguém interagir.
   Cenários como rede com cabos quebrados, chunks saindo, fusão de redes e receitas pausando devem caber aí.
8. **Em mundo, só o que falta:** visual, som e a integração com o Horizon. Superplano, peças colocadas como jogador (`place`,
   não `setblock`), prints e vídeo no WhatsApp por etapa.

Onde o código fica segue a regra do `CLAUDE.md`: mecânica reaproveitável vai para a `machinery_basis`, sem nada exclusivo do
Mekanism.

## Fase 0: o que o Horizon permite

Respondida em 2026-11-05 contra o pin `f319701`, lendo o código. As rotas críticas foram provadas depois no spike, logo abaixo; as
demais seguem sem compilar, e injeção ou helper privado só valem depois de um `check` real. Um hook só no código, fora de `native-hooks.json`, dá
`E_HOOK_UNKNOWN` com `hook =` e pede `target =`, como `container_accepts`.

| Pergunta | Resposta | Caminho |
|---|---|---|
| Ponto equivalente a `ServerTickEvent.Post` | Dá por hook | `server_tick`, fase `ServerTickEnd`: depois do tick de todas as dimensões. Só o último nível está no lugar, então entre em cada nível com `enter_level`, como faz `close_idle_levels`. O hook `tick` roda antes das block entities e não serve. Em uso desde 2026-11-08 na fase `ServerTickStart` da `machinery_basis`: o `broadcastChanges` do menu lê o valor vivo e manda o pacote por jogador (decisões 5 e 8; `Game::mb_in_levels` entra em cada nível). A fase `ServerTickEnd` serviu à decisão 5 (o `server_tick_end` do Mekanism escrevia os data slots da tela) até a parte 2 dos trackers; hoje nada a usa, e ela segue sendo o equivalente do `ServerTickEvent.Post` para a rede da Fase 3 |
| `chunk_unload` antes de a block entity sair | Dá por hook | A block entity ainda é legível e gravável; os blocos do chunk não. Não dispara ao fechar o mundo |
| Equivalente a `ChunkTicketLevelUpdatedEvent` | Dá sem hook | Guardar um bool por chunk com transmissor e comparar com `tickets.levels().loaded_ticking` no fim do tick. O Java já acumula até o `onTick`. O limiar é `ChunkMap.MAX_VIEW_DISTANCE`, que vale 32 |
| Entrada no mundo | Dá por hooks | `chunk_load`, `place_block` e o primeiro `block_entity_tick`, que é o `markJoined` do Java |
| Remoção (`setRemoved`) | Dá com injeção | HEAD em `GameLevel::remove_entity`. `block_changed` só escuta, e serve para enfileirar. `native_edit_blocks` fura os dois. Descarga de chunk é caminho à parte, como no Java |
| Save shares no save | Dá com injeção | HEAD em `Game::save_level`, antes do `modified.drain`. No unload, `chunk_unload` basta |
| Quad translúcido com alfa por block entity | Dá por hook | Em `block_entity_submit`, desenhar com `native_draw_model` e reescrever os vértices por `vertices_mut`: `color` com alfa e `Kd`, `layer` 32, `SHADED_CULL_W` e `draw.light`. `native_draw_quad` força alfa 255. Precisa de `mek_liquid`, do `atlases/blocks.json` e da ordenação: ver o spike |
| Camada do modelo | Dá por dados | A faixa do braço é cutout, não translúcida: os `*_vertical.png` têm alfa só 0 ou 255. Basta a textura não opaca |
| OBJ e partes por visibilidade | Dá por dados | `neoforge:obj` com `visibility` existe. Os dois OBJ têm 24 partes e 270 quads. `.obj.mek` por pasta de conteúdo só tem teste em memória. O `.mtl` perde o `neoforge_TintIndex` |
| Estado por lado | Dá por dados | `state_properties` com template de um estado, não chorus. Quatro valores por lado, 4.096 estados por bloco, porque os `NONE_CONTIGUOUS` derivam dos outros lados. Limite real: 2^20 estados no mundo, e 22 transmissores dão 90.112. Sair do chorus perde `connects_to` |
| Forma de colisão e seleção | Dá por dados | `shapes` e `collision_shapes` por estado, máximo de 16 caixas por forma: a do Java tem 19, e sobrepondo dá 15 |
| Seleção parcial com Configurator | Dá com injeção maior | HEAD em `context_outline` devolve uma caixa só, e PUSH e PULL têm várias: é preciso injetar onde as arestas são geradas. Ver as decisões abaixo |
| Luz por estado | Dá com injeção | HEAD em `Plan::override_gameplay`. Em `set_state_properties` não, porque só roda com `state_properties` declarado e não recebe o id do bloco |
| Regra de slot no clique | Feito (2026-11-07), com duas injeções cruas | `menu_slot_rules` v1 não tem `Game`, nem a posição do bloco, nem o outro slot, e o Java olha o tanque e a entrada (`InventoryContainerSlot.mayPlace`). O crate `[api]` da `machinery_basis` implementa o hook e consulta `slot_rules` (núcleo), que `result_take_before` (Mekanism) e `menu_click_before` (Generators) enchem antes de cada clique com a regra do bloco aberto. Detalhes em `docs/injecoes-horizon.md`, "A regra de slot do clique" |
| Faces por instância | Dá com injeção | HEAD cancelável em `native_energy_at`, `native_fluid_at`, `Containers::view`, `can_take` e `native_container_accepts`. Só `native_energy_transfer` não basta |
| Comparador | Feito (2026-11-08), por hook | `has_analog_output`, `analog_output` e `native_update_output_neighbors`. O aviso pode sair dentro do `block_entity_tick`, como no Java (`TileEntityMekanism.tickServer`, `:645-652`): a primeira versão do port o enfileirava para `level_tick_post`, por cautela da leitura do código, e em jogo (2026-11-08) a chamada dentro do tick funciona, com o tile da máquina fora do mundo, porque o nível que o comparador lê fica numa tabela à parte (`COMPARATOR_LEVELS`). O hook `level_tick_post` deixou de ser usado |
| Aviso de vizinho que mudou (`neighborChanged`, `onNeighborChange`, `setPlacedBy`) | Feito (2026-11-08), com hook e duas injeções cruas | `neighbor_changed` (`listen`, `&GameLevel`: só anota quem foi avisado), a injeção `flush_signals` (HEAD de `Game::flush_spill`: o fim de cada operação lê o sinal, H34), `Game::container_changed` (HEAD, o `onNeighborChange` dos contêineres do vanilla, H35), `notify_comparators` (o comparador das máquinas do Mekanism) e `place_block` (`onAdded`). O motor de redstone é genérico (`Sim<L>`) e o `horizon-mixins` não o aceita como alvo. Decisão 7 |
| Item guardando conteúdo | Dá por hooks | `component_types`, `destroy_block`, `loot_drops` e `place_block`, com `explosion_detonate`. `loot_function` estraga o NBT, e `block_entity_data` não vale para bloco de mod. Pistão e dispenser ficam de fora |
| Som contínuo | Dá com módulo | Nada no catálogo: `[[module]]` em `crate::game` com `SoundInstance`, `Ticking::Native` e `stop_and_release`. `Ticking::Native` só aparece em teste |
| Tooltip e shift | Dá por hook | `tooltip` com `Game::has_shift_down`. A tecla N da descrição precisa de rastreio próprio |
| Tela com abas e sub-janelas | Dá por hooks | `screen_*`, tudo desenhado pelo mod, com `EditBox` disponível. Slot que anda com a janela pede HEAD em `native_screen_slot`, não testado |
| Sincronizar valor grande | Dá por hook | `native_net_send_to_player`, recebido em `net_from_server`, análogo ao `PacketUpdateContainer` |
| Energia acima de `i32` | Dá em NBT próprio | Ver as decisões abaixo |
| Tempo de queima | Dá por dados | Componente `minecraft:cooking_fuel` no item, com o provider escrito por extenso |
| Tags `c:` | Conferido, com pendência (2026-11-08) | O Horizon não traz nenhuma. Das 92 do port, 30 vêm da datagen do Mekanism e 62 não e são do NeoForge, conferidas contra o jar dele (ver a Fase 1). Faltam 143 das 173 da datagen, as que citam o que o port não tem; só 2 delas (`entity_type/creepers` e `endermen`) citam entidades do vanilla, e não se sabe se o Horizon lê tag de fluido e de `entity_type` (pendência do usuário). Tag vazia faz a receita sumir em silêncio |
| Rodar o Mekanism real para comparar | **Rodado** | `runGameTestServer` no desktop, cerca de 30 s, 70 casos e 0 falhas. `dev/referencia/testes/TickTraceTest.java` grava o estado por tick, e duas rodadas deram arquivos idênticos |

### Decisões

1. **Seleção do Configurator (usuário, 2026-11-05).** Injetar mais fundo para ter várias caixas, como o Java, sem quebrar o jogo nem
   a compatibilidade com outros mods: a injeção só age no bloco do Mekanism e com a ferramenta na mão, e o resto segue o caminho
   normal.
2. **Energia em `long` (usuário, 2026-11-05).** O Energy Cube criativo tem `Long.MAX_VALUE` e a Induction Cell básica,
   8.000.000.000, e a API do Horizon é `i32`. A energia do Mekanism vive em `i64` no NBT próprio, e `ModEnergyBuffer` fica só para
   falar com outros mods.
3. **Duas entradas com regra de slot (aberta).** Aproximação ou nova injeção, a decidir quando chegar o Combiner.
4. **Fase 1 sem pausas e a mais próxima do original (usuário, 2026-11-08).** As frentes seguem uma após a outra até a Fase 1 cumprir "Pronto significa". Quando nem uma injeção dá o 1:1, as opções são medidas contra o original e fica a mais próxima, registrada aqui com os números. O que as diretrizes não decidem fica de fora, anotado como pendente, e vai ao usuário no fim da fase. Cada escolha assim ganha uma entrada nesta seção com as opções medidas, a escolhida e o porquê; o usuário as revisa e, se discordar, muda o método.
5. **Quadro do overlay de 0,98 (fechada, 2026-11-08).** *O transporte descrito aqui foi trocado na parte 2 dos trackers (decisão 8): o `server_tick_end` e os data slots saíram, e quem manda o pacote do menu é a fase `ServerTickStart`. O que segue é o raciocínio de então, e a atualização no fim diz o que vale hoje.* O original manda os data slots da tela no `AbstractContainerMenu.broadcastChanges` (`AbstractContainerMenu.java:197-215`), que o `ServerPlayer.tick` chama (`ServerPlayer.java:598-606`) na fase das entidades de `ServerLevel.tick` (`ServerLevel.java:448-477`), antes dos block entities (`:478`); o `MekanismContainer.broadcastChanges` (`MekanismContainer.java:647-667`) lê o valor vivo de cada `ISyncableData` (`isDirty`, `getPropertyData`) e manda um `PacketUpdateContainer`. O cliente recebe então o estado do fim do tick anterior: a energia que chegou depois do tick da máquina com o erro que ela ainda não recalculou (a barra lê `handler.getLevel()` e `warningSupplier` na hora de desenhar, `GuiBar.java:56-70`). O port escrevia `entity.data` no tick da máquina, antes de a energia chegar, e o quadro nunca existia na tela dele. **Feito:** os data slots são escritos no `server_tick`, fase `ServerTickEnd` (`server_tick_end`, `mods/mekanism/native/glue.rs`), com a energia e o tanque que o tile guarda agora e o resto do objeto vivo; o `tick_lan` do tick seguinte os manda, antes dos block entities (`game.rs:7954` e `7981` do pin), como o `broadcastChanges`. Sem injeção crua: é um hook do catálogo usado com estrutura privada (`enter_level`, `levels_to_tick`, `world.block_entities`; ficha H33 em `docs/injecoes-horizon.md`; na data eram 81 injeções, 57 hooks, 30 deles com estrutura privada, e hoje são 89 injeções, 61 hooks e 30 deles com estrutura privada, conferido em 2026-11-08 contra os três `horizon-mixin.toml`). O tick da máquina não escreve mais os data slots (`TileScreen::machine_ticked` só guarda o que a janela de upgrades mostra, um tick atrás). Teste: `warnings_trace.rs` repete `warnings_overlay` com o quadro do `ServerTickEnd` no tick da chegada (t=105: energia 9.999, erro aceso, overlay; t=106: erro apagado); com o quadro escrito no tick da máquina o teste falha em t=105. Em jogo (desktop, `horizon`, build `native-f319701-24e67f906e59`, `dev/referencia/comparar_lote.py` em guiScale 2 e 3): `chamber_overlay_erro_velho` (energia 400, velocidade 8, `data merge` de energia 9999 depois dos passos, servidor parado) mostra a barra cheia listrada e a aba de avisos, com 3 pixels diferentes do original a guiScale 2 e 6 a 3 (os cantos do mundo atrás da tela, H22), e `chamber_overlay_abaixo` (9000) e `chamber_overlay_sem_erro` ficam iguais ao original; antes da mudança a barra saía vazia e sem aba. Ficou de fora, sem efeito medido: o que um pacote do jogador muda entre o `ServerTickEnd` e o `tick_lan` do tick seguinte (o Horizon o drena em `tick_world`) chega ao quadro no fim desse tick, e a posição exata pediria uma injeção em `sync_container_data` (H33). O Heat Generator não mudou: só empurra energia no próprio tick, então o quadro dele já é o do fim do tick; um cabo que puxe dele (Fase 3) pede o mesmo `server_tick_end`. **Atualização (2026-11-08, parte 2 dos trackers):** o transporte virou o pacote por jogador e o envio passou para o `ServerTickStart` (decisão 8); o `TileScreen::machine_ticked` e os data slots saíram, e o resto desta decisão vale.
6. **Lista de trackers por fonte, lida do objeto vivo (fechada, 2026-11-08).** Opções para a parte 1: (a) cada dado guardar closures que
   capturam o objeto vivo, como o Java; (b) a lista ser uma descrição (`Tracked`: fonte e classe) e o valor vir de um `TrackedSource`
   que o mod implementa sobre o objeto. Em Rust (a) pede `Rc<RefCell>` em volta de `ProgressMachine`, do `State` do gerador e do que o
   glue refaz a cada tick, e o glue do gerador não tem objeto vivo; (b) deu 5 rastros iguais ao original, tick a tick (486, 486, 486, 526 e
   378 ticks, 150 ou mais deles sujos em cada um), sem mudar nenhum tipo das máquinas. Escolhida (b): é o mesmo comportamento (o último
   valor e a regra de sujeira ficam no `ContainerSync`, por jogador, e o getter é a leitura do objeto no `Post`). Revisão: se a parte 2 achar que
   o getter precisa de estado que a fonte não tem, volta-se a (a) só para esse dado. A parte 2 (2026-11-08) não precisou: o gerador guarda o estado do fim do tick (`View`) e as máquinas leem o objeto vivo.
7. **Quando o `updatePower` lê o sinal (fechada, 2026-11-08).** O original lê `level.hasNeighborSignal(pos)` dentro do aviso (`BlockTile.neighborChanged`, `BlockTile.java:115-124`, que chama `TileEntityMekanism.updatePower`, `:1109-1117`) e guarda o resultado em `redstone` até o próximo aviso (`neighbors_*`). O hook `neighbor_changed` do pin (`native_hooks.rs:1317`, `listen`) roda com `&GameLevel` no meio de `Behaviours::update_neighbors_at`, que tem `level` e `ticks` emprestados como `&mut`: o gancho não tem como ler o sinal ali. Ler dentro do motor de redstone (`Sim::neighbor_changed_oriented`, `hooks.rs:23`) pediria um alvo genérico (`impl<'a, L: LevelAccess> Sim<'a, L>`), que o `horizon-mixins` recusa. Opções, medidas tick a tick contra o Java no simulador (`neighbors_trace.rs`; diferença de comportamento = `active`, `progress`, `can_function` e `energy` que divergem): **A**, o gancho anota e o sinal é lido no tick da máquina: `chamber_unnotified` 264 diferenças em 150 ticks e 3 máquinas (a primeira em t=101: uma operação que avisa e outra, no mesmo tick, que muda o sinal sem avisar, `src 0 on 3; src 0 off 2`, deixam `redstone = true` no Java e `false` em A), `chamber_mid_tick` 50, `chamber_notified` 8, `chamber_placed` 3, `chamber_output_signal` 4; **B**, o gancho anota e o fim da operação no mundo lê o sinal (a injeção crua `flush_signals`, HEAD de `Game::flush_spill`, ficha H34 e ficha própria `flush_signals`): 0 diferenças nos nove rastros de máquina do simulador, e em jogo (`h2_vizinhos.py`, servidor parado, `tick step 1`, o `/setblock` e o `/setblock ... strict` de verdade, a alavanca puxada por `interact`, o bloco posto por `place`) 0 diferenças nos nove cenários repetidos (Enrichment Chamber: `chamber_notified`, `chamber_unnotified`, `chamber_mid_tick`, `chamber_placed`, `chamber_output_signal`; Heat Generator: `heat_notified`, `heat_unnotified`, `heat_placed`, `heat_output_signal`). **Escolhida B**, a mais próxima do Java que o Horizon deixa; o que resta é uma operação que avisa um bloco e depois, dentro dela mesma, muda o sinal sem avisar, que nenhuma do jogo ou dos mods daqui faz. Se o Horizon passar a entregar o sinal no `neighbor_changed` (H34), a injeção sai. **O `onNeighborChange` dos blocos do vanilla** (`BlockEntity.setChanged` -> `Level.updateNeighbourForOutputSignal`): o Horizon só avisa comparadores nesse caminho, genérico e fora do alcance de um alvo cru. Opções: **A**, nada (um baú ao lado de uma máquina com `redstone` velho não o conserta: `chamber_output_signal` t=20 e `heat_output_signal` t=20 divergem); **B**, `Game::container_changed` (HEAD, injeção crua, H35), o `setChanged` dos contêineres do vanilla, com a outra metade de um baú duplo: `chamber_output_signal` dava 99 diferenças em 226 leituras sem a injeção (o baú vizinho que muda não relia o sinal) e 0 com ela, e `heat_output_signal` 0 com ela. **Escolhida B**; ficam de fora os block entities do vanilla sem contêiner (uma placa, um sino) e `Level.removeBlockEntity`, que mandam o mesmo aviso no original e que o Horizon não expõe.
8. **Quando o `broadcastChanges` roda e quem conta como jogador olhando (fechada, 2026-11-08).** O original manda o `PacketUpdateContainer` no `ServerPlayer.tick`, antes dos block entities, com o valor vivo de cada dado (`MekanismContainer.java:647-667`), e conta o jogador em `playersUsing` quando o menu nasce, o espectador também (`MekanismTileContainer.openInventory`). Uma ação de jogador (clique, pacote) roda antes do tick: `PacketUtils.ensureRunningOnSameThread` agenda o tratador no executor do servidor, que `waitUntilNextTick` esvazia (`runAllTasks`, `MinecraftServer.java:871-872`) antes de `tickServer`; `tickConnection` (`:1146`, depois dos níveis) só tica as conexões. Então um comando ou clique entre o tick T-1 e o T é visto pelos block entities e pelo `broadcastChanges` de T, e o Horizon, que o drena antes do `tick_server`, faz igual. Opções para o envio: **A**, escrever o quadro no `ServerTickEnd` e mandá-lo no tick seguinte (a decisão 5, com data slots); **B**, ler o valor vivo no `ServerTickStart` e mandar o pacote ali (**escolhida**). Medido em jogo (desktop, `horizon`, a Enrichment Chamber com a tela aberta e `data merge` de energia a intervalos aleatórios, o estado de cada dado lido nos dois hooks): com o servidor rodando, 496 pares (`ServerTickEnd` de T, `ServerTickStart` de T+1), 456 iguais e 40 diferentes, os 40 exatamente os ticks com um comando no meio; parado (`tick freeze`), 382 pares entre passadas, 356 iguais e 26 diferentes, os 26 dos comandos; nos 17 ticks de `tick step` nenhuma diferença. Ou seja, nada roda entre os dois hooks fora o que entra pela porta do servidor, e A entregaria esse comando um tick depois do original (40 de 40 e 26 de 26 comandos), B o entrega no tick em que o Java o vê. Opções para quem olha: **A**, manter o hook `mod_container_viewers` e somar os espectadores por sondagem; **B**, uma injeção crua no `tick_mod_container_viewers`, a fase das entidades (a primeira versão); **C**, a mesma conta no `server_tick` já usado, no `ServerTickStart` (**escolhida**: nenhuma injeção). Medido em jogo (15 aberturas e fechamentos da tela da Enrichment Chamber, 430 ticks, 30 transições): o número de menus abertos no bloco lido no `ServerTickStart` e no começo da fase das entidades do mesmo tick (onde B contaria) foi o mesmo nos 430 ticks (0 divergências), porque o Horizon trata o pacote do jogador antes de começar o tick; então C conta no mesmo tick que B, sem a injeção. A, pelo hook, não vê o espectador (`!g.spectator()`); medido em jogo: um menu aberto em criativo e mantido ao passar para `/gamemode spectator` (que fecha a tela do cliente e deixa o menu do servidor) fica contado com C (`playersUsing` 1 durante todo o modo) e fechar em criativo o leva a 0. O Horizon não abre a tela de um bloco para um espectador pelo `interact`. Se o menu de um jogador abre sem passar pelo `container_opened`, a sessão nasce no `broadcast` seguinte (`adopt_open_menus`).
9. **A ordem do mapa de upgrades (fechada, 2026-11-08).** O original guarda os upgrades num `Object2IntOpenHashMap<Holder<Upgrade>>` (`TileComponentUpgrade.java:63`), e a ordem de
    `getInstalledTypes()` (a lista da janela de upgrades, `:196-197`) e a dos bytes do `SyncableStreamCodec.upgradeMap` (`SyncableStreamCodec.java:54-57`, `ByteBufCodecs.map`) é a de iteração dele. A chave é um
    `Holder.Reference`, cujo `hashCode` é o do `ResourceKey` (`Holder.java:280-283`), e `ResourceKey` não sobrescreve `hashCode`: vale o de identidade, que o JVM sorteia a cada execução. Então não há ordem a reproduzir (a
    pergunta "a ordem de hash é reproduzível?" tem resposta não), e a decisão é qual ordem o port usa. Opções: **A**, a ordem do registro (a de agora: uma só, igual em toda execução, a mesma em que a janela lista os tipos
    suportados); **B**, a ordem de inserção (a ordem em que o jogador instalou); **C**, copiar a do hash (impossível). Medido (`TrackerTraceTest.upgradeOrder`, rastro `upgrade_order.jsonl`: a Enrichment Chamber com os quatro tipos que
    ela aceita, quatro execuções do `runGameTestServer`): `[xp_containment, muffling, speed, energy]`, `[speed, muffling, xp_containment, energy]`, `[muffling, xp_containment, energy, speed]` e
    `[speed, xp_containment, muffling, energy]`, quatro ordens diferentes de 24 possíveis, nenhuma a do registro (`energy, muffling, speed, xp_containment`) nem a de inserção. O que muda para o jogador: só a ordem das linhas instaladas
    na janela de upgrades, que no original é arbitrária por execução (o pacote não conta: o cliente monta o próprio `Object2IntOpenHashMap`, e a ordem de iteração dele vem do hash das chaves dele, não da ordem dos bytes; o port o lê
    de qualquer ordem na mesma contagem por tipo, `tracker_trace.rs`). **Escolhida: A**, porque é uma das ordens que o original pode mostrar, é a única estável (o comparador de telas e os testes ficam reproduzíveis) e não pede
    estado a mais; B daria uma ordem que o original nunca promete e pediria guardar a sequência no NBT. Se o usuário preferir outra, o ponto de troca é `machinery_basis_core::machine_sync::upgrade_map` e a lista da janela.

10. **O cache de `Biome.getTemperature`, que ignora o nível do mar (fechada, 2026-11-08).** O original guarda a temperatura de cada posição num mapa por bioma e por thread
   (`Biome.java:81-142`, 1.024 entradas, a mais antiga sai primeiro) com a chave `pos.asLong()`, sem o nível do mar: a mesma posição lida com dois níveis do mar devolve o primeiro valor. Rastro
   `heat_ambient_table`, linhas `aliased` (plains em (7.000.000, 150, 7.000.000), níveis 63, -63, 32 e 63 em sequência): 297,766 K nas quatro leituras do original, onde o cálculo dá 297,766,
   293,829 e 296,797. Só se vê com o mesmo bioma nas mesmas coordenadas de duas dimensões de níveis do mar diferentes (um `/fillbiome` igual no overworld e no nether, ou um data pack), e só
   enquanto a posição fica no mapa. Opções, medidas contra essas quatro leituras: **A**, sem cache (o cálculo a cada leitura): 2 das 4 iguais ao original (a primeira e a última, no nível 63), as outras
   duas 3,94 K e 0,97 K abaixo; **B**, um mapa de 1.024 entradas por bioma só com as leituras do Mekanism: 4 das 4 iguais. O mapa do original, porém, é de todo chamador do vanilla, e o do Horizon
   não é alcançável do glue: `ServerLevel.tickChunk` (`ServerLevel.java:531-533`) sorteia 3 posições por chunk que tica a cada tick com chance de 1 em 48 de chamar `tickPrecipitation`, e esta
   começa por `Biome.shouldFreeze` -> `warmEnoughToRain` -> `getTemperature`; com 121 chunks ticando num mesmo bioma são cerca de 7,6 consultas novas por tick, e as 1.024 entradas se renovam em
   cerca de 135 ticks (7 segundos). B reproduziria o original só num mundo parado, e num mundo de verdade daria o valor velho onde o original já esqueceu. **Escolhida A**: é igual ao original em
   todo caso que dura mais que alguns segundos e em todo caso de uma dimensão só, e não custa um mapa que nunca será o mesmo. Se o Horizon expuser o cache dele (ou um helper `biome_temperature`, H8), a diferença
   some. A leitura que o gerador faz do ar acontece uma vez por objeto (o cache do tile), então o caso é ainda mais estreito: dois geradores novos em dimensões diferentes, nas mesmas coordenadas,
   no mesmo bioma, criados no mesmo intervalo de segundos.
11. **A ordem das receitas complexas dos índices (fechada, 2026-11-08).** O original guarda as receitas de ingrediente complexo (as que o índice não indexa por item: `neoforge:components` sem `strict`, tipos de mod) num `HashSet<RecipeHolder>` por índice
    (`SingleInputRecipeCache.java:23`, `DoubleInputRecipeCache.java:24-26`) e `findFirstRecipe`, `findTypeBasedRecipe` e `containsInput` as percorrem nessa ordem depois do mapa. `RecipeHolder.hashCode` é `id.hashCode()` (`RecipeHolder.java:19-20`) e `ResourceKey` não sobrescreve
    `hashCode`: vale o de identidade do JVM, sorteado a cada execução e igual para o mesmo id durante ela (a chave é internada, então a recarga de dados não o muda). Só importa com mais de uma receita complexa casando a mesma entrada. Como na decisão 9, a ordem não se reproduz; a decisão é qual o port usa.
    Opções, medidas contra o rastro `recipe_components` (duas execuções do `runGameTestServer`, 12 itens com 4 receitas complexas cada e 1 item com 6; os hashes de cada id vão no rastro): **A**, a ordem da lista (a de antes): casa o vencedor do Java em 4 de 24 perguntas (17%), e é sempre a
    primeira da lista; **B**, o mesmo `HashSet` do JDK com um hash de identidade sorteado por id e por processo: com os hashes do rastro dá o vencedor em 24 de 24 perguntas, nas duas execuções (que sorteiam hashes e vencedores diferentes: 6 de 12 itens mudam de vencedor de uma execução para a outra), e
    sem eles reproduz a distribuição (cada receita vence uma fração igual dos itens: no rastro os índices vencedores foram 4, 5, 7 e 8 de 24) e a estabilidade dentro de uma execução. **Escolhida B**: é o mecanismo do original, com as mesmas propriedades (vencedor sorteado entre as que casam, fixo durante a
    execução e pela recarga de dados, outro na seguinte), e o que muda para o jogador é só qual receita vence quando um data pack põe duas complexas para a mesma entrada, o que no original também é arbitrário. Diferente da decisão 9, aqui a ordem decide a saída de uma máquina, e A daria sempre a primeira. Os testes dão os hashes
    do rastro (`SingleInputRecipeCache::with_identity_hash`). A tabela de `HashSet` mantém o tamanho a que chegou depois do `clear` (a recarga), como o `HashMap.clear`. Se o usuário preferir A (reprodutível), o ponto de troca é `recipe_cache::ComplexSet::seal`.
12. **O arquivo de cliente do port: um só, com o que algo lê (fechada, 2026-11-08).** O `Mekanism/client.toml` do original tem 8.770 bytes e 61 chaves (acessibilidade, sons, renderização, HUD, QIO e posições de janela), quase todas de coisas que o port ainda não tem, e o port já
    escrevia as posições da janela de upgrades em `config/mekanism/client.toml` (pasta em minúscula, `config/<ns>/client.toml`, uma por mod) com um escritor próprio que reescrevia o arquivo inteiro. Duas decisões. **Conteúdo.** Opções: **A**, escrever o arquivo inteiro do original (8.770 bytes) com todas as chaves, a maioria sem
    efeito; **B**, só as seções que o port lê: `sounds` com `baseVolume` e `enableMachine` (`enablePlayer` fica de fora: os sons de jogador não estão portados), 237 bytes mais as posições das janelas. O `ModConfigSpec` reescreve o arquivo inteiro quando acha um valor errado e apaga as chaves que a especificação não conhece (`ModConfigSpec.java:213-290`),
    e a especificação do original é a do mod inteiro. **Escolhida B**: uma opção que não faz nada no jogo engana quem a muda, e a seção `sounds` do port é, byte a byte, a do original sem a chave de jogador (`client_config.rs`). Cada seção entra junto de quem a lê. Se o usuário preferir A, o ponto de troca é `mekanism_rules::CLIENT_CONFIG`
    (`runtime.rs`). **Pasta e partilha.** Opções: **A**, `Mekanism/client.toml` como o original: no Linux é uma pasta nova ao lado da `mekanism/`, com duas `client.toml` do mesmo mod, e no Windows e no macOS (sistemas de arquivos que não distinguem caixa) é o mesmo arquivo que as posições das janelas, que o escritor delas apagaria a cada gravação; **B**,
    `mekanism/client.toml`, o arquivo que já existe, com os dois escritores cooperando (`Spec::rewrite` troca só a seção `sounds`, `replace_sections` troca só as `window.*`; 12 testes em `client_config.rs`). **Escolhida B**: um arquivo só, como no original, sem migrar nem duplicar nada; a diferença para o original é a caixa da pasta, que já era a do port. As posições do Heat Generator seguem no
    arquivo do namespace dele (`config/mekanismgenerators/client.toml`), enquanto no original todas moram no mesmo; fica como estava. Número usado: 12 (conferido o último em 11).

### Esperando decisão do usuário (fim da Fase 1)

Pontos que a Fase 1 deixou em aberto por serem escolha do usuário (decisão 4, 2026-11-08). Cada um tem uma linha e o link para a entrada que o detalha. Nenhum impede a próxima frente de código da Fase 1.

1. **Decisão 3 (aberta).** Duas entradas com regra de slot: aproximação ou nova injeção, a decidir quando chegar o Combiner (Fase 6). Detalhe: [Decisões, item 3](#decisões).
2. **A fase do componente de segurança.** `TileComponentSecurity` e a frequência de segurança (`TileComponentFrequency`) não estão em nenhuma fase: a Fase 5 lista a aba de segurança, mas não o componente, e o port manda `null` e `PUBLIC` nos dados 0 e 1 do menu. Detalhe: [Fase 1, `MekanismContainer`, "Diferenças que ficam"](#fases).
3. **As partículas.** A máquina acesa solta `SMOKE` e `REDSTONE` (o gerador, `SMOKE` e `FLAME`); o catálogo do Horizon não tem `animateTick`, e o único caminho (`send_particles` pelo servidor) muda quem sorteia e ignora `machineEffects`. Sem decisão. Detalhe: [Inventário, "Partículas da máquina acesa"](#inventário-do-que-diverge-hoje).
4. **As tags `c:` não copiadas e as tags de fluido e de `entity_type`.** Faltam 143 das 173 da datagen (30 de bloco, 87 de item, 23 de fluido, 3 de `entity_type`); as de item e de bloco entram com os itens (Fase 6). Não se confirmou que o Horizon leia as de fluido e as de `entity_type` que um mod traz: uma leitura rápida do pin (2026-11-08) acha o carregamento de tags de fluido (`horizon-client/src/game/debug_lines.rs:574`, só para o debug) e o de `entity_type` de mods
   (`horizon-client/src/mod_content.rs:2057-2075`), sem provar que as receitas ou o jogo as usem. Detalhe: [Fase 0, "Tags `c:`"](#fase-0-o-que-o-horizon-permite) e [Fase 1, item de receitas](#fases).
5. **A configuração de servidor do Mekanism.** O port não a lê: `UPGRADE_MULTIPLIER` é uma constante de 10 (`maxUpgradeMultiplier`), e o `heatTankCapacity` do gerador também é fixo (1.000, o padrão). O caso de mais de uma operação por tick só existe com a configuração mudada (acima de 200), então fica sem prova em jogo. Decidir se a configuração entra e em que fase. Detalhe: [Fase 1, "Receita de uso constante"](#fases) e o item do Heat Generator (inventário).
6. **As aproximações das decisões 5 a 12**, uma linha cada, para o usuário revisar o método ([Decisões](#decisões)):
   - **5** (quadro do overlay, superada em parte pela 8): sem efeito medido o que um pacote do jogador muda entre o `ServerTickEnd` e o `tick_lan`; o overlay a 3 e 6 pixels do original é o mundo atrás da tela (H22).
   - **6** (trackers por fonte): o valor vem do objeto vivo por uma fonte e não por closures, com o mesmo comportamento (5 rastros iguais).
   - **7** (quando o `updatePower` lê o sinal): a injeção `flush_signals` lê o sinal no fim da operação, não dentro do aviso; ficam de fora uma operação que avisa e depois muda o sinal sem avisar, e os block entities do vanilla sem contêiner.
   - **8** (`broadcastChanges` e jogadores olhando): o envio sai no `ServerTickStart` e a conta de quem olha sai do `server_tick`, sem injeção; o espectador por `interact` não abre a tela no Horizon.
   - **9** (ordem do mapa de upgrades): a do registro, porque a do original é sorteada a cada execução.
   - **10** (cache de `Biome.getTemperature`): sem cache, o cálculo a cada leitura, então a mesma posição em dois níveis do mar devolve valores que o original (primeiro valor) não devolveria.
   - **11** (receitas complexas dos índices): `HashSet` do JDK com hash de identidade sorteado por id e por processo; só importa com duas receitas complexas casando a mesma entrada.
   - **12** (arquivo de cliente): só as seções que o port lê (`sounds` e as posições das janelas), em `config/mekanism/client.toml`, com a pasta em minúscula.
7. **O que não dá para testar com um cliente só.** O convidado de LAN (o clique do menu, a aba de xp, o reenvio dos tipos de upgrade no `/reload`, a contagem de silêncio por pacote, o transporte de tela de depois da parte 2 dos trackers), a sincronização da armadura com o menu aberto quando outro agente a muda, o espectador por `interact`
   (o Horizon não abre a tela de um bloco para espectador) e os casos `*_reload` (o jogo não descarrega um tile sem o chunk, só o simulador os repete). E o clique do meio e o clique duplo, que o `horizon-ctl` não manda (só o esquerdo e o direito). Detalhe: [Inventário, "Clique do cursor"](#inventário-do-que-diverge-hoje) e [Fase 1, "Upgrades"](#fases).
8. **A forma de onda do som do original não foi comparada.** A regra do usuário de 2026-11-08 (o jogo de teste nunca toca nas caixas dele; o áudio vai para um sink nulo) impediu gravar o original; os `.ogg` são byte a byte os do original, então só o ganho podia diferir, e o ganho do port foi medido (razões de 0,496 e 1,058). Detalhe: [Fase 1, "Som da máquina"](#fases).
9. **A issue do Horizon do fim do port.** [docs/injecoes-horizon.md](injecoes-horizon.md) é a base dela, e só se abre ao final do port: hoje são 28 injeções cruas, 17 módulos e 30 hooks com estrutura privada (89 `[[injection]]`), e os pedidos H1 a H38.

### Spike de validação (2026-11-05)

Rodado em jogo, no pin `f319701`, numa mudança descartável. Comparado com o original pela cena de `dev/referencia/testes/CenaCabo.java`.

| Rota | Resultado |
|---|---|
| `native_energy_at`, faces por instância | Compila e funciona: o Crusher com a chave ficou em 0, sem ela recebeu 32 |
| `GameLevel::remove_entity` | Compila e funciona: lê a block entity antes de ela sair |
| `Game::save_level` | Compila e funciona: 19 cabos carimbados, valor visível no NBT |
| `Plan::override_gameplay` | Compila e funciona: Crusher com 8, os demais com 13 na tabela gerada |
| Corpo por `transmitter_small.obj.mek` em multipart | Funciona: geometria igual à do original no print, recorte do braço incluído |
| Conteúdo translúcido, alfa por instância | Funciona: `mek_liquid/energy` animado, sem aviso de modelo ausente |
| Contorno com várias caixas | Funciona: `block_highlight` cancela e `native_frame_visuals` (HEAD) põe as linhas. Só age no nosso bloco com a ferramenta. Faltam fundir as arestas internas e cair no centro quando o raio não toca nenhuma parte, como o Java |

**`Kd` do `.mtl`.** O NeoForge multiplica a cor do quad pelo `Kd` (`ObjGeometry.makeQuad`), e o `parse_mtl` do Horizon lê só `map_Kd`.
Os materiais do cabo têm `Kd 0.8`, e sem isso tudo sai 20% mais claro. RETURN em `horizon_assets::bake::obj_quad` preenchendo `color`
resolveu; a versão real precisa ler o `Kd` do arquivo e agir só nos modelos do Mekanism.

**Lateral do conteúdo.** O original desenha com `ITEM_TRANSLUCENT`, cujo shader aplica a luz direcional pela normal. Reescrever a normal
com `CULL_BACK` mantinha a cor assada: o certo é preservar a normal e pôr `SHADED_CULL_W` no `w`. Lateral: 124 contra 126 no canal verde.

**Sobreposição.** O original tem `sortOnUpload`. O Horizon não ordena esses quads e escreve profundidade, então um conteúdo desenhado
antes apagava o de trás, com manchas sem tinta. HEAD em `Game::outline_fill`, que recebe todos os vértices depois das block entities,
ordena os quads da camada `TRANSLUCENT_ITEM_OVERLAY` do fundo para a frente. Só troca esses quads entre si. Em pontos que são janela nos
dois jogos, com 3 conteúdos alinhados, canal verde: 168/169, 167/167, 179/183, 74/75, 84/87, 81/81, olhando para baixo e para cima. Atrás
de água e vidro a imagem bate no print.

**Ainda diverge, nosso contra oficial:**

- Emenda: o original esconde o centro nos trechos retos (`NONE_CONTIGUOUS`). Não implementado no spike.
- Lateral do corpo, 54 contra 57, e topo, 45 contra 47, em média de 8 quadros. A grama dá 113 contra 116: é a base da cena, fora do mod.
- Não testado: shader pack e `improvedTransparency`. O original troca para `OIT_ITEM` com ela ligada.

O código do spike ficou numa mudança à parte, sem push, para servir de referência às Fases 3 e 4: `spike: validate the critical
injections`. Na versão real, `Kd` vem do `.mtl`, e as injeções entram com o escopo pedido: só blocos do Mekanism.

## Referência e simulador

O Mekanism original compilado do commit de referência fica no desktop: `dev/referencia/README.md`. A Fase 1 começa criando na
`machinery_basis` um mundo simulado para testes, com blocos, block entities, ticks e chunks falsos atrás das mesmas regras que
o jogo chama. As sequências esperadas saem do próprio Java: um cenário em `dev/referencia/testes/` roda no `gameTestServer`, sem
ninguém interagir, e grava um `.jsonl` por tick para o teste em Rust comparar. Os cenários de chunk do `TransmitterNetworkTest`
do Mekanism servem de ponto de partida.

## Fases

Cada fase depende da anterior. Ao fechar uma fase, atualize o inventário abaixo.

**Fase 1, base de tile.** *Estado (2026-11-08): em andamento, perto do fim, não fechada (fechar é do usuário, que tem pendências). A fase foi conferida inteira contra o código e os testes do `main` em 2026-11-08. Feitos nas quatro máquinas e no Heat Generator: a base de tile,
o `CachedRecipe` com erros e avisos na tela, upgrades, redstone pelo aviso de vizinho, comparador, nome, som, luz, experiência, o clique do cursor, a capacidade de energia, o hopper, o ejetor (só por comando; a configuração é da Fase 2), os trackers do menu, o ar do gerador, as receitas por componentes e o cozimento do Smelter.
**Ainda não cumpre "Pronto significa" em três pontos que não dependem do usuário, e são a próxima frente:** (1) o `bio_fuel` sem o componente `minecraft:cooking_fuel` (100 ticks no original), que o forno e o slot de combustível do Heat Generator recusam (inventário, "`bio_fuel` sem tempo de queima"); (2) a mensagem de receita incompleta no chat
(`IncompleteRecipeScanner`, o item de receitas abaixo, "Descartes sem aviso"); (3) o carvão que não converte com ferro na entrada do Infuser, contra uma receita (`processing/iron/enriched`) que existe no conteúdo ("Fora: carbon e tin", abaixo). **Fora esses três, o que sobra espera decisão do usuário:** a seção "Esperando decisão do usuário (fim da Fase 1)", logo depois de "Decisões".
O que a fase listava e pertence a outra fase foi movido para ela em 2026-11-08 (Fases 2, 3, 5 e 6, cada uma com o que já está feito e o que falta).*
Porte de `TileEntityMekanism`, `TileEntityConfigurableMachine`, `TileEntityRecipeMachine`,
`TileEntityProgressMachine` e `CapabilityTileEntity`: ordem do tick, `setActive` com seu atraso, controle de redstone, nome,
comparador, som, upgrades, sincronização de tela (`MekanismContainer`). Inclui `CachedRecipe` completo, com `OperationTracker`,
`pausedForErrors`, `recheckAllErrors`, erros e avisos da tela, e `usedSoFar` para `per_tick_usage`.

Estado em 2026-11-05: Enrichment Chamber, Crusher, Energized Smelter e Metallurgic Infuser rodam na base nova em jogo, e a fase segue
aberta. Em `machinery_basis/core` estão os 59 módulos de `mods/machinery_basis/core/src/lib.rs` (lista conferida em 2026-11-08),
por assunto: contêineres e transferência, `transaction`, `containers`, `resource`, `resource_tank`, `tank`, `fluid`, `fluid_logged`, `chemical`,
`item_handler`, `slot_view`, `slot_rules`, `quick_move`, `hopper`, `transit`, `ejector`, `energy_conversion` e `sides`; receitas, `cached_recipe`,
`operation_tracker`, `recipe`, `recipe_cache`, `recipe_handlers`, `recipe_monitor`, `two_input_recipe`, `item_chemical_constant_recipe`, `components`,
`components_ingredient`, `identity_hash` e `warning`; tile e estado, `tile`, `tile_trackers`, `machine_sync`, `last_energy`, `menu_source`,
`container_sync`, `neighbors`, `name`, `experience`, `upgrade`, `upgrade_window`, `heat`, `temperature`, `biome_temperature`, `java_random`,
`network` e `composter`; tela e janela, `gui`, `tooltip`, `units`, `window`, `window_session`, `scroll_list`, `side_tab`, `screen_elements`,
`armor_bar`, `armor_slots`, `client_config` e `sound`; e `sim`, que agora tem chunks falsos (`sim/chunks.rs`: níveis de ticket, `toDrop`, carga e descarga na ordem do NeoForge, block
entities que somem e voltam com o chunk; `sim/open_hash.rs` porta a ordem de iteração do fastutil, conferida contra o jar real).
`mods/mekanism/tests/chamber_trace.rs` e `infuser_trace.rs` repetem tick a tick os rastros de
`dev/referencia/rastros/`, gravados por `dev/referencia/testes/MachineTraceTest.java`.

O Heat Generator tem três rastros, `heat_generator_lava_run`, `heat_generator_fuel_run` e `heat_generator_lava_sides`, e
`mods/mekanismgenerators/tests/heat_trace.rs` os repete tick a tick, igual ao bit: calor, temperatura, energia, lava, produção, queima,
combustível, a lava que acaba, o calor guardado que segue gerando, o buffer cheio, o item que não cabe e o que cabe por exato, o balde, e
as faces com lava. Dois erros que só o rastro mostrou ficaram corrigidos: a perda para o ambiente usa o ar de cada face
(`getAmbientTemperature(side)`, o do bloco vizinho) e `HeatAPI.EPSILON` é `1e-6` (um `float` ampliado), não `1e-5`. Com a regra do tile
igual ao Java, a diferença da barra de energia e do medidor de lava na comparação de telas não vem dela. O ar que o gerador lê (a temperatura do bioma com o ajuste por altura e o cache do tile) tem cinco rastros próprios, `heat_ambient_*` (2026-11-08, no Inventário do Heat Generator).

Em jogo foram comparados estado a estado `chamber_energy` (342 estados), `crusher_run` (377) e `smelter_run` (342). Em `infuser_run`, os 201
estados até o primeiro evento e, dali em diante, as transições, porque os comandos não caem no tick exato do rastro. Ejetor, redstone e salvar
e reabrir foram conferidos em jogo sem comparar cada tick. Um mundo antigo migrou o tanque para `chemical_tanks`.

O Horizon não simula inserção, então o ejetor transaciona numa cópia do vizinho, e `slot_accepts.rs`, injetado em
`game/machines/native_items.rs`, dá a regra real de cada slot, que é privada ali: é o `isValid` do `ResourceHandler`. O rastro
`chamber_eject_furnace` confere a regra sem o jogo, e em jogo forno e baú quase cheio deram o que o original dá. O ejetor ligado à configuração por instância (modo e lados) e o `TileEntityConfigurableMachine` que o guarda são da Fase 2 (movidos em 2026-11-08, lá está o que já se fez).

Falta na Fase 1 (revisto em 2026-11-08: o que sobra sem depender do usuário está no estado da fase, acima; o resto da lista é o registro do que se fez, com o que saiu para outras fases marcado "movido"):

- Receita de uso constante nas máquinas: **feito em 2026-11-07 para o Metallurgic Infuser.** `ItemChemicalConstantUsageRecipe`
  (`ItemStackConstantChemicalToObjectCachedRecipe`), o `ChemicalUsageMultiplier`, o `usedSoFar` e os handlers de fluido e de saída de
  químico estão na lib, conferidos tick a tick com o Osmium Compressor original (`compressor_run`) e agora também com o Infuser original
  (`mods/mekanism/tests/infuser_constant_trace.rs`, rastros `infuser_const_{run,dry,speed,placed_speed,reload,reload_speed,reload_without_usage,ops,ops_placed}`,
  gravados por `dev/referencia/testes/ConstantUsageTraceTest.java` com uma receita de `per_tick_usage` num data pack do próprio teste,
  `dev/referencia/testes/dados/`: gravel e 2 de redstone por tick). O Infuser escolhe por `per_tick_usage` no `createNewCachedRecipe`, como o Java
  (`ConstantUsageRecipeLookupHandler`, com o `usedSoFar` do tile, que ele grava e relê). O rastro cobre a operação inteira com químico sobrando,
  o tanque que seca no meio e volta (menos que a quantidade da receita, outro químico, entrada que some), os upgrades de velocidade e de energia
  postos e tirados no meio, a máquina posta com upgrades no item, e salvar e reler o tile (com upgrades, parada por falta de químico, com o
  progresso que não cabe mais e sem o `used_so_far` no NBT). Divergência achada: nenhuma, o port já seguia o Java. Em jogo (`horizon`, 2026-11-07,
  superplano, máquina posta por um jogador, servidor parado e `tick step`) `infuser_const_run` e `infuser_const_dry` batem em 216 pontos de 1.490
  ticks, e salvar e reabrir no meio da operação devolve progresso, tanque e `used_so_far` iguais e a operação termina com o total (`h2_infuser_const.py`,
  `h2_infuser_const_video.py`). Nenhuma receita dos arquivos do original usa a opção no Infuser (só o Osmium Compressor, a Purification Chamber e a Chemical
  Injection Chamber). **Mais de uma operação por tick (2026-11-08):** com o padrão de `maxUpgradeMultiplier` (10) o Infuser nunca passa de `200 / 10 = 20` ticks por operação (`UpgradeUtils.getOperationsPerTick`,
  `UpgradeUtils.java:95-101`: só passa de 1 com `ticksD < 1`), então o caso só existe com a configuração acima de 200. O rastro `infuser_const_ops` e `infuser_const_ops_placed` (`ConstantUsageTraceTest`, a
  configuração posta em 1.000 pelo valor em cache de `CachedIntValue` e devolvida ao fim) tem `max_operations` 1, 2 e 5 (velocidade 7 e 8, `ticksRequired` 1) e o gasto por tick multiplicando, e
  `infuser_constant_trace.rs` os repete tick a tick com `max_upgrade_multiplier` 1.000 (a mutação que faz `operations_per_tick` devolver sempre 1 derruba os dois testes). O port **não lê a configuração do Mekanism**
  (`UPGRADE_MULTIPLIER` é uma constante de 10), então em jogo só o padrão existe e o caso não é alcançável: fica sem a prova em jogo, e a configuração do mod é pendência de escopo para o usuário. **Upgrades em jogo:** o item
  de bloco do port não traz `UPGRADES` (Fase 5), então `h2_infuser_const.py placed_speed` põe a máquina, instala 4 de velocidade e 1 de energia pela janela de upgrades (o cursor no slot de entrada, 25 ticks cada) e repete
  `infuser_const_placed_speed` com `tick freeze` e `tick step`, a energia reposta a cada passo como o `refill` do Java: 51 pontos de 300 ticks, 0 diferenças (entrada, saída, tanque, progresso e `used_so_far`). A Enrichment Chamber, o Crusher e o Energized Smelter não leem o campo (o original também não: a receita deles não o tem). As três máquinas que só
  existem no original, Osmium Compressor, Purification Chamber e Chemical Injection Chamber, são pendência da Fase 6 (é onde máquina nova entra), não da Fase 1.
- `fillTankFromSlot` e `ChemicalAttributeValidator` estão na lib, com testes, e nada portado os alimenta (conferido em 2026-11-08); o `setLastTransferDirection` não foi portado. A razão e
  o que os liga estão na Fase 6 (Chemical Tank e os itens de químico) e na Fase 5 (conteúdo guardado no item).
- Índice de receitas ligado. `ElectricLookup` e `ChemicalLookup` perguntam ao `SingleInputRecipeCache` e ao `DoubleInputRecipeCache`
  (`findFirstRecipe`, com a quantidade que o `SizedIngredient.test` confere), a conversão de item em químico do slot de infusão usa
  o `SingleInputRecipeCache` do `chemical_conversion` (`findFirstRecipe` ao converter, `findTypeBasedRecipe` na regra do slot,
  `ChemicalInventorySlot.canFillOrConvert`), e as regras de inserção são as do Java: slot da Chamber com `containsRecipe`, slot de
  entrada do Infuser com `containsRecipeAB` mais `containsRecipeA`, tanque com `containsRecipeBA` e `containsRecipeB`. O hopper e o
  shift-clique do glue usam o mesmo índice. O índice é um por tipo de receita e por thread (`MekanismRecipeType.getInputCache`), e
  `data_loaded` o limpa (`clear`, via `basis::data_generation`) e manda o monitor largar a receita em cache (`invalidateCache`).
  Os ingredientes saem do Horizon assim: `Ingredient.order` (já com as tags de item resolvidas, na ordem do `HolderSet`) vira
  `IngredientShape::Types`; um filho de `CompoundIngredient` que não é simples vira `Compound`, e um ingrediente de tipo de mod, ou
  interseção e diferença com um filho assim, vira `Custom` (complexo). As tags de químico vêm de `Tags::members`. Não precisou de
  injeção. Conferido em `mods/mekanism/tests/recipe_order.rs`: as máquinas de verdade respondem como o Java nas perguntas de
  `recipe_cache.jsonl` (7 execuções de `findFirstRecipe` e 4 perguntas por tipo em que a ordem da lista erra). O `DataComponentIngredient` (`neoforge:components`, **feito em 2026-11-08**, sem o jogo): o Horizon `f319701` não o lê. `native_recipe_ingredient` aceita só texto e lista e devolve `None` para qualquer objeto, então a receita de data pack com esse ingrediente
  (e com `neoforge:compound`, `intersection`, `difference` ou de mod) saía da máquina sem aviso, e a de cozimento do jogo (Smelter) cai no `Ingredient::parse`, que manda o tipo ao hook `ingredient` e, sem dono, descarta o arquivo. O original
  lê o arquivo (`DataComponentIngredient.CODEC`: `items`, `components` no formato do `DataComponentPatch`, `strict`) e indexa o `strict` por componentes exatos e o resto como complexo (`ItemInputCache.mapIngredient`, `ItemInputCache.java:34-59`). Agora o mod lê o arquivo das
  receitas de tipo próprio (`enriching`, `crushing`, `metallurgic_infusing`, as conversões de químico e de energia): `machinery_basis_core::components_ingredient` porta o `test` (`DataComponentIngredient.java:62-87`: o valor tem de ser igual, `!tipo`
  pede que a pilha não tenha o componente, `strict` pede que nenhum componente da pilha fique fora do patch) e a chave exata (`PatchedDataComponentMap.fromPatch`, que tira do patch o que repete o padrão do item), sobre os componentes todos da pilha: o padrão
  do item (`items.json`, com o que os mods mudam) e o patch dela. Rastro `recipe_components` (285 pares de ingrediente e pilha, com `test`, `testType` e a classificação, e 6 caches, com 5 perguntas por pilha) repetido em `recipe_components_reference.rs`;
  4 mutações morrem (sem `strict`, sem a remoção `!tipo`, a ordem da lista no lugar da de hash, e a receita exata de cada item sem o bug abaixo). Em jogo (desktop, 2026-11-08, `horizon`, build `native-f319701-a1150953690b`, mundo novo com o data pack `mektest`, Enrichment Chamber com a energia posta por `data merge`, `h2_componentes.py`): 9 de 9 casos como o Java: o graveto com `custom_name` "Foo" dá o diamante da receita e o sem nome e o "Bar" não;
  o sílex com `repair_cost` 5 dá a barra de ouro e o de 6 e o sem nada não; o carvão padrão dá a esmeralda da receita `strict` feita de uma pilha (a lista dos 14 componentes padrão do `items.json` do Horizon fecha com a do NeoForge) e o carvão com nome cai na receita simples (`enriched_carbon`), como o índice exato do Java; o osso de um `neoforge:compound` dá o diamante (a receita `compound_bone`, que a primeira rodada recusava). O log
  de carga disse a causa de `bad_count`, `bad_missing_output`, `empty_tag` e `unknown_item`. Smelter (segunda rodada, build `native-f319701-35efd66d6130`, `h2_componentes.py smelter`, receitas de `minecraft:smelting` do mesmo data pack): 8 de 8 como o Java: o graveto "Foo" dá a pepita de ouro e o sem nome e o "Bar" não; o `neoforge:compound` de um sílex com `repair_cost` 5 e um osso dá a pepita de ferro para os dois e não para o sílex sem nada; `items` com `#minecraft:coals` e `repair_cost` 7 dá o cobre e o 8 não
  (a tag chega resolvida pelo módulo). Mutação em jogo: sem as duas injeções (`components-ingredient-stash` e `-items`) o mesmo teste falha em 4 casos (graveto "Foo", sílex 5, osso e carvão com tag 7), a receita volta a ser descartada. Não coberto: os componentes que o Horizon guarda de outra forma que o JSON do arquivo (um `custom_name` de texto composto, por exemplo). Cuidado de escrita: no 26.3 o `custom_name` do `data merge` é `"minecraft:custom_name":"Foo"`,
  e a forma antiga `'"Foo"'` põe as aspas no texto. **Cozimento do Smelter (feito em 2026-11-08, segunda rodada):** o `Ingredient::parse` do Horizon já lê `neoforge:compound`, `intersection` e `difference` sozinho, com os filhos pelo mesmo `parse`; só o `neoforge:components` caía no hook `ingredient`, que não tem dono e que não dá as tags de item (`items` de um
  `DataComponentIngredient` é um `HolderSet`), e a receita de cozimento inteira era descartada. Uma injeção crua (`components-ingredient-stash`, HEAD de `Ingredient::parse_object`, que tem o `Items` à mão, mais o hook `ingredient` e um módulo em `crafting.rs`; ficha H38 em `docs/injecoes-horizon.md`, 86 injeções, 17 módulos) guarda as tags para o hook resolver `items` (o Horizon guarda o JSON em `custom`).
  O mod monta então a árvore do ingrediente (`machinery_basis_core::components_ingredient::Tree`: folha de componentes, `compound`, `intersection`, `difference`, itens e ingredientes de outro mod) para o índice e para o teste, tanto nas receitas de cozimento quanto nas de tipo próprio, que também passam a ler
  `compound`, `intersection` e `difference` (antes caíam fora). Rastro `recipe_components` ampliado: 368 pares (4 ingredientes em volta de componentes, com a classificação `compound`) e o cache `compound_strict_child`. Mutações mortas: `intersection` com `any`, `difference` sem o subtraído, a classificação do composto como complexa.
  Limite que fica: o `Ingredient::test` do próprio Horizon (forno, mesa de crafting) usa só o item, porque o hook `ingredient_test` não tem dono; a mod só responde pela árvore dela (as máquinas e o Smelter). Pedido ao Horizon (H38): passar o `&Items` ao hook `ingredient`, para a injeção sair.
  Descartes sem aviso (**feito em 2026-11-08**): a receita ou conversão que o mod recusa agora diz por quê no log, como o original (`Couldn't parse data file '<id>': ...` do `SimpleJsonResourceReloadListener`, e `Incomplete recipe detected: <id>` com `Empty tag: <tag>` de
  `MekanismRecipeType.checkMyIncompleteRecipes`). O original guarda a receita de ingrediente vazio (ela não casa com nada) e o mod a deixa de fora: o resultado é o mesmo. Falta a mensagem no chat aos jogadores (`IncompleteRecipeScanner.sendMessageToPlayer`,
  `MekanismLang.RECIPE_WARNING`).
  Em jogo (desktop, 2026-11-06, `horizon2`, mundo `spike` em x=300, build `dev` de 2 min, sem erro nem aviso do compilador),
  nas quatro máquinas, com energia de um Heat Generator encostado: o hopper de cima põe no slot de entrada o item que alguma receita
  usa e deixa no hopper o que nenhuma usa (graveto) e o que o slot ocupado por outro item não aceita; o shift-clique faz o mesmo e o
  que a máquina recusa vai para o inventário; a receita certa sai na saída (Chamber: barro vira 4 bolas de barro e teia vira 9 fios;
  Crusher: pedregulho vira cascalho; Smelter: areia vira vidro e ferro bruto vira lingote; Infuser: terra com `bio` vira podzol,
  com `fungi` vira micélio, areia com `bio` vira terra, netherrack com `fungi` vira nylium carmesim). Com `bio` no tanque o slot de
  entrada recusa netherrack e aceita areia (`containsRecipeAB`); com o tanque vazio aceita o que alguma receita usa
  (`containsRecipeA`). O slot de infusão converte `bio_fuel`, cogumelo, carvão e redstone quando o tanque os aceita e deixa o carvão
  parado quando a terra está na entrada e nenhuma receita junta terra com carbono (`canFillOrConvert`). O `/reload` (um data pack do
  mundo ligado e desligado por `/datapack`) refaz o índice: o graveto passa a ser aceito e esmagado em 6 pós de osso, e depois de
  desligado a máquina para (a receita em cache cai) e o funil volta a recusar o graveto. Salvar e sair, reabrir: itens, progresso,
  energia e tanque idênticos nas quatro. Dois defeitos achados e corrigidos: a tag `c:fuels/bio` não existia no conteúdo
  (`bio_fuel` não virava `bio`; arquivo copiado da datagen do original), e o `neoforge:or` aninhado do `fungi/from_mushrooms`
  (um `or` dentro de um `or`, com lista dentro) perdia os filhos no `plain_ingredient`, então cogumelo e fungo não viravam `fungi`;
  agora a lista sai achatada, na ordem dos filhos (teste e mutação em `recipe.rs`). Em jogo, depois: o glue combinado (heat, receitas, slots) compilou em `horizon2` sem aviso, e as quatro máquinas
  processam, o Dump esvazia o tanque na hora, a janela de upgrades abre, o Heat Generator gera e o sprite do químico aparece no tanque
  (2026-11-06, build `dev` incremental de 1 min 21 s).
  **Crusher com `melon_slice`** (resolvido): o original acha uma só receita (`crushing/biofuel/melon_slice`, índice 32 de 255 na lista
  carregada, `crusher_melon.jsonl`), porque `c:crops/melon` do NeoForge lista só `minecraft:melon`. A tag escrita à mão no port listava a
  fatia, e as duas receitas casavam: o port dava 6 de bio fuel. A tag foi corrigida (`melon`), em jogo agora sai 1, e `c_tags.rs` confere
  a tag e o conjunto de receitas que casam contra o rastro. Conferidas as outras 61 tags `c:` escritas à mão contra o jar do NeoForge
  26.3.0.100 (`data/c/tags`): só as listas achatadas diferem (o port expande `#minecraft:*_ores`, `#c:sands/*` e `#c:seeds/*` em itens),
  com o mesmo conteúdo. A escolha entre duas receitas que casem a mesma entrada depende da ordem da lista, e essa ordem foi medida (2026-11-07):
  `SlotsTraceTest.recipeOrder` grava, no `runGameTestServer`, o que `MekanismRecipeType.getRecipes` devolve para cada tipo
  (`recipe_order.jsonl`: `enriching` 223, `crushing` 255, `smelting` 105, `metallurgic_infusing` 31, `chemical_conversion` 24,
  `energy_conversion` 2, mais o `smelting` e o `blasting` do vanilla). **Não é ordem de mapa de hash**: as oito listas estão na ordem do
  caminho do arquivo com `.json` e depois o namespace (`ResourceManagerRegistryLoadTask` faz `sorted(Entry.comparingByKey())` e o
  `RecipeMap` mantém a ordem por tipo), e a chave por namespace primeiro daria outra ordem em três delas. O Horizon já entrega nessa
  ordem (`registration_key` em `crafting.rs`, no pin `f319701`; o comentário "in file order" de `native_recipes_of_type` está
  desatualizado), então não houve nada a corrigir e nenhuma injeção. `recipe_registration_order.rs` confere que cada lista está nessa
  ordem, que o conteúdo do port (sem os 18 `enriching` do Mekanism Additions, que o port não tem) sai na mesma ordem relativa nos cinco
  tipos, e o único item que o original casa com duas receitas (`pitcher_pod`, em `crushing/biofuel/pitcher_pod` e
  `crushing/biofuel/seeds`, ambas dando 1 de bio fuel) escolhe a mesma. Em jogo (`horizon2`), um data pack com `aaa:crushing/zzz`
  (cascalho trocado por pedregulho) e `zzz:crushing/aaa` para o mesmo item: o Crusher usou `zzz:crushing/aaa`, a de caminho menor, que
  é a ordem do original, e não a do namespace nem a da pasta.
  **Hopper embaixo** (resolvido, ver "Slots de entrada"). Tags `c:` da datagen: das 173 de `mekanism` e `mekanismgenerators`, 30 estão
  no port (conferido em 2026-11-08 pelo caminho do arquivo: 29 de item, que são `alloys` e as três de `alloys/*`, `circuits` e as quatro de `circuits/*`, 15 de `dusts/*`, `fuels/bio`,
  `ingots/osmium`, `ingots/steel`, `ores/osmium` e `raw_materials/osmium`, e `c:ores/osmium` de bloco); as 143 que faltam (30 de bloco, 87 de item, 23 de fluido e 3 de `entity_type`) citam item, bloco ou
  fluido que o port não tem (pós, lingotes, pepitas e blocos de bronze, chumbo, estanho, urânio e fluorita; baldes; pellets; baús e barris
  pessoais; ferramentas Meka; `#mekanism:transmitters/buffered`), e não se edita uma tag do original para listar menos. Fluid e
  entity_type não foram copiados: não se confirmou que o Horizon os leia. Movido para a Fase 6 (2026-11-08): comparar o que o Horizon descarta em silêncio (entrada ou `#tag` desconhecida) com cada receita portada quando esses itens chegarem, e as 143 tags de item e de bloco, que entram junto dos itens; as de fluido e de `entity_type` esperam saber se o Horizon as lê ("Esperando decisão do usuário (fim da Fase 1)").
- Slots de entrada. `InputInventorySlot` passa `ConstantPredicates.notExternal()` como `canExtract` (`InputInventorySlot.java`, o
  construtor protegido chama `super(capacity, notExternal(), insertPredicate, ..)`), e `BasicResourceContainer
  .isCurrentValidForExtraction` aplica o predicado com o `AutomationType` de quem pede: o hopper e o cano (`External`) não tiram a
  entrada, o jogador (`Manual`) e a máquina (`Internal`) tiram. A lib usava `Access::Any` nos slots de entrada da Chamber, do
  Crusher, do Smelter e do Infuser; agora usa `Access::NotExternal`. Teste em `mods/mekanism/tests/input_slot_access.rs`; trocar
  `NotExternal` por `Any` em qualquer dos dois slots o faz falhar (mutação feita, uma de cada vez). Sem rastro do Java: o
  `mekanism-ref` estava fora de uso por regra, então a prova é a leitura do código. Os slots de infusão e de energia agora seguem `fillOrConvert` (`ChemicalInventorySlot`
  e `EnergyInventorySlot`): tiram com `!external || !canFillOrConvert(item)` e inserem com `internal || canFillOrConvert(item)`, na
  lib (`SlotChecks::fill_or_convert`, com `extract_by` e `insert_by`). `slot_rules.rs` repete as 56 perguntas do slot de infusão
  (4 tanques, 2 estados da entrada, 7 itens, as três automações, inserir e tirar) e as 7 do de energia, por máquina, gravadas do
  original por `SlotsTraceTest.java` (`slots_*.jsonl`); trocar a regra de tirar, a de inserir, a conversão de energia ou o slot de
  infusão por `Any` o faz falhar (mutação feita, uma de cada vez).
  **Hopper embaixo** (`TileComponentConfig.setupItemIOConfig`, `setupItemIOExtraConfig` e `AttachedSideConfig`,
  `LightConfigInfo.MACHINE` e `EXTRA_MACHINE`): a máquina posta pelo item tem todas as faces em `INPUT`, a direita em `OUTPUT` e as
  costas em `ENERGY`; no Infuser a base é `EXTRA` (o slot de infusão). O hopper só puxa do bloco acima dele, então só a base
  conta. `INPUT` é `InventorySlotInfo(true, false, ..)`: não deixa tirar. Por isso no original o hopper de baixo não leva nada da
  Chamber, do Crusher nem do Smelter (a saída é pela direita), e o port já fazia o mesmo. No Infuser a base é
  `InventorySlotInfo(true, true, infusão)`, e o hopper de baixo tira do slot de infusão o que `fillOrConvert` deixa um `EXTERNAL`
  tirar (graveto; bio fuel com o tanque cheio de bio ou com fungi), e o port não tirava: `item_slots` dava `outputs: &[]`. Corrigido
  (`item_slots`, `sides` e o `can_give` do `tick_hoppers`). Rastro do Java com o hopper embaixo, e um em cada face apontando para a
  máquina com um item de cada tipo, nas quatro máquinas, posta como um jogador a poria; `slot_rules.rs` repete as seis faces e as
  fases de baixo. Em jogo (`horizon2`): o hopper embaixo do Infuser leva o graveto, não leva o cogumelo que o tanque converte, leva o
  bio fuel quando o tanque tem fungi, e embaixo da Chamber não leva nada. Movido para a Fase 2 em 2026-11-08 (lá está o que já se fez e o que falta): uma máquina posta por `setblock` ficaria
  com todos os lados em `NONE` no original (`ConfigInfo` nasce em `NONE` e só `applyImplicitComponents` aplica o padrão do item;
  lido no código, não gravado), e o port trata toda máquina como se viesse do item; e a configuração por instância (a tela de lados, o Configurator).
  **Slot de energia** (feito, 2026-11-07): `energy_conversion` e `EnergyInventorySlot.fillContainerOrConvert`. A lib tem
  `energy_conversion` (`EnergyConversion`, o `getInputCache` `SingleItem` do tipo, e `fill_container_or_convert`), chamada no tick
  da máquina antes da receita, como `onUpdateServer`; as receitas `redstone` (10.000) e `redstone_block` (90.000) são as duas da datagen,
  copiadas byte a byte (nenhuma ficou de fora: as tags `c:dusts/redstone` e `c:storage_blocks/redstone` existem no port). As costas
  (`DataType.ENERGY`, `InventorySlotInfo(true, true, ..)`) agora alcançam o slot de energia, e o hopper pelas costas e o shift-click do
  menu seguem `canFillOrConvert`. **O original só converte se o contêiner aceita tudo o que a receita dá**
  (`insert(output, .., MANUAL) == output`), e a capacidade das quatro máquinas depende de como elas chegaram ao mundo (item "Capacidade de energia" abaixo): 20 (4 ticks
  de consumo) numa posta por `setblock` ou lida do disco sem upgrade, 1.000 (1.600 no Crusher) numa posta por um jogador, e com upgrades de
  velocidade de 1.000 a 10.000 (16.000 no Crusher; `upgrade_table.jsonl`, 324 combinações). A `redstone` (10.000) só converte onde a capacidade
  chega a 10.000 com o contêiner vazio: velocidade 8 na Chamber, no Smelter e no Infuser, e velocidade 7 ou 8 (com 0 ou 1 de energia, nos pares de
  12.000 e 16.000) no Crusher; o `redstone_block` (90.000) nunca, o maior é 16.000. Na máquina de um jogador sem upgrade (1.000) a `redstone` entra pelas costas, fica no slot e a energia
  não sobe, no original (o hopper pelas costas do `SlotsTraceTest`, que põe a máquina como um jogador, deixa `[redstone, 1]` no slot por 45
  ticks) e no port. Em jogo (`horizon2`): o hopper pelas costas da Chamber põe os 8 `redstone` no slot de
  energia e o graveto fica no hopper; um hopper no lado esquerdo põe o `redstone` no slot de entrada (é entrada de enriquecimento, como
  no original); no Infuser o `redstone_block` entra pelas costas e o segundo item (`redstone`) espera, porque o slot guarda um tipo;
  o shift-click de 3 `redstone_block` com o menu aberto soma ao slot de energia (2 para 5); Na máquina de um jogador sem upgrade a energia fica sem subir. Rastros do Java: `chamber_energy_conversion` (recusa
  com 20, um item por tick com espaço, exatamente o espaço que há, o bloco de 90.000, item sem receita), `chamber_energy_conversion_running`
  (a conversão vem antes da receita no mesmo tick) e `infuser_energy_conversion` (energia e infusão no mesmo tick), com a capacidade
  posta por `setMaxEnergy`; o port os repete tick a tick. Movidos para a Fase 6 em 2026-11-08: `EitherSide`, `Triple` e `Rotary`, sem máquina portada que os use, e os limites de taxa. O `HashSet` das receitas
  complexas (**feito em 2026-11-08**, decisão 11): o `hashCode` de um `RecipeHolder` é o do `ResourceKey` do id (`RecipeHolder.java:19-20`), o de identidade do JVM, que muda a cada execução; o port faz o mesmo `HashSet` do JDK (`sim::java_hash`) com um hash de identidade
  sorteado por id e por processo (`machinery_basis_core::identity_hash`), e as caches de um e de dois ingredientes iteram as complexas nessa ordem. Rastro `recipe_components` (duas execuções do `runGameTestServer`, 48 receitas em 12 itens e 6 em 1, com o `System.identityHashCode` de cada id): o
  port com os hashes do rastro dá a receita certa em 24 de 24 perguntas e a ordem da lista em 4 de 24.
- Chegada dos itens no Infuser (feito, 2026-11-07). Relato: com `dirt` na entrada e `bio_fuel` no slot de infusão ao mesmo tempo, com o
  tanque vazio, "nada converte nem processa"; com a conversão antes da entrada funciona. **Não é defeito do port.** Rastro do Java
  `infuser_arrival.jsonl` (`ArrivalTraceTest`): tanque vazio, o item da receita e o item que vira o químico dela chegam por `insert` com
  `MANUAL` em três ordens (entrada primeiro, conversão primeiro, os dois no mesmo tick), 13 pares (bio com terra, areia e argila; fungi com terra e
  nylium; redstone, diamond, gold e refined_obsidian, cada um com o pó e o `enriched`/bloco), 981 ticks. No original o slot de infusão
  aceita o item com a terra na entrada (a conversão sai 1 tick depois: tanque 5 e infusão 1, depois tanque 10 e progresso 1), nas três
  ordens e nos 13 pares. `infuser_arrival_trace.rs` repete tudo tick a tick (estado, `MEKTRACE` e quanto cada slot aceitou) com as receitas e as
  conversões lidas dos arquivos do port, e **em jogo** (`horizon2`, `h2_infuser_arrival.py`, shift-clique com o servidor parado) as 39 fases
  deram o mesmo estado a cada tick. **A causa** é como o cenário foi montado: `container X Y Z dirt` põe no primeiro slot livre, que no Infuser
  é o de infusão (slot 0), e o `bio_fuel` seguinte vai para a entrada (slot 1), e ali nada converte, no original também; e um
  `data merge ... Items:[terra no slot 1, bio_fuel no slot 0]` perde o `bio_fuel`, porque `decode_mod_block_entity` do Horizon só
  decodifica a lista que reescreve igual (slots em ordem). Com a conversão primeiro o `bio_fuel` ocupa o slot 0 e funciona. Mutações
  que o rastro mata: o tanque recusar o químico com item na entrada (`containsRecipeBA` com item), a conversão depois da receita no tick, e
  a quantidade convertida; o slot de infusão aceitar químico diferente do que o tanque guarda não passa por este rastro (ele começa com o tanque
  vazio) e é morto por `slot_rules.rs`. **Fora:** carbon e tin, que o rastro não cobre porque a conta dos 13 pares usa só as 19 receitas da pasta
  `recipe/metallurgic_infusing`. **Linha velha corrigida em 2026-11-08 (frente do plano):** o texto de antes dizia que o original tem 31 receitas e o port 19 e que por isso o `coal` não entra no slot de infusão. O conteúdo
  do port guarda as 30 receitas de tipo `mekanism:metallurgic_infusing` do `mekanism` (as 19 da pasta e 11 em `control_circuit/` e `processing/`, contadas por `type` em `tests/common/mod.rs`, `infuser_recipes_by_type`; o jogo lê o `type`, não a pasta),
  e a 31ª do original, `mekanismgenerators:hohlraum`, é do Generators e o port não a tem. Então `processing/iron/enriched` (ferro com `#mekanism:carbon`) existe no port, e o
  `chemical_conversion` tem 19 conversões contra 24 do original (as de estanho e as de itens que o port não tem, Fase 6). **Em aberto, sem depender do usuário (Fase 1):** a medição em jogo de 2026-11-08 (item Som da máquina, "Também visto") viu o `coal` não converter com `iron_ingot` na entrada
  e a atribuiu a essa receita faltar; como o arquivo existe, a causa não é essa e não foi achada. Pontas a investigar: se a receita é carregada (`#c:ingots/iron` e `#mekanism:carbon` resolvem), e o `containsRecipeAB` do tanque com o ferro na entrada.
- Upgrades: a aba e a janela estão portadas nas quatro máquinas. Em jogo (desktop, 2026-11-06), nas quatro: abrir, instalar (20 ticks, vários de uma vez até o máximo) e remover; saída cheia (64) bloqueia a remoção; saída com outro tipo bloqueia; a lista rola com 8 tipos; o Dump atrás da janela no Infuser não recebe o clique (sem a janela recebe); mundo antigo (sem `component_upgrade` e sem os slots 3 e 4) abre sem perder item; salvar e reabrir devolve os upgrades e os dois slots cheios; jogador que cai com a janela aberta não deixa seleção (o que reentra faz shift-clique normal); `upgradeTicks` anda com a tela fechada e o progresso aparece ao abrir; o convidado vê os tipos (sincronia por rede; **medido em 2026-11-06, antes de o transporte da tela trocar para o pacote por jogador (trackers, parte 2, 2026-11-08), e o caso não foi refeito depois**). Pixel contra o oficial (guiScale 2 e 3, quatro máquinas, sem tolerância e sem máscara além da das abas): zero nos elementos do mod, exceto os ícones de item em escala (H25: "Supported:" e linha da lista), o mundo atrás da janela (H22) e, com o servidor parado, o Dump que o port só aplica no tick seguinte; a ordem da lista com mais de um tipo é de hash por identidade no original, então a comparação vale com um tipo (casos `_um_`). Não testado em jogo: o reenvio dos tipos ao convidado no `/reload`, e Save and Quit com a janela aberta. Estão portados e conferidos contra rastros do original (2026-11-06): o tooltip do item (`upgrade_tooltip`, os 8 tipos, linha a linha), a receita e a tag `muffling_center` do `muffling` e o efeito dele no som (`chamber_muffling`), e o `xp_containment` (`chamber_xp_containment`, `chamber_xp_vanilla`, `chamber_xp_broken`, repetidos por `effects_trace.rs`). A aba de XP (`GuiXpContainmentTab`) está portada, veja o item dela abaixo. Os tipos que
  nenhuma máquina portada aceita são da Fase 6 (movidos em 2026-11-08): entram com a máquina que os aceita. Em jogo foram conferidos a receita, o shift-clique, reabrir e a capacidade por instância em
  `native_energy_at` (1000, 600 com 2 de energia, 400 com 4, e a vizinha não muda). A instalação de 20 ticks só foi comparada sem o jogo.
  **Divergência achada e corrigida (feito, 2026-11-08):** o upgrade na mão com shift sobre o bloco (`ItemUpgrade.useOn`, `ItemUpgrade.java:28-48`) só instalava depois que a tela daquele tipo de máquina abriu
  ao menos uma vez no processo do jogo: a spec da janela (`mb_window_register`) era registrada em `screen()` (`glue.rs`, quando a tela se cria), e `adder_of` não achava o bloco antes disso. Medido em jogo antes do conserto: o Crusher, o Energized
  Smelter, o Infuser e o Heat Generator recém-postos não instalaram, e o Crusher instalou 2 de velocidade logo depois de o menu dele abrir e fechar; a Enrichment Chamber instalou porque a tela dela já tinha aberto. **Conserto:** o registro passou
  para o hook `block_entity` de cada mod (o jogo o pergunta ao criar o tile, antes de qualquer uso do item, também na carga de um mundo salvo), idempotente como antes; sem injeção nova, a contagem não muda. Teste sem o jogo:
  `mods/mekanism/tests/window_registration.rs` lê o glue dos dois mods e exige a chamada em `block_entity` e fora de `screen` (a mutação que volta o registro para `screen()` o derruba). Em jogo (`h2_upgrade_use.py`, `horizon`, build `native-f319701-669af0816d85`, jogo recém-aberto e nenhuma tela aberta, as máquinas postas por um jogador que passa a sobrevivente): o Crusher, o Energized Smelter, o Infuser e a Chamber instalam 8 de 20 de velocidade no primeiro uso e deixam 12 na mão, e o Heat Generator instala 1 de 3 de silêncio e deixa 2, como `ItemUpgrade.useOn` (`addUpgrades` até o máximo, `stack.consume(added, player)`).
  **Salvar e sair com a janela aberta (2026-11-08).** No original o tile é gravado com o menu ainda aberto (`PlayerList.remove` salva o jogador antes de `Player.remove` fechar o contêiner,
  `PlayerList.java:316-336` e `Player.java:1311-1317`) e o mundo reaberto o lê; `MekanismContainer.removed` só limpa a janela selecionada (`MekanismContainer.java:191-198`), então nada do tile depende dela.
  Rastro `upgrade_quit_{enrichment_chamber,metallurgic_infuser,heat_generator}` (`UpgradeQuitTraceTest`, a janela selecionada pelos pacotes reais, upgrades no slot da janela instalando, a máquina andando,
  `quit` = `saveWithFullMetadata`, `menu.removed`, `BlockEntity.loadStatic`; 5, 2 e 2 saídas): os itens dos dois slots de upgrade, os instalados, a energia, a entrada, a saída, o tanque e o progresso voltam, e
  `upgradeTicks` e `canCheckUpgrades` não são gravados (`TileComponentUpgrade.serialize`, `:256`), então a instalação **recomeça dos 20 ticks** depois de reabrir (`upgrade_ticks` 8 e 0 no tick do `quit`, e o upgrade entra
  20 ticks depois; o que passa do máximo do tipo, como 32 de energia ou 2 de silêncio, fica no slot). O port já fazia assim: `upgrade_quit.rs` (a Chamber, com 5 saídas, e o Infuser com o tanque) e
  `the_generator_keeps_the_upgrade_slot_and_restarts_the_install_after_save_and_quit` repetem os três rastros tick a tick, e duas mutações os matam (o `restore` sem o slot de entrada, e `UpgradeTransient::default` com
  `can_check: false`). **Em jogo** (`h2_quit.py antes|depois`, `horizon`, build `native-f319701-2c628113bfe2`, o servidor parado, duas rodadas): três máquinas com upgrades no slot da janela (30 de velocidade no Infuser parado por falta de
  energia, com progresso 48, energia 3 e tanque 500; 3 de silêncio no Heat Generator; 40 de energia na Chamber andando, com a janela dela aberta na hora do `horizon-ctl quit`); o mundo reaberto tem instalados 8, 1 e 8 e sobram 22, 2 e 32 no
  slot, o Infuser com progresso, energia e tanque exatos (48, 3 e 500) e a Chamber com o progresso adiante (60 para 119 e 126) e a energia gasta: nada se perdeu nem duplicou. O jogo não mostra a instalação recomeçar dos 20 ticks (o mundo reaberto anda antes da
  primeira leitura): isso é do rastro e do Rust.
- Capacidade de energia, consumo e tempo de uma máquina posta por um jogador (2026-11-07). **Causa no Java:** `AttributeEnergy.getStorage()` dá o
  contêiner 4 ticks de consumo (20 na Chamber, no Smelter e no Infuser; 32 no Crusher), e é só o construtor. `MachineEnergyContainer.updateMaxEnergy`
  (chamado por `recalculateUpgrades`) troca isso, com a velocidade aceita, por `energyPerTick * max(4, ticksRequired)`: 1.000 (1.600). Quem chama
  `recalculateUpgrades` sem upgrade nenhum é `TileEntityMekanism.applyImplicitComponents`, que roda quando um item põe o bloco (o item de bloco
  traz `UPGRADES = UpgradeAware.EMPTY`) e na leitura do NBT *se* o filho `component_upgrade` está lá (`ITileComponent.read` é `readChild`, e o `write`
  apaga o filho sem upgrade nem item). Medido no Java (`PlacedTraceTest`, `placed_<máquina>.jsonl`, as 4 máquinas e o Heat Generator): `setBlock`
  (gabarito, comando, estrutura) 20; `useOn` do item 1.000 (1.600); `useOn` com upgrades no item, os 9 pares medidos, igual à `upgrade_table`;
  **o tile lido do disco** (`BlockEntity.loadStatic` do NBT salvo) **volta a 20 se não tinha upgrade nem item no slot de upgrade**, também o que um jogador pôs, e fica com a
  capacidade dos upgrades quando tinha. O Heat Generator é `BasicEnergyContainer.output`, nenhum `recalculateUpgrades` o toca: 240 de todos os jeitos.
  **O que o port fazia:** `ProgressMachine::restore` recalculava os upgrades sempre, então toda máquina nascia com 1.000, inclusive a do `setblock` e a
  lida do disco. **Agora:** `restore` só recalcula com o filho `component_upgrade` (`UpgradeSaved::is_empty`), o item que pôs a máquina recalcula
  (`apply_item_components`, chamado no primeiro tick de uma máquina marcada por `place_block`, `PLACED` no glue), e um objeto vivo que o glue refaz sem
  o tile mudar (a varredura de `STALE_TICKS`, o tile é o mesmo se o Horizon ainda guarda os `data` que o objeto velho escreveu) leva o `capacity_recalculated`
  do objeto velho (`SWEPT`): sem isso a máquina que o jogador deixou de ver por 5 segundos voltaria a 20, e o Java não a perde. Em jogo (`horizon2`, `h2_placed.py`, energia entrando por um Heat Generator encostado, máquina parada e sem entrada): posta por um jogador, a Chamber, o Smelter e o Infuser enchem até 1.000 e o
  Crusher passa de 1.276 (o gerador esgotou os 64 carvões antes de 1.600; com mais carvão, 1.600); posta por `setblock`, 20, 20, 20 e 32. Depois de o chunk descarregar de verdade (o jogador a 4.000 blocos, `loaded: false`, geradores tirados e energia zerada) e voltar,
  a do jogador sem upgrade fica em 20, 32, 20 e 20 (e a energia que ela guardava, 727 a 274, não sobe mais), e a Chamber com 1 de velocidade segue acima de 1.000 (1.341 de capacidade, como o Java). Com o chunk carregado mas
  fora da distância de simulação (140 blocos, simulação 5, 600 ticks, outra máquina andando lá) a capacidade das quatro volta inteira (1.000, 1.000, 1.000 e 1.276 do Crusher); **provado em jogo (2026-11-08)** que a varredura dos objetos vivos
  correu e que o caminho de `SWEPT` leva a capacidade de volta: no mesmo roteiro (`h2_placed.py gap`, `horizon`, build de teste com um `eprintln` de diagnóstico que não entrou no commit,
  oito máquinas, quatro postas por um jogador e quatro por `setblock`, e uma Enrichment Chamber nova andando a 140 blocos, que é quem dispara a varredura) o log mostra a varredura no tick 24.711
  (`sweep live=9 stale=8`), cada um dos oito objetos guardado em `SWEPT` com o `capacity_recalculated` dele (`true` nas quatro do jogador, `false` nas quatro do `setblock`) e, quando o jogador
  voltou, no tick 25.183, cada objeto novo tirando o valor do seu antecessor (`SWEPT take ... -> Some(true)` ou `Some(false)`); a capacidade depois disso foi a do rastro `placed_*` do Java: 1.000, 1.600
  (com o gerador reabastecido: na primeira leitura o carvão tinha acabado e o Crusher marcou 1.276), 1.000 e 1.000 nas postas por jogador, e 20, 32, 20 e 20 nas do `setblock`. Sem o `SWEPT` o objeto novo nasceria
  com `recalculated_before = false` e as quatro do jogador voltariam a 20 (32) (pela leitura do código: a mutação não foi rodada em jogo). Uma operação de 200 ticks de uma Chamber do jogador, energia posta de uma vez e servidor parado: 100 ticks depois
  energia 500 e progresso 100, aos 200 energia 0, progresso 0, saída `clay_ball` 4 e a entrada em 4 (a segunda operação não começa), como `placed_run_enrichment_chamber_plain`. **Consumo e tempo** de uma máquina posta por um jogador, com uma só carga de energia
  (`placed_run_*`, 233 ticks cada, plain e velocidade 2, as 4 máquinas): a carga é a capacidade (1.000, 1.792 com velocidade 2, 1.600 e 2.912 no Crusher), a operação gasta
  tudo em 200 (112) ticks e a segunda não começa; `placed_run.rs` repete cada tick (energia, progresso, ticks, consumo, saída). **O que a conclusão
  anterior errava:** "no máximo 1.000 com upgrades" (a tabela vai a 16.000) e "a `redstone` nunca converte nas 4 máquinas": converte com velocidade 8
  (Chamber, Smelter, Infuser; 7 e 8 no Crusher) e o contêiner vazio; `placed_capacity.rs` confere, nas 324 linhas, que o port converte exatamente onde o Java chega a 10.000.
  **Fica de fora (Fase 5, conteúdo guardado no item; movido em 2026-11-08):** o item que guarda os upgrades do bloco quebrado (o `UPGRADES` do item; o port solta o conteúdo ao quebrar), então a linha `use_on` com
  upgrades no item só é repetida por `apply_item_components` com o mapa na mão. **A primeira capacidade (feito em 2026-11-08):** a máquina posta por um jogador tinha a do bloco (20) até o primeiro tick
  dela, e o Java já tem a do item desde o `setPlacedBy` (`BlockMekanism.java:179-196` -> `applyImplicitComponents`): um gerador ou cabo que a enche no tick da colocação via 20 e o Java vê 1.000. Rastro `placed_fed_<máquina>` (`PlacedTraceTest.fed`:
  um Heat Generator cheio, 240, de frente para a máquina, que um jogador põe com cinco pares de upgrade no item e em dois momentos do tick, `pre` (antes dos block entities, como uma ação de jogador, e o gerador tica depois no mesmo tick) e
  `step` (depois deles, e o gerador só empurra no tick seguinte)): a capacidade e a energia lidas no tick da colocação e nos sete seguintes. O Java dá `max_energy` 1.000 (1.792 com velocidade 2, 1.600 e 2.912 no Crusher) já na
  colocação, e em `pre` os 240 entram de uma vez no tick dela. **Agora** `place_block` (`PLACED`) faz `placed_capacity`: nasce o objeto da máquina como o do primeiro tick (`born`, com `applyImplicitComponents`) e publica a capacidade
  que o Horizon lê por instância (`set_energy_capacity`); o primeiro tick só a confirma. `placed_fed.rs` repete as 10 colocações de cada máquina (energia do gerador, da máquina e capacidade a cada tick) e a mutação de
  deixar a capacidade do bloco até o primeiro tick morre nas quatro. O item do port não carrega os upgrades do bloco quebrado, então só o par (0, 0) se repete em jogo.
- Upgrade sem tipo: o `select` do modelo já cai em `ItemModelNode::Select { fallback: None }`, que o Horizon resolve com `LayerContent::Missing`, o `missingItemModel` do original. O que aparece vazio é o slot do inventário: em jogo (2026-11-06) o upgrade sem componente some da barra de atalhos, tanto com esse caminho quanto com um `Model` explícito de `minecraft:builtin/missing`. É do desenho de ícone do Horizon (a camada `Missing` não sai no slot) e fica registrado como pedido ao renderer; o código do mod não muda.
- Som da máquina (`TileEntityMekanism.updateSound`, `SoundHandler.TileTickableSound`): portado nas quatro máquinas (módulo `tile-sound`, H19) e conferido contra o cliente oficial em
  2026-11-08, sem ouvir, pelo estado do motor de som a cada tick. **O que o original faz** (`CenaSomMedida.java`, rastro `som_medido.jsonl`, 27 cenários, 4.044 linhas que valem 8.394 ticks do cliente): o
  evento de cada som de tile é `mekanism:tile.machine.<máquina>`, categoria `block` (`SoundSource.BLOCKS`), tom 1, atenuação `LINEAR`, em laço, atraso 0, não relativo, `canStartSilent`, a
  distância de atenuação de 16 blocos (`sounds.json` sem `attenuation_distance`) e a posição no centro do bloco; o volume é `baseVolume * getVolumeFactor()` (1 sem upgrade de silêncio, 0 com o
  único que cabe, `max` 1) e vale desde o tick em que a máquina acende até o tick em que apaga (a máquina que fica sem energia apaga o `lit` e o som para no mesmo tick). O `.ogg` e a entrada
  no `sounds.json` de cada um dos 8 sons (4 das máquinas e 4 de Natal) são byte a byte os do original (`sound_assets.rs`). **Configuração de cliente (feito, 2026-11-08):** `baseSoundVolume`
  (`SoundHandler.java:308`, lido na criação da instância: um som que já toca não muda) e `enableMachineSounds` (`TileEntityMekanism.java:1426`, lido a cada tick) vêm de `config/mekanism/client.toml`
  (no original `Mekanism/client.toml`, `MekanismConfigHelper.registerConfig`: `Mekanism.MOD_NAME` + `/` + `client` + `.toml`; o `mekanism-client.toml` do pedido não é o nome do original; a pasta em minúscula é a que o port já usava para as posições das janelas, decisão 12), registrado pelo Horizon (`native_register_configs`, API 397) com o texto que
  o NightConfig escreve, os mesmos comentários, `# Default:` e `# Range:` (`ClientConfig.java:74-81`; `mekanism_client.toml` é o arquivo que o cliente oficial escreveu sozinho, e `client_config.rs` confere o texto do port contra
  a seção `sounds` dele). O jogo relê o arquivo quando muda; um número fora de 0 a 10 vai para o limite, um valor de outro tipo ou ausente para o padrão, e o arquivo é escrito de novo, como o `ModConfigSpec` (só a seção `sounds`: as posições das janelas, que a janela de upgrades escreve no mesmo arquivo, ficam). A
  regra genérica está em `machinery_basis_core::client_config`. O arquivo do port só tem as chaves que algo lê (decisão 12): `enablePlayer` e as outras seções entram com os sons de jogador, a HUD etc. **Distância (feito, 2026-11-08):** a linha do
  plano estava certa, o original usa os PÉS (`SoundHandler.isClientPlayerInRange`, `:229-250`, `Minecraft.getInstance().player.position()`, na criação e a cada intervalo de 20 a 39 ticks), enquanto a atenuação do
  `OpenAL` usa a câmera (o ouvinte, `SoundEngine`); o port usava a câmera do último quadro nos dois e passou a usar `native_player()`. Medido: com os pés a 15,5 blocos acima do centro (a câmera a 17,1) o
  som começa, e a 16,5 não (`pes_perto` e `pes_longe`). **Quatro comportamentos do original que parecem bug e agora o port repete** (veja a seção própria): o som segue tocando com `enableMachineSounds`
  desligado (ele nem para quando a máquina apaga nem quando o bloco sai, e um bloco novo no mesmo lugar o adota); o upgrade de silêncio que sai com a máquina acesa não avisa o cliente (só o `lit` que muda leva
  a contagem verdadeira); `baseSoundVolume` só vale para um som novo; e o som sem tile volta ao volume cheio no próximo ciclo. **Teste:** `sound_trace.rs` repete os 27 cenários tick a tick contra o rastro (o volume da
  instância de cada tick do original, com os pés, os upgrades, o relógio parado, `baseSoundVolume` e `enableMachineSounds` que o original tinha), `client_config.rs` a configuração, `sound_assets.rs` os arquivos de som.
  **Em jogo (desktop, container `horizon`, 2026-11-08; `h2_som_medida.py`, os mesmos 27 cenários, stderr do port com `TRACE`; mídia em `~/.cache/rust-mekanism/wa/som/`):** o evento de cada som é o do original (id,
  `source` 4 = `BLOCKS`, volume, tom 1, laço, atraso 0, atenuação linear, não relativo, começa mudo, posição no centro do bloco); o som começa no tick em que o bloco acende e para no tick em que apaga (180 ticks nas quatro
  máquinas, a Enrichment Chamber, o Crusher, o Smelter e o Infuser, medidos em cada uma); `baseSoundVolume` 0, 0,5, 2 e 10 dão volume 0, 0,5, 2 e 10 nos dois jogos; o silêncio no máximo não começa som e o posto com o som tocando o
  cala no próximo ciclo; o silêncio tirado pela janela de upgrades (botão Uninstall) não devolve o som, e o `lit` que muda leva a contagem verdadeira, como no rastro; `enableMachine` desligado não começa som, e o som que já tocava
  fica até o próximo bloco o parar (o port o adotou e parou no `lit` apagado, como o original); os pés a 15,5 blocos acima (a câmera a 17,1) tocam e a 16,5 não; 15 blocos na horizontal tocam e 17 não; com `baseSoundVolume` 2, 25 blocos tocam, e
  com 1 ou 0,5 (17 blocos) não; o jogador que sai do alcance cala o som no próximo ciclo; o relógio parado o cala. **Mutação em jogo:** com a câmera no lugar dos pés, `pes_perto` não começa o som (o original começa). Diferenças que sobram: o
  instante da queda de volume depende do ciclo sorteado (20 a 39 ticks) e do instante do comando, então no silêncio posto com a máquina tocando a queda sai 10 a 25 ticks adiante do original, e no relógio parado a volta difere porque o tempo parado do port
  é medido em segundos; a contagem de silêncio chega ao cliente do anfitrião um tick antes do estado do bloco (a tabela é memória do processo), então o `lit` que apaga mostra o fator já certo um tick antes dele. **Áudio:** o port foi gravado pelo
  PipeWire (sink nulo, WAV de 48 kHz): RMS 97,7 com o volume 1, 48,4 com 0,5 e 103,3 com 2, razões de 0,496 e 1,058, que é o `calculateVolume` do motor (o volume vai a 1 no máximo: 0,5 dá metade e 2 dá o mesmo que 1). O áudio do original **não foi gravado**: a
  primeira tentativa não achou o stream do `mekanism-ref` no PipeWire do host e o som foi para as caixas do usuário, e não insisti (**regra do usuário, 2026-11-08: o jogo de teste, o port e o original, nunca toca nas caixas dele; o áudio vai para um sink nulo (`audio_nulo.sh`) ou o volume mestre vai a 0, e os scripts de `dev/referencia` já fazem o segundo**); a forma de onda do original **fica pendente**, gravada só pelo sink nulo; sem ele a forma de onda não foi comparada, e os `.ogg` são byte a byte os do original, então o que podia
  diferir é só o ganho. Também visto, fora do som (o carvão no slot de infusão do Metallurgic Infuser, investigado em jogo em 2026-11-08 e **não é divergência nova**): com o slot de entrada vazio, 64 `coal` postos por `data merge` no slot de infusão viram 640 de carbono (10 por item, em poucos ticks), e o mesmo vale pela tela; com um `iron_ingot` na entrada eles não viram, nem pela ordem (ferro antes ou depois), porque o tanque de infusão só aceita um químico que alguma receita junte com o item da entrada (`canFillOrConvert`, regra já registrada e coberta por `slot_rules.rs`) e o original tem `processing/iron/enriched` (ferro com `#mekanism:carbon`, a explicação "que o port ainda não traz" estava velha, o arquivo existe no port e a causa ficou em aberto: veja "Fora: carbon e tin" acima). O `data merge` do driver só passou o ferro e o carvão juntos, sem passar pela regra do slot, mas a regra que recusou o carvão é a do original, aplicada com as receitas que o port tem. O que ainda difere: o som de Natal usa o fuso do jogador, como `LocalDate.now()`; o convidado de LAN recebe a contagem de silêncio por pacote, mas isso não foi testado com convidado; o booleano escrito como texto no
  arquivo de configuração (`enableMachine = "true"`) o validador do `ModConfigSpec` aceita e a leitura falha com `ClassCastException`, e o port o lê como o booleano; o intervalo de verificação (20 a 39 ticks) é
  sorteado por instância nos dois jogos e não tem semente a reproduzir; e a forma de onda do original não foi gravada (veja "Em jogo"). O silêncio vai de 1 (sem upgrade) a 0 (máximo 1: o `muffling.json` do original diz `max` 1, então não há 2 nem 4).
- Experiência (`TileEntityRecipeMachine`, `RecipeCacheLookupMonitor`): `recipesUsed`, a última receita, `hadContainment` (`has_containment`) e `setRecipeUsed` têm rastro. `awardUsedRecipesAndPopExperience` roda ao tirar o resultado (injeção H27: o Horizon não avisa a retirada em slot de resultado de mod), com a experiência de cada receita lida do JSON (`experience`) ou do cozimento do jogo. Em jogo, 2 carvões com `xp_containment` e a retirada pelo clique deram 2 pontos ao jogador (4 operações vezes 0,5) e limparam `recipes_used`. Quebrar a máquina solta a experiência guardada (`destroy_block`, só o jogador; explosão, pistão e `/setblock` não passam por ele), e tirar o upgrade a perde, como o original (`hasXpContainment` já é falso dentro do `recalculateUpgrades`). A parte fracionária (`Math.random()` no Java, `AbstractFurnaceBlockEntity.createExperience`) sai de um `java.util.Random` de 48 bits semeado a cada execução (`machinery_basis_core::java_random`, igual ao JDK 21 em seis saídas; **feito em 2026-11-08**): não há semente a reproduzir, mas a distribuição é a mesma, uniforme em [0, 1), testada em 200.000 sorteios (dez faixas, média e independência de sorteios seguidos). Antes era uma fração de nanossegundos do relógio, que anda poucos nanossegundos entre dois sorteios seguidos (diferença média de 0,0001 contra 1/3 de dois uniformes independentes).
- Aba de XP (`GuiXpContainmentTab`, `SupportsXpContainment`; 2026-11-07): portada nas quatro máquinas. Aparece com o upgrade `xp_containment` instalado (`hasXpContainment`), na lateral direita (`imageWidth`) a 90 (com o livro de receitas na tela, como as quatro máquinas têm no original; sem ele o Java sobe a aba 28, para 62), suporte de 26 por 26 tingido de `0xFF80FF20` (`SpecialColors.TAB_XP_CONTAINMENT`) e botão de 18 por 18 em (179, 94), com o ícone `tab/xp_containment` e o fundo `button`, `button_focus` com o mouse em cima (e sem foco atrás da janela de upgrades, `isMouseOverCheckWindows`). Tooltip `tab.mekanism.xp_containment` ("Collect all stored experience."), quebrado em 170 pixels, ao lado do botão. O clique esquerdo (o direito não conta, `isValidClickButton`) manda o botão `COLLECT_XP` (`menu_button`, id 1) ao servidor, que roda `collect_xp` (só com o upgrade) no objeto vivo e solta as bolas no jogador que clicou (`player.position()`); o jogo sincroniza `hasXpContainment` por um data slot a mais (o `SyncableBoolean` de `addContainerTrackers`; o mapa de upgrades só viaja com a janela aberta, então não serve). A mecânica genérica (a geometria de uma aba inset, o fundo do botão, o clique e o foco) está em `machinery_basis_core::side_tab`, e o desenho em `machinery_basis::inset_tab`; o Mekanism só dá `y`, cor, ícone e texto. Contra o original: rastro `xp_tab` (posição, tamanho e visibilidade da aba em 21 estados do cliente oficial), `chamber_xp_containment` e `chamber_xp_vanilla` para o efeito, e pixel (guiScale 2 e 3, 21 estados: sem e com o upgrade nas quatro máquinas, cursor no botão e na borda, janela de upgrades por cima com o cursor atrás dela e fora dela, upgrade instalando e já instalado, clique): zero pixels diferentes na aba, no título e na janela; sobra só o mundo atrás da tela (H22), os cantos transparentes do suporte e do painel e, nos tooltips, o que o fundo de 94 % de opacidade deixa passar do mundo (tolerância de 16 níveis por canal só nessa região; antes eram 5 pixels de GUI). O caso da janela de upgrades com a aba de xp (`chamber_xp_janela`) deu 4800 pixels na lista de upgrades numa rodada: não era corrida do tempo, e sim que a janela só recebe os dados do servidor no tick seguinte à abertura e o servidor estava parado (`tick freeze`); o roteiro do port agora dá um tick depois de abrir a janela, e o caso deu 0 (a lista, a seleção e o resto da janela). A linha de ícone da lista é a diferença H25 de sempre (61 pixels de GUI a guiScale 3). Em jogo (desktop, Enrichment Chamber, speed 8 e `xp_containment`): 2 carvões dão `recipes_used {enriching/enriched/carbon: 2}`, o clique na aba dá 1 ponto de experiência ao jogador e zera `recipes_used`, como o rastro; sem o upgrade a aba some e o mesmo clique não faz nada. As receitas (`ServerPlayer.awardRecipes` e `triggerRecipeCrafted`): o Horizon tem o livro (`level.recipe_book.add_recipes`) e os gatilhos (`recipe_unlocked`, `trigger_recipe_crafted`) como privados de `game.rs`, e o glue os chama sem injeção nova (o módulo é filho de `game.rs`); o jogador que age é o do clique (no `menu_button` o estado dele já está no lugar) ou, ao tirar o resultado, o que clicou (`as_guest` no tick da máquina, e no tick seguinte se o servidor não está à mão). Em jogo (Crusher, `crushing/blaze_rod`): o `recipeBook` salvo do jogador ganha `mekanism:crushing/blaze_rod` só depois do clique na aba (antes tinha só o do Enrichment Chamber, de outro teste). O critério `recipe_crafted` anda pela mesma chamada, mas nenhum avanço do Mekanism o usa no Horizon: a chamada compila e roda, e o efeito não foi observado. Não testado em jogo: a aba para um convidado de LAN (o clique segue o mesmo `menu_button`) e o livro de receitas da tela (o port não o tem; a aba ficaria a 62 só se o original o desligasse).
- Nome com estilo (`CustomName`; 2026-11-07): o componente passa inteiro do item ao tile (`applyImplicitComponents`, o tag do `custom_name` da pilha, que o Horizon guarda inteiro em `StackData.custom_name`), do tile ao NBT salvo (`CustomName`, o mesmo tag), do tile à tela (por NBT no canal `machinery_basis:name`, lido por `name_text` e desenhado por `HudBatch::text_styled`, com a largura do texto estilizado) e do tile ao item solto (`copy_name`). Contra o original (`chamber_name_styled`: um item com nome vermelho e negrito, e outro com um filho azul em itálico): o NBT que o tile grava é o mesmo SNBT do Java, `{bold:1b,color:"red",text:"Camara Colorida"}` e a versão com `extra`, conferido em jogo com `data get block`; o item solto leva `minecraft:custom_name` com o mesmo tag. O título da tela, pixel contra o oficial (guiScale 2 e 3, Enrichment Chamber com nome vermelho e negrito, com filho de outra cor, e Infuser): zero pixels diferentes no título e na tela inteira, fora os cantos do painel. O Horizon tira o tile do mundo antes de rolar a tabela de loot de um jogador, e `NativeLootDrops` não traz o block entity: o nome do item solto vem do que `destroy_block` guardou antes disso (só a quebra pelo jogador; explosão e pistão dependem de o tile ainda existir, não testado). A regra (`machinery_basis_core::name::TileName<C>`) não lê o componente. O Heat Generator é nomeável no original (`nameable = hasGui() && !AttributeGui.hasCustomName()`, e ele não tem nome de tela próprio; rastro `heat_generator_name_styled`: o componente passa inteiro e, sem nome, o título é `container.mekanismgenerators.heat_generator`) e está portado como as quatro máquinas, pelas mesmas funções da `machinery_basis` (`name_*`) e quatro hooks novos em `mekanismgenerators`. Em jogo, o gerador guarda o `CustomName` do item, mostra o título vermelho e negrito (print em `~/.cache/rust-mekanism/wa/xp/`) e solta o item com `minecraft:custom_name`; pixel: o título do Heat Generator com nome (`generator_nome_cor` e `generator_nome_filho`, guiScale 2 e 3) não difere. A explosão: o original (`chamber_explosion`: `ServerExplosion` de raio 1 sem resistência no centro da câmara com nome e 2 operações guardadas) quebra o bloco, solta 1 bola de 1 ponto e o item com o componente do nome. No port, as duas coisas saem de um raio maior (doze TNT no mesmo ponto, porque o bloco resiste a 16 e uma explosão só não o quebra, e uma bomba matando a bola e o item que a anterior soltou impede medir pelo número): uma bola de valor 1 nasce no centro da câmara e o item solto leva `minecraft:custom_name` com o mesmo tag. A experiência de uma explosão passou a ser solta pelo hook `explosion_detonate` (antes só a quebra pelo jogador soltava: `destroy_block`), e o nome do item já saía com o tile ainda no mundo. Pistão: o original recusa (`BlockMekanism.getPistonPushReaction` devolve `IMMOVEABLE` com block entity); no port a máquina também não se move e um bloco de controle ao lado anda (medido com um pistão e um bloco de terra). Mutações do `effects_trace` e do `name_trace` cobrem o que é regra pura.
- Aba de energia (`GuiEnergyTab`; 2026-11-07): um `GuiTexturedElement` de 26 por 26 em (-26, 137), sprite `tab/energy_info`, sem clique e sem configuração de unidade (a unidade é fixa em FE; `compactEnergyDisplay`, que o port já supõe ligado, é o único valor de cliente), nas quatro máquinas e no Heat Generator. Tooltip com `Tooltip.create` de uma lista, uma linha por vez quebrada em 170 pixels: nas máquinas `Using: <consumo>/t` (`getEnergyPerTick` com a máquina ativa, senão zero; o consumo é o do contêiner do cliente, que o servidor sincroniza como `SyncableInt` e aqui vai em dois data slots novos), `Needed: <max(0, capacidade - energia)>` e `Unit: FE`; no gerador `Producing: <taxa>/t` (`getProductionRate`, que o port já sincronizava) e `Unit: FE`. A mecânica genérica está em `machinery_basis_core` (`side_tab::InfoTab`: o retângulo e a janela por cima; `units::energy_info_lines`), o Mekanism e o Generators só dão posição, sprite e textos. Contra o original, pixel (guiScale 2 e 3): a aba e o tooltip sem diferença em 5 estados por máquina (parada, cheia, ativa, com upgrades de velocidade e de energia, e o cursor sobre ela) e 3 no gerador (parado, produzindo), com os cantos transparentes do sprite contados à parte (`canto_aba_energia`) e o tooltip com a tolerância de 16 níveis por canal que o fundo `0xF0100010` (94 % opaco) dá ao mundo atrás, que é o resíduo H22; um texto diferente passa dela (teste do comparador). O que o comparador descobriu escondido em `mundo_atras` e agora conta como elemento do mod (e falha enquanto faltar): no Heat Generator, a aba de calor (`GuiHeatTab`, -26, 109, visível), os cinco slots de armadura e mão (`GuiSlot` em x = -21, y = 10 a 82, visíveis, e o suporte, oculto na cena até 2026-11-07 e visível depois) e a aba de upgrades à direita (o gerador aceita o upgrade de silêncio e o original a mostra); nas máquinas, tudo o que sobra fora da tela é suporte oculto pela cena e sem equivalente no port (configuração de itens e de transporte à esquerda; segurança, livro de receitas e redstone à direita). Medidas do que faltava nessa data (tudo portado depois, em 2026-11-07: a aba de calor com a unidade de temperatura e as conversões, os slots de armadura e de mão e a aba de upgrades do gerador, itens do Heat Generator abaixo): a aba de calor pede `lastTransferLoss` e `lastEnvironmentLoss` do gerador (dois `double`, quatro data slots), a unidade de temperatura do cliente (`MekanismConfig.local.tempUnit`, que o clique troca entre K, C, F, R e STP, e que o port não tem como config de cliente), as conversões de unidade e três linhas de idioma; os slots de armadura pedem o menu do gerador com cinco slots a mais e o suporte, o que muda o layout do menu e as regras de quem os enche (portados em 2026-11-07, veja o item dos slots de armadura do Heat Generator); a aba de upgrades do gerador pede a janela de upgrades do gerador (menu, spec, rastro). No Infuser, o controle `infuser_operacao_10` (sem aba nenhuma: 10 passos operando) já difere na barra de químico (536 pixels a guiScale 2, e 1431 a 3), então a diferença que os casos `ativa` e `velocidade` do Infuser mostram na barra de químico é do alinhamento de ticks do químico, anterior a esta mudança. Explicada depois: é o H26 (mistura de sprites animados em float), com teto medido em "Tetos dos resíduos" (`infuser_operacao_10`, 0 pixel a guiScale 2 e 90 a 3).
- `MekanismContainer`: os avisos da tela estão nas quatro máquinas, comparados lado a lado com o oficial em 17 cenas, e a
  máscara de erros e o redstone vão no menu sincronizado (antes data slots escritos no `ServerTickEnd`), lidos do estado vivo (2026-11-08, decisões 5 e 8: o `broadcastChanges` do original os lê no `ServerPlayer.tick` do tick seguinte, então a energia que chega depois do tick da máquina vai com o erro velho). A lista de trackers está portada como regra e rastro (parte 1, abaixo) e é o transporte da tela das cinco máquinas desde 2026-11-08 (parte 2, abaixo): não há mais data slots de tela. O nome (`getName`, `getDisplayName`, `CustomName`) está portado com o componente inteiro (cor, negrito, itálico e filhos): o item o dá ao tile ao colocar, o tile o dá ao item solto, e o servidor o manda ao cliente ao abrir a tela, que o desenha como título (`name_trace.rs` contra `chamber_name` e `chamber_name_styled`; item abaixo).
  **Lista de trackers, parte 1 (2026-11-08): regras e rastro, sem trocar o transporte.** O original sincroniza uma lista de `ISyncableData`
  por menu (um objeto de menu por jogador, então "o que o jogador viu" é o último valor de cada dado): `MekanismContainer.track`
  (`MekanismContainer.java:479`), `broadcastChanges` (`:647-667`, só o sujo, com `isDirty` e `getPropertyData`), `sendAllDataToRemote`
  (`:670`, `sendInitialDataToRemote` `:675`: pergunta `isDirty` só para guardar o valor e manda tudo como sujo), `startTrackingServer`
  (`:155`) e `stopTracking` (`:171`). O `@ContainerSync`/`SyncMapper` (`SyncMapper.java`, reflexão no `collectScanData`) só tem campo nos
  dados de multibloco: nenhuma classe das cinco máquinas o usa, então a lista vem toda de `addContainerTrackers`. A regra de cada classe
  (`Syncable*.java`): `SyncableBoolean`/`Int`/`Long`/`Double` (`isDirty` na linha 18 ou 19) comparam com `!=` e guardam o valor lido, e o
  último valor começa em zero; `SyncableEnum` (`:42`) começa no padrão dado na criação e manda o ordinal; `SyncableLargeResourceStack`
  (`:67`) começa na pilha vazia e devolve `SIZE` (só um `LongPropertyData`) quando só a quantidade mudou, `DIRTY` (a pilha inteira) quando o
  recurso mudou; `SyncableStreamCodec` (`:99`) compara o `hashCode` do valor com o último (zero no começo) e manda os bytes do codec.
  Tabela medida (posição, campo, classe; o sufixo `$1` é a versão de vetor, `$2`/`$3` a de getter e setter), igual nos rastros
  `chamber_container_tracking`, `infuser_container_tracking` e `heat_generator_container_tracking`:

  - Enrichment Chamber, Crusher e Energized Smelter (16): 0 `StreamCodec` (frequência de segurança, `TileComponentFrequency.java:408-425`; sem dono é
    `null`, hash 0, bytes `00`); 1 `Enum` (`SecurityMode`, `TileComponentSecurity.java:138`); 2 `Enum` (`controlType`, `TileEntityMekanism.java:930`);
    3 e 4 `Boolean$3` (`redstone`, `redstoneLastTick`, `:931-932`); 5 `Long$2` (`lastEnergyReceived`, `trackLastEnergy`, `:967-972`);
    6 `Long$2` (`getCapacityAsLong`) e 7 `Int$2` (`getEnergyPerTick`), só com upgrades ou taxa ajustável (`:954-958`); 8 `Long$2`
    (a energia, `:962`); 9 a 12 `Boolean$1` (`trackedErrors`, `TileEntityRecipeMachine.java:170`); 13 `Boolean$3` (`hasXpContainment`, `:171`);
    14 `Int$2` (`operatingTicks`, `ProgressRecipeCacheLookupMonitor.java:56`) e 15 `Int$2` (`ticksRequired`, `TileEntityProgressMachine.java:66-67`).
  - Metallurgic Infuser (18): 0 a 4 iguais; 5 `LargeResourceStack` (o tanque de infusão, `:934-937`); 6 a 9 a energia (como 5 a 8
    acima); 10 a 14 `Boolean$1` (cinco erros); 15 `Boolean$3`; 16 e 17 `Int$2`.
  - Heat Generator (13): 0 a 4 iguais; 5 `LargeResourceStack` (a lava, `:939-942`); 6 e 7 `Double$2` (calor e capacidade de calor,
    `:944-950`); 8 `Long$2` (`lastEnergyReceived`, sempre 0: o buffer só aceita de dentro) e 9 `Long$2` (a energia); 10 `Int$2`
    (`getProductionRate`), 11 e 12 `Double$2` (`getLastTransferLoss`, `getLastEnvironmentLoss`; `TileEntityHeatGenerator.java:245-247`).
  - Janela de upgrades, por jogador, na chave `UPGRADE_WINDOW` (2): `Int$2` (`upgradeTicks`) e `StreamCodec` (o mapa de upgrades, hash do
    mapa), `TileComponentUpgrade.java:290-296`.

  Os cinco menus têm 16, 16, 16, 18 e 13 dados (a janela de upgrades acrescenta 2 por jogador, e pedir de novo sem parar vaza a lista
  antiga). O gerador não tem o par capacidade/consumo porque o contêiner dele é um `BasicEnergyContainer` (`TileEntityGenerator.java:47`).
  **Feito (lib `machinery_basis_core`):** `container_sync` (`Kind`, `Value`, `ContainerSync`: o estado por jogador, `send_all`,
  `broadcast_changes`, `start_tracking_server`, `stop_tracking`, com a regra de sujeira de cada classe; reaproveita o `Tracking` de
  `window_session` para a posição e o vazamento), `tile_trackers` (a ordem de `TileEntityMekanism`, `TileEntityRecipeMachine`,
  `TileEntityProgressMachine` e da janela, com as fontes `source::*`), `machine_sync::MachineSync` (o valor vivo de uma
  `ProgressMachine`) e `last_energy::LastEnergyTracker` (`ProgressMachine::energy_received` e `last_energy_received`, o fim do tick
  pergunta `checkTickChanged`). **Nos mods:** `mekanism_rules::{electric_trackers, infuser_trackers, upgrade_window_data}` e
  `mekanismgenerators_rules::{heat_generator_trackers, HeatGeneratorSync}`; o `tick` do gerador passou a respeitar `canFunction()` (a
  redstone; no jogo o gerador ainda carrega sempre `Disabled`, então nada muda lá até a frente de redstone). **Rastros (desktop,
  `runGameTestServer`):** `{chamber,crusher,smelter,infuser,heat_generator}_trackers` (`dev/referencia/testes/TrackerTraceTest.java`):
  cada tick, o que o `broadcastChanges` real mandaria (índice, classe, `DIRTY`/`SIZE`, valor) a um `ServerPlayer` falso com a tela aberta,
  e o que o `sendAllDataToRemote` e o `startTrackingServer` mandam. Cenários: abrir, energia fora do tick (`setEnergy` e inserção pela
  capability, que alimenta o `lastEnergyReceived`: 15 no tick seguinte, 5 depois, 0), receita até o fim (200 ticks), os quatro modos de
  redstone com sinal ligando e desligando, upgrade pelo slot com a janela fechada e aberta (`startTrackingServer`, a segunda abertura sem
  parar e a parada), fechar; o gerador com lava, combustível, energia cheia e o upgrade de silêncio. **Teste sem o jogo:**
  `mods/mekanism/tests/tracker_trace.rs` (quatro máquinas) e `mods/mekanismgenerators/tests/tracker_trace.rs` repetem cada rastro tick a
  tick com `SimWorld` (`tick_level`, as marcas, `finish_tick`, o `broadcastChanges` no lugar do `ServerTickEvent.Post`) e comparam o
  conjunto sujo, a classe e o valor. 19 mutações (ordem de dois dados, classe de um dado, regra de sujeira, `lastEnergyReceived`, janela)
  e as 19 morrem (duas só pelo teste unitário da lib: o último valor inicial de `Enum` e de `StreamCodec` nunca aparece num rastro, porque o
  `sendAllDataToRemote` roda antes do primeiro `broadcastChanges`, no Java também; e o `.max(stored)` da capacidade, que só difere no jogo, onde a
  energia vem do block entity, por `the_capacity_data_never_reads_below_the_stored_energy`).
  **Diferenças que ficam (o port ainda não tem a peça):** (1) o componente de segurança: a frequência de segurança e o `SecurityMode`
  existem na lista mas valem `null`/`PUBLIC` (o rastro foi gravado com o bloco do molde, sem dono; uma máquina posta por jogador manda a
  frequência do dono, e o hash dela sai de `Frequency.hashCode`); entra com a aba de segurança. (2) O mapa de upgrades vai na ordem do
  registro e não na do mapa de hash do Java: essa ordem não existe para copiar (decisão 9, medida: quatro execuções do original, quatro ordens diferentes, nenhuma a do
  registro), e o cliente lê os dois do mesmo jeito. **(3) Feito em 2026-11-08:** o pacote leva o `ResourceType` da pilha (item 0, fluido 1, químico 2, antes do id), como o `ResourceStackPropertyData` do Java;
  o espelho só aceita a pilha do tipo do dado, e `tile_trackers` dá químico aos tanques de químico e fluido aos de fluido (ver "Dados sincronizados do menu" em `docs/machinery-basis.md`).
  **Parte 2 (feita, 2026-11-08): o transporte.** *Formato* (`machinery_basis_core::container_sync::wire`): o `PacketUpdateContainer`
  (`PacketUpdateContainer.java`) é um pacote no canal de mod `machinery_basis:container`, um por jogador por tick: se abre um menu (o
  `sendAllDataToRemote` do menu novo) e então traz o id do bloco, que diz ao cliente a lista de dados; a posição do bloco no lugar do `windowId`;
  e a lista de `PropertyData`, cada uma com o ordinal do `PropertyType` (`BOOLEAN` 0, `DOUBLE` 2, `INT` 4, `LONG` 5, `BYTE_ARRAY` 7,
  `LARGE_RESOURCE_STACK` 12), o índice (`short`) e o valor (`VarInt`, `VarLong`, `double`, bytes, o id e a quantidade da pilha).
  *Servidor* (`windows.rs` da `machinery_basis`, módulo `machinery_basis_windows`): `Sessions` guarda um `ContainerSync<u8>` por (bloco,
  jogador), criado em `container_open` (hook `container_opened`, o `MekanismContainer.<init>`) com `electric_trackers()`,
  `infuser_trackers()` ou `heat_generator_trackers()` (a lista que a spec do bloco dá) e todos os dados mandados na hora (`send_all`,
  `MekanismContainer.java:670`; sem objeto vivo ainda, no primeiro `broadcast`); `broadcast` (hook `server_tick`, fase `ServerTickStart`) faz
  o `broadcastChanges` (`:647-667`) de cada sessão com o valor vivo que o dono do bloco entrega (`SourceFn`: `MachineSync` sobre o objeto
  vivo, a energia e o tanque do block entity nas máquinas; o estado do fim do último tick no gerador), e manda só os sujos; o pedido da
  janela de upgrades (`OP_TRACK`) é o `startTrackingServer` (`:155`) da sessão de quem a abriu, e `OP_STOP` o `stopTracking` (`:171`); a
  sessão sai quando o jogador deixa de ter o menu daquele bloco (`recount`, no `server_tick`). *Cliente*: `ContainerMirror` por posição
  (`Vec<Option<PropertyData>>` por dado, a classe da posição diz como entra, `handleWindowProperty`, `MekanismContainer.java:560-645`,
  inclusive o `SyncableLargeResourceStack.set(long)` do `SIZE`), que `screen_draw`/`screen_click`/`widget_tooltip` das cinco telas, a aba de
  avisos, a de energia, a de xp, o `redstone`, os `screen_elements` e a janela de upgrades leem (`ScreenData::read(spec, &mirror)`,
  `read_upgrade_map`); a janela faz o `start_tracking`/`stop_tracking` do espelho nas mesmas posições do servidor; o espelho nasce no
  primeiro pacote e some quando a tela fecha. O tint e o sprite do químico saem do registro de recursos pelo id (o servidor o manda a quem
  entra, `machinery_basis:resources`, como já mandava os tipos de upgrade). *Apagado*: `DataLayout`, `ELECTRIC_DATA`, `INFUSER_DATA`,
  `ChemicalSlots`, `ScreenData::write`/`of_machine`, `TileScreen` (`machine_ticked`, `server_tick_end`), `write_upgrade_sync`/`read_upgrade_sync`
  (nos dois mods), `clear_chemical_screen`, os `DATA_*` do gerador, `UpgradeSync::write`/`read`, `ErrorMask`/`RedstoneSync::to_slot`/`from_slot`, o
  módulo `data_slots`, o `data_slots` dos cinco arquivos de menu, `mb_window_tracking`, `VIEWERS` e o `server_tick_end`. O atraso de um tick dos
  upgrades some (o `MachineSync` lê o valor vivo, como o rastro confirma). *Momento*: o `broadcastChanges` do original roda em
  `ServerPlayer.tick`, antes dos block entities, e lê o valor vivo; o port o roda no `ServerTickStart`, e o pacote sai no `tick_lan` do mesmo tick
  (decisão 8). *Testes*: `screen.rs`, `warnings.rs`, `warnings_trace.rs`, `upgrade_window.rs` (nos dois mods), `gui.rs`, `energy_tab.rs`, `xp_tab.rs`,
  `screen_elements.rs` e `chamber_trace.rs` montam o espelho pelo caminho inteiro (`ContainerSync::send_all`, o pacote em bytes, `apply`; o
  `common::Live` do Mekanism e o `Client` do gerador); o `tracker_trace.rs` dos dois mods leva o rastro `*_trackers` ao servidor do port, os
  pacotes ao espelho em bytes, e compara a cada tick o que a tela lê de cada dado com o que o cliente do Java leria depois de aplicar o que o
  original mandou (cinco contêineres, milhares de leituras por rastro). *Itens do roteiro*: (d) `players_using` com espectador, abaixo; `lastEnergyReceived` em
  jogo, abaixo; `redstoneLastTick` do gerador, abaixo.
  **`players_using` e o espectador (2026-11-08).** O original conta o jogador em `playersUsing` quando o menu nasce
  (`MekanismTileContainer.openInventory`, `tile.open(inv.player)`, sem olhar o modo de jogo) e o tira em `closeInventory`; o menu de um
  espectador também nasce (`ServerPlayer.openMenu` não o recusa; o `PacketTileButtonPress` abre sem checar). O hook `mod_container_viewers`
  do Horizon não conta espectador (`BarrelBlockEntity.startOpen`), então saiu das duas `horizon-mixin.toml`: a sessão do jogador é o
  `playersUsing` (`Sessions::players`), criada no `container_opened` e largada pelo `server_tick` (`ServerTickStart`, também com o servidor parado), que a refaz a cada passada
  com o menu que cada jogador tem agora (espectador incluso; sem injeção, decisão 8). A máquina aplica a
  diferença com `open()`/`close()` no tick dela e no nascimento do objeto vivo (`born`). Em jogo: ver o registro abaixo.
  **`lastEnergyReceived` em jogo (2026-11-08).** Injeção `energy_store` (H37) no HEAD de `Game::native_energy_store`, o fim de todo `insert` e
  `move` de energia do Horizon: o que sobe no buffer soma no `LastEnergyTracker` do objeto vivo (`ProgressMachine::energy_received`, no
  `game_time` do jogo). Regra e rastro já conferidos na parte 1 (15 no tick seguinte, 5 depois, 0); em jogo: ver o registro abaixo.
  **`redstoneLastTick` do Heat Generator (2026-11-08).** O glue do gerador chama `TileBase::finish_tick` no fim do tick, como o
  `TileEntityMekanism.tickServer` (`:652-655`), e guarda o campo na memória do gerador (`UpgradeMemory.was_powered`, que o Java guarda no
  objeto e não salva); o aviso de vizinho que chega depois do tick entra no `redstone` do estado que o menu lê e no NBT, de onde o tick seguinte
  refaz o tile.
  **Tela aberta antes do primeiro tick do tile (2026-11-08, feito).** O original lê o tile direto: o `sendAllDataToRemote` (`MekanismContainer.java:670`) e o `broadcastChanges` (`:647-667`) perguntam o valor a cada `ISyncableData`, e o
  tile existe desde o `loadAdditional` (`BlockEntity.loadStatic`), antes do primeiro tick dele, então uma tela aberta num mundo reaberto nesse intervalo mostra o que o NBT guardou (energia, `operatingTicks`, upgrades, modo de redstone,
  tanque, calor). Confirmado no original (`CenaTelas` com `reabrir=1`: o tile é gravado e relido por `loadStatic` com o servidor parado, e a tela abre antes de qualquer tick; Enrichment Chamber operando, com upgrade de velocidade e sem energia,
  Infuser com tanque e Heat Generator: a barra de energia, a seta, o tanque e o calor saem do NBT, e o erro de energia fica apagado porque os erros não são salvos). O port só tinha o objeto vivo a partir do primeiro tick (`born`), e sem ele a fonte
  do `broadcast` devolvia "nada a ler": a tela abria com os dados padrão e, num tile novo na posição de um velho, lia o objeto velho. **Escolha (a tarefa dava duas):** nascer o objeto do NBT quando o tile carrega pediria um hook de carga de
  tile, que o Horizon não tem (injeção crua, e o objeto ficaria vivo para todo tile carregado, tick ou não); a fonte lendo o NBT enquanto o objeto não existe dá o mesmo valor sem nenhum hook, e é o que o tick faz ao nascer (`born`). Fica
  `machinery_basis_core::menu_source::object`: o menu lê o objeto vivo se o tile é o que ele andou (a marca que o tick deixa em `ModBlockEntity.data`, que um tile novo ou lido do disco não tem) e, senão, um objeto feito do NBT agora (as quatro
  máquinas: `born` sem guardá-lo; o aviso de vizinho que o tile novo ainda não leu fica na fila, `peek_power`, para o primeiro tick). O Heat Generator ganhou o mesmo (`loaded_view`: calor, lava, energia, upgrades e base do tile do NBT, produção e perdas em
  zero como o tile do Java antes de rodar, `redstoneLastTick` falso; o ar do bloco, só para um calor nunca salvo, sai da mesma leitura do tick num `AmbientCache` descartável, sem preencher o do objeto vivo: o `loadAdditional` não lê o ar e a tela não chama `getAmbientTemperature`) e a marca no fim do tick. Sem injeção nova (83, nada muda em `docs/injecoes-horizon.md`). **Teste sem o jogo:** rastros do Java `{chamber,infuser,heat_generator}_trackers_reloaded`
  (`TrackerTraceTest`: a máquina operando com upgrade de velocidade ou de silêncio e modo de redstone baixo, o tile gravado e relido por `loadStatic`, a tela aberta no mesmo tick e a janela de upgrades logo depois, 130 a 155 ticks), repetidos por
  `tracker_trace.rs` dos dois mods (o `reload` refaz a máquina do que foi gravado, com a energia, os slots e o tanque, como o `born`) e conferidos dado a dado e na leitura do espelho, mais os quatro testes de `menu_source`. Mutações (cada uma rodou o
  teste): o menu sempre lê o objeto vivo (2 testes de `menu_source` morrem) e o `ticked` invertido (4), o `reload` do teste sem o `restore` do que foi gravado (2), sem a energia (2), sem o tanque (1), sem os upgrades (2), o do gerador sem os upgrades (1), sem a base do
  tile (1) e mantendo a produção do tick (1): as nove morreram; sobreviveu a do `recheckOffset` que o tile reaberto sorteia, que o rastro não distingue (os erros só mudam de tick quando o recheck cai neles, e o teste não tem erro pendente no
  reabrir). Em jogo (desktop, `horizon`, build `native-f319701-3b92fc0bb580`, 1920 por 1080, `comparar_lote.py`): a cena `reabrir=1` contra o original, nos dois `guiScale`, em `chamber_reaberta`, `_upgrades` e `_sem_energia`, `infuser_reaberta`
  (copper com redstone 10, contra o ciclo `infuser_fase_redstone_10`) e `generator_reaberto`: 10 estados por escala, nenhum pixel fora dos resíduos conhecidos (o mundo atrás da tela, até 1 por canal, e as partículas da máquina acesa); e o mundo de verdade salvo e
  reaberto (`reabrir_real.py`: as máquinas a 600 blocos, `quit`, o jogo aberto de novo, o servidor parado, o jogador vai até a máquina e abre a tela antes de qualquer tick): a Enrichment Chamber com e sem upgrade, o Infuser e o Heat Generator mostram a energia, o progresso e o
  tanque gravados. O objeto vivo de um tile velho que a cena troca (`setblock` do mesmo bloco no mesmo lugar, na memória do mod) não é lido pelo tile novo. Os 10 estados de regressão que ainda tinham referência (Chamber, Crusher, janela de upgrades, aba de xp) saem só
  com os resíduos; o Infuser com tanque animado e o Heat Generator ficaram sem comparação nessa rodada (a referência de 2026-11-07 não tem o ciclo do bio nem o suporte da armadura de depois), e o lote de 50 cenas não foi repetido.
  **Em jogo (2026-11-08, desktop, `horizon`, build `native-f319701-da5ad9f6ad16` para o lote e os ciclos; depois dele mudaram só a conta de quem tem o menu (`server_tick`, sem injeção) e a ordem do item de upgrade no roteiro, conferidos no build `native-f319701-1fda68033eee`: cinco aberturas e fechamentos e o espectador contam como antes, e as cenas de janela, de instalação e de saída ficam sem a diferença na seta; 1920 por 1080, `cena_telas.py port` e `comparar_lote.py` contra o original rodado de novo no `mekanism-ref`).**
  Lote: 50 cenas (as quatro máquinas e o Heat Generator: vazia, operando, cheia, aviso de energia e de saída, hover da barra e do químico, abas de energia, xp e
  upgrades, a janela de upgrades instalando e já instalada, a entrada curta e exata, os três do overlay) em guiScale 2 e 3, 100 estados, mais 9 ciclos de 40 prints
  da animação do químico e da lava nos dois guiScale. Nenhum pixel de barra, seta, slot, aba, texto ou janela fora dos resíduos conhecidos: 66 dos 100 estados não têm
  nada fora deles, e os outros 34 só diferem no que a comparação classifica como o mundo atrás da tela (`alem_mundo_atras`, `alem_janela_fundo`, a margem da aba
  de avisos vista como mundo, H22 e H30; dois níveis por canal no preto do concreto, até 14.554 pixels no Heat Generator aceso) e na linha de ícones da lista
  de upgrades (`janela_lista_icone_*`, 58 a 107 pixels, H25). Ciclos: 40 de 40 prints idênticos na barra de redstone e de carbono do Infuser e na lava do gerador (nos dois
  guiScale); o bio segue com 17 a 19 de 40 (pior caso 72 a 297 pixels; o fungi não foi rodado), a diferença H26 já registrada. A janela de upgrades instalando mostrou um tick de
  diferença (36 pixels na seta, `janela_progresso_instalar`) com o roteiro antigo, e a causa é do roteiro, não do servidor nem da ordem dos pacotes: o roteiro do port
  mesclava o item de upgrade da entrada no NBT do componente antes do tick em que a máquina nasce, então o tick do nascimento já contava no `upgradeTicks`, e o
  `CenaTelas` do original só põe o item depois desse tick (`state` roda depois do `tick step 1` da montagem, `CenaTelas.java:139-157`). O port ficava um tick adiante, que o
  atraso de um tick escondia no código (`upgrade_sync_seen`, que saiu), e que depois pedia o passo a menos no roteiro. Medido: com o item antes do nascimento e 10 passos, 36
  pixels na seta; com 9 passos, 0; com o item depois do nascimento (a ordem do original, `Items` do bloco no `data merge` seguinte, `nbt_itens`) e os 10 passos do caso, 0 nas
  três cenas de janela da Enrichment Chamber (`chamber_aba_upgrades`, `_instalando`, `_instalado`) e nas duas de instalação do Heat Generator, nos dois guiScale (`chamber_aba_instalando` e `chamber_aba_instalado` contra o
  original rodado de novo no `mekanism-ref`). O servidor do port já batia com o rastro `*_trackers` no `upgradeTicks` tick a tick (`upgrade_window.rs` e `tracker_trace.rs`).
  A hipótese da ordem dos pacotes (o Java trataria cliques e comandos depois dos níveis) não vale: ver a decisão 8. O roteiro agora não tem compensação de tick: os `passos`
  do caso, o item de upgrade depois do nascimento e nenhum passo depois de abrir a janela, que recebe os dados na abertura. Espectador: com o menu da Enrichment Chamber aberto em criativo e a conversão para espectador (`/gamemode spectator`, que fecha a tela do
  cliente e deixa o menu do servidor), `playersUsing` fica em 1 durante todo o modo (log de teste `MEKPLAYERS`), e fechar em criativo o leva a 0; reabrir o menu do mesmo bloco não dá 2 (o jogador
  é um conjunto). O Horizon não abre a tela de um bloco para um espectador por `interact`. `lastEnergyReceived`: um Heat Generator de frente para a Enrichment Chamber
  (`front`, a face de saída dele) com a tela dela aberta mandou o dado 5 do menu subindo com a produção (1, 2, 3, 5, 6, 7, 8) e descendo a zero depois que a lava acabou, um valor por
  tick. `redstoneLastTick`: um bloco de redstone ao lado do gerador com a tela dele aberta mandou `redstone` (dado 3) e `redstoneLastTick` (dado 4) ligando, o segundo um tick
  depois, e desligando do mesmo jeito. Salvar e reabrir o mundo com a tela aberta: o mundo reabre sem erro, a tela abre de novo com a energia gravada; antes do primeiro tick
  do tile reaberto a tela mostrava os valores padrão (o objeto vivo ainda não existia): corrigido em 2026-11-08, ver "Tela aberta antes do primeiro tick do tile" abaixo. Um menu aberto sem passar pelo hook `container_opened` (visto uma vez, logo depois de abrir
  o jogo) ficava sem dados: a sessão agora nasce no `broadcast` seguinte (`adopt_open_menus`). Mutações do núcleo (cada uma rodou `cargo test --workspace`): dado limpo no
  pacote (17 testes falham), dado sujo que não sai (10), espelho lendo `Long` como `Int` (13), pilha mandada só pelo tamanho que não esvazia com zero (1), a tela lendo a
  posição errada do xp (2), a sessão que não conta no `playersUsing` (9) e a janela de upgrades acrescentada a quem não a abriu (1, depende da ordem do `HashMap`);
  as sete morreram. A mutação do espectador (o jogo contar só quem não é espectador) mora no glue (`open_container`), que só o jogo exercita: não há teste sem o jogo. Não testado: convidado de LAN
  (um cliente por vez); o pacote para outro processo usa o mesmo caminho do anfitrião, que também recebe pelo canal de mod.
- `setRecipeUsed` agora tem rastro (`chamber_xp_*`). `recheckAllErrors` com jogador olhando e `NOT_ENOUGH_ENERGY_REDUCED_RATE` agora têm rastro.
  Este último só existe no Electrolytic Separator, que não está portado: `separator_reduced_rate` é usado só pelo tracker de avisos.
- `players_using` (2026-11-08): é a sessão de menu do jogador (`Sessions::players`), espectador incluso; ver o item `MekanismContainer`. O caso de `born` com a tela já aberta e o espectador foram exercitados em jogo (registro no item).
- `sim::chunks` (carga, descarga, blocos, block entities e a ordem dos eventos) existe e é testado, e cobre as notificações de vizinhança do `setBlock`
  (2026-11-08): `set_block_flags` com a flag 1 (`Level.markAndNotifyBlock`, `Level.java:270-275`), `update_neighbors_at` na ordem de
  `NeighborUpdater.UPDATE_ORDER` (oeste, leste, baixo, cima, norte, sul; `NeighborUpdater.java:18`), `update_neighbour_for_output_signal` com o `onNeighborChange` do
  NeoForge na ordem de `Level.NEIGHBOR_UPDATE_LIST` (norte, leste, sul, oeste, cima, baixo; `Level.java:990-1006`), `place_by_player` (`setPlacedBy`) e
  `detach_block_entity`/`attach_block_entity` (um tile que sai do chunk e volta do NBT). As duas ordens vêm do jogo, lidas por reflexão na primeira linha de cada
  `neighbors_*.jsonl`, e `sim_neighbors.rs` e `neighbors_trace.rs` as conferem. A fila aninhada do `CollectingNeighborUpdater` (um aviso que dispara outro) não existe no simulador, e **nenhum bloco portado a usa** (provado em 2026-11-08, `neighbors_nested.jsonl`, `neighbors_nested.rs`):
  `BlockTile.neighborChanged` e `onNeighborChange` só chamam `TileEntityMekanism.updatePower`, que lê o sinal e guarda `redstone`; `onPowerChange` (`ITileRedstone.java:16`, vazio) não é sobrescrito em nenhum tile das cinco máquinas (a reflexão dá `""`), só no `TileEntityIndustrialAlarm` e no
  `TileEntityFissionReactorLogicAdapter`, que não estão portados. O `debugListener` da fila anunciou 12 posições (os seis vizinhos e as seis formas) em cada operação ao lado de cada máquina, as mesmas de um bloco de pedra; um fio de redstone, que avisa em cadeia, anuncia 72 (a guarda que pode falhar).
  `updateNeighbourForOutputSignal` e o `setChanged` do baú chamam o `onNeighborChange` direto, sem a fila (0 posições). Quando entrarem o cabo e as máquinas com `onPowerChange` (o Industrial Alarm, os reatores), a fila aninhada precisa entrar no simulador. O aviso a vizinho em chunk descarregado:
  `MultiNeighborUpdate.runNext` lê `level.getBlockState(neighborPos)`, que carrega o chunk (`Level.getChunk` em `FULL`), e o Horizon não o carrega (não medido): uma máquina num chunk sem ticket, vizinha de um bloco que muda na borda de um chunk carregado, perde o aviso e fica com o `redstone` velho até o próximo. Só
  se alcança com `/forceload` ou uma máquina na borda do anel de chunks; fica registrado, sem medida em jogo. Também não cobre o `onNeighborChange` passado ao bloco de trás de um condutor
  (só quem pede "weak changes" o recebe, e nenhum bloco do Mekanism pede). Os testes de rede em si são da Fase 3.
- `updatePower` pela vizinhança, como o original (**feito em 2026-11-08**, com duas ressalvas abaixo). Divergência que o plano não registrava: o port lia o sinal dos seis vizinhos a cada tick da
  máquina (`native_neighbor_signal` em `tick_machine`), e o Heat Generator ignorava o controle de redstone e o `redstone` salvo (`canFunction()` de `TileEntityHeatGenerator.java:140`
  nunca era falso). No original `redstone` só muda em três pontos: `BlockTile.neighborChanged` (`BlockTile.java:115-124`, o `updateNeighborsAt` de um vizinho que mudou com a flag 1 do
  `setBlock`), `BlockTile.onNeighborChange` (`:105-113`, o `onNeighborChange` do NeoForge, que `Level.updateNeighbourForOutputSignal` manda a cada vizinho quando um block entity muda,
  `BlockEntity.java:216-221`, e o comparador de uma máquina do Mekanism, `TileEntityMekanism.java:508-510`) e `TileEntityMekanism.onAdded` (`:1000-1008`, de `BlockMekanism.setPlacedBy`,
  `BlockMekanism.java:179-196`: só o bloco posto com um item); os três chamam `updatePower` (`:1109-1117`), que lê `level.hasNeighborSignal(pos)` na hora. Uma mudança de sinal que não avisa
  deixa o `redstone` velho (e o NBT o guarda), e as cinco máquinas herdam isso de `BlockTile` (`neighbors_block_classes`). **Port:** `machinery_basis_core::neighbors` (as duas ordens de
  aviso, as flags do `setBlock` e `PendingPower`), o hook `neighbor_changed` (anota quem foi avisado), a injeção crua `flush_signals` (HEAD de `Game::flush_spill`, H34: o fim de cada operação no mundo lê o sinal, decisão 7),
  `Game::container_changed` (injeção crua H35: o `onNeighborChange` dos contêineres do vanilla), `notify_comparators` (o das máquinas do Mekanism) e `place_block` (`onAdded`); o tick da máquina
  pega o que ficou (`take_power`), o `ServerTickEnd` também (a tela). O Heat Generator guarda `redstone` e `control_type` no NBT e `canFunction()` entra na regra (`rules::tick`).
  **Rastros do Java** (`dev/referencia/rastros/neighbors_*.jsonl`, `NeighborTraceTest`, 15 arquivos): `chamber_notified` e `heat_notified` (bloco de redstone e alavanca, no passo e antes do tick da
  máquina, nos modos HIGH, LOW e DISABLED), `chamber_unnotified` e `heat_unnotified` (flags 2 e 818, depois um aviso de outro bloco, e um aviso desfeito no mesmo tick), `chamber_mid_tick`,
  `chamber_placed` e `heat_placed` (gabarito, `setBlock` e `useOn`), `chamber_reload` e `heat_reload` (o tile sai e volta com o sinal mudado), `chamber_output_signal` e `heat_output_signal` (um
  baú vizinho muda; uma máquina vizinha muda o comparador) e `crusher_core`, `smelter_core` e `infuser_core`. **Testes sem o jogo:** `neighbors_trace.rs` (Mekanism, 10 testes) e `neighbors_trace.rs`
  (Generators, 5) repetem os rastros tick a tick no `SimLevel` e conferem o `redstone` de todas as máquinas logo depois de cada operação, no meio do tick; `sim_neighbors.rs` (9) confere a ordem
  das faces. Mutações, todas mortas: ler o sinal a cada tick (8 de 10 testes do Mekanism falham), `setBlock` que avisa sempre (7), a ordem de `UPDATE_ORDER` trocada (9), esquecer o
  `onAdded` (4), esquecer o `onNeighborChange` (1), o tile que volta do NBT lendo o sinal (1), o bloco posto avisando a si mesmo (1) e, no gerador, `canFunction()` sempre verdadeiro (5 de 5).
  **Em jogo** (desktop, `horizon`, build `native-f319701-3baabd0d9efd`, `h2_vizinhos.py`, servidor parado, `tick step 1`, os rastros repetidos como comandos): `chamber_notified` (339 leituras), `chamber_unnotified` (429), `chamber_mid_tick` (73), `chamber_placed` (306), `chamber_output_signal` (226), `heat_notified` (339), `heat_unnotified` (429), `heat_placed` (306) e `heat_output_signal` (226): 0 diferenças em todos, comparando `redstone`, `active_state` e `progress` do NBT depois de cada tick com o rastro (o campo no meio do tick só se lê no simulador, porque o NBT é escrito no tick). Só no simulador ficam `*_reload` (o jogo não descarrega um tile sem o chunk) e `crusher_core`, `smelter_core` e `infuser_core` (as quatro máquinas rodam o mesmo `tick_machine`; `neighbors_block_classes` prova a herança de `BlockTile`). No Heat Generator a energia que ele produz depende do ambiente (a temperatura do bioma) e o tanque do port guarda 1.000 (`TANK_CAPACITY`), que é o `heatTankCapacity` padrão do original (`GeneratorsConfig.java:110-111`, `FluidType.BUCKET_VOLUME`) e não uma aproximação; o rastro pôs 10.000 por `setContents`, que não confere a capacidade (é o cenário do Java, não o jogo), então os dois voltam ao valor do rastro a cada tick, como `Scene::seed`. O port não lê a configuração do gerador, então o `heatTankCapacity` ajustável fica na pendência da configuração de servidor.
  Prints e vídeo: `~/.cache/rust-mekanism/wa/vizinhos/`. **Ressalvas:** (1) o `onNeighborChange` que o NeoForge manda quando um block entity do vanilla sem contêiner muda (uma placa, um
  sino) e em `Level.removeBlockEntity` o Horizon não expõe: sem a injeção H35 o baú vizinho que muda não relia o sinal (99 diferenças em 226 leituras em `chamber_output_signal`), com ela 0; ficam de fora uma placa, um sino e a remoção do block entity; (2) a mudança de sinal dentro da mesma operação que já avisou o bloco, sem aviso, lê o valor do fim da operação e não
  o do aviso (decisão 7). O aviso do comparador de uma máquina saía em `level_tick_post`, depois de todos os block entities, e não dentro do tick da máquina que mudou, como no
  Java (`TileEntityMekanism.tickServer`, `:645-652`): **corrigido em 2026-11-08**, veja o item "Comparador das cinco máquinas" abaixo. (3) O glue do Heat Generator agora chama `TileBase::finish_tick` no fim do tick (2026-11-08, parte 2 dos trackers), e `redstoneLastTick` (`WAS_POWERED`) acompanha o sinal um tick depois, como o Java (`TileEntityMekanism.java:653-655`).
- Comparador das cinco máquinas (**feito em 2026-11-08**). **Java:** `TileEntityMekanism.tickServer` (`:645-652`) relê o nível só quando `updateComparators` está aceso (`markDirtyComparator`, `:500-506`, que o
  `setChanged` acende nas máquinas e a mudança do tanque acende no Heat Generator: `makesComparatorDirty`, `:1149-1151`, `:1158-1161`) e, se `getRedstoneLevel()` mudou, guarda em `currentRedstoneLevel` e chama
  `notifyComparatorChange` (`:508-510`, `Level.updateNeighbourForOutputSignal`) **dentro do tick da máquina**, antes de `ticker++` e de `redstoneLastTick`. O bloco devolve esse
  `currentRedstoneLevel` (`BlockMekanism.getAnalogOutputSignal`, `:234-245`; `hasAnalogOutputSignal` é `AttributeComparator`, que `Machine.java:26` e `Generator.java:15` põem nas cinco). As quatro
  máquinas usam `getRedstoneLevel` de `TileEntityMekanism` (`:1136-1144`: `ResourceContainerType.getRedstoneSignalFromContainers` sobre `getInventorySlots()`, a média da fração de cada slot com capacidade
  maior que zero, `Mth.lerpDiscrete`), o Heat Generator o tanque de lava (`TileEntityHeatGenerator.java:233-235`, `getRedstoneSignalFromContainer`). Quem tica depois da máquina que mudou, no mesmo tick, já tem o
  `onNeighborChange` dela (`BlockTile.java:105-113` -> `updatePower`); quem tica antes o tem no tick seguinte. A ordem dos tickers é a da lista de `Level.tickBlockEntities`: quem entrou no mundo antes tica antes.
  **O que o port fazia:** (1) o aviso era enfileirado e saía em `level_tick_post`, depois de todos os block entities; (2) `report_comparator` guardava `reported_level = None` num objeto novo e avisava os vizinhos no
  primeiro tick de toda máquina, mesmo sem mudar de nível (o Java não avisa: `currentRedstoneLevel` vem do NBT ou 0, e `updateComparators` nasce falso); (3) o Heat Generator não tinha comparador (`finish_tick(|| 0)`, "o gerador
  não tem comparador"), e o Java tem. **Agora:** `tick_machine` chama `publish_comparator` logo depois de `tick_server`, com o bool que o próprio `finish_tick` do tile devolve (só quando o nível mudou), e o gerador
  faz o mesmo com `rules::finish_tick` (o tanque de lava marca sujo se mudou desde a última leitura; `current_redstone` salvo no NBT); o `SimLevel` avisa depois de cada ticker. A injeção `level_tick_post` saiu, e o gerador
  ganhou os hooks `has_analog_output` e `analog_output` (84 injeções). **Rastros do Java** (`ComparatorTraceTest`, `dev/referencia/rastros/`): `comparator_order` (duas Enrichment Chambers encostadas, S com o `redstone` velho,
  nas duas ordens de ticker, com um comparador lendo cada uma: a linha de N-antes-de-S anda em t=21 e t=55, a de S-antes-de-N em t=22 e t=56; a saída do comparador sobe dois ticks depois da mudança de nível, igual nas duas),
  `comparator_first_tick` (N posta sem avisar não corrige o `redstone` velho de S no primeiro tick dela: S segue com `redstone = false` de t=21 a t=29, com o sinal ligado ao lado; o port a ligava em t=21) e `comparator_levels_*` (as cinco máquinas: vazia 0, 1 item 1, 32 itens 3 (2 no Infuser), 64 itens 5 (4),
  entrada e saída 10 (8), tudo 15; o Heat Generator: lava 1 dá 1, 62 e 63 dão 1, 500 dá 8, 999 dá 14, 1000 e acima 15). **Testes sem o jogo:** `mods/mekanism/tests/comparator_trace.rs` (7: as duas ordens, o primeiro tick, o valor
  analógico das quatro máquinas tick a tick com o bloco do comparador simulado, e a prova de que avisar depois dos block entities dá outro rastro) e `mods/mekanismgenerators/tests/comparator_levels.rs` (2). Mutações, todas mortas: avisar no
  fim do tick do nível em vez de dentro do tick (2 testes de `comparator_trace` falham, `comparator_order` e a prova da mutação), avisar no primeiro tick (`comparator_first_tick`), avisar sem mudança de nível (`comparator_order` e
  `comparator_first_tick`), a fórmula do nível sem o `+1` de `lerpDiscrete` (os 4 de níveis e os 2 do gerador). **Em jogo** (desktop, `horizon` headless, build `native-f319701-a69cbc5396bd`, `h2_comparador.py` e `h2_placed_fed.py`, servidor parado, `tick step 1`, as operações do rastro como comandos, os comparadores de verdade e o `OutputSignal` deles lido do NBT): `comparator_order` 1.094 leituras e `comparator_first_tick` 612 (`active_state`, `progress` e `current_redstone` de S e de N, e a saída dos dois comparadores a cada tick), `comparator_levels_*` 128 leituras em cada uma das cinco máquinas (`current_redstone` e a saída do comparador a cada tick, com a lava no Heat Generator): 0 diferenças em todos. A ordem de ticker do Horizon é a de inserção, como a do Java (as duas linhas de `comparator_order` deram as duas ordens com a mesma sequência de colocação). O `first_tick` deu 2 leituras diferentes em 1 de 5 rodadas, as duas em t=7 e só na saída de um comparador (o port adiantado um tick): corrida do `tick step` no começo do cenário (a leitura caiu depois do tick seguinte), que sumiu com meio segundo de folga por passo; as outras 4 rodadas deram 0. **Chamar `native_update_output_neighbors` dentro do `block_entity_tick` funciona**: o tile da máquina está fora do mundo, e o nível que o comparador lê vem de uma tabela à parte (`COMPARATOR_LEVELS`), então a linha 49 da Fase 0 ("não notifique dentro do `block_entity_tick`") não vale mais; nenhuma decisão nova (o 1:1 saiu sem opção a medir). Prints e vídeo: `~/.cache/rust-mekanism/wa/comparador/`. **A capacidade na colocação**, em jogo (`h2_placed_fed.py`, as 4 máquinas, 8 ticks cada): 0 diferenças contra `placed_fed_*` (energia 240 no tick da colocação, a capacidade 1.000 e 1.600 no Crusher); sem `placed_capacity` (mutação posta só nos fontes do desktop) a Enrichment Chamber dava gerador 220 e máquina 20 depois do tick da colocação, e 240 só no seguinte.
- Ordem dos eventos de chunk, conferida no jogo (2026-11-07): `dev/referencia/testes/ChunkTraceTest.java` roda `reloadIntermediary` e
  `inaccessibleNotUnloaded` no `gameTestServer` com um cabo que grava cada chamada (ticket, carga, descarga, `onLoad`, `setRemoved`,
  `onChunkUnloaded`, `saveAdditional`, tickers), e `mods/machinery_basis/core/tests/sim_chunks.rs` repete os rastros em
  `dev/referencia/rastros/chunk_*.jsonl` evento a evento, com as posições absolutas deles (12 rastros de rodadas diferentes repetem, 6 ficam guardados).
  O que a leitura do NeoForge já dava se confirmou: ticket antes de gravar, gravar antes de `ChunkEvent.Unload`, todos os
  `onChunkUnloaded` antes de qualquer `setRemoved`, `onLoad` de todos antes do primeiro ticker, tickers antigos antes dos
  recarregados, ticker novo no fim, mapa do chunk na ordem do fastutil. Quatro divergências, corrigidas em `chunks.rs`: (1) a
  gravação não sai na ordem do mapa, e sim na do `HashSet` de `getBlockEntitiesPos` (`sim/java_hash.rs`, conferido contra o JDK 21),
  e o chunk recarregado recria os cabos nessa ordem; (2) `ChunkTicketLevelUpdatedEvent` só sai quando o nível muda; (3) o passo de
  um `gameTest` roda entre os tickers do nível e o `ServerTickEvent.Post` (`tick_level` e `finish_tick`); (4) a carga leva de 5 a
  11 ticks entre o pedido e o `ChunkEvent.Load`, e não 1 (`set_load_delay`; o simulador não prevê quantos).
- Os dois cenários do Mekanism são mais fracos do que o nome diz. O nível posto à mão em `inaccessibleNotUnloaded` (33) volta ao
  dos tickets (31) 2 ticks depois em 2 de 6 rodadas, quando o `LoadingChunkTracker` recalcula o chunk por causa de um ticket
  vizinho (`update_ticket_level`; o simulador não sabe quando o jogo faz). Em `reloadIntermediary` o próprio jogo pode recarregar o
  chunk descarregado 2 ticks depois (`chunk_reload_intermediary_game_reloads.jsonl`), e o pedido do teste vira um `skipped`. E o
  chunk `(1, 0)` do Mekanism é o que contém o meio da estrutura mais 8 em `z`: com `z` dentro do chunk em 8 ou mais (metade
  das rodadas) ele é da fileira vizinha e não tem cabo nenhum (`chunk_*_empty_chunk.jsonl`), e o teste passa sem exercitar a rede.
  Mesmo na fileira certa, a estrutura cruza quatro chunks (3, 16, 16 e 13 cabos, por exemplo), não três de 16, e as coordenadas
  mudam a cada rodada, e com elas a ordem dos mapas.
- `reloadIntermediary` do Mekanism é instável no jogo: 1 falha ("Chunk has not been marked as unloaded yet", timeout no tick
  402) em 16 rodadas completas do `runGameTestServer`, e nenhuma em 71 cópias dele (64 num arquivo temporário, 7 no `ChunkTraceTest`).
  A mensagem só sai se o `ChunkEvent.Unload` do chunk `(1, 0)` nunca chega: o `updateChunkLoading` do teste não faz nada
  quando `isChunkLoaded` já é falso, e o chunk da fileira vizinha (sem cabos, só o ticket da estrutura o segura) pode não estar
  carregado quando o cenário começa. Não reproduzido em rodada controlada, então é a hipótese que a leitura do código sustenta,
  não um fato medido.

Nota: `dev/referencia/rastros/upgrade_table.jsonl` tem 324 combinações medidas no Java, e `mods/mekanism/tests/upgrade_table.rs`
confere o port. O upgrade de energia reduz a capacidade, porque as quatro máquinas aceitam speed.

**Desenho da base (usuário, 2026-11-05).** Duas opções foram implementadas e medidas em jogo, com critérios fixados antes, e ficou a que
se saiu melhor. B é a cópia literal do Java: closures, contêineres compartilhados e transação que registra sozinha quem mudou. C mantinha
traits no lugar das closures, com a mesma transação. Paridade e estabilidade empataram, e o custo decidiu: 200 câmaras, 3 rodadas de 6000
ticks, perfil `dev`, B com 29.206 ns por tick de máquina (desvio 108) e C com 32.251 (desvio 263). A margem é de 10% e não foi medida em
`release`. Medir só depois de as duas fazerem o mesmo trabalho importou: na primeira rodada, B fazia menos e C regravava um JSON a cada
tick, e a diferença parecia ser de 19 vezes.

**Comportamento do original que parece bug e fica.** Limpar os erros deixa `pausedForErrors` em `true`, porque o conjunto vazio não
contém `NOT_ENOUGH_ENERGY_REDUCED_RATE`. Com a máquina sem sinal no modo HIGH, ligar o sinal depois disso não a faz andar: só um
listener despausa (`chamber_paused`, tick 35). PULSE também não existe na Enrichment Chamber: `supportsMode` recusa e cai em DISABLED.

**Fase 2, lados coloridos.** *Estado (2026-11-08, conferido): aberta. Adiantado, na Fase 1: a regra por slot (`slot_accepts`, o `isValid` do `ResourceHandler`) e o ejetor sobre ela, só por comando; falta o resto.*
Porte de `TileComponentConfig`, `ConfigInfo`, `DataType`, `ISlotInfo`, `TransmissionType` e
`TileComponentEjector`: por instância, por tipo de transmissão, com as cores, salvo no NBT, `AttachedSideConfig` como padrão.
Tela de lados, Configurator e os slots por `DataType` (`INPUT_1`, `INPUT_2`, `EXTRA`). Substitui `Sides`, `FaceSlots` e o
`machinery_basis/energy` atual.

Vindo da Fase 1 (movido em 2026-11-08), com o que já está feito e o que falta:

- `TileEntityConfigurableMachine` (`TileEntityConfigurableMachine.java`): é a casca dos dois componentes desta fase por instância, `configComponent` (`TileComponentConfig`, que não tica) e `ejectorComponent`
  (`TileComponentEjector`, que tica em `onUpdateServer` depois do `super`), com `ISideConfiguration` e o par `writeConfigurationData`/`setConfigurationData`, que o Configurator usa para copiar a configuração. As quatro máquinas
  o herdam por `TileEntityRecipeMachine`; o Heat Generator não (`TileEntityGenerator` estende `TileEntityMekanism`). O plano citava a classe na Fase 1 sem dizer a fase; como ela só guarda a configuração e o ejetor,
  cai aqui. Feito: o ejetor tica no tick da máquina, só por comando (`mekanism_eject` e `mekanism_eject_face`), com a regra do slot do vizinho e a composteira (rastro `chamber_eject_furnace`). Falta: o componente de configuração, salvo no NBT, e o ejetor guiado por ele.
- Máquina posta por `setblock`: no original `ConfigInfo` nasce em `NONE` e só `applyImplicitComponents` aplica o padrão do item (lido no código, não gravado), e o port trata toda máquina como se viesse do item. O hopper embaixo do Infuser e o `Access::NotExternal` dos
  slots de entrada já seguem o padrão do item (Fase 1, feito).
- Faces por instância: o original responde pela capability por lado e por instância, o port por tipo de bloco (H1 e H2; só a capacidade de energia é por instância).
- As abas de configuração de itens (-21, 10) e de transporte (-21, 38): a lógica é daqui, e o desenho da Fase 5; entram no registro `screen_elements` quando portadas.

**Fase 3, rede de transmissores.** *Estado (2026-11-08, conferido): aberta. Adiantado: um cabo de energia simplificado (buffer só nos cabos de entrada, ver o inventário); falta o resto.*
Porte de `TransmitterNetworkRegistry`, `OrphanPathFinder`, `DynamicNetwork`,
`DynamicBufferedNetwork`, `Transmitter`, `BufferedTransmitter`, `AcceptorCache`, `NetworkAcceptorCache`, `EnergyNetwork`,
`UniversalCable`, `Target` e `SplitInfo`: buffer somado, save shares, fusão e divisão de rede, chunks, `ConnectionType`, pull,
redstone reativo, upgrade de tier no lugar, `currentScale` e `PacketNetworkScale`. Substitui `network.rs`, `transmit_energy` e o
cache de 20 ticks. A base genérica já precisa servir fluido, químico, item e calor.
Vindo da Fase 1 (movido em 2026-11-08): a lava vinda de cano. O tanque do Heat Generator (`lavaTank`, `VariableCapacityFluidTank.input` que só aceita lava, exposto por todas as faces menos a da frente, `TileEntityHeatGenerator.java:98-103`) não recebe de cano no port; nada disso está feito além do tanque e do balde, e pede
o fluido por instância (H2, `native_fluid_at`) e a rede de fluido desta fase.

**Fase 4, visual do cabo.** *Estado (2026-11-08, conferido): aberta. Adiantado: um modelo de cabo em JSON aproximado; falta o resto.*
Modelo original com todas as partes, textura com recorte (não a `opaque/`), `hideContiguousJoin`,
conteúdo `mek_liquid/energy` animado com alfa pela escala, opção `opaqueTransmitters`, forma de seleção e colisão por lado
(`BlockSmallTransmitter`: `SIDES`, `CENTER`, flanges).

**Fase 5, refazer os blocos prontos sobre a base.** *Estado (2026-11-08): em andamento (as frentes de tela, upgrades e abas). Já saiu: as telas das quatro máquinas e do Heat Generator
com os sprites e widgets do original, comparadas em pixel contra o original (`dev/referencia/comparar_lote.py`, 129 estados em guiScale 2 e 3; ver "Tetos dos resíduos"); upgrades (aba
e janela, nas cinco); aba de XP, de energia e de avisos nas máquinas e de energia, de calor e de upgrades no gerador; som das quatro máquinas; luz do bloco aceso (8 e 13); nome com estilo;
o fundo escurecido das telas (injeção `container_dim`); o tooltip da lista de upgrades; os slots de armadura e de mão do gerador (2026-11-07). Falta: as partículas das máquinas e do gerador; as abas de
configuração, segurança, redstone e livro de receitas; o slot de energia funcional e a transferência de calor do gerador; o som do gerador; o conteúdo guardado no item; o Steel Casing não foi
revisado nesta frente. A remoção da bateria `mek_teste` está feita (conferido em 2026-11-08: nenhum item ou bloco com esse nome no conteúdo nem no código).* Heat Generator, Metallurgic Infuser, Enrichment Chamber, Crusher,
Energized Smelter e Steel Casing, um a um, conferindo o inventário. Telas com os sprites e widgets originais, energia em slot,
upgrades, conteúdo guardado no item, som, partículas e luz. A bateria de teste `mek_teste` já saiu do build.

Vindo da Fase 1 (movido em 2026-11-08), com o que já está feito e o que falta:

- Partículas da máquina acesa e do gerador: nada feito, e sem decisão (inventário, "Partículas da máquina acesa"; "Esperando decisão do usuário (fim da Fase 1)").
- Abas de lado (os elementos fora da imagem): feitos o registro `screen_elements` e as abas de energia, de avisos, de xp, de upgrades e, no gerador, a de calor. Faltam, nas máquinas, a configuração de itens (-21, 10) e de transporte (-21, 38) à esquerda
  (lógica na Fase 2) e a segurança (179, 38), o livro de receitas (179, 66) e a redstone (179, 141) à direita; no gerador, a segurança, a redstone e a de avisos à direita. Enquanto faltam, o clique nelas com item no cursor solta o item (no original não solta).
- Som do gerador: nada feito (`GeneratorsSounds.HEAT_GENERATOR`). O `tile-sound` das máquinas e os quatro comportamentos do original que parecem bug (seção própria) valem para ele, e o `sync_muffling` do gerador ainda manda a contagem de silêncio na hora, onde o original
  (`TileComponentUpgrade.removeUpgrade`) não avisa o cliente.
- Conteúdo guardado no item: nada feito. O item de bloco do port não traz `UPGRADES` nem o tanque (`addAttachmentOnlyContainers`, `MekanismBlocks.java:305` e `:375`), e o bloco quebrado solta o conteúdo; em jogo só o par (0, 0) de upgrades se repete.
- Slot de energia funcional do gerador (`EnergyInventorySlot.drain`, nenhum item portado o alimenta) e a transferência de calor pela base (`simulateAdjacent`, `lastTransferLoss` vale 0): nada feito.
- Sincronização das cinco peças da barra de armadura quando mudam por fora com o menu aberto: segue a do menu do inventário, um tick depois.

**Fase 6, seguir a cadeia com paridade.** *Estado (2026-11-08): aberta, nenhum bloco ou item portado; o que a Fase 1 deixou para cá está na lista ao fim do parágrafo.* Itens com capability de químico e o que a lib já tem para eles (2026-11-08): `fillTankFromSlot` e `ChemicalAttributeValidator` estão na lib, com testes, e nada portado os alimenta; o
`setLastTransferDirection` que o Java grava no slot dentro do `fillContainerFromSlot` não foi portado, porque só o `handleContainer` o lê e nenhum slot portado o chama. Os itens com capability de químico do original são o Chemical Tank
(`ItemBlockChemicalTank`, `MekanismBlocks.java:973`), o Gauge Dropper, o flamethrower, o scuba tank e os jetpacks (`MekanismItems.java`, `addAttachedContainerCapabilities(ContainerType.CHEMICAL, ...)`), nenhum portado; os itens das
máquinas usam `addAttachmentOnlyContainers` (`MekanismBlocks.java:305` e `:375`), que guarda o tanque no item sem expor capability (é o "conteúdo guardado no item" da Fase 5). O tanque do Infuser
(`TileEntityMetallurgicInfuser.java:135`) valida com `containsRecipeB`, que recusa qualquer químico radioativo antes do `ChemicalAttributeValidator.DEFAULT`, então os mapas de dados
`chemical_attribute_*` (sodium, superheated_sodium, ethene, hydrogen, nuclear_waste, plutonium, polonium, spent_nuclear_waste) não mudam nada nas máquinas portadas: nenhum é lido hoje para o `ChemicalAttributes`, e entram com o Chemical Tank e com os itens acima. Pendência herdada da Fase 1: Osmium Compressor, Purification Chamber e Chemical Injection Chamber, as máquinas de `per_tick_usage` que só existem no original; a mecânica (`ItemChemicalConstantUsageRecipe`, `usedSoFar`) já está na lib e conferida com o Compressor e o Infuser, falta cada bloco (valores, receitas, texturas, tela). Energy Cube e Energy Tablet, Solar Generator, minérios de estanho, chumbo, urânio e
fluorita, tubos, canos e transportadores sobre a rede da Fase 3.

Vindo da Fase 1 (movido em 2026-11-08), com o que já está feito e o que falta:

- Itens de químico e o que liga a lib a eles: feitos `fillTankFromSlot` e `ChemicalAttributeValidator` (lib, com testes); faltam o Chemical Tank e os itens acima, os mapas de dados `chemical_attribute_*` e o `setLastTransferDirection`.
- As máquinas a mais e as receitas delas: Osmium Compressor, Purification Chamber e Chemical Injection Chamber (as de `per_tick_usage`; a mecânica está na lib e conferida com o Compressor e o Infuser), com valores, receitas, texturas e tela. Chegam junto as 24
  conversões `chemical_conversion` do original (o port tem 19) e as receitas que citam item que o port não tem (bronze, estanho, chumbo, urânio, fluorita).
- `EitherSide`, `Triple` e `Rotary`, os limites de taxa, e os tipos de upgrade que nenhuma máquina portada aceita: entram com a primeira máquina que os usa.
- As 143 tags `c:` da datagen que faltam (de item e de bloco), e comparar o que o Horizon descarta em silêncio (entrada ou `#tag` desconhecida) com cada receita portada quando esses itens chegarem.
- A decisão 3 (duas entradas com regra de slot) é decidida com o Combiner.
- O bio fuel em bloco (`COOKING_TIME_BIO_FUEL_BLOCK`, 10 vezes o do item) com o bloco; o tempo de queima do item é da Fase 1 (aberto, inventário).

### Tetos dos resíduos do comparador de telas (2026-11-07)

`dev/referencia/comparar_lote.py` compara, em guiScale 2 e 3, os 129 estados de `cena_telas.py` entre o original (`CenaTelas.java`, no `mekanism-ref`) e o port
(`horizon`), pixel a pixel, sem tolerância fora do tooltip. O que o Horizon desenha diferente de verdade (H22 a H26, H30) é resíduo, mas **só até o teto medido**:
passar dele é diferença de verdade. Cada teto tem teste (`test_comparar_tela.py`) e mutação (`telas-tools/mut_py6.py`).

| Resíduo | Teto (guiScale 2 / 3) | De onde vem |
|---|---|---|
| Barra de químico do Infuser (`barra_quimico_fase`, H26) | 192 px e 1 nível / 342 px e 1 nível | O pior print de cada ciclo do original contra o quadro mais próximo do port: carbono 0 / 0, fungi 60 / 99, bio 104 / 162, bio 800 108 / 270, bio 1000 192 / 342. Os casos sem ciclo (tanque parado) deram de 0 a 54 px e 1 nível (`infuser_operacao_10` 0 / 90, `aba_energia_hover_ativa` 28 / 27, `aba_energia_hover_velocidade` 16 / 0). Sem o "pior custo do próprio ciclo" que antes alargava o teto: um nível ou uma cor errados passam de 1 nível |
| Medidor de lava (`medidor_lava_fase`) | 0 px e 0 níveis / 0 e 0 | Os ciclos a 500 e a 1000 mB e com o cursor no medidor (`generator_lava_ciclo`, `_fase_lava_1000`, `_fase_lava_hover`): 40 de 40 prints do original idênticos a um quadro do port, nas duas escalas; e os casos de medidor parado (`generator_lava_meio`, `_lava_cheia`, `_hover_barra`, `_hover_medidor`) idênticos ao ciclo do mesmo nível |
| Ícone de cada linha da lista de upgrades (`janela_lista_icone_*`, H25) | 0 px / 107 px e 206 níveis | Em guiScale 2 todos os estados com a janela dão 0. Em guiScale 3 o pior é 107 de 576 pixels, e só vale se o mesmo estado deu 0 em guiScale 2: sem isso, um ícone trocado aparece |
| Cantos do tooltip de item (`tooltip_canto_*`, H23) | 6 px e 72 níveis / 21 px e 72 níveis | `chamber_hover_slot_item` |
| Mundo atrás da tela (`mundo_atras`, os cantos transparentes e a sombra da janela) | 1 nível por canal, por nível de luz do bloco: 0, 8 e 13 | Depois da injeção `container_dim` (H22): os controles com fonte vanilla `minecraft:light` de nível 4, 8, 12 e 15 deram 1 por canal (antes 4, 4, 8 e 12), e o lote inteiro dá no máximo 1. **Sem causa provada** (H30, e o resto do escurecimento, H22): o relatório marca o estado `NE` (não explicado), nunca `ok` |

Fora dos tetos, o relatório separa o que sobra: **diferença conhecida** (`particulas_da_maquina`: a fumaça e a poeira de redstone de `Machine.java:23`, que o port não solta,
na caixa onde caem, no estado aceso e no caso logo depois de um aceso, porque o original roda os casos em sequência no mesmo lugar e as partículas ainda estão no ar).

A barra de armadura do Heat Generator deixou de ser "não portada" em 2026-11-07 e conta como elemento do mod: o fundo vazio de cada peça (`fundo_armadura_0` a `_4`, 16 por 16), o quadro de cada slot (`slots_armadura`, 18 por 18) e o suporte
(`suporte_armadura`, `holder_left` 26 por 98, tingido) não têm teto nem resíduo, e uma diferença em qualquer um é `DIF` e faz o lote falhar. Os cantos transparentes do suporte (6 pixels, `canto_suporte_armadura`)
são o mundo atrás, como os das abas. O recorte de todo caso do gerador inclui a coluna (`FORA_DA_TELA`), com ou sem margem, e sob a sombra da janela de upgrades a barra passa por baixo dela e conta como barra,
não como `janela_fundo`.

**Pixel da barra de armadura contra o original (2026-11-07).** O `CenaTelas` ocultava o suporte (`GuiSideHolder`, `active = false`, como todos os suportes que o port não tinha), então os prints antigos do original
(`of_final2`) não serviam para o suporte: com eles, o suporte do port dava 3688 de 3688 pixels diferentes em guiScale 2 e 8298 em 3 (2492 e 5607 nos 7 casos com a janela aberta), e os fundos e os quadros 0. O `CenaTelas` agora
deixa visível só o suporte da barra (`isArmorHolder`: `GuiHeatGenerator`, em (-26, 6); os da segurança e da redstone, à direita, seguem ocultos, o port não tem essas abas: a primeira tentativa, que deixou todo
`GuiSideHolder` da tela visível, mostrou o cadeado e a redstone no original e o lote acusou 5360 pixels de `alem_mundo_atras` à direita) e o original dos 30 casos do gerador foi recapturado nas duas escalas
(`runGameTestClient`, `mekanism-ref` ocioso nas 3 conferências com intervalo). Resultado, nos 294 estados (27 casos de estado único e 3 ciclos de lava de 40 prints, guiScale 2 e 3): `fundo_armadura_0` a `_4`,
`slots_armadura`, `suporte_armadura` e `canto_suporte_armadura` com **0 pixel diferente** em todos (guiScale 2: 150 mil pixels comparados por fundo, 198 mil nos quadros e 536 mil no suporte; guiScale 3: 338 mil,
445 mil e 1,2 milhão), inclusive sob a sombra da janela de upgrades. O teste do comparador pode falhar: com o original antigo (suporte oculto) o mesmo lote dá os 3688 pixels. O que sobra nos 294 estados é o que já era: o
mundo atrás da tela até 1 nível por canal (`NE`, H22 e H30), os cantos das abas, `janela_aceitos` e o ícone da lista (H25), nenhum fora dos tetos; o lote termina com 0 diferenças fora dos resíduos e 0 sem print.
Os `alem_janela_fundo` (1164 e 2619) de antes eram a coluna sob a sombra, e sumiram. Tabelas e prints do lote no desktop em `~/.cache/rust-mekanism/telas-final/` (`original2/`, `port` em
`~/.local/share/jogo-isolado/horizon/telas-final/port/`, `saida_final/`).

Provas das diferenças que o lote achou (2026-11-07): o `tooltip` de `chamber_aba_hover_lista` (9602 px em guiScale 2, 21603 em 3) era do port: `GuiUpgradeScrollList` usa
`TooltipUtils.create`, que é `Tooltip.create` (quebra em 170 pixels, sem a folga do título), e o port desenhava a descrição numa linha com o espaçamento de tooltip de item
(corrigido em `windows.rs`, 0 pixel depois); `alem_janela_fundo` (1164 e 2619) é a coluna de armadura sob a sombra da janela (a contagem dos pixels diferentes na interseção dá
exatamente esses números); `alem_mundo_atras` em `crusher_aba_energia` (713), `smelter_aba_energia` (226) e `generator_aba_energia` (180) são partículas da máquina do caso anterior
(os três casos têm energia 0 e vêm logo depois de um caso aceso). O nível da lava queimando continua coberto: o caso de tela `generator_queima` (lava a 500, energia 0, 100 passos)
dá 0 pixel no medidor, e `heat_trace.rs` repete o rastro `heat_generator_lava_run` do Java tick a tick (300 ticks).

## Inventário do que diverge hoje

**Rede e cabo**

- Sem objeto de rede: buffer só nos cabos de entrada, em vez da soma da rede. Aceita menos por tick.
- Sem save shares: quebrar um cabo de entrada perde a energia dele.
- Travessia guardada por 20 ticks: mudanças levam até 1 s, e nesse tempo a energia pode entrar por uma face errada.
- Sem `ConnectionType`, pull, redstone reativo e upgrade no lugar.
- Modelo em JSON aproximado, textura `opaque/`, sem o recorte do braço, sem verde animado, sem flange, emenda visível.
- Faces declaradas por tipo de bloco; o original responde pela capability por lado e por instância.

**Máquinas (infuser e elétricas)**

- Faces fixas e sem tela de lados. O ejetor existe, mas só liga por comando (`mekanism_eject`). **Fase 2** (movido em 2026-11-08).
- `CachedRecipe` portado, e a tela mostra os erros e os avisos (feito em 2026-11-07 e 2026-11-08: a aba de avisos, o overlay das barras e os dados do `MekanismContainer`, abaixo). `per_tick_usage` só no Infuser, sem receita dos arquivos que o use, mas conferido tick a tick com o original por uma receita de teste (`infuser_constant_trace.rs`).
- Clique do cursor (feito, 2026-11-07). Relato: terra com o cursor entrava no slot de infusão do Infuser, e o clique (e o arrasto) punham qualquer item em
  qualquer slot da máquina. O original usa `InventoryContainerSlot.mayPlace`, que é `isValidForInsertion(item, MANUAL)` do slot. **Medido no
  Java** (`SlotClickTraceTest`, `slot_click_<máquina>.jsonl`, 4 máquinas e o Heat Generator, 15 itens por slot, o slot vazio, com outro item e com o mesmo,
  clique esquerdo, direito, tecla numérica e arrasto; o Infuser em 6 estados de tanque e entrada, 2.880 linhas): o `mayPlace` de um (estado, slot, item) é o
  mesmo com o slot vazio ou cheio e com qualquer clique, e a saída o recusa (`ModResult`). O motor de menu faz o resto, e o do Horizon bate com o do Java.
  **Como:** `menu_slot_rules` v1 (o hook do Horizon, H20) não tem o `Game` nem a posição, e as regras do Infuser olham o tanque e a entrada, então a regra do
  bloco aberto é deixada em `slot_rules` (núcleo) antes de cada clique, e o crate `[api]` da `machinery_basis` responde ao hook com ela. Regra de
  inserção: `rules::slot_accepts` (`runtime.rs`), a mesma do hopper e do shift-click. Em jogo (`horizon2`, `h2_cursor.py`, o menu e os cliques de verdade): 311 casos, 0 diferenças contra as linhas do Java (slot, cursor, barra e slot do arrasto): Enrichment Chamber 56, Crusher 56, Energized Smelter 21 (os cliques do jogador com o slot vazio), Metallurgic Infuser 114 (tanque vazio, bio e fungi, com e sem terra na entrada: a terra deixa de entrar no slot de infusão, e o `bio_fuel` e a `redstone` entram só onde o Java os aceita) e Heat Generator 64 (combustível e energia, com o tanque vazio e cheio de lava, servidor parado). Clique esquerdo, direito, tecla numérica (o cursor sobre o slot e `key 1`) e arrasto para o primeiro slot do inventário, com o slot vazio, com outro item e com o mesmo. O que ficou de fora: o convidado de LAN (o clique segue o mesmo `lan_machine_click`).
  **Q, clique do meio, clique duplo, baldes empilhados e os slots de upgrade (2026-11-08).** `SlotClickTraceTest` ganhou o Q (`THROW`, uma unidade, e Ctrl+Q, a pilha), o clique do meio (`CLONE`, no criativo),
  o clique duplo (`PICKUP_ALL` sobre o slot vazio, com o item nos outros slots e no inventário), os baldes com `max_stack_size` 16 no componente (`lava_bucket+16`, `water_bucket+16`, a mão segura mais de um) e,
  em `slot_click_upgrade_<máquina>.jsonl`, os dois slots virtuais da janela de upgrades com os oito tipos. O que o Java faz: o Q larga uma unidade (Ctrl+Q a pilha) de **qualquer** slot, inclusive a saída, a energia e a
  infusão (`AbstractContainerMenu.java:516`), sem olhar o `mayPlace`; o do meio põe no cursor a pilha cheia do item do slot (`:511`); o clique duplo recolhe o item de **todos** os outros slots da máquina, inclusive saída e energia,
  e do inventário (`:537`; `MekanismContainer.canTakeItemForPickAll`, `MekanismContainer.java:179-187`, só barra o slot de uma janela não selecionada), e o balde de lava e o de água, que não empilham, ficam onde estão; um balde com o
  componente `max_stack_size` 16 entra inteiro no slot de combustível (3 de 3, o limite do slot é o menor entre o dele e o da pilha), e sem o componente o Java põe 1 e deixa 2 no cursor. A entrada de upgrades
  (`UpgradeInventorySlot.input`, `UpgradeInventorySlot.java:25-30`) aceita o upgrade de um tipo da tag da máquina (energia, silêncio, velocidade e xp nas quatro, só o silêncio no gerador) e nenhum item comum, e a saída
  (`internalOnly`) não aceita nada. **Divergência achada e corrigida: um upgrade na mão não entrava em nenhum slot da janela.** A regra de slot do clique (`slot_accepts`, 2026-11-07) só conhece a entrada, a energia e a infusão
  e vetava todo o resto, inclusive o slot de entrada de upgrades (o shift-clique passa por outro caminho e entrava): o clique esquerdo, o direito, a tecla numérica e o arrasto sobre a janela deixavam o upgrade no cursor
  (medido em jogo antes da correção, `click-slot 303` com 3 de velocidade na mão: slot vazio, cursor com 3). **Como:** o hook `menu_slot_rules` v1 já entrega a pilha, e a regra do mod só via o id; `slot_rules::ask` passa agora a pilha como
  `&dyn Any` (a lib não conhece o tipo do jogo), e as regras do Mekanism (`publish_slot_rules`) e do Generators (`menu_click_before`) leem o componente `mekanism:upgrade_type` dela (`StackData::other`) e perguntam ao `UpgradeData::supports`.
  Sem injeção nova (é o mesmo hook do catálogo). **Em jogo** (`horizon`, build `native-f319701-2c628113bfe2`, `h2_cursor_extra.py upgrade`, servidor parado, janela aberta pela aba): 36 casos por máquina (20 no gerador), 0 diferenças de aceitação e de
  resultado do clique (slot, cursor, barra e slot do arrasto) na Enrichment Chamber, no Metallurgic Infuser e no Heat Generator; ficam de fora os casos com conteúdo prévio que a regra recusa (o `setContents` do rastro o põe
  sem passar pela regra, o jogo não): 2 por máquina nas quatro e 6 no gerador. **Q e Ctrl+Q em jogo** (`h2_cursor_extra.py throw`, pelo teclado sobre o slot, o servidor parado onde a máquina converte ou queima): Enrichment Chamber 72 casos, Heat Generator 96 e Metallurgic Infuser 96 (estado bio com terra),
  o slot depois e quantos itens caíram no chão: 0 diferenças em 58, 88 e 88; os 22 que ficam de fora são os de uma pilha de 4 baldes de lava ou de água no slot (o rastro a pôs por `setContents`, que o jogo, sem o componente, limita a 1).
  **Baldes empilhados em jogo** (`h2_cursor_extra.py stacked`, `lava_bucket[minecraft:max_stack_size=16]` na mão): Heat Generator 64 casos, Chamber 48 e Infuser 64, 0 diferenças contra as linhas `lava_bucket+16` e `water_bucket+16` (a
  mão com 3 baldes entra inteira no slot de combustível, 3 de 3). As linhas antigas, com 3 baldes de pilha 1, não têm equivalente em jogo: o jogo não deixa a mão segurar mais que o máximo da pilha. **Não alcançável pelo `horizon-ctl`:** o clique do meio (`mouse` só age no mundo e `click-at`
  só tem o esquerdo e o direito) e o clique duplo (dois `click-at` seguidos num `script` pegam e põem de volta, sem `PICKUP_ALL`): ficam só no rastro do Java e em `slot_click.rs`
  (`the_original_throws_clones_and_collects_without_asking_the_slot_rules`), até o Horizon ou um `xdotool` na imagem do jogo os mandar.
- Feito (2026-11-05 a 2026-11-07): upgrades (aba e janela, 2026-11-06), a aba de XP, a de energia e a de avisos, o controle de redstone e o comparador do tile, o som, o nome com
  estilo, a luz e o tooltip da barra de químico com o nome do químico (`infuser_hover_barra_quimico`, 0 pixel contra o original). **Falta**: as partículas (abaixo, Fase 5); a aba de
  configuração de itens e a de transporte (à esquerda: a lógica é da Fase 2 e o desenho da Fase 5), a de segurança, a de redstone e o livro de receitas (à direita, Fase 5): a cena do original as oculta e o port não as tem.
- Conteúdo solto ao quebrar, em vez de guardado no item. **Fase 5** (movido em 2026-11-08).
- Luz do bloco aceso (feito, 2026-11-07): `Attributes.ACTIVE_LIGHT` é 8 nas quatro máquinas (`Machine.java:26`, `AttributeStateActive.java:49-51`) e 0 apagadas; o port herdava 13 do
  gabarito `minecraft:furnace` aceso, porque `properties.light` do Horizon é um inteiro por bloco. A injeção `active_light` troca a emissão dos estados acesos dessas quatro por 8; o
  Heat Generator segue em 13 (`ACTIVE_MELT_LIGHT`). Em jogo, `horizon-ctl light`: máquina acesa 8, vizinhos 7, 6, 5 (a propagação do Java), apagada 0. O mundo atrás da tela com a máquina
  acesa deixou de precisar de um limite à parte (era 16): vale o limite do nível de luz (`LIMITE_MUNDO_POR_LUZ`).
- **Partículas da máquina acesa (aberta, não portada; Fase 5, sem decisão).** `Machine.java:23` registra um `AttributeParticleFX` com duas partículas, `ParticleTypes.SMOKE`
  e `DustParticleOptions.REDSTONE`, ambas em `(rand * 0,6 - 0,3, rand * 6/16, 0,52)` relativo ao centro do bloco (a face da frente, girada pelo `facing`:
  `yRot(PI)` para norte). `BlockTile.animateTick` (`BlockTile.java:80-103`) as solta com `world.addParticle(type, x, y, z, 0, 0, 0)` a cada chamada de
  `animateTick` (o cliente sorteia posições em volta do jogador a cada tick), só com `Attribute.isActive(state)` e `machineEffects` ligado
  (`MekanismConfig.client.machineEffects`). Valem as quatro máquinas, e o Heat Generator tem as dele (`GeneratorsBlockTypes.java:69`: `SMOKE` e `FLAME`, em `z = -0,52`). Nenhuma está no port. O catálogo de hooks do Horizon (pin) não tem `animateTick`; só o helper
  `send_particles` (`native_level_send_particles`, `host_bridge`), que pede a partícula pelo servidor. Um caminho seria o tick do bloco chamar esse helper, o que muda
  quem sorteia (o servidor, não o cliente, e sem `machineEffects`); não decidido. No comparador elas aparecem como `particulas_da_maquina`, diferença conhecida contada à parte
  (`comparar_lote.DIFERENCAS_CONHECIDAS`, `CAIXA_DAS_PARTICULAS`), nunca como resíduo do Horizon.
- Barras com os sprites do Mekanism (feito em 2026-11-07, `textures/gui/sprites/bar` e `gauge`; comparadas em pixel contra o original em `dev/referencia/comparar_lote.py`, com os tetos de "Tetos dos resíduos").
- `bio_fuel` sem tempo de queima (aberto): o original o registra com `cookingFuel(COOKING_TIME_BIO_FUEL)` (`MekanismItems.java:182`, 5 segundos, 100 ticks, `MekanismDatapackRegistries.java:231-233`), e o item do port (`horizon/item/bio_fuel.json`) não tem o componente `minecraft:cooking_fuel`, então o forno e o slot de combustível do Heat Generator (`burn_time` do jogo) o recusam. Vem do dado, sem injeção (Fase 0, "Tempo de queima").
- Tags `c:` escritas à mão: conferidas contra o jar do NeoForge 26.3.0.100 (feito, ver a Fase 1, item de receitas); as 143 da datagen que faltam estão em "Esperando decisão do usuário (fim da Fase 1)".

**Heat Generator**

- **Temperatura do ar por altura (feito, 2026-11-08).** **Java** (lido por inteiro): `TileEntityMekanism.getAmbientTemperature(side)` (`TileEntityMekanism.java:1310-1315`) devolve
  `ambientTemperature.getTemperature(side)` (`CachedAmbientTemperature.java:30-45`): um `double[7]` por tile (seis faces e o bloco, `-1` como vazio), preenchido na primeira leitura com
  `HeatAPI.getAmbientTemp(world, pos.relative(side))` e **nunca renovado** (só um tile novo lê de novo; o gerador lê o do bloco em `simulate`, `TileEntityHeatGenerator.java:196`, e o de cada face em
  `ITileHeatHandler.simulateEnvironment`, `:53`). `HeatAPI.getAmbientTemp(level, pos)` (`HeatAPI.java:42-47`) lê `world.getBiome(pos).value().getTemperature(pos, world.getSeaLevel())` e leva a
  kelvin com `300 + 25 * (clamp(t, -5, 5) - 0,8)` (`:60-65`). `world.getBiome(pos)` é o `BiomeManager.getBiome` (`BiomeManager.java:35-72`, o quadrado de 4 por 4 mais próximo de (x - 2, y - 2, z - 2)
  com sorteio por quadrado), não o quadrado que contém o bloco. `Biome.getTemperature` (`Biome.java:130-143`) guarda o valor em um cache por bioma e por thread, de 1.024 entradas, com a chave
  `pos.asLong()` e sem o nível do mar; o cálculo é `getHeightAdjustedTemperature` (`:118-127`): o `temperatureModifier` do bioma (`none` ou `frozen`, `:419-441`) e, se `pos.getY() > seaLevel + 17`,
  `t - (v + y - snowLevel) * 0,05F / 40F` com `v = TEMPERATURE_NOISE.get(x / 8F, z / 8F) * 8F`, um `SimplexNoise(new WorldgenRandom(new LegacyRandomSource(1234L)), true)` (`:59`) em duas dimensões;
  o `frozen` lê `FROZEN_TEMPERATURE_NOISE` (três camadas, semente 3456) e `BIOME_INFO_NOISE` (2345), `:61-72`. O nível do mar é o do gerador de chunks (`ChunkGenerator.getSeaLevel`):
  `noise_settings` 63 no overworld, 32 no nether e 0 no end, e -63 num `FlatLevelSource` (`FlatLevelSource.java:147`). O gerador gerado por `/setblock` no alto de um mundo plano já está acima
  de `seaLevel + 17 = -46`.
  **Horizon:** tem a mesma conta (`BiomeInfo::temperature_at` -> `horizon_world::worldgen::biome_temperature`), mas só dentro do jogo e com o nível do mar fixo por dimensão em 63, 32 e 0
  (`weather.rs:11`: um mundo plano também dá 63); tem o `BiomeZoom` do `BiomeManager` (`game.streaming.biome_zoom`) e sabe que o mundo é plano (`world_flat`). **Port:** o ruído e a conta foram
  portados na lib (`machinery_basis_core::biome_temperature`, para o teste repetir o Java sem o jogo), e o glue lê o bioma pelo `BiomeZoom` do Horizon, o nível do mar por `world_flat` e a dimensão,
  e guarda o ar por objeto (`heat::AmbientCache` na memória do tile do gerador, que o resto da vida do objeto já usa). Antes disso o port lia a temperatura base do bioma, do quadrado que contém o bloco.
  **Rastros** (`HeatAmbientTraceTest`, `dev/referencia/testes/`): `heat_ambient_table` (12 biomas, 6 colunas, 10 alturas e 4 níveis do mar; 1.800 posições do modificador `frozen`; os níveis do mar; o
  cache do bioma; posições reais do nível de teste), `heat_ambient_height` (12 geradores em 4 colunas por 3 linhas, alturas de -46 a 300, desert, plains, snowy_plains, jungle, frozen_ocean, taiga e
  badlands, dois com a face leste de outro bioma, dois sem lava; 160 ticks), `heat_ambient_cache` (o bioma debaixo de dois geradores muda e o tile é recriado do NBT em seguida; 300 ticks) e
  `heat_ambient_game_{desert,frozen}` (7 alturas na mesma coluna, 200 ticks, o que o jogo repete). **Testes:** `machinery_basis/core/tests/biome_temperature.rs` (todas as linhas da tabela iguais ao bit:
  bits do `float` da temperatura e o `double` do ar) e `mekanismgenerators/tests/ambient_trace.rs` (os quatro rastros de gerador tick a tick: calor, temperatura, energia, lava, produção, perda para o
  ambiente, o ar do bloco e de cada face, e o ar que o mundo daria sem o cache). **Mutações** (uma por vez, cada uma quebra o teste): sem o ajuste por altura, limiar em `seaLevel + 16` e `>=`,
  semente 1235, 3457 e 2346, divisor 4 em vez de 8, multiplicador 7 em vez de 8, deslocamento do ruído sem descartar, modificador `frozen` ignorado, limiar do gelo 0,2, variação pequena 0,7, valor do gelo 0,3,
  frequência 0,06, divisor 41, 0,06 em vez de 0,05, nível do mar -63 trocado por 63 e o cache refeito a cada tick; sobrevivem só os limiares do gelo e da variação pequena movidos 0,01 (0,31 e 0,79), que nenhuma
  das 1.800 posições da grade fica entre os dois valores, e o último dígito da amplitude da terceira camada (0,5714285 por 0,5714286, o mesmo `float` no resultado), além do `nextDouble` em `double`, que o
  deslocamento descartado (escala 0) torna sem efeito.
  **Em jogo** (desktop, `horizon`, build `native-f319701-4fa47f56a278`, mundo plano, `dev/referencia/horizon2/h2_temperatura.py`): `heat_ambient_game_desert` e `heat_ambient_game_frozen` repetidos
  nas mesmas coordenadas do rastro (7 geradores na mesma coluna, y = 6, -46, -44, 40, 100, 200 e 300, `fillbiome` em volta de cada um, o servidor parado e `tick step 1`; depois de cada tick o calor, a
  energia e a lava saem do NBT do tile e vão contra a linha do rastro): 40 ticks e 280 leituras de gerador por bioma, 0 diferenças. O calor é o `double` do NBT igual ao bit, e ele carrega o ar do bloco e o de
  cada face de todos os ticks, então é o ar do original a cada altura (328,3671 K a y = 6 e 319,1796 K a y = 300 no deserto; antes da mudança o port lia 330 K nas duas). `heat_ambient_cache` (3 geradores,
  135 ticks): o bioma muda no tick 60, o gerador segue com o ar velho até o tile ser recriado no tick 120 (o bloco sai e volta no mesmo tick), 0 diferenças; o gerador de cima ficou 60 ticks com 322,2 K
  enquanto o mundo dava 272,2 K, como no original. Os geradores não ficam encostados (dois Heat Generators vizinhos trocam calor pela face em `simulateAdjacent`, que o port não troca: `lastTransferLoss`
  vale 0). O tanque do port guarda 1.000 (`TANK_CAPACITY`), o `heatTankCapacity` padrão do original (`GeneratorsConfig.java:110-111`, `FluidType.BUCKET_VOLUME`), e o rastro tem milhares por causa do `setContents` do cenário, que não confere a capacidade: o script devolve ao tanque o que o rastro tem, até 1.000, a cada tick. O zoom do bioma (`BiomeZoom`) usa a semente do
  mundo do Horizon e não a do jogo de teste do original, por isso o cenário põe o bioma em 25 por 25 por 25 blocos em volta de cada gerador, onde as duas sementes dão o mesmo bioma; o `BiomeZoom` em si é
  o do Horizon, não conferido contra o Java aqui. Mídia: `~/.cache/rust-mekanism/wa/temperatura/` (a aba de calor a y = 6 e a y = 300, e as tabelas original contra port).
  **Fica diferente:** o cache de `Biome.getTemperature` (que ignora o nível do mar, de modo que a mesma posição lida em duas dimensões com o mesmo bioma devolve o valor da primeira) não está
  portado: decisão 10. Também de fora: dimensão de data pack com outro `noise_settings` (o nível do mar sairia do JSON) e predefinições que o Horizon não oferece (`floating_islands`, -64); um chunk
  não carregado dá 300 K (o Java o carregaria).
- O atraso de desligar do bloco está portado: `lit` segue por `blockDeactivationDelay` (60 ticks) depois que o gerador para, com
  `currentActive` e `updateDelay` salvos (`active_state`, `update_delay`). `heat_trace.rs` compara o `block_active` tick a tick, e em jogo
  o bloco ficou aceso 60 ticks depois do segundo desligamento (`update_delay` 60, 30, 1, 0).
- Conferido em jogo (2026-11-06): o glue do Generators compila; queima com lava no tanque, com carvão e com balde; `face_ambient` lê o
  bioma de cada vizinho (262,5, 280 e 330 K com `fillbiome` em baixo, sul e leste); `lava_faces` conta as faces com lava.
- A barra de energia: o "58→66 contra 60" não se reproduz. Com a mesma cena nos dois jogos (`generator_queima`, energia 0, lava 500, 100
  passos) o servidor do original e o `MEKTRACE` do port dão energia 20, lava 0 e temperatura 300,3102685960236, e em `generator_lava_meio`,
  `generator_lava_cheia` e `generator_hover_barra` a barra tem 0 pixel de diferença em escala 2 e 3. Os números antigos vinham
  de uma cena em que o gerador queimava lava durante o print, com a energia subindo 1 a 3 por tick, então o que mudava era quantos
  ticks passavam entre montar o estado e o print, não a regra. O medidor de lava fecha (2026-11-07): 0 pixel e 0 níveis contra o ciclo do mesmo nível (`CICLO_DA_LAVA`), em
  guiScale 2 e 3. Esses quatro casos agora têm a energia cheia (com 120 a lava baixava e o nível do print deixava de ser o do ciclo); o gerador queimando segue em
  `generator_queima` (0 pixel no medidor) e no rastro `heat_generator_lava_run`.
- Lava dentro do bloco desenhada pela block entity; entidade ali dentro não conta como dentro de lava.
- Feito (2026-11-07): as abas de energia, de calor (com a unidade de temperatura e as conversões) e de upgrades, o nome com estilo. O som do gerador não está portado (abaixo, em "Falta"). A tela tem o
  medidor de lava e a barra de energia do original; o painel de texto que havia ali não existe no Java. **Falta (Fase 5, movido em 2026-11-08)**: o slot de energia funcional (o item não converte), a
  transferência de calor pela base (`lastTransferLoss` vale 0), o som (`GeneratorsSounds.HEAT_GENERATOR` não existe no port) e as partículas (`SMOKE` e `FLAME`, abaixo). Os slots de
  armadura e de mão estão portados (abaixo).
- Tanque de lava não recebe de cano. **Fase 3** (movido em 2026-11-08).
- **Slots de armadura e de mão do Heat Generator (feito, 2026-11-07; pixel contra o original e som medidos no mesmo dia).** **Java** (lido e gravado em
  `heat_generator_armor_{layout,may_place,clicks,binding,quick_move}.jsonl`): `GeneratorsContainerTypes.java:29` registra o gerador com `.armorSideBar(-20, 11, 0)` (o mesmo nos geradores bio,
  a gás, solar e eólico), que chama `MekanismContainer.addArmorSlots(inv, -20, 11, 0)` (`MekanismContainer.java:231`): quatro `ArmorSlot` em `(-20, 11 + 18 i)` com
  `EquipmentSlot.VALUES.get(2 + 4 - i - 1)` (cabeça em cima) sobre os índices 36 a 39 do inventário do jogador, e um `OffhandSlot` em `(-20, 83)` (índice `SLOT_OFFHAND`): os slots 40 a 44 do
  menu (0 e 1 virtuais, 2 e 3 do gerador, 4 a 30 o principal, 31 a 39 a barra de atalhos). `ArmorSlot`: `mayPlace` é `stack.canEquip(slotType, player)`, `getMaxStackSize` 1, `mayPickup` recusa o que tem
  `PREVENT_ARMOR_CHANGE` fora do criativo, `setByPlayer` chama `player.onEquipItem`, e o fundo vazio é `InventoryMenu.EMPTY_ARMOR_SLOT_{BOOTS,LEGGINGS,CHESTPLATE,HELMET}` (o da mão,
  `EMPTY_ARMOR_SLOT_SHIELD`). `OffhandSlot` aceita qualquer item (pilha do contêiner, 99, e a do item). `GuiHeatGenerator.java:35` põe o suporte
  `GuiSideHolder.create(this, -26, 6, 98, true, true, TAB_ARMOR_SLOTS)` (`holder_left`, tingido de branco: o pixel 21 de `colormap/gui_objects.png`), e `dynamicSlots = true` dá um `GuiSlot` a cada slot do menu.
  `quickMoveStack` (`MekanismContainer.java:278-330`): do slot do tile, tenta armadura, barra de atalhos e inventário principal (armadura só nos que já têm item, depois nos
  vazios); do inventário ou da barra, tenta o tile (empilhar, depois vazio), e então a armadura (só vazios) e o outro grupo; de um slot de armadura ou de mão, o tile e depois
  a barra e o inventário (`insertItem(playerSlots)`, sem a armadura). `quickMoveStack` move por `insert` e `remove`: sem `setByPlayer`, então **sem som e sem evento de jogo**, nem dois ticks depois.
  O clique com o cursor (colocar, trocar, tecla numérica, tecla F) passa por `setByPlayer`: toca `item.armor.equip_*` pelo `Equippable` e dispara `minecraft:equip`; tirar dispara só `minecraft:unequip`, sem som;
  capacete na mão não toca nada, escudo na mão toca `equip_generic`, e tirar da mão não dispara evento.
  **Como ficou** (opção (a): os cinco são slots reais do jogador no menu do servidor, com os ids de cliente 500 a 504 do jogo, e o motor verificado do Horizon, `Role::Armor`, faz
  o clique): `machinery_basis_core::armor_slots` (layout, `may_place`, `may_pickup`, `insert`) e `armor_bar` (o registro por tipo de menu, o mapa `slots + 36 + k` para 500 + k, os
  retângulos de elementos de tela), o Generators só declara `armorSideBar(-20, 11, 0)` e as abas de calor e de energia, desenha o suporte e os quadros, e liga o shift-clique
  (`GeneratorMenu` com a armadura e a mão, `armor_slots::insert` com o `canEquip` do jogo: `equip_slot` e `equip_allowed`; sem efeito de equipar). Nove injeções cruas na `machinery_basis`
  (`docs/injecoes-horizon.md`, "A barra de armadura e mão"), com o hook que as substituiria (H31 e H32). Testes puros: `armor_bar` (9, no núcleo), `armor_slots_menu.rs` (o shift-clique do núcleo) e
  `mekanismgenerators/tests/armor_bar.rs` (11: a declaração, os índices 40 a 44, as abas, o suporte contra o `.mcmeta`, e o shift-clique com os índices do glue). Em jogo (desktop, container `horizon`,
  `dev/referencia/horizon2/h2_armor.py`, 15 casos, 0 diferenças): os cinco slots em (-20, 11 + 18 i) e (-20, 83); capacete na cabeça aceita e nas botas e no peitoral recusa; terra recusada na armadura e aceita na mão
  (pilha toda); pilha de 2 capacetes entra 1 e o cursor fica com 1; trocar, tecla numérica sobre a cabeça e F (sobre a mão nada muda, sobre a armadura leva a peça para a mão); shift-clique do inventário e da barra
  para o slot certo, a pilha de 2 capacetes (1 na armadura, 1 no principal, a barra esvazia) e da armadura de volta para a barra de atalhos; o servidor guarda o que a tela mostra (`data get entity @s equipment`); o
  evento de jogo medido com um sensor de sculk (clique que equipa e que tira emitem, shift-clique e terra na mão não); a peça com maldição do vínculo em sobrevivência não sai por clique nem por shift-clique;
  o equipamento aparece no jogador e no HUD de armadura (de 1 para 4 ícones com o peitoral, e de volta a 1 no shift-clique de volta). **Pixel contra o original (2026-11-07):** 0 pixel de diferença nos fundos vazios, nos quadros dos
slots e no suporte, nos 294 estados do Heat Generator em guiScale 2 e 3 (ver "Pixel da barra de armadura contra o original", acima). **Som (2026-11-07, `horizon2/h2_armor_sound.py`, 18 medidas, 0 diferenças):** o jogo sem tela
não expõe o som, mas com `showSubtitles` ligado o `horizon-ctl screenshot` mostra a legenda de cada som audível (`sounds.json` do 26.3 liga `item.armor.equip_leather` a "Leather armor rustles", `equip_iron` a "Iron armor
clanks", `equip_elytra` a "Elytra rustle" e `equip_generic` a "Gear equips"), e o servidor parado (`tick freeze`) deixa o resto da tela igual de um print para o outro. O clique com o cursor equipando capacete de couro,
de ferro, élitro no peitoral e escudo na mão mostra a legenda certa de cada um; tirar a peça, capacete e terra na mão, e os três shift-cliques (do inventário para a armadura, da armadura para a barra, peitoral de
couro para o slot dele) não mostram nenhuma (0 pixel mudado na região das legendas); um clique que põe o peitoral de couro de volta, de controle, mostra "Leather armor rustles". É o que o rastro do original grava
(`heat_generator_armor_clicks.jsonl`: `sounds` com `equip_leather`, `equip_iron`, `equip_elytra` e `equip_generic` só nesses cliques, vazio em tirar, na mão com capacete e em todo `quick_move`). Sem injeção nova.
**Falta:** a sincronização das cinco peças quando elas mudam por fora com o menu aberto (um comando, outro jogador) segue a do menu do inventário, um tick depois, como antes; as barras dos outros geradores
e das máquinas que o Mekanism ainda vai portar entram só declarando (`armorSideBar(x, y, offhand)` por tipo de menu).
- Clique sobre os elementos laterais da tela (`GuiMekanism.hasClickedOutside`, `GuiMekanism.java:397-399`; feito para o Heat Generator e para a aba de upgrades das cinco telas, e para as abas de energia, avisos e xp das quatro máquinas, 2026-11-07). O original só chama
  de "fora" o que está fora da imagem e não está sobre um elemento ativo (`findElementOver`), e o NeoForge nunca chama de "fora" um ponto sobre um slot (`AbstractContainerScreen.mouseClicked`:
  `if (slot != null) clickedOutside = false`). O Horizon olhava só o retângulo de 176 por 166 e dava a vez a "fora" sobre um slot: com um item no cursor, o clique numa aba lateral soltava o item. Em
  jogo, no Heat Generator com 5 de terra no cursor: antes (build `native-f319701-e353b34123f7`) o clique na aba de energia soltou a pilha no chão; depois (`h2_armor.py tabs`) o cursor continua nas abas de energia,
  de calor e de upgrades, e nos slots da cabeça (peça errada), e o clique na margem do suporte (sem slot nem elemento: `GuiSideHolder` e `GuiSlot` têm `active = false`) solta, como no original. **Feito para as quatro máquinas (2026-11-07):** os elementos de tela que ficam fora da imagem e recebem o mouse são agora um registro próprio da lib,
  `machinery_basis_core::screen_elements` (o retângulo de cada elemento e, opcional, uma condição que lê o espelho do menu da tela: ativo e visível agora), separado da barra de armadura (`armor_bar` só guarda os slots,
  e `over_slot` é o "slot sob o mouse" do NeoForge). O Mekanism declara por máquina (`mekanism_rules::screen_elements(ScreenSpec)`, só dados): a aba de energia (`GuiEnergyTab`, -26, 137, 26 por 26, sempre), a de avisos
  (`GuiWarningTab`, -26, 109, 26 por 26, só com algum aviso: `ScreenSpec::view(data).tab()`) e o botão da aba de xp (`GuiInsetElement.isMouseOver` só olha o botão de 18 por 18 em (179, 94), não o suporte de 26 por 26; só com
  o upgrade instalado: o dado `hasXpContainment` do menu). O Heat Generator declara a de calor e a de energia pelo mesmo registro, sem mudar de comportamento. A injeção `bar_clicked_outside` é a mesma (sem injeção nova): o
  helper dela agora lê também o espelho do menu da tela aberta (`mb_open_mirror`, no módulo `windows`, onde `mod_screen` é visível). **Contra o original:** rastro `fora.jsonl` (o cliente oficial, `CenaTelas` com `fora=1`:
  `hasClickedOutside` em 272 por 182 pontos inteiros ao redor da tela, 18 estados em 3 modos: as quatro máquinas sem e com o upgrade de xp e sem e com aviso, e o Heat Generator parado e com lava), repetido ponto a ponto por
  `mekanism/tests/screen_elements.rs` e `mekanismgenerators/tests/screen_elements.rs` (0 diferenças; o teste falha de verdade: trocar a condição da aba de xp por "sempre" ou deslocar a aba de avisos 1 pixel quebra, medido).
  Em jogo (desktop, container `horizon`, `horizon2/h2_abas.py`, 16 estados, terra no cursor): o clique esquerdo e o direito (que o original também não solta: o elemento está sob o mouse, só `isValidClickButton` falha)
  nas abas de energia, de avisos e de xp mantêm a pilha, e o clique onde a aba não existe naquele estado (avisos sem aviso, xp sem o upgrade), nas bordas do suporte da aba de xp (3 e 5 pixels) e na margem solta, como o rastro.
  Antes (build `native-f319701-4815b98243f2`, Enrichment Chamber e Infuser): o clique nas abas de energia e de avisos soltava a pilha, e o direito na de xp também; o esquerdo na de xp já mantinha, porque `screen_click`
  a trata antes de `clicked_outside`. Prints de antes e depois em `~/.cache/rust-mekanism/wa/abas/`. **Falta (Fase 2 para a lógica, Fase 5 para o desenho; movido em 2026-11-08):** os elementos que o original tem fora da imagem e o port ainda não desenha (o rastro modo `completo` os mostra,
  todos com botão de 18 por 18): a configuração de itens (-21, 10) e a de transporte (-21, 38) à esquerda, e a segurança (179, 38), o livro de receitas (179, 66) e a redstone (179, 141) à direita. Com um item no cursor, o
  clique neles ainda solta (no original não solta); cada um entra no registro quando a aba for portada. O Heat Generator tem a mesma coisa nas abas de segurança e de redstone, e a de avisos à direita (`GuiHeatGenerator.addWarningTab`,
  invisível nos estados do rastro: o gerador não tem aviso a mostrar). **Fechado em 2026-11-08, sem divergência:** o Metallurgic Infuser com item de entrada, sem químico e sem energia não mostra a aba de avisos, e o original faz o mesmo. Medido no original (`CenaTelas`, 1920 por 1080, `guiScale` 2 e 3) e no
  port (build `native-f319701-3b92fc0bb580`) em cinco estados, todos sem a aba nos dois jogos e iguais pixel a pixel fora dos resíduos conhecidos: `infuser_entrada_sem_nada` (clay_ball sem químico nem energia, o caso observado), `_cobre_sem_nada` (copper_ingot),
  `_sem_receita` e `_sem_receita_com_energia` (stick, que nenhuma receita usa) e `infuser_aviso_infusao` (clay_ball com energia). A causa no Java: com a entrada e o tanque vazio o `TwoInputRecipeCache` não acha receita, então não há `CachedRecipe`
  (`RecipeCacheLookupMonitor.updateAndProcess`, `:106-120`, devolve falso sem processar) e nenhum erro é marcado: `trackedErrors` fica todo falso (`TileEntityRecipeMachine.clearRecipeErrors`, `:174-177`), e nenhum `getWarningCheck` da tela dispara
  (`GuiMetallurgicInfuser.java:31-41`). Com 500 de bio e sem energia a receita existe, `NOT_ENOUGH_ENERGY` acende e a aba aparece (`infuser_aviso_energia`). É o que o rastro `warnings_input_infuser` já dizia (a entrada que não basta nunca dá `NOT_ENOUGH_INPUT`).

## Comportamento do original que parece bug e fica

1:1 inclui os bugs: o que o original faz e parece erro é portado igual, com teste de referência contra o Java, e registrado aqui com a linha do Java. Nesta frente (telas, upgrades,
abas, luz e o comparador) o que pareceu estranho no original foi conferido e o port o reproduz, nada foi "corrigido":

- O blur do menu (`Screen.extractBlurredBackground`) existe mas não roda numa tela de contêiner: `AbstractContainerScreen.isInGameUi()` é `true` (jar de fontes do 26.3,
  `AbstractContainerScreen.java:1225`), então o fundo é só o degradê `0xC0101010` a `0xD0101010` (`Screen.java:435-437`). O port faz o mesmo (`container_dim`), sem blur.
- O tooltip de uma linha da lista de upgrades (`GuiUpgradeScrollList.java:80`) guarda o `Tooltip` por upgrade em `tooltips.computeIfAbsent`, quebrado em 170 pixels uma vez; o port
  quebra no mesmo ponto, e a caixa tem as duas linhas a 10 pixels, sem a folga de título.
- A lista de upgrades com mais de um tipo instalado é ordenada por hash de identidade no original (a ordem muda de uma execução para outra); o comparador só compara a lista
  com um tipo (casos `_um_`). A sincronização do mapa (`SyncableStreamCodec.upgradeMap`) o troca por um `Object2IntOpenHashMap` novo no cliente, de ordem de hash, e o `CenaTelas`
  refaz a ordem (`orderClientUpgrades`) antes de cada clique e de cada quadro só para a imagem ser repetível.

- O índice de receitas de componentes exatos (`ComponentSensitiveInputCache.addComponentInputCache`, `ComponentSensitiveInputCache.java:97-120`) guarda só a primeira receita de cada item: a segunda copia o `Collections.singletonMap` para um
  `Object2ObjectOpenCustomHashMap` que fica numa variável local (`holderMatch = newMap`) e nunca volta a `componentInputCache`, então uma segunda receita `strict` do mesmo item, com outra chave ou a mesma, se perde. Rastro `recipe_components`, caso `strict_written`
  (uma `strict` de `repair_cost` 5 e depois uma `strict` vazia: a pilha sem patch não acha a segunda). O port faz o mesmo (`ComponentSensitiveInputCache::add_component_input_cache`).
- A receita `strict` escrita à mão com só alguns componentes nunca passa em `test` (a pilha tem os componentes padrão do item, e `strict` pede que todos estejam no patch) mas o índice a acha pela chave: `type_based` devolve a receita e `find_first` não (`strict_written`, a pilha com `repair_cost` 5).
  Só o `strict` feito de uma pilha (`DataComponentIngredient.of(true, stack)`, que lista os componentes padrão) casa nos dois. O port repete as duas respostas.
- O `ItemResource` sem patch devolve o mapa do protótipo, que não é igual a nenhuma chave do índice exato, e o `ItemStack` sem patch devolve o `PatchedDataComponentMap` vazio, que é: o mesmo item sem componentes é achado por `findFirstRecipe` (pilha) e não por `containsInput` (recurso) (`strict_defaults`).
  Já estava portado (`ComponentsKey::Unmatchable`); o rastro novo repete.
- `GuiInsetElement.isMouseOver` (`GuiInsetElement.java:43-48`) só considera o botão de dentro (18 por 18), não o suporte de 26 por 26 (`GuiSideHolder`, `active = false`, `GuiSideHolder.java:36`). Com um item no cursor, o clique na borda do
  suporte de uma aba inset (3 pixels à esquerda e 5 à direita do botão de xp, e a faixa de 4 em cima e embaixo) é "fora" e solta o item, mesmo parecendo parte da aba. As abas de textura (`GuiEnergyTab`, `GuiWarningTab`,
  `GuiHeatTab`) têm o retângulo inteiro. O port reproduz (`mekanism_rules::screen_elements` declara o botão da aba de xp, `xp_tab().tab.button()`), conferido ponto a ponto no rastro `fora.jsonl`.
- O clique direito sobre uma aba ativa não solta o item: `GuiElement` é um `AbstractWidget`, `isValidClickButton` recusa o botão, mas `hasClickedOutside` olha só `isMouseOver`. O port mantém o item e não manda o `COLLECT_XP`.
- O suporte da barra de armadura (`GuiSideHolder.java`: `active = false`) e os `GuiSlot` (`GuiSlot.java:91,101`: `active = false`) não são "elemento sob o mouse" (`GuiElement.isMouseOver` exige `active`):
  um clique com item no cursor na margem do suporte (os 5 pixels à esquerda e os 3 à direita dos quadros, o topo e o fim), fora de qualquer slot e de qualquer aba, é "fora" e solta o item.
  O port faz o mesmo (`armor_bar::over_slot` e `screen_elements::over_element`, `mekanismgenerators/tests/armor_bar.rs`, e em jogo `h2_armor.py tabs`). Um slot da barra sob o mouse nunca é "fora" (NeoForge, `if (slot != null) clickedOutside = false`).
- O shift-clique de uma peça com maldição do vínculo em sobrevivência não chega ao `quickMoveStack`: `AbstractContainerMenu.doClick` volta antes por `Slot.mayPickup`. O port confere o `mayPickup` antes de chamar o mod
  (`bar-quick-move`) e o clique, que passa pelo motor, recusa do mesmo jeito; em criativo sai.
- Chamado direto, sem passar pelo clique, o `quickMoveStack` não confere `mayPickup`: uma peça com maldição do vínculo em sobrevivência sai da armadura para a barra
  (`MekanismContainer.java:276-330`; `transferSuccess`, `:382-387`, chama `currentSlot.remove` sem `mayPickup`, e o próprio Java anota o risco num TODO). Só o clique do jogo a segura, porque
  `doClick` confere antes. O port reproduz os dois caminhos (rastro `heat_generator_armor_binding.jsonl`, casos `direct`; `armor_slots_menu.rs`).
- O slot da mão secundária (`OffhandSlot.java:9-24`) não sobrescreve `mayPlace` nem `getMaxStackSize`: aceita qualquer item (terra, capacete, balde de lava), com o teto de 99 do `Slot`, limitado
  pelo item (64 para terra, 1 para capacete). O port faz o mesmo (`armor_slots::may_place` e `max_stack_size`, rastro `heat_generator_armor_layout.jsonl`).
- O shift-clique de um combustível vai para o slot de combustível do Heat Generator mesmo com o tanque de lava cheio, e mesmo saindo da mão secundária: o `canInsert` de
  `FluidFuelInventorySlot.forFuel` (`FluidFuelInventorySlot.java:56-60`) aceita todo item com `COOKING_FUEL` (`TileEntityHeatGenerator.java:110-113`) sem olhar o tanque. Vale para balde de lava,
  carvão e graveto (rastro `heat_generator_armor_quick_move.jsonl`, casos `_lava_full` e `direct_offhand_*`/`shift_offhand_*`).
- Tirar uma peça da armadura emite o evento de jogo `minecraft:unequip`, e tirar ou pôr algo na mão secundária não emite nenhum (nem `equip` com o escudo, que ainda toca `equip_generic`):
  é o `onEquipItem` do vanilla, que só emite evento para os slots de armadura do jogador. O port segue o motor (rastro `heat_generator_armor_clicks.jsonl`, casos `take_*` e os da mão secundária).

- O Metallurgic Infuser com uma receita de `per_tick_usage` (`ItemStackConstantChemicalToObjectCachedRecipe`) gasta do tanque a quantidade da receita vezes o multiplicador a cada tick
  (`InputResourceHandler.java:66`, `getAmount(recipeInput) * operations`), mas o `usedSoFar` conta só o multiplicador (`ItemStackConstantChemicalToObjectCachedRecipe.java:129-135`):
  com 2 de redstone por tick o tanque cai 2 por tick e o `used_so_far` sobe 1. O tick que termina a operação gasta de novo (`finishProcessing`, `:151`), então a operação de 200 ticks
  custa 400 e não 398. O port reproduz (rastro `infuser_const_run.jsonl`).
- O multiplicador do Infuser é `constantUse(() -> ticksRequired, this::getTicksRequired)` (`TileEntityMetallurgicInfuser.java:90`): o total de uso é o `ticksRequired` de agora, não o da receita
  sem upgrades, então ele nunca alcança nada depois de um upgrade de velocidade (o Compressor, com o total fixo, alcança). Fica 1 por tick em todas as mudanças de velocidade
  (`infuser_const_speed.jsonl`).
- Salvar com um progresso que já não cabe nos upgrades que voltam: `loadSavedOperatingTicks` recusa o progresso (`CachedRecipe.java:241`, `operatingTicks < requiredTicks`), e o
  `loadSavedUsageSoFar` aceita o gasto (`ItemStackConstantChemicalToObjectCachedRecipe.java:59-63`, só confere `> 0`). O gasto fica maior que o progresso, o multiplicador dá 0
  (`:164`) e a operação inteira roda sem gastar químico (`infuser_const_reload_speed.jsonl`, t=106 a t=124: o tanque fica em 800 e o `used_so_far` em 100). Com o progresso e sem o
  `used_so_far` no NBT (`getIntOr(USED_SO_FAR, 0)`, `TileEntityMetallurgicInfuser.java:228`) acontece o contrário: o multiplicador vira 2 e o gasto alcança o total
  (`infuser_const_reload_without_usage.jsonl`). O port reproduz os dois.

- Uma máquina que perde os upgrades de velocidade com o erro de energia aceso continua com ele aceso (pausada, `CachedRecipe.java:250`, `pausedForErrors`): só uma mudança de energia ou de entrada a acorda, e a barra de energia fica
  com o nível da energia sobre a capacidade nova (`getCapacityAsLong` é o maior entre o máximo e o guardado). Rastro `warnings_overlay`, t=25 a t=45. O port reproduz.
- O Metallurgic Infuser dá ao item o erro `NOT_ENOUGH_SECONDARY_INPUT` e ao químico o `NOT_ENOUGH_INPUT` (`TileEntityMetallurgicInfuser.java:121-122`), e a tela os usa pelo nome: a barra de químico avisa pelo erro do item e o slot de
  entrada pelo do químico (`GuiMetallurgicInfuser.java:36`, `TileEntityMetallurgicInfuser.java:151`). Como nenhum dos dois acende, não se vê. O port tem o par (`CHEMICAL_MACHINE_INPUT_ERRORS`, `INFUSER_WARNINGS`).

- O `redstone` de uma máquina é o sinal lido no último aviso, e só três coisas avisam (`BlockTile.neighborChanged`, `BlockTile.java:115-124`; `BlockTile.onNeighborChange`, `:105-113` ->
  `TileEntityMekanism.onNeighborChange`, `:993-997`; `TileEntityMekanism.onAdded`, `:1000-1008`; os três chamam `updatePower`, `:1109-1117`, que lê `level.hasNeighborSignal` na hora). Logo, e o port
  reproduz (`neighbors_*`, repetidos tick a tick por `neighbors_trace.rs`): (a) um bloco de redstone posto sem avisar (`/setblock ... strict`, `Level.setBlock` sem a flag 1) não liga a máquina em HIGH, e
  tirado sem avisar a deixa ligada, até um aviso qualquer dos vizinhos (outro bloco posto ao lado, uma alavanca vizinha): `chamber_unnotified`, t=10 a 29 e t=45 a 59; (b) uma máquina posta por
  `/setblock`, pelo gabarito de teste ou por uma estrutura nasce com `redstone = false` mesmo encostada num sinal ligado, porque `onAdded` só roda no `setPlacedBy` de quem põe com um item (jogador,
  dispenser): `chamber_placed`, linhas 0 e 1 contra a 2; (c) o tile lido do disco volta com o `redstone` salvo, mesmo que o sinal tenha mudado com o chunk descarregado: `chamber_reload`, t=35 e t=65; (d) o bloco que muda e
  avisa não avisa a si mesmo; (e) um aviso e, no mesmo tick, uma mudança sem aviso deixam o `redstone` do aviso: `chamber_unnotified`, t=100 a 114. O Heat Generator segue o mesmo caminho, e o `canFunction()` dele
  (`TileEntityHeatGenerator.java:140`) usa esse `redstone`.
- O `onNeighborChange` do NeoForge chega a toda máquina vizinha de qualquer block entity que chame `setChanged` (um baú, `BlockEntity.java:216-221`) e do comparador de uma máquina do Mekanism, e
  conserta um `redstone` velho: `chamber_output_signal`, t=20 e t=56. O `TileEntityMekanism.setChanged` ela mesma não avisa ninguém (`TileEntityUpdateable.java:89-100`).

- **O ar que um tile lê não acompanha o bioma (Heat Generator, 2026-11-08).** `CachedAmbientTemperature.getTemperature` (`CachedAmbientTemperature.java:30-45`) guarda cada valor na primeira leitura e não
  tem invalidação: um `/fillbiome` embaixo de um gerador que já ticou não muda o ar que ele lê, só o tile recriado (chunk que descarrega e carrega) lê de novo. O rastro `heat_ambient_cache` mostra
  o ar de 322,2 K ficando 60 ticks depois de o bioma virar neve (272,2 K direto do mundo). O port guarda o ar na memória do tile (`heat::AmbientCache`), repetido por `ambient_trace.rs`.
- **O cache de temperatura do `Biome` ignora o nível do mar (2026-11-08).** `Biome.getTemperature` (`Biome.java:130-143`) usa `pos.asLong()` como chave e devolve o valor guardado sem olhar o
  `seaLevel`: a mesma posição lida com outro nível do mar devolve o primeiro valor (rastro `heat_ambient_table`, linhas `aliased`: 297,766 K para os níveis 63, -63, 32 e 63 de novo, onde o cálculo
  dá 297,766, 293,829 e 296,797). Não está portado: decisão 10.

- O nível do comparador de uma máquina é a média da fração de **todos** os slots do inventário com capacidade, os vazios inclusive (`ResourceContainerType.getRedstoneSignalFromContainers`, que segue
  `AbstractContainerMenu.getRedstoneSignalFromContainer` e não ignora vazio): a entrada cheia sozinha dá 5 (4 no Infuser, que tem quatro slots) e só com a saída e o slot de energia cheios chega a 15. O Heat Generator lê só o
  tanque de lava (`getRedstoneSignalFromContainer`). Rastros `comparator_levels_*`; o port reproduz.
- Um tile que nasce (posto, ou lido do NBT) não avisa os vizinhos no primeiro tick: `currentRedstoneLevel` vem do NBT (ou 0) e `updateComparators` nasce falso (`TileEntityMekanism.java:645-651`), então o `redstone` velho de uma
  máquina encostada só é corrigido por uma mudança real de nível, e não por uma máquina que apareceu. Rastro `comparator_first_tick`, repetido por `comparator_trace.rs`; o port avisava no primeiro tick (`reported_level = None`) até 2026-11-08.

- **O som de uma máquina com `enableMachineSounds` desligado não para.** `TileEntityMekanism.updateSound` (`:1426`) volta na primeira linha quando a configuração está desligada, antes do ramo que
  chama `SoundHandler.stopTileSound` (`:1454-1458`): ligar a configuração com a máquina acesa não muda nada, mas desligá-la com o som tocando o deixa tocar até a instância sair sozinha (o `TileTickableSound.tick`
  para o som se o jogador sai do alcance, `SoundHandler.java:330-333`). O som não para quando a máquina apaga nem quando o bloco sai (`setRemoved` chama `updateSound`, que volta do mesmo jeito), e `soundMap`
  o guarda por posição: um tile novo no mesmo bloco acha a instância viva em `startTileSound` (`:195-201`) e a usa, e o primeiro `lit` apagado dele chama `stopTileSound` e a para. Sem tile, `getTileVolumeFactor`
  (`:358-368`) vale 1, e um som calado pelo upgrade volta ao volume cheio no próximo ciclo. Rastro: cenários `maquinas_off_depois` e `maquinas_off_ligando`, repetidos tick a tick por `sound_trace.rs`; o port guarda
  o som sem dono (`ORPHANS` em `tile_sound.rs`).
- **`baseSoundVolume` só vale para um som novo.** `TileTickableSound` guarda `originalVolume = volume * baseSoundVolume` no construtor (`SoundHandler.java:308`) e `tick()` o relê dele mesmo (`:347`): mudar a configuração
  com a máquina tocando não muda o som até ele parar e começar de novo. Cenário `base_depois`.
- **O upgrade de silêncio que sai não avisa o cliente.** `TileComponentUpgrade.setUpgrades` chama `sendUpdatePacket` quando o upgrade de silêncio entra (`:149-156`), mas `removeUpgrade` (`:167-189`) não: o
  cliente segue com a contagem velha, o som que o upgrade calou continua mudo com a máquina acesa, e só o próximo pacote de dados do bloco (o `lit` que muda, por `ChunkHolder.broadcastBlockEntityIfNeeded`) traz a contagem
  verdadeira. Cenários `silencio_tocando_tirado`, `silencio_tirado` e `silencio_tirado_religa`; o port guarda a contagem do servidor à parte (`MufflingSync`, `mb_muffling_server`) e quem entra depois recebe a verdadeira. O
  Heat Generator tem o mesmo componente e a mesma regra quando o som dele for portado (Fase 5): o `sync_muffling` dele ainda manda na hora.

Quando uma frente achar outro, entra aqui, com a linha do Java e o teste que o prova.

- Salvar e sair com a janela de upgrades aberta recomeça a instalação: `upgradeTicks` e `canCheckUpgrades` são só memória (`TileComponentUpgrade.java:65-66`; `serialize` e `deserialize`, `:230-258`, não os tocam), então os 20 ticks
  de `UPGRADE_TICKS_REQUIRED` contam de novo do primeiro tick do tile reaberto, qualquer que fosse o ponto da instalação. O port faz o mesmo (`UpgradeTransient::default`; rastros `upgrade_quit_*`).
- O Q (uma unidade) e o Ctrl+Q (a pilha) largam o item de **qualquer** slot da máquina, inclusive o de saída, o de energia e o de infusão, e o clique duplo (`PICKUP_ALL`) recolhe de todos os outros slots da máquina,
  também o de saída e o de energia: `AbstractContainerMenu.doClick` (`:516` e `:537`) só confere `mayPickup` e `canTakeItemForPickAll`, que o Mekanism só nega ao slot de uma janela não selecionada (`MekanismContainer.java:179-187`),
  e nenhum olha o `mayPlace` (rastro `slot_click_*`, linhas `throw`, `throw_all` e `double`). O motor de menu do Horizon é quem faz isso no port; o Q e o Ctrl+Q foram conferidos em jogo, o clique duplo não (o `horizon-ctl` não o manda).
- O slot de entrada de upgrades aceita todo upgrade que a máquina suporta mesmo acima do máximo do tipo: 40 de energia entram no slot, 8 são instalados e 32 ficam nele (`TileComponentUpgrade.tickServer`, `:75-112`).

## Pronto significa

Uma parte só sai do inventário quando: o Java foi lido por inteiro, o teste de referência passa, o cenário foi repetido em
mundo, e o que a pessoa vê e ouve foi comparado com o original. Se a referência em NeoForge existir, com o mesmo cenário nos
dois jogos.
