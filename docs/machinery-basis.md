# machinery_basis: escopo

Biblioteca genérica para mods de máquina no Horizon. Nada nela é exclusivo do Mekanism: ela não conhece hidrogênio nem
Metallurgic Infuser. Um mod de magia usa os mesmos tanques para mana, e o Mekanism registra químicos como dados.

Levantado em 2026-11-05. Pin: Nightly `f319701` (API 409), a primeira com `native_container_accepts` e
`native_menu_extra_data`. O mod de teste compilou nela sem mudanças. A pesquisa em `main` foi feita no commit `85d8e28`.

## Regra de escopo

Entra na lib o que atende às três condições:

1. Dois mods sem relação entre si precisariam disso.
2. O Horizon não oferece, ou oferece com um limite que a lib resolve.
3. Um consumidor real vai usar agora.

A terceira segura o motor de multibloco: ele é parte do plano, mas só nasce com a primeira estrutura real.

Na primeira condição, código que vários blocos do Mekanism usam conta a favor: o que se repete dentro do mod provavelmente
serve a outros mods.

## Decisão por item

**Usar como está:** a lib não embrulha, e o mod chama o Horizon direto.

| Item | No Horizon | Observação |
|---|---|---|
| Registrar blocos, itens e menus | Hooks `blocks`, `items`; `data/<id>/horizon/menu/` | |
| Block entity, tick e salvamento | `block_entity`, `block_entity_ticks`, `block_entity_tick` | Salvamento testado em mundo |
| Slots de item e funis | `ModContainer`, `native_insert_item`, `native_extract_item`, `native_move_items` | Interopera com hoppers |
| Buffer de energia | `ModEnergyBuffer`, `native_energy_insert` | Testado em mundo. `i32` cobre o maior cubo não criativo (256.000.000) |
| Energia em itens | `item_energy_buffer` | Para o `energy_tablet` |
| Tanques de fluido | `ModFluidTanks`, `native_move_fluids` | Até 16 tanques de 1.000.000 mB; o Fluid Tank Ultimate tem 256.000 |
| Fluido em itens | `native_item_fluid_tank` | |
| Estado do bloco | `state_properties`, `native_set_block_property` | `facing`, `active` |
| Redstone | `neighbor_signal`, `native_update_signal_neighbors` | |
| Manter chunk carregado | `native_force_chunk` | Tickets não são salvos: guarde e reaplique |
| Rede cliente e servidor | `native_net_send_to_player`, `native_net_send_to_server` | |

**Embrulhar:** existe, mas tem uma armadilha que todo mod repetiria.

| Item | Problema | A lib oferece |
|---|---|---|
| Distribuir energia | O bloco em tick sai do mundo: `native_energy` responde `None` e `native_move_energy` não esvazia o próprio bloco | `distribuir_energia(game, pos, entity, faces)` |
| Data slots | Trafegam em 16 bits | Codec que divide `i32` e `i64` em slots |
| Receitas de tipo próprio | `native_recipes_of_type` entrega JSON cru, sem busca | Índice por tipo, refeito depois de `server_data_loaded`. O mod diz em quais campos cada parte fica |
| Telas | `native_screen` e `native_sprite` são primitivas, e o Horizon só lê o `gui.scaling` dos sprites do vanilla | Painel e molduras de nove partes pelo `.mcmeta` do mod, mosaico com âncora e corte, barra e medidor de tanque, seta de progresso, slots pelo papel que a configuração de lados dá, rótulos, tooltip de widget |
| Faces relativas | `faces` são absolutas e por tipo de bloco, e a máquina gira | Conversão de lado relativo para direção, pelo `facing` |

**Reimplementar:** o Horizon não tem, ou não serve para a meta.

| Item | Por quê | Tamanho no Mekanism |
|---|---|---|
| Recursos tipados e seus tanques | Químico como fluido não dá paridade: o Chemical Tank Elite tem 1.024.000 mB e o Ultimate 8.192.000, acima do limite de 1.000.000. Um mod de fluidos também trataria hidrogênio como líquido | 1.519 linhas (`api/chemical`) |
| Lados configuráveis por máquina | Faces do Horizon são por tipo de bloco | Parte de 2.449 (`tile/component`) |
| Ejetor | Empurra saídas conforme a configuração de lados | Parte de 2.449 (`tile/component`) |
| Base de máquina | Progresso, operação e consumo por tick; falta o controle de redstone | 1.589 (`tile/prefab`) |
| Redes de transporte | `connects_to` só dá a forma do cabo, não a rede. A de energia está pronta; faltam fluido, químico e item | 5.172 (`lib/transmitter`, `content/network`) |
| Motor de multibloco | Não existe | 2.614 (`lib/multiblock`) |

### Lados configuráveis

A máquina declara `inputs` e `outputs` vazios, então hopper nenhum a enxerga. Quem age é ela, imitando o hopper e
respeitando as faces configuradas. Assim a regra vale por instância e acompanha a rotação.

| Situação | O que a máquina faz |
|---|---|
| Hopper logo abaixo, com a face de baixo como saída | Entrega um item, mesmo com o push desligado, porque um hopper puxa do bloco acima |
| Hopper em outra face | Não entrega: hopper só puxa de cima |
| Hopper apontando para a máquina, com aquela face como entrada | Recebe um item, como se ele entregasse |
| Hopper apontando para outro lugar | Ignora |
| Hopper travado por redstone (`enabled=false`) | Ignora |

O ritmo é o do hopper: 1 item a cada 8 ticks por elo, e só um sucesso espera. O push ligado na configuração é outra coisa:
entrega ativa a qualquer inventário que aceite entrada, pelas faces de saída, hopper incluído, como um dropper atirando
num hopper. O ritmo é o do ejetor do Mekanism: dispara a cada 11 ticks, tenha saído algo ou não.

Limites: outros mods que automatizam pelas faces do Horizon também não enxergam a máquina, e fluido ou energia vindos
deles seguem as faces fixas do tipo de bloco. Hopper minecart ainda não entra na regra. O elo tem contador próprio, sem
sincronizar com o cooldown interno do hopper, que os helpers não expõem.

### Fluido dentro do bloco

Um bloco de qualquer mod guarda lava declarando `fluid_logged` nos `state_properties`, com os valores `empty` e `lava`.
A lib cuida do balde, do fluxo, da luz e do desenho.

| Parte | Como |
|---|---|
| Fonte de lava real | Injeção em `set_state_properties`, que escreve lava na linha do estado na tabela do motor de fluidos. O Horizon só reconhece fluido em bloco para água. É um alvo interno, preso à versão do jogo |
| Balde | Hooks `use_item_on` e `use_item`: balde de lava enche, balde vazio tira. Um bloco com tela só recebe o balde com shift |
| Fluxo | Hook `update_shape`: um vizinho mudou, a lava olha de novo para onde correr |
| Luz | Nível 15 no estado com lava |
| Desenho | A block entity desenha a lava (hook `block_entity_submit`), e o bloco precisa responder `block_entity_update_tag` |

A lava respeita a forma do bloco, como no vanilla: não passa por uma face toda fechada. O Heat Generator tem a base sólida,
então a lava sai pelas laterais e nunca por baixo. O atraso do fluxo é o do Overworld, 30 ticks, também no Nether.

Limites: o mesher, a física de entidades e a colisão de fluido leem `FluidState::from_block`, que nenhuma injeção alcança
na versão fixada, porque o corpo usa `?` e o retorno usa `Self`. Por isso um bloco sem block entity flui mas não mostra a
lava, e uma entidade dentro do bloco não conta como dentro de lava. Um estado com lava posto por comando ou pela geração do
mundo só corre quando um vizinho muda, porque só o balde agenda o fluxo.

### Rede de energia

Não existe objeto de rede. Um transmissor é um bloco com block entity e `ModEnergyBuffer`, e quem gera entrega nele como numa
máquina, o que vale para qualquer mod. Quem entra na rede se declara em `data/<ns>/machinery_basis/energy/<bloco>.json`:
`{"transmitter": true}`, ou `inputs` e `outputs` com os lados relativos ao `facing`, como `{"inputs": "all"}` ou
`{"inputs": ["back"], "outputs": ["front"]}`. Os hooks `state_for_placement` e `update_shape` decidem as ligações por esses
dados, no lugar da regra de tag do `PipeBlock`: qualquer um dos dois tipos liga o cabo, e só `inputs` recebe da rede. As faces
do `ModEnergyBuffer` continuam absolutas e por tipo de bloco, então quem usa o Horizon direto, sem a lib, não vê essa regra. O transmissor com energia percorre as ligações (as propriedades de lado do bloco),
pergunta quanto cada consumidor aceita e divide como o `SplitInfo` do Mekanism: quem pede menos leva o que pediu, e o resto da
divisão vai aos primeiros. Por isso persistência e chunk descarregado saem de graça, e um transmissor vazio só custa o tick.

| Medida, perfil `performance` | Tempo |
|---|---|
| Travessia de 4 cabos | 6 a 17 µs |
| Travessia de 1.004 cabos, ligados nas seis faces | 1 a 3 ms |
| A mesma rede com a travessia guardada, por tick | Cerca de 2 µs, e 1 a 3 ms a cada 20 ticks |

Sem guardar, dez geradores numa rede de mil cabos custariam até 30 ms dos 50 ms de um tick. A travessia dura 20 ticks, então
uma mudança leva até um segundo para valer, e a energia só espera no transmissor.

## Dados de um mod que já tem formato

A lib lê `machinery_basis/resource`, `machinery_basis/upgrade` e as tags deles, no formato dela, para quem não tem formato próprio. Um mod
que já tem pasta e formato lê os dele e entrega o resultado como outra fonte (`resource::set_source`, `set_tags_source`,
`upgrade::set_types_source`, `set_tags_source`), e a lib junta tudo. Cada fonte troca só a própria parte num `/reload`. O Mekanism lê
`mekanism/chemical`, `mekanism/upgrade` e as tags deles, com os arquivos do original, para um datapack feito para o Mekanism valer aqui.
`machinery_basis/energy` e `horizon/` continuam no port porque não têm equivalente: no original isso é código.

## Componente de item nos modelos e nos comandos

O Horizon só conhece os componentes vanilla nos dois lugares, e o Mekanism injeta o seu. `component_item_model.rs` entra em
`foreign::decode_node` e transforma `mekanism:component_item` num `select` sobre o componente, com o `items/upgrade.json` igual ao do
original. `command_components.rs` entra em `ItemState::component_type`, o parser do argumento de item, e aceita `mekanism:upgrade_type`.
Um alvo com duas lifetimes se escreve `ItemState<'_,'_>`, sem espaço. O mixin recusa `RETURN` em função que devolve `Self` ou que tem função
aninhada, e uma ligação só pode ser parâmetro ou `self` inteiro.

## Regra de slot do vizinho

`slot_accepts.rs` é um módulo injetado em `game/machines/native_items.rs`, porque as regras de slot são privadas ali. Ele só lê: expõe
`machinery_slot_accepts`, a parte de `move_in_item` que decide se o item entra, sem escrever nem mexer no cooldown do hopper. Módulo de
mixin não aceita `//!` no topo, que conta como atributo de arquivo.

## Regra de slot do clique

`slot_rules` (núcleo) é a ponte entre quem vê o jogo e o hook `menu_slot_rules` do Horizon, que roda dentro do menu sem o `Game` nem a posição do
bloco: o mod deixa ali, antes de cada clique de menu de máquina, a regra do bloco aberto (`set(dono, menu, regra)`, com a regra
`Fn(slot, id do item) -> bool`, já com o estado do bloco), e o crate `[api]` da `machinery_basis` (`api/src/lib.rs`, `Hooks::menu_slot_rules`) a consulta
(`ask`), só para tirar (`may_place &= regra`; o slot de resultado que o tipo de menu já recusa continua recusando). Uma regra por dono, para o Mekanism e o
Generators não se apagarem, e nenhum id do Mekanism: o menu e o item chegam como texto. A cópia do núcleo é uma só no processo, porque o `[api]` o declara
por caminho (`native-crates/machinery_basis/core`), o mesmo `[[crate]]` dos módulos de glue; um `thread_local` vale porque o hook responde na thread do clique.

## Objetos vivos das máquinas

A base de máquina é uma cópia literal do Java: `Transaction` com gerenciador por thread, contêineres (`EnergyContainer`,
`InventorySlot`) como `Rc` que se registram sozinhos na transação, listeners como closures que disparam no commit da raiz, e
`CachedRecipe` guardando as closures do tile. O ejetor de itens (`ItemEjector`) abre uma raiz por envio, cobrindo a saída da
máquina e o inventário do vizinho.

No Horizon o objeto de uma máquina é um cache da `ModBlockEntity`. Mora numa tabela por thread, chave (dimensão, posição), em
`mods/mekanism/native/glue.rs`. Nasce no primeiro tick depois de uma lacuna, refeito dos itens, da energia e do progresso que a
`ModBlockEntity` guarda; todo tick lê dela o que mudou fora (tela, hopper, cabo) e escreve o que mudou; é varrido depois de
100 ticks sem andar. Salvar e reabrir o mundo funciona porque a `ModBlockEntity` é a verdade.

A capacidade de energia de uma máquina de progresso é a única coisa do contêiner que o Java não grava (`currentMaxEnergy`): o construtor dá 4 ticks de
consumo, `recalculateUpgrades` dá os ticks da receita, e o tile lido do disco só recalcula se o filho `component_upgrade` está no NBT. Por isso a
`ProgressMachine` tem três portas, e o glue escolhe a certa: `restore` (o `loadAdditional`, recalcula só com o filho), `apply_item_components` (o item que
pôs a máquina, recalcula sempre) e nenhuma (o `setBlock`); e `capacity_recalculated` leva a diferença de um objeto vivo para o seguinte quando o glue refaz o objeto sem o
tile ter mudado.

### Máquina com tanque

A `ProgressMachine` (`tile.rs`) é a base das quatro máquinas de progresso: a Enrichment Chamber, o Crusher e o Energized Smelter
(`ProgressMachine::electric`) e o Metallurgic Infuser (`ProgressMachine::chemical`). As duas compartilham o tile, a energia, o
slot de saída, o slot de energia, o ejetor, o monitor de receita e o `CachedRecipe`; a segunda acrescenta o `ChemicalTank`, o slot
que o enche e a receita de duas entradas. Cada máquina escolhe só o que o `TileEntity` dela escolhe: os erros de cada handler
(no Infuser o item usa `NOT_ENOUGH_SECONDARY_INPUT` e o químico `NOT_ENOUGH_INPUT`), o `setBaselineMaxOperations` com
`getOperationsPerTick` e quem escuta cada contêiner (o tanque e a entrada usam o `recipeCacheListener`, o slot de infusão e o
de energia só marcam para salvar, a saída despausa).

| Peça da lib | Java |
|---|---|
| `resource_tank::ResourceTank` | `BasicResourceContainer`, com diário, `canInsert`, `canExtract` e validador, para qualquer recurso grande |
| `chemical::ChemicalTank`, `fluid::FluidTank` | `BasicChemicalTank` e `BasicFluidTank`: o mesmo contêiner, para químico e para fluido |
| `chemical::ChemicalAttributeValidator` | `ChemicalAttributeValidator` (`DEFAULT`, `ALWAYS_ALLOW`, `create`, `createStrict` e o `process` sobrescrito) |
| `chemical::fill_tank_or_convert`, `fill_tank_from_slot` | `ChemicalInventorySlot.fillTankOrConvert` e `fillTankFromSlot` |
| `energy_conversion::EnergyConversion`, `fill_container_or_convert` | `ItemStackToEnergyRecipe` (o índice `SingleItem` do tipo `energy_conversion`) e `EnergyInventorySlot.fillContainerOrConvert`, sem a parte de item com capability de energia, que nenhum item portado tem |
| `resource_tank::fill_container_from_slot` | `ResourceHandlerSlot.fillContainerFromSlot`, sobre a capability de recurso do item |
| `recipe_handlers` | `InputResourceHandler`, `IOutputHandler` e o `OutputHelper` de slot, de tanque de químico e de tanque de fluido |
| `two_input_recipe` | `TwoInputCachedRecipe` e `CachedRecipeHelper.twoInputCalculateOperationsThisTick` |
| `recipe_cache` | `InputRecipeCache` (`SingleInputRecipeCache`, `DoubleInputRecipeCache`) e os `IInputCache` por tipo, componentes e químico |
| `item_chemical_constant_recipe` | `ItemStackConstantChemicalToObjectCachedRecipe` e `ChemicalUsageMultiplier`, com o `usedSoFar` |

A lib não conhece id nenhum: o ingrediente de item e o de químico são predicados com a forma que o índice lê, e as conversões e as
receitas chegam como dados. O tanque do Infuser só aceita um químico que alguma receita usa (`containsRecipeB`) e, com item na entrada, que combine
com ele (`containsRecipeBA`).

O índice de receitas (`recipe_cache`) é o `getInputCache` do tipo de receita: um por tipo, que todas as máquinas do tipo e as regras de
inserção compartilham, montado na primeira pergunta depois de um `clear` (`ItemToItemRecipe::cache`, `ItemChemicalToItemRecipe::cache`,
`ChemicalConversion::cache`). A receita guarda um `ItemIngredient` ou `ChemicalIngredient`, que embrulham o `ShapedIngredient`: o teste
de tipo do jogo e a `IngredientShape` que o índice lê (o que o `mapIngredient` do Java faz com o ingrediente). O mod entrega a forma:
`ResourceInput::shaped` faz a do químico, com os membros da tag (`Tags::members`) como estavam na carga de dados. A máquina procura a
receita no índice (`findFirstRecipe`, quantidade incluída), o slot de entrada da Chamber tem `containsRecipe` como validador, o do
Infuser `containsRecipeA` e `containsRecipeAB`, e o tanque `containsRecipeB` e `containsRecipeBA`. O slot e o tanque se perguntam um
pelo outro; o tanque segura o slot por `WeakInventorySlot`, para os dois não se segurarem.

Limites conhecidos: a receita de uso constante (`per_tick_usage`) é a que o Infuser cria quando a receita a pede, com o `usedSoFar` do
tile (`getSavedUsedSoFar`). Os arquivos de infusão do original não a usam, e o rastro dela é o do Osmium Compressor. O
`fillTankFromSlot` e o `ChemicalAttributeValidator` existem, mas nenhum item portado tem a capability de químico (o tile entrega
`ChemicalItems`) e nenhum mapa de atributos é lido (o mod entrega `ChemicalAttributes`); sem eles o comportamento é o de antes.
O `FluidResource` é só o id, sem o `DataComponentPatch` do NeoForge.

### Upgrades

`upgrade.rs` porta o `TileComponentUpgrade`, os dois `UpgradeInventorySlot` e o que o `Upgrade` e o `IUpgradeHelper` dão à máquina,
sem conhecer upgrade nenhum. O mod entrega, como dados, os tipos com o máximo de cada um
(`data/mekanism/mekanism/upgrade/<nome>.json`, o mesmo arquivo do `Upgrade.DIRECT_CODEC`), as tags de suporte
(`data/mekanism/tags/mekanism/upgrade/...`, com `#` para tag dentro de tag) e o item que os carrega. Quem não tem formato
próprio usa `data/<ns>/machinery_basis/upgrade/<nome>.json` e `data/<ns>/tags/machinery_basis/upgrade/...`, com o mesmo formato.
O item é um `ItemResource` com `variant`, o valor que distingue itens do mesmo id, como o
`DataComponentPatch` do `ItemResource` do NeoForge: no Mekanism, o valor do componente `mekanism:upgrade_type`. Os ids de velocidade e de
energia, que mexem no tempo e no consumo, entram em `tile::UpgradeConfig` junto do `maxUpgradeMultiplier`.

| Peça da lib | Java |
|---|---|
| `UpgradeComponent::tick_server` | `TileComponentUpgrade.tickServer`: 20 ticks contados, a instalação no 21º, `canCheckUpgrades` |
| `add_upgrades`, `remove_upgrade`, `get_upgrades`, `scaled_upgrade_progress` | os métodos de mesmo nome |
| `UpgradeComponent::saved` e `restore` | `serialize` e `deserialize`, que recalcula cada tipo da tag antes de carregar os slots |
| slot de entrada | `UpgradeInventorySlot.input`: `notExternal` para tirar, o tipo suportado para pôr, `isValid` pelo componente |
| slot de saída | `UpgradeInventorySlot.output`: só `manualOnly` tira, só `internalOnly` põe |
| `tile::recalculate_upgrades` | `recalculateUpgrades` do `TileEntityProgressMachine` e do `TileEntityMekanism`, com `updateEnergyPerTick` e `updateMaxEnergy` |

O `tick_server` da `ProgressMachine` chama o componente primeiro, antes do atraso do estado ativo, como o `TileEntityMekanism.tickServer`.
O `InventorySlot` ganhou `SlotChecks` (o `validator` e o item no `canInsert`), `Access::ManualOnly` e a capacidade zero para o item que o
`isValid` recusa.

No jogo, o componente vive no NBT como o Java o grava: o filho `component_upgrade`, com `upgrades` (tipo para total) e `items` (os dois
slots, `{container, stored: {type, amount}}`). O glue nunca escreve o componente se nada mudou, e a mudança chega ao NBT na hora quando é um
clique. A capacidade de energia de uma máquina muda com os upgrades, mas o `ModEnergyBuffer` do Horizon é um só por bloco e em `i32`:
`energy_capacity.rs` é uma injeção cancelável em `native_energy_at` que responde com a capacidade da instância, guardada numa tabela por
posição e por id de bloco. O que passa de `i32::MAX` fica em `i32::MAX` e vai para o log.

A tela dos upgrades usa estes slots assim: dois slots a mais no `ModContainer`, depois dos da máquina, ligados aos slots do componente
como o glue liga a entrada e a saída (`slot_from_entity` antes do tick, `slot_to_entity` depois), e a aba lê o progresso e os totais por
o espelho do menu (`UpgradeWindowSpec`: só dados, e `Game::mb_window_register` com as funções que o mod dá, tirar um upgrade, pôr da mão e a fonte dos dados do menu).
O `ItemUpgrade.useOn` e a limpeza da sessão de quem sai do menu são da lib e valem para todo bloco registrado.

Um tile sem objeto vivo (o Heat Generator, que lê e grava tudo no NBT a cada tick) refaz o componente a cada tick com `upgrade::rebuild`: o
NBT (`deserialize`), o que a memória levava (`UpgradeTransient`: `upgradeTicks` e `canCheckUpgrades`, que o Java não salva) e os dois slots
do menu, que mandam sobre o NBT. O glue lê e grava o NBT com `read_upgrades` e `write_upgrades`, e a memória fica numa tabela por thread.

### Dados sincronizados do menu

`container_sync.rs` porta o `MekanismContainer.trackedData` e as classes `Syncable*` sem conhecer campo nenhum: um dado é uma fonte
(`u16`, que quem usa entende) e uma classe (`Kind`: `Boolean`, `Int`, `Long`, `Double`, `Enum`, `LargeResourceStack`, `StreamCodec`). O
`ContainerSync` é o menu de um jogador: guarda o último valor de cada dado e, a cada `broadcast_changes`, devolve só os sujos com a regra
da classe (`!=`, a pilha que distingue o tipo do tamanho, o hash do codec). `send_all` é o `sendAllDataToRemote`, e `start_tracking_server`
e `stop_tracking` são os da lista específica da janela de upgrades (reaproveitam o `Tracking` de `window_session`, inclusive o vazamento
de pedir de novo sem parar). O valor vivo vem de um `TrackedSource`.

`tile_trackers.rs` tem a ordem que as classes de tile do Mekanism dão à lista (`TileEntityMekanism`, `TileEntityRecipeMachine`,
`TileEntityProgressMachine`, a janela de upgrades) com as fontes `source::*`; as máquinas acrescentam as delas a partir de `source::MOD`, e
`position_of` acha a posição de uma fonte na lista (o cliente e o servidor combinam posições só por ela). `machine_sync.rs` lê esses valores de
uma `ProgressMachine` e lê de volta o mapa de upgrades (`read_upgrade_map`), e `last_energy.rs` é o `LastEnergyTracker` (o que a capability de
energia entregou no tick anterior).

O transporte (2026-11-08) é o do original, um pacote por jogador. No servidor, `window_session::Sessions` guarda um `ContainerSync` por (contêiner,
jogador): a sessão nasce quando o jogador abre o menu (`Game::mb_container_open`, que manda tudo) e é largada quando ele deixa de ter o menu
daquele bloco (`mb_container_recount`); `mb_container_broadcast`, no começo do tick do servidor, lê o valor vivo que o dono do bloco entrega
(`SourceFn`, dada em `mb_window_register`) e manda a cada sessão só o sujo; o número de sessões de um bloco é o `playersUsing` do tile
(`mb_container_players`, espectador incluso). No cliente, `container_sync::ContainerMirror` (`Mirror`) é o `trackedData` do cliente: um
espelho por posição, que entrega cada `Property` à posição dela pela classe do dado (`apply`, o `handleWindowProperty`) e que a tela lê
(`boolean`, `int`, `long`, `double`, `enum_ordinal`, `stack`, `bytes`; `Game::mb_mirror(bloco)`). O pacote (`container_sync::wire`) viaja no canal
`machinery_basis:container`; o registro de recursos (cor e sprite dos químicos, que o cliente resolve pelo id) no canal `machinery_basis:resources`.

### Unidade de temperatura

`temperature.rs` porta o `UnitDisplayUtils.TemperatureUnit` (K, C, R, F e STP, nessa ordem, com `convertFromK`, `convertToK`, `getNext` e
`getPrevious`), o `getDisplayBase` que escreve o número com a medida (de `f` a `Y`, em `units.rs`) e o `MekanismUtils.getTemperatureDisplay`.
A unidade que o cliente escolhe é o `MekanismConfig.local.tempUnit`: `LocalUnit` a guarda em `config/<ns>/local.toml` (a chave
`temperatureUnit`, o nome do enum), relê o arquivo quando a data dele muda (no máximo uma olhada por segundo, porque o desenho pergunta
a cada quadro) e grava o clique sem tocar nas outras chaves. O mod escolhe o namespace, os sprites (`tab/heat_info_<getTabName>`) e os
textos de idioma; o clique que troca a unidade usa `screen_mouse_button_pressed_pre`, que toma o clique sem o som nem o pacote do
`screen_click`. Um `double` que a tela precisa (o calor e as duas perdas do Heat Generator) viaja inteiro, como um `SyncableDouble`.

### Barra de armadura e mão

`armor_slots.rs` porta `MekanismContainer.addArmorSlots`, `ArmorSlot` e `OffhandSlot` (a coluna de quatro slots com a mão logo abaixo, `mayPlace`, tamanho, `mayPickup` e o `insert`),
e `armor_bar.rs` guarda, por tipo de menu, o que o mod declarou de `ContainerBuilder.armorSideBar(x, y, offhandOffset)`. A declaração é uma só por processo, porque o servidor (que monta o menu com os cinco slots depois dos 36 do inventário) e o cliente (que os põe na tela, nos ids 500 a 504 que o
jogo já usa para a armadura e a mão) rodam em threads diferentes. Os slots são reais, do jogador, e o motor de menus do Horizon os trata (`Role::Armor`: só a peça do slot, uma por vez, a maldição do
vínculo, o `onEquipItem` do clique). A lib faz o que o motor não faz: a lista de slots e a posição na tela do mod, o mapa dos índices, o "clicar fora" que respeita os slots da barra (`armor_bar::over_slot`) e as abas
(`screen_elements`, abaixo; `GuiMekanism.hasClickedOutside`), o shift-clique que parte da armadura ou da mão (com o `mayPickup`) e o envio da armadura e da mão com a resposta do clique. Tudo isso é injeção crua, e está em
[injecoes-horizon.md](injecoes-horizon.md#a-barra-de-armadura-e-mão-machinery_basis-9-injeções). O mod declara a barra, desenha o suporte e os quadros, e liga o shift-clique dele
(`GeneratorMenu`). A lib não conhece id de menu: ele chega por parâmetro.

### Elementos de tela fora da imagem

`screen_elements.rs` guarda, por tipo de menu, os elementos da tela que recebem o mouse fora dos 176 por 166 da imagem (as abas laterais), que é o que `GuiMekanism.findElementOver` acha e o que impede
`hasClickedOutside` de soltar o item do cursor (`GuiElement.isMouseOver` exige `active && visible`). A lib não conhece nenhum id: o mod declara, uma vez por tipo de menu, uma lista de `Element`.

- `Element::always(rect)`: um retângulo (relativo ao canto da tela) sempre ativo e visível (`GuiEnergyTab`, `GuiHeatTab`).
- `Element::when(rect, |state: &Mirror| -> bool)`: o mesmo com uma condição que lê o espelho do menu que o servidor sincroniza para a tela (a aba de xp só com o upgrade, a de avisos só com algum aviso). A condição
  roda fora da trava do registro.
- `declare(menu, elements)`, `declare_once(menu, || elements)`, `declared(menu)` e `over_element(menu, at, state)`. O registro é um só por processo.

O que é do elemento e o que não é: o retângulo de uma aba inset (`GuiInsetElement`) é o botão de dentro (18 por 18), não o suporte de 26 por 26 (`side_tab::InsetTab::button`); os `GuiSlot` e o `GuiSideHolder`
são inativos e não contam. A aba de upgrades e a janela dela são da `window` (`mb_window_over`), e os slots da barra de armadura são de `armor_bar::over_slot`; a injeção `bar_clicked_outside` junta os três
(`mb_bar_over_element`) com o espelho da tela aberta (`mb_open_mirror`).

## Fora da lib

Fica no mod que usa: ids de químicos, receitas, tiers, fábricas, texturas e o layout de cada tela. Também ficam de fora
configurador, segurança por dono, QIO e radiação, até dois mods precisarem deles. Do upgrade, a lib tem o componente e os slots; os
efeitos de cada tipo (velocidade e energia na `ProgressMachine`, abafamento, anchor, filtro) ficam em quem os usa.

## Arquitetura

Um mod `machinery_basis` com três partes, cada uma com um papel que os testes confirmaram:

| Parte | Tipo | Conteúdo | Quem alcança |
|---|---|---|---|
| `core/` | `[[crate]]` | Regras puras, sem tipos do jogo, e os registros de recursos | Mixin e `[api]`, uma cópia só |
| `glue.rs` | `[[module]]` | Tudo que chama o jogo | Só mixin, como `crate::game::machinery_basis` |
| `api/` | `[api]` | `pub use` do `core` e a dependência | Mods tipados, como `machinery_basis::core` |

O `core` guarda quase toda a lógica e testa com `cargo test`, sem jogo. O `[api]` alcança o `core` por um caminho que só
existe no build do launcher, `../../native-crates/machinery_basis/core`, então o `cargo test` local roda só no `core`.

Limite atual: um mod só tipado usa o `core`, mas não o `glue.rs`, porque o `Host` não tem energia. Esse mod precisa de
mixin para mover energia até o trabalho de [capabilities](https://github.com/horizon-rust/horizon/issues/7387) chegar.
Quando elas chegarem, boa parte do `glue.rs` deve encolher, por isso a lib mantém o código do jogo isolado ali.

## Ordem

1. **`core`:** recursos, tanque, lados, a base de máquina de progresso, o menu sincronizado (`container_sync`) e índice de receitas.
2. **`glue.rs`:** salvar tanques no NBT, distribuir energia e ejetar.
3. **Telas:** barra, medidor e seta, com a primeira tela real.
4. **Rede:** cabo de energia, depois tubo de fluido e de químico.
5. **Multibloco:** com a primeira estrutura do Mekanism, como o Dynamic Tank.

Os consumidores das etapas 1 a 3 são o Heat Generator e o Metallurgic Infuser. Um consumidor que não seja de máquina,
como o caso do Ars Nouveau, é o teste de que a lib continua genérica.

## Decisões

1. **Registro de recursos por dados:** cada recurso é um arquivo em `data/mekanism/mekanism/chemical/<nome>.json` no Mekanism,
   ou em `data/<ns>/machinery_basis/resource/<nome>.json` para quem não tem formato próprio, lido pelo
   `native_add_server_reload_listeners` (API 400). O id vira `<ns>:<nome>`. Isso vale para mods de conteúdo e para
   data packs do mundo, e o `/reload` relê. A leitura só existe no servidor: o cliente recebe o que precisa pela rede.
2. **Repositório:** a lib fica neste repo por enquanto, para mudar junto com o primeiro consumidor.

3. **Tags de recursos:** ficam em `data/mekanism/tags/mekanism/chemical/<nome>.json` no Mekanism, ou em
   `data/<ns>/tags/machinery_basis/resource/<nome>.json` para quem não tem formato próprio, o caminho de uma tag de registro,
   com a lista em `values`. Uma receita aponta para `#<ns>:<nome>` ou para o id do recurso.
4. **Formato dos tanques:** versionado desde o início, com `v: 1` no NBT, em `machinery_basis.tanks`. É o contrato entre
   mods. Uma versão desconhecida carrega vazia. Vale para o `Tank` simples, que o Heat Generator usa. O tanque de químico da
   base de máquina (`ChemicalTank`) segue o NBT do Java, em `chemical_tanks`, porque o objeto vivo o lê e grava a cada tick.

## Verificado

| Teste | Resultado |
|---|---|
| Mod de teste na Nightly `f319701` | `native.py check` OK |
| `[api]` usando `[[crate]]` pelo caminho do stage, com outro mod tipado chamando | `native-build` OK, sem rodar em mundo |
| Gerador e bateria em mundo, com salvamento | OK, ver [modding-horizon.md](modding-horizon.md) |
| Recursos por dados em mundo | Mod de conteúdo lido ao abrir; data pack do mundo entrou no `/reload`; arquivo `.txt` ignorado |
| Lib real com consumidor separado, em mundo | Energia, hoppers, ejetor, tanque e recursos funcionam; 85 itens entraram e 85 chegaram aos baús |
| Tanque depois de fechar e reabrir | Preservado, com 100 de hidrogênio |
| Arquivo de recurso sem cor | Recusado com a mensagem certa, sem afetar os outros |
| Regra de hoppers em mundo, com a máquina sem interface vanilla | Entrega ao hopper de baixo a 1 item por 8,13 ticks; hoppers de lado, com a face bloqueada, apontando para outro lugar ou travados não foram tocados |
| Máquina de progresso em mundo, com o Metallurgic Infuser (código anterior, já substituído pela base nova) | Progresso sobe 1 por tick; sem energia ou com a saída ocupada espera; sem entrada zera; 3 operações gastam 30 mB; progresso, tanque e receita sobrevivem a reabrir o mundo |
| Máquina só de itens em mundo, com receitas de tipo próprio e de fornalha (código anterior, já substituído pela base nova) | Enrichment Chamber, Crusher e Energized Smelter usavam a mesma `run_machine`, sem tanque |
| Rede de energia em mundo | Um gerador alimenta três máquinas; a divisão escassa fica igual; cortar pausa, religar volta em menos de um segundo |
| Recursos, tags e conversões em mundo | 8 recursos lidos; carvão vira 10 mB de carbono a 1 item por tick; a redstone não converte com ferro na entrada, porque nenhuma receita junta os dois |
| Hoppers por face em mundo | Topo, esquerda e frente entregam na entrada; pedra, sem receita, fica no hopper; hopper na direita não entrega |
| Lava dentro do bloco, em mundo | Balde com shift enche, a lava corre com níveis 2, 4 e 6 pelas quatro laterais e nada sai pela base; balde vazio tira e ela recua; parede removida com a lava parada abre o fluxo; quebrar com picareta deixa `lava[level=0]`; luz 15 com o bloco cercado de vidro; sem shift só abre a tela |

Lido, não executado: redes, `native_force_chunk` e `native_container_accepts`.

Os ritmos têm teste puro no `core`, sem o jogo: hopper a exatos 8 ticks, ejetor a exatos 11, e o ejetor comparado tick a
tick com o laço do `TileComponentEjector` por 1.000 ticks.
