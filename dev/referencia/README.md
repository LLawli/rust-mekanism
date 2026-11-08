# Mekanism de referência

O mod original, compilado do commit que o port segue, para comparar comportamento lado a lado. Roda no mesmo isolamento do
Horizon: é código de terceiros, e o `runClient` não usa conta.

```sh
./instalar <repositório do Mekanism>   # precisa do jogo-horizon já construído
mekanism-ref offline                   # compila na primeira vez e abre o jogo
JOGO_CMD='cd "$FONTE" && ./gradlew --no-daemon compileJava' mekanism-ref
JOGO_CMD='cd "$FONTE" && ./gradlew --no-daemon runGameTestServer' mekanism-ref
```

O `runGameTestServer` roda sem janela e sem ninguém: os testes do Mekanism e os de `testes/`, que gravam o estado a cada tick,
depois de `TransmitterNetworkRegistry.onTick`, em `src/runs/gameTestServer/trace/<cenário>.jsonl`. Os testes em Rust comparam
com esses rastros. Os das máquinas ficam copiados em `rastros/`, e `mods/mekanism/tests/chamber_trace.rs` os repete tick a tick.

Para comparar a imagem, `testes/CenaCabo.java` monta a cena do cabo no cliente e tira os prints pelo próprio jogo. Precisa de um
mundo em `src/runs/client/saves/GameTestWorld`, do arquivo `src/runs/client/cena-cabo`, de `fullscreen:true` e
`onboardAccessibility:false` no `options.txt`, e do `runGameTestClient` dentro de um `gamescope`. Os prints saem em
`src/runs/client/screenshots/`. Coordenada inteira no `tp` é centralizada: escreva `-2.0`.

`testes/SlotsTraceTest.java` grava os hoppers em volta das quatro máquinas (posta com o item, como um jogador, com a configuração de lados
padrão), as regras dos slots de infusão e de energia para quem está de fora, e a receita que o Crusher escolhe para `melon_slice`
(`slots_<máquina>.jsonl` e `crusher_melon.jsonl`). Copie o arquivo para `src/gameTest/java/mekanism/common/tests/network/` só na hora de
rodar o `runGameTestServer` e apague-o depois. `mods/mekanism/tests/slot_rules.rs` e `c_tags.rs` os repetem.

`SlotsTraceTest.recipeOrder` grava `recipe_order.jsonl`: uma linha por tipo (`enriching`, `crushing`, `smelting`, `metallurgic_infusing`,
`chemical_conversion`, `energy_conversion`, e o `smelting` e o `blasting` do vanilla) com os ids na ordem de `getRecipes(level)`, e todo
item que mais de uma receita do tipo casa, com a que o índice escolhe. `mods/mekanism/tests/recipe_registration_order.rs` o repete.

`MachineTraceTest` também grava a conversão de energia do slot de energia (`chamber_energy_conversion`,
`chamber_energy_conversion_running`, `infuser_energy_conversion`): o `Trace` ganhou `recordEnergySlot` (o campo `energy_slot`) e
`setMaxEnergy`, porque o contêiner das máquinas portadas nunca tem espaço para os 10.000 de um `redstone`. `chamber_trace.rs` e
`infuser_trace.rs` os repetem. Para rodar só os seus, copie `MachineTraceTest.java` e `SlotsTraceTest.java` para
`src/src/gameTest/java/mekanism/common/tests/network/` no desktop (o repositório do Mekanism de referência fica em `src/`, e o código
de teste em `src/src/gameTest`), confira antes que o `mekanism-ref` está ocioso, e devolva a árvore como estava.

`testes/IdentityTraceTest.java` grava (`identity_<máquina>.jsonl`, uma por máquina e uma para o Heat Generator) o que o `ItemResource` faz com dois
itens do mesmo id e componentes diferentes (`custom_name` e `custom_data`): o hopper de cima tick a tick (cinco fases de 40 ticks) e o
`quickMoveStack` do menu, do jogador para a entrada e da entrada para a barra. `chamberComposter` grava `composter_enrichment_chamber.jsonl`:
a Enrichment Chamber ejetando para uma composteira (`ComposterWrapper`) embaixo, em cima e ao lado, em oito fases de até 60 ticks (a
duração de cada uma está em `composter_trace.rs`). Copie o arquivo para `src/src/gameTest/java/mekanism/common/tests/network/` só na hora de
rodar o `runGameTestServer` e apague-o depois. `mods/mekanism/tests/identity_trace.rs` e `composter_trace.rs` os repetem. Em jogo, no
`horizon2`, `horizon2/h2_identity.py`, `h2_heat.py` (hopper e shift-clique do Heat Generator) e `h2_composter.py`, mais `h2_smoke.py` (as quatro máquinas processando com um Heat Generator encostado, e o som) (rodam no notebook e falam com o container pelo `ctl.sh`, que usa o `horizon-ctl` do
`horizon2`) repetem as mesmas fases pelo `horizon-ctl` e comparam com os rastros; o da composteira usa `tick freeze` e `tick step 1` e confere os
360 ticks.

`testes/ArrivalTraceTest.java` grava `infuser_arrival.jsonl`: o Metallurgic Infuser com o tanque vazio, recebendo o item de entrada de uma
receita e o item que vira o químico dela, em três ordens (a entrada primeiro, a conversão primeiro, os dois no mesmo tick), para 13
pares (bio com terra, areia e argila; fungi com terra e nylium; redstone, diamond, gold e refined_obsidian, cada um com o pó e o
`enriched`/bloco). Cada item chega como o jogador o põe (`insert` com `AutomationType.MANUAL`), a marca guarda quanto o slot aceitou, e a
linha é a de `infuser_run` mais o id do químico do tanque. `mods/mekanism/tests/infuser_arrival_trace.rs` o repete com as receitas e as
conversões lidas dos arquivos do port (`common::infuser_from_content`); `horizon2/h2_infuser_arrival.py` repete as fases no jogo, por
shift-clique, com `tick freeze` e `tick step 1`. Carbon e tin não estão: o port não tem nenhuma receita de infusão que os use (o original tem 31
receitas, o port 19), então o tanque os recusa. Não monte uma chegada com `container X Y Z item n` (põe no primeiro slot livre, que no
Infuser é o de infusão) nem com `data merge ... Items` fora da ordem dos slots (medido: `[terra no slot 1, bio_fuel no slot 0]` deixou só a terra no tile; o Horizon
só decodifica a lista que reescreve igual, `decode_mod_block_entity`).

`testes/ConstantUsageTraceTest.java` grava (`infuser_const_{run,dry,speed,placed_speed,reload,reload_speed,reload_without_usage}.jsonl`) o Metallurgic Infuser com uma receita de
`per_tick_usage`, que nenhuma receita do original usa nele: a receita está em `testes/dados/data/mekanismtests/recipe/metallurgic_infusing/` (gravel e 2 de redstone por tick) e vai, com a
pasta `data/`, para `src/src/gameTest/resources/` só na hora de rodar o `runGameTestServer` (o mod de teste `mekanismtests` a serve como data pack); copie também o `.java` para
`src/src/gameTest/java/mekanism/common/tests/network/` e apague os dois depois. A máquina é posta por um jogador (`useOn`, com os upgrades do item) com a energia de um cabo, e as marcas
mexem no item, no tanque, nos upgrades e no tile salvo e relido (`BlockEntity.loadStatic`, com `reload-without-usage` tirando `used_so_far` do NBT). `mods/mekanism/tests/infuser_constant_trace.rs`
o repete com a receita lida do mesmo arquivo. Em jogo, `horizon2/h2_infuser_const.py run|dry` repete `infuser_const_run` e `infuser_const_dry` com `tick freeze` e `tick step` (`H_NAME=horizon`), e
`horizon2/h2_infuser_const_video.py antes|depois|montar` faz o ciclo completo com salvar e reabrir no meio, em prints e um vídeo com legenda.

`testes/HeatArmorTraceTest.java` grava (`heat_generator_armor_{layout,may_place,clicks,binding,quick_move}.jsonl`) os cinco slots que o
`.armorSideBar(-20, 11, 0)` acrescenta ao menu do Heat Generator (quatro `ArmorSlot` e um `OffhandSlot`): o menu é aberto pela fábrica registrada
(`MekanismContainerType.create`, 45 slots; os outros testes de slot usam `new MekanismTileContainer<>` e saem com 40, sem armadura). O layout, o `mayPlace`
de cada peça em cada slot, os cliques com o cursor (esquerdo, direito, tecla numérica, arrasto, clique duplo), o `PREVENT_ARMOR_CHANGE` em sobrevivência e
no criativo (dois jogadores falsos), e o `quickMoveStack` chamado direto e pelo shift-clique (`clicked(QUICK_MOVE)`), cada linha com o estado de todos os
slots, o cursor, o equipamento do jogador, o som e os eventos de jogo. O `quickMove` roda em lote próprio (`trace_armor_ticks`) porque o som dos ticks seguintes
não tem dono no evento. Copie o arquivo para `src/src/gameTest/java/mekanism/common/tests/network/` só na hora de rodar o `runGameTestServer` e apague-o depois.

`horizon2/h2_armor.py` (também com `H_NAME=horizon`) repete no jogo os casos do rastro da barra de armadura; `horizon2/h2_armor_sound.py` mede o som de equipar pelas legendas do jogo
(`showSubtitles`, o servidor parado e o `screenshot` de antes e de depois de cada clique: legenda é som, e o texto diz qual `equip_*`), e `horizon2/h2_armor_video.py capturar` e `montar` fazem o vídeo do ciclo completo
(um print por passo, com a legenda do jogo ligada, e o `ffmpeg` junta com a legenda de cada passo). Os três supõem a janela de 1920 por 1080 em guiScale 3 (o canto da tela em (232, 97)).

`testes/WarningsTraceTest.java` grava os avisos de entrada, o overlay da barra de energia e o PULSE (`warnings_input_chamber`, `warnings_input_infuser`, `warnings_overlay`,
`warnings_sweep_<máquina>`, `redstone_pulse`). A máquina é posta por um jogador (`useOn`), e as linhas dos três primeiros levam o que a tela mostra (`warnings`, `warnings_shown`, o
recheck e `bar_overlay`: o aviso de energia aceso e o nível da barra, `GuiBar.drawBackground`, >= 0,98). O que mostrou: a entrada que não basta (menos itens que a receita pede, menos
químico que ela pede) nunca dá `NOT_ENOUGH_INPUT` nem `NOT_ENOUGH_SECONDARY_INPUT`, porque `getRecipeInput` devolve vazio e o `CachedRecipe` marca `mismatchedRecipe`, que limpa os erros. A varredura
(`warnings_sweep_*`) passa por todas as receitas de `ENRICHING`, `CRUSHING`, `SMELTING` e `METALLURGIC_INFUSING` do jogo, a cada uma com a entrada (e o tanque, no Infuser) abaixo, na quantidade e acima do que
ela pede, e com velocidade 8, em ciclos de 5 ticks, e grava os erros que apareceram em cada caso: nenhum é de entrada. `redstone_pulse` põe PULSE por `setControlType`, pelo NBT e pelo componente do item nas
quatro máquinas e no Heat Generator. `mods/mekanism/tests/warnings_trace.rs` e `mods/mekanismgenerators/tests/redstone_pulse.rs` os repetem. Copie o arquivo para
`src/src/gameTest/java/mekanism/common/tests/network/` só na hora de rodar o `runGameTestServer` e apague-o depois. A cena de telas ganhou `energia_depois=N` (a energia chega depois dos passos, fora do
tick da máquina) e o roteiro `casos_avisos` de `cena_telas.py`; `so_oficial=1` marca o que só o original mostra (hoje nenhum caso: o overlay do port saiu do `so_oficial` em 2026-11-08, com os data slots no `ServerTickEnd`). `oficial_fora.sh` aceita `LARGURA` e `ALTURA` (os prints do lote são de 1920 por 1080).
Em jogo, `horizon2/h2_avisos.py capturar` e `montar` (com `H_NAME=horizon`) fazem o ciclo (entrada curta sem aviso, a entrada chega, o aviso de energia acende e apaga) em prints e vídeo com legenda.

`testes/NeighborTraceTest.java` grava (`neighbors_<cenário>.jsonl`) o campo `redstone` das máquinas e o que ele manda (`canFunction`, atividade, progresso, o bloco aceso), a cada tick e no meio dele,
quando o sinal dos vizinhos muda de jeitos que avisam o bloco (`BlockTile.neighborChanged` por `updateNeighborsAt`, o `onNeighborChange` do NeoForge por `updateNeighbourForOutputSignal`, `setPlacedBy` -> `onAdded`)
e de jeitos que não avisam (`setBlock` sem a flag 1, bloco posto por `setBlock` ou pelo gabarito, chunk descarregado). Cada linha leva as operações do tick (`ops`), em `pre` (`ServerTickEvent.Pre`, antes do tick da máquina) ou
`step` (o passo do gameTest), cada uma com o `redstone` de todas as máquinas logo depois dela. Linha 0: as ordens do jogo (`NeighborUpdater.UPDATE_ORDER`, `Level.NEIGHBOR_UPDATE_LIST`), lidas por reflexão.
Cenários: `chamber_notified`, `chamber_unnotified`, `chamber_mid_tick`, `chamber_placed`, `chamber_reload`, `chamber_output_signal`, `heat_*` (os mesmos cinco) e `crusher_core`, `smelter_core`, `infuser_core`;
`neighbors_block_classes` prova que as cinco máquinas herdam `neighborChanged` e `onNeighborChange` de `BlockTile`. Copie o arquivo para `src/src/gameTest/java/mekanism/common/tests/network/` só na hora de rodar o
`runGameTestServer` e apague-o depois. `mods/mekanism/tests/neighbors_trace.rs` e `mods/mekanismgenerators/tests/neighbors_trace.rs` os repetem no `SimLevel` (`sim::neighbor_replay`), e em jogo
`horizon2/h2_vizinhos.py CENARIO` (`H_NAME=horizon`) repete as operações como comandos (`setblock`, `setblock ... strict` para a mudança sem aviso, `interact` na alavanca, `place` como jogador) com `tick freeze` e
`tick step 1` e compara `redstone`, `active_state` e `progress` do NBT com o rastro.

`TrackerTraceTest` também grava, para a tela aberta num tile reaberto antes do primeiro tick dele, `{chamber,infuser,heat_generator}_trackers_reloaded` (a máquina operando, com upgrade e o modo de redstone baixo; `Track.reload`
grava o tile e o relê por `BlockEntity.loadStatic`, e a tela abre no mesmo tick: o `sent` dessa linha é o `sendAllDataToRemote` do tile que o `loadAdditional` carregou; a linha traz o `recheck_offset` que o tile novo sorteou) e
`upgradeOrder` (`upgrade_order.jsonl`: a ordem de `getInstalledTypes()` da Enrichment Chamber com os quatro upgrades, que muda a cada execução do JVM; `rastros/upgrade_order.jsonl` tem as quatro execuções da decisão 9 do plano).
`mods/mekanism/tests/tracker_trace.rs` e `mods/mekanismgenerators/tests/tracker_trace.rs` repetem os `_reloaded` (a marca `reload` refaz a máquina do que foi gravado, como o `born` do primeiro tick) e o `upgrade_order`.

`testes/ComparatorTraceTest.java` grava o comparador das máquinas. `comparator_order.jsonl` (duas Enrichment Chambers encostadas, N e S, com um comparador a leste de cada, nas duas ordens de ticker: a linha 0 põe N antes de S, a 1
põe S antes de N) e `comparator_first_tick.jsonl` (uma máquina posta sem avisar, que não avisa ninguém no primeiro tick) levam, a cada tick, o estado de S (`powered`, `can_function`, atividade, progresso, energia), o
`currentRedstoneLevel` de N e a saída dos dois comparadores (`ComparatorBlockEntity.getOutputSignal`); `comparator_levels_<máquina>.jsonl` (as cinco máquinas) leva, a cada marca, o conteúdo de cada slot (item, quantidade, capacidade),
`getRedstoneLevel()` e a saída do comparador, com a máquina vazia, com itens, com a saída cheia e com todos os slots cheios (o tanque de lava, no Heat Generator). `PlacedTraceTest` ganhou `placed_fed_<máquina>.jsonl`: a máquina posta
por um jogador ao lado de um Heat Generator cheio (240) que a encara, com cinco pares de upgrade no item e em dois momentos do tick (`pre`: antes dos block entities, como uma ação do jogador; `step`: depois deles), com a capacidade e a energia
no tick da colocação e nos sete seguintes. Copie os arquivos para `src/src/gameTest/java/mekanism/common/tests/network/` só na hora de rodar o `runGameTestServer` e apague-os depois (os testes do `mekanism-ref` que já estão lá, `MachineTraceTest` e
companhia, rodam todos juntos: tire-os da pasta enquanto isso). `mods/mekanism/tests/comparator_trace.rs` (as duas ordens, o primeiro tick e o valor analógico das quatro máquinas, com o bloco do comparador simulado), `mods/mekanismgenerators/tests/comparator_levels.rs`
e `mods/mekanism/tests/placed_fed.rs` os repetem, e em jogo `horizon2/h2_comparador.py order|first_tick|levels MAQUINA` e `horizon2/h2_placed_fed.py` (`H_NAME=horizon`) repetem as operações como comandos, com `tick freeze` e `tick step 1`.
`horizon2/h2_comparador_video.py capturar|montar ordem|cinco|posta` faz os prints e o vídeo com legenda.

`testes/ComponentIngredientTraceTest.java` grava `recipe_components.jsonl` (e `recipe_components_2.jsonl`, uma segunda execução do mesmo teste): o `DataComponentIngredient` do NeoForge (`neoforge:components`, com `strict` verdadeiro e falso, lido do JSON como um arquivo de receita o escreve) contra
pilhas (`ItemStackIngredient.test`, `testType`, os componentes todos da pilha e a classificação de `ItemInputCache.mapIngredient`), e seis caches de entrada (`InputRecipeCache.SingleItem`) com as perguntas de `findFirstRecipe`, `containsInput` e `findTypeBasedRecipeHolder`. Os casos
`complex_hash_order*` levam o `System.identityHashCode` da chave do id de cada receita (o `RecipeHolder.hashCode`), que muda a cada execução: por isso são duas. `machinery_basis/core/tests/recipe_components_reference.rs` os repete. `NeighborTraceTest.nestedUpdates` grava `neighbors_nested.jsonl`: as posições que o
`debugListener` do `CollectingNeighborUpdater` anuncia em cada operação ao lado de cada máquina, de um bloco de pedra e de um fio de redstone (o controle que avisa em cadeia), e a classe que declara `onPowerChange`, `updatePower` e `onNeighborChange` em cada tile; `mods/mekanism/tests/neighbors_nested.rs` o confere. Copie os dois
arquivos para `src/src/gameTest/java/mekanism/common/tests/network/` só na hora de rodar o `runGameTestServer` (tire de lá os outros testes enquanto isso) e apague-os depois.
Em jogo, `horizon2/h2_componentes.py` (`H_NAME=horizon`) repete as receitas do data pack `testes/dados/mektest` (graveto com nome, sílex com `repair_cost`, carvão `strict`, e as receitas que o mod recusa e o log explica) na Enrichment Chamber; o data pack vai em `saves/<mundo>/datapacks/mektest` e se liga com `reload`.

| Caminho | Conteúdo |
|---|---|
| `~/.local/share/jogo-isolado/mekanism-ref/src` | Código do Mekanism |
| `~/.local/share/jogo-isolado/mekanism-ref/gradle` | Cache do Gradle, com o Minecraft e o NeoForge |
| `~/.local/share/jogo-isolado/mekanism-ref/src/runs/client` | Mundos e logs do jogo |
| `~/.local/share/jogo-isolado/mekanism-ref/src/runs/gameTestServer/gameTest` | Resultado dos testes, em JUnit |

## Comparar a tela

`comparar_tela.py` compara dois prints da janela inteira, do original e do port, tirados com o mesmo `guiScale` e do mesmo tamanho:

```sh
python3 -I -B comparar_tela.py original.png port.png --tela infuser --escala 2 --saida diferenca.png
```

Recorta a tela de 176 por 166 (centrada, ou onde `--origem x,y` disser), tira da conta as colunas das abas que o port não tem (a aba de
avisos, que ele tem, fica) e conta os pixels diferentes por elemento: slots, barras, setas, medidor, título, rótulo, inventário e o
fundo. `--margem N` inclui N pixels de GUI em volta, para ver as abas, e `--mascara x,y,largura,altura` tira mais uma área. O código
de saída é 0 sem diferença, 1 com alguma e 2 para entrada inválida. Telas: `electric`, `infuser` e `generator`. O alvo é zero.
Os testes (`test_comparar_tela.py`) rodam em `scripts/ci.sh`.

O sprite de químico do Infuser é animado nos dois jogos: compare com a barra vazia, ou mascare `7,15,6,54`.

### Todos os estados

`cena_telas.py` guarda a tabela de estados (máquina, energia, itens, químico, lava, passos do servidor, cursor). `cena_telas.py oficial
ARQUIVO ESCALA...` escreve o `cena-telas` que `testes/CenaTelas.java` lê (copie o Java para `src/gameTest/java/mekanism/common/tests/network/`
e o arquivo para `runs/client/`, tire o `cena-cabo` de lá, e rode `runGameTestClient` no gamescope). A cena deixa o servidor parado
(`tick freeze`), monta o estado, avança com `tick step`, abre a tela e tira o print; as abas que o port não tem ficam invisíveis no original, e o
mundo atrás é uma caixa de concreto preto. `cena_telas.py port DESTINO ESCALA...` faz o mesmo no Horizon, pelo `horizon-ctl`, e
`comparar_lote.py PASTA_ORIGINAL PASTA_PORT SAIDA ESCALA...` compara tudo e grava `tela_<caso>.png` (original, port e diferença) e um JSON.
Os cantos transparentes do painel mostram o mundo e contam à parte (`canto_*`). Os casos `ciclo` tiram 40 prints no original e `ciclo_port` no port (o ciclo inteiro da animação), um por passo, e cada print
do original tem de ser igual a algum do port.


`CenaTelas` com `reabrir=1` grava o tile e o relê por `BlockEntity.loadStatic` (como o jogo ao descarregar e carregar o chunk) depois dos `passos`, com o servidor parado, e abre a tela antes de qualquer tick do tile novo; `cena_telas.py port` faz o mesmo no
Horizon (`reabre_no_port`: o NBT que o `data get block` mostra, `setblock` do ar e do bloco, `data merge`). Os casos `casos_reabertas` e `casos_infuser_entrada` (o Infuser com item de entrada e sem químico nem energia, que não mostra a aba de avisos
nos dois jogos) estão em `todos_os_casos`. `reabrir_real.py montar|abrir` é a versão com o mundo de verdade: monta as máquinas a 600 blocos, salva e sai (`quit`); com o jogo aberto de novo e o servidor parado, vai até cada uma e tira o print da tela
(o mundo reaberto não nasce parado, e o chunk longe só carrega com o jogador perto, já com o servidor parado).


### O clique sobre as abas (`hasClickedOutside`)

`CenaTelas` com `fora=1` chama `GuiMekanism.hasClickedOutside(x, y, left, top)` por reflexão numa grade de pixels de GUI inteiros (x de -32 a 239, y de -6 a 175, relativos ao
canto da tela) e grava `fora_rastro.jsonl`: por estado e por modo (`completo`: o que o original tem; `oculto`: sem as abas que o port não desenha; `sem_upgrades`: e sem a aba de
upgrades, que a janela de upgrades da lib responde), os elementos que o original tem fora da imagem (classe, retângulo, `visible`, `active`) e, por linha, os intervalos de x em que o clique NÃO é
"fora". `cena_telas.py oficial ARQUIVO 2 --so <casos de casos_fora()>` escreve a lista (as quatro máquinas sem e com o upgrade de xp e sem e com aviso, e o Heat Generator), e `oficial_fora.sh`
roda tudo no desktop e devolve a árvore. `rastros/fora.jsonl` é esse arquivo; `mods/mekanism/tests/screen_elements.rs` e `mods/mekanismgenerators/tests/screen_elements.rs` o repetem ponto a ponto.
Em jogo, `horizon2/h2_abas.py` (com `H_NAME=horizon`) clica com terra no cursor em cada aba, nos mesmos estados, e confere que o item fica onde o rastro diz que há elemento e solta onde não há.

### Som e efeitos

`testes/MachineTraceTest.java` grava, além dos rastros de tick, os de efeitos: `chamber_muffling` (`getVolumeFactor` e `isFullyMuffled`),
`chamber_xp_containment`, `chamber_xp_vanilla` e `chamber_xp_broken` (`recipesUsed`, `hadContainment`, o que cai de experiência), `chamber_name`
(`getName`, `getDisplayName`, `CustomName`) e `upgrade_tooltip` (as linhas do tooltip do item, em JSON de componente). Os testes
`effects_trace.rs` e `tooltip_trace.rs` do Mekanism os repetem. A experiência só entra em remessas inteiras (2 operações de 0,5): a parte
fracionária usa `Math.random`. O jogo mantém bolas de experiência perto da máquina, e o jogador falso colhe as bolas aos poucos, então
`orb_points` sobe no tick da remessa e cai depois.

`testes/CenaSom.java` roda no cliente (`runGameTestClient`) com o arquivo `cena-som` (`maquina chamber|crusher` e `muffling N`): monta a
máquina, liga com carvão, corta a energia e tira um print a cada 3 ticks, com o instante real em `cena-som-tempos.txt`. O áudio é gravado
de fora, pelo PipeWire (sink nulo e `pw-link`), porque o container só reproduz. Silencie a música da cena antes de medir níveis.

### A aba de xp e o nome com estilo

`CenaTelas` agora deixa visível a aba de xp (`GuiXpContainmentTab`) e escreve no log, para cada caso, `cena: <caso> xp_tab visible <v> x <x> y
<y> largura <w> altura <h>`: `rastros/xp_tab.jsonl` é esse log, um caso por linha, e `mods/mekanism/tests/xp_tab.rs` o repete. Os casos
`*_xp_*` de `cena_telas.py` (`casos_xp`) montam a aba com e sem o upgrade (as quatro máquinas, dois `guiScale`), o cursor sobre o botão e
na borda do suporte, a janela de upgrades aberta por cima (cursor atrás dela e fora dela), o upgrade ainda instalando e já instalado, e o
clique. O comparador tem o retângulo da aba (`aba_xp`), conta à parte os cantos transparentes do suporte (`canto_aba_*`, onde se vê o
mundo atrás da tela, H22) e aceita `ausentes=["aba_xp"]` para o estado sem a aba (o que sobra no lugar é o mundo). Os casos `*_nome_*`
põem um `CustomName` com cor, negrito e um filho de outra cor (`nome=` na tabela, JSON sem espaços) e comparam o título.

`MachineTraceTest.chamberNameStyled` grava `chamber_name_styled`: para um item com nome vermelho e negrito, e para outro com um filho azul em
itálico, o componente em JSON (`ComponentSerialization.CODEC`) que `getName`, `getDisplayName` e `getCustomName` devolvem, o `CustomName`
que o tile grava (em SNBT, que o jogo lê de volta com `data get block`) e o `custom_name` do item que o bloco solta (`copy_name`).
`name_trace.rs` o repete.

`MachineTraceTest` também grava `heat_generator_name_styled` (o Heat Generator é nomeável) e `chamber_explosion` (`ServerExplosion` de raio 1 sem
resistência de bloco no centro da câmara com nome e 2 operações guardadas: se o bloco saiu, as bolas e o `custom_name` de cada item solto).

`CenaTelas` agora escreve também `cena: <caso> fora <classe> x y largura altura visivel`, para tudo que o original desenha fora dos 176 por 166
da tela: é a lista do que o comparador não vê sem margem (a de energia e a de calor são elementos de textura visíveis; os suportes de
configuração, transporte, segurança, livro e redstone a cena oculta; o Heat Generator mostra cinco slots de armadura em x = -21 e, desde 2026-11-07, o suporte deles em (-26, 6), `isArmorHolder`; os suportes da segurança e da redstone, à direita, seguem ocultos). Os casos
`_aba_energia` (`casos_energia`) põem o cursor na aba de energia com a máquina parada, cheia, ativa e com upgrades de velocidade e de energia
(`energia=300`, porque a capacidade muda com eles), e `infuser_operacao_10` é o controle da barra de químico. O roteiro do port tira o print até
dois seguidos serem iguais quando o caso clica, e dá um tick depois de abrir a janela de upgrades, que só recebe os dados no tick seguinte.

### Capacidade de energia posta por um jogador, e o clique do cursor

`testes/PlacedTraceTest.java` grava (`placed_<máquina>.jsonl`, quatro máquinas e o Heat Generator) a capacidade de energia (`currentMaxEnergy`), o consumo por tick e os
ticks por operação de uma máquina posta de três jeitos: `setBlock` do gabarito (`template`), `useOn` do item como um jogador (`use_on`, também com 9 pares de upgrade de
velocidade e de energia no componente `UPGRADES` do item) e lida do NBT que o tile gravou (`BlockEntity.loadStatic`, `*_loaded`, com as chaves salvas e o filho `component_upgrade`).
`placed_run_<máquina>_<plain|speed2>.jsonl` é uma máquina posta por um jogador (com velocidade 2 no item, ou sem upgrade) a que se dá uma só carga de energia, a
que ela aceita, e que depois anda 230 ticks. `mods/mekanism/tests/placed_capacity.rs` (inclui o par de upgrades em que a `redstone` converte), `placed_run.rs` e
`mods/mekanismgenerators/tests/placed_capacity.rs` os repetem.

`testes/SlotClickTraceTest.java` grava (`slot_click_<máquina>.jsonl`) o clique do cursor no menu de verdade (`MekanismTileContainer.clicked`): `mayPlace` e o que o clique
esquerdo, o direito, a tecla numérica e o arrasto (para o primeiro slot do inventário) deixam no slot, no cursor, na barra e no slot do arrasto, com 15 itens, o slot vazio, com
outro item e com o mesmo, e o Metallurgic Infuser em seis estados de tanque e entrada. `mods/mekanism/tests/slot_click.rs` repete o `mayPlace` com `rules::slot_accepts`.
Em jogo, no `horizon2`, `horizon2/h2_cursor.py MAQUINA` repete uma amostra das linhas (cada tipo de slot, ação, conteúdo e resposta; com o servidor parado no
Infuser e no Heat Generator, porque o slot de infusão converte e o gerador queima no tick seguinte) e compara slot, cursor, barra e arrasto; `h2_placed.py` mede a
capacidade pela energia que um Heat Generator encostado deixa na máquina, e uma operação de 200 ticks com o servidor parado. Copie os dois `.java` para
`src/src/gameTest/java/mekanism/common/tests/network/` só na hora de rodar, como os outros, e apague-os depois.
Além dos quatro cliques, o `SlotClickTraceTest` grava, nas mesmas linhas (`machine_after` com todos os slots da máquina e `dropped` no Q): o Q (`throw`, uma unidade) e o Ctrl+Q (`throw_all`) de um slot com outro
item ou com o mesmo, o clique do meio (`clone`, com o jogador no criativo) e o clique duplo (`double`: `PICKUP_ALL` sobre o slot vazio, com o item nos outros slots da máquina, 2 na barra e 5 no inventário);
dois baldes com o componente `max_stack_size` 16 (`lava_bucket+16`, `water_bucket+16`), varridos com os quatro cliques; e, em `slot_click_upgrade_<máquina>.jsonl` (outros campos: `kind` é `upgrade_in` ou
`upgrade_out`, `item` é `upgrade:<tipo>` ou o id de um item comum), os dois slots virtuais da janela de upgrades com os oito tipos do registro e dois itens comuns. `mods/mekanism/tests/slot_click.rs`
(`the_original_throws_clones_and_collects_without_asking_the_slot_rules`, `the_cursor_puts_a_supported_upgrade_in_the_upgrade_input_and_nothing_in_the_output`) e
`mods/mekanismgenerators/tests/upgrade_window.rs` os repetem. Em jogo, `horizon2/h2_cursor_extra.py throw|stacked|upgrade MAQUINA` (`H_NAME=horizon`): o Q e o Ctrl+Q pelo teclado sobre o slot, os baldes empilhados
com `[minecraft:max_stack_size=16]` e os slots da janela (a aba em (422, 116) com guiScale 3, o servidor parado); o `horizon-ctl` não manda o clique do meio nem o clique duplo.

`testes/UpgradeQuitTraceTest.java` grava (`upgrade_quit_{enrichment_chamber,metallurgic_infuser,heat_generator}.jsonl`) o `Save and Quit` com a janela de upgrades aberta: o menu e a janela selecionados pelos pacotes reais
(`PacketWindowSelect`, `PacketGuiInteract.CONTAINER_TRACK_UPGRADES`), upgrades no slot da janela por um clique do cursor (instalando, os 20 ticks), a máquina andando com energia, entrada e progresso, e `quit`: o tile gravado com o
menu aberto (`saveWithFullMetadata`, como `PlayerList.remove` faz antes de `Player.remove` fechar o menu) e relido (`BlockEntity.loadStatic`). Mostra que os itens dos dois slots, os instalados, a energia, a entrada, a saída, o tanque e o
progresso voltam, e que `upgradeTicks` e `canCheckUpgrades` não são gravados (a instalação recomeça dos 20 ticks). `mods/mekanism/tests/upgrade_quit.rs` e `mods/mekanismgenerators/tests/upgrade_window.rs`
(`the_generator_keeps_the_upgrade_slot_and_restarts_the_install_after_save_and_quit`) o repetem; em jogo, `horizon2/h2_quit.py antes|depois` (o jogo é fechado com `horizon-ctl quit` com a janela aberta e aberto de novo entre as etapas).
`testes/ConstantUsageTraceTest.java` ganhou `infuser_const_ops` e `infuser_const_ops_placed`, com `maxUpgradeMultiplier` 1.000 (o valor em cache de `CachedIntValue`, devolvido ao fim): `getOperationsPerTick` passa de 1 (2 e 5 por tick).

`testes/TrackerTraceTest.java` grava (`{chamber,crusher,smelter,infuser,heat_generator}_trackers.jsonl`) o que o `MekanismContainer.broadcastChanges` e o
`sendAllDataToRemote` mandariam a um jogador com a tela aberta, uma linha por tick (`sent`, `dirty`, cada entrada `[índice, classe, "DIRTY"|"SIZE", valor]`).
O jogador é um `ServerPlayer` falso que não é o dono do menu fora dos pacotes, então o teste chama os métodos reais do menu uma vez por tick em
`ServerTickEvent.Post` (um espião troca cada `ISyncableData` só durante a chamada, para anotar o `getPropertyData`); o `startTrackingServer` da janela de upgrades
é lido logo depois da chamada. A energia entra pela capability do bloco depois de os lados de energia irem para `INPUT` (o bloco do molde não aceita de nenhum
lado), o que alimenta o `lastEnergyReceived`. Copie o arquivo para `src/src/gameTest/java/mekanism/common/tests/network/` só na hora de rodar o
`runGameTestServer` e apague-o depois. `mods/mekanism/tests/tracker_trace.rs` e `mods/mekanismgenerators/tests/tracker_trace.rs` os repetem.
