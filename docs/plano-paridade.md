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
| Regra de slot no clique | Dá por hook, com ressalva | `menu_slot_rules` não tem `Game`: retrato das receitas em `server_data_loaded`, num `static` do core, com o hook no crate `[api]`. **Não vê o outro slot**, e o Java vê (`TileEntityCombiner`). Completar pilha igual não passa pela regra |
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

**Fase 1, base de tile.** Porte de `TileEntityMekanism`, `TileEntityConfigurableMachine`, `TileEntityRecipeMachine`,
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
  (`insert(output, .., MANUAL) == output`), e a capacidade das quatro máquinas é 20 (recém-posta) ou, recalculada pelos upgrades, no máximo 1.000
  (`upgrade_table.jsonl`, 324 combinações), então a `redstone` entra pelas costas, fica no slot e a energia não sobe, no original (o
  hopper pelas costas do `SlotsTraceTest` deixa `[redstone, 1]` no slot por 45 ticks) e no port; a conversão só acontece com espaço
  para 10.000, que nenhuma máquina portada tem. Em jogo (`horizon2`): o hopper pelas costas da Chamber põe os 8 `redstone` no slot de
  energia e o graveto fica no hopper; um hopper no lado esquerdo põe o `redstone` no slot de entrada (é entrada de enriquecimento, como
  no original); no Infuser o `redstone_block` entra pelas costas e o segundo item (`redstone`) espera, porque o slot guarda um tipo;
  o shift-click de 3 `redstone_block` com o menu aberto soma ao slot de energia (2 para 5). A energia ficou sem subir. Rastros do Java: `chamber_energy_conversion` (recusa
  com 20, um item por tick com espaço, exatamente o espaço que há, o bloco de 90.000, item sem receita), `chamber_energy_conversion_running`
  (a conversão vem antes da receita no mesmo tick) e `infuser_energy_conversion` (energia e infusão no mesmo tick), com a capacidade
  posta por `setMaxEnergy`; o port os repete tick a tick. Sobram `EitherSide`, `Triple` e `Rotary`, sem máquina portada que os use, e os limites de taxa. O `HashSet` das receitas
  complexas do Java itera em ordem de hash, e aqui na da lista, o que só difere com mais de uma receita complexa casando a mesma
  entrada.
- Upgrades: a aba e a janela estão portadas nas quatro máquinas. Em jogo (desktop, 2026-11-06), nas quatro: abrir, instalar (20 ticks, vários de uma vez até o máximo) e remover; saída cheia (64) bloqueia a remoção; saída com outro tipo bloqueia; a lista rola com 8 tipos; o Dump atrás da janela no Infuser não recebe o clique (sem a janela recebe); mundo antigo (sem `component_upgrade` e sem os slots 3 e 4) abre sem perder item; salvar e reabrir devolve os upgrades e os dois slots cheios; jogador que cai com a janela aberta não deixa seleção (o que reentra faz shift-clique normal); `upgradeTicks` anda com a tela fechada e o progresso aparece ao abrir; o convidado vê os tipos (sincronia por rede). Pixel contra o oficial (guiScale 2 e 3, quatro máquinas, sem tolerância e sem máscara além da das abas): zero nos elementos do mod, exceto os ícones de item em escala (H25: "Supported:" e linha da lista), o mundo atrás da janela (H22) e, com o servidor parado, o Dump que o port só aplica no tick seguinte; a ordem da lista com mais de um tipo é de hash por identidade no original, então a comparação vale com um tipo (casos `_um_`). Não testado em jogo: o reenvio dos tipos ao convidado no `/reload`, e Save and Quit com a janela aberta. Estão portados e conferidos contra rastros do original (2026-11-06): o tooltip do item (`upgrade_tooltip`, os 8 tipos, linha a linha), a receita e a tag `muffling_center` do `muffling` e o efeito dele no som (`chamber_muffling`), e o `xp_containment` (`chamber_xp_containment`, `chamber_xp_vanilla`, `chamber_xp_broken`, repetidos por `effects_trace.rs`). Falta a aba de XP do original (o botão `COLLECT_XP`: a regra `collect_xp` existe e é testada, e o port ainda não a desenha) e os tipos que
  nenhuma máquina portada aceita. Em jogo foram conferidos a receita, o shift-clique, reabrir e a capacidade por instância em
  `native_energy_at` (1000, 600 com 2 de energia, 400 com 4, e a vizinha não muda). A instalação de 20 ticks só foi comparada sem o jogo.
- Upgrade sem tipo: o `select` do modelo já cai em `ItemModelNode::Select { fallback: None }`, que o Horizon resolve com `LayerContent::Missing`, o `missingItemModel` do original. O que aparece vazio é o slot do inventário: em jogo (2026-11-06) o upgrade sem componente some da barra de atalhos, tanto com esse caminho quanto com um `Model` explícito de `minecraft:builtin/missing`. É do desenho de ícone do Horizon (a camada `Missing` não sai no slot) e fica registrado como pedido ao renderer; o código do mod não muda.
- Som da máquina (`TileEntityMekanism.updateSound`, `SoundHandler.TileTickableSound`): portado nas quatro máquinas (módulo `tile-sound`, H19). Em jogo (2026-11-06, Enrichment Chamber), o `status` do jogo mostra `mekanism:tile.machine.enrichment_chamber` tocando e o stderr diz `start`, `volume 1 -> 0` com o upgrade de silêncio no máximo, e `stop` ao cortar a energia. O que ainda difere: `baseSoundVolume` e `enableMachineSounds` valem os padrões (1 e ligado), porque o port não tem arquivo de configuração de cliente; a distância usa a câmera do último quadro e não os pés do jogador; o som de Natal usa o fuso do jogador, como `LocalDate.now()`; o convidado de LAN recebe a contagem de silêncio por pacote, mas isso não foi testado com convidado. O silêncio vai de 1 (sem upgrade) a 0 (máximo 1: o `muffling.json` do original diz `max` 1, então não há 2 nem 4).
- Experiência (`TileEntityRecipeMachine`, `RecipeCacheLookupMonitor`): `recipesUsed`, a última receita, `hadContainment` (`has_containment`) e `setRecipeUsed` têm rastro. `awardUsedRecipesAndPopExperience` roda ao tirar o resultado (injeção H27: o Horizon não avisa a retirada em slot de resultado de mod), com a experiência de cada receita lida do JSON (`experience`) ou do cozimento do jogo. Em jogo, 2 carvões com `xp_containment` e a retirada pelo clique deram 2 pontos ao jogador (4 operações vezes 0,5) e limparam `recipes_used`. Quebrar a máquina solta a experiência guardada (`destroy_block`, só o jogador; explosão, pistão e `/setblock` não passam por ele), e tirar o upgrade a perde, como o original (`hasXpContainment` já é falso dentro do `recalculateUpgrades`). A parte fracionária usa `Math.random` no Java e uma fração de nanossegundos aqui: sem semente a reproduzir.
- `MekanismContainer`: os avisos da tela estão nas quatro máquinas, comparados lado a lado com o oficial em 17 cenas, e a
  máscara de erros e o redstone vão nos data slots. Falta a lista de trackers do `MekanismContainer` e a aba de XP. O nome (`getName`, `getDisplayName`, `CustomName`) está portado: o item o dá ao tile ao colocar, o tile o dá ao item solto, e o servidor o manda ao cliente ao abrir a tela (`name_trace.rs` contra `chamber_name`); só o texto puro e o campo `text` de um componente com estilo são lidos, sem cor nem negrito.
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
  - abas laterais: duas à esquerda e três à direita, energia embaixo à esquerda, redstone embaixo à direita e o aviso dela,
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
  (as oito cenas das máquinas, graveto sobre o combustível ocupado e sobre o vazio, carvão e balde de lava sobre o combustível vazio e o
  tablet de energia, que não se repete porque o item não é portado), repetido por `identity_trace.rs` e em jogo.
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

**Fase 2, lados coloridos.** Porte de `TileComponentConfig`, `ConfigInfo`, `DataType`, `ISlotInfo`, `TransmissionType` e
`TileComponentEjector`: por instância, por tipo de transmissão, com as cores, salvo no NBT, `AttachedSideConfig` como padrão.
Tela de lados, Configurator e os slots por `DataType` (`INPUT_1`, `INPUT_2`, `EXTRA`). Substitui `Sides`, `FaceSlots` e o
`machinery_basis/energy` atual.

**Fase 3, rede de transmissores.** Porte de `TransmitterNetworkRegistry`, `OrphanPathFinder`, `DynamicNetwork`,
`DynamicBufferedNetwork`, `Transmitter`, `BufferedTransmitter`, `AcceptorCache`, `NetworkAcceptorCache`, `EnergyNetwork`,
`UniversalCable`, `Target` e `SplitInfo`: buffer somado, save shares, fusão e divisão de rede, chunks, `ConnectionType`, pull,
redstone reativo, upgrade de tier no lugar, `currentScale` e `PacketNetworkScale`. Substitui `network.rs`, `transmit_energy` e o
cache de 20 ticks. A base genérica já precisa servir fluido, químico, item e calor.

**Fase 4, visual do cabo.** Modelo original com todas as partes, textura com recorte (não a `opaque/`), `hideContiguousJoin`,
conteúdo `mek_liquid/energy` animado com alfa pela escala, opção `opaqueTransmitters`, forma de seleção e colisão por lado
(`BlockSmallTransmitter`: `SIDES`, `CENTER`, flanges).

**Fase 5, refazer os blocos prontos sobre a base.** Heat Generator, Metallurgic Infuser, Enrichment Chamber, Crusher,
Energized Smelter e Steel Casing, um a um, conferindo o inventário. Telas com os sprites e widgets originais, energia em slot,
upgrades, conteúdo guardado no item, som, partículas e luz. Sai a bateria de teste `mek_teste` do build.

**Fase 6, seguir a cadeia com paridade.** Energy Cube e Energy Tablet, Solar Generator, minérios de estanho, chumbo, urânio e
fluorita, tubos, canos e transportadores sobre a rede da Fase 3.

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
- Clique põe qualquer item na entrada. Slot de energia sem função.
- Sem upgrades, controle de redstone, comparador, som, partículas, abas, nome do químico no tooltip.
- Luz 13 em vez de 8. Conteúdo solto ao quebrar, em vez de guardado no item.
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
  `generator_lava_cheia` e `generator_hover_barra` (energia 120) a barra tem 0 pixel de diferença em escala 2 e 3. Os números antigos vinham
  de uma cena em que o gerador queimava lava durante o print, com a energia subindo 1 a 3 por tick, então o que mudava era quantos
  ticks passavam entre montar o estado e o print, não a regra. Sobra o medidor de lava (fase da animação em prints únicos).
- Lava dentro do bloco desenhada pela block entity; entidade ali dentro não conta como dentro de lava.
- Sem slot de energia funcional, transferência de calor pela base, som, partículas e abas (energia, calor e a que guarda os slots de
  armadura). A tela tem o medidor de lava e a barra de energia do original; o painel de texto que havia ali não existe no Java.
- Tanque de lava não recebe de cano.

## Pronto significa

Uma parte só sai do inventário quando: o Java foi lido por inteiro, o teste de referência passa, o cenário foi repetido em
mundo, e o que a pessoa vê e ouve foi comparado com o original. Se a referência em NeoForge existir, com o mesmo cenário nos
dois jogos.
