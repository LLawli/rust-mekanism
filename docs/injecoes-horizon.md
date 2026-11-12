# Injeções no Horizon: o que o port ainda pede por injeção crua

O port do Mekanism usa injeção crua do horizon-mixins onde o Horizon não tem hook oficial. Este documento lista cada uma, diz por
que existe e propõe o hook que a substituiria. É a base da issue que o mantenedor vai abrir no Horizon, pedindo esses hooks, quando
o mod estiver completo. É um documento vivo: toda injeção crua nova entra aqui, no mesmo change que a cria (ver
[Como manter](#como-manter-o-documento)).

Pin: `f319701e8c9ab657ff562250b3ba07b2d654e3bc`, o mesmo `horizon_revision` dos três `horizon-mixin.toml`. Código do Horizon citado
daqui em diante é o desse commit, em `/mnt/dados/caches/rust-mekanism-ref/horizon-pin`. Java citado é o do Mekanism `b2d04e166`
(branch 26.3), em `/mnt/dados/caches/rust-mekanism-ref/mekanism/src`.

## Resumo

O que é: uma lista de contratos que faltam no Horizon. Um hook do catálogo (`hook = ...` em `native-hooks.json`) tem contrato
estável e não é problema. Só entram aqui as injeções que dependem de algo que o Horizon pode mudar em qualquer commit.

Contagem em 2026-11-12 (depois da frente `transmissor-obj`, que acrescentou 2 injeções cruas, a `obj_material_parse` e a `obj_material_quads`, e 1 módulo, o `obj-material`, todos da `machinery_basis`: o `Kd` e o `neoforge_TintIndex` do `.mtl`, H16; e antes dela a frente `oclusao`, que acrescentou 3 injeções cruas, `occlusion_shapes` da `machinery_basis` (a forma de oclusão real, H51) e `glass_ids` e `glass_skip` do `mekanismgenerators` (o corte de face entre os vidros, H51), 1 módulo, o `glass-skip`, e 1 hook do catálogo, o `fill_bucket` do `mekanism`, e antes dela da frente `item-antes-do-bloco`, que trocou o hook `right_click_block` do `mekanism` por 2 injeções cruas, a `use_hand` e a `predict_item_on_block` (H72), e antes dela a frente `nota-instrumento`, que acrescentou 1 injeção crua, a `note_instrument`, do `mekanism`: o instrumento do bloco de nota do sal e do bio fuel, H44, e antes dela a frente `cubo-nucleo`, que acrescentou 1 injeção crua, a `energy_cube_item_core`, do `mekanism`, e antes dela as frentes `armadura-vestida` (6 injeções cruas, o hook `standalone_models` e 2 módulos), `fluidos` (5 hooks do catálogo com estrutura privada, 2 injeções cruas e 2 módulos), `itens-equipamento` (1 injeção crua, a `item_component_type`, sobre as 2 injeções cruas e o módulo das `itens-craft`) e `blocos-maquinas`, que acrescentou 1 injeção crua, a `shape_limit`, e 1 módulo, o `block-rows`, e ampliou a `active_light`) e `generators-conteudo` (2 injeções cruas, `block_rows` e `creative_tabs`, e 2 módulos, `block-rows` e `creative-tabs-table`, todos do Generators: as cópias das do Mekanism com as tabelas do Generators) e `acabamento-conteudo` (1 injeção crua, a `enhanced_ao`, e 1 módulo, o `enhanced-lighting`) e `transform-modelo` (2 injeções cruas, `model_transform_decode` e `model_transform_bake`, e 1 módulo, o `model-transform`, todos da `machinery_basis`: o `transform` do modelo, H67) e `mek-data` (1 injeção crua, a `mek_data_alias`, do `mekanism`: o tipo de receita `mekanism:mek_data` na mesa, H79), e `energia-item` (2 injeções cruas, `mek_data_energy_grid` e `mek_data_energy_use`, e 1 módulo, o `mek-data-energy`, do `mekanism`: a energia que a `mek_data` copia, H79; mais os hooks `item_bar` e, no Generators, `tooltip`, do catálogo), sobre os `horizon-mixin.toml` (conferência na [próxima seção](#contagem-e-conferência)):

| Categoria | Quantas | Assunto deste documento |
|---|---|---|
| `[[injection]]` no total | 159 | |
| Hook do catálogo, usado só com o contrato dele | 46 | não |
| Hook do catálogo, usado com estrutura privada do jogo | 45 | sim |
| Injeção crua (`target = ...`) | 68 | sim |
| `[[module]]` (fora da contagem de `[[injection]]`) | 34 | sim |

46 + 45 + 68 = 159. Os 34 módulos são outra coisa: código nosso compilado dentro do `horizon-client`. O assunto do documento são, então,
113 injeções e 34 módulos em uso, mais o que já foi provado em spike e o que as fases futuras vão pedir:

| Estado | Cruas | Módulos | Hook com estrutura privada |
|---|---|---|---|
| Em uso | 68 | 33 | 45 |
| Provada em spike | 4 | 0 | 1 |
| Prevista | 5 | 0 | 0 |

As provadas em spike não contam `native_energy_at` nem `override_gameplay`, que já estão em uso. As previstas vêm do plano e não foram compiladas.

### Hook proposto contra injeções que ele elimina

Todos os nomes são propostas: nenhum existe no pin. "v2" é uma nova versão de um hook do catálogo, porque o contrato antigo nunca
muda. Cada proposta tem sua ficha em [Hooks e pedidos propostos](#hooks-e-pedidos-propostos), e as correções H22 a H26 e H30 em
[Correções pedidas ao renderer do Horizon](#correções-pedidas-ao-renderer-do-horizon).

| Proposta | Tipo | Elimina | Estado |
|---|---|---|---|
| H1 `block_entity_energy_instance` | hook | `energy_capacity` | em uso |
| H2 `block_entity_container_instance` | hook | `Containers::view`, `can_take`, `native_container_accepts`, `native_fluid_at` | prevista (4) |
| H3 `block_fluid` | hook | `logged-fluid-row` | em uso |
| H4 `screen_tooltip` v2 | hook | `widget_tooltip` (mekanism e mekanismgenerators) | em uso (2) |
| H5 `component_types` v2 | hook | `command_components` e `item_component_type` | em uso (2) |
| H6 `item_model_node` | hook | `component_item_model` | em uso |
| H7 `item_container_accepts` | helper | módulo `slot-accepts` | em uso |
| H8 helpers de leitura e escrita | helpers | 7 hooks com estrutura privada, os módulos `glue` | em uso (7) |
| H9 `screen_layout` | hook | módulo `windows` em `native_hooks.rs`, escrita em `mod_screen` | em uso (módulo) |
| H10 `screen_slot_at` | hook | `slot_at` (RETURN) | em uso |
| H55 `mob_layer` v2 e as partes do modelo do mob | hook | `worn_hide`, `worn_hide_player`, `worn_mob`, `worn_stand`, `worn_arm`, `worn_cape` e os módulos `worn-gear` e `worn-hides` | em uso (6 cruas, 2 módulos) |
| H79 o tipo de receita de mesa de um mod (`mekanism:mek_data`) e o resultado montado com a grade | hook | `mek_data_alias` (cru, HEAD de `Recipes::from_sources`), `mek_data_energy_grid` e `mek_data_energy_use` (crus, os dois HEAD de `Recipes::find_with`) e o módulo `mek-data-energy` | em uso (3 crus, 1 módulo) |
| H70 o sombreamento de vértice dos modelos de bloco (`EnhancedBlockModelLighter`) | hook | `enhanced_ao` (cru, RETURN de `lighting::ambient_occlusion`) e o módulo `enhanced-lighting` | em uso (1 cru, 1 módulo) |
| H71 `block_shape_context` | hook | `context_outline` (mekanism) | em uso |
| H72 `item_use_first` | hook | as injeções `use_hand` e `predict_item_on_block`, para o Configurator e o cartão passarem à frente da tela do bloco e darem o resultado do clique | em uso (2 cruas) |
| H73 `key_mappings` | hook | o `native_mod_key` sem entrada na tela de Controles (a tecla `N` do modo) | prevista |
| H91 `native_draw_model` com alfa, e `CubeFace` com rotação de uv | helper / dado | `BlockEntityDraw::vertices_mut` (privado) para o alfa do núcleo do cubo (`RenderEnergyCube`: `ARGB.as8BitChannel(getEnergyScale())`), e a rotação de uv de cada face que `native_draw_cube` não aceita (`rotation` de 90 e 270 nas faces finas das portas) | em uso (alfa pela estrutura privada; a rotação de uv diferente do original) |
| H93 abrir o menu de um bloco de mod declarado pelo conteúdo | dado | `Game::open_mod_menu` só é alcançado por um gabarito com a ação de uso (`furnace`, `beacon`); o cubo de energia precisa de `facing` com seis valores (o `furnace` o recusa) e o `beacon` toca o instrumento `hat` no bloco de nota de cima (o original, `harp`). Hoje o cubo usa o gabarito `lodestone` e abre o menu pela injeção `use_hand` (decisões 47 e 68). Um campo `opens_menu` em `properties` (ou o `menu/<bloco>.json` bastando) resolveria | em uso (contorno na injeção crua `use_hand`) |
| H92 forma de colisão e de seleção por instância | hook | o `BlockEnergyCube.getShape` lê a configuração de lados do tile e o `facing` (256 formas: o quadro, girado, mais as placas dos lados habilitados); o Horizon tem uma forma por estado (as 6 placas sempre) | seleção em uso (`outline_boxes`, 1 injeção crua); colisão prevista (só há a forma por estado) |
| H87 `PrimitiveShape::TexturedQuad` | dado | `world_overlay` (cru, HEAD cancelável de `world_primitive_geometry`) e o módulo `world-overlay` (a sobreposição colorida da face do Configurator) | em uso (1 cru, 1 módulo) |
| H88 estado do HUD para um mod | helper | os campos privados que `gui_layer_post` lê (`health_hud`, `titles`, `mode`, `armor`, `inventory`, `ui.hide_gui`, `settings`) | em uso (hook com estrutura privada) |
| H89 o alvo do olhar e a guinada da câmera para um mod | helper | `Game::pick`, `may_build`, `yaw` e `basis::front` na sobreposição do Configurator | em uso (hook com estrutura privada) |
| H90 envio ao servidor dos ganchos de tela (`&self`) | hook | a fila `pending` de `configurator.rs` que o `client_tick` esvazia | em uso (contorno) |
| H11 `screen_draw_over_slots` | hook | `draw_inventory_slots` (HEAD, cancelável) | em uso |
| H12 `menu_hide_slots` | helper | uso de `lan.hub` em `net_from_client` | em uso |
| H13 `block_entity_save` | hook | `save_level` | spike |
| H14 `block_entity_removed` | hook | `remove_entity` | spike |
| H15 `visuals` v2 | hook | `native_frame_visuals` | spike |
| H16 leitura do `Kd` e do `neoforge_TintIndex` do `.mtl` | correção | as injeções `obj_material_parse` e `obj_material_quads` e o módulo `obj-material` | em uso (2 cruas, 1 módulo) |
| H17 ordenação da camada translúcida | correção | `outline_fill` | spike |
| H18 `properties.light` condicional | dado | `active_light` (mekanism) e `block_rows` (mekanismgenerators), só a luz | em uso |
| H19 `looping_sound` | helpers | módulo `tile-sound` | em uso (módulo) |
| H27 `menu_result_taken` | hook | `result_take_before` (cru) | em uso |
| H20 `menu_slot_rules` v2 | hook | o contexto de clique de `result_take_before` e `menu_click_before` | prevista |
| H21 `screen_slot_highlight` | hook | `slot_highlight` (mekanism) | em uso (1) |
| H22 fundo escurecido da tela de contêiner (degradê do vanilla) | correção | `container_dim` (HEAD de `HudBatch::fill`) | em uso (1) |
| H30 iluminação do mundo, mais clara no original | correção | nenhuma (diferença de pixel, causa no renderer) | pedido |
| H23 caixa do tooltip de item | correção | nenhuma (diferença de pixel) | pedido |
| H24 ícone de item de bloco | correção | nenhuma (diferença de pixel) | pedido |
| H25 item desenhado em escala (`renderItem(..., 0.75)`) | correção | nenhuma (diferença de pixel) | pedido |
| H28 modelo ausente no ícone de slot | correção | nenhuma (diferença de pixel) | pedido |
| H29 `composter_item_handler` | helper | módulo `composter-wrapper` | em uso |
| H26 mistura de sprites animados na GPU, em float | correção | `sprite_ties` (HEAD) e módulo `sprite-ties` | em uso (1 injeção, 1 módulo) |
| H31 barra de armadura e mão no tipo de menu (`armorSideBar`) | dado + hook | `bar_slot_position`, `bar_slots`, `bar_slot_count`, `bar_menu_slot`, `bar_machine_menu`, `bar_quick_move`, `bar_answer_click`, `bar_resend_menu` e os módulos `armor-bar`, `bar-menu`, `bar-machine`, `bar-hub`, `bar-quick-move` | em uso (8 injeções, 5 módulos) |
| H32 elementos da tela fora da imagem (`hasClickedOutside`) | dado | `bar_clicked_outside` | em uso (1) |
| H33 `menu_data_sync` | hook | `server_tick` (fase `ServerTickStart`) da `machinery_basis`, que entra em cada nível por `enter_level` para ler o valor vivo e mandar o pacote do menu | em uso (1) |
| H34 `neighbor_changed` v2 | hook | `flush_signals` (cru, HEAD de `Game::flush_spill`) | em uso (1) |
| H35 `block_entity_changed` | hook | `container_changed` (cru, HEAD de `Game::container_changed`) | em uso (1) |
| H36 `mod_container_viewers` v2 | hook | nenhuma injeção: o `server_tick` da `machinery_basis` faz a conta com o que lê de `hub.guests` (o proposto dá a lista de jogadores com o menu, espectadores marcados) | pedido |
| H37 `block_entity_energy_inserted` | hook | `energy_store` (cru, HEAD de `Game::native_energy_store`) | em uso (1) |
| H39 `block_animate_tick` | hook | `animate_tick` (cru, HEAD de `block_animate_tick`) e o módulo `animate-registry` | em uso (1 cru, 1 módulo) |
| H38 `ingredient` v2 | hook | `components-ingredient-stash` (cru, HEAD de `Ingredient::parse_object`), o hook `ingredient` v1 `components-ingredient-items` e o módulo `components-ingredient` | em uso (1 cru, 1 hook, 1 módulo) |
| H40 `NativeConfigKind::Synced` | tipo de configuração | nenhuma (diferença de comportamento, sem injeção) | pedido |
| H41 saída de áudio a 48.000 Hz | correção do mixer | nenhuma (diferença de 0,045 % do RMS, sem injeção) | pedido |
| H42 `properties.experience` | dado | `fluorite_experience` (cru, RETURN de `Game::block_experience`) | em uso (1) |
| H43 abas criativas: pilhas e abas do vanilla | dado + hook | `creative_tabs` (cru, RETURN de `Plan::creative_tabs`; uma do Mekanism e uma do Generators) e o módulo `creative-tabs-table` (um de cada) | em uso (2 injeções, 2 módulos) |
| H44 `properties.instrument` | dado | `note_instrument` (cru, RETURN de `J::rename`) | em uso (1) |
| H45 `item_name` padrão do item | correção | nenhuma (a cor do nome vai pelo hook `tooltip`, só na primeira linha) | pedido |
| H49 propriedades de linha do bloco (reação ao pistão, oclusão, resistência negativa, `canBeReplaced`, `isPathfindable`) | dado | `active_light` (mekanism) e `block_rows` (mekanismgenerators), o resto que não é luz, e o módulo `block-rows` (um de cada) | em uso (2 injeções, 2 módulos) |
| H50 limite de caixas por forma | dado | `shape_limit` (cru, HEAD cancelável de `BlockShapes::check`) | em uso (1) |
| H51 a forma de oclusão de um bloco de mod e a regra de corte de face entre blocos diferentes | dado | `occlusion_shapes` (cru, HEAD cancelável de `tables`), `glass_ids` (cru, HEAD de `FaceVisibility::new`), `glass_skip` (cru, HEAD cancelável de `FaceVisibility::skips`) e o módulo `glass-skip` | em uso (3 cruas, 1 módulo) |
| H110 `canBeReplaced(state, fluid)` de um bloco de mod e o dispensador no gancho do balde | dado + hook | o hook `fill_bucket` (só o jogador; o dispensador e o `bucket_empty` do mundo ficam sem) | proposta |
| H111 `isPathfindable` por tipo de caminho de um bloco de mod | dado | nenhuma (o gabarito já dá `BLOCKED` aos blocos; ver a decisão 87 do plano) | proposta |
| H65 tags de um mod que é dependência | dado | nenhuma (as quatro tags `mekanism:structures` que a datagen do Generators acrescenta ficam para o arquivo do Mekanism) | proposta |
| H66 tags de mod no comando `execute if block #tag` | dado | nenhuma (só a conferência em jogo é afetada) | proposta |
| H67 `transform` do modelo (translação, rotação, escala e origem, na raiz e no filho do `composite`) | dado | as injeções `model_transform_decode` e `model_transform_bake` e o módulo `model-transform` (o `CuboidModel` ganha o campo e o cozimento junta a transformação ao `ModelState`) | em uso (2 cruas, 1 módulo) |
| H46 `piglin_currency` | hook | nenhuma (o lingote de glowstone refinado não é moeda de troca) | pedido |
| H47 fluidos de mod no motor | contrato | o módulo `liquid-glue`, o módulo `water-like`, as injeções `fluid_state` e as oito `fluid_entity_*` e os hooks `tick_block`, `dispense_item`, `block_changed` e `block_spread` (o `FlowingFluid` portado na lib, bloco com `level`, a física de entidade do jogador, do mob e do item) | em uso (2 módulos, 9 cruas, 4 hooks) |
| H48 tinta constante no modelo de bloco | dado | nenhuma (22 pares de texturas cozidas com a cor, a água pesada fica sem a cor, e as 10 do vidro dos tanques de fluido) | pedido |
| H60 variante de blockstate de mod com modelo de reserva e item especial de mod | dado | nenhuma (modelos escritos à mão para o cubo de energia, o tanque de fluido e os transmissores, decisão 26) | pedido |

| H99 desenhista de item de mod por quadro (`SpecialModelRenderer`: a pilha, os componentes e o `gameTime` como argumento, para o núcleo do cubo de energia no item da barra e da mão) | hook | `energy_cube_item_core` (cru, RETURN de `Game::resolve_item_in_hand`: uma camada de item a mais, o núcleo) | em uso |
| ~~H100~~ `BlockEntityDraw::partial` preso com o `tick freeze` | retirado em 2026-11-12 | não procede: o `partial` é 1,0 e o `gameTime` fica parado sob `tick freeze` (medido em jogo) | retirado |

H1 a H6, H13 a H15 e H21 são genéricos: qualquer mod de máquina, de rede, de fluido ou de tela com slot virtual usa. Nada neles cita o Mekanism.

## A regra de slot do clique

O clique do cursor, a tecla numérica e o arrasto num menu de máquina perguntam `Slot.mayPlace`, que no Mekanism é
`InventoryContainerSlot.mayPlace`: `isValidForInsertion(item, MANUAL)` do slot da máquina (`InputInventorySlot`, `ChemicalInventorySlot`,
`EnergyInventorySlot`, `FluidFuelInventorySlot`; a saída recusa, porque só aceita `INTERNAL`). O Horizon tem o hook `menu_slot_rules` v1
(`listen`, `native_menu_slot_rules(menu, slot, stack, may_place, max_stack)`, em `horizon_client::menu`) para isso, mas ele roda dentro do menu,
no crate `horizon-client-lib`: sem o `Game`, sem a posição do bloco e sem o outro slot. As regras do Mekanism olham o estado do bloco
(a entrada do Infuser pergunta pelo químico do tanque, e o slot de infusão pelo item da entrada), então a resposta não sai só de menu, slot e item.

Como está feito, sem hook novo: o crate `[api]` da `machinery_basis` implementa `Hooks::menu_slot_rules` e consulta
`machinery_basis_core::slot_rules`, uma regra por dono (`mekanism`, `mekanismgenerators`) guardada numa `thread_local` do núcleo. O núcleo é um
`[[crate]]`, uma cópia só no processo, que o `[api]` e os módulos de glue compartilham. Antes de cada clique de menu de máquina
(`lan_machine_click`, que o servidor roda para o anfitrião e para os convidados), os dois mods deixam a regra do bloco aberto: `result_take_before`
(Mekanism) e `menu_click_before` (Generators) leem o bloco no mundo e guardam o `isValidForInsertion` dos slots. A regra só tira
(`may_place &= regra`), e o clique de quem não tem menu nosso aberto apaga a do dono. É uma regra por clique e por thread: o clique
roda na thread do servidor, a mesma onde o hook responde.

A regra recebe também a pilha que o hook entregou, como `&dyn Any` (a lib não conhece o tipo do jogo): os slots virtuais da janela de upgrades
(`UpgradeInventorySlot`) dependem do tipo do upgrade, que é um componente do item (`mekanism:upgrade_type`) e não está no id. Os mods a leem por
`ItemStack::data().other(..)`, que não precisa do `Game`. Sem isto a regra só via o id e vetava os dois slots, e um upgrade na mão não entrava
na janela (2026-11-08). É o mesmo hook do catálogo e a mesma estrutura privada de antes: nenhuma injeção nova, a contagem não muda.

O que a regra v2 (H20) elimina: as duas injeções cruas e o contexto por clique. Com `pos` e `Game` no hook, o mod responderia pelo bloco direto.

## Termos e estados

- **Hook do catálogo:** `hook = "x"` com `hook_version`, resolvido por `native-hooks.json`. O contrato (assinatura, quando dispara,
  o que faz) não muda, só ganha versão.
- **Injeção crua:** `target = "..."` (um caminho Rust que pode mudar em qualquer commit, por isso o `horizon_revision` fixo) ou
  `[[module]]` (um arquivo nosso enxertado num módulo do Horizon).
- **Hook com estrutura privada:** `hook =`, mas o helper recebe `&Game` ou `&GameLevel` e o código dele lê campo ou método que não
  é API: `game.world`, `game.res`, `game.recipes()`, `game.gui`, `level.registry`, `held_mut`. O hook é estável, o uso não. O que
  conta aqui é só isso. Helpers documentados com número de API, fora da lista `helpers` do catálogo, e helpers de fragmento ainda
  pendente ficam em [Dependências a vigiar](#dependências-a-vigiar), porque o risco deles é menor e é outro.
- **Estados:** `em uso` (está nos `horizon-mixin.toml`), `provada em spike` (compilou e funcionou na mudança descartável
  `spike: validate the critical injections` ou no diff `~/.cache/rust-mekanism/spike-upgrade/spike-upgrade-A-fim.diff`),
  `prevista` (o plano pede, nunca foi compilada).

## Contagem e conferência

| Arquivo | `[[injection]]` | `hook =` | `target =` | `[[module]]` |
|---|---|---|---|---|
| `mods/machinery_basis/horizon-mixin.toml` | 65 | 24 | 41 | 21 |
| `mods/mekanism/native/horizon-mixin.toml` | 67 | 46 | 21 | 8 |
| `mods/mekanismgenerators/native/horizon-mixin.toml` | 27 | 21 | 6 | 5 |
| Total | 159 | 91 | 68 | 34 |

Todas são `kind = "HEAD"`, menos `window_slot_at`, `bar_slot_position`, `bar_slots`, `bar_slot_count`, `fluid_state`, `enhanced_ao`, `worn_hide_player`, `creative_tabs` (a do Mekanism e a do Generators) `fluorite_experience`, `energy_cube_item_core`, `note_instrument`, `fluid_entity_mob_air` e `obj_material_parse` (`RETURN`), e `model_transform_decode` (`MODIFY_VARIABLE`). 27 das 63 cruas são `cancellable = true` (as cinco `fluid_entity_*` de 2026-11-12 que cancelam: `fluid_entity_splash`, `fluid_entity_fall`, `fluid_entity_touch`, `fluid_entity_travel` e `fluid_entity_mob_travel`; as outras vinte e duas: a lista a seguir é a de 2026-11-09, sem as posteriores, entre elas a `use_hand` e a `predict_item_on_block`; a oitava é `container_dim`; as três últimas, `bar_menu_slot`, `bar_clicked_outside` e `bar_quick_move`; `worn_arm`, `worn_cape`, `item_component_type` e `shape_limit`, de 2026-11-09; `model_transform_bake`, de 2026-11-09; e o `widget_tooltip` de cada um dos dois mods); as que não são: `logged-fluid-row`, `container_dim_flag`, `components-ingredient-stash`, `window_slot_at`, `sprite_ties`, `bar_slot_position`, `bar_slots`, `bar_slot_count`, `bar_machine_menu`, `bar_answer_click`, `bar_resend_menu`, `flush_signals`, `container_changed`, `animate_tick`, `fluid_state`, `fluid_entity_update`, `fluid_entity_mob_update`, `fluid_entity_mob_air`, `enhanced_ao`, `worn_hide`, `worn_hide_player`, `worn_mob`, `worn_stand`, `result_take_before`, `active_light`, `energy_store`, `creative_tabs`, `fluorite_experience` e `menu_click_before`. Nenhuma usa `trait_method`: os crates `[api]` do Mekanism e do Generators têm só
`impl Hooks for Mod {}`, e o da `machinery_basis` implementa um hook do catálogo, `menu_slot_rules` v1 (ver [A regra de slot do clique](#a-regra-de-slot-do-clique)), que não passa pelo `horizon-mixin.toml` e por isso não entra na contagem. Os 44 nomes distintos de `hook` (conferido em 2026-11-08 com `sort -u` sobre as 67 linhas `hook =`: 23 repetem um nome que outro mod já usa, 67 - 23 = 44; o `tick` do `mekanismgenerators`, de 2026-11-08, é um desses 23) existem no catálogo do pin, na versão 1.

Dos 91 hooks, 45 têm estrutura privada (lista em [Hooks do catálogo com estrutura privada](#hooks-do-catálogo-com-estrutura-privada)),
e 46 não (o `fill_bucket` do `mekanism`, de 2026-11-12 (frente `oclusao`), só usa o contrato: cancelar o clique do balde; (o `block_entity_update_tag` do `mekanism`, de 2026-11-11 (frente `cubo-energia`), só usa o contrato: o cliente recebe a cópia da block entity do cubo; os `screen_key_pressed`, `screen_mouse_pressed` e `block_highlight` dos efeitos de tela do Configurator, de 2026-11-11, só usam o contrato; o `item_bar` do `mekanism` e o `tooltip` do `mekanismgenerators`, de 2026-11-11 (frente `energia-item`), só usam o contrato: a barra de energia e a linha do tooltip; o `net_from_server` e o `item_model_property` do Configurator, de 2026-11-11, só usam o contrato; o `standalone_models` do `mekanism`, de 2026-11-09, só devolve ao jogo os ids que o mod pede e entrega a tabela à `machinery_basis`: limpo; a divisão foi inferida pela frente do som e conferida em 2026-11-08, hook a hook, contra os três `horizon-mixin.toml`: `machinery_basis` tem 20 hooks, 15 com estrutura privada e 5 sem (`add_server_reload_listeners`, `server_reload_listener_apply`, `block_entity_submit`, `server_data_loaded` e `ingredient`); `mekanism` tem 27, 10 e 17; `mekanismgenerators` tem 20, 6 e 14; 15 + 10 + 6 = 31 e 5 + 17 + 14 = 36, e o total dos três arquivos fecha em 67; os 3 hooks novos de `mekanismgenerators` são os da configuração de servidor, `register_configs`, `config_loading` e `config_reloading`, API 397, limpos): 5 em `machinery_basis`, 17 em `mekanism` (os 3 da configuração, de cliente e de servidor, e o `player_logged_in` da mensagem de receita incompleta entre eles), 14 em `mekanismgenerators` (o `client_tick`, o hook `tick` só com o contrato dele, que registra o animador das partículas do Heat Generator; um deles, `heat_tab_press`, é `screen_mouse_button_pressed_pre`, o mesmo hook de `window_press`, usado só com o contrato dele: a aba de calor toma o clique sem o som e o pacote que o `screen_click` mandaria). Os 31 com estrutura privada são os 7 de antes, o `server_data_loaded` do `mekanism` (a varredura de receitas incompletas, 2026-11-08), os 9 da janela de upgrades (`window_*`; o `window_viewers` saiu em 2026-11-08, veja `container_viewers`), os 2 do som e da experiência (`client_tick` e `destroy_block`), os 3 do nome (`place_block`, `loot_drops` e `container_opened`), o `menu_quick_move` do Heat Generator, 1 da aba de xp (`menu_button`), 1 da experiência de uma explosão (`explosion_detonate`) os 4 do nome do Heat Generator (`place_block`, `loot_drops`, `container_opened` e `destroy_block` de `mekanismgenerators`) o `server_tick` da `machinery_basis` (o `broadcastChanges` no começo do tick do servidor, 2026-11-08; era do `mekanism`) e o `neighbor_changed` da `machinery_basis` (2026-11-08).

## Injeções cruas em uso (68)

### `window_draw_slots` (machinery_basis)

- **Alvo:** `Game::draw_inventory_slots` (`game.rs:9461`), HEAD cancelável. **Proposta:** H11.
- **Para quê:** a ordem de desenho do original: itens dos slots, janela de upgrades, itens dos slots virtuais, e só depois o tooltip e o
  item no cursor. `native_screen_draw` fica sob os itens e `screen_render_post` cobre o tooltip, então nenhum hook do catálogo dá essa
  ordem. Sem janela aberta devolve `Continue` e o Horizon desenha como sempre.
- **Java:** `GuiMekanism.extractContents` (`GuiMekanism.java:199-246`), `GuiVirtualSlot.drawContents`.
- **Risco:** é o núcleo do desenho do inventário e muda sempre que o Horizon mexe na tela. O helper reimplementa o laço com
  `slot_view`, `slot_position` e `draw_slot_item`.

### `window_slot_at` (machinery_basis)

- **Alvo:** `Game::slot_at` (`game.rs:9349`), RETURN. **Proposta:** H10.
- **Para quê:** `GuiMekanism.isHovering` (`GuiMekanism.java:702-706`): sob a janela só os slots virtuais dela são alvo, e um slot da
  máquina coberto não é, esteja ele antes ou depois do virtual na ordem dos slots. Vale para hover, tooltip, clique, as teclas 1 a 9,
  F e Q e a divisão por arrasto, que `screen_mouse_button_pressed_pre` cancelando não alcança.
- **Risco:** mudar o retorno de `slot_at` ou quem o chama.

### `container_dim` e `container_dim_flag` (machinery_basis)

- **Estado:** em uso desde 2026-11-07. **Alvos:** `horizon_client_render::hud::HudBatch::fill` (`hud.rs`, método público), HEAD cancelável (`container_dim`);
  `horizon_client::game::Game::draw_inventory` (`game.rs:9486`), HEAD (`container_dim_flag`); mais o módulo `container-dim` em `hud.rs` (ver Módulos).
  **Proposta:** H22.
- **Para quê:** o fundo escurecido de uma tela de máquina. O original desenha `fillGradient(0, 0, width, height, 0xC0101010, 0xD0101010)`
  (`Screen.extractTransparentBackground`), porque `AbstractContainerScreen.isInGameUi()` é verdadeiro (nem blur, nem `inworld_menu_background`). O Horizon pinta
  a janela toda com um retângulo fixo `rgba(16, 16, 16, 160)` em `draw_inventory` (`game.rs:9494`) e em outras telas de contêiner (`beacon.rs`, `trading.rs`,
  `stations.rs`, o Creative, o livro e a placa). O HEAD de `draw_inventory` escreve no módulo se a tela aberta é de uma máquina nossa (`Game::mb_screen_owns_dim`:
  o `open_spec`, a spec de janela do bloco aberto, registrada por quem usa a `machinery_basis`; o `frame` também o escreve, para limpar o estado quando a tela
  fecha). O HEAD de `HudBatch::fill` lê o estado e, só com ele ligado e com o retângulo de sempre (a janela toda, `rgba(16, 16, 16, 160)`), desenha no lugar o
  degradê do vanilla (`HudBatch::fill_gradient`, `rgba(16, 16, 16, 0xC0)` a `0xD0`) e cancela o `fill`. Qualquer outro `fill` segue. O baú e a fornalha do vanilla
  ficam com o retângulo do Horizon (conferido: `dev/referencia/conferir_fundo.py`).
- **Por que não é um hook do catálogo nem um `MODIFY_ARG`:** o retângulo é uma chamada no meio de `draw_inventory`, e a função tem um closure
  (`let part = |batch, name, ...|`): o planejador recusa as injeções de chamada dentro dela (`E_UNSUPPORTED_CONTROL_FLOW`, medido). Não há flag em
  `NativeScreen` (`mod_api.rs:288`: `own_labels`, `own_background`) nem hook que cancele o fundo. Um HEAD em `draw_inventory` não pula só o retângulo. O `HudBatch`
  não conhece o `Game`, por isso o estado vai por um módulo no `hud.rs`.
- **Escopo:** sem id do Mekanism; vale para toda tela que a `machinery_basis` abre (decisão do usuário: só as do mod, não as de contêiner do vanilla).
- **Hook mais próximo:** nenhum.
- **Hook proposto:** H22 (o renderer desenhar o degradê do vanilla no fundo de toda tela de contêiner, no lugar do retângulo fixo, e uma flag em `NativeScreen` para
  o mod pedir o mesmo).
- **Risco:** depende de `HudBatch::fill` e da cor do retângulo, de `draw_inventory` e do campo `mod_screen`/`open_spec`. Mudar a cor, o alfa ou o método, e o fundo volta ao
  do Horizon, sem erro. O custo é uma leitura de `AtomicBool` e cinco comparações por `fill`.
- **Conferido em jogo** (2026-11-07, `horizon`, contra o original, mesmo mundo): ver H22.

### `energy_capacity` (machinery_basis)

- **Estado:** em uso. **Alvo:** `horizon_client::game::machines::native_energy::Game<'_>::native_energy_at`, HEAD, cancelável
  (`native_energy.rs:62`, função privada de `impl Game`).
- **Helper:** `mods/machinery_basis/energy_capacity.rs`, que chama `machinery_basis::energy_capacity` em `glue.rs`.
- **Por que é necessária:** a capacidade de uma máquina muda com os upgrades e é por instância
  (`MachineEnergyContainer.setMaxEnergy`, `MachineEnergyContainer.java:73-76`, recalculada em `updateMaxEnergy`, `:90-105`). O
  `ModEnergyBuffer` do Horizon é um só por tipo de bloco.
- **Hook mais próximo:** `block_entity_energy` (`replace`, servidor). Recebe `block: &str` e devolve o buffer do tipo: sem posição,
  sem a block entity. Falta a instância.
- **Hook proposto:** H1.
- **Faces por instância (desde 2026-11-09, frente `config-lados`):** a mesma injeção devolve, além da capacidade, as faces por onde a
  energia entra e sai (`insert_faces` e `extract_faces` do buffer), do `DataType` que a configuração de lados dá à face (`ConfigHolder.canInsert`
  e `canExtract`, pela frente de agora). O buffer do bloco sai com todas as faces fechadas: uma máquina sem configuração (posta por
  `setBlock`) não aceita energia por face nenhuma, como o original, e a que um item pôs publica as dela na colocação. Nenhuma injeção nova.
- **Risco:** a função é privada, dentro de um `impl` com `#[allow(dead_code)]` (o comentário diz "Native mods use these; nothing
  in Horizon does"), e `native_energy_transfer`, `native_energy` e `native_energy_insert` dependem dela. Renomear, mudar a
  assinatura ou deixar de passar por ela quebra o build ou, pior, faz a capacidade voltar à do bloco sem aviso.

### `fluid_state` (machinery_basis)

- **Alvo:** `horizon_client_lib::motion::fluids::FluidResources::fluid`, `kind = "RETURN"`, helper `fluid_state.rs`, e o módulo `water-like` (`water_like.rs`, em `motion.rs`).
- **Por que é necessária:** a física de entidade (jogador, itens, flechas) lê o fluido de cada estado de uma tabela só de água e lava. O RETURN dá a um estado de bloco de líquido de mod o `FluidState` da água, só nesta consulta (o mesher não a usa), para o fluxo empurrar, o item subir e o ar acabar do jogador como no original (`FluidType`: `canSwim`, `canDrown`, `canPushEntity`, `motionScale`).
- **Hook mais próximo:** nenhum; H47 pede o fluido de mod no motor.
- **Risco:** depende do nome e da assinatura de `FluidResources::fluid`. Não dá ao bloco o resto da água (fonte, hidratar). O que a água faz a mais na entidade (travel de água, respingo, `wasTouchingWater`, queda zerada, fogo apagado) é desfeito pelas oito `fluid_entity_*` logo abaixo; o mob não passa por aqui (lê a tabela de fatos) e é tratado por elas.

### `fluid_entity_*` (machinery_basis, 8 injeções, 2026-11-12, frente `fluido-entidade`)

Substituem a `fire_before` (removida). O `fluid_state` empresta a água ao bloco de líquido de mod para a altura, a correnteza (0,014, o `motionScale` padrão do `FluidType`) e o item;
estas oito tiram da entidade o que a água faz e o `FluidType` do Mekanism não faz (`isWaterLike` falso, `canExtinguish` falso, `FluidType.move` falso, `fallDistanceModifier` 0,5):
a decisão 18 do plano e `rastros/fluids_entity.jsonl`. As contas puras ficam no módulo `water-like` (`water_like.rs`, testado por `include!` em `tests/fluid_entity_module.rs`).
Todas têm o alvo e o helper abaixo; o risco comum é o nome e a assinatura de cada alvo (privados do `horizon_client_lib`) e, para as de trait, a forma `<Tipo as Trait>`.

| Injeção | Alvo (`horizon_client_lib::...`) | Tipo | O que faz |
|---|---|---|---|
| `fluid_entity_update` | `motion::fluids::MotionWorld<'_,'_,'_>::update_fluids` | HEAD | guarda, para esta chamada, se a água que o jogador toca é só de fluido de mod (`Contact::mod_only`) |
| `fluid_entity_splash` | `motion::fluids::<MotionWorld<'_,'_,'_> as EntityFluidTickWorld>::water_splash` | HEAD, cancela | sem respingo na entrada (`isWaterLike` falso) |
| `fluid_entity_fall` | `motion::fluids::<MotionWorld<'_,'_,'_> as EntityFluidTickWorld>::reset_fall_distance` | HEAD, cancela | a queda vale 0,5 vezes a de antes em vez de zerar (`Entity.baseTick`, `fallDistanceModifier`) |
| `fluid_entity_touch` | `motion::fluids::<MotionWorld<'_,'_,'_> as EntityFluidTickWorld>::set_was_touching_water` | HEAD, cancela | `wasTouchingWater` fica falso (o fogo não apaga; era a `fire_before`) |
| `fluid_entity_travel` | `motion::MotionWorld<'_,'_,'_>::player_travel` | HEAD, cancela | o jogador num fluido de mod que não é água nem lava não se move (`LivingEntity.travelInFluid`: `FluidType.move` falso e nenhum ramo de água ou lava), com a direção de nado do `Player.travel` |
| `fluid_entity_mob_travel` | `mobs::living::Mob::travel` | HEAD, cancela | o mesmo para o mob que não voa |
| `fluid_entity_mob_update` | `mobs::living::Mob::update_fluids` | HEAD | o mob não passa pelo `FluidResources::fluid` (lê a tabela de fatos): a correnteza de 0,014, a queda por 0,5 e a marca de olhos no fluido |
| `fluid_entity_mob_air` | `breathing::breathe` | RETURN | o `LivingBreatheEvent` do mob com os olhos no fluido de mod: não respira e não enche (`canDrown`) |

- **Hook mais próximo:** nenhum; H47 (o motor aceitar o fluido de mod com o `FluidType`: `isWaterLike`, `canSwim`, `canDrown`, `canExtinguish`, `motionScale`, `move`), e H106 abaixo, que pede os pontos de entrada de movimento e de ar por fluido.
- **Helpers:** `mods/machinery_basis/fluid_entity_*.rs`. O `alvo` do mob usa a marca por thread de `water_like.rs` (`set_mob_eye`), porque `breathe` só recebe o id e o tipo, e o `base_tick` do mob (`mobs/ai.rs`) não aceita `MODIFY_VARIABLE` (`E_UNSUPPORTED_MACRO`, a macro `identity`).
- **Risco:** o jogador lê o `PlayerMotion` e o `MotionWorld` (campos `player`, `resources.fluids`, `fluids.was_touching_water`, `fall_distance`); o mob lê `Mob`, `MobWorld`, `fluid_currents` e `Pushed` (privados de `mobs/living.rs`). O estado por thread vale porque `update_fluids` e o que ele chama correm na mesma thread, sem outra entidade no meio. Não cobre: o item (vale a água emprestada), a espécie com `travel` próprio (cai no hold do mob, menos as que voam), o pulo dentro do fluido (`jumpInFluid`, só soma velocidade que não aparece parada) e a sobreposição e a névoa de dentro.

### `logged-fluid-row` (machinery_basis)

- **Estado:** em uso. **Alvo:** `horizon_client_lib::mod_content::set_state_properties`, HEAD (`mod_content.rs:451`, chamada em
  `:1272`).
- **Helper:** `mods/machinery_basis/logged_fluid_row.rs`.
- **Por que é necessária:** o bloco com `fluid_logged=lava` tem de ser fonte de lava para o motor de fluidos, com luz 15
  (`IStateFluidLoggable.getFluid` e `getFluidLightLevel`, este em `IStateFluidLoggable.java:62`; `BlockMekanism.getFluidState`,
  `BlockMekanism.java:125-130`). O Horizon só reconhece fluido em bloco para água.
- **Hook mais próximo:** nenhum. O contrato de `state_properties` declara a propriedade, mas não diz que valor dela é fluido. O
  helper reescreve a linha da tabela de fatos (`facts.json`): os campos 2, 3 e 4 do vetor (tipo do fluido, nível 8, e um bool), e
  `emission` 15 no formato de objeto. Isso depende do formato posicional da linha.
- **Hook proposto:** H3.
- **Risco:** o formato da linha é interno. Mudar a ordem dos campos faz o helper sair sem efeito (ele só age se `parts[4]` for
  `Bool`), e o bloco volta a não ter fluido, sem erro. A função só roda com `state_properties` declarado e não recebe o id do bloco.
  Hoje o `block_entity_submit` desenha a lava pela block entity porque o mesher lê `FluidState::from_block`, que nenhuma injeção
  alcança: H3 acabaria com esse desvio visual também.

### `widget_tooltip` (mekanism)

- **Estado:** em uso. **Alvo:** `horizon_client::game::native_hooks::Game<'_>::draw_native_screen_tooltip`, HEAD, cancelável
  (`native_hooks.rs:10131`). É o mesmo alvo do `widget_tooltip` de `mekanismgenerators`.
- **Helper:** `mods/mekanism/native/widget_tooltip.rs`. Lê `game.carried`, `game.cursor` e `game.native_screen_at()`, todos
  privados. A glue (`mods/mekanism/native/glue.rs`, `widget_tooltip`) chama `machinery_basis::menu_tooltip`, que usa
  `game.gui.atlas`, `Game::tooltip_geometry`, `game.tooltip_box` e `Game::tooltip_text`.
- **Por que é necessária:** cobre quatro widgets: a aba de avisos, a de xp, a barra de energia e a barra de químico. A aba de xp
  (`GuiXpContainmentTab`, `setTooltip(MekanismLang.XP_CONTAINMENT_TAB)`, `GuiXpContainmentTab.java:33`) é um `Tooltip.create` ao lado do
  botão, escondido pela janela de upgrades quando ela cobre o ponto (`isMouseOverCheckWindows`). `GuiWarningTab.updateTooltip`
  (`GuiWarningTab.java:45-61`) põe o cabeçalho em `EnumColor.YELLOW` (0xFFDD4F). A barra de energia e a de químico seguem
  `GuiBar.updateTooltip` (`GuiBar.java:96`), e a de químico é um `GuiTankBar` (`GuiChemicalBar.java:26`). Nos três, o tooltip fica
  por cima ou por baixo do widget, e não sobre o cursor.
- **Hook mais próximo:** `screen_tooltip` (`collect`, `lines: &mut Vec<String>`). Entrega só texto com os códigos `§`, que têm 16
  cores e não 0xFFDD4F, e `draw_native_screen_tooltip` sempre posiciona no cursor. O Horizon já converte as linhas em
  `Vec<(Text, u32)>` por dentro: o formato que falta existe, só não é exposto. O Mekanism deixou de usar `screen_tooltip` nesta
  mudança, porque a posição é o que o hook não dá.
- **Hook proposto:** H4.
- **Risco:** a injeção depende de quatro funções de desenho e de três campos de `Game`. Mudar o tema do tooltip, a ordem de
  parâmetros de `tooltip_geometry` ou o nome de `native_screen_at` quebra o build. Um ajuste no desenho do tooltip muda o
  resultado sem erro.

### `widget_tooltip` (mekanismgenerators)

- **Estado:** em uso. **Alvo:** o mesmo de `mekanism`: `horizon_client::game::native_hooks::Game<'_>::draw_native_screen_tooltip`,
  HEAD, cancelável (`native_hooks.rs:10131`).
- **Helper:** `mods/mekanismgenerators/native/widget_tooltip.rs`, que chama `mekanismgenerators_glue::widget_tooltip`
  (`glue.rs:294`). Lê os mesmos campos privados de `Game` que o do Mekanism.
- **Por que é necessária:** o Heat Generator tem a barra de energia, `GuiVerticalPowerBar` (`GuiHeatGenerator.java:41`, um
  `GuiBar`, `GuiBar.java:96`), e o medidor de lava, `GuiFluidGauge` (`GuiHeatGenerator.java:40`, que estende `GuiGauge`,
  `GuiGauge.java:130`). Os dois têm tooltip próprio no original, e o `native_screen_tooltip` do Horizon põe o texto sempre sobre o
  cursor, sem posição do widget.
- **Hook mais próximo:** `screen_tooltip`, o mesmo do `mekanism`: sem posição e sem cor por linha.
- **Hook proposto:** H4, o mesmo que cobre o `widget_tooltip` do `mekanism`.
- **Risco:** a mesma causa do `mekanism`: `carried`, `cursor`, `native_screen_at` e `draw_native_screen_tooltip` são privados. Um
  nome que mude quebra o build, e um ajuste no desenho do tooltip muda o resultado sem erro.

### `component_item_model` (mekanism)

- **Estado:** em uso. **Alvo:** `horizon_assets::item_model::foreign::decode_node`, HEAD, cancelável (`foreign.rs:52`).
- **Helper:** `mods/mekanism/native/component_item_model.rs`.
- **Por que é necessária:** o item do upgrade usa o modelo `mekanism:component_item`
  (`ClientRegistration.registerItemModels`, `ClientRegistration.java:420`; `ComponentItemModel.Unbaked`,
  `ComponentItemModel.java:95-117`), que escolhe o modelo pelo componente. O arquivo do item é o do original; sem a injeção o
  Horizon cai no modelo base, sem textura.
- **Hook mais próximo:** `item_model_property` (`listen`, cliente), que dá uma condição booleana por pilha e serve a
  `minecraft:condition`. Não decide o tipo do nó, e o texto dele diz que renderers especiais próprios não entram. A injeção monta o
  `select` sobre o componente à mão porque `minecraft:select` e `has_component` só aceitam componentes vanilla (nota
  `horizon-regra-de-slot-injetada` do ai-memory).
- **Hook proposto:** H6, e H5 para o `select` aceitar o componente do mod.
- **Risco:** a assinatura de `decode_node` e o enum `ItemModelNode` (`Select`, `SelectProperty::Component`, `ModelLeaf`) são
  internos. A injeção constrói os nós à mão: um campo novo em `ModelLeaf` quebra o build.

### `item_component_type` (mekanism)

- **Estado:** em uso desde 2026-11-09. **Alvo:** `horizon_assets::item_model::component_type`, HEAD, cancelável (`item_model.rs:277`, função livre que valida o `component` de um `select`).
- **Helper:** `mods/mekanism/native/item_component_type.rs`: um id `mekanism:*` passa, qualquer outro volta ao caminho normal.
- **Por que é necessária:** o item do Configurator escolhe o modelo pelo componente `mekanism:configurator_mode` (`items/configurator.json` da datagen: `minecraft:select`
  com a propriedade `minecraft:component`, os casos `empty`, `rotate` e `wrench` e o `fallback` do modo padrão). `component_type` só aceita os 122 componentes do vanilla
  (`DATA_COMPONENT_TYPES`), então o Horizon descartava o arquivo do item inteiro (`item definition mekanism:configurator not loaded: unknown data component type`) e o item aparecia
  sem modelo no inventário e na aba criativa. Com a injeção o arquivo é o do original; enquanto o port não tem o componente (função do Configurator, Fase 2), nenhuma pilha o traz,
  nenhum caso bate e vale o `fallback`, que é o modelo do modo padrão, como no original.
- **Hook mais próximo:** `component_types` (H5) registra o tipo para a pilha e o NBT, mas o `select` do arquivo de item e o `has_component` só olham a lista vanilla; o `item_model_property`
  só dá uma condição booleana.
- **Hook proposto:** H5 (a lista de componentes de mod valer também nos arquivos de item), a mesma que elimina `command_components`.
- **Risco:** renomear ou mudar a assinatura de `component_type` quebra o build. Trocar o `select` por outro nó que valide o componente em outro ponto deixa o item sem modelo outra vez, sem erro.
- **Conferido em jogo** (2026-11-09, `horizon`): o log perde a linha `item definition mekanism:configurator not loaded`, e o item aparece com o modelo `mekanism:item/configurator` na aba criativa (prints em `~/.cache/rust-mekanism/wa/itens-equipamento/`).

### `mek_data_alias` (mekanism)

- **Estado:** em uso desde 2026-11-11. **Alvo:** `horizon_client_lib::crafting::Recipes::from_sources` (`crafting.rs`), HEAD, com o `sources` (`Vec<(String, Value)>`, os arquivos de receita já lidos, o do jogo e os dos mods e data packs). **Proposta:** H79.
- **Java:** `MekanismRecipeSerializersInternal.java:125` (`mek_data`), `MekanismShapedRecipe.java` (`assemble`), `WrappedShapedRecipe.java` (`matches`).
- **Helper:** `mods/mekanism/native/mek_data_alias.rs`: troca o `type` de cada receita `mekanism:mek_data` por `minecraft:crafting_shaped`, antes de a carga separar as receitas.
- **Por que é necessária:** `mekanism:mek_data` (`MekanismShapedRecipe`, `WrappedShapedRecipe`) é uma receita de mesa com o formato exato da `crafting_shaped` (`key`, `pattern`, `result`, `group`), mas `custom_type` manda todo tipo fora de `minecraft:` para a tabela `custom`, que só `native_recipes_of_type` lê: a mesa de crafting nunca a vê. São 86 receitas (85 do Mekanism e a do Wind Generator): os depósitos, tanques, cubos de energia e células de indução acima do básico, as fábricas, as máquinas que levam outra máquina como entrada e 12 itens de equipamento. Com a troca o arquivo continua o do original, byte a byte, e um data pack do Mekanism que a sobrescreva também vale.
- **O que a `assemble` do original copia:** a energia, os tanques, os itens, os upgrades, a segurança, a trava, a ordenação e o drive do QIO (`RecipeUpgradeData`), pela entrada. O port copia hoje a energia (2026-11-11, frente `energia-item`: as duas injeções `mek_data_energy_grid` e `mek_data_energy_use` e o módulo `mek-data-energy`, abaixo); os outros dados esperam o item guardá-los (Fase 5, segurança; Fase 6, tanques e depósito; Fase 8, o QIO). A regra de cada tipo está em `machinery_basis_core::recipe_data`, testada contra o rastro (`mek_data_assemble.jsonl`).
- **Hook mais próximo:** `prepare_item_craft` (`replace`), que só roda no menu do convidado de LAN (`lan_crafting.rs`) e não dá a grade. **Hook proposto:** H79: uma tabela de aliases de tipo de receita (`recipe_type_alias`: o tipo do mod que se lê como `crafting_shaped` ou `crafting_shapeless`), e um hook `craft_assemble` (`replace`) chamado por `find_with` com o id da receita, a grade e o resultado, para todos os caminhos da mesa. Com os dois a injeção sai.
- **Risco:** renomear `from_sources` ou mudar o tipo de `sources` quebra o build. Se o Horizon passar a rejeitar na carga o tipo `mekanism:mek_data` antes de `from_sources` (por tipo não registrado), as 86 receitas somem sem erro.
- **Conferido em jogo** (2026-11-11, `horizon`, build `native-f319701-e077180adb3d`): as 86 receitas montadas uma a uma na mesa (`horizon-ctl click-slot`) dão o item do `result` (86 de 86); prints em `~/.cache/rust-mekanism/wa/mek-data/`.

### `mek_data_energy_grid`, `mek_data_energy_use` e o módulo `mek-data-energy` (mekanism)

- **Estado:** em uso desde 2026-11-11 (frente `energia-item`). **Alvo:** `horizon_client_lib::crafting::Recipes::find_with` (`crafting.rs`), dois HEAD, o segundo cancelável. **Proposta:** H79 (o hook `craft_assemble`).
- **Java:** `MekanismShapedRecipe.assemble` (`MekanismShapedRecipe.java`), `EnergyRecipeData` (`merge` e `applyToStack`), `RecipeUpgradeData.getSupportedTypes`.
- **Helpers:** `mods/mekanism/native/mek_data_energy_stash.rs`, `mek_data_energy_hook.rs` e o módulo `mek_data_energy.rs` (em `crafting.rs`, `visibility = "pub"`): a `mek_data_alias` anota o nome de cada receita `mek_data`, o glue diz a capacidade de cada item que guarda energia (`set_capacity`, a cada leitura de `gear.toml`, `tiers.toml` e `machine-usage.toml`), e a energia das entradas é somada (saturada) e posta no resultado até a capacidade dele, o excesso se perde.
- **Por que é injeção:** o Horizon monta o resultado de uma receita de mesa só com o modelo dela (`Template::create`), sem a grade. O ponto natural, o `result` local de `find_with` (`MODIFY_VARIABLE`) é um padrão `if let Some(result)`, que o `horizon-mixins` recusa (`E_UNSUPPORTED_LOCAL`), e um `RETURN` de `find_with` ou de `assemble_if_matches` também (fechamentos no corpo: `E_UNSUPPORTED_CONTROL_FLOW`). Sobrou o HEAD cancelável: o primeiro HEAD guarda a grade se ela tem algum item com energia (`stash`), e o segundo, só com a grade guardada, roda a `find_with` inteira por dentro e devolve o resultado completado. O helper cancelável não pode ter o tempo de vida como parâmetro (`E_INVALID_HELPER`) e o binding só aceita parâmetro ou empréstimo (`E_INVALID_BINDING`), por isso `recipes` é a única referência dele e a grade vem do primeiro HEAD. Custo: uma grade com item de energia roda a busca duas vezes, e a escala de mapa (`map_scale`) não vale nessa segunda (uma grade com energia nunca é a do mapa que se estende).
- **Hook mais próximo:** `prepare_item_craft`, que só roda no convidado de LAN e não dá a grade. **Hook proposto:** H79 (`craft_assemble`, `replace`, com o id da receita, a grade e o resultado, para todos os caminhos da mesa): as duas injeções e o módulo saem.
- **Risco:** renomear `find_with`, mudar a assinatura dele ou o tipo do retorno quebra o build; os dois HEAD rodam na ordem alfabética do id, não na do arquivo (medido em jogo: a troca de lugar no `horizon-mixin.toml` não mudou o código gerado): por isso `mek_data_energy_grid` (que guarda a grade) vem antes de `mek_data_energy_use` (que a lê); na ordem trocada o segundo lê a grade da chamada anterior e a mesa deixa de dar resultado.
- **Conferido em jogo** (2026-11-11, `horizon`): ver o plano (frente `energia-item`).

### `command_components` (mekanism)

- **Estado:** em uso. **Alvo:** `horizon_command::item::ItemState<'_,'_>::component_type`, HEAD, cancelável (`item.rs:160`).
- **Helper:** `mods/mekanism/native/command_components.rs`.
- **Por que é necessária:** os componentes `mekanism:upgrade_type` (`MekanismDataComponents.java:226`), `mekanism:side_config` e
  `mekanism:ejector` (a configuração de lados e o ejetor, desde 2026-11-09, frente `config-lados`) são persistentes e valem em
  `/give mekanism:upgrade[mekanism:upgrade_type=...]` e no item de uma máquina. `component_type` só aceita o que está em `reg.persistent_components`, a lista
  que o jogo exporta, e o hook `component_types` não entra nela.
- **Hook mais próximo:** `component_types` (`collect`, `types: &mut Vec<String>`). Registra o tipo para a pilha e o NBT, mas o
  comando, `minecraft:select` e `has_component` só olham a lista vanilla.
- **Hook proposto:** H5.
- **Risco:** o helper lê `state.reader.cursor` e chama `super::args::read_identifier`: privados de `horizon-command`. Mudar o
  leitor de argumentos quebra o build.

### `slot_highlight` (mekanism)

- **Estado:** em uso. **Alvo:** `horizon_client::game::Game<'_>::draw_slot_highlight`, HEAD, cancelável (`game.rs:9365`, método
  privado de `impl Game`).
- **Helper:** `mods/mekanism/native/slot_highlight.rs`. Só age na camada da frente (`front`) e só numa tela de bloco `mekanism*`.
- **Por que é necessária:** `VirtualSlotContainerScreen.extractSlotHighlightFront` chama `super.extractSlotHighlightBack` (linha 74),
  e não `Front`: o realce da frente sai com o sprite de trás (`VirtualSlotContainerScreen.java:70-75`). O vanilla não faz isso, e o
  original faz, então o port faz também.
- **Hook mais próximo:** nenhum do catálogo troca o realce do slot. `screen_draw_over_slots` (H11) pinta por cima, mas não troca o
  desenho padrão do realce.
- **Hook proposto:** H21.
- **Risco:** o helper chama `game.draw_slot_highlight(batch, false, x, y)` de dentro da própria injeção; ele não recursa só porque
  sai no `if !front`. Mudar a assinatura de `draw_slot_highlight` ou a ordem das camadas quebra o build, ou faz o realce sair certo
  só numa das camadas, sem erro.

### `active_light` (mekanism)

- **Estado:** em uso desde 2026-11-07; ampliada em 2026-11-09 (frente `blocos-maquinas`) e outra vez (frente `blocos-estrutura`, só a tabela). **Alvo:** `horizon_client_lib::mod_content::Plan<'_>::override_gameplay`, HEAD
  (`mod_content.rs:1319`, método privado do plano de conteúdo, que recebe o `NewBlock`, a combinação das propriedades de estado próprias `combo` e a linha `J` da tabela de jogo de cada estado).
- **Helper:** `mods/mekanism/native/active_light.rs`. Dois usos. (1) O Crusher, a Enrichment Chamber, o Energized Smelter e o Metallurgic Infuser, e só nos estados em que a linha já emite luz (o `lit=true` do gabarito `minecraft:furnace`, 13): troca por 8.
  (2) Os 84 blocos de máquina e blocos avulsos com tile e os 62 de armazenamento, indução, multibloco e transmissores (gabarito `minecraft:lodestone`, estados `active` e `fluid_logged` próprios, ou o `minecraft:chorus_plant` dos 18 transmissores novos e dos 4 cabos, sem estado próprio): a tabela `mekanism_rows` (módulo `block-rows`, `native/block_rows_table.rs`, gerada de `rastros/blocos.jsonl`, o original em jogo, por `dev/referencia/blocos_conteudo.py`, e conferida
  estado a estado por `tests/blocos_registry.rs`) diz a luz do `active` (8, 13 ou 15), a da lava (15), a reação ao pistão, `can_occlude`, `solid_render`, `dampening`, `skylight_down` (com a oclusão do estado com fluido dentro) e a resistência negativa dos três blocos de `AttributeCustomResistance(-1)`, que a injeção escreve na linha de cada estado.
- **Por que é necessária:** as quatro são `Machine` (`Machine.java:26`), com `Attributes.ACTIVE_LIGHT`, `AttributeStateActive(8)`
  (`Attributes.java:27`): luz 8 quando ativas e 0 quando não (`AttributeStateActive.java:49-51`, que soma ao 0 do bloco pela
  `BlockStateHelper.applyLightLevelAdjustments`, `BlockStateHelper.java:83-94`). O gabarito dá 13 ao estado aceso, e
  `properties.light` é um inteiro só, aplicado a todos os estados (`mod_content.rs:1345`): com `light: 8`, a máquina desligada
  também ficaria acesa. O Heat Generator usa `ACTIVE_MELT_LIGHT`, 13 (`GeneratorsBlockTypes.java:68`), o mesmo do gabarito, e não
  passa por aqui. Os blocos novos: a luz tem a mesma causa; a reação ao pistão (`BlockMekanism.getPistonPushReaction`, `IMMOVEABLE` com bloco de entidade) e a oclusão de uma forma menor que o cubo (`isSolidRender` falso, `getLightDampening` 0, `propagatesSkylightDown`) são colunas da linha de jogo que `properties` não alcança
  (`BlockProperties` só tem `hardness`, `explosion_resistance`, `light`, `requires_tool`, `friction`, `map_color`, `sounds`, `random_ticks`, `render_shape` e `occludes`, e `occludes: false` põe `can_occlude` falso, que o original não tem), e a resistência negativa o arquivo recusa (`content.rs`, `explosion_resistance is 0 or more`).
- **Hook mais próximo:** nenhum. `random_ticks` já aceita condição por estado, `light` não.
- **Hook proposto:** H18 (a luz) e H49 (o resto), dados sem hook.
- **Risco:** depende de `NewBlock` (`decl.id`), do `combo` e do formato de linha (`J`, chaves `emission`, `push`, `can_occlude`, `solid_render`, `dampening`, `skylight_down` e `explosion_resistance`), privados de `mod_content.rs`. Renomear
  o método ou a chave quebra o build, ou deixa a máquina com a luz do gabarito, sem erro.
- **Conferido em jogo** (2026-11-07, `horizon`, `horizon-ctl light` no bloco e no vizinho): as quatro máquinas acesas dão 8 e 7, a
  Enrichment Chamber apagada 0 e 0, o Heat Generator aceso 13 e 12, e o `minecraft:furnace` aceso 13 e 12. Os blocos novos (2026-11-09, `horizon`, `horizon-ctl light` na célula do bloco): 159 estados, todos os que acendem no original dão a luz dele (o `combiner` ativo 8, o `resistive_heater` 13, o `industrial_alarm` 15, o `teleporter` 3); o bloco que não acende lê o resto de luz de um vizinho apagado pouco antes, que some com o tempo (`tests/blocos_registry.rs` confere as 1.775 linhas sem o jogo).

### `context_outline` (mekanism)

- **Estado:** em uso desde 2026-11-11 (frente `configurator`, decisão 1 e 46). **Alvo:** `horizon_client::game::Game<'_>::context_outline` (`game.rs:3491`), HEAD cancelável.
- **Helper:** `mods/mekanism/native/context_outline.rs` chama `mekanism_configurator::partial_outline`: com o Configurator em uma das mãos (`CollisionContext.isHoldingItem`) e um transmissor do Mekanism na mira, devolve só a caixa que o olhar toca (o raio de `aim()`, com o alcance de `reach()`, contra as caixas da forma do estado) ou o centro; qualquer outro caso devolve `Continue`.
- **Por que é necessária:** `BlockTransmitter.getShape` (`BlockTransmitter.java:84-103`) devolve, com o Configurator na mão, só o pedaço do transmissor que o jogador olha (`MultipartUtils.collisionRayTrace` sobre `TileEntityTransmitter.getCollisionBoxes`), para o Configurator poder escolher um lado. O Horizon usa `context_outline` para as formas que dependem do contexto (a luz e o andaime com o próprio item na mão, o shulker), e o contorno e o raio de seleção (`outline_boxes`) leem a mesma caixa, então a injeção cobre o desenho e a mira, como o `getShape` do original.
- **Hook mais próximo:** `block_highlight` só cancela o contorno; `visuals` v1 só desenha o contorno de uma forma. **Hook proposto:** H71 `block_shape_context` (uma caixa ou lista de caixas por jogador e bloco, lida pelo contorno e pelo raio).
- **Risco:** renomear `context_outline` ou mudar o tipo `Option<[f64; 6]>`. Uma caixa só por seleção: com PUSH e PULL (Fase 3, `ConnectionType`) o grupo tem três caixas, e a forma delas pede a lista (H71).

### `outline_boxes` (mekanism)

- **Estado:** em uso desde 2026-11-11 (frente `cubo-forma`, H92). **Alvo:** `horizon_client::game::Game<'_>::outline_boxes` (`game.rs:3521`), HEAD cancelável.
- **Helper:** `mods/mekanism/native/outline_boxes.rs` chama `mekanism_cube::outline_boxes`: para um cubo de energia devolve as caixas de `BlockEnergyCube.getShape` do cubo posto (o quadro, girado pelo `facing`, e a placa de cada lado habilitado; a regra pura é `cube_shape` em `runtime.rs`, conferida contra o rastro `energy_cube_shapes.jsonl` do original); qualquer outro bloco devolve `Continue`.
- **Por que é necessária:** `getShape` lê o tile (`isSideEnabled`) e o Horizon só guarda uma forma por estado de bloco. `outline_boxes` é onde o contorno e o raio de seleção leem a forma, e `context_outline` (H71) só devolve uma caixa. A colisão do jogador segue a forma por estado (as seis placas): o Horizon não tem ponto equivalente por instância.
- **Hook mais próximo:** `block_shape_context` (H71) com lista de caixas. **Hook proposto:** H92, uma forma por instância lida pela seleção e pela colisão.
- **Risco:** renomear `outline_boxes` ou mudar o tipo `Vec<[f64; 6]>`; e o cubo sem a cópia do tile no cliente cai no índice "norte, tudo habilitado" do original.

### `use_hand` e `predict_item_on_block` (mekanism)

- **Estado:** em uso desde 2026-11-12 (frente `item-antes-do-bloco`, decisão 47, H72). Trocam o hook `right_click_block` (hook do catálogo com estrutura privada), que cancelava o clique e perdia o resultado do original. **Alvos:** `horizon_client::game::Game<'_>::use_hand` (`game.rs`, o turno de uma mão no despacho de uso do bloco, no servidor e no servidor integrado) e `horizon_client::game::remote::Game<'_>::predict_item_on_block` (`remote.rs`, a previsão do `Item.useOn` no cliente de um convidado de LAN), os dois HEAD cancelável.
- **Helpers:** `mods/mekanism/native/use_hand.rs` e `predict_item_on_block.rs` chamam `mekanism_configurator::use_hand` e `predict_item_on_block`. Num bloco de Mekanism, no turno da mão principal, `mekanism_rules::tile_use` (conferida contra os rastros `configurator_click_*.jsonl` e `configurator_card.jsonl`) diz o que o bloco faz antes do item: o Configurator em `ROTATE` gira o bloco (`SUCCESS`); o cartão passa a vez da tela e o `Item.useOn` dele tem o clique (`SUCCESS_SERVER`, ou `PASS` sem dados); agachado `openGui` passa; o resto abre a tela. O helper devolve `Break(Ok(true))` (agiu: a mão balança para todo jogador e `ITEM_USED_ON_BLOCK` dispara), `Break(Ok(false))` (`PASS`: a outra mão tem a vez) ou `Continue` (o despacho segue; o cubo de energia abre o menu aqui, H93). A segunda injeção devolve `true` para o cartão num bloco de Mekanism: o `useOn` dele consome o clique no cliente, e o cliente do Horizon, que só prevê os itens dele, mandava o `use` do mesmo clique, que esvaziava o cartão (o `card_just_used` que existia saiu).
- **Por que são necessárias:** o despacho do Horizon abre a tela do bloco antes de o item agir (`use_item_on` roda depois do bloco), e o original deixa o item passar à frente (`TileEntityMekanism.openGui`, `:565-587`, devolve `TRY_WITH_EMPTY_HAND`). O hook `right_click_block` só deixa cancelar: perde o resultado (`SUCCESS` ou `PASS`), não balança a mão do convidado e marca `use_failed`. **Medido em jogo com dois clientes** (2026-11-12, `horizon2`): sem a segunda injeção o convidado que copia o cartão agachado o vê esvaziado um instante depois (os `use` duplicados do cliente), na máquina e no cubo; com ela o cartão segue com os dados.
- **Hook mais próximo:** `right_click_block` (cancela, sem resultado) e `use_item_on` (depois do bloco). **Hook proposto:** H72 `item_use_first`, antes do uso do bloco, com o item da mão principal e a face, devolvendo o resultado do clique; e, para o cliente, que um item de mod declare por dados a previsão do `useOn` (`SUCCESS_SERVER` em um bloco com a capability).
- **Risco:** renomear `use_hand` ou mudar `Option<&Hit>` e o `Result<bool>` (o helper tem de devolver `ControlFlow<Result<bool>>`, com o `Result` do módulo); renomear `predict_item_on_block` ou o `&Hit`; e o campo privado `use_failed`.

### `world_overlay` (mekanism)

- **Estado:** em uso desde 2026-11-11 (frente `efeitos-configurator`, decisão 62). **Alvo:** `horizon_client::game::world_primitives::Game<'_>::world_primitive_geometry`, HEAD cancelável, mais o módulo `world-overlay` (`overlay_quads.rs`, enxertado em `game/world_primitives.rs`, alcança os campos privados de `PrimitiveGeometry`).
- **Helper:** `mods/mekanism/native/world_overlay.rs` chama `mekanism_overlay::geometry`: com o Configurator numa das mãos, num modo de configurar, olhando para uma máquina que aceita o tipo e tem o lado ligado (`mekanism_configurator::overlay_request`), devolve os primitivos do quadro com um quadrilátero (o sprite `mekanism:gui/sprites/overlay/<tipo>_overlay`, a cor do `DataType` a 60 % de opacidade, a face deslocada 0,01 para fora, com a rotação do `ConfiguratorOverlayHandler` nas faces de cima e de baixo); qualquer outro caso devolve `Continue`. Se outro mod desenhou primitivos no quadro, também `Continue` (o jogo vence, sem sobreposição naquele quadro).
- **Por que é necessária:** `ConfiguratorOverlayHandler` (`RenderTickHandler.onBlockHover`, `RenderTypes.eyes`) desenha na face que o olhar toca um quadrilátero com textura, com teste de profundidade, translúcido e sem escrever profundidade. Os primitivos do mundo do Horizon (`PrimitiveShape`) só têm linha, caixa, caixa cheia, feixe e texto; um quadrilátero com textura não cabe em nenhum.
- **Hook mais próximo:** `native_world_primitives` (os primitivos) sem forma de quadrilátero; `world_drawn` (um passe próprio do wgpu) serviria, mas pede um pipeline inteiro por um quadrilátero. **Hook proposto:** H87, `PrimitiveShape::TexturedQuad { corners, sprite, color }`.
- **Risco:** renomear `world_primitive_geometry` ou mudar o `PrimitiveGeometry` (`quads: [Vec<Vertex>; 2]`) ou o `Vertex`. O corpo tem `return`, então RETURN não serve: o HEAD cancelável substitui o conjunto do quadro só nos quadros em que a sobreposição aparece.

### `occlusion_shapes` (machinery_basis)

- **Estado:** em uso desde 2026-11-12 (frente `oclusao`, decisão 22). **Alvo:** `horizon_client_lib::mod_content::tables` (`mod_content.rs:2385`, função livre que escreve as tabelas JSON e NBT do overlay do conteúdo), HEAD cancelável.
- **Helper:** `mods/machinery_basis/occlusion_shapes.rs`. O RETURN não aceita o `?` do corpo de `tables`, então o HEAD cancelável a chama de novo por dentro, com uma marca de reentrada (`thread_local`): a segunda passagem segue para o corpo original e escreve as tabelas todas;
  na volta, o helper lê `occlusion.nbt` do overlay (`Overlay::out`), troca o índice de forma de cada estado dos blocos com `shapes` pela forma declarada (a mesma codificação de `Plan::collision_shapes`, `encode_shape`) e a reescreve; nos canos e cabos (`thickness`), a forma é a do centro (`pipe_shape` sem lados) e `render-definitions.json` ganha `can_occlude` verdadeiro.
- **Por que é necessária:** o vanilla esconde a face do vizinho pela forma de oclusão (`BlockState.getOcclusionShape`, a de seleção no `Block` comum; a do centro no `BlockTransmitter`), e o mesher do Horizon lê `occlusion.nbt`. `Plan::nbt` copia para o bloco de mod a forma do gabarito (o cubo cheio do `lodestone`), e o mod não tem como dizer a dele:
  com `can_occlude` verdadeiro a face do chão sob uma máquina de forma menor sumia, e com `occludes: false` (a saída da decisão 22 até 2026-11-12) o vizinho desenhava faces que o original esconde.
- **Hook mais próximo:** nenhum. **Hook proposto:** H51 (a forma de oclusão do bloco, como dado: o mesmo `shapes` ou um `occlusion_shapes` próprio).
- **Risco:** depende de `tables(base, plan, overlay)` (nome e argumentos), de `Plan::blocks`, de `NewBlock::{shapes, rename, order, decl}`, de `Overlay::{out, write}`, de `encode_shape`, de `occlusion.nbt` ser um compound plano com `shapes` e `cached_shapes`, e de `render-definitions.json` ter uma entrada por bloco. Se a assinatura de `tables` mudar, o build quebra; se o formato mudar, o jogo recusa a tabela ao abrir (erro, não silêncio).

### `glass_ids` (mekanismgenerators)

- **Estado:** em uso desde 2026-11-12 (frente `oclusao`, decisão 30). **Alvo:** `horizon_world::block_render::FaceVisibility::new`, HEAD (`block_render.rs:65`, constrói a tabela de visibilidade das faces com o registro de blocos).
- **Helper:** `mods/mekanismgenerators/native/glass_ids.rs`. Anota no módulo `glass-skip` o estado padrão de `mekanism:structural_glass`, `mekanismgenerators:reactor_glass` e `mekanismgenerators:laser_focus_matrix`, pelo `registry()` do `CachedCollisionRegistry` (`MISSING` se o bloco não está no registro). O RETURN não aceita o `?` de `new`.
- **Por que é necessária:** `FaceVisibility::skips` só vê o estado padrão de cada bloco (`State::block`), não o nome; o registro só existe em `new`.
- **Hook mais próximo:** nenhum. **Hook proposto:** H51.
- **Risco:** depende de `new` receber `&CachedCollisionRegistry`, de `registry().default_state(&str)` e de `BlockStateId(pub u32)`.

### `glass_skip` (mekanismgenerators)

- **Estado:** em uso desde 2026-11-12 (frente `oclusao`, decisão 30). **Alvo:** `horizon_world::block_render::FaceVisibility::skips` (`block_render.rs:157`, a regra `skipRendering` por face, `fn` associada sem `self`), HEAD cancelável.
- **Helper:** `mods/mekanismgenerators/native/glass_skip.rs`, com a tabela do módulo `glass-skip`. O original: `BlockTileGlass.skipRendering` (o vizinho é um `BlockTileGlass`: o vidro estrutural e o do reator), `BlockReactorGlass.skipRendering` (isso ou o vizinho é um `BlockLaserFocusMatrix`) e `BlockLaserFocusMatrix.skipRendering` (o vizinho é vidro do reator ou matriz): sete pares, conferidos contra o original inteiro (`rastros/blocos_oclusao_pular.jsonl`, 42.436 pares).
- **Por que é necessária:** a regra `same_block` do Horizon só pula a face entre estados do mesmo bloco; o vidro do reator ao lado da matriz de foco (e o vidro estrutural ao lado do do reator) desenhava uma linha de 1 a 2 pixels.
- **Hook mais próximo:** nenhum. **Hook proposto:** H51 (uma regra de corte de face entre blocos que o mod declare, por tag ou por lista).
- **Risco:** depende de `skips(a: State, b: State, ..)` e do campo `State::block`.

### `shape_limit` (mekanism)

- **Estado:** em uso desde 2026-11-09 (frente `blocos-maquinas`). **Alvo:** `horizon_mods::content::BlockShapes::check` (`horizon-mods/src/content.rs:165-195`), HEAD cancelável, no crate `horizon-mods`, que o launcher e o cliente compartilham (o mixin o alcançou: compilou à primeira).
- **Helper:** `mods/mekanism/native/shape_limit.rs`. Só entra quando uma forma passa de 16 caixas; abaixo disso devolve `Continue` e o Horizon confere como sempre. Passando, refaz as outras regras de `check` (propriedades distintas, uma por valor, caixas finitas e ordenadas) e devolve `Break(Ok(()))` ou o erro.
- **Por que é necessária:** `check` recusa mais de 16 caixas por forma (`boxes.len() > 16`), e o arquivo de bloco que a tem é pulado inteiro. O original tem blocos de máquina com formas de até 47 caixas, depois de cobertas por poucas caixas que se sobrepõem (o `digital_miner`): doze blocos, decisão 20 do plano. `Plan::states` e `shape_edges` somam as caixas que vierem, sem limite.
- **Hook mais próximo:** nenhum. **Hook proposto:** H50.
- **Risco:** depende de `BlockShapes::check` e dos campos públicos `property`, `properties`, `values` e `default`. Renomear o método ou mudar o limite quebra o build ou deixa a injeção sem efeito, sem erro (o bloco passaria de 16 e seria recusado pelo Horizon).
- **Conferido em jogo** (2026-11-09, `horizon`): os doze blocos carregam, e a forma de cada estado é a do original (`tests/blocos_registry.rs`, célula a célula contra o `VoxelShape.toAabbs()` do original).

### `note_instrument` (mekanism)

- **Estado:** em uso desde 2026-11-12 (frente `nota-instrumento`). **Alvo:** `horizon_client_lib::mod_content::J::rename` (`mod_content.rs:237`, método do JSON do plano de conteúdo, que `Plan::states` chama em cada linha copiada do gabarito: `row.rename(r, ids)`), RETURN, com `$value` do tipo `()` (primeiro binding, porque o corpo é um `match self` que se avalia dentro da chamada: com o `&mut *self` do helper antes, o build dá `E0499`). **Proposta:** H44.
- **Helper:** `mods/mekanism/native/note_instrument.rs`. Recebe a linha (`&mut J`) e o `Rename` (`r`; um binding só pode ser parâmetro do alvo ou o empréstimo dele, `r.to.as_str()` dá `E_INVALID_BINDING`). Quando `r.to` é `mekanism:block_salt` ou `mekanism:block_bio_fuel` e o nó é uma linha de estado (lista de nove ou mais valores, o primeiro um número) com `HARP` na posição 8, escreve `SNARE` ou `BANJO`. Qualquer outro nó (e o `rename` chama a si mesmo em cada nó do JSON) passa sem mudança.
- **Por que é necessária:** o original registra `block_salt` com `SNARE` e `block_bio_fuel` com `BANJO` (`MekanismBlocks.java:980-982`) e o Horizon não tem `instrument` em `properties`. O instrumento é a posição 8 da linha do estado de `facts.json` e de `block-behaviours.json` (`StateFacts::instrument`, `horizon-world/src/level/data.rs:882`), copiada do gabarito `lapis_block` (`HARP`). Os outros pontos não dão: `Plan::states` (HEAD cedo demais, RETURN barrado pelo `?`), `override_gameplay` (só a linha de jogo, sem instrumento) e `Sim::note_block_instrument` (`horizon-world/src/level/shapes.rs:1160`, onde o bloco de nota lê o de baixo; impl genérica, o alvo dá `E_TARGET_NOT_FOUND`).
- **Hook mais próximo:** nenhum. **Hook proposto:** H44 (`properties.instrument`).
- **Risco:** depende de a posição 8 da linha do estado ser o instrumento, de `Plan::states` renomear a linha com `J::rename` (e não só copiar), de o nome do método e do tipo `Rename` (campo `to`). Mudar qualquer um deixa a injeção sem efeito sem erro (o bloco volta a `HARP`: o teste `the_note_instrument_injection_gives_the_original_instrument` não pega, só o jogo) ou quebra o build. Custo: uma comparação por nó do JSON a cada `rename` na carga do conteúdo.
- **Conferido em jogo** (2026-11-12, `horizon2`, `h2_nota_instrumento.py medir`): `instrument=snare` e `banjo` nos blocos de nota sobre o sal e o bio fuel (postos por jogador e por troca da base), `harp` sobre o lápis, e o som tocado (`heard`) `block.note_block.snare`, `banjo` e `harp`, como no original e como a areia e o feno do vanilla.

### `result_take_before` (mekanism)

- **Estado:** em uso. **Alvo:** `horizon_client::game::lan_crafting::Game<'_>::lan_machine_click`, HEAD, não cancelável
  (`lan_crafting.rs:248`, método de `impl Game`). **Proposta:** H27.
- **Helper:** `mods/mekanism/native/result_take_before.rs`. Guarda o slot de saída da máquina aberta e onde o jogador estava, e o
  próximo tick da máquina confere se o slot diminuiu (`result_take_after`, no começo de `tick_machine`). **Também deixa a regra de slot
  do clique** (2026-11-07): antes de cada clique do menu de uma máquina, `publish_slot_rules` guarda em `slot_rules` (núcleo) o
  `isValidForInsertion(item, MANUAL)` de cada slot, com o que o tanque e a entrada guardam agora, e o `menu_slot_rules` do Horizon
  (v1, no crate `[api]` da `machinery_basis`) o consulta dentro do menu. Veja [A regra de slot do clique](#a-regra-de-slot-do-clique).
- **Por que é necessária:** `InventoryContainerResultSlot.onTake` chama `onTakeCheck`, que é `awardUsedRecipesAndPopExperience`: tirar o
  resultado de uma máquina com `xp_containment` solta a experiência, e sem o upgrade solta a da última receita, como o forno. O Horizon
  não avisa quando um jogador tira de um slot de resultado (`results`) de um menu de mod: `menu_clicked` é só dos menus que o mod abre,
  `menu_quick_move` só do shift-clique e `native_furnace_extract` só do forno. O clique do jogador, no anfitrião e nos convidados, passa por
  `lan_machine_click`, no servidor.
- **Java:** `InventoryContainerResultSlot.java:30-48` (`checkTakeAchievements`), `OutputInventorySlot.java:33-35` (`setOnTakeCheck`),
  `RecipeCacheLookupMonitor.awardUsedRecipesAndPopExperience` (`RecipeCacheLookupMonitor.java:273-304`).
- **Hook mais próximo:** nenhum. Um RETURN em `lan_machine_click` não serve: a função tem itens aninhados (`E_UNSUPPORTED_CONTROL_FLOW`), e
  `menu_ops`, no cliente, só vê o que o cliente prevê.
- **Hook proposto:** H27 para a retirada do resultado, H20 v2 para a regra de slot.
- **Risco:** mudar a assinatura de `lan_machine_click`, o nome do módulo de `lan_crafting` ou `Slots::Block`. Dois cliques no mesmo
  tick, antes de a máquina andar, são resolvidos na ordem: o segundo clique confere o primeiro antes de guardar o seu. A regra de slot
  vale para um clique: o seguinte a troca antes de qualquer pergunta, e o clique de quem não está num menu nosso a apaga.

### `menu_click_before` (mekanismgenerators)

- **Estado:** em uso. **Alvo:** o mesmo de `result_take_before`, `horizon_client::game::lan_crafting::Game<'_>::lan_machine_click`, HEAD,
  não cancelável. **Proposta:** H20 v2.
- **Helper:** `mods/mekanismgenerators/native/menu_click_before.rs`. Deixa em `slot_rules` (núcleo) a regra de `mayPlace` do Heat Generator
  aberto: o slot de combustível aceita o que queima (`FluidFuelInventorySlot.forFuel`, que também aceita o que enche o tanque de lava, e o
  balde de lava queima) e o de energia nada (`EnergyInventorySlot.drain`, e nenhum item portado tem handler de energia).
- **Por que é necessária:** o tempo de queima é do jogo (`game.res.providers().burn_time`), e o `menu_slot_rules` do Horizon roda dentro do menu, sem
  o `Game`. A regra responde para cada item que o clique pode perguntar, que são os que o menu tem: o inventário de quem clica, o cursor e
  os slots do bloco.
- **Java:** `InventoryContainerSlot.java:68-79` (`mayPlace`), `FluidFuelInventorySlot.java:39-58` (`forFuel`), `EnergyInventorySlot.java:49-54` (`drain`).
- **Hook mais próximo:** `menu_slot_rules` v1, sem `Game`.
- **Hook proposto:** H20 v2, com o `Game` em leitura.
- **Risco:** o mesmo de `result_take_before`: as duas injeções são HEAD do mesmo método.

### `sprite_ties` (machinery_basis)

- **Estado:** em uso. **Alvo:** `horizon_client_render::hud::HudRenderer::animate_sprites`, HEAD, não cancelável (`hud.rs:2631`, método
  público de `impl HudRenderer`). **Proposta:** H26.
- **Helper:** `mods/machinery_basis/sprite_ties_head.rs`, mais o módulo `sprite-ties` (`sprite_ties.rs`, enxertado em `hud.rs` como
  `pub mod machinery_sprite_ties`), onde `sprite_tinted` e `tiled_sprite` do `glue` registram o retângulo do atlas de cada sprite que
  uma tela de mod desenha.
- **Por que é necessária:** o `HudRenderer` só tem o `Rect` de cada animação, não o nome do sprite, então o único jeito de saber quais
  sprites animados são de uma tela de mod é a tela dizer. O helper mistura os quadros desses sprites antes do laço do Horizon e grava
  o estado em `previous`, e o laço original pula o sprite. Quadro inteiro (`millis == 0`) e sprite que nenhuma tela de mod desenhou
  ficam com o laço do Horizon.
- **Java:** `SpriteContents.AnimationState.drawToAtlas` e `animate_sprite_interpolate.fsh`: `mix(atual, proximo, id / 1000.0)` em float,
  na GPU, gravado em 8 bits.
- **Hook mais próximo:** nenhum. `animate_sprites` não é hook do catálogo, e a diferença está dentro dele.
- **Hook proposto:** nenhum, é a correção H26 (a mistura do próprio renderer).
- **Risco:** mudar a assinatura de `animate_sprites`, o tipo de `sprite_animations` (`(Rect, SpriteAnimation, (usize, usize, u32))`) ou
  `SpriteAnimation::{blend, resized_pixels}`. O módulo exige que `hud.rs` não tenha `use ...::*` de topo (o planejador recusa).

### A barra de armadura e mão (machinery_basis, 9 injeções)

- **Estado:** em uso desde 2026-11-07. Nove injeções (`bar_slot_position`, `bar_slots`, `bar_slot_count`, `bar_menu_slot`, `bar_clicked_outside`,
  `bar_machine_menu`, `bar_quick_move`, `bar_answer_click`, `bar_resend_menu`) e cinco módulos (`armor-bar`, `bar-menu`, `bar-machine`, `bar-hub`,
  `bar-quick-move`). **Proposta:** H31 (as oito primeiras, menos `bar_clicked_outside`) e H32 (`bar_clicked_outside`).
- **Para quê:** `GeneratorsContainerTypes.java:29` registra o Heat Generator com `.armorSideBar(-20, 11, 0)` (`ContainerTypeDeferredRegister.java:145`),
  e `MekanismContainer.addArmorSlots` (`MekanismContainer.java:231`) põe, depois dos 36 do inventário, quatro `ArmorSlot` (a cabeça em cima, sobre os
  índices 39 a 36 do jogador) e um `OffhandSlot` (índice 40): os slots 40 a 44 do menu. Decisão de desenho (2026-11-07): são slots reais do jogador no
  menu do servidor, que o motor verificado do Horizon já tem (`Role::Armor`: `mayPlace` por `canEquip`, tamanho 1, `mayPickup` com a maldição do
  vínculo, `onEquipItem` no clique), com os ids de cliente 500 a 504 que o jogo usa para `Game::armor` e `Game::offhand`. A lib guarda a declaração por
  tipo de menu (`machinery_basis_core::armor_bar`: a posição; `screen_elements`: as abas fora da imagem), o mod só a declara (o Heat Generator: `armorSideBar(-20, 11, 0)`, as abas
  de calor e de energia; as quatro máquinas, as de energia, de avisos e de xp) e desenha o `GuiSideHolder` e os quadros.
- **O que cada injeção faz:**
  - `bar_slot_position` (RETURN de `Game::slot_position`), `bar_slots` (RETURN de `Game::slots`) e `bar_slot_count` (RETURN de `Game::slot_count`): na tela de
    mod aberta com barra, a posição declarada para 500 a 504 e a lista (e a contagem, que o hook de `screen_render_post` confere) com esses cinco.
  - `bar_menu_slot` (HEAD cancelável de `Game::menu_slot`, `remote.rs`): o índice `slots + 36 + k` do menu do servidor é 500 + k. É HEAD cancelável, não
    RETURN, porque o corpo tem `?`. `menu_slot_index` percorre os índices até o primeiro `None`, então os cinco têm de vir logo depois do inventário.
  - `bar_machine_menu` (HEAD de `lan_machine_click`, o mesmo alvo de `result_take_before` e `menu_click_before`) e o módulo `bar-machine`: `Menu::new` devolve
    `Self`, e o mixin recusa HEAD cancelável e RETURN nesse caso (`E_UNSUPPORTED_SIGNATURE`, medido). O menu que o servidor roda para a conexão nasce de
    `Menu::new(Kind::Mod(t))`, então o HEAD o deixa pronto em `crafting.machine`, com os cinco slots a mais (`machinery_bar_menu::extend`, módulo em
    `menu.rs`), e o clique o reaproveita (`take().filter(janela e tipo iguais)`).
  - `bar_quick_move` (HEAD cancelável de `lan_mod_quick_move`) e o módulo `bar-quick-move`: `quick_move_slot` dá `None` para o que passa de `size + 36`, então
    o shift-clique que parte da armadura ou da mão cairia no `ChestMenu.quickMoveStack` do motor. O módulo repete as guardas de `lan_mod_quick_move`, confere
    `Slot.mayPickup` (a maldição do vínculo recusa em sobrevivência, e o `doClick` volta antes do `quickMoveStack`) e chama o `native_menu_quick_move` do mod, com
    a pilha da peça; o que sobra volta ao slot de origem. Nenhum efeito de equipar: `quickMoveStack` move por `insert` e `remove`, sem `setByPlayer`.
  - `bar_answer_click` (HEAD de `Hub::answer_click`) e `bar_resend_menu` (HEAD de `Hub::resend_menu`) e o módulo `bar-hub`: o hub sincroniza `size + 36` slots, e
    a armadura chegaria ao cliente um tick depois, pela sincronização do menu do inventário (um quadro com o item em lugar nenhum: o cursor já vazio e o slot
    ainda vazio). `Hub::machinery_sync_bar` manda na hora os cinco slots (janela 0, slots 5 a 8 e 45) que o clique mudou. O quadro sem o item em lugar nenhum é de leitura do código
    (`Hub::sync_player_state` (`hub.rs`, o `broadcastChanges` do menu do inventário, no tick do hub) contra `answer_click`): o `ctl` não o mede, porque o intervalo é de um tick (50 ms) e uma troca de `item replace` com o servidor parado
    também chega ao cliente, então o caso `frozen` de `h2_armor.py` só confere que o capacete está no slot depois do clique.
  - `bar_clicked_outside` (HEAD cancelável de `Game::clicked_outside`): o original (`GuiMekanism.java:397-399`) é
    `super.hasClickedOutside(..) && findElementOver(..) == null`, e o NeoForge acrescenta `if (slot != null) clickedOutside = false`
    (`AbstractContainerScreen.mouseClicked`). O Horizon dá a vez a "fora" sobre um slot (`container_screen::Target::of`, a regra do vanilla), e o retângulo da
    imagem tem 176 de largura: um clique sobre um slot da barra (x -21 a -3) ou sobre uma aba lateral com um item no cursor soltava o item. O HEAD responde falso
    sobre um elemento ativo da tela: os quadros dos slots da barra (`armor_bar::over_slot`), os elementos que o mod declarou em `screen_elements` e que estão ativos com o
    espelho do menu de agora (`GuiHeatTab` e `GuiEnergyTab` do gerador; `GuiEnergyTab`, `GuiWarningTab` e o botão da `GuiXpContainmentTab` das quatro máquinas, as duas últimas condicionais), e a
    aba de upgrades e a janela dela, que a lib sabe. O helper lê o espelho do menu do cliente (que o módulo `windows`, filho de `native_hooks.rs`, guarda), por um método dele (`mb_open_mirror`), que é
    filho dele: nenhuma injeção nova, e a contagem não muda. O suporte (`GuiSideHolder`) e o `GuiSlot` têm `active = false` (`GuiSideHolder.java`, `GuiSlot.java:91`), então a margem deles continua "fora".
    Cursor com 5 de terra e um clique em cada aba, no jogo (`horizon`, 2026-11-07): antes soltava o item, depois fica (o gerador, e as quatro máquinas em 16 estados, com o botão direito também).
- **Java:** `MekanismContainer.java:231,278-330` (`addArmorSlots`, `quickMoveStack`), `ArmorSlot.java`, `OffhandSlot.java`,
  `GuiHeatGenerator.java:35` (`GuiSideHolder.create(this, -26, 6, 98, true, true, TAB_ARMOR_SLOTS)`), `GuiMekanism.java:397-399` e `:715-775` (`GuiSlot` para cada slot).
- **Hook mais próximo:** nenhum. `native_screen` só descreve os slots do contêiner (300 e acima) e o inventário (0 a 35), `Menu::new` não conhece slots de
  armadura fora de `Kind::Inventory`, e o hub sincroniza `size + 36`.
- **Hook proposto:** H31 e H32.
- **Risco:** `slot_position`, `slots`, `slot_count` e `clicked_outside` são o núcleo da tela de inventário; `menu_slot` e `answer_click` são o protocolo do menu. Renomear
  um deles quebra o build; mudar a ordem dos slots do menu (os cinco depois dos 36) ou o que `lan_machine_click` faz com `crafting.machine` quebra a barra sem erro. O
  registro tem uma cópia por crate (`armor_bar` no cliente, `machinery_bar_menu` no crate de baixo, que o servidor usa), e o mod declara nos dois pela mesma chamada.

### `flush_signals` (machinery_basis)

- **Estado:** em uso desde 2026-11-08. **Alvo:** `horizon_client::game::spill::Game<'_>::flush_spill` (`spill.rs:733`), HEAD. **Proposta:** H34.
- **Helper:** `mods/machinery_basis/flush_signals.rs` chama `machinery_basis::flush_neighbor_signals(game)`. `Game::with_level` (`game.rs:4577`) chama
  `flush_spill` por último (`game.rs:4755`), depois de a operação no mundo acabar e de o `GameLevel` sair de uso, então `flush_spill` é o fim de toda
  operação (`setblock`, uso de item, comando, alavanca, pistão): um ponto só, não genérico e com `&mut Game`.
- **Por que é necessária:** `BlockTile.neighborChanged` chama `TileEntityMekanism.updatePower` (`BlockTile.java:115-124`, `TileEntityMekanism.java:1109-1116`), que lê
  `level.hasNeighborSignal(pos)` na hora do aviso, e o campo `redstone` fica com aquele valor até o próximo aviso (rastros `neighbors_*`). O hook `neighbor_changed`
  (`native_hooks.rs:1317`, `listen`) avisa quem foi notificado, mas roda com `&GameLevel` no meio de `Behaviours::update_neighbors_at`, que tem `level` e `ticks` emprestados
  como `&mut`: não há como ler o sinal ali (`Behaviours::has_neighbor_signal` pede os dois), e o código do motor de redstone é genérico (`Sim<L: LevelAccess>`,
  `impl<'a, L> Sim<'a, L>`), que o `horizon-mixins` não aceita como alvo (`boundary-head-and-modules.md`: "Type/const generic impls ... remain unsupported").
  O hook anota (dimensão e posição) e `flush_spill` lê o sinal com `native_neighbor_signal` no fim da operação que avisou. Lido no tick da máquina em vez disso,
  uma operação que avisa e outra, no mesmo tick, que muda o sinal sem avisar (rastro `chamber_unnotified`, t=100) davam outro valor: 264 diferenças de
  comportamento em 150 ticks e 3 máquinas, medidas no simulador contra o Java.
- **Hook mais próximo:** `neighbor_changed` v1 (`listen`, `&self`), `level_tick_post` (depois do tick do nível, tarde demais para o fim de cada operação).
- **Hook proposto:** H34: o `neighbor_changed` v2 entregaria `signal: i32` (`getBestNeighborSignal` lido dentro do aviso, pelo motor que já está de posse do nível) ou
  pediria `&mut` à `Game`. Elimina esta injeção e o `flush_neighbor_signals`.
- **Risco:** renomear ou mover `flush_spill`, ou `with_level` deixar de chamá-lo no fim, deixa as máquinas sem o `redstone` (o aviso fica na fila e só é lido
  no tick da máquina, o que ainda funciona para o caso comum, mas perde o caso composto).
- **Conferido em jogo** (2026-11-08, `horizon`, build `native-f319701-3baabd0d9efd`): os nove cenários de `h2_vizinhos.py` contra os rastros `neighbors_*` do Java, 0 diferenças em 2.673 leituras; decisão 7 do plano.

### `container_changed` (machinery_basis)

- **Estado:** em uso desde 2026-11-08. **Alvo:** `horizon_client::game::machines::Game<'_>::container_changed` (`machines.rs:565`), HEAD. **Proposta:** H35.
- **Helper:** `mods/machinery_basis/container_changed.rs` chama `machinery_basis::container_changed(game, pos)`: para o block entity do vanilla em `pos` (e a outra metade de um baú duplo,
  `chest_other_half`), cada vizinho com tile do Mekanism relê o sinal na hora (`onNeighborChange`, na ordem de `Level.NEIGHBOR_UPDATE_LIST`).
- **Por que é necessária:** `BlockEntity.setChanged` (`BlockEntity.java:216-221`) chama `Level.updateNeighbourForOutputSignal` (`Level.java:992-1006`), que no NeoForge manda `onNeighborChange` a
  cada vizinho (`BlockTile.onNeighborChange`, `BlockTile.java:105-113`, -> `updatePower`). O Horizon só avisa comparadores nesse caminho (`Sim::update_neighbour_for_output_signal`, `update.rs:679`,
  genérico e fora do alcance de um alvo cru). `Game::container_changed` é o `setChanged` dos contêineres do vanilla (o funil que enche um baú, o jogador que mexe nos itens, o forno que
  queima), um ponto não genérico e com `&mut Game`; não cobre os block entities do vanilla sem contêiner (uma placa, um sino), que no original também mandam o aviso.
- **Hook mais próximo:** nenhum (`block_changed` só vê `setBlock`).
- **Hook proposto:** H35 `block_entity_changed` (`listen`): `dimension`, `pos` e o tipo do block entity, chamado no `BlockEntity.setChanged` e em `Level.removeBlockEntity` (`Level.java:686`).
- **Risco:** renomear `container_changed` ou mudar a posição (`Pos`) quebra o build.
- **Conferido em jogo** (2026-11-08, `horizon`): `chamber_output_signal` (um baú vizinho que muda, e uma máquina vizinha que muda o comparador) dava 99 diferenças em 226 leituras sem a injeção e 0 com ela; `heat_output_signal` 0.

### `energy_cube_item_core` (mekanism)

- **Estado:** em uso desde 2026-11-12 (frente `cubo-nucleo`). **Alvo:** `horizon_client::game::item_draw::Game<'_>::resolve_item_in_hand` (`item_draw.rs:328`), RETURN, com o binding `$value` do tipo `ItemRenderState` (o tipo escrito como no fonte do Horizon: o `horizon-mixins` recusa `crate::item_render::ItemRenderState`, que é o mesmo tipo). **Proposta:** H99.
- **Para quê:** `RenderEnergyCubeItem.submit` desenha, depois do quadro, o núcleo translúcido do cubo com energia (`Sheets.translucentBlockItemSheet`, luz máxima, a tinta do tier com o alfa `ARGB.as8BitChannel(ContainerType.ENERGY.getRatio(stack))`, o `gameTime` mais o `partialTick` girando e subindo e descendo como em `RenderEnergyCube`). O Horizon troca o `minecraft:special` do item pelo modelo base (H60), e `resolve_item_in_hand` é por onde a barra, a mão, o chão e o quadro resolvem o item a cada quadro. O helper acrescenta ao `ItemRenderState` uma `LayerRender` com o modelo `mekanism:block/energy_core` assado pelo `ItemRenderer` (as faces com `force_translucent`), a matriz da camada do quadro vezes a pose do núcleo e a tinta com o alfa. A conta da pose e do alfa está em `machinery_basis_core::energy_cube` (`core_pose`, `core_alpha`), a mesma do tile.
- **Risco:** o nome e o módulo de `resolve_item_in_hand`, o tipo `ItemRenderState` e os campos de `LayerRender` e `LayerGeometry` (privados ao crate), e `Game::clock`, `world.game_time` e `pack.item_renderer`. Muda quando o Horizon mexer no pipeline de item. O hook H99 troca tudo por um callback com a pilha, o `gameTime` e o `partial`.

### `energy_store` (mekanism)

- **Estado:** em uso desde 2026-11-08. **Alvo:** `horizon_client::game::machines::native_energy::Game<'_>::native_energy_store` (`native_energy.rs:82`), HEAD. **Proposta:** H37.
- **Helper:** `mods/mekanism/native/energy_store.rs` chama `mekanism_glue::energy_stored(game, pos, energy)`: se o tile em `pos` é uma máquina e o valor subiu, a diferença entra no `LastEnergyTracker` do
  objeto vivo (`ProgressMachine::energy_received`, no `game_time` do jogo).
- **Por que é necessária:** `lastEnergyReceived` (`TrackingEnergyHandler.insert` -> `LastEnergyTracker.received`) é um dado do menu (`trackLastEnergy`, `TileEntityMekanism.java:967-972`) e entra na conta de
  `getInputRate`. A energia entra nas máquinas por `native_energy_insert` e `native_move_energy`, e as duas terminam em `native_energy_store`, que grava o valor novo no tile: nenhum hook do catálogo
  avisa a inserção, e o `ModEnergyBuffer` é um dado, sem lugar para um observador.
- **Hook mais próximo:** nenhum.
- **Hook proposto:** H37 `block_entity_energy_inserted` (`listen`): `dimension`, `pos`, a face e a quantidade que de fato entrou.
- **Risco:** renomear `native_energy_store` ou fazer outro caminho gravar `energy` sem passar por ele (ainda não existe) deixa o `lastEnergyReceived` em zero, sem erro.
- **Conferido em jogo** (2026-11-08, `horizon`): ver o item `MekanismContainer` do plano.

### `fluorite_experience` (mekanism)

- **Estado:** em uso desde 2026-11-09. **Alvo:** `horizon_client::game::Game<'_>::block_experience` (`game.rs:14035`), RETURN. **Proposta:** H42.
- **Helper:** `mods/mekanism/native/fluorite_experience.rs` chama `mekanism_glue::block_experience(name, value)`, que consulta `rules::ore_experience`: a fluorita (`mekanism:fluorite_ore` e `mekanism:deepslate_fluorite_ore`) dá de 1 a 4 de experiência, os outros blocos ficam com o valor que o Horizon calculou.
- **Por que é necessária:** `BlockOre.getExpDrop` (`BlockOre.java`) sorteia `Mth.nextInt(random, minExp, maxExp)` com o `minExp` e o `maxExp` do `OreType` (`OreType.java`, fluorita 1 e 4, os metais 0 e 0). O Horizon só dá experiência a um minério pelo nome do vanilla (`Game::block_experience` em `game.rs`, `removal::break_experience`): um bloco de mod devolve 0, e o hook `block_exp` só dispara quando o nome do vanilla já tinha experiência.
- **Hook mais próximo:** `block_exp` (`native_block_exp`, API 190), que muda a quantidade de um bloco que já dá experiência; falta o caso do bloco de mod que dá.
- **Hook proposto:** H42, um campo `experience` (`min`, `max`) nas `properties` do bloco, ou o `block_exp` chamado também para o bloco que dá 0.
- **Risco:** renomear `block_experience` ou mudar o retorno quebra o build; o sorteio é o `Math.random()` do Java (`java_random::math_random`) porque o gerador do nível está emprestado dentro do corpo (o `return` do spawner), o que dá a distribuição do original e outro fluxo de números. Só vale a quebra do jogador: a de máquina (`removal::break_experience`, genérica no gerador, que o `horizon-mixins` recusa como alvo) não passa por aqui.
- **Conferido em jogo** (2026-11-09, `horizon`, `h2_craft.py mining`): a fluorita e a de deepslate dão de 1 a 4 de experiência em 8 quebras cada (1, 3, 1, 3, 1, 2, 4, 2 e 1, 2, 1, 4, 3, 2, 4, 1) e os cinco metais 0.

### `block_rows` (mekanismgenerators)

- **Estado:** em uso desde 2026-11-09 (frente `generators-conteudo`). **Alvo:** `horizon_client_lib::mod_content::Plan<'_>::override_gameplay`, HEAD (o mesmo da `active_light`, `mod_content.rs:1319`).
- **Helper:** `mods/mekanismgenerators/native/block_rows.rs`: a `mekanism_active_light` do Mekanism com a tabela `mekanismgenerators_rows` (módulo `block-rows`, `native/block_rows_table.rs`, gerada de `rastros/blocos_generators.jsonl`, o original em jogo,
  por `dev/referencia/generators_conteudo.py` e conferida estado a estado por `tests/generators_registry.rs`, 148 estados). Escreve na linha de cada estado dos 23 blocos a luz do `active` (13 no Bio-Generator e no Gas-Burning Generator, 8 no Wind Generator) e a da lava (15), a reação ao pistão
  (`IMMOVEABLE`), `can_occlude`, `solid_render`, `dampening` e `skylight_down` (com a oclusão do estado com fluido dentro).
- **Por que é necessária:** a mesma da `active_light`: `properties` não alcança a luz condicional, a reação ao pistão (`BlockMekanism.getPistonPushReaction`) nem a oclusão de uma forma menor que o cubo. O Generators é outro mod e dois mods não dividem namespace nem injeção por id: o `mekanism_active_light` só olha ids `mekanism:`.
- **Hook mais próximo:** nenhum. **Hook proposto:** H18 (a luz) e H49 (o resto), dados sem hook.
- **Risco:** o do `active_light`: depende de `NewBlock` (`decl.id`), do `combo` e do formato de linha (`J`, chaves `emission`, `push`, `can_occlude`, `solid_render`, `dampening`, `skylight_down` e `explosion_resistance`). As duas são HEAD do mesmo método e o Horizon as põe em sequência.
- **Conferido em jogo** (2026-11-09, `horizon`, `h2_generators.py luz`): os 30 estados que a leitura alcança (27 sem luz e 3 com: os acesos do Bio-Generator, do Gas-Burning Generator e do Wind Generator) dão a luz do original; os 148 estados pela tabela em `tests/generators_registry.rs`.

### `creative_tabs` (mekanismgenerators)

- **Estado:** em uso desde 2026-11-09 (frente `generators-conteudo`). **Alvo:** `horizon_client_lib::mod_content::Plan<'_>::creative_tabs`, RETURN (o mesmo da `creative_tabs` do Mekanism, H43), com o módulo `creative-tabs-table` do Generators.
- **Helper:** `mods/mekanismgenerators/native/creative_tabs.rs` percorre as linhas de aba e, para cada uma, chama `mekanismgenerators_tabs::edit_tab(id, itens)` (módulo `creative-tabs-table`, `native/creative_tabs_table.rs`, gerado de `rastros/blocos_generators.jsonl`, testado em `tests/generators_creative_tabs.rs`): acrescenta ao fim das abas do vanilla
  o que `GeneratorsCreativeTabs.addToExistingTabs` acrescenta (os seis geradores nos blocos funcionais, os sete blocos com comparador na redstone, os quatro baldes nas ferramentas, o hohlraum e o painel solar nos ingredientes).
- **Por que é necessária:** a do Mekanism: o Horizon só deixa o mod declarar a aba própria (`creative_tab/mekanismgenerators.json`, só ids), e não acrescenta às abas do vanilla.
- **Detalhe que quebrou o build:** as duas injeções são RETURN do mesmo método, e o Horizon aninha a segunda em volta da primeira (`helper_b(&mut *doc, helper_a(&mut *doc, resultado))`): com `doc` antes de `result` nos `bindings`, o segundo `&mut *doc` é um segundo empréstimo mutável (`E0499`).
  Com `result` primeiro (`helper_b(helper_a(..), &mut *doc)`) o interno acaba antes. Por isso os `bindings` do Generators trazem `result` antes de `doc`, e o do Mekanism segue com `doc` antes: se a ordem das duas se inverter, o build quebra (alto), não passa em silêncio.
- **Hook mais próximo:** nenhum. **Hook proposto:** H43.
- **Risco:** o da do Mekanism, mais a ordem de aninhamento das duas.
- **Conferido em jogo** (2026-11-09, `horizon`, prints `porte_aba_*` em `~/.cache/rust-mekanism/wa/generators-conteudo/`): a aba "Mekanism: Generators" aparece depois da "Mekanism", com os 31 itens na ordem do original; os seis geradores estão no fim dos blocos funcionais (depois dos do Mekanism, o do vento sem desenho: o item é `minecraft:special`), o hohlraum e o painel solar no fim dos ingredientes.

### `creative_tabs` (mekanism)

- **Estado:** em uso desde 2026-11-09. **Alvo:** `horizon_client_lib::mod_content::Plan<'_>::creative_tabs` (`mod_content.rs:1704`), RETURN (o corpo não tem `?`), com o módulo `creative-tabs-table`. **Proposta:** H43.
- **Helper:** `mods/mekanism/native/creative_tabs.rs` percorre as linhas de aba que o Horizon montou (`doc.at("tabs")`) e, para cada uma, chama `mekanism_tabs::edit_tab(id, itens)` (módulo `creative-tabs-table`, `creative_tabs_table.rs`, código puro testado em `tests/creative_tabs.rs`): na aba `mekanism:mekanism` troca o item `mekanism:upgrade` por uma pilha por tipo (`{count:1,id:"mekanism:upgrade",components:{"mekanism:upgrade_type":"mekanism:<tipo>"}}`), e no fim de cada aba do vanilla acrescenta o que o evento `BuildCreativeModeTabContentsEvent` do original acrescenta.
- **Por que é necessária:** `MekanismCreativeTabs.addToExistingTabs` põe os itens do Mekanism nas abas do vanilla (construção, natureza, funcionais, redstone, ferramentas, combate e ingredientes), e `ItemUpgrade` (`ICustomCreativeTabContents`, `addDefault` falso) entra nas abas só pelas pilhas de cada tipo. O Horizon só deixa o mod declarar a aba própria, com ids de item (`creative_tab/*.json`, `stack(id)` em `Plan::creative_tabs`): não acrescenta a abas do vanilla e não escreve componente na pilha.
- **Hook mais próximo:** nenhum.
- **Hook proposto:** H43: o arquivo `creative_tab` aceitar uma pilha completa (SNBT com `components`) nos itens e um `extends` (uma aba do vanilla e a lista a acrescentar ao fim dela).
- **Risco:** renomear `creative_tabs`, mudar o formato da linha da aba (`id` e `items` em SNBT de pilha) ou a aba de busca (que continua só com o `upgrade` sem tipo) quebra o build ou deixa a aba sem as pilhas, sem erro.
- **Conferido em jogo** (2026-11-09, `horizon`): a aba "Mekanism" mostra as oito pilhas de upgrade com os ícones de tipo, e a aba Ingredients do vanilla termina com os itens do Mekanism na ordem do original (prints em `~/.cache/rust-mekanism/wa/itens-craft/`).

### O equipamento vestido (machinery_basis, 6 injeções e 2 módulos)

- **Estado:** em uso desde 2026-11-09. Seis injeções e dois módulos (`worn-gear` e `worn-hides`), mais o hook `standalone_models` do `mekanism` (limpo). **Proposta:** H55.
  - `worn_mob`: HEAD de `horizon_client::game::mob_blocks::Game<'_>::head_item_geometry` (o `CustomHeadLayer`: o jogador da janela em terceira pessoa, os outros jogadores, os mobs e o jogador da
    tela do inventário passam por ela, depois do modelo e da armadura do vanilla). Desenha os modelos do equipamento vestido na pose da parte do corpo.
  - `worn_stand`: HEAD de `horizon_client::game::armor_stands::Game<'_>::armor_stand_vertices` (`ArmorStandRenderer`: o armor stand tem o desenho próprio, fora do caminho dos mobs). Mesma coisa na pose do stand.
  - `worn_arm`: HEAD cancelável de `horizon_client::game::Game<'_>::first_person_arm` (`renderPlayerHand`): com o peito do MekaSuit o braço do vanilla não se desenha e o traje desenha o dele (`RenderArmEvent`).
  - `worn_cape`: HEAD cancelável de `horizon_client::game::perspective::Game<'_>::player_cape_geometry`: a capa some com o peito do MekaSuit, o jetpack e o tanque (`avatarState.showCape = false`).
  - `worn_hide`: HEAD de `horizon_client_lib::mobs::anim::setup` e `worn_hide_player`: RETURN de `horizon_client_lib::mobs::anim::player_model_visibility`. As partes do corpo que o equipamento esconde
    (`overrideModelPartVisibility`: o MekaSuit esconde a cabeça, o corpo, os braços e as pernas do modelo) saem por `Poses::visible(parte, false)`. O jogador mostra de novo o corpo, os braços e as pernas ao montar a
    visibilidade do modelo, depois de `setup` ter começado, então o fim de `player_model_visibility` esconde de novo (uma lista numa variável da thread, que `setup` enche). Um RETURN de `setup` não serve: o corpo dele tem
    `matches!` e o planejador exige o caminho absoluto de `std` no macro.
- **Helpers:** `mods/machinery_basis/worn_gear_mob.rs`, `worn_gear_stand.rs`, `worn_gear_arm.rs`, `worn_gear_cape.rs`, `worn_gear_hide.rs` e `worn_gear_hide_player.rs`, que chamam o módulo `worn-gear`
  (`crate::game::machinery_basis_gear`, `worn_gear.rs`, enxertado em `game.rs`) e o módulo `worn-hides` (`crate::mobs::anim::machinery_basis_worn`, `worn_hides.rs`, enxertado em `mobs/anim.rs` com
  `visibility = "pub"`, porque quem diz o que cada item esconde é o crate do jogo e quem esconde é o crate de baixo). O mod entrega a tabela (`mekanism_rules::worn_gear::WORN_GEAR`, dados e passos de
  pose, em `machinery_basis_core::worn_gear`) pelo hook `standalone_models` (`machinery_basis_gear::register`), que também devolve ao Horizon os ids dos modelos avulsos que ele tem de assar (`native_draw_model`
  e `native_standalone_models` são o contrato que o hook usa).
- **Por que são necessárias:** o `MekanismArmorLayer` do original (`HumanoidArmorLayer` com o `ICustomArmor` de cada item, `client/render/armor`) pede o modelo de cada peça na pose da PARTE do modelo
  (`baseModel.body.translateAndRotate`, `leftLeg`...), no jogador, no armor stand, em qualquer mob humanoide, na tela do inventário e, para o braço, na primeira pessoa, e pede as partes do modelo do vanilla escondidas
  (`registerRenderStateModifiers`). O hook `mob_layer` do Horizon (`native_mob_layer`, API 149) dá o quadro do mob (giro do corpo, da cabeça, o passo), não as partes do modelo (agachar, nadar, voar não estão nele), e
  não roda para o armor stand, nem para o jogador da tela do inventário, nem na primeira pessoa; `native_render_hand` só esconde o braço; nenhum hook esconde uma parte do modelo nem a capa.
- **Java:** `MekaSuitArmor.java` (`ModelPos`, `createQuads`, `PartData.render`), `ChestArmor.java:33-57`, `FreeRunnerArmor.java:33-55`, `ClientRegistration.registerRenderStateModifiers` (`ClientRegistration.java:322-399`),
  `MekanismArmorLayer.java`, `RenderTickHandler.renderArm` (`RenderTickHandler.java:131-142`), `BaseModelCache.OBJModelData` (a rotação de PI em Z do estado do modelo, `:127-129`).
- **Hook mais próximo:** `mob_layer` (API 149) e `native_draw_model` (API 151): o desenho do modelo avulso, a lista dos modelos e a luz dos LEDs usam o que o Horizon dá, sem injeção; o que falta é o que as seis cobrem.
- **Hook proposto:** H55, abaixo.
- **Risco:** renomear `head_item_geometry` (o que a chama passa `mob, camera, alpha, light, lit, out`), `armor_stand_vertices` (`a, base, packed, skin, out`), `first_person_arm` (`left, m, light, out`),
  `player_cape_geometry` (`m, ...`), `anim::setup` (`m, s, p, w_anims`) ou `player_model_visibility` (`p, hidden, spectator`); `render::part_transform`/`part_matrix`, `Poses::{visible, new, get, has}`,
  `Game::baked_model_vertices` (privado de `minecarts.rs`), `Game::{armor, player_model, armor_stand_assets, pack, res}` e `Resources::player_arm`; o módulo `worn-hides` exige que `anim.rs` não tenha `use ...::*` de topo.
  Os dois `matches!` de `anim::setup` fazem o planejador recusar o RETURN dele; um RETURN de `player_model_visibility` precisa do valor como primeiro argumento (o corpo é um laço que usa `p`).
- **Conferido em jogo** (2026-11-09, `horizon`): ver "Armadura vestida" no inventário do plano.

### `enhanced_ao` (machinery_basis)

- **Estado:** em uso desde 2026-11-09. **Alvo:** `horizon_client_render::mesher::lighting::ambient_occlusion` (`mesher/lighting.rs:319`, `pub(super)`), `kind = "RETURN"` (o corpo não tem `?`), helper `enhanced_lighting_ao.rs`, com o módulo `enhanced-lighting` (`enhanced_lighting.rs`, em `mesher.rs`, `visibility = "pub"`). **Proposta:** H70.
- **Por que é necessária:** o NeoForge troca o `BlockModelLighter` do vanilla pelo `EnhancedBlockModelLighter` (`enhancedLighting`, ligado por padrão em `NeoForgeClientConfig`) em todo bloco: o quad de face inteira recebe os consertos do NeoForge (o canto sem os dois lados claros usa o lado vizinho, a luz de dentro vem sempre do ponto de amostra, a mistura de luz ignora o bloco que não deixa a luz passar, a interpolação não supõe a ordem dos vértices) e o quad que não é de face inteira (um elemento girado) combina o AO das faces que a normal toca, pelo quadrado de cada componente. O mesher do Horizon só tem o do vanilla (`lighting::ambient_occlusion`, sem hook), e a diferença aparece: a caixa de papelão aberta tem as abas giradas 22,5 graus, e o original as sombreia pela normal (a aba da esquerda quase preta, a da direita mais clara que no port). Provado: com `enhancedLighting = false` no cliente do original o print da cena `avulsos` fica igual ao do port, pixel a pixel nos pontos medidos.
- **O que faz:** para um quad de bloco de mod (`machinery_enhanced::uses(estado)`, a tabela de estados de nome fora de `minecraft:` que o `Game` refaz a cada carga de dados em `server_data_loaded`), o RETURN troca o brilho e a luz que o vanilla deu pelos do NeoForge (`machinery_enhanced::ambient_occlusion`, código puro, conferido quad a quad contra o NeoForge: `rastros/lighting_enhanced.jsonl`, `tests/enhanced_lighting_module.rs`) e devolve o brilho antes do sombreamento direcional (`before_directional`), porque o mesher o multiplica depois e o NeoForge já o tem dentro do brilho. Os blocos do vanilla ficam com o `BlockModelLighter` do vanilla (decisão 35 do plano).
- **Java:** `EnhancedBlockModelLighter.java`, `FullFaceCalculator.java`, `AoFace.java` e `BakedNormals.java` (`net.neoforged.neoforge.client.model.ao`), `BlockModelLighter.java`, `ModelBlockRenderer.java`.
- **Hook mais próximo:** nenhum. **Hook proposto:** H70, abaixo.
- **Risco:** renomear `ambient_occlusion` ou mudar a lista de argumentos (`pad`, `info`, `own`, `pos`, `positions`, `face`), os tipos privados `Padded` e `Info` (`full`, `shade`, `permeable`, `solid`) ou as funções `state`, `coords`, `cardinal` de `lighting.rs`; o módulo exige que `mesher.rs` não tenha `use ...::*` de topo. Diferenças que ficam: o `CardinalLighting` do Nether (o helper supõe o do Overworld, e o mesher multiplica pelo do Nether), o quad de modelo sem AO que a luz do bloco manda para o caminho plano (`prepareQuadFlat` do NeoForge pesa a luz direcional pela normal, e o do Horizon pela face mais próxima: o elemento girado de um bloco que emite luz ainda sai como no vanilla), o `shadeDirectionOverride` (nenhum modelo do Mekanism usa `shade: false`, mas 41 quads de 4 blocos têm `shade_direction_override: up`, e o helper dá o resultado limpo) e o cache de faces do NeoForge, que guarda as faces num `int` de bits e indexa até 83 (`1 << índice` passa de 31): um quad com sombra forçada pode receber de volta o resultado velho ou zerado de outra configuração, o que depende do que a thread de malha calculou antes e não se reproduz (no rastro, 7 de 41).
- **Conferido em jogo** (2026-11-09, `horizon`, a cena `avulsos`): na região da caixa de papelão aberta os pixels diferentes do original caem de 217 para 0,6 (`-fuzz 3%`), e a aba da esquerda passa de 51,40,28 a 18,14,10 e a da direita de 157,125,86 a 187,149,102, os valores do original (prints em `~/.cache/rust-mekanism/wa/acabamento-conteudo/luz/`).

### O `transform` do modelo (machinery_basis, 2 injeções e 1 módulo)

- **Estado:** em uso desde 2026-11-09 (frente `transform-modelo`). Duas injeções cruas e um módulo. **Proposta:** H67.
  - `model_transform_decode`: MODIFY_VARIABLE (o `STORE` do `let mut textures` de `horizon_assets::model::CuboidModel::decode`). Só lê o JSON que cita `"transform"`: lê a chave `transform` da raiz como o
    `TransformationHelper.Deserializer` do NeoForge e guarda a matriz no espaço de textura `\u{1}neoforge_transform` do modelo (o `CuboidModel` não tem campo para ela). Cada filho de um `composite` passa por aqui na
    própria `decode`. Um `transform` que o NeoForge recusa (o modelo não carrega) fica sem efeito, com um aviso no log: o valor de uma variável não devolve o erro. Um HEAD cancelável de `decode` não serve: o
    planejador pede um tipo de retorno concreto e `Result<Self>` não é.
  - `model_transform_bake`: HEAD cancelável de `horizon_assets::bake::bake_into`, o ponto por onde passam o bloco, o item e a detecção de cubo. Para o modelo que tem o espaço, deixa a original cozinhar (elementos,
    filhos do `composite` e `.obj`, com a rotação do blockstate; a injeção volta a si mesma com o mesmo modelo, e uma marca na thread a faz seguir para o corpo original) e move os vértices que ela acrescentou pela
    matriz do modelo, conjugada pela rotação do blockstate (`R M R^-1`, em volta do centro): é o `pai.compose(raiz)` do NeoForge. Um filho que só herdou o espaço do `composite` (o mesmo id) não o aplica de novo; o
    filho com o próprio soma o dele por dentro do do `composite`. Quando a matriz muda a direção das faces (rotação ou escala), a direção do quad e a face de corte são recalculadas (`Direction.rotate`).
- **Helpers:** `mods/machinery_basis/model_transform_decode.rs` e `model_transform_bake.rs`, que chamam o módulo `model-transform` (`crate::math::machinery_model_transform`, `model_transform.rs`, enxertado em `math.rs`).
- **Por que são necessárias:** o `CuboidModel` do Horizon não tem o campo, o `Loader::Obj` não o lê e o cozimento só conhece a rotação do blockstate. O limite de -16 a 32 do `Element::decode` não atrapalha: o vanilla o checa nos
  elementos antes da transformação, e os elementos do painel solar avançado já cabem.
- **Java:** `NeoForgeModelProperties.java` (`TRANSFORM`, `deserializeRootTransform`, `fillRootTransformProperty`), `TransformationHelper.java` (`Deserializer`, `parseRotation`, `TransformOrigin`), `ITransformationExtension.applyOrigin`,
  `UnbakedElementsHelper.java:163-171` (`composeRootTransformIntoModelState`), `ComposedModelState.java`, `UnbakedCuboidGeometry.bake`, `CompositeUnbakedGeometry.java:30-33`, `ObjGeometry.java:520-521` e `makeQuad`,
  `FaceBakery.java:134-136` e `rotateVertexBy`. Os modelos: `generators/resources/assets/mekanismgenerators/models/block/wind_generator.json:109` e `advanced_solar_generator.json:10`.
- **Hook mais próximo:** nenhum; o Horizon não tem o campo.
- **Hook proposto:** H67.
- **Risco:** os da ficha H67. Diferenças que ficam: o `FaceBakery` do original não reordena os vértices de um elemento sem rotação quando há `transform` (`mayApplyArbitraryRotation`), e o Horizon os reordena; só a ordem dos
  quatro vértices de um quad muda, não a geometria. Com o blockstate girado a conta tem 1 ulp de diferença do original (o Horizon soma 0,5 depois de girar).
- **Conferido contra o original:** `core/tests/model_transform.rs` contra `rastros/model_transform.jsonl` (as classes do NeoForge no `runGameTestServer`), `dev/referencia/transform_horizon_assets.py` contra o `horizon-assets` do pin,
  o `horizon-mixins plan` e `apply` contra o pin, e em jogo (2026-11-09, `horizon`): ver "Conteúdo do Generators" no inventário do plano.

### O `Kd` e o `neoforge_TintIndex` do `.mtl` (machinery_basis, 2 injeções e 1 módulo)

- **Estado:** em uso desde 2026-11-12 (frente `transmissor-obj`). Duas injeções cruas e um módulo. **Proposta:** H16. Faz de verdade o que a `spike_obj_kd` provou em 2026-11-05 (que punha `Kd` 0,8 em todo material do Mekanism).
  - `obj_material_parse`: RETURN de `horizon_assets::obj::parse_mtl`. O mapa de material para textura que ela devolve não tem lugar para o `Kd` nem para o `neoforge_TintIndex`: cada material do `.mtl` que tem `Kd` diferente de branco ou
    índice de tinta ganha ali uma entrada extra, de chave `\u{1}mtl:` + nome do material (nenhuma face procura essa chave) e valor `MtlTexture::Slot` com as duas propriedades escritas (`ffcccccc|-1`).
  - `obj_material_quads`: HEAD cancelável de `horizon_assets::bake::bake_obj` (um RETURN não serve: o corpo tem `?`, e o `horizon-mixins` recusa com `E_UNSUPPORTED_CONTROL_FLOW`, visto no primeiro `native-build`). Chama `bake_obj` de novo por dentro, com uma marca na thread (o endereço do `ObjModel`) que faz a segunda passagem seguir para o corpo original, como a `model_transform_bake`. Os quads que a segunda passagem acrescentou a `output` são um por face visível do OBJ, na ordem das faces (`obj.faces` sem as partes escondidas pela `visibility`): cada um
    leva a cor `Kd` do material da face em `color` (o Horizon a multiplica na do vértice, o que o spike mediu) e o `neoforge_TintIndex` em `tint_index`. Sem a entrada o quad fica como o Horizon o assou; um erro de `bake_obj` passa como veio.
- **Helpers:** `mods/machinery_basis/obj_material_parse.rs` e `obj_material_quads.rs`, que chamam o módulo `obj-material` (`crate::obj::machinery_obj_material`, `obj_material.rs`, enxertado em `obj.rs`). O módulo é só `std`: a conta do
  `ObjMaterialLibrary` (a última linha de cada material vence, `forge_TintIndex` também vale, `newmtl` repetido recomeça), `ObjGeometry.parseVector4` (de um a quatro números, o quarto vale 1) e a cor do vértice
  (`VertexConsumer.setColor(float, float, float, float)`: `(int)(x * 255.0F)`, e `ARGB.color` mascara cada canal em 8 bits). `Kd 1 1 1` não gera entrada (é a cor que o quad já tem), então os modelos sem `Kd` útil (MekaSuit, Wind Generator) não mudam.
- **Por que são necessárias:** o `parse_mtl` do Horizon lê só o `map_Kd`, e `ModelQuad.color` é a cor que o NeoForge multiplica; os materiais dos transmissores têm `Kd 0.8 0.8 0.8` e, sem isso, o corpo sai 20% mais claro que o do original. O
  `neoforge_TintIndex` (o vidro do transportador logístico e o conteúdo, `transmitter_glass_large.mtl` e `transmitter_contents.mtl`, índice 0) também se perde; os transmissores que o port desenha hoje não o usam (o vidro só se desenha
  com cor, e a cor é dado da block entity, Fase 3), e o módulo e a injeção já o levam ao quad.
- **Java:** `ObjMaterialLibrary.java` (`Kd`, `neoforge_TintIndex`, `forge_TintIndex`), `ObjGeometry.java:276-284` (`parseVector4`) e `:314-420` (`makeQuad`: `quadBaker.setTintIndex` e `setColor` com `color * colorTint`),
  `VertexConsumer.setColor(float, float, float, float)`, `ARGB.color(int, int, int, int)`.
- **Hook mais próximo:** nenhum. **Hook proposto:** H16, abaixo.
- **Risco:** renomear `parse_mtl` (o argumento `text`) ou `bake_obj` (`model`, `obj`, `flip_v`, `shade`, `hidden`, `transform`, `output`), mudar o tipo `MtlTexture` (a entrada extra é um `MtlTexture::Slot`), `bake_obj` deixar de acrescentar um quad por face visível, ou o Horizon passar a ler o `Kd`
  sozinho (a cor sairia multiplicada duas vezes: tirar as duas injeções e o módulo). O módulo exige que `obj.rs` não tenha `use ...::*` de topo.
- **Conferido contra o original:** `core/tests/obj_material.rs` (o módulo compilado como está, contra os `.mtl` do original que o conteúdo traz e as regras do NeoForge acima); `dev/referencia/obj_horizon_assets.py` (o texto das duas injeções e do módulo posto à mão no `horizon-assets` do pin, como `transform_horizon_assets.py`: as 264 partes dos 22 transmissores cozinham com os quads do objeto do OBJ, a cor `0xFFCCCCCC` em todos e as posições e uv do OBJ; `--sem-injecao` deixa a cor vazia e o teste falha). **Em jogo: pendente** (o `native-build` de 2026-11-12 aplicou os mixins e compilou o `horizon-assets` com as duas injeções, e morreu por memória no `horizon_client`; ver a decisão 26 no plano).

### `animate_tick` (machinery_basis)

- **Estado:** em uso desde 2026-11-08. **Alvo:** `horizon_client_lib::animate_tick::block_animate_tick` (`animate_tick.rs:819`, função livre privada), HEAD, não cancelável. **Proposta:** H39.
- **Helper:** `mods/machinery_basis/animate_tick.rs` chama `machinery_animate::animate(engine, ctx, state, pos)`, do módulo `animate-registry` (`animate_registry.rs`, enxertado em
  `animate_tick.rs` como `pub mod machinery_animate`, com `visibility = "pub"`). O módulo guarda os animadores que os mods registram (`register(dono, animador)`, uma vez por mod:
  o `client_tick` do Mekanism e o `client_tick` do Heat Generator, este um hook `tick` só com o contrato dele, chamam no primeiro tick) e os chama, na ordem, para cada posição
  sorteada. O animador do mod é do crate do jogo (`machine_animator` em `mekanism_glue`, `heat_generator_animator` em `mekanismgenerators_glue`), que usa
  `machinery_basis::animate_active_block`.
- **Por que é necessária:** `ClientLevel.animateTick` sorteia 667 posições a 16 blocos e 667 a 32 em volta do jogador, a cada tick, e chama `Block.animateTick` (`BlockTile.animateTick`,
  `BlockTile.java:80-103`: `AttributeParticleFX`, `Machine.java:23` e `GeneratorsBlockTypes.java:69`) do bloco de cada uma. O Horizon faz esse sorteio (`animate_tick`, `do_animate_tick`) mas
  só anima os blocos do vanilla, por nome (`classify`, uma tabela `Animate` fechada), e não tem hook para o bloco de mod: o bloco do Mekanism cai em `Animate::None`. O sorteio fica no
  cliente, no mesmo gerador do `ParticleEngine`, e a posição sorteada vai ao animador.
- **Por que o módulo e não uma chamada direta:** `animate_tick.rs` é do crate `horizon_client_lib`, que o `horizon-client` (onde moram o `glue` e o `machinery-basis-core`) depende, então o helper
  não pode chamar o `glue`. O animador entra por registro, como o `machinery_sprite_ties` (H26) faz no sentido contrário.
- **Java:** `ClientLevel.animateTick` e `doAnimateTick` (667 vezes, a 16 e a 32), `BlockTile.animateTick`.
- **Hook mais próximo:** nenhum. `native_block_highlight` e `native_visuals` não são por bloco sorteado.
- **Hook proposto:** H39 `block_animate_tick`: `engine`, o estado, a posição e o jogador, chamado por `do_animate_tick` com o bloco sorteado, que dispensa a injeção e o módulo.
- **Risco:** renomear `block_animate_tick`, mudar a lista de argumentos (`tables`, `engine`, `ctx`, `state`, `pos`, `player`), ou `AnimateContext::registry`; o módulo exige que `animate_tick.rs` não
  tenha `use ...::*` de topo (o planejador recusa). O `next_double` do `LegacyRandom` do Horizon faz a conta em `double` (2^-53) e o jogo em `float` (`BitRandomSource.nextDouble`,
  `long * 1.110223E-16F`): a posição sorteada de cada partícula difere até 1,8e-8 (o glue refaz a conta do jogo a partir de `next_int_bound(1 << 26)` e `(1 << 27)`, que são um passo do gerador cada).
- **Conferido em jogo** (2026-11-08, `horizon`): taxa de partículas por segundo e prints contra o original, ver o item "Partículas da máquina acesa" do plano.

### `components-ingredient` (machinery_basis)

- **Estado:** em uso desde 2026-11-08. **Alvo cru:** `horizon_client_lib::crafting::Ingredient::parse_object` (`crafting.rs`), HEAD, com o `o` (o JSON do objeto) e o `items` (`&Items`). **Hook:** `ingredient` v1 (`native_ingredient`, HEAD, só o contrato). **Módulo:** `components-ingredient`, em `crafting.rs`. **Proposta:** H38.
- **Helper:** `components_stash.rs` chama `machinery_components::stash(o, &items.item_tag_order)`, que guarda uma cópia das tags de item só se o objeto é `neoforge:components` e cita `#tag`; `components_resolve.rs` chama `machinery_components::resolve(kind, data, items)`, que
  preenche a lista do hook com os ids, as tags guardadas e o `neoforge:or` achatado (e larga a cópia). O teste dos componentes é da mod (`Tree` em `machinery_basis_core::components_ingredient`, montada em `machinery_basis/glue.rs` do `Ingredient` com `custom` do Horizon).
- **Por que é necessária:** o `neoforge:components` cai no braço de tipos de mod de `parse_object`, que chama o hook `ingredient`; sem dono a lista fica vazia e a receita de cozimento inteira (Smelter) é descartada. O hook recebe `kind`, o JSON e uma lista para preencher, mas não o `Items`: `items` de um `DataComponentIngredient`
  é um `HolderSet` e `#tag` não se resolve ali. Tentado antes, e recusado pelo `horizon-mixins`: um RETURN em `parse_object` (`E_UNSUPPORTED_CONTROL_FLOW`, o corpo tem `?`) e um HEAD cancelável (`E_UNSUPPORTED_SIGNATURE`, o resultado `Option<Self>` não é concreto).
- **Hook mais próximo:** `ingredient` (collect), sem as tags. **Hook proposto:** H38 `ingredient` v2: o mesmo contrato com `&Items` (ou um resolvedor de `HolderSet`) entre os argumentos. Com ele a injeção crua e o módulo saem e a `machinery_basis` implementa o hook tipado.
- **Risco:** renomear `parse_object` ou `native_ingredient`, ou chamar o hook de outro lugar que não `parse_object` (a cópia das tags só vale dentro da chamada). Não cobre o teste do `Ingredient` pelo próprio Horizon (forno, mesa de crafting): o hook `ingredient_test` não tem dono, então ali o ingrediente casa só pelo item. O Smelter da mod usa a árvore própria.
- **Conferido em jogo** (2026-11-08, `horizon`): `h2_componentes.py smelter`, ver o item `DataComponentIngredient` do plano.

## Hooks do catálogo com estrutura privada

Os 45 abaixo usam hooks do catálogo, mas o helper alcança o que não é API. Nenhum é injeção crua, e todos ficam de pé enquanto o
Horizon não renomear o campo. O pedido ao Horizon é de helpers de leitura e escrita (H8), não de hooks novos.

| Injeção (mod) | Hook | O que toca de privado | O que o resolve |
|---|---|---|---|
| `update_shape` (basis) | `update_shape` | `level.registry.state(state).properties` (lê `fluid_logged`) | H8: `state_property(state, key)` no `GameLevel` |
| `state_for_placement` (basis) | `state_for_placement` | `level.world.block(x, y, z)` (o vizinho; desde 2026-11-09 o hook também recebe `fluid` e usa `context.yaw`, `pitch` e `face`, públicos, para o `facing` e o `fluid_logged` dos blocos com regra em `machinery_basis/placement`) | H8: `block_state_at(dimension, pos)` só leitura, no `GameLevel` |
| `use_item_on` (basis) | `use_item_on` | `game.world.game_time`, `game.with_level(...)` para agendar o tick do fluido, e, para o `ItemUpgrade.useOn` (que o Mekanism e o Heat Generator usam por aqui), `game.held_mut()` e, nas funções de cada mod, `game.world.block_entities.get/get_mut` e `game.world.modified.insert` | `level_game_time` já existe; H8: `schedule_fluid_tick` no `Game`, `mod_block_entity`, `change_mod_block_entity`, `consume_held` |
| `use_item_on` (mekanism) | `use_item_on` | o módulo `configurator`: `game.held_mut()`, `swing_arm`, `spawn_item`, `game.world.block_entities`, `loot_block_entity`, `block_loot_at`, `pending` e `with_level` (a chave inglesa que desmonta, o esvaziar, o cartão) | H8: `set_held`, `drop_item`, `dismantle_block` |
| `use_item` (mekanism) | `use_item` | `game.held_mut()` (o cartão que se esvazia no ar) | H8: `set_held` |
| `key` (mekanism) | `key` | `game.inventory`, `game.selected` (o item de modo na mão principal) | H8: `held` |
| `mouse_scrolling` (mekanism) | `mouse_scrolling` | `game.inventory`, `game.selected`, `game.visual_ticks()` | H8: `held` |
| `gui_layer_post` (mekanism) | `gui_layer_post` | o texto do modo sobre a barra e o texto do HUD: `game.health_hud` (`left_height`, `right_height`), `game.titles.overlay_time`, `game.mode`, `game.creative`, `game.player`, `game.ui.hide_gui`, `game.armor`, `game.offhand`, `game.inventory`, `game.settings.text_background`, `game.gui.atlas`, `game.visual_ticks()` | H88 |
| `screen_render_post` (mekanism) | `screen_render_post` | o menu radial: `game.gui.atlas` (o desenho do anel, dos ícones e dos nomes), `game.visual_ticks()` | H88 |
| `screen_mouse_scrolled` (mekanism) | `screen_mouse_scrolled_pre` | `game.visual_ticks()` (o `ScrollIncrementer` do menu) | H88 |
| `net_from_client` (mekanism) | `net_from_client` | `game.lan.take()` e `game.as_guest`, `game.held_mut()` (o `PacketModeChange` troca o modo da pilha na mão de quem mandou) | H12, H8 |
| `tick_block` (basis) | `tick_block` | `GameLevel::behaviours` (`facts`, `collision_shape`, `tag`, `set_block`, `destroy_block`) e o registro de estados, para o `FlowingFluid` de um fluido de mod (módulo `liquid-glue`) | H47 |
| `dispense_item` (basis) | `dispense_item` | `Game::with_level`, `Game::audio` e o registro, para o balde de um fluido de mod no dispensador | H47 |
| `block_changed` (basis) | `block_changed` | `GameLevel::registry` (o nome e a propriedade `level` do bloco novo), para agendar o tick de um bloco de líquido que entrou no mundo | H47 |
| `block_spread` (basis) | `block_spread` | `GameLevel::registry` e `block_state`, para recusar a água e a lava do vanilla que entram de lado num bloco de líquido de mod (`BaseFlowingFluid.canBeReplacedWith`) | H47 |
| `consume_effect` (mekanism) | `consume_effect` | `Game::player` (`health.food`), para a fome e a saturação do balde de pasta nutritiva lidas da configuração | nenhum helper de fome no `Host` |
| `use_item` (basis) | `use_item` | `game.world.game_time`, e, para o balde de um fluido de mod, `Game::pick`, `Game::aim`, `Game::reach`, `Game::may_build`, `Game::can_eat`, `Game::audio`, `Game::with_level` e `Game::world.block` (a cola passou a pedir `&mut Game`); antes só `game.world.game_time` | `level_game_time`, que já existe |
| `block_entity_tick` (mekanism) | `block_entity_tick` | `game.world.game_time`, o método do módulo `slot-accepts` no ejetor, e, para premiar as receitas ao jogador que tirou o resultado (o tick não tem jogador agindo), `game.lan.take()` e `game.as_guest` com o estado dele no lugar do anfitrião | `level_game_time`, H7 e H27 entregando o jogador |
| `menu_quick_move` (mekanismgenerators) | `menu_quick_move` | `game.inventory` (a barra de atalhos e o inventário, que o shift-clique do Heat Generator enche), `game.armor` e `game.offhand` (a barra de armadura que ele também enche e esvazia), e `game.items()` (em que slot cada peça se veste, `canEquip`) | o mesmo helper de escrita no inventário de quem age, e um de leitura de `equip_slot` |
| `menu_quick_move` (mekanism) | `menu_quick_move` | `game.recipes().cooking` (via `machine_recipes`), `game.inventory` (a barra de atalhos e o inventário, que o shift-clique do Mekanism enche) | H8: `cooking_recipes(kind)`; falta um helper de escrita no inventário de quem age |
| `block_entity_tick` (generators) | `block_entity_tick` | `game.res.providers().burn_time(world_items!)`, `game.world.chunks` e `world_biomes!`; a temperatura do ar (módulo `glue` da `machinery_basis`, `ambient_temperature`) lê também `game.streaming.biome_zoom` (o `BiomeManager.getBiome`) e `game.world_flat` (o nível do mar de um mundo plano, -63) | H8: `burn_time(stack)`, `biome_temperature(dimension, pos)`, que devolva a temperatura de `Biome.getTemperature(pos, seaLevel)` do bioma que `Level.getBiome(pos)` dá, com o nível do mar do mundo (o `BiomeInfo::temperature_at` do Horizon fixa 63, 32 e 0 por dimensão) |
| `window_frame` (basis) | `frame` | `game.mod_screen` (escreve `screen.slots`), `game.renderer`, `game.remote`, `game.world.block_entities` | H9 |
| `window_key` (basis) | `screen_key_pressed_pre` | `game.mod_screen` (o bloco da tela aberta) | H9 |
| `block_entity_submit` (mekanism) | `block_entity_submit` | o módulo `cube`: `game.native_block`, `basis::tile_view`, `BlockEntityDraw::{push, pop, translate, scale, rotate_about, vertices_mut}`, `native_draw_cube`, `native_draw_model` | H91 |
| `window_press` (basis) | `screen_mouse_button_pressed_pre` | `game.mod_screen`, `game.renderer` (o tamanho da tela em pixels de GUI) | H9 |
| `window_release` (basis) | `screen_mouse_button_released_pre` | `game.mod_screen` | H9 |
| `window_drag` (basis) | `screen_mouse_dragged_pre` | `game.mod_screen`, `game.renderer` | H9 |
| `window_scroll` (basis) | `screen_mouse_scrolled_pre` | `game.mod_screen`, `game.renderer` | H9 |
| `window_net` (basis) | `net_from_client` | `game.lan.hub.guests`, `hide_menu_slots` e `play.crafting.machine` do menu de LAN, `game.container`, `game.world.block_entities` | H12 |
| `window_player_in` (basis) | `player_logged_in` | `native_net_send_to_player` (`pub(super)` do `game`), pelo módulo `machinery_basis_windows` | H12 |
| `client_tick` (mekanism) | `tick` | `game.world.block_entities` (os tiles que o cliente vê), `game.native_window`, e pelo módulo `tile-sound` o motor de som (`game.audio`), o relógio (`game.clock`) e a posição do jogador (`native_player`, `pub(super)` de `native_hooks` e fora da lista `helpers` do catálogo) | H19 e H8 |
| `place_block` (mekanism) | `place_block` | `game.world.block_entities.get_mut`, `game.world.modified` | H8: `mod_block_entity`, `change_mod_block_entity` |
| `loot_drops` (mekanism) | `loot_drops` | `game.world.block_entities.get` (o `CustomName` do tile que quebrou: o Horizon já o tirou do mundo quando rola a tabela de um jogador, então o nome vem do que `destroy_block` guardou) | H8: `mod_block_entity`; `NativeLootDrops` sem o block entity é o buraco: `block_entity: Option<&Tag>` resolve |
| `container_opened` (mekanism) | `container_opened` | `game.world.block_entities.get` | H8: `mod_block_entity` |
| `destroy_block` (mekanism) | `destroy_block` | `game.spawn_orbs` (`ExperienceOrb.award`, privado de `game.rs`), e `game.world.block_entities.get` (o nome do tile, guardado antes de o Horizon o tirar do mundo, para `loot_drops`) | H8: `award_experience(at, points)`, `mod_block_entity` e `block_entity` em `NativeLootDrops` |
| `window_net_server` (basis) | `net_from_server` | o módulo `machinery_basis_windows`, que guarda os tipos de upgrade nos estáticos da lib | H12 |
| `explosion_detonate` (mekanism) | `explosion_detonate` | `game.spawn_orbs` (`ExperienceOrb.award`, privado de `game.rs`) | H8: `award_experience(at, points)` |
| `place_block`, `loot_drops`, `container_opened` e `destroy_block` (mekanismgenerators) | os mesmos | os da linha de `place_block` etc. do `mekanism`: `game.world.block_entities` e `game.world.modified`, pelas funções `name_*` da `machinery_basis` | H8: `mod_block_entity`, `change_mod_block_entity` e `block_entity` em `NativeLootDrops` |
| `menu_button` (mekanism) | `menu_button` | `game.spawn_orbs` (`ExperienceOrb.award`, privado de `game.rs`), para a aba de xp (`COLLECT_XP`) dar a experiência ao jogador que clicou; e, para `ServerPlayer.awardRecipes` + `triggerRecipeCrafted`, `game.level.recipe_book.add_recipes`, `game.recipe_unlocked`, `game.trigger_recipe_crafted` e a macro `world_stations!` (o livro de receitas e os critérios de `advancements.rs`) | H8: `award_experience(at, points)` e `award_recipes(player, recipes, items)` |
| `server_data_loaded` (mekanism) | `server_data_loaded` | `game.recipes().cooking` e `game.native_recipes_of_type` (pelas funções `machine_recipes`, `conversions` e `energy_conversions` da `machinery_basis`, que a varredura de receitas incompletas chama para ler já na carga o que o primeiro uso leria), e `native_tell` e `native_server_players` (documentados, na lista de baixo) para a mensagem a quem está no servidor | H8: `cooking_recipes(kind)`; o resto é o que `native_menu_quick_move` já pede |
| `neighbor_changed` (basis) | `neighbor_changed` | `level.native_state_block(state)` (a que bloco o estado pertence: o hook só dá o id do estado), e a fila estática da `machinery_basis` que `flush_signals` esvazia | H34: o nome do bloco no hook, ou o sinal dos vizinhos lido dentro do aviso |
| `server_tick` (basis) | `server_tick` | `game.world.block_entities.get` (o id do bloco do menu), os campos privados que os `SourceFn` dos mods leem do mundo, `game.container` e `game.lan.hub.guests[].menu` (quem tem o menu de que bloco aberto, o espectador inclusive: o `recount`), e, pelo `mb_in_levels` da `machinery_basis` (módulo `glue`), `game.levels_to_tick`, `game.enter_level` e `game.dim`, porque no começo do tick só o último nível está no lugar; e `native_net_send_to_player` (`pub(super)` do `game`) | H33: um hook que o hub chame na fase das entidades, com o bloco do nível certo, e um helper de envio ao jogador; H36: a lista de jogadores com o menu aberto |

Java de cada um: `IStateFluidLoggable.updateFluids` e `BlockStateHelper.getStateForPlacement` (`BlockStateHelper.java:102-115`) para
os dois primeiros; `ItemUpgrade.useOn` (`ItemUpgrade.java:28-45`) para o `use_item_on` da `machinery_basis` (o Mekanism e o Heat Generator); `MekanismRecipeType.java:334` e
`TileEntityEnergizedSmelter.java:26` para a receita de cozimento que vem do `RecipeType.SMELTING` do jogo;
`TileEntityHeatGenerator.java:111`, `MekanismUtils.getBurnTime` (`MekanismUtils.java:469-478`) e `HeatAPI.getAmbientTemp`
(`HeatAPI.java:42-46`, que chama `Biome.getTemperature(pos, seaLevel)`) para o gerador; `MekanismContainer.broadcastChanges` (`MekanismContainer.java:647-667`) e
`ServerPlayer.tick` (`ServerPlayer.java:598-606`) para o `server_tick` (H33), e `MekanismTileContainer.openInventory` e `closeInventory` (`MekanismTileContainer.java:53-62`) para a conta de quem tem o menu aberto (H36).

Por que cada um existe em vez de usar helper que já há:

- `game.world.game_time` tem `level_game_time` no catálogo. É migração nossa, não pedido.
- `game.held_mut()` existe porque `native_take_items` só tira item "plain" (sem componente), e o upgrade tem o componente
  `upgrade_type`. O Java faz `stack.consume(added, player)`, que consome da mão qualquer pilha.
- `game.recipes().cooking` existe porque `native_recipes_of_type` só devolve `recipes().custom`, os tipos de receita do mod, e a
  Energized Smelter lê as receitas de `minecraft:smelting`.
- `burn_time` e a temperatura do bioma não têm helper algum. A temperatura hoje ignora o ajuste por altura que o Java faz (nota do
  inventário do plano).

### Dependências a vigiar

Não são injeção, e o risco é menor, mas quebram numa mudança do pin.

Helpers documentados com número de API em `docs/native-mods.md`, fora da lista `helpers` de `native-hooks.json`. O catálogo diz
que não cobre helpers além da lista; pedir que entrem nela dá contrato a eles:

| Helper | Documentado em | Quem usa |
|---|---|---|
| `native_energy_insert`, `native_move_energy` | API 142 | `distribute_energy`, `transmit_energy` |
| `native_update_output_neighbors` | API 64 | `publish_comparator` (mekanism e mekanismgenerators), chamado dentro do `native_block_entity_tick` da própria máquina: funciona com o tile fora do mundo porque `analog_output` responde por uma tabela do mod |
| `native_container_slots` | API 48 | ejetor, hoppers |
| `native_tell`, `native_server_players` | API 21 | `player_logged_in` e `server_data_loaded` do Mekanism (a mensagem de receita incompleta, `IncompleteRecipeScanner`); `native_server_players` também no som e nos tipos de upgrade da `machinery_basis`. O jogador da janela entra por memória no servidor integrado (`integrated.rs`), por isso o `player_logged_in` e o `native_tell` o alcançam em mundo de um jogador só |
| `native_state_block`, `native_state_with_property` | API 164, 76 | `has_analog_output` (mekanism e mekanismgenerators), `update_shape`, `state_for_placement` |
| `native_schedule_fluid_tick` | API 32 | `update_shape`, `use_item_on` |

Helpers em fragmento ainda pendente de carimbo (`tools/native-hook-fragments/`), que o README do próprio Horizon manda não
usar antes de ter número de API:

| Helper | Fragmento | Quem usa |
|---|---|---|
| `native_sneaking`, `native_component` | `host_bridge.json` | `use_item_on` da `machinery_basis` (`ItemUpgrade.useOn`) |
| `native_draw_model` | `host_bridge.json` | spike do cabo, Fase 4 |
| `native_text_width` | `client_services.json` | `machine_panel`, `menu_tooltip` |
| hook `container_accepts` | `container_accepts.json` | previsto (Fase 2); hoje `hook =` dá `E_HOOK_UNKNOWN` |

Pedido: carimbar esses fragmentos (`python3 tools/stamp_native_hooks.py`) e listar os quatro helpers documentados em `helpers`.

O título da tela (`machine_panel`) desenha o componente de nome inteiro (cor, negrito, filhos) com `HudBatch::text_styled` e mede com
`game.gui.atlas.font.width_text`, os mesmos de `menu_tooltip`, porque `native_text` e `native_text_width` só aceitam `&str` sem estilo
(`Text::literal`). Pedido: `native_text_styled(batch, &Text, at, rgb, shadow)` e `native_text_width_styled(&Text)`. O componente
chega do servidor ao cliente por NBT (`write_network`/`read_network`, no canal `machinery_basis:name`) porque `native_screen_title`
devolve `Text::plain()` e o título que o Horizon manda no pacote de abertura (`display_name(custom_name: &str, ...)`, `lan_actions.rs:1280`)
guarda só o texto do nome (`World::block_names` é `String`).

## Módulos (34)

Um `[[module]]` enxerta um arquivo nosso em um arquivo do Horizon e, por ser filho dele, enxerga o que é privado ali.

| Módulo | Arquivo alvo | Para quê | Precisa de |
|---|---|---|---|
| `glue` (machinery_basis) | `game.rs` | liga a lib ao jogo: tudo `pub(in crate::game)`. `Game::mb_in_levels` (2026-11-08) roda uma ação em cada nível ticado com o nível no lugar, para o `ServerTickStart`: usa `levels_to_tick`, `enter_level` e `Dim`, privados de `levels.rs` e `game.rs` | H8, H33 e o que está em [Dependências a vigiar](#dependências-a-vigiar) |
| `glue` (mekanism) | `game.rs` | liga as máquinas e a tela: 2.691 linhas em 2026-11-08 | idem |
| `glue` (mekanismgenerators) | `game.rs` | liga o Heat Generator: 1.143 linhas em 2026-11-08 | idem |
| `cube` (mekanism) | `game.rs` | os cubos de energia no jogo (`TileEntityEnergyCube`, 2026-11-11, frente `cubo-energia`): o objeto vivo `EnergyCube` entre os ticks, o NBT (`energy`, `energy_amount` do criativo), os slots de carga e descarga contra a energia dos itens, o ejetor para os vizinhos (`native_energy_insert`), as faces de energia por instância (H1), o comparador, o Configurator e o cartão, o item posto e solto, e o desenho da block entity (`block_entity_submit`: as portas e os LEDs de cada lado, o núcleo translúcido que gira). Usa `BlockEntityDraw::vertices_mut` (`pub(super)`) para pôr o alfa no núcleo | idem, H91 |
| `configurator` (mekanism) | `game.rs` | o Configurator e o cartão de configuração: o clique do item e do bloco (`use_item_on`, `use_hand`, `predict_item_on_block`), o componente da pilha, a cópia e a colagem do cartão, a troca de modo (tecla, rolagem, pacote), a seleção parcial dos transmissores. Usa `Game::held`, `held_mut`, `offhand`, `hand`, `aim`, `reach`, `swing_arm`, `spawn_item`, `loot_block_entity`, `block_loot_at`, `pending` e `with_level(player_destroy_block)` | H8, H71, H72 |
| `world-overlay` (mekanism) | `game/world_primitives.rs` | o quadrilátero da sobreposição da face do Configurator no formato dos primitivos do mundo (`PrimitiveGeometry.quads`, privado de lá), para a injeção `world_overlay` | H87 |
| `block-rows` (mekanism) | `mod_content.rs` | a tabela das linhas de jogo dos blocos de máquina (luz do `active` e da lava, reação ao pistão, oclusão, resistência), gerada do original em jogo, para a injeção `active_light` | H18, H49 |
| `creative-tabs-table` (mekanism) | `mod_content.rs` | a tabela e a conta das pilhas da aba criativa (código puro, sem tipos do jogo), para a injeção `creative_tabs` | H43 |
| `glass-skip` (mekanismgenerators) | `block_render.rs` | o estado padrão do vidro estrutural, do vidro do reator e da matriz de foco (estáticos atômicos) e a regra de quem pula a face de quem (`BlockTileGlass`, `BlockReactorGlass`, `BlockLaserFocusMatrix`), que `glass_ids` anota e `glass_skip` consulta | H51 |
| `block-rows` (mekanismgenerators) | `mod_content.rs` | a tabela das linhas de jogo dos 23 blocos do Generators (luz do `active` e da lava, reação ao pistão, oclusão), gerada do original em jogo, para a injeção `block_rows` | H18, H49 |
| `creative-tabs-table` (mekanismgenerators) | `mod_content.rs` | o que o Generators acrescenta às abas do vanilla (`addToExistingTabs`), gerado do original em jogo, para a injeção `creative_tabs` do Generators | H43 |
| `windows` (machinery_basis) | `game/native_hooks.rs` | a mecânica de janela e a janela de upgrades: `OpenNativeScreen` tem campos privados de lá | H9 |
| `slot-accepts` (machinery_basis) | `game/machines/native_items.rs` | `isValid` por slot, privado de `machines` | H7 |
| `tile-sound` (machinery_basis) | `game.rs` | o som contínuo de um tile (`updateSound` e `TileTickableSound`): usa `Game.audio` (`play_instance`, `stop_and_release`, `instance_mut`), `Game.clock` e `native_player` (os pés do jogador local), privados | H19 |
| `composter-wrapper` (machinery_basis) | `game/composter.rs` | o sorteio das camadas de um item e a subida de um nível da composteira, como o `ComposterWrapper` do NeoForge faz | H29 |
| `container-dim` (machinery_basis) | `horizon-client-render/src/hud.rs` | o estado "a tela aberta é de uma máquina nossa" (`machinery_container_dim`), que o HEAD de `HudBatch::fill` lê e o `Game` escreve | H22 |
| `sprite-ties` (machinery_basis) | `horizon-client-render/src/hud.rs` | os retângulos do atlas que as telas de mod desenham, para a injeção `sprite_ties` | H26 |
| `armor-bar` (machinery_basis) | `game.rs` | a barra de armadura do lado do cliente: a lista de slots, a posição, o "fora" da tela e o mapa dos índices do menu | H31, H32 |
| `model-transform` (machinery_basis) | `math.rs` do `horizon-assets` | a conta do `transform` do modelo do NeoForge (o JSON de `TransformationHelper`, `applyOrigin`, a rotação do blockstate, `Direction.rotate`) em código puro, sem tipos do Horizon, para as injeções `model_transform_decode` e `model_transform_bake`; `core/tests/model_transform.rs` o compila como está | H67 |
| `obj-material` (machinery_basis) | `obj.rs` do `horizon-assets` | o `Kd` e o `neoforge_TintIndex` de um `.mtl` (a conta do `ObjMaterialLibrary` e da cor do vértice) em código puro, sem tipos do Horizon, para as injeções `obj_material_parse` e `obj_material_quads` | H16 |
| `bar-menu` (machinery_basis) | `menu.rs` (crate `horizon_client_lib`, `pub`) | os cinco slots no fim do menu de um tipo com barra, e o registro dele, que o crate de baixo enxerga (a `machinery-basis-core` só liga ao `horizon-client`) | H31 |
| `bar-machine` (machinery_basis) | `game/lan_crafting.rs` | deixa pronto o menu da conexão com os cinco slots antes do clique (`Menu::new` devolve `Self`) | H31 |
| `bar-hub` (machinery_basis) | `lan/containers.rs` | `Hub::machinery_sync_bar`: a armadura e a mão que o clique mudou vão ao cliente com a resposta dele | H31 |
| `bar-quick-move` (machinery_basis) | `game/native_quick_move.rs` | o shift-clique que parte de um slot da barra, com as guardas de `lan_mod_quick_move` e o `mayPickup` | H31 |
| `components-ingredient` (machinery_basis) | `crafting.rs` | o `Ingredient::parse_object` guarda as tags de item e o hook `ingredient` as resolve, para o `neoforge:components` do cozimento (H38; ficha em [`components-ingredient`](#components-ingredient-machinery_basis)) | H38 |
| `liquid-glue` (machinery_basis) | `game.rs` | os fluidos de mod como bloco de líquido: o `FlowingFluid` (`machinery_basis_core::liquid`) ligado ao tick agendado, ao balde (pôr e pegar a fonte, com o raio do balde vazio), ao dispensador e à recusa da água do vanilla; lê o mundo pelos métodos privados de `GameLevel` e `Game` | H47 |
| `water-like` (machinery_basis) | `motion.rs` (crate `horizon_client_lib`, `pub`) | a tabela dos estados de bloco de líquido de mod por número de estado, que o `Game` escreve e o RETURN `fluid_state` lê (a biblioteca não vê a `machinery-basis-core`) | H47 |
| `animate-registry` (machinery_basis) | `animate_tick.rs` (crate `horizon_client_lib`, `pub`) | os animadores que os mods registram para o `animateTick` dos blocos de mod, que a injeção `animate_tick` chama: o crate do jogo registra, o de baixo chama | H39 |
| `worn-gear` (machinery_basis) | `game.rs` | o desenho do equipamento vestido com modelo próprio: a pose da parte do corpo de um mob (`render::part_transform`) ou de um armor stand (`render::part_matrix`), `baked_model_vertices` (privado de `minecarts.rs`), `model_catalog.standalone`, `armor_stand_assets`, `armor`, `player_model` e `res.player_arm` | H55 |
| `worn-hides` (machinery_basis) | `mobs/anim.rs` (crate `horizon_client_lib`, `pub`) | as partes do modelo que o equipamento vestido esconde (`Poses::visible`), que as injeções `worn_hide` e `worn_hide_player` aplicam: o crate do jogo diz o que cada item esconde, o de baixo aplica | H55 |
| `rules` (mekanism) | `game.rs` | regras puras, sem tipo do jogo | nada do Horizon |
| `rules` (mekanismgenerators) | `game.rs` | regras puras, sem tipo do jogo | nada do Horizon |

- **`slot-accepts`:** o ejetor (`TileComponentEjector.outputItems`, `TileComponentEjector.java:255-279`) pergunta à
  `ResourceHandler` do vizinho o `isValid` de cada slot antes de transacionar, e o Horizon não simula inserção:
  `native_insert_item` não tem `simulate`. O método `machinery_slot_accepts` chama `containers.can_place` e
  `mod_container_accepts`, privados de `machines`. Hook mais próximo: `container_accepts`, que é uma resposta do mod ao Horizon,
  não uma consulta (e é fragmento pendente). Risco: renomear `View`, `containers()` ou `can_place`.
- **`composter-wrapper`:** o ejetor que sai por cima ou por baixo de uma composteira passa pelo `ComposterWrapper`
  (`neoforge/transfer/item/ComposterWrapper.java`): a face de cima sorteia `Compostable.layers()` uma vez por tentativa
  (`getComposterValue`), recusa se o sorteio deu 0 sem gastar o item, e sobe o nível em um, não nas camadas sorteadas. O caminho
  do Horizon para um mod (`native_insert_item`, que cai em `composter_container_insert`) é o do hopper vanilla (`InputContainer`):
  soma as camadas sorteadas e gasta o item mesmo quando o sorteio dá 0. E `native_container_slots` não mostra a composteira. O
  módulo expõe `machinery_composter_layers` (o sorteio, com `compostable`, `provider_int` e o contexto do bloco, privados de
  `composter.rs`) e `machinery_composter_raise` (`composter_insert` com uma camada: nível, evento de jogo, tick agendado ao chegar
  a 7 e evento de nível 1500). A decisão é da lib (`machinery_basis_core::composter`, conferida contra o rastro do original). Risco:
  renomear `compostable`, `provider_int`, `ComputeContext`, `with_level` ou `composter_insert`.
- **`armor-bar`, `bar-menu`, `bar-machine`, `bar-hub` e `bar-quick-move`:** a barra de armadura e mão (ver [A barra de armadura e mão](#a-barra-de-armadura-e-mão-machinery_basis-9-injeções)).
  `bar-menu` mora em `menu.rs`, que é do crate `horizon_client_lib`, e não enxerga a `machinery-basis-core` (o Cargo a liga só ao `horizon-client`, que depende dele), então
  guarda o próprio registro dos slots (`declare`, `declared`, `extend`), que `armor-bar` preenche junto do da lib. `bar-hub` mora em `lan/containers.rs`, também de lá.
- **`glue`:** existe porque um `[[crate]]` não depende de `horizon-client` (Cargo proíbe o ciclo) e o crate `[api]` só vê o
  `dyn Host`. Os três `glue` somem quando o `Host` cobrir H1 a H8 e o código passar a viver no crate `[api]` como trait. Hoje os
  três `[api]` estão vazios. O Horizon cita um crate de API estável para crates de mod (`#3897`), que resolveria o resto.
- **`rules`:** não tocam o Horizon. São módulos porque usam `machinery_basis_core`, que é `[[crate]]` de outro mod, e a documentação
  proíbe dependência de crate por caminho. Não confirmei se é essa a única razão: ver [o que não foi confirmado](#o-que-não-foi-confirmado).
  Se `[[crate]]` de um mod puder depender de `[[crate]]` de outro, saem do número.

## Provadas em spike

### Cabo: fase 3 e 4

Mudança `spike: validate the critical injections and the translucent cable contents (do not push)`. O código foi escrito para
referência: a versão real age só nos blocos do Mekanism. Em cada linha, o alvo é HEAD, salvo quando dito.

| Injeção | Alvo | O que provou | Java | Proposta |
|---|---|---|---|---|
| `spike_energy_at` | `native_energy_at`, HEAD cancelável | faces por instância: Crusher com a chave ficou em 0, sem ela recebeu 32 | Fase 2, `TileComponentConfig` | H1 (a mesma de `energy_capacity`) |
| `spike_save_level` | `Game::save_level`, HEAD | carimbou 19 cabos antes do `modified.drain` | `TileEntityTransmitter.java:136`, `DynamicBufferedNetwork.java:156-159` (`saveAdditional`, `updateSaveShares`) | H13 |
| `spike_remove_entity` | `GameLevel::remove_entity` (`spill.rs:490`), HEAD | lê a block entity antes de ela sair | `TileEntityTransmitter.setRemoved`, `:165` | H14 |
| `spike_override_gameplay` | `Plan::override_gameplay` (`mod_content.rs:1319`), HEAD | luz 8 só no Crusher, 13 nos demais (em uso desde 2026-11-07 como `active_light`) | `BlockStateHelper.java:83-94`, `Attributes.java:147-157` | H18 |
| `spike_frame_visuals` | `native_visuals::native_frame_visuals`, HEAD | linhas de várias caixas, só no nosso bloco com a ferramenta | `BlockTransmitter.java:103`, `TileEntityTransmitter.java:234`, `MultipartUtils.java:39` | H15 |
| `spike_obj_kd` | `horizon_assets::bake::obj_quad`, RETURN | cor do quad vezes o `Kd` do `.mtl` (0,8): sem isso tudo sai 20% mais claro (em uso desde 2026-11-12 como `obj_material_parse` e `obj_material_quads`, que leem o `Kd` e o `neoforge_TintIndex` do arquivo; a `spike_obj_kd` pôs 0,8 em todo material do Mekanism) | NeoForge `ObjGeometry.makeQuad` | H16 |
| `spike_sort_translucent` | `pvp::Game::outline_fill`, HEAD | ordena os quads de `TRANSLUCENT_ITEM_OVERLAY` do fundo para a frente: manchas sumiram | `MekanismRenderType.java:20,61,69` (`sortOnUpload`) | H17 |

Hooks do catálogo que o spike usou e que são limpos: `standalone_models`, `block_entity_update_tag`. Os que tocam estrutura privada:
`block_highlight` (`game.held()`, `game.world`, `game.res.facts()`, `game.pack.assets.registry`) e `block_entity_submit`, na versão
do spike, com os mesmos campos. Substitutos que já existem: `held`, `block`, `block_property`, e `view_hit` e `camera` no lugar de
`game.pick()` e `game.aim()`. Se `view_hit` dá o mesmo raio que `pick()` não foi conferido.

Riscos: `spike_energy_at`, `spike_save_level` e `spike_remove_entity` quebram em renomeação. `spike_override_gameplay` depende de
`NewBlock` e do formato de linha. `spike_sort_translucent` é a mais frágil: altera a pipeline de render, e
ela reordena todos os vértices de `outline_fill`, o que muda com qualquer ajuste de camada.

### Aba de upgrades (em uso desde 2026-11-06)

A aba e a janela saíram do spike e estão em uso: os 10 itens `window_*` e o módulo `windows` acima, mais o `menu_quick_move` do
Mekanism, que passou a ter `&mut Game`. O que o spike provou em jogo (diff `~/.cache/rust-mekanism/spike-upgrade/spike-upgrade-A-fim.diff`
e a página `gotchas/spike-aba-de-upgrades-viabilidade.md` do ai-memory) continua valendo; a etapa 1 desta versão não rodou em jogo.

Decisões que o código tem e o spike não tinha:

- **Clique.** `GuiWindow.mouseClicked` (`GuiWindow.java:100-125`) com `InteractionStrategy.ALL`: o clique DENTRO do retângulo da janela é
  consumido pela janela, porque ela é um `AbstractWidget` ativo e o `clicked` dele é o `isMouseOver` (`GuiElement.java:494-496`); só o
  clique FORA dela atravessa para a tela. Os slots atrás ficam bloqueados por `GuiMekanism.isHovering` (`:702-706`), que é o filtro em
  `slot_at`. O spike cancelava sempre dentro da janela, e o código faz o mesmo, exceto sobre um slot virtual, onde o clique segue para o
  jogo (`GuiVirtualSlot.mouseClicked` chama `slotClicked`).
- **Esc.** `GuiMekanism.keyPressed` (`:577-585`) pergunta a cada janela: a que não está fixa fecha e consome a tecla; fixa, a tecla
  segue e a tela fecha (`GuiWindow.keyPressed`, `:152-162`).
- **Seleção por jogador no servidor.** `net_from_client` guarda a escolha por (contêiner, jogador) e esconde os slots virtuais no menu
  de quem está sem janela. Quem sai sem avisar é largado no `server_tick` (`recount`; até 2026-11-08 era o hook `mod_container_viewers`): o hook de jogador (`player_container_close`)
  não dispara para o anfitrião.
- **Posição.** Fica no cliente, em `config/Mekanism/client.toml` (a `client_config` do mod aponta o arquivo), tabela `[window.upgrade]` com `x`, `y` e `pinned`, gravada em `close()`
  e lida na primeira janela: persiste entre sessões como o `lastWindowPositions` do `ClientConfig` (`ClientConfig.java:59,145-158`),
  que o NeoForge grava em `config/Mekanism/client.toml`.

- **Heat Generator (2026-11-07).** O gerador usa a mesma janela sem injeção nova: a spec dele (`UpgradeWindowSpec`) e as duas funções que o
  mod dá (tirar um upgrade e pôr da mão) entram em `Game::mb_window_register`, e as injeções `window_*` já valem para qualquer bloco
  registrado. Para isso o `ItemUpgrade.useOn` (antes o `use_item_on` do Mekanism) passou a rodar no `use_item_on` da `machinery_basis`, e a
  limpeza da seleção de quem sai do menu sem fechá-lo (`prune`, antes dentro do `mod_container_viewers` do Mekanism) ganhou a injeção
  `window_viewers`, também da `machinery_basis`. O Mekanism perdeu uma injeção (`use_item_on`) e a basis ganhou uma: o total fica em 66.

Riscos: `slot_at` e `draw_inventory_slots` são o núcleo do desenho de inventário e mudam sempre que o Horizon mexe na tela. O módulo
em `native_hooks.rs` depende de um arquivo de 11 mil linhas. `hide_menu_slots` e `play.crafting.machine` são o protocolo de menu de LAN:
mudar quem fala com o convidado quebra a seleção por jogador, sem erro.

## Previstas

Do plano, sem compilar. O Java está nas fases correspondentes.

| Prevista | Fase | Alvo (do plano) | Proposta |
|---|---|---|---|
| Faces e contêiner por instância | 2, 3 | HEAD cancelável em `native_fluid_at`, `Containers::view`, `can_take` e `native_container_accepts` | H2 |
| Regra de slot que vê o outro slot | feita por contexto de clique (2026-11-07), sem H20 | `menu_slot_rules` v1 não tem `Game`, nem a posição, nem o outro slot; ver [A regra de slot do clique](#a-regra-de-slot-do-clique) | H20 |
| Seleção do Configurator com `context_outline` (não conta: o spike já cobriu) | decisão 1 | o spike resolveu por `block_highlight` mais `native_frame_visuals`: `context_outline` devolve uma caixa só | H15 |
| `native_screen_slot` (não conta: o spike já cobriu) | 1 | HEAD, "não testado": o spike da aba cobriu o caso por H9 | H9 |

Java: `TileEntityMekanism.java:253,327,1436` e `client/sound/SoundHandler.java` para o som; `TileEntityCombiner.java:114-117`
(`containsRecipeAB`, `containsRecipeBA`) para a regra de slot.

Sem injeção conhecida ainda: a energia do Mekanism vive em `i64` no NBT próprio porque o `ModEnergyBuffer` é `i32` (decisão 2 do
plano). O ejetor alcança a composteira pelo módulo `composter-wrapper` (H29).

## Hooks e pedidos propostos

Todos são propostas: nenhum existe no pin. Os nomes seguem o catálogo. "Contrato" é o `mod_api.contract` (`listen`, `collect`,
`replace`, `cancel`) e `out` o campo de saída. Quando o catálogo exigir nova versão, é `v2` e o v1 fica como está. Os
exemplos são o corpo do método do trait `Hooks`, na forma da documentação do Horizon.

### H1 `block_entity_energy_instance`

- **Lado:** servidor. **Contrato:** `replace`, `out = buffer`.
- **Quando dispara:** dentro de `native_energy_at`, depois de achar o buffer do bloco e a block entity em `pos`, a cada leitura:
  `native_energy`, `native_energy_insert`, `native_energy_extract`, `native_move_energy`. Não dispara enquanto a block entity está
  em tick (ela está fora do mundo, como hoje).
- **Assinatura:**

```rust
fn native_block_entity_energy_instance(
    &self, dimension: &str, pos: [i32; 3], entity: &ModBlockEntity, buffer: &mut Arc<ModEnergyBuffer>,
)
```

- **Exemplo:**

```rust
fn block_entity_energy_instance(&self, _host: &dyn Host, event: &mut BlockEntityEnergyInstanceEvent<'_>) {
    if let Some(capacity) = capacity_from_nbt(&event.entity.nbt) {
        event.buffer = Arc::new(ModEnergyBuffer { capacity, max_insert: capacity, ..*event.buffer });
    }
}
```

- **Nota:** serve também às faces por instância (campos `insert_faces` e `extract_faces` do mesmo buffer), que o spike do cabo
  provou. O limite `i32` do buffer continua e é outro pedido.

### H2 `block_entity_container_instance`

- **Lado:** servidor. **Contrato:** `replace`, `out = container`. Irmão: `block_entity_fluids_instance`, mesma forma para os
  tanques (`native_fluid_at`).
- **Quando dispara:** sempre que o jogo consulta o `ModContainer` do bloco em `pos`: `Containers::view`, `can_take`,
  `native_container_accepts` e `native_container_slots`. A resposta vale para a consulta, não é guardada.
- **Assinatura:**

```rust
fn native_block_entity_container_instance(
    &self, dimension: &str, pos: [i32; 3], entity: &ModBlockEntity, container: &mut Option<ModContainer>,
)
```

- **Exemplo:**

```rust
fn block_entity_container_instance(&self, _host: &dyn Host, event: &mut BlockEntityContainerInstanceEvent<'_>) {
    if let (Some(container), Some(config)) = (event.container.as_mut(), side_config(&event.entity.nbt)) {
        container.inputs = config.input_slots_per_face();
        container.outputs = config.output_slots_per_face();
    }
}
```

- **Nota:** `block_entity_container` v1 continua declarando o tipo de bloco, e H2 afina por instância, como `energy_instance` faz com
  `block_entity_energy`. É o que a Fase 2 (`TileComponentConfig`, `ConfigInfo`, `DataType`) precisa: lados coloridos por instância.
- **Por que a frente `config-lados` (2026-11-09) não o usou:** o hopper vizinho, o único consumidor de contêiner que a máquina tem hoje, é emulado
  pela própria máquina (`tick_hoppers`), com as faces do `DataType` da instância, e o rastro dos hoppers (`slots_*`, `identity_*`) já
  confere essa emulação com o original. Expor as faces pelo `ModContainer` ligaria os dois caminhos ao mesmo tempo (o hopper do Horizon
  e o da máquina), e três coisas faltam para o resto: (1) `Containers::can_take` e `can_place` não olham a face (`outputs` e `inputs` são
  uma lista só), (2) `can_take` não consulta a regra do slot (`isValidForExtraction` de quem está de fora: o slot de entrada não
  deixa tirar, o de infusão só o que o tanque não converte), e (3) não há como o `ModContainer` mudar por instância. O dropper, o
  dispenser e o ejetor de outra máquina que apontam para uma máquina não a alcançam (nem antes), e a Fase 3 (os canos de item) é a
  que vai precisar. A ficha segue **prevista**, com estes três pontos como o que o hook tem de resolver (o terceiro é o H2; os dois
  primeiros acrescentam a face e a regra do slot ao `can_take`).

### H3 `block_fluid`

- **Lado:** cliente e servidor. **Contrato:** `replace`, `out = fluid`.
- **Quando dispara:** uma vez por estado do bloco, na carga de conteúdo, quando a tabela de fatos é montada (onde hoje roda
  `set_state_properties`). O resultado alimenta o motor de fluidos (fonte, tick, propagação), o mesher, a luz do fluido e o
  agendamento no `update_shape`, como num bloco alagado.
- **Assinatura:**

```rust
fn native_block_fluid(&self, block: &str, properties: &[(String, String)], fluid: &mut Option<NativeBlockFluid>)
pub struct NativeBlockFluid { pub fluid: String /* "minecraft:lava" */, pub source: bool }
```

- **Exemplo:**

```rust
fn block_fluid(&self, _host: &dyn Host, event: &mut BlockFluidEvent<'_>) {
    if event.properties.iter().any(|(k, v)| k == "fluid_logged" && v == "lava") {
        event.fluid = Some(NativeBlockFluid { fluid: "minecraft:lava".into(), source: true });
    }
}
```

- **Alternativa de dados**, equivalente, que o Horizon pode preferir: um campo no JSON do bloco
  (`"fluid_state": { "property": "fluid_logged", "values": { "water": "minecraft:water", "lava": "minecraft:lava" } }`), no
  lugar do hook. O que importa é o motor ler o fluido do estado, em vez da tabela de fatos ser reescrita por quem não a conhece.

### H4 `screen_tooltip` v2

- **Lado:** cliente. **Contrato:** `collect` sobre uma estrutura, `out = tooltip`.
- **Quando dispara:** no mesmo lugar do v1, em `draw_native_screen_tooltip`, com a mesma condição (sem item no cursor e sem slot
  sob ele). Se v1 e v2 existirem, v2 roda depois.
- **Quem precisa:** hoje duas injeções cruas fazem este trabalho, as duas com o nome `widget_tooltip` (uma em `mekanism_native`,
  outra em `mekanismgenerators_native`, ver [Injeções cruas em uso](#injeções-cruas-em-uso-28)). O tooltip da aba de avisos, da
  barra de energia, da barra de químico e do medidor de lava fica por cima ou por baixo do widget, e o da aba de avisos sai com o
  cabeçalho amarelo. Com o v2, as duas injeções saem.
- **Assinatura:**

```rust
fn native_screen_tooltip_v2(&self, block: &str, data: &[i32], at: [f32; 2], tooltip: &mut NativeScreenTooltip)
pub struct NativeScreenTooltip {
    /// Texto com cor ARGB própria, como o Horizon já converte as linhas por dentro. `u32::MAX` é a cor padrão.
    pub lines: Vec<(Text, u32)>,
    /// Retângulo do widget, `[x, y, largura, altura]` em pixels de GUI: o tooltip fica acima ou abaixo dele
    /// (`MenuTooltipPositioner`). Vazio, fica junto do cursor.
    pub anchor: Option<[f32; 4]>,
}
```

- **Exemplo:**

```rust
fn screen_tooltip_v2(&self, _host: &dyn Host, event: &mut ScreenTooltipV2Event<'_>) {
    if let Some(widget) = widget_at(event.block, event.at) {
        event.tooltip.lines.extend(widget.lines());
        event.tooltip.anchor = Some(widget.rect());
    }
}
```

### H5 `component_types` v2

- **Lado:** servidor e cliente. **Contrato:** `collect`, `out = types`, a mesma assinatura do v1.
- **Quando dispara:** antes de o conteúdo carregar, como o v1.
- **O que muda no contrato:** o tipo registrado também entra na lista persistente dos comandos (`reg.persistent_components`), que
  dá `/give`, `/item` e predicados `components`, e é aceito por `minecraft:select` (`component`) e `has_component` nos modelos.
- **Exemplo:** o mesmo corpo de `component_types` v1 do mod: `types.push("acme:upgrade_type".into())`.

### H6 `item_model_node`

- **Lado:** cliente. **Contrato:** `replace`, `out = node`.
- **Quando dispara:** em `foreign::decode_node`, para cada nó de `type` não vanilla e desconhecido, ao carregar recursos, antes do
  desvio por `MODEL_KEYS`. É uma função livre fora de `Game`: precisa de um callback instalado pelo executável, como o de
  `menu_slot_rules`.
- **Assinatura:**

```rust
fn native_item_model_node(kind: &str, json: &serde_json::Map<String, Value>, node: &mut Option<ItemModelNode>)
```

- **Exemplo:**

```rust
fn item_model_node(&self, event: &mut ItemModelNodeEvent<'_>) {
    if event.kind == "acme:component_item" {
        event.node = Some(select_on_component("acme:upgrade_type", event.json));
    }
}
```

### H7 `item_container_accepts` (helper do `Host`)

- **Lado:** servidor. **Tipo:** consulta, sem escrita.
- **O que faz:** responde a decisão que `native_insert_item` toma por slot (`can_place` mais a resposta do `container_accepts` do mod
  dono) sem escrever no slot nem mexer no cooldown do hopper.
- **Assinatura:**

```rust
fn item_container_accepts(&self, dimension: &str, pos: [i32; 3], slot: usize, stack: &ItemStack, face: Direction) -> bool
```

- **Exemplo:** o ejetor (`TileComponentEjector.outputItems`) pergunta antes de gravar na cópia do vizinho:
  `host.item_container_accepts(dim, at, slot, &stack, face)`.
- **Alternativa menos útil:** um `simulate` em `native_insert_item`. Responde "quanto cabe" para a pilha inteira, e o ejetor
  precisa da regra de cada slot para a transação.

### H29 `composter_item_handler` (helper do `Host`)

- **Lado:** servidor. **Tipo:** consulta e escrita.
- **O que faz:** dá a um mod o `ResourceHandler<ItemResource>` que o NeoForge dá à composteira, no lugar do caminho do hopper:
  `composter_layers(dimension, pos, stack) -> Option<i32>` (`Compostable.layers().get(lootContext, 0)`, com o sorteio do nível,
  `None` sem o componente) e `composter_raise(dimension, pos)` (o `onRootCommit` do wrapper depois de subir um nível).
- **Assinatura:**

```rust
fn composter_layers(&mut self, dimension: &str, pos: [i32; 3], stack: &ItemStack) -> Option<i32>
fn composter_raise(&mut self, dimension: &str, pos: [i32; 3])
```

- **Exemplo:** um ejetor de mod com o lado de baixo configurado como saída, sobre uma composteira: pergunta as camadas do item
  uma vez, e só se der mais de 0 e o nível for menor que 7 sobe um nível e gasta um item.
- **Alternativa menos útil:** `native_insert_item` com o contrato do wrapper. Resolve a escrita, mas não a transação do ejetor, que
  pergunta antes de gravar e desfaz se a resposta não vale.

### H8 helpers de leitura e escrita

Todos no `Host`, sem hook novo. As cinco primeiras linhas são novas. A última só pede o carimbo do que já está nos fragmentos.

| Helper | Lado | Assinatura | Substitui |
|---|---|---|---|
| `cooking_recipes` | servidor e cliente | `fn cooking_recipes(&self, kind: &str) -> &[Cooking]` (`smelting`, `blasting`...) | `game.recipes().cooking` |
| `burn_time` | servidor | `fn burn_time(&self, stack: &ItemStack) -> i32` (`CookingFuel`, o que a fornalha lê) | `game.res.providers().burn_time(world_items!)` |
| `biome_temperature` | servidor | `fn biome_temperature(&self, dimension: &str, pos: [i32; 3]) -> Option<f32>` (`Biome.getTemperature(pos, seaLevel)`) | `world_biomes!`, `game.world.chunks` |
| `consume_held` | servidor | `fn consume_held(&mut self, count: u32) -> u32` (a pilha da mão, com componentes) | `game.held_mut()` |
| `block_state_at`, `state_property`, `schedule_fluid_tick` | servidor | `block_state_at(dimension, pos) -> BlockStateId` e `state_property(state, key) -> Option<&str>` no `GameLevel` dos hooks `update_shape` e `state_for_placement`; `schedule_fluid_tick` também no `Game` | `level.world.block`, `level.registry`, `game.with_level` |
| `mod_block_entity`, `change_mod_block_entity`, `sneaking`, `component`, `text_width` | | já em `host_bridge.json` e `client_services.json` | só falta o carimbo |

`consume_held` existe porque `native_take_items` pega só item "plain", sem componente, e a mão do upgrade tem.

### H9 `screen_layout`

- **Lado:** cliente. **Contrato:** `replace`, `out = slots`.
- **Quando dispara:** a cada frame com a tela do mod aberta, antes do desenho e da entrada. `slots[i] = None` esconde o slot:
  não é desenhado, não é alvo de hover, clique ou arrasto. Hoje o `native_screen` devolve um layout estático e não basta.
- **Assinatura:**

```rust
fn native_screen_layout(&self, block: &str, data: &[i32], slots: &mut Vec<Option<[f32; 2]>>)
```

- **Exemplo:** a janela de upgrades aberta põe os dois slots virtuais em `window_pos + offset`, e fechada devolve `None`.

### H10 `screen_slot_at`

- **Lado:** cliente. **Contrato:** `replace`, `out = found`.
- **Quando dispara:** no fim de `Game::slot_at`, com a resposta do Horizon já em `found`. Vale para hover, tooltip, clique, as teclas
  1 a 9, F e Q, e a divisão por arrasto.
- **Assinatura:**

```rust
fn native_screen_slot_at(&self, block: &str, data: &[i32], at: [f32; 2], found: &mut Option<usize>)
```

- **Exemplo:** sob a janela, `found` vira o slot virtual na posição, ou `None` se o slot da máquina está coberto.

### H11 `screen_draw_over_slots`

- **Lado:** cliente. **Contrato:** `listen`.
- **Quando dispara:** entre o desenho dos slots comuns e o dos slots da camada de cima, antes do tooltip e do item no cursor. O mod
  declara a camada de cima em `NativeScreen::overlay_slots: Vec<usize>`.
- **Assinatura:**

```rust
fn native_screen_draw_over_slots(&self, block: &str, data: &[i32], batch: &mut HudBatch, origin: [f32; 2])
```

- **Exemplo:** o mod pinta o painel da janela. Os itens dos slots de cima saem depois, sobre o painel.

### H12 `menu_hide_slots` (helper do `Host`)

- **Lado:** servidor. **Tipo:** escrita.
- **O que faz:** esconde os slots `slots` do menu de um jogador, só no menu dele, e manda o menu de novo (o spike fazia
  `hide_menu_slots` e zerava `play.crafting.machine`).
- **Assinatura:** `fn menu_hide_slots(&mut self, player: u128, slots: &[usize])`, com `u128` o uuid do jogador.
- **Exemplo:** o cliente avisa pela mensagem `net_from_client` que fechou a janela de upgrades, e o servidor esconde os slots
  virtuais dele, para o shift-clique não encher o slot de quem está com a janela fechada.

### H13 `block_entity_save`

- **Lado:** servidor. **Contrato:** `listen`.
- **Quando dispara:** uma vez por block entity de mod quando o chunk dela é serializado (autosave, descarga do chunk, fechar o
  mundo), antes de o compound ser gravado. É o `saveAdditional` do Java.
- **Assinatura:**

```rust
fn native_block_entity_save(&mut self, dimension: &str, pos: [i32; 3], entity: &mut ModBlockEntity)
```

- **Exemplo:** a rede de cabos grava a parte do buffer que cabe a este cabo (`save shares`) no NBT dele.

### H14 `block_entity_removed`

- **Lado:** servidor. **Contrato:** `listen`.
- **Quando dispara:** quando a block entity vai sair do mundo por quebra ou troca de bloco, antes de sair, ainda legível e gravável.
  Cobre o caminho de `native_edit_blocks`, que `block_changed` não vê. Não dispara na descarga do chunk, que é `chunk_unload`.
- **Assinatura:**

```rust
fn native_block_entity_removed(&mut self, dimension: &str, pos: [i32; 3], entity: &ModBlockEntity)
```

- **Exemplo:** o cabo removido devolve à rede o que ele guardava (`setRemoved`).

### H15 `visuals` v2

- **Lado:** cliente. **Contrato:** `listen`, com uma lista nova de saída.
- **Quando dispara:** igual ao `visuals` v1, a cada frame com mundo visível, com o mesmo orçamento por mod.
- **Assinatura:** o v1 recebe `rims: &mut Vec<NativeBlockRim>` (o contorno da forma de seleção do bloco em `pos`) e
  `lights`. O v2 ganha `boxes`:

```rust
pub struct NativeOutlineBox { pub min: [f64; 3], pub max: [f64; 3], pub argb: u32, pub width: f32 }
fn native_visuals_v2(&self, rims: &mut Vec<NativeBlockRim>, lights: &mut Vec<NativeColoredLight>, boxes: &mut Vec<NativeOutlineBox>)
```

- **Exemplo:** com `block_highlight` cancelando o contorno padrão, o mod põe as caixas da parte do cabo que o raio toca.

### H16 leitura do `Kd` e do `neoforge_TintIndex` do `.mtl` (correção, sem hook)

O NeoForge multiplica a cor do quad pelo `Kd` do material (`ObjGeometry.makeQuad`) e dá ao quad o `neoforge_TintIndex` dele, e `parse_mtl` do Horizon lê só `map_Kd`. Os materiais dos transmissores têm `Kd 0.8`: sem isso, tudo sai 20% mais claro; o
vidro do transportador logístico e o conteúdo têm `neoforge_TintIndex 0`. O pedido é que o Horizon leia `Kd` e `neoforge_TintIndex` ao assar um `.obj` (`MtlTexture` ganha cor e índice de tinta, e `obj_quad` os põe no quad). Não é hook porque o dado já está no arquivo:
qualquer mod com `.obj` ganha.

- **Hoje:** as injeções `obj_material_parse` e `obj_material_quads` e o módulo `obj-material`, desde 2026-11-12 (ficha "O `Kd` e o `neoforge_TintIndex` do `.mtl`"). Antes era a `spike_obj_kd` do spike.
- **Risco do que existe hoje:** os da ficha da injeção. Se o Horizon passar a ler o `Kd`, tirar as duas injeções e o módulo.

### H17 ordenação da camada translúcida (correção, sem hook)

O original usa `sortOnUpload` (`MekanismRenderType.java:20,61,69`) e ordena os quads translúcidos do fundo para a frente. O Horizon
não ordena e escreve profundidade, então um conteúdo desenhado antes apaga o de trás. O pedido é que o Horizon ordene a camada de
quads translúcidos das block entities por distância (`VertexSorting.DISTANCE_TO_ORIGIN`), o que beneficia qualquer mod.

### H18 `properties.light` condicional (dado, sem hook)

O Horizon já aceita condição por estado em `random_ticks` (`ticks.applies(combo)`), mas `properties.light` é um inteiro por bloco
(`mod_content.rs:1345`). O Java dá luz por estado (`BlockStateHelper.applyLightLevelAdjustments`, `BlockStateHelper.java:83-94`).
O pedido é aceitar em `light` a mesma condição de `random_ticks`. Elimina `active_light` (mekanism) e `block_rows` (mekanismgenerators).

### H19 `looping_sound`

- **Estado:** em uso, pelo módulo `tile-sound` (machinery_basis, `tile_sound.rs`), conferido em jogo em 2026-11-06: o som da Enrichment
  Chamber toca ao ligar, para ao parar e fica mudo com o upgrade de silêncio no máximo (`MEKSOUND` no stderr e vídeo com áudio).
  O módulo guarda o `InstanceId` por tile e chama `Audio::play_instance` (com `Ticking::Native`), `stop_and_release` e
  `engine().instance_mut` a cada tick do cliente, e a regra (cooldown de 20 ticks, `isFullyMuffled`, alcance, verificação a cada 20 a 39
  ticks) vem de `machinery_basis_core::sound`, repetida do Java. A posição do jogador é `native_player()` (os pés, `Minecraft.player.position()`; até 2026-11-08 era a câmera do último quadro, que fica na altura dos olhos e, em terceira pessoa, atrás do jogador). `baseSoundVolume` e `enableMachineSounds` vêm do `Mekanism/client.toml` pelos hooks `register_configs`, `config_loading` e `config_reloading` (API 397, limpos), no `mekanism`.
- **Lado:** cliente. **Tipo:** helpers do `Host`.
- **O que faz:** toca um som em laço preso a um ponto e o para, e o som cessa sozinho se o chunk descarrega. Hoje só há
  `native_play_local_sound` (uma vez, com atraso).
- **Assinatura:**

```rust
fn looping_sound(&mut self, sound: &str, at: [f64; 3], volume: f32, pitch: f32) -> Option<SoundHandle>
fn stop_sound(&mut self, handle: SoundHandle)
```

- **Exemplo:** a máquina ativa chama `looping_sound` ao ligar e `stop_sound` ao parar (`TileEntityMekanism` e `SoundHandler`).
- **O que falta no helper proposto:** mudar o volume de um som em laço já tocando (`set_sound_volume(handle, volume)`), perguntar se ele
  ainda toca (`is_sound_active`) e dar a posição do jogador local no tick do cliente: `native_player` existe no `Game` (os pés) mas não está na lista de helpers do catálogo, e `native_camera` é a câmera, não os pés, e só existe depois de um quadro.

### H27 `menu_result_taken`

- **Lado:** servidor. **Contrato:** `listen`.
- **Quando dispara:** depois de um jogador tirar itens de um slot de resultado (`results`) de um menu de contêiner de mod, no clique, no
  shift-clique e no arrasto, para o anfitrião e para os convidados.
- **Assinatura:** `native_menu_result_taken(dimension, pos, slot, taken: &ItemStack)`, com o jogador que age lido pelos helpers.
- **Exemplo:** a máquina solta a experiência guardada (`InventoryContainerResultSlot.onTake`, `RecipeCacheLookupMonitor.awardUsedRecipesAndPopExperience`).

### H20 `menu_slot_rules` v2

- **Lado:** servidor. **Contrato:** `listen`, com o menu inteiro em vista.
- **Quando dispara:** no mesmo lugar do v1, mas com uma visão dos outros slots do menu, da posição do bloco que o menu mostra e do `Game` em leitura.
- **Assinatura:** `native_menu_slot_rules_v2(menu, pos, slot, stack, may_place, max_stack, slots: &[Option<ItemStack>])`.
- **Exemplo:** o Combiner aceita no segundo slot só o item que forma receita com o que está no primeiro
  (`TileEntityCombiner.java:114-117`), e o Metallurgic Infuser aceita no slot de infusão o item que vira o químico que o tanque aceita e que forma receita
  com o item da entrada (`TileEntityMetallurgicInfuser.java:135-136,150`).
- **Em uso hoje:** a v1 (`menu_slot_rules`, `listen`) é a que o crate `[api]` da `machinery_basis` implementa. Com a v2, `result_take_before` e
  `menu_click_before` deixam de montar o contexto do clique.

### H21 `screen_slot_highlight`

- **Lado:** cliente. **Contrato:** `cancel`, `out = handled`: o mod desenha o realce e o Horizon não desenha o padrão.
- **Quando dispara:** no começo de `draw_slot_highlight`, a cada chamada, nas duas camadas (`front` diz qual). Vale com a tela
  aberta, e o mod recebe o bloco da tela e os dados dela, como em H11.
- **Assinatura:**

```rust
fn native_screen_slot_highlight(&self, block: &str, data: &[i32], front: bool, at: [f32; 2], batch: &mut HudBatch) -> bool
```

- **Exemplo:**

```rust
fn screen_slot_highlight(&self, _host: &dyn Host, event: &mut ScreenSlotHighlightEvent<'_>) {
    if event.front && is_slot_screen(event.block) {
        draw_back_highlight(event.batch, event.at);
        event.handled = true;
    }
}
```

- **Nota:** o original desenha a frente do realce com o sprite de trás (`VirtualSlotContainerScreen.java:70-75`). O hook deixa
  cada mod escolher o desenho de cada camada, e hoje a injeção crua `slot_highlight` faz esse papel. Não é um pedido de correção
  do Horizon: o padrão do Horizon, sem mod, está certo para o vanilla.

### H31 barra de armadura e mão no tipo de menu

- **Lado:** servidor e cliente. **Tipo:** dado, no JSON do tipo de menu (`data/<id>/horizon/menu/`), mais o que o motor já tem.
- **O que faz:** `{ "slots": 4, "armor_bar": { "x": -20, "y": 11, "offhand_offset": 0 } }`. O `Menu::new(Kind::Mod)` põe, depois dos 36 do inventário,
  quatro `Role::Armor` (a cabeça primeiro, sobre os índices 39 a 36 do jogador) e a mão (índice 40), como o `Kind::Inventory` já faz; a tela de mod põe os ids 500 a
  504 nessas posições, o `menu_slot` os mapeia, o hub os sincroniza com a resposta do clique e o `native_menu_quick_move` os recebe como origem, com `mayPickup` conferido antes.
  `offset` ausente ou -1 deixa a mão fora (`armorSideBar(x, y)`).
- **Exemplo:** o Heat Generator e os outros geradores do Mekanism declaram `armorSideBar(-20, 11, 0)`, o Energy Cube `(180, 41, 0)` e a Induction Matrix `(-20, 41, 0)`.
- **Alternativa menos útil:** slots virtuais por hook (`screen_layout`): cada mod reimplementa as regras de `ArmorSlot` e o equipar com som e evento que o motor já faz.

### H32 elementos da tela fora da imagem

- **Lado:** cliente. **Tipo:** dado, em `NativeScreen`: `elements: Vec<[f32; 4]>`.
- **O que faz:** os retângulos (relativos ao canto da tela) que recebem o mouse fora da imagem de 176 por 166 (abas laterais, a janela de upgrades), cada um com uma condição de ativo e visível agora que o mod lê dos data slots da tela
  (a aba de xp só com o upgrade; a de avisos só com algum aviso, que depende da máscara de erros e do redstone, então uma função sobre os data slots serve melhor que o índice de um slot). `clicked_outside` e
  o clique de contêiner os tratam como `GuiMekanism.findElementOver`: o clique neles não é "fora" (não solta o item do cursor). O vanilla não tem esse caso (o
  `AbstractContainerScreen.hasClickedOutside` só olha a imagem), e o NeoForge ainda impede que um slot sob o mouse seja "fora".
- **Exemplo:** `NativeScreen { elements: vec![[-26.0, 109.0, 26.0, 26.0], [-26.0, 137.0, 26.0, 26.0]], .. }` para as abas de calor e de energia.

### H33 `menu_data_sync`

- **Lado:** servidor. **Tipo:** hook, `listen`, por bloco de mod com menu aberto.
- **O que faz:** o hub o chama na fase das entidades (onde o `ServerPlayer.tick` chama o `broadcastChanges`), uma vez por jogador com o menu de um bloco de mod
  aberto, com a posição e a dimensão, e o mod devolve o valor vivo de cada dado sincronizado (ou os pacotes que mandaria). É o que o `AbstractContainerMenu.broadcastChanges`
  (`AbstractContainerMenu.java:197-215`, chamado pelo `ServerPlayer.tick`, `ServerPlayer.java:606`, antes dos block entities) e o `MekanismContainer.broadcastChanges`
  (`MekanismContainer.java:647-667`) fazem com cada `ISyncableData`: lê o campo na hora de mandar, e não o que o bloco escreveu no próprio tick. O estado que o cliente recebe
  é o do fim do tick anterior, com a energia que chegou depois do tick da máquina e o erro que ela ainda não recalculou (o overlay de nível >= 0,98 da barra de energia,
  `GuiBar.java:70`, depende disso).
- **Hoje (2026-11-08, parte 2 dos trackers):** o hook `server_tick` da `machinery_basis` roda `Game::mb_container_broadcast` no `ServerTickStart` (a cada passada, também com o servidor parado,
  onde o jogador continua andando): para cada sessão de menu entra no nível do bloco (`Game::mb_in_levels`), pede ao dono do bloco o valor vivo (`SourceFn`) e manda o `ContainerSync` do jogador,
  um pacote por jogador, por `native_net_send_to_player`; o `tick_lan` do mesmo tick o leva ao cliente, antes dos block entities. O estado é o mesmo que o do Java, e o que um pacote do jogador muda
  antes do `ServerTickStart` entra no quadro. Pede estrutura privada porque no começo do tick só o último nível está no lugar (`levels_to_tick`, `enter_level`) e porque o `SourceFn` lê
  `world.block_entities`.
- **Exemplo:** `fn menu_data_sync(&mut self, player: u128, dimension: &str, pos: [i32; 3]) -> Option<Vec<u8>>`: o mod devolve os bytes do pacote de atualização desse jogador.
- **Risco do que existe hoje:** renomear ou mudar a assinatura de `levels_to_tick`, `enter_level`, `Dim` ou `world.block_entities`; o canal de mod limita a 32 mensagens por jogador por tick.

### H34 `neighbor_changed` v2

- **Lado:** servidor. **Tipo:** hook, `listen`, por `neighborChanged` entregue.
- **O que faz:** o `neighbor_changed` v1 observa cada `neighborChanged` antes de o bloco tratá-lo, com a dimensão, a posição, o estado e o bloco que mudou. A v2 acrescentaria o
  que o Mekanism lê dentro do aviso (`BlockTile.neighborChanged` -> `TileEntityMekanism.updatePower` -> `level.hasNeighborSignal(pos)`): `signal: i32` (o `getBestNeighborSignal`,
  ou `hasNeighborSignal` como bool) calculado pelo motor de redstone, que já tem o nível e o `Ticks` em mãos, e o nome do bloco do estado.
- **Hoje:** `flush_signals` (cru) lê o sinal no fim da operação que avisou, e o hook v1 só anota. Só diverge do Java se a mesma operação avisa um bloco e depois muda o sinal dele sem
  avisar, o que nenhuma operação do jogo ou dos mods daqui faz; os rastros `neighbors_*` repetidos no simulador e em jogo (repetições separadas em operações) não mostram diferença.
- **Exemplo:** `fn neighbor_changed(&mut self, dimension: &str, pos: [i32; 3], block: &str, changed: &str, signal: i32, moved: bool)`.
- **Risco do que existe hoje:** `Game::flush_spill` e a ordem de `with_level` (ver a ficha de `flush_signals`).

### H35 `block_entity_changed`

- **Lado:** servidor. **Tipo:** hook, `listen`, por `BlockEntity.setChanged` e por `Level.removeBlockEntity`.
- **O que faz:** avisa que o block entity de `pos` mudou (ou saiu), com a dimensão e o tipo, antes de os comparadores relerem. No NeoForge esse é o caminho do `onNeighborChange`
  de todo vizinho (`Level.updateNeighbourForOutputSignal`), que um mod de máquina usa para reler o que vem dos vizinhos (o sinal de redstone, uma capability).
- **Hoje:** `container_changed` (cru) cobre os contêineres do vanilla; falta o resto dos block entities do vanilla e `removeBlockEntity`.
- **Exemplo:** `fn block_entity_changed(&mut self, dimension: &str, pos: [i32; 3], block_entity: &str, removed: bool)`.
- **Risco do que existe hoje:** `Game::container_changed`, `Game::chest_other_half` e `world.block_entities`.

### H40 `NativeConfigKind::Synced` (pedido, sem injeção)

- **Lado:** servidor e cliente. **Tipo:** variante de `NativeConfigKind` (API 397).
- **O que faz:** o `ModConfig.Type.SYNCED` do NeoForge 26.3: o arquivo vive em `config/`, é lido quando o servidor de um mundo vai começar e solto quando ele para (como o `Server` de hoje), pode ser trocado por uma cópia em `<mundo>/syncedconfig/<arquivo>` (a
  pasta se chama `syncedconfig`, e o `readme.txt` que o NeoForge escreve nela explica; o `Server` do Horizon procura em `serverconfig/`) e o servidor manda o texto aos convidados, que o carregam enquanto a conexão dura (`ConfigSync`, `ClientNeoForgeMod`).
- **Hoje:** o Mekanism e o Mekanism Generators registram os arquivos de servidor (`general`, `machine-usage`, `tiers`, `generators`, `generator-storage`) como `Server`, com hooks do catálogo e sem injeção. As duas diferenças são o nome da pasta da cópia do mundo e o convidado de LAN, que não recebe os valores do
  anfitrião (o texto do efeito dos upgrades, que o cliente calcula, usa os padrões). Decisão 15 do plano.
- **Exemplo:** `NativeConfigKind::Synced`, com o `native_config_loading` do convidado chamado com o texto do anfitrião (`NativeConfigFile` com `kind = Synced`).
- **Risco do que existe hoje:** nenhum: usa só o hook do catálogo.

### H41 saída de áudio a 48.000 Hz (pedido, sem injeção)

- **Lado:** cliente. **Tipo:** correção do mixer (`audio.rs`), sem hook.
- **O que faz:** o cliente do original deixa o OpenAL Soft abrir a saída a 48.000 Hz e reamostrar cada som (os `.ogg` são de 44.100 Hz) no próprio motor. O Horizon abre o dispositivo a 44.100 Hz, mixa com passo 1 (`audio.rs::mix`) e deixa o PipeWire converter.
- **Hoje:** o som da máquina sai 0,045 % mais alto no port (0,004 dB), com a razão port/original de 1,0004 abaixo de 500 Hz subindo a 1,09 (8 a 12 kHz) e 1,29 (16 a 20 kHz), medido num mundo e em posições controlados (frente `som-plano`, decisão 16 do plano). O reamostrador exato do original não foi identificado: pela simulação no mesmo `.ogg`, fica entre o linear e o cúbico.
- **Exemplo:** abrir a saída na taxa do dispositivo (48.000 Hz) e reamostrar no mixer como o OpenAL Soft do cliente.
- **Risco do que existe hoje:** nenhum: não há injeção.

### H42 `properties.experience`

- **Lado:** servidor. **Tipo:** dado nas `properties` do bloco (ou o hook `block_exp` chamado também quando o bloco do vanilla daria 0).
- **O que faz:** um bloco de mod declara a experiência que solta ao ser quebrado por um jogador (`min` e `max`, sorteados como `UniformInt`), como o `DropExperienceBlock` do vanilla e o `BlockOre.getExpDrop` do Mekanism (fluorita de 1 a 4).
- **Hoje:** `fluorite_experience` (cru, RETURN de `Game::block_experience`).
- **Risco do que existe hoje:** ver a ficha da injeção.

### H43 abas criativas: pilhas e abas do vanilla

- **Lado:** cliente (e o servidor que valida a aba). **Tipo:** dado no `creative_tab/*.json`.
- **O que faz:** o item de uma aba pode ser uma pilha completa (SNBT com `components`, como o `upgrade` com o tipo, o balde cheio ou o tanque preenchido), e um mod pode acrescentar itens ao fim de uma aba do vanilla (o `BuildCreativeModeTabContentsEvent` do NeoForge). Hoje só ids e só a aba do próprio mod.
- **Hoje:** `creative_tabs` (cru, RETURN de `Plan::creative_tabs`; do Mekanism e do Generators) e o módulo `creative-tabs-table` (um de cada).
- **Risco do que existe hoje:** ver a ficha da injeção.

### H44 `properties.instrument`

- **Lado:** servidor e cliente (o bloco de nota). **Tipo:** dado nas `properties` do bloco.
- **O que faz:** o instrumento do bloco de nota que toca sobre o bloco (`BlockBehaviour.Properties.instrument`): o sal do Mekanism é `SNARE` e o bloco de bio fuel `BANJO`. Hoje o instrumento é o do gabarito: não há como dizê-lo sem um gabarito do vanilla que o tenha e que seja um bloco liso.
- **Hoje:** a injeção crua `note_instrument` (ver a ficha dela em "Injeções cruas em uso"; decisão 17 do plano): o gabarito é `minecraft:lapis_block` (`HARP`) e o RETURN de `J::rename` troca o `HARP` da linha do estado dos dois blocos pelo do original. `Plan::states` tem `?` no corpo (RETURN barrado) e `override_gameplay` só alcança a linha de jogo, que não tem instrumento; `Sim::note_block_instrument`, onde o bloco de nota o lê, é de uma impl genérica que o horizon-mixins não alcança.
- **Risco do que existe hoje:** o da ficha da `note_instrument`.

### H45 `item_name` padrão do item

- **Lado:** cliente. **Tipo:** correção.
- **O que faz:** o nome de um item sem o dado de pilha `item_name` vem do componente padrão do item (`ItemStack.getItemName`, `ItemStack.getStyledHoverName`), com o estilo (cor) dele. Hoje `item_tooltip::styled_name` e `Game::stack_display_name` leem o `item_name` só do dado da pilha (`stack.data()`), então o `item_name` colorido que um bloco de mod ou um item de mod declara no JSON (`setNameColor`, `ItemDeferredRegister`) não aparece: o Mekanism põe a cor na primeira linha do tooltip pelo hook `tooltip`, e o nome sobre a barra de atalhos e o do `/give` ficam sem cor.
- **Hoje:** o hook `tooltip` (`glue::tooltip`, `rules::name_color`), só no tooltip.
- **Risco do que existe hoje:** nenhum: usa só o hook do catálogo.

### H46 `piglin_currency`

- **Lado:** servidor. **Tipo:** hook (`collect`, uma pergunta por item).
- **O que faz:** pergunta ao mod se um item é moeda de troca do piglin (`Item.isPiglinCurrency`, `IItemExtension`): o lingote de glowstone refinado do Mekanism (`ItemRefinedGlowstoneIngot`). Hoje o código dos piglins compara com `minecraft:gold_ingot` em vários pontos (`mobs/piglin.rs`, `mobs/equip.rs`).
- **Hoje:** nada: o lingote de glowstone refinado não é moeda.
- **Risco do que existe hoje:** nenhum: não há injeção.

### H47 fluidos de mod no motor

- **Lado:** cliente e servidor. **Tipo:** contrato novo (o motor aceita um fluido de mod), sem hook único.
- **Por que:** o motor só conhece água e lava. `FluidKind` (`horizon-world/src/fluid.rs`) é uma enumeração de cinco valores (`Empty`, `FlowingWater`, `Water`, `FlowingLava`, `Lava`), a máscara de tipos é `1 << kind` em `u8`, e `FluidState::from_block` decide pelo nome `minecraft:water` ou `minecraft:lava` ou pela propriedade `waterlogged`. Nada do que um mod declara chega aí: o hook `block_entity_fluids` guarda mB num tanque, sem bloco nem balde, e `docs/native-mods.md` ("A mod's own fluids in tanks") diz que o mundo só tem os fluidos do vanilla. Uma injeção não troca uma enumeração (os hooks acertam funções, e o tipo está em 30 arquivos), e disfarçar o fluido de água ou de lava daria a física dela (nadar, apagar fogo, virar fonte).
- **O que o original dá a um fluido de mod** (`FluidDeferredRegister`): o fluido parado e o que corre (`BaseFlowingFluid`), o `FluidType` (temperatura, densidade, viscosidade, luz, sons, `canSwim`, `canPushEntity`, `canDrown`, `motionScale`), o `LiquidBlock`, o balde, as tags de fluido (`c:` e `minecraft:`), o modelo de fluido com a tinta (`FluidTintSources.constant`) e a textura de sobreposição, a névoa e a sobreposição de dentro do fluido.
- **O que o port faz hoje:** um bloco de líquido por fluido (a propriedade `level` do `LiquidBlock` e a `up`), o `FlowingFluid` portado em `machinery_basis_core::liquid` e ligado por ticks agendados do bloco, o balde (pôr e pegar a fonte), o dispensador, a recusa da água e da lava do vanilla de lado (`block_spread`), e a física de entidade pela água (o RETURN `fluid_state` dá aos estados do bloco de líquido o `FluidState` da água só em `FluidResources::fluid`, que o jogador, as entidades e as flechas leem, e o mesher não). O fluxo é igual ao original tick a tick (13 cenários do Java no simulador, `flat` e `waterfall` em jogo), o desenho é um cubo por célula na altura do fluido, sem a superfície inclinada do mesher.
- **O que o port faz da entidade dentro do fluido, medido contra o original** (`rastros/fluids_entity.jsonl`, `h2_fluido_entidade.py`, desde 2026-11-12): o jogador e o porco ficam parados (`fluid_entity_travel` e `fluid_entity_mob_travel`: no 26.3 o `FluidType.move` é falso e `travelInFluid` só trata água e lava), o porco cai e para na superfície em 4,1555 com a velocidade de queda de 0,8439 guardada, como o original, o ar cai 1 por tick (jogador e porco), o fogo não apaga, a correnteza soma 0,014 por tick e o item sobe pelo `setItemMovement` igual ao rastro (y e velocidade a 5 casas). Tampouco: a névoa e a sobreposição de dentro do fluido, as tags de fluido (as 23 `c:` da datagen não entram: só citariam ids que ninguém registra como fluido), a luz do balde (`applyFluidLuminosity` dos três fluidos de luz 15), as partículas e o som de dentro, e o que a ficha H106 lista.
- **O que pedir:** `FluidKind` aberto a ids de mod (um registro de fluidos com o par parado e corrente, o tipo e o bloco), o mesher de fluido lendo o modelo e a tinta do fluido, a física de entidade lendo o `FluidType`, as tags de fluido lidas pelo motor, e o dispensador e o balde pelos mesmos caminhos da água.
- **Risco do que existe hoje:** o módulo lê campos privados de `GameLevel` e `Game` (`behaviours`, `registry`, `world`, `audio`, `rules`, `events`) e o tick agendado é por bloco, e não por fluido (um tick de fonte e um de corrente na mesma célula contam como um só); o RETURN `fluid_state` depende de `FluidResources::fluid`. Cair em quase todo commit do Horizon que mexa nesses campos.

### H106 a física de entidade lê o `FluidType` do fluido (não só água e lava)

- **Lado:** cliente e servidor. **Tipo:** contrato novo, junto do H47.
- **Por que:** o jogador (`motion.rs::player_travel` e `motion/fluids.rs`), o mob (`mobs/living.rs`) e o item (`entities/fluids.rs`) leem o fluido por três tabelas diferentes que só conhecem água e lava, e a decisão do que cada fluido faz com a entidade (`motionScale`, `canPushEntity`, `canSwim`, `canDrown`, `canExtinguish`, `fallDistanceModifier`, `isWaterLike`, `FluidType.move`, `setItemMovement`) não existe. O port faz hoje as oito `fluid_entity_*` e o `fluid_state` (a água emprestada ao bloco) para o jogador, o mob que não voa e o ar do mob.
- **O que o original faz** (`rastros/fluids_entity.jsonl`, `LivingEntity.java:2594-2607`, `Entity.java:577-581`, `ItemEntity.java:130-132`): num fluido de mod que não é água nem lava, a entidade viva não se move (sem gravidade, sem atrito), o ar cai 1 por tick, a queda anda por `fallDistanceModifier`, a correnteza soma 0,014 por tick, o item sobe pelo `setItemMovement`, e o fogo não apaga.
- **O que pedir:** um ponto de entrada por fluido de mod, no movimento da entidade viva (`travelInFluid` chama o `FluidType.move` do fluido, devolvendo se tratou) e no `baseTick` (`wasTouchingWater` por `isWaterLike`, a queda por `fallDistanceModifier`), lendo as propriedades do `FluidType` declarado pelo mod; o mesmo por mob e por item, pela mesma tabela do jogador.
- **Risco do que existe hoje:** as oito injeções dependem de métodos privados do `horizon_client_lib` (ver a ficha `fluid_entity_*`).

### H48 tinta constante no modelo de bloco

- **Lado:** cliente. **Tipo:** dado (`tint` num elemento do modelo, ou uma tinta por bloco no arquivo do bloco).
- **O que faz:** um modelo de bloco de mod multiplica a cor de uma face por uma constante, como o `FluidTintSources.constant` e o `BlockTintSources` do original. O mesher só tem fontes de cor por nome de bloco (grama, folhagem, água, redstone).
- **Hoje:** os 19 fluidos coloridos têm uma cópia da textura de líquido com a cor multiplicada no pixel (`scripts/gerar_fluidos.py`), e a água pesada, que tinge a água do vanilla, não tem a cor, porque o port não pode trazer a textura da Mojang. O vidro do tanque de fluido (`ClientRegistration.registerBlockColorHandlers`, a cor de cada tier lida de `colormap/tiers.png`) também: dez cópias (`fluid_tank_side_off` e `_on` por tier) e um modelo por tier (`dev/referencia/blocos_conteudo.py`, decisão 26).
- **Risco do que existe hoje:** nenhum: são arquivos de conteúdo.

### H49 propriedades de linha do bloco

- **Lado:** servidor e cliente (o pistão, a luz, o mob, o fluido). **Tipo:** dado nas `properties` do bloco (e, para o que depende de estado, aceitando a condição de `random_ticks`).
- **O que faz:** diz as colunas da linha de jogo que `BlockProperties` não tem: a reação ao pistão (`pushReaction`: `BlockMekanism.getPistonPushReaction` dá `IMMOVEABLE` a todo bloco com bloco de entidade), `canOcclude` e `isSolidRender` por separado (hoje `occludes: false` põe os dois falsos, e o original dá `canOcclude` verdadeiro e `isSolidRender` falso nas formas menores que o cubo),
  o amortecimento de luz e `propagatesSkylightDown`, a forma de oclusão (o mesher do Horizon esconde a face do vizinho pela forma da tabela `occlusion.nbt`, que um bloco de mod copia do gabarito, e pelo `can_occlude` de `render-definitions.json`; o port usa `occludes: false`, que desenha todas as faces, e o original esconde pela forma real), a luz condicional ao estado (H18), a resistência a explosões negativa (`AttributeCustomResistance(-1)`: o arquivo recusa), `canBeReplaced(state, fluid)` falso (`BlockMekanism.java`: um fluido não substitui a máquina) e `isPathfindable` falso (`BlockBase.isPathfindable`: a forma própria não deixa o mob passar).
- **Hoje:** `active_light` (mekanism) e `block_rows` (mekanismgenerators) (crus, HEAD de `Plan::override_gameplay`) com a tabela do módulo `block-rows` de cada um (a do Mekanism com 150 blocos: os 84 de máquina, os 62 da frente `blocos-estrutura` e os quatro cabos, que o chorus plant deixa com a reação normal ao pistão), para as colunas da linha; `canBeReplaced` e `isPathfindable` ficam no H110 e no H111 (2026-11-12); a forma de oclusão real e o `can_occlude` de volta entraram no H51.
- **Risco do que existe hoje:** ver a ficha da injeção.

### H51 forma de oclusão de um bloco de mod e corte de face entre blocos diferentes

- **Lado:** cliente. **Tipo:** dado.
- **O que faz:** (1) o mod diz a forma de oclusão do bloco (o `getOcclusionShape` do `Block`, que no comum é a de seleção): o mesher esconde a face do vizinho por `occlusion.nbt`, que um bloco de mod copia do gabarito. (2) o mod diz quais blocos pulam a face de quais (o `skipRendering` de `BlockTileGlass`: vidro estrutural, vidro do reator e matriz de foco se pulam em pares que a regra `same_block` não alcança).
- **Hoje:** `occlusion_shapes`, `glass_ids` e `glass_skip` (cruas) e o módulo `glass-skip`.
- **Risco do que existe hoje:** ver as fichas das injeções.

### H110 `canBeReplaced(state, fluid)` de um bloco de mod

- **Lado:** servidor. **Tipo:** dado nas `properties` do bloco (`replaceable_by_fluid`), lido pelo balde, pelo dispensador e pelo plenisher.
- **O que faz:** `BucketItem.emptyContents` troca o bloco por fluido quando `blockState.canBeReplaced(content)` (`replaceable || !isSolid`, e `BlockMekanism` o dá falso). O Horizon (`buckets.rs:104`) faz a conta com a bandeira `SOLID` do gabarito, e o `chorus_plant` dos cabos e transmissores não é sólido. O fluxo de fluido (`FlowingFluid`) usa a tag `washed_away_by_fluids` e já é igual ao original.
- **Hoje:** o hook `fill_bucket` do `mekanism` cancela o clique do jogador no transmissor (o `InteractionResult.FAIL` do original). O dispensador (`dispenser.rs`) e o `bucket_empty` do nível, genéricos em `LevelAccess`, são alvos que o horizon-mixins não aceita (impl genérico): ficam sem. A tag `washed_away_by_fluids` não leva nenhum bloco do mod.
- **Risco do que existe hoje:** nenhum (contrato do hook).

### H111 `isPathfindable` por tipo de caminho de um bloco de mod

- **Lado:** servidor. **Tipo:** dado.
- **O que faz:** `BlockBase.isPathfindable` (falso em toda forma própria), `BlockTransmitter` e `BlockBounding` (falso). O Horizon guarda um tipo de caminho por estado (`mob-data.json`, copiado do gabarito); os gabaritos dos 206 blocos do Mekanism e do Generators (`lodestone`, `lapis_block`, `furnace`, `glass`, `chorus_plant`, `obsidian`, minérios) são todos `BLOCKED`, o que é o `isPathfindable` falso para a terra, e o original dá falso também para água e ar. Sem divergência medida; o pedido é só uma propriedade para um bloco de mod com forma própria sobre um gabarito `OPEN`.
- **Hoje:** nenhuma injeção.

### H60 variante de blockstate de mod com modelo de reserva e item especial de mod

- **Lado:** cliente. **Tipo:** dado.
- **O que faz:** o Mekanism escreve três coisas que o Horizon não lê: uma variante de blockstate de tipo próprio (`mekanism:energy_cube_sided`, `mekanism:special/transmitter`, `mekanism:special/qio_redstone_adapter`), que o Horizon recusa inteira quando a variante não tem `model` ("blockstate definition of mekanism:basic_energy_cube not loaded: missing model", visto no log e no print, o cubo sai como o modelo ausente), um modelo com carregador de mod (`loader: mekanism:energy_cube`, com `frame`, `leds` e `ports` no lugar de `elements`, e o OBJ `.obj.mek` dos transmissores) e um item `minecraft:special` com desenhista de mod (`mekanism:fluid_tank`, `mekanism:energy_cube`), que o Horizon troca pelo modelo base. O pedido é aceitar a variante de tipo desconhecido quando ela traz um `model` (ou um `fallback`) e desenhar com ele, e o item especial desconhecido com o modelo do `base`.
- **Hoje:** nenhuma injeção: o gerador escreve o modelo comum equivalente (o quadro e os LEDs apagados do cubo de energia, o centro e o braço dos transmissores, o modelo do estado padrão no item do tanque de fluido), conferido contra o original, e o blockstate do QIO Redstone Adapter ganha o `model` (decisão 23). O que o desenhista faz com o conteúdo (o fluido, o núcleo de energia, as portas ativas) é a função de cada bloco.
- **Risco do que existe hoje:** nenhum: são arquivos de conteúdo.

### H50 limite de caixas por forma

- **Lado:** servidor e cliente (a seleção e a colisão). **Tipo:** dado: o arquivo de bloco aceitar mais de 16 caixas por forma de `shapes` e `collision_shapes`.
- **O que faz:** o original tem formas de até 47 caixas depois de cobertas (o Digital Miner), e `Plan::states` e `shape_edges` já somam as caixas que vierem. O pedido é subir o limite de `BlockShapes::check` (por exemplo 256) ou tirá-lo.
- **Hoje:** `shape_limit` (cru, HEAD cancelável de `BlockShapes::check`).
- **Risco do que existe hoje:** ver a ficha da injeção.

### H65 tags de um mod que é dependência

- **Lado:** servidor e cliente. **Tipo:** dado: um mod de conteúdo poder acrescentar valores a uma tag de outro mod de que depende (como já pode às de `minecraft` e de `c`).
- **O que faz:** a datagen do Generators acrescenta o conteúdo dele a duas tags do Mekanism (`mekanism:structures` e `mekanism:structures/common`, de bloco e de item, que o Mekanism lê para as estruturas de multibloco). No Horizon o arquivo `data/mekanism/tags/block/structures.json` num mod que não é o Mekanism recusa o mod inteiro
  (`a mod adds files in its own namespaces (data/mekanismgenerators/ and those listed in namespaces), to vanilla and convention (c) tags`; visto no log de 2026-11-09: todo o conteúdo do Generators ficou de fora até o arquivo sair), e `namespaces` não serve (dois mods não dividem namespace: `content::clash`). O pedido é aceitar tags de qualquer mod
  que o manifesto declare como dependência.
- **Hoje:** nada: os quatro arquivos não entram no Generators; o Mekanism os leva com as entradas do Generators como `{"id": "#mekanismgenerators:...", "required": false}` quando a frente `blocos-estrutura` criar as tags dele (ver o plano, "Conteúdo do Generators").
- **Risco do que existe hoje:** nenhum.

### H66 tags de mod no comando `execute if block #tag`

- **Lado:** servidor. **Tipo:** dado: o comando aceitar as tags de bloco e de item que os mods de conteúdo carregam.
- **O que faz:** em jogo, `execute if block X Y Z #mekanism:factories`, `#c:storage_blocks/osmium` (do Mekanism, já existentes) e `#mekanismgenerators:structures/turbine` dão `Unknown block tag`, e só as do vanilla respondem. As tags do mod valem para as receitas e para o que o mod lê, mas não dá para conferi-las por comando.
- **Hoje:** nada; a conferência das tags de mod é `tests/generators_content.rs` e `tests/blocos_content.rs`, contra o `Holder.tags()` do original em jogo.
- **Risco do que existe hoje:** nenhum.

### H67 `transform` do modelo

- **Lado:** cliente (o desenho). **Tipo:** dado: o arquivo de modelo aceitar o `transform` do NeoForge (`translation`, `rotation` ou `left_rotation`, `scale`, `right_rotation` ou `post-rotation`, `origin`, ou `matrix`) na raiz do modelo e em cada filho de um `neoforge:composite`, e o cozimento juntá-lo à rotação do blockstate como o NeoForge faz (`NeoForgeModelProperties.TRANSFORM`, `UnbakedElementsHelper.composeRootTransformIntoModelState`).
- **O que faz:** o `wind_generator` leva `"transform": {"translation": [0.5, 0, 0.5]}` no filho `.obj` (o `.obj` está centrado na origem) e o `advanced_solar_generator` leva `[0, 1, 0]` no modelo inteiro (os elementos vão de -16 a 32 pixels, o limite do Horizon em `model.rs:396`, e o modelo sobe um bloco). Hoje o Horizon só lê `model`, `flip_v`, `shade_quads` e `mtl_override` do `neoforge:obj` e nada de `transform`. O NeoForge guarda a transformação por modelo (o mais perto da folha vence, `fillRootTransformProperty`), junta a do `composite` à de cada filho e aplica o resultado em volta do centro do bloco depois da rotação do blockstate (`pai.compose(raiz)`, com `applyOrigin(-0,5)` na raiz; a origem padrão do JSON é o canto oposto, `(1, 1, 1)`).
- **Hoje:** as duas injeções e o módulo da ficha "O `transform` do modelo", desde 2026-11-09. A opção D da decisão 31 (a injeção) deu o 1:1, e o `.obj` do Wind Generator é a cópia do original; as opções A e B saíram.
- **Hook proposto:** `CuboidModel` com o campo `transform: Option<Transformation>` (lido em `decode`, herdado como o `render_type`, o mais perto da folha vence), `ResolvedCuboidModel::root_transform()` e `bake_into` juntando-o ao `BlockModelTransform` (que hoje só guarda a rotação do blockstate): então as duas injeções e o módulo saem.
- **Risco do que existe hoje:** o local `textures` de `CuboidModel::decode` (o MODIFY_VARIABLE o acha pelo nome) e o campo público `textures`, o espaço `\u{1}neoforge_transform` que carrega a matriz (o `resolve_chain` o herda do pai e do `composite` como qualquer espaço, e a pré-carga de sprites o ignora porque o espaço de nomes `neoforge_transform` não existe), `bake_into(model, transform, skip_flat, output)`, `BlockModelTransform::matrix_rows`, `ModelQuad.{positions, facing, cull}` e `Direction::ALL`; o módulo `model-transform` exige que `math.rs` não tenha `use ...::*` de topo. Se o Horizon ler o `transform` sozinho, o vértice desloca duas vezes: tirar as injeções.

### H39 `block_animate_tick`

- **Lado:** cliente. **Tipo:** hook, `listen`, por posição sorteada do `ClientLevel.animateTick`.
- **O que faz:** para cada posição que o sorteio de 667 + 667 por tick escolhe em volta do jogador (16 e 32 blocos), chama o mod dono do bloco com o estado, a posição, o jogador e o
  motor de partículas (o gerador do sorteio e `add`), depois de o Horizon tratar os blocos do vanilla. É o `Block.animateTick(state, level, pos, random)` de um bloco de mod: o Mekanism solta
  a fumaça e o pó de redstone da máquina acesa e o Heat Generator a fumaça e a chama (`BlockTile.animateTick`).
- **Hoje:** `animate_tick` (cru, HEAD de `block_animate_tick`) mais o módulo `animate-registry`, pelas razões da ficha de `animate_tick`.
- **Exemplo:** `fn block_animate_tick(&mut self, state: BlockState, pos: [i32; 3], player: [i32; 3], particles: &mut Particles)`, com `Particles::random()` e `Particles::add(options, pos, velocity)`.
- **Risco do que existe hoje:** `block_animate_tick`, `AnimateContext::registry` e `ParticleEngine::{random, add}`.

### H55 `mob_layer` v2 e as partes do modelo do mob

- **Lado:** cliente. **Tipo:** hook, `listen`, mais um pedido de dado e dois de cancelamento.
- **O que faz:** (a) `native_mob_layer` roda também para o armor stand, para o jogador da tela do inventário (e a do smithing) e para cada mão da primeira pessoa, com a pose de cada parte do modelo do mob
  (`part(nome) -> Mat4`: `head`, `body`, `left_arm`, `right_arm`, `left_leg`, `right_leg`, já com o agachar, o nado, o voo e o passo), a escala do bebê (`age_scale`) e o que ele veste em cada slot, para o mod desenhar
  um modelo avulso na pose de uma parte, como o `ICustomArmor` do original (`baseModel.<parte>.translateAndRotate`). (b) `native_mob_part_visible(mob, parte, &mut visivel)`: o mod esconde uma parte do modelo
  (`HumanoidModel.head.visible`, `PartNames.BODY`: a parte e as filhas, o chapéu, a jaqueta, as mangas) enquanto veste algo, como o `overrideModelPartVisibility` do `registerRenderStateModifiers`. (c) a capa do jogador
  cancelável (`showCape = false`), e `native_render_hand` com um `draw` para o braço que o mod desenha no lugar do do vanilla (`RenderArmEvent`, hoje só esconde).
- **Hoje:** as seis injeções e os dois módulos da ficha "O equipamento vestido".
- **Exemplo:** `fn mob_layer(&self, mob: &NativeMobRender<'_>, who: Option<&NativePlayerRender<'_>>, draw: &mut BlockEntityDraw)` com `mob.part("body") -> Option<Mat4>` e `mob.worn(slot) -> Option<&str>`.
- **Risco do que existe hoje:** os nomes da ficha.

### H70 o sombreamento de vértice dos modelos de bloco (`EnhancedBlockModelLighter`)

- **Lado:** cliente. **Tipo:** hook (uma troca do cálculo, `replace`), ou uma opção do renderer.
- **O que faz:** o renderer do Horizon dá aos quads de um modelo de bloco o AO e a luz suave do `BlockModelLighter` do vanilla. O NeoForge troca esse cálculo pelo `EnhancedBlockModelLighter` (a opção `enhancedLighting`, ligada por padrão), e os mods feitos para ele (o Mekanism entre eles) saem com o sombreamento dele. O pedido é um hook `native_block_lighter` que decide, por bloco, qual cálculo vale (o do vanilla ou o do NeoForge), ou o cálculo do NeoForge como opção do renderer, ligada para os blocos de mod.
- **Hoje:** a injeção `enhanced_ao` e o módulo `enhanced-lighting` da ficha acima.
- **Risco do que existe hoje:** os nomes da ficha.

### H79 o tipo de receita de mesa de um mod e o resultado montado com a grade

- **Lado:** servidor e cliente. **Tipo:** dado (um alias de tipo de receita) e hook (`replace`).
- **O que faz:** um mod com receita de mesa de tipo próprio e formato igual ao do vanilla (`mekanism:mek_data`, a `crafting_shaped` cujo resultado copia dados das entradas) poder dizer ao Horizon "leia como `crafting_shaped`", e montar o resultado com a grade (o item de cada célula) no `find_with`, para a mesa do jogador, a do convidado e o crafter.
- **Hoje:** a injeção `mek_data_alias` da ficha acima, para o tipo, e `mek_data_energy_grid` e `mek_data_energy_use` (com o módulo `mek-data-energy`), para a energia das entradas.
- **Risco do que existe hoje:** os nomes da ficha.

## Correções pedidas ao renderer do Horizon

Estas não são injeções. São diferenças de pixel que vêm do próprio renderer do Horizon, então afetam o jogo inteiro, não só o
Mekanism. Por decisão do usuário, não são corrigidas por injeção: entram na issue do Horizon.

Os valores são em `guiScale` 2, medidos com `dev/referencia/comparar_tela.py`. Em escala 3 os números são outros, e aparecem
entre parênteses.

### H71 `block_shape_context`

- **Lado:** cliente e servidor. **Contrato:** `collect`. **Quando dispara:** a cada vez que o jogo pede a forma de seleção de um bloco para o contorno ou para o raio de mira (`outline_boxes`, `pick`).
- **Assinatura:** `(game, pos, state, player, boxes: &mut Vec<[f64; 6]>)`: o mod troca as caixas de um bloco que depende do contexto (o item na mão do jogador que mira). Hoje o Horizon só tem um caso interno (`context_outline`, uma caixa).
- **Elimina:** `context_outline` (mekanism), a seleção parcial do Configurator (`BlockTransmitter.getShape`); com a lista, o grupo PUSH/PULL de três caixas também cabe.

### H72 `item_use_first`

- **Lado:** servidor. **Contrato:** `listen` cancelável. **Quando dispara:** antes do uso do bloco (`useItemOn`/`useWithoutItem`), com o item da mão principal e a face clicada, como `Item.onItemUseFirst` do NeoForge, mas só para o item que pediu (um item pode dizer, por dados, que passa à frente da tela de qualquer bloco).
- **Por que:** o Configurator em `ROTATE` e o cartão de configuração agem antes de a tela da máquina abrir (`TileEntityMekanism.openGui` devolve `TRY_WITH_EMPTY_HAND` quando a mão tem um deles). Hoje o port usa as injeções cruas `use_hand` (servidor) e `predict_item_on_block` (cliente de um convidado de LAN), que devolvem o resultado do clique (ver a ficha delas); antes usava `right_click_block` e cancelava o clique, o que perdia o resultado do original (`SUCCESS` ou `PASS` para o item seguinte), a mão do convidado e exigia descartar o `use` duplicado do cliente. O hook pedido tira as duas: o item passa à frente do bloco e devolve o resultado, e o cliente prevê o `useOn` do item por dados.

### H73 `key_mappings`

- **Lado:** cliente. **Contrato:** `collect`. **Quando dispara:** ao montar a tela de Controles: o mod entrega `(mod_id, nome, tecla padrão, categoria, texto)` e a tela lista, deixa trocar e avisa conflito, como o `KeyMapping` do NeoForge. Hoje `native_mod_key` lê a tecla que o jogador escolheu, mas nada a deixa escolher na tela.

### H87 `PrimitiveShape::TexturedQuad`

- **Lado:** cliente. **Tipo:** dado (uma forma nova de `PrimitiveShape`, que já é `non_exhaustive`). **Quando dispara:** com o resto dos primitivos, em `native_world_primitives`.
- **Assinatura:** `TexturedQuad { corners: [[f64; 3]; 4], sprite: &str, uv: [[f32; 2]; 4], color: u32 }`: um quadrilátero em coordenadas do mundo com um sprite do atlas de telas (`mod:gui/sprites/...`), a cor multiplicada (o alfa vale), com o teste de profundidade do primitivo e sem escrever profundidade.
- **Elimina:** a injeção `world_overlay` e o módulo `world-overlay` (a sobreposição colorida da face do Configurator, `ConfiguratorOverlayHandler`); qualquer mod que queira marcar uma face de bloco usa.
- **Risco do que existe hoje:** `world_primitive_geometry`, `PrimitiveGeometry` e `Vertex`.

### H88 estado do HUD para um mod

- **Lado:** cliente. **Tipo:** helper (leitura). **O que dá:** `native_hud_state() -> HudState { left_height, right_height, can_hurt, overlay_message, gui_ticks, game_mode, main_hand, offhand, armor, hidden, text_background }`, o que `MekanismStatusOverlay` e `MekanismHUD` leem do `Minecraft.gui` e do jogador.
- **Elimina:** os campos privados que o `gui_layer_post` do Mekanism lê (a linha da tabela de estrutura privada).

### H89 o alvo do olhar e a guinada da câmera para um mod

- **Lado:** cliente. **Tipo:** helper (leitura). **O que dá:** `native_block_target() -> Option<(pos, face)>` com as mesmas portas do contorno (sem F1; pode construir ou aventura) e `native_camera_yaw()`.
- **Elimina:** `Game::pick`, `may_build` e `yaw` na sobreposição do Configurator (`RenderTickHandler.onBlockHover` olha o `HitResult` do contorno).

### H99 desenhista de item de mod por quadro

- **Lado:** cliente. **Tipo:** hook (o `SpecialModelRenderer` do 26.x).
- **O que faz:** o `RenderEnergyCubeItem` do original desenha o quadro do cubo com as portas e os LEDs do estado de cada lado (padrão ou guardado no item) e, com energia, o núcleo translúcido girando pelo `gameTime` com o alfa da razão de energia. O pedido é um callback de desenho de item (na barra, na mão e no chão) que receba a pilha com os componentes e o `gameTime` mais o `partial`, e escreva vértices como o `block_entity_submit`.
- **Hoje:** a injeção crua `energy_cube_item_core` (ver a ficha dela em "Injeções cruas em uso"): o item usa o modelo gerado (H60, decisão 26) com a porta de todo lado e o LED aceso onde sai, e o núcleo é uma camada a mais no item resolvido. Medido em jogo com `comparar_cubo.py`: a mão com energia difere em 1 pixel ou menos contra o original (era 128) e a barra em 49 (a barra de energia).
- **Risco do que existe hoje:** o da injeção: o nome e o módulo de `resolve_item_in_hand`, os campos de `LayerRender`.

### H100 `BlockEntityDraw::partial` preso com o `tick freeze` (retirado)

- **Retirado em 2026-11-12 (frente `cubo-item`).** O pedido dizia que o `draw.partial` varia entre dois prints sob `tick freeze`. Medido em jogo: com o servidor parado o `partial` entregue é 1,0 (`Clock::fraction` devolve 1,0 com o jogo congelado) e o `gameTime` fica parado em 211 chamadas de desenho seguidas, e três prints com o chat já apagado diferem em menos de 1 pixel. Os 96 pixels que a comparação mostrava eram o texto do chat que apaga. Sem pedido.

### H90 envio ao servidor dos ganchos de tela

- **Lado:** cliente. **Tipo:** helper. **O que dá:** `native_net_send_to_server` chamável dos ganchos de tela (`&self`), ou um contexto de envio no gancho. Hoje o menu radial põe o modo escolhido numa fila (`pending`) e o `client_tick` a esvazia no tique seguinte, o que atrasa o pacote até 50 ms.
- **Elimina:** a fila e a espera de um tique.

### H26 mistura de sprites animados na GPU, em float (correção, sem hook)

- **O que acontece:** um sprite animado com `interpolate` (os químicos do Infuser, `bio` e `fungi`) é misturado no original por
  `animate_sprite_interpolate.fsh`, na GPU: `mix(atual, próximo, id / 1000.0)` em `float`, gravado em 8 bits. O Horizon mistura na CPU, em
  inteiros (`(a*(1000-m) + b*m + 500) / 1000`, `HudRenderer::animate_sprites`, `hud.rs:2631`). Os dois diferem nos empates exatos e em cerca
  de 0,5 % das amostras perto do empate, que dependem da precisão da GPU (RX 9060 XT, headless): 48 combinações de `float32` e `float16`
  testadas, nenhuma fecha tudo.
- **Medido em 2026-11-06** (guiScale 2, 40 prints do ciclo por químico, sem tolerância e sem máscara, `cena_telas.py` e `comparar_lote.py`;
  cada número é quantos prints do original têm um estado idêntico no port):

  | Sprite | Horizon como é | Com a injeção `sprite_ties` (empate para baixo) | Pior print sem par |
  |---|---|---|---|
  | `carbon` (`base`) | 18 de 40 | 40 de 40 | 64 px, depois 0 |
  | `bio` | 10 de 40 | 16 de 40 | 208 px, depois 72 |
  | `fungi` | 6 de 40 | 21 de 40 | 184 px, depois 84 |
  | `lava_still` | 40 de 40 | 40 de 40 | 0 |

- **Por que é injeção:** o mod não tem como mudar a mistura, e o `HudRenderer` só guarda o `Rect` de cada animação. Por decisão do
  usuário (2026-11-06) a injeção entra mesmo sem fechar tudo (ver `sprite_ties`).
- **Pedido:** que o Horizon misture os quadros interpolados na GPU, em `float`, como o `animate_sprite_interpolate.fsh` do vanilla (ou
  imite a conta com o mesmo arredondamento), para qualquer pack e mod. Com isso a injeção e o módulo `sprite-ties` saem.

### H25 item desenhado em escala (correção, sem hook)

Medido em 2026-11-06 na janela de upgrades, guiScale 2 e 3, quatro máquinas. O original desenha os ícones de "Supported:" (escala 0,75) e os
das linhas da lista (0,5) com `guiGraphics.item` sob uma matriz escalada; o Horizon escala os quads do item depois de montá-los
(`native_item_stack`, `scale_since`). A textura é a mesma (md5 igual) e a posição também; muda a amostragem das bordas: 545 pixels em
2 (448 acima de 3 níveis, máximo 199) e 665 em 3 nos 8 ícones de "Supported:", e 65 pixels em 3 no ícone da linha da lista. O pedido ao Horizon é
um desenho de item em escala que amostre como o `GuiRenderer` do 26.x. Sem ele não há como zerar esses pixels pelo mod.

### H22 fundo escurecido da tela de contêiner (correção)

- **Original (provado no código):** `AbstractContainerScreen.isInGameUi()` é `true` (26.3, `AbstractContainerScreen.java:1225` do jar de fontes do
  `minecraft-patched`), então `Screen.extractBackground` chama só `extractTransparentBackground`: `fillGradient(0, 0, width, height, 0xC0101010,
  0xD0101010)`, um degradê de cima para baixo. Não há blur (`extractBlurredBackground` fica para tela que não é `isInGameUi`) nem
  `inworld_menu_background`. Em pixel, o original com tela é uma função exata do mesmo mundo sem tela (sem espalhamento: um blur o espalharia), e o
  preto do mundo sai 12 no topo e 13 embaixo (16 vezes a opacidade, 0,753 a 0,816).
- **Horizon:** um retângulo fixo `rgba(16, 16, 16, 160)` (`Game::draw_inventory` e as outras telas de contêiner): o preto sai 10 e o mundo passa na proporção 0,373
  (medido: `p = 0,376 w + 10,05`), contra 0,247 a 0,184 do original. Isso dava 72 pixels de diferença nos cantos transparentes do painel por estado (162 em
  escala 3), 3 níveis por canal nos cantos e 4, 4, 8 e 12 no mundo dos controles de luz 4, 8, 12 e 15.
- **Correção no mod:** as injeções `container_dim` e `container_dim_flag` (ficha acima) trocam esse retângulo pelo degradê do vanilla, só nas telas da `machinery_basis`. Antes dela, uma camada composta por cima
  do retângulo (opacidade 86 a 129, `(1 - 160/255)(1 - a/255) = 1 - vanilla`) chegou a 1 nível, mas com o degrau do degradê em outra linha (no preto o
  original passa de 12 para 13 na linha 244 do print e a camada na 509); foi descartada.
- **Medido contra o original** (`controle_luz_*`, mesma vista, opções iguais): o degrau do degradê no preto cai na linha 253 (original 244); o mundo dos
  controles de luz 4, 8, 12 e 15 dá 1 por canal; e, nos pixels em que o mundo sem tela é igual nos dois jogos (661.862 canais), o escurecimento dá
  0 de diferença em 94,5 % deles e 1 nível nos outros 5,5 %. O 1 nível que sobra é o arredondamento do caminho de cor do Horizon (a mistura do `fill_gradient`
  não cai no mesmo byte que a do original); não há o que o mod faça além do `fill_gradient` do próprio Horizon.
- **Pedido:** que o renderer desenhe, no fundo de **toda** tela de contêiner (o inventário, o baú, a fornalha, o Creative, o livro, a placa, o beacon e as dos mods: no
  original todas são `isInGameUi`), o degradê `0xC0101010` a `0xD0101010` com a mistura do original em vez do retângulo fixo `rgba(16, 16, 16, 160)` (ou que `NativeScreen`
  tenha um campo que o suprima). Com isso o `container_dim` sai. Hoje o baú e a fornalha do vanilla ficam com o retângulo fixo (10 no preto, igual de cima a baixo).
- **Causa do resto (hipótese, não provada):** o 1 nível que sobra em 5,5 % dos canais e o degrau na linha 253 contra a 244 vêm do caminho de cor do Horizon (o `fill_gradient`
  mistura em outro arredondamento). Não foi localizado no código.
- **Mesma causa:** o tooltip de barra dava 12 px, e o rótulo "Inventory" sob um tooltip dava 4 px.

### H30 iluminação do mundo, mais clara no original (diferença sem causa provada)

- **O que acontece:** com o mesmo mundo (uma caixa de `black_concrete` sem céu), a mesma vista (`tp x -60 0 0 0`, `look 0 0`) e o mesmo nível de luz, o original
  sai mais claro, **sem tela aberta**. Brilho médio por canal (0 a 255) do print inteiro menos a mira, o hotbar e a mão, original contra port
  (`controle_*_semtela`, guiScale 2): luz 0 (máquina parada) 2,25 contra 0,95 (maior diferença por pixel 17), luz 8 (fonte vanilla `minecraft:light`) 3,02 contra
  1,65 (32), luz 15 5,31 contra 3,75 (109) e a máquina acesa (luz 8) 3,63 contra 2,26 (87). Na face da frente da máquina, que é clara, a diferença é de
  2 % (luz 15: 27,26 contra 26,71); no chão escuro, de 1 a 1,5 nível: a diferença parece aditiva (um nível), não multiplicativa.
- **Entradas conferidas nos dois jogos, iguais:** o mundo (`fill ... black_concrete hollow`, a caixa fechada, `sky` 0 em toda a caixa), a vista, o
  `time set 6000` e `weather clear` (o roteiro do port os aplica como o `CenaTelas`; sem eles o port estava em 23576: o brilho não mudou, 0,97), a dimensão
  (`overworld`), o `ambientLight` do `dimension_type` (0,0) e o `minecraft:visual/ambient_light_color` (`#0a0a0a` no `overworld.json` do jar do cliente
  e no `data/minecraft/dimension_type/overworld.json` do Horizon), o `blockFactor` (1,4 mais o tremor, `LightmapRenderStateExtractor.java:56`, e
  `1.4 + flicker` em `Gfx::update_lightmap`), o gamma (0,5, conferido em execução) e o `darknessEffectScale` (1,0), sem visão noturna nem chefe. As opções
  gráficas (`cena_telas.OPCOES_DO_ORIGINAL`: preset `fancy`, `ao`, vinheta, filtragem de textura, distância, partículas) também são iguais.
- **A fórmula do mapa de luz também:** `lightmap.fsh` do vanilla (`get_brightness`, `notGamma`, `parabolicMixFactor`) e `Gfx::update_lightmap` do Horizon
  calculam o mesmo, a menos de o Horizon truncar o texel (`as u8`) onde a GPU arredonda.
- **Não é a luz do port:** o `horizon-ctl light` dá 8, 7, 6, 5 ao longo do eixo da máquina, como a propagação do Java, e os controles usam uma fonte vanilla
  (`minecraft:light` de nível 4, 8, 12 e 15) nos dois jogos, sem nenhum bloco do Mekanism emitindo, com a mesma diferença.
- **Causa não localizada.** Sobram o shader do terreno e o caminho de cor do Horizon até o byte final, que não li até o fim. A hipótese que cabe nos
  números (uma diferença de cerca de um nível no preto, mais alta no chão que na face clara) é de arredondamento ou truncamento na saída, mas é só
  hipótese. Por isso o comparador não trata a diferença como ok: o relatório do lote a marca `NE` (não explicada), com o quanto ela chega.
- **Efeito na comparação:** com tela, o fundo (H22) deixa passar só 0,2 do mundo, e o mundo atrás da tela dá 1 por canal nos controles de luz 4, 8, 12 e 15
  (e 4, 4, 8 e 12 antes da correção do H22). A tabela `LIMITE_MUNDO_POR_LUZ` do comparador guarda o que se mediu, para a diferença não crescer sem ninguém ver.
- **Pedido:** que a luz de bloco e de céu de uma caixa fechada saia do renderer com o mesmo brilho do vanilla, no byte, quando se achar onde ele perde o nível.

### H23 caixa do tooltip de item (correção, sem hook)

- **O que acontece:** o Horizon desenha a caixa antiga, com cantos quadrados. A diferença é de 42 px (123 em escala 3).
- **Original:** usa os sprites `tooltip/background` e `tooltip/frame`, em nove partes.
- **Pedido:** desenhar a caixa do tooltip de item com esses sprites, em nove partes, como o original.

### H24 ícone de item de bloco (correção, sem hook)

- **O que acontece:** o lado esquerdo sombreado do ícone difere em 224 px.
- **Efeito na medição:** a cena de comparação usa itens planos para desviar disso. Por isso, o "0 pixels" em slot com item
  vale só para item plano.
- **Pedido:** que o renderer desenhe o ícone de item de bloco com o sombreado do original.

### H28 modelo ausente no ícone de slot (correção, sem hook)

- **O que o original faz:** `ComponentItemModel.update` usa o `missingItemModel` quando o componente falta (`ComponentItemModel.java`, `update`),
  e o item aparece como o cubo magenta e preto.
- **O que o Horizon faz:** `ItemModelNode::Select` sem caso nem `fallback` empurra `LayerContent::Missing` (`item_model.rs:1209`), mas o
  ícone do slot e da barra de atalhos sai vazio, com ou sem um `Model` explícito de `minecraft:builtin/missing`. Visto em 2026-11-06 com
  `/give @s mekanism:upgrade` (sem `upgrade_type`).
- **Pedido:** que o renderer desenhe a camada `Missing` (o cubo de `missing_model()`) no ícone de item, como o do mundo.

## Como manter o documento

1. **Toda injeção crua nova entra aqui no mesmo change que a cria**, com a ficha completa: alvo exato e tipo, arquivo do helper, o
   Java com arquivo e linha, o hook mais próximo e o que falta nele, o hook proposto (ou o que uma proposta existente cobre) e o
   risco. Uma injeção que só chama um hook do catálogo com o contrato dele não entra.
2. **Um hook do catálogo usado com campo privado entra na tabela de estrutura privada.** O teste é perguntar se o helper
   acessa `game.x` ou `level.x` que não seja um método de helper do catálogo.
3. **Mudança de estado:** quando uma `prevista` vira spike, ou um spike vira `em uso`, atualize o estado na ficha e a tabela de
   contagem. Quando uma injeção sai (o Horizon ganhou o hook, ou deixamos de precisar), apague a ficha e a linha da tabela
   H, e diga o commit do Horizon no changelog.
4. **Ao subir o pin:** rode a contagem abaixo, confira cada alvo cru no novo commit (`grep -rn 'fn <nome>' crates/`), troque
   `horizon_revision` nos três `horizon-mixin.toml` e as linhas citadas aqui.
5. **Contagem.** A soma de hook com target tem de dar o total de `[[injection]]`:

```bash
cat mods/machinery_basis/horizon-mixin.toml mods/mekanism/native/horizon-mixin.toml \
    mods/mekanismgenerators/native/horizon-mixin.toml |
  awk '/^\[\[injection\]\]/{i++} /^hook = /{h++} /^target = /{t++} /^\[\[module\]\]/{m++}
       END{printf "injection=%d hook=%d target=%d module=%d soma_ok=%s\n", i, h, t, m, (i==h+t)?"sim":"NAO"}'
```

   Hoje: `injection=157 hook=91 target=66 module=33 soma_ok=sim`. Se `target` mudar, uma ficha nova ou removida tem de
   acompanhar. Se `hook` mudar, reclassifique entre limpo e com estrutura privada: 46 + 45 tem de somar `hook`.
6. **Quem lê:** o texto vira a issue no Horizon. Escreva cada ficha para um leitor que não conhece o Mekanism: o comportamento
   do original, e o que um mod qualquer ganha com o hook.

## O que não foi confirmado

- **`rules` como `[[crate]]`.** Presumo que são módulos porque `machinery_basis_core` é crate de outro mod e a documentação
  proíbe dependência de crate por caminho. Não testei um `[[crate]]` dependendo do de outro mod. Se for possível, os dois saem.
- **`view_hit` e `camera` no lugar de `game.pick()` e `game.aim()`.** Existem no catálogo, mas não comparei o raio com o do
  spike do cabo.
- **O hook `item_model_property`** consta no catálogo do pin, mas a documentação ainda o chama de "pending hook". Usei o que o
  catálogo diz.
- **Assinaturas e estruturas propostas** (`NativeScreenTooltip`, `NativeBlockFluid`, `NativeOutlineBox`, `SoundHandle`) são o que
  o port precisa, não o que o Horizon aceitaria: o formato final é do mantenedor.
- **O Java por linha** foi lido onde há número. `ObjGeometry.makeQuad` é do NeoForge e não está no repositório de referência. Não
  localizei no Java a classe de `TileComponentConfig` para a linha de faces por instância (só o nome, pelo plano).
- **A tabela de bloqueios** que a regra 5 do plano cita não existe hoje no `plano-paridade.md`. Usei a tabela da Fase 0, as
  Decisões e as fases 3 e 4.
- **O estado em jogo** das cruas em uso vem dos commits (`9d4d91e`, `ccf5035`, `83f070b`): não rodei o jogo agora.
