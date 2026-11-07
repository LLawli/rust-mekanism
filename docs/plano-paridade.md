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
`tooltip` e `sim`, que agora tem chunks falsos (`sim/chunks.rs`: níveis de ticket, `toDrop`, carga e descarga na ordem do NeoForge, block
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
  agora a lista sai achatada, na ordem dos filhos (teste e mutação em `recipe.rs`). Não testado em jogo: o Crusher com
  `minecraft:melon_slice` (duas receitas casam, `melon` pela tag e `melon_slice`; a escolha do Java depende da ordem de carga), e o
  hopper de baixo, que não puxou nada da saída nem da entrada em nenhuma das quatro (a configuração de lados do original não foi
  comparada). Faltam também tags `c:` que a datagen do Mekanism gera e o conteúdo não tem (`c:alloys`, `c:circuits`, `c:dusts/bronze`,
  `c:dusts/lead`, os `c:buckets/*`, entre outras): comparar a lista inteira e copiar o que cada receita portada usa.
- Slots de entrada. `InputInventorySlot` passa `ConstantPredicates.notExternal()` como `canExtract` (`InputInventorySlot.java`, o
  construtor protegido chama `super(capacity, notExternal(), insertPredicate, ..)`), e `BasicResourceContainer
  .isCurrentValidForExtraction` aplica o predicado com o `AutomationType` de quem pede: o hopper e o cano (`External`) não tiram a
  entrada, o jogador (`Manual`) e a máquina (`Internal`) tiram. A lib usava `Access::Any` nos slots de entrada da Chamber, do
  Crusher, do Smelter e do Infuser; agora usa `Access::NotExternal`. Teste em `mods/mekanism/tests/input_slot_access.rs`; trocar
  `NotExternal` por `Any` em qualquer dos dois slots o faz falhar (mutação feita, uma de cada vez). Sem rastro do Java: o
  `mekanism-ref` estava fora de uso por regra, então a prova é a leitura do código. Diverge ainda, e não foi corrigido: o slot de
  infusão (`ChemicalInventorySlot.fillOrConvert`) e o slot de energia (`EnergyInventorySlot.fillOrConvert`) extraem com
  `!external || !canFillOrConvert(item)` e inserem com `internal || canFillOrConvert(item)`, e a lib usa `Access::Any` nos dois.
  Sobram `EitherSide`, `Triple` e `Rotary`, sem máquina portada que os use, e os limites de taxa. O `HashSet` das receitas
  complexas do Java itera em ordem de hash, e aqui na da lista, o que só difere com mais de uma receita complexa casando a mesma
  entrada.
- Upgrades: a aba e a janela estão portadas nas quatro máquinas. Em jogo (desktop, 2026-11-06), nas quatro: abrir, instalar (20 ticks, vários de uma vez até o máximo) e remover; saída cheia (64) bloqueia a remoção; saída com outro tipo bloqueia; a lista rola com 8 tipos; o Dump atrás da janela no Infuser não recebe o clique (sem a janela recebe); mundo antigo (sem `component_upgrade` e sem os slots 3 e 4) abre sem perder item; salvar e reabrir devolve os upgrades e os dois slots cheios; jogador que cai com a janela aberta não deixa seleção (o que reentra faz shift-clique normal); `upgradeTicks` anda com a tela fechada e o progresso aparece ao abrir; o convidado vê os tipos (sincronia por rede). Pixel contra o oficial (guiScale 2 e 3, quatro máquinas, sem tolerância e sem máscara além da das abas): zero nos elementos do mod, exceto os ícones de item em escala (H25: "Supported:" e linha da lista), o mundo atrás da janela (H22) e, com o servidor parado, o Dump que o port só aplica no tick seguinte; a ordem da lista com mais de um tipo é de hash por identidade no original, então a comparação vale com um tipo (casos `_um_`). Não testado em jogo: o reenvio dos tipos ao convidado no `/reload`, e Save and Quit com a janela aberta. Faltam o tooltip do item, os efeitos de `muffling` e `xp_containment` (a aba de XP do original aparece com o upgrade de xp_containment e o port não a tem), a receita do `muffling` e os tipos que
  nenhuma máquina portada aceita. Em jogo foram conferidos a receita, o shift-clique, reabrir e a capacidade por instância em
  `native_energy_at` (1000, 600 com 2 de energia, 400 com 4, e a vizinha não muda). A instalação de 20 ticks só foi comparada sem o jogo.
- Upgrade sem tipo aparece vazio, e o original mostra o modelo de item ausente.
- `MekanismContainer`: os avisos da tela estão nas quatro máquinas, comparados lado a lado com o oficial em 17 cenas, e a
  máscara de erros e o redstone vão nos data slots. Falta a lista de trackers do `MekanismContainer`, som, nome, experiência e
  `xpContainmentChanged`.
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
- Ejetor para composteira: `native_container_slots` não expõe composteira, então nada sai para ela. O original sai.
- Identidade do item: `ItemResource` é id e pilha máxima, então itens iguais com dano ou componentes diferentes se confundem.
- Sem rastro ainda: `setRecipeUsed`. `recheckAllErrors` com jogador olhando e `NOT_ENOUGH_ENERGY_REDUCED_RATE` agora têm rastro.
  Este último só existe no Electrolytic Separator, que não está portado: `separator_reduced_rate` é usado só pelo tracker de avisos.
- `players_using` vem do hook `mod_container_viewers`, que não conta espectador. O `PacketTileButtonPress` do Java abre a tela sem
  checar espectador, então esse caminho diverge. O caso de `born` com a tela já aberta não foi exercitado em jogo.
- `sim::chunks` (carga, descarga, blocos, block entities e a ordem dos eventos) existe e é testado, mas ainda não cobre as notificações
  de vizinhança do `setBlock` (`onNeighborBlockChange`), os testes de rede em si são da Fase 3, e a ordem dos eventos veio de ler o
  NeoForge 26.3.0.100, não de um rastro: falta gravar `reloadIntermediary` e `inaccessibleNotUnloaded` no `gameTestServer` e comparar.

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
