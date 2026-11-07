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
| Ponto equivalente a `ServerTickEvent.Post` | Dá por hook | `server_tick`, fase `ServerTickEnd`: depois do tick de todas as dimensões. Só o último nível está no lugar, então entre em cada nível com `enter_level`, como faz `close_idle_levels`. O hook `tick` roda antes das block entities e não serve |
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
| Comparador | Dá por hook | `has_analog_output`, `analog_output` e `native_update_output_neighbors`. Não notifique dentro do `block_entity_tick`: enfileire e rode em `level_tick_post` |
| Item guardando conteúdo | Dá por hooks | `component_types`, `destroy_block`, `loot_drops` e `place_block`, com `explosion_detonate`. `loot_function` estraga o NBT, e `block_entity_data` não vale para bloco de mod. Pistão e dispenser ficam de fora |
| Som contínuo | Dá com módulo | Nada no catálogo: `[[module]]` em `crate::game` com `SoundInstance`, `Ticking::Native` e `stop_and_release`. `Ticking::Native` só aparece em teste |
| Tooltip e shift | Dá por hook | `tooltip` com `Game::has_shift_down`. A tecla N da descrição precisa de rastreio próprio |
| Tela com abas e sub-janelas | Dá por hooks | `screen_*`, tudo desenhado pelo mod, com `EditBox` disponível. Slot que anda com a janela pede HEAD em `native_screen_slot`, não testado |
| Sincronizar valor grande | Dá por hook | `native_net_send_to_player`, recebido em `net_from_server`, análogo ao `PacketUpdateContainer` |
| Energia acima de `i32` | Dá em NBT próprio | Ver as decisões abaixo |
| Tempo de queima | Dá por dados | Componente `minecraft:cooking_fuel` no item, com o provider escrito por extenso |
| Tags `c:` | Conferir | O Horizon não traz nenhuma. Das 85 do port, 62 não vêm da datagen do Mekanism e devem ser do NeoForge. Tag vazia faz a receita sumir em silêncio |
| Rodar o Mekanism real para comparar | **Rodado** | `runGameTestServer` no desktop, cerca de 30 s, 70 casos e 0 falhas. `dev/referencia/testes/TickTraceTest.java` grava o estado por tick, e duas rodadas deram arquivos idênticos |

### Decisões

1. **Seleção do Configurator (usuário, 2026-11-05).** Injetar mais fundo para ter várias caixas, como o Java, sem quebrar o jogo nem
   a compatibilidade com outros mods: a injeção só age no bloco do Mekanism e com a ferramenta na mão, e o resto segue o caminho
   normal.
2. **Energia em `long` (usuário, 2026-11-05).** O Energy Cube criativo tem `Long.MAX_VALUE` e a Induction Cell básica,
   8.000.000.000, e a API do Horizon é `i32`. A energia do Mekanism vive em `i64` no NBT próprio, e `ModEnergyBuffer` fica só para
   falar com outros mods.
3. **Duas entradas com regra de slot (aberta).** Aproximação ou nova injeção, a decidir quando chegar o Combiner.

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

**Fase 1, base de tile.** *Estado (2026-11-07): em andamento. Feitos nas quatro máquinas e no Heat Generator: a base de tile, o `CachedRecipe` com erros e avisos na tela, upgrades,
redstone, comparador, nome, som, luz, experiência, o clique do cursor, a capacidade de energia, o hopper e o ejetor (este só por comando). Falta o que está em "Falta na Fase 1" abaixo.*
Porte de `TileEntityMekanism`, `TileEntityConfigurableMachine`, `TileEntityRecipeMachine`,
`TileEntityProgressMachine` e `CapabilityTileEntity`: ordem do tick, `setActive` com seu atraso, controle de redstone, nome,
comparador, som, upgrades, sincronização de tela (`MekanismContainer`). Inclui `CachedRecipe` completo, com `OperationTracker`,
`pausedForErrors`, `recheckAllErrors`, erros e avisos da tela, e `usedSoFar` para `per_tick_usage`.

Estado em 2026-11-05: Enrichment Chamber, Crusher, Energized Smelter e Metallurgic Infuser rodam na base nova em jogo, e a fase segue
aberta. Em `machinery_basis/core` estão `transaction`, `containers`, `chemical`, `operation_tracker`, `cached_recipe`,
`recipe_handlers`, `two_input_recipe`, `recipe_monitor`, `tile`, `item_handler`, `transit`, `ejector`, `warning`, `gui`,
`tooltip`, `sound`, `experience` e `sim`, que agora tem chunks falsos (`sim/chunks.rs`: níveis de ticket, `toDrop`, carga e descarga na ordem do NeoForge, block
entities que somem e voltam com o chunk; `sim/open_hash.rs` porta a ordem de iteração do fastutil, conferida contra o jar real).
`mods/mekanism/tests/chamber_trace.rs` e `infuser_trace.rs` repetem tick a tick os rastros de
`dev/referencia/rastros/`, gravados por `dev/referencia/testes/MachineTraceTest.java`.

O Heat Generator tem três rastros, `heat_generator_lava_run`, `heat_generator_fuel_run` e `heat_generator_lava_sides`, e
`mods/mekanismgenerators/tests/heat_trace.rs` os repete tick a tick, igual ao bit: calor, temperatura, energia, lava, produção, queima,
combustível, a lava que acaba, o calor guardado que segue gerando, o buffer cheio, o item que não cabe e o que cabe por exato, o balde, e
as faces com lava. Dois erros que só o rastro mostrou ficaram corrigidos: a perda para o ambiente usa o ar de cada face
(`getAmbientTemperature(side)`, o do bloco vizinho) e `HeatAPI.EPSILON` é `1e-6` (um `float` ampliado), não `1e-5`. Com a regra do tile
igual ao Java, a diferença da barra de energia e do medidor de lava na comparação de telas não vem dela.

Em jogo foram comparados estado a estado `chamber_energy` (342 estados), `crusher_run` (377) e `smelter_run` (342). Em `infuser_run`, os 201
estados até o primeiro evento e, dali em diante, as transições, porque os comandos não caem no tick exato do rastro. Ejetor, redstone e salvar
e reabrir foram conferidos em jogo sem comparar cada tick. Um mundo antigo migrou o tanque para `chemical_tanks`.

O Horizon não simula inserção, então o ejetor transaciona numa cópia do vizinho, e `slot_accepts.rs`, injetado em
`game/machines/native_items.rs`, dá a regra real de cada slot, que é privada ali: é o `isValid` do `ResourceHandler`. O rastro
`chamber_eject_furnace` confere a regra sem o jogo, e em jogo forno e baú quase cheio deram o que o original dá.

Falta na Fase 1:

- Receita de uso constante nas máquinas. `ItemChemicalConstantUsageRecipe` (`ItemStackConstantChemicalToObjectCachedRecipe`), o
  `ChemicalUsageMultiplier`, o `usedSoFar` e os handlers de fluido e de saída de químico estão na lib, conferidos tick a tick com o
  Osmium Compressor original (`compressor_run`). O Metallurgic Infuser já escolhe por `per_tick_usage` no `createNewCachedRecipe`,
  como o Java (`ConstantUsageRecipeLookupHandler`, com o `usedSoFar` do tile): nenhuma receita dos arquivos do original usa a opção
  nele, então o caminho só é exercitado por um teste sintético (`recipe_order.rs`), sem rastro do original para o Infuser. A
  Enrichment Chamber, o Crusher e o Energized Smelter não leem o campo (o original também não: a receita deles não o tem). Seguem
  sem máquina portada o Osmium Compressor, a Purification Chamber e a Chemical Injection Chamber, que o usam.
- `fillTankFromSlot` e `ChemicalAttributeValidator` estão na lib, com testes, mas nada os alimenta: nenhum item portado tem a capability
  de químico (`ChemicalItems`), e nenhum mapa de dados de atributos (`chemical_attribute_*`) é lido para o `ChemicalAttributes`. O
  `setLastTransferDirection` que o Java grava no slot dentro do `fillContainerFromSlot` não foi portado, porque só o `handleContainer`
  o lê e nenhum slot portado o chama.
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
  `recipe_cache.jsonl` (7 execuções de `findFirstRecipe` e 4 perguntas por tipo em que a ordem da lista erra). Não coberto sem o jogo: o
  `DataComponentIngredient` exato, que o Horizon não representa (os arquivos das quatro máquinas não o usam).
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
  **Hopper embaixo** (resolvido, ver "Slots de entrada"). Tags `c:` da datagen: das 173 de `mekanism` e `mekanismgenerators`, 6 foram
  copiadas (`c:alloys`, `c:circuits` e as três de `circuits/*`, e `c:ores/osmium` de bloco); as 143 que faltam citam item, bloco ou
  fluido que o port não tem (pós, lingotes, pepitas e blocos de bronze, chumbo, estanho, urânio e fluorita; baldes; pellets; baús e barris
  pessoais; ferramentas Meka; `#mekanism:transmitters/buffered`), e não se edita uma tag do original para listar menos. Fluid e
  entity_type não foram copiados: não se confirmou que o Horizon os leia. Falta ainda comparar o que o Horizon descarta em silêncio
  (entrada ou `#tag` desconhecida) com cada receita portada quando esses itens chegarem.
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
  bio fuel quando o tanque tem fungi, e embaixo da Chamber não leva nada. Fica para a Fase 2: uma máquina posta por `setblock` ficaria
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
  posta por `setMaxEnergy`; o port os repete tick a tick. Sobram `EitherSide`, `Triple` e `Rotary`, sem máquina portada que os use, e os limites de taxa. O `HashSet` das receitas
  complexas do Java itera em ordem de hash, e aqui na da lista, o que só difere com mais de uma receita complexa casando a mesma
  entrada.
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
  vazio) e é morto por `slot_rules.rs`. **Fora:** carbon e tin: o original tem 31 receitas `metallurgic_infusing` e o port 19 (faltam
  `processing/iron/enriched`, `processing/iron/enriched_dust`, `processing/steel/enriched_iron_to_dust`, `processing/bronze/*`,
  `processing/netherite/scrap_to_dust`, `processing/refined_obsidian/dust/from_obsidian_dust`, os quatro `control_circuit/*` e o
  `mekanismgenerators:hohlraum`), então nenhuma receita usa carbono nem estanho, o tanque os recusa e o `coal` não entra no slot de infusão (no original entra).
- Upgrades: a aba e a janela estão portadas nas quatro máquinas. Em jogo (desktop, 2026-11-06), nas quatro: abrir, instalar (20 ticks, vários de uma vez até o máximo) e remover; saída cheia (64) bloqueia a remoção; saída com outro tipo bloqueia; a lista rola com 8 tipos; o Dump atrás da janela no Infuser não recebe o clique (sem a janela recebe); mundo antigo (sem `component_upgrade` e sem os slots 3 e 4) abre sem perder item; salvar e reabrir devolve os upgrades e os dois slots cheios; jogador que cai com a janela aberta não deixa seleção (o que reentra faz shift-clique normal); `upgradeTicks` anda com a tela fechada e o progresso aparece ao abrir; o convidado vê os tipos (sincronia por rede). Pixel contra o oficial (guiScale 2 e 3, quatro máquinas, sem tolerância e sem máscara além da das abas): zero nos elementos do mod, exceto os ícones de item em escala (H25: "Supported:" e linha da lista), o mundo atrás da janela (H22) e, com o servidor parado, o Dump que o port só aplica no tick seguinte; a ordem da lista com mais de um tipo é de hash por identidade no original, então a comparação vale com um tipo (casos `_um_`). Não testado em jogo: o reenvio dos tipos ao convidado no `/reload`, e Save and Quit com a janela aberta. Estão portados e conferidos contra rastros do original (2026-11-06): o tooltip do item (`upgrade_tooltip`, os 8 tipos, linha a linha), a receita e a tag `muffling_center` do `muffling` e o efeito dele no som (`chamber_muffling`), e o `xp_containment` (`chamber_xp_containment`, `chamber_xp_vanilla`, `chamber_xp_broken`, repetidos por `effects_trace.rs`). A aba de XP (`GuiXpContainmentTab`) está portada, veja o item dela abaixo. Faltam os tipos que
  nenhuma máquina portada aceita. Em jogo foram conferidos a receita, o shift-clique, reabrir e a capacidade por instância em
  `native_energy_at` (1000, 600 com 2 de energia, 400 com 4, e a vizinha não muda). A instalação de 20 ticks só foi comparada sem o jogo.
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
  fora da distância de simulação (140 blocos, simulação 5, 600 ticks, outra máquina andando lá) a capacidade das quatro volta inteira (1.000, 1.000, 1.000 e 1.276 do Crusher); **não provado** que a varredura dos objetos vivos
  correu nesse teste (o jogo não diz), então o caminho de `SWEPT` só foi conferido pela leitura do código, sem prova em jogo. Uma operação de 200 ticks de uma Chamber do jogador, energia posta de uma vez e servidor parado: 100 ticks depois
  energia 500 e progresso 100, aos 200 energia 0, progresso 0, saída `clay_ball` 4 e a entrada em 4 (a segunda operação não começa), como `placed_run_enrichment_chamber_plain`. **Consumo e tempo** de uma máquina posta por um jogador, com uma só carga de energia
  (`placed_run_*`, 233 ticks cada, plain e velocidade 2, as 4 máquinas): a carga é a capacidade (1.000, 1.792 com velocidade 2, 1.600 e 2.912 no Crusher), a operação gasta
  tudo em 200 (112) ticks e a segunda não começa; `placed_run.rs` repete cada tick (energia, progresso, ticks, consumo, saída). **O que a conclusão
  anterior errava:** "no máximo 1.000 com upgrades" (a tabela vai a 16.000) e "a `redstone` nunca converte nas 4 máquinas": converte com velocidade 8
  (Chamber, Smelter, Infuser; 7 e 8 no Crusher) e o contêiner vazio; `placed_capacity.rs` confere, nas 324 linhas, que o port converte exatamente onde o Java chega a 10.000.
  **Fica de fora:** o item que guarda os upgrades do bloco quebrado (o `UPGRADES` do item; o port solta o conteúdo ao quebrar), então a linha `use_on` com
  upgrades no item só é repetida por `apply_item_components` com o mapa na mão; a primeira capacidade de uma máquina recém-posta é a do bloco (20) até o primeiro tick
  dela (um cabo que enche nesse tick vê 20, e o Java vê 1.000).
- Upgrade sem tipo: o `select` do modelo já cai em `ItemModelNode::Select { fallback: None }`, que o Horizon resolve com `LayerContent::Missing`, o `missingItemModel` do original. O que aparece vazio é o slot do inventário: em jogo (2026-11-06) o upgrade sem componente some da barra de atalhos, tanto com esse caminho quanto com um `Model` explícito de `minecraft:builtin/missing`. É do desenho de ícone do Horizon (a camada `Missing` não sai no slot) e fica registrado como pedido ao renderer; o código do mod não muda.
- Som da máquina (`TileEntityMekanism.updateSound`, `SoundHandler.TileTickableSound`): portado nas quatro máquinas (módulo `tile-sound`, H19). Em jogo (2026-11-06, Enrichment Chamber), o `status` do jogo mostra `mekanism:tile.machine.enrichment_chamber` tocando e o stderr diz `start`, `volume 1 -> 0` com o upgrade de silêncio no máximo, e `stop` ao cortar a energia. O que ainda difere: `baseSoundVolume` e `enableMachineSounds` valem os padrões (1 e ligado), porque o port não tem arquivo de configuração de cliente; a distância usa a câmera do último quadro e não os pés do jogador; o som de Natal usa o fuso do jogador, como `LocalDate.now()`; o convidado de LAN recebe a contagem de silêncio por pacote, mas isso não foi testado com convidado. O silêncio vai de 1 (sem upgrade) a 0 (máximo 1: o `muffling.json` do original diz `max` 1, então não há 2 nem 4).
- Experiência (`TileEntityRecipeMachine`, `RecipeCacheLookupMonitor`): `recipesUsed`, a última receita, `hadContainment` (`has_containment`) e `setRecipeUsed` têm rastro. `awardUsedRecipesAndPopExperience` roda ao tirar o resultado (injeção H27: o Horizon não avisa a retirada em slot de resultado de mod), com a experiência de cada receita lida do JSON (`experience`) ou do cozimento do jogo. Em jogo, 2 carvões com `xp_containment` e a retirada pelo clique deram 2 pontos ao jogador (4 operações vezes 0,5) e limparam `recipes_used`. Quebrar a máquina solta a experiência guardada (`destroy_block`, só o jogador; explosão, pistão e `/setblock` não passam por ele), e tirar o upgrade a perde, como o original (`hasXpContainment` já é falso dentro do `recalculateUpgrades`). A parte fracionária usa `Math.random` no Java e uma fração de nanossegundos aqui: sem semente a reproduzir.
- Aba de XP (`GuiXpContainmentTab`, `SupportsXpContainment`; 2026-11-07): portada nas quatro máquinas. Aparece com o upgrade `xp_containment` instalado (`hasXpContainment`), na lateral direita (`imageWidth`) a 90 (com o livro de receitas na tela, como as quatro máquinas têm no original; sem ele o Java sobe a aba 28, para 62), suporte de 26 por 26 tingido de `0xFF80FF20` (`SpecialColors.TAB_XP_CONTAINMENT`) e botão de 18 por 18 em (179, 94), com o ícone `tab/xp_containment` e o fundo `button`, `button_focus` com o mouse em cima (e sem foco atrás da janela de upgrades, `isMouseOverCheckWindows`). Tooltip `tab.mekanism.xp_containment` ("Collect all stored experience."), quebrado em 170 pixels, ao lado do botão. O clique esquerdo (o direito não conta, `isValidClickButton`) manda o botão `COLLECT_XP` (`menu_button`, id 1) ao servidor, que roda `collect_xp` (só com o upgrade) no objeto vivo e solta as bolas no jogador que clicou (`player.position()`); o jogo sincroniza `hasXpContainment` por um data slot a mais (o `SyncableBoolean` de `addContainerTrackers`; o mapa de upgrades só viaja com a janela aberta, então não serve). A mecânica genérica (a geometria de uma aba inset, o fundo do botão, o clique e o foco) está em `machinery_basis_core::side_tab`, e o desenho em `machinery_basis::inset_tab`; o Mekanism só dá `y`, cor, ícone e texto. Contra o original: rastro `xp_tab` (posição, tamanho e visibilidade da aba em 21 estados do cliente oficial), `chamber_xp_containment` e `chamber_xp_vanilla` para o efeito, e pixel (guiScale 2 e 3, 21 estados: sem e com o upgrade nas quatro máquinas, cursor no botão e na borda, janela de upgrades por cima com o cursor atrás dela e fora dela, upgrade instalando e já instalado, clique): zero pixels diferentes na aba, no título e na janela; sobra só o mundo atrás da tela (H22), os cantos transparentes do suporte e do painel e, nos tooltips, o que o fundo de 94 % de opacidade deixa passar do mundo (tolerância de 16 níveis por canal só nessa região; antes eram 5 pixels de GUI). O caso da janela de upgrades com a aba de xp (`chamber_xp_janela`) deu 4800 pixels na lista de upgrades numa rodada: não era corrida do tempo, e sim que a janela só recebe os dados do servidor no tick seguinte à abertura e o servidor estava parado (`tick freeze`); o roteiro do port agora dá um tick depois de abrir a janela, e o caso deu 0 (a lista, a seleção e o resto da janela). A linha de ícone da lista é a diferença H25 de sempre (61 pixels de GUI a guiScale 3). Em jogo (desktop, Enrichment Chamber, speed 8 e `xp_containment`): 2 carvões dão `recipes_used {enriching/enriched/carbon: 2}`, o clique na aba dá 1 ponto de experiência ao jogador e zera `recipes_used`, como o rastro; sem o upgrade a aba some e o mesmo clique não faz nada. As receitas (`ServerPlayer.awardRecipes` e `triggerRecipeCrafted`): o Horizon tem o livro (`level.recipe_book.add_recipes`) e os gatilhos (`recipe_unlocked`, `trigger_recipe_crafted`) como privados de `game.rs`, e o glue os chama sem injeção nova (o módulo é filho de `game.rs`); o jogador que age é o do clique (no `menu_button` o estado dele já está no lugar) ou, ao tirar o resultado, o que clicou (`as_guest` no tick da máquina, e no tick seguinte se o servidor não está à mão). Em jogo (Crusher, `crushing/blaze_rod`): o `recipeBook` salvo do jogador ganha `mekanism:crushing/blaze_rod` só depois do clique na aba (antes tinha só o do Enrichment Chamber, de outro teste). O critério `recipe_crafted` anda pela mesma chamada, mas nenhum avanço do Mekanism o usa no Horizon: a chamada compila e roda, e o efeito não foi observado. Não testado em jogo: a aba para um convidado de LAN (o clique segue o mesmo `menu_button`) e o livro de receitas da tela (o port não o tem; a aba ficaria a 62 só se o original o desligasse).
- Nome com estilo (`CustomName`; 2026-11-07): o componente passa inteiro do item ao tile (`applyImplicitComponents`, o tag do `custom_name` da pilha, que o Horizon guarda inteiro em `StackData.custom_name`), do tile ao NBT salvo (`CustomName`, o mesmo tag), do tile à tela (por NBT no canal `machinery_basis:name`, lido por `name_text` e desenhado por `HudBatch::text_styled`, com a largura do texto estilizado) e do tile ao item solto (`copy_name`). Contra o original (`chamber_name_styled`: um item com nome vermelho e negrito, e outro com um filho azul em itálico): o NBT que o tile grava é o mesmo SNBT do Java, `{bold:1b,color:"red",text:"Camara Colorida"}` e a versão com `extra`, conferido em jogo com `data get block`; o item solto leva `minecraft:custom_name` com o mesmo tag. O título da tela, pixel contra o oficial (guiScale 2 e 3, Enrichment Chamber com nome vermelho e negrito, com filho de outra cor, e Infuser): zero pixels diferentes no título e na tela inteira, fora os cantos do painel. O Horizon tira o tile do mundo antes de rolar a tabela de loot de um jogador, e `NativeLootDrops` não traz o block entity: o nome do item solto vem do que `destroy_block` guardou antes disso (só a quebra pelo jogador; explosão e pistão dependem de o tile ainda existir, não testado). A regra (`machinery_basis_core::name::TileName<C>`) não lê o componente. O Heat Generator é nomeável no original (`nameable = hasGui() && !AttributeGui.hasCustomName()`, e ele não tem nome de tela próprio; rastro `heat_generator_name_styled`: o componente passa inteiro e, sem nome, o título é `container.mekanismgenerators.heat_generator`) e está portado como as quatro máquinas, pelas mesmas funções da `machinery_basis` (`name_*`) e quatro hooks novos em `mekanismgenerators`. Em jogo, o gerador guarda o `CustomName` do item, mostra o título vermelho e negrito (print em `~/.cache/rust-mekanism/wa/xp/`) e solta o item com `minecraft:custom_name`; pixel: o título do Heat Generator com nome (`generator_nome_cor` e `generator_nome_filho`, guiScale 2 e 3) não difere. A explosão: o original (`chamber_explosion`: `ServerExplosion` de raio 1 sem resistência no centro da câmara com nome e 2 operações guardadas) quebra o bloco, solta 1 bola de 1 ponto e o item com o componente do nome. No port, as duas coisas saem de um raio maior (doze TNT no mesmo ponto, porque o bloco resiste a 16 e uma explosão só não o quebra, e uma bomba matando a bola e o item que a anterior soltou impede medir pelo número): uma bola de valor 1 nasce no centro da câmara e o item solto leva `minecraft:custom_name` com o mesmo tag. A experiência de uma explosão passou a ser solta pelo hook `explosion_detonate` (antes só a quebra pelo jogador soltava: `destroy_block`), e o nome do item já saía com o tile ainda no mundo. Pistão: o original recusa (`BlockMekanism.getPistonPushReaction` devolve `IMMOVEABLE` com block entity); no port a máquina também não se move e um bloco de controle ao lado anda (medido com um pistão e um bloco de terra). Mutações do `effects_trace` e do `name_trace` cobrem o que é regra pura.
- Aba de energia (`GuiEnergyTab`; 2026-11-07): um `GuiTexturedElement` de 26 por 26 em (-26, 137), sprite `tab/energy_info`, sem clique e sem configuração de unidade (a unidade é fixa em FE; `compactEnergyDisplay`, que o port já supõe ligado, é o único valor de cliente), nas quatro máquinas e no Heat Generator. Tooltip com `Tooltip.create` de uma lista, uma linha por vez quebrada em 170 pixels: nas máquinas `Using: <consumo>/t` (`getEnergyPerTick` com a máquina ativa, senão zero; o consumo é o do contêiner do cliente, que o servidor sincroniza como `SyncableInt` e aqui vai em dois data slots novos), `Needed: <max(0, capacidade - energia)>` e `Unit: FE`; no gerador `Producing: <taxa>/t` (`getProductionRate`, que o port já sincronizava) e `Unit: FE`. A mecânica genérica está em `machinery_basis_core` (`side_tab::InfoTab`: o retângulo e a janela por cima; `units::energy_info_lines`), o Mekanism e o Generators só dão posição, sprite e textos. Contra o original, pixel (guiScale 2 e 3): a aba e o tooltip sem diferença em 5 estados por máquina (parada, cheia, ativa, com upgrades de velocidade e de energia, e o cursor sobre ela) e 3 no gerador (parado, produzindo), com os cantos transparentes do sprite contados à parte (`canto_aba_energia`) e o tooltip com a tolerância de 16 níveis por canal que o fundo `0xF0100010` (94 % opaco) dá ao mundo atrás, que é o resíduo H22; um texto diferente passa dela (teste do comparador). O que o comparador descobriu escondido em `mundo_atras` e agora conta como elemento do mod (e falha enquanto faltar): no Heat Generator, a aba de calor (`GuiHeatTab`, -26, 109, visível), os cinco slots de armadura e mão (`GuiSlot` em x = -21, y = 10 a 82, visíveis, e o suporte, oculto na cena até 2026-11-07 e visível depois) e a aba de upgrades à direita (o gerador aceita o upgrade de silêncio e o original a mostra); nas máquinas, tudo o que sobra fora da tela é suporte oculto pela cena e sem equivalente no port (configuração de itens e de transporte à esquerda; segurança, livro de receitas e redstone à direita). Medidas do que falta: a aba de calor pede `lastTransferLoss` e `lastEnvironmentLoss` do gerador (dois `double`, quatro data slots), a unidade de temperatura do cliente (`MekanismConfig.local.tempUnit`, que o clique troca entre K, C, F, R e STP, e que o port não tem como config de cliente), as conversões de unidade e três linhas de idioma; os slots de armadura pedem o menu do gerador com cinco slots a mais e o suporte, o que muda o layout do menu e as regras de quem os enche (portados em 2026-11-07, veja o item dos slots de armadura do Heat Generator); a aba de upgrades do gerador pede a janela de upgrades do gerador (menu, spec, rastro). Não foram portados. No Infuser, o controle `infuser_operacao_10` (sem aba nenhuma: 10 passos operando) já difere na barra de químico (536 pixels a guiScale 2, e 1431 a 3), então a diferença que os casos `ativa` e `velocidade` do Infuser mostram na barra de químico é do alinhamento de ticks do químico, anterior a esta mudança e não investigada.
- `MekanismContainer`: os avisos da tela estão nas quatro máquinas, comparados lado a lado com o oficial em 17 cenas, e a
  máscara de erros e o redstone vão nos data slots. Falta a lista de trackers do `MekanismContainer`. O nome (`getName`, `getDisplayName`, `CustomName`) está portado com o componente inteiro (cor, negrito, itálico e filhos): o item o dá ao tile ao colocar, o tile o dá ao item solto, e o servidor o manda ao cliente ao abrir a tela, que o desenha como título (`name_trace.rs` contra `chamber_name` e `chamber_name_styled`; item abaixo).
  **Lista de trackers (medida em 2026-11-07, não portada: é muito maior que parece).** O original sincroniza
  16 `ISyncableData` na Enrichment Chamber (`chamber_container_tracking.jsonl`: `SyncableStreamCodec`, `SyncableEnum` x2,
  `SyncableBoolean$3` x2, `SyncableLong$2` x2, `SyncableInt$2`, `SyncableLong$2`, `SyncableBoolean$1` x4, `SyncableBoolean$3`,
  `SyncableInt$2` x2, nessa ordem) e 18 no Infuser (`infuser_container_tracking.jsonl`: acrescenta um `SyncableLargeResourceStack` na
  sexta posição e um `SyncableBoolean$1`); o rastro dá a ordem e a classe, não o campo de cada posição, e o Heat Generator não tem
  rastro da lista; a janela de upgrades acrescenta 2 por jogador. A lista é por jogador (`broadcastChanges` manda um `PacketUpdateContainer` só
  com os que sujaram, e `sendAllDataToRemote` manda todos ao abrir). O port não tem essa lista: tem um layout de data slots inventado
  (`DataLayout`: energia em 2, progresso, capacidade, máscara de erros, redstone, `ticksRequired`, 9 dos upgrades e, no Infuser, 16
  para o id do químico), que o Horizon manda por diferença a cada tick do menu, e que não corresponde um para um (o tanque do Infuser
  vai em quantidade, `tint`, sprite e id; o original manda a pilha). Portar a lista de verdade é: (1) ler as `@ContainerSync` de cada
  classe da hierarquia das cinco máquinas para saber o campo de cada posição (o rastro só dá a classe do tracker); (2) as classes
  `Syncable*` (seis nas máquinas, mais o `SyncableDouble` do calor no Heat Generator) com `isDirty` e `getPropertyData`; (3) um rastro do Java do que cada `PacketUpdateContainer` leva a cada tick, nos cinco
  contêineres, abrindo, rodando uma receita e fechando; (4) trocar o transporte por `native_net_send_to_player` com um espelho no
  cliente que a tela, a aba de avisos, a de redstone e a janela de upgrades leiam no lugar dos data slots, o que mexe em `tile.rs`,
  `windows.rs`, `screen_draw` das cinco telas e nos testes `screen.rs`, `warnings.rs` e `upgrade_window.rs`; (5) a contagem de jogadores
  (`players_using`, que hoje não vê o espectador). Não foi começado porque ou se troca o transporte inteiro ou o port fica com duas
  fontes da verdade.
- Tela, ainda diferente do original:
  - abas laterais (a de energia foi portada, veja o item dela): duas à esquerda, três à direita e a de redstone embaixo à direita com o aviso dela,
    que pede o blend `DST_COLOR` que `native_sprite` não tem. Ficam fora do retângulo da tela, então não mexem em nenhum pixel dos
    elementos que existem; `dev/referencia/comparar_tela.py` as tira da conta;
  - o sprite do químico do Infuser e a lava do medidor são tiras animadas do atlas de blocos. A tira vai no caminho do original
    (`textures/mek_chemical`) com o `assets/minecraft/atlases/blocks.json` dele, e o HUD do Horizon a anima como o jogo, sem
    injeção. Em imagem, os quadros inteiros são idênticos (16 de 40 prints do ciclo do químico e 37 de 40 da lava casam pixel a
    pixel com um estado do port); nos prints entre dois quadros a diferença é de um nível em até 64 pixels, e **não** vem do
    `partialTick`: o original também interpola com ticks inteiros (`AnimationState.tick` soma `subFrame`, `drawToAtlas` manda
    `(int)(subFrame / time * 1000)` e o shader faz `mix(atual, próximo, millis / 1000)`), e o Horizon usa a mesma conta. O que muda é o
    arredondamento da mistura para 8 bits: o Horizon faz em inteiros, com o empate exato para cima (`+ 500`), e o original, na GPU do
    desktop, arredonda os empates para baixo e ainda tem cerca de 0,5 % das amostras perto do empate (fração .52 a .56) que nenhuma
    regra fechada reproduziu. A injeção `sprite_ties` (H26) mistura com o empate para baixo, e medida em jogo contra o original, sem
    tolerância e sem máscara, dá 40 de 40 em `base` (antes 18), 16 em `bio` (antes 10, pior caso 72 px contra 208) e 21 em `fungi`
    (antes 6, pior 84 contra 184), e a lava segue 40 de 40. Nenhum caso piorou; o resto é a precisão da GPU. Medido nos prints
    existentes, antes da injeção: `base` 40 de 40 com o empate para baixo (16 de 40 com o do Horizon);
    `bio` 19 de 40 (12) e `fungi` 22 de 40 (4). Os três prints da lava e os 464 pixels de `barra_energia` eram do gerador queimando a
    lava (a energia sobe, e o port estava um passo adiantado), não da animação: o caso agora usa energia cheia;
  - o mundo atrás da tela, e o brilho dos quatro cantos transparentes do painel, saem 2 a 3 níveis mais escuros no Horizon (13 contra
    10). Os cantos são 72 pixels por tela e entram na tabela à parte;
  - o ícone de um item de bloco no slot (carvão não, que é plano; argila, areia) e a borda do tooltip de item são desenhados pelo
    próprio Horizon, com sombreamento e cantos diferentes do vanilla;
  - o que a tela desenha agora segue o Java (`GuiBar`, `GuiVerticalPowerBar`, `GuiChemicalBar`, `GuiFluidGauge`, `GuiSlot`,
    `GuiProgress`, rótulos e tooltips de widget), coberto por testes sem o jogo e comparado em imagem com o original, por
    `dev/referencia/comparar_lote.py`, em 36 estados por escala de GUI (2 e 3): slots, barras, setas, medidor, rótulos, título e
    tooltips de widget com 0 pixels diferentes, fora as diferenças de renderer acima.
- `NOT_ENOUGH_INPUT` e `NOT_ENOUGH_SECONDARY_INPUT` não acenderam em nenhum dos dois jogos, e o overlay de nível >= 0,98 e
  PULSE só têm teste de unidade.
- Ejetor para composteira (feito, 2026-11-07). O caminho do original é `Capabilities.Item.BLOCK` da composteira, o `ComposterWrapper` do
  NeoForge (lido em `neoforge-26.3.0.100-sources.jar`): só as faces vertical têm capacidade, e **a saída padrão do item é a
  direita, uma face lateral, então com a configuração padrão o ejetor nunca chega a uma composteira, no original nem no port**
  (rastro: a Chamber ejetando para uma composteira ao lado, fase `east_cake_level0`, não tira nada). Só chega com a saída configurada em cima ou
  embaixo, que o original faz pela tela de lados e o port ainda não tem (Fase 2). A lib porta o wrapper
  (`machinery_basis_core::composter`): a face de cima aceita um item por vez, sorteia as camadas do componente `compostable` uma vez
  (`getComposterValue`), recusa se o nível já é 7 ou se o sorteio dá 0 (o item não é gasto) e sobe o nível em **um**, não nas
  camadas sorteadas (o vanilla, que o Horizon porta em `composter_container_insert`, soma as camadas e gasta o item mesmo com 0); a
  de baixo só entrega o farelo de osso do nível 8; o `onRootCommit` faz o `setBlockAndUpdate` com o evento de jogo, agenda 20 ticks
  ao chegar a 7 e manda o evento 1500. O Horizon não mostra a composteira em `native_container_slots`, e o sorteio e a subida
  vivem em funções privadas de `composter.rs`: um módulo (`composter-wrapper`, H27) expõe `machinery_composter_layers` e
  `machinery_composter_raise`, e o glue sorteia na hora de criar o handler (uma vez por tentativa, só com o nível abaixo de 7 e o
  item compostável, como o Java) e aplica o nível no `deliver`. Rastro `composter_enrichment_chamber.jsonl` (`IdentityTraceTest`,
  `chamberComposter`): oito fases (de baixo com `cake` nos níveis 0, 6, 7 e 8, `stick`, `oak_leaves` no nível 0, de cima, ao lado),
  360 ticks, repetidos tick a tick por `composter_trace.rs` e **em jogo** (`horizon2`, `tick freeze` e `tick step 1`, nível e saída
  iguais nos 360 ticks). O sorteio que não é determinístico (`low`, `medium`... fora do nível 0) não tem rastro: só o `always_add_one`
  (`cake`) e o nível 0 de qualquer item. A chave `mekanism_eject_face` escolhe a face do ejetor enquanto a tela de lados não existe.
- Identidade do item (feito, 2026-11-07). `ItemResource` agora é o item, a pilha máxima e os **componentes** (`Components`, um mapa de
  id de componente para o valor em texto canônico: a lib não conhece nenhum componente), e o dano é o componente `minecraft:damage`,
  como no NeoForge (`ItemResource.equals` e `ItemStack.isSameItemSameComponents`). No glue, o texto é o `canon_tag` do
  `encode_components` do Horizon, e uma tabela por chave guarda o dano e o `StackData` de onde o texto veio, para refazer a pilha
  quando o recurso sai de um slot da lib (`stack_of_resource`). `room_for` é a conta de quanto cabe num slot (outro recurso, nada) e
  é a mesma que o `InventorySlot::insert` usa; o hopper escolhe com `hopper::first_taker` e o shift-clique do menu das máquinas
  insere com `room_for`; o limite de pilha lê o componente `minecraft:max_stack_size`. O tipo do upgrade é um componente comum
  (`mekanism:upgrade_type`, o valor é um texto), `UpgradeData.upgrade_of` lê dele e ignora os outros componentes, e o NBT dos slots de
  upgrade grava o `components` inteiro. Rastros do Java (`IdentityTraceTest`): nas quatro máquinas e no Heat Generator, o hopper de cima
  tick a tick (nome A no slot e B, simples e A no hopper; slot vazio; simples no slot; dados diferentes; quase cheio) e o shift-clique
  (seis combinações do jogador para o slot, e duas do slot para a barra). Repetidos por `identity_trace.rs` e **em jogo**
  (`horizon2`, `h2_identity.py` e `h2_heat.py` nas quatro máquinas e no Heat Generator: mesmo estado final em todas as fases do hopper, e nos
  shift-cliques das quatro máquinas). **Fica de fora:** o dano não tem rastro (nenhuma entrada de receita é um item com durabilidade, então
  nenhum item com dano chega a um slot de entrada), só o código e a leitura do NeoForge; o `DataComponentIngredient` exato continua
  sem representação no Horizon, então um ingrediente que exige componentes não é testado com eles (as receitas portadas não têm
  nenhum); a tabela guarda cada conjunto de componentes que já passou por uma máquina e cresce com eles. **Heat Generator, shift-clique (corrigido,
  2026-11-07):** o gerador usava o roteamento padrão do Horizon, e o item que o slot de combustível recusava ia para o slot de energia e
  não para a barra de atalhos. Agora o `menu_quick_move` dele (uma injeção, `menu_quick_move` de `mekanismgenerators`) roda o mesmo
  `quick_move` da lib com os dois slots do bloco: combustível (o que queima, ou o balde de lava) e energia (`EnergyInventorySlot.drain`,
  `energy_can_drain`, que nenhum item portado satisfaz, porque nenhum tem handler de energia). Rastro `identity_heat_generator.jsonl`
  (as oito cenas das máquinas, graveto sobre o combustível ocupado e sobre o vazio, carvão, balde de lava e argila sobre o combustível vazio e o
  tablet de energia, que não se repete porque o item não é portado), repetido por `identity_trace.rs` e em jogo. A cena da argila
  (`clay_ball_player_fuel_empty`, 2026-11-07) existe porque a mutação "o combustível aceita tudo" sobrevivia: nenhuma cena punha um item que
  não queima num slot de combustível vazio. Com a cena, a mutação (`burns` sempre verdadeiro) falha (`os slots`: a argila vai para o
  combustível, no original fica no inventário); o rastro inteiro foi regravado (as outras cinco máquinas e a composteira saíram byte a byte iguais), e `h2_heat.py`
  confere as 13 cenas em jogo.
  Fora: o `energy_tablet` (e qualquer item com handler de energia) e o `drainContainerIntoSlot`, que o gerador não porta; os dois
  slots virtuais da janela de upgrades que o Heat Generator tem no original (índices 0 e 1 do menu) não existem no port, e a ordem de
  `slots_of` já os põe fora.
- `setRecipeUsed` agora tem rastro (`chamber_xp_*`). `recheckAllErrors` com jogador olhando e `NOT_ENOUGH_ENERGY_REDUCED_RATE` agora têm rastro.
  Este último só existe no Electrolytic Separator, que não está portado: `separator_reduced_rate` é usado só pelo tracker de avisos.
- `players_using` vem do hook `mod_container_viewers`, que não conta espectador. O `PacketTileButtonPress` do Java abre a tela sem
  checar espectador, então esse caminho diverge. O caso de `born` com a tela já aberta não foi exercitado em jogo.
- `sim::chunks` (carga, descarga, blocos, block entities e a ordem dos eventos) existe e é testado, mas ainda não cobre as notificações
  de vizinhança do `setBlock` (`onNeighborBlockChange`), e os testes de rede em si são da Fase 3.
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

**Fase 2, lados coloridos.** *Estado (2026-11-07): aberta. Adiantado: a regra por slot (`slot_accepts`, o `isValid` do `ResourceHandler`) e o ejetor sobre ela; falta o resto.*
Porte de `TileComponentConfig`, `ConfigInfo`, `DataType`, `ISlotInfo`, `TransmissionType` e
`TileComponentEjector`: por instância, por tipo de transmissão, com as cores, salvo no NBT, `AttachedSideConfig` como padrão.
Tela de lados, Configurator e os slots por `DataType` (`INPUT_1`, `INPUT_2`, `EXTRA`). Substitui `Sides`, `FaceSlots` e o
`machinery_basis/energy` atual.

**Fase 3, rede de transmissores.** *Estado (2026-11-07): aberta. Adiantado: um cabo de energia simplificado (buffer só nos cabos de entrada, ver o inventário); falta o resto.*
Porte de `TransmitterNetworkRegistry`, `OrphanPathFinder`, `DynamicNetwork`,
`DynamicBufferedNetwork`, `Transmitter`, `BufferedTransmitter`, `AcceptorCache`, `NetworkAcceptorCache`, `EnergyNetwork`,
`UniversalCable`, `Target` e `SplitInfo`: buffer somado, save shares, fusão e divisão de rede, chunks, `ConnectionType`, pull,
redstone reativo, upgrade de tier no lugar, `currentScale` e `PacketNetworkScale`. Substitui `network.rs`, `transmit_energy` e o
cache de 20 ticks. A base genérica já precisa servir fluido, químico, item e calor.

**Fase 4, visual do cabo.** *Estado (2026-11-07): aberta. Adiantado: um modelo de cabo em JSON aproximado; falta o resto.*
Modelo original com todas as partes, textura com recorte (não a `opaque/`), `hideContiguousJoin`,
conteúdo `mek_liquid/energy` animado com alfa pela escala, opção `opaqueTransmitters`, forma de seleção e colisão por lado
(`BlockSmallTransmitter`: `SIDES`, `CENTER`, flanges).

**Fase 5, refazer os blocos prontos sobre a base.** *Estado (2026-11-07): em andamento (as frentes de tela, upgrades e abas). Já saiu: as telas das quatro máquinas e do Heat Generator
com os sprites e widgets do original, comparadas em pixel contra o original (`dev/referencia/comparar_lote.py`, 129 estados em guiScale 2 e 3; ver "Tetos dos resíduos"); upgrades (aba
e janela, nas cinco); aba de XP, de energia e de avisos nas máquinas e de energia, de calor e de upgrades no gerador; som das quatro máquinas; luz do bloco aceso (8 e 13); nome com estilo;
o fundo escurecido das telas (injeção `container_dim`); o tooltip da lista de upgrades; os slots de armadura e de mão do gerador (2026-11-07). Falta: as partículas das máquinas e do gerador; as abas de
configuração, segurança, redstone e livro de receitas; o slot de energia funcional e a transferência de calor do gerador; o som do gerador; o conteúdo guardado no item; a remoção
da bateria `mek_teste`; o Steel Casing não foi revisado nesta frente.* Heat Generator, Metallurgic Infuser, Enrichment Chamber, Crusher,
Energized Smelter e Steel Casing, um a um, conferindo o inventário. Telas com os sprites e widgets originais, energia em slot,
upgrades, conteúdo guardado no item, som, partículas e luz. Sai a bateria de teste `mek_teste` do build.

**Fase 6, seguir a cadeia com paridade.** *Estado (2026-11-07): aberta, sem nada feito.* Energy Cube e Energy Tablet, Solar Generator, minérios de estanho, chumbo, urânio e
fluorita, tubos, canos e transportadores sobre a rede da Fase 3.

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

- Faces fixas e sem tela de lados. O ejetor existe, mas só liga por comando (`mekanism_eject`), até a Fase 2.
- `CachedRecipe` portado, mas a tela ainda não mostra erros e avisos. `per_tick_usage` só no Infuser, sem receita dos arquivos que o use.
- Clique do cursor (feito, 2026-11-07). Relato: terra com o cursor entrava no slot de infusão do Infuser, e o clique (e o arrasto) punham qualquer item em
  qualquer slot da máquina. O original usa `InventoryContainerSlot.mayPlace`, que é `isValidForInsertion(item, MANUAL)` do slot. **Medido no
  Java** (`SlotClickTraceTest`, `slot_click_<máquina>.jsonl`, 4 máquinas e o Heat Generator, 15 itens por slot, o slot vazio, com outro item e com o mesmo,
  clique esquerdo, direito, tecla numérica e arrasto; o Infuser em 6 estados de tanque e entrada, 2.880 linhas): o `mayPlace` de um (estado, slot, item) é o
  mesmo com o slot vazio ou cheio e com qualquer clique, e a saída o recusa (`ModResult`). O motor de menu faz o resto, e o do Horizon bate com o do Java.
  **Como:** `menu_slot_rules` v1 (o hook do Horizon, H20) não tem o `Game` nem a posição, e as regras do Infuser olham o tanque e a entrada, então a regra do
  bloco aberto é deixada em `slot_rules` (núcleo) antes de cada clique, e o crate `[api]` da `machinery_basis` responde ao hook com ela. Regra de
  inserção: `rules::slot_accepts` (`runtime.rs`), a mesma do hopper e do shift-click. Em jogo (`horizon2`, `h2_cursor.py`, o menu e os cliques de verdade): 311 casos, 0 diferenças contra as linhas do Java (slot, cursor, barra e slot do arrasto): Enrichment Chamber 56, Crusher 56, Energized Smelter 21 (os cliques do jogador com o slot vazio), Metallurgic Infuser 114 (tanque vazio, bio e fungi, com e sem terra na entrada: a terra deixa de entrar no slot de infusão, e o `bio_fuel` e a `redstone` entram só onde o Java os aceita) e Heat Generator 64 (combustível e energia, com o tanque vazio e cheio de lava, servidor parado). Clique esquerdo, direito, tecla numérica (o cursor sobre o slot e `key 1`) e arrasto para o primeiro slot do inventário, com o slot vazio, com outro item e com o mesmo. O que ficou de fora: baldes empilhados (o Java pôs 3 numa pilha de 1), o clique duplo (`PICKUP_ALL`), o Q e o clique do meio, que o rastro não cobre, e o convidado de LAN (o clique segue o mesmo `lan_machine_click`).
- Feito (2026-11-05 a 2026-11-07): upgrades (aba e janela, 2026-11-06), a aba de XP, a de energia e a de avisos, o controle de redstone e o comparador do tile, o som, o nome com
  estilo, a luz e o tooltip da barra de químico com o nome do químico (`infuser_hover_barra_quimico`, 0 pixel contra o original). **Falta**: as partículas (abaixo); a aba de
  configuração de itens e a de transporte (à esquerda, Fase 2), a de segurança, a de redstone e o livro de receitas (à direita): a cena do original as oculta e o port não as tem.
- Conteúdo solto ao quebrar, em vez de guardado no item.
- Luz do bloco aceso (feito, 2026-11-07): `Attributes.ACTIVE_LIGHT` é 8 nas quatro máquinas (`Machine.java:26`, `AttributeStateActive.java:49-51`) e 0 apagadas; o port herdava 13 do
  gabarito `minecraft:furnace` aceso, porque `properties.light` do Horizon é um inteiro por bloco. A injeção `active_light` troca a emissão dos estados acesos dessas quatro por 8; o
  Heat Generator segue em 13 (`ACTIVE_MELT_LIGHT`). Em jogo, `horizon-ctl light`: máquina acesa 8, vizinhos 7, 6, 5 (a propagação do Java), apagada 0. O mundo atrás da tela com a máquina
  acesa deixou de precisar de um limite à parte (era 16): vale o limite do nível de luz (`LIMITE_MUNDO_POR_LUZ`).
- **Partículas da máquina acesa (aberta, não portada).** `Machine.java:23` registra um `AttributeParticleFX` com duas partículas, `ParticleTypes.SMOKE`
  e `DustParticleOptions.REDSTONE`, ambas em `(rand * 0,6 - 0,3, rand * 6/16, 0,52)` relativo ao centro do bloco (a face da frente, girada pelo `facing`:
  `yRot(PI)` para norte). `BlockTile.animateTick` (`BlockTile.java:80-103`) as solta com `world.addParticle(type, x, y, z, 0, 0, 0)` a cada chamada de
  `animateTick` (o cliente sorteia posições em volta do jogador a cada tick), só com `Attribute.isActive(state)` e `machineEffects` ligado
  (`MekanismConfig.client.machineEffects`). Valem as quatro máquinas, e o Heat Generator tem as dele (`GeneratorsBlockTypes.java:69`: `SMOKE` e `FLAME`, em `z = -0,52`). Nenhuma está no port. O catálogo de hooks do Horizon (pin) não tem `animateTick`; só o helper
  `send_particles` (`native_level_send_particles`, `host_bridge`), que pede a partícula pelo servidor. Um caminho seria o tick do bloco chamar esse helper, o que muda
  quem sorteia (o servidor, não o cliente, e sem `machineEffects`); não decidido. No comparador elas aparecem como `particulas_da_maquina`, diferença conhecida contada à parte
  (`comparar_lote.DIFERENCAS_CONHECIDAS`, `CAIXA_DAS_PARTICULAS`), nunca como resíduo do Horizon.
- Barras desenhadas com retângulo, não com os sprites do Mekanism.
- `bio_fuel` sem tempo de queima. Tags `c:` escritas à mão: conferir com o NeoForge da versão.

**Heat Generator**

- Temperatura do bioma sem ajuste por altura (`Biome.getTemperature(pos, seaLevel)` baixa a temperatura acima de `seaLevel + 17`, com um
  ruído por posição). O rastro mostrou: no mundo do `gameTestServer` o ar do bloco e o de cada vizinho diferem na terceira casa.
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
- Feito (2026-11-07): as abas de energia, de calor (com a unidade de temperatura e as conversões) e de upgrades, o nome com estilo, o som de queima não (abaixo). A tela tem o
  medidor de lava e a barra de energia do original; o painel de texto que havia ali não existe no Java. **Falta**: o slot de energia funcional (o item não converte), a
  transferência de calor pela base (`lastTransferLoss` vale 0), o som (`GeneratorsSounds.HEAT_GENERATOR` não existe no port) e as partículas (`SMOKE` e `FLAME`, abaixo). Os slots de
  armadura e de mão estão portados (abaixo).
- Tanque de lava não recebe de cano.
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
  `machinery_basis_core::screen_elements` (o retângulo de cada elemento e, opcional, uma condição que lê os data slots da tela: ativo e visível agora), separado da barra de armadura (`armor_bar` só guarda os slots,
  e `over_slot` é o "slot sob o mouse" do NeoForge). O Mekanism declara por máquina (`mekanism_rules::screen_elements(ScreenSpec)`, só dados): a aba de energia (`GuiEnergyTab`, -26, 137, 26 por 26, sempre), a de avisos
  (`GuiWarningTab`, -26, 109, 26 por 26, só com algum aviso: `ScreenSpec::view(data).tab()`) e o botão da aba de xp (`GuiInsetElement.isMouseOver` só olha o botão de 18 por 18 em (179, 94), não o suporte de 26 por 26; só com
  o upgrade instalado: o data slot de `hasXpContainment`). O Heat Generator declara a de calor e a de energia pelo mesmo registro, sem mudar de comportamento. A injeção `bar_clicked_outside` é a mesma (sem injeção nova): o
  helper dela agora lê também os data slots da tela aberta (`mb_screen_data`, no módulo `windows`, onde `mod_screen` é visível). **Contra o original:** rastro `fora.jsonl` (o cliente oficial, `CenaTelas` com `fora=1`:
  `hasClickedOutside` em 272 por 182 pontos inteiros ao redor da tela, 18 estados em 3 modos: as quatro máquinas sem e com o upgrade de xp e sem e com aviso, e o Heat Generator parado e com lava), repetido ponto a ponto por
  `mekanism/tests/screen_elements.rs` e `mekanismgenerators/tests/screen_elements.rs` (0 diferenças; o teste falha de verdade: trocar a condição da aba de xp por "sempre" ou deslocar a aba de avisos 1 pixel quebra, medido).
  Em jogo (desktop, container `horizon`, `horizon2/h2_abas.py`, 16 estados, terra no cursor): o clique esquerdo e o direito (que o original também não solta: o elemento está sob o mouse, só `isValidClickButton` falha)
  nas abas de energia, de avisos e de xp mantêm a pilha, e o clique onde a aba não existe naquele estado (avisos sem aviso, xp sem o upgrade), nas bordas do suporte da aba de xp (3 e 5 pixels) e na margem solta, como o rastro.
  Antes (build `native-f319701-4815b98243f2`, Enrichment Chamber e Infuser): o clique nas abas de energia e de avisos soltava a pilha, e o direito na de xp também; o esquerdo na de xp já mantinha, porque `screen_click`
  a trata antes de `clicked_outside`. Prints de antes e depois em `~/.cache/rust-mekanism/wa/abas/`. **Falta:** os elementos que o original tem fora da imagem e o port ainda não desenha (o rastro modo `completo` os mostra,
  todos com botão de 18 por 18): a configuração de itens (-21, 10) e a de transporte (-21, 38) à esquerda, e a segurança (179, 38), o livro de receitas (179, 66) e a redstone (179, 141) à direita. Com um item no cursor, o
  clique neles ainda solta (no original não solta); cada um entra no registro quando a aba for portada. O Heat Generator tem a mesma coisa nas abas de segurança e de redstone, e a de avisos à direita (`GuiHeatGenerator.addWarningTab`,
  invisível nos estados do rastro: o gerador não tem aviso a mostrar). **Observado, não investigado:** o Metallurgic Infuser com item de entrada, sem químico e sem energia não mostra a aba de avisos no port (nenhum erro rastreado chega à tela);
  com 500 de bio ele a mostra. Os rastros `infuser_trace` batem com o Java, então o mais provável é que o original faça o mesmo; confirme antes de tratar como divergência.

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

Quando uma frente achar outro, entra aqui, com a linha do Java e o teste que o prova.

## Pronto significa

Uma parte só sai do inventário quando: o Java foi lido por inteiro, o teste de referência passa, o cenário foi repetido em
mundo, e o que a pessoa vê e ouve foi comparado com o original. Se a referência em NeoForge existir, com o mesmo cenário nos
dois jogos.
