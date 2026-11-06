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

Contagem em 2026-11-06, sobre os `horizon-mixin.toml` (conferência na [próxima seção](#contagem-e-conferência)):

| Categoria | Quantas | Assunto deste documento |
|---|---|---|
| `[[injection]]` no total | 41 | |
| Hook do catálogo, usado só com o contrato dele | 26 | não |
| Hook do catálogo, usado com estrutura privada do jogo | 8 | sim |
| Injeção crua (`target = ...`) | 7 | sim |
| `[[module]]` (fora da contagem de `[[injection]]`) | 6 | sim |

26 + 8 + 7 = 41. Os 6 módulos são outra coisa: código nosso compilado dentro do `horizon-client`. O assunto do documento são, então,
15 injeções e 6 módulos em uso, mais o que já foi provado em spike e o que as fases futuras vão pedir:

| Estado | Cruas | Módulos | Hook com estrutura privada |
|---|---|---|---|
| Em uso | 7 | 6 | 8 |
| Provada em spike | 8 | 1 | 8 |
| Prevista | 5 | 1 | 0 |

As provadas em spike não contam `native_energy_at`, que já está em uso. As previstas vêm do plano e não foram compiladas.

### Hook proposto contra injeções que ele elimina

Todos os nomes são propostas: nenhum existe no pin. "v2" é uma nova versão de um hook do catálogo, porque o contrato antigo nunca
muda. Cada proposta tem sua ficha em [Hooks e pedidos propostos](#hooks-e-pedidos-propostos).

| Proposta | Tipo | Elimina | Estado |
|---|---|---|---|
| H1 `block_entity_energy_instance` | hook | `energy_capacity` | em uso |
| H2 `block_entity_container_instance` | hook | `Containers::view`, `can_take`, `native_container_accepts`, `native_fluid_at` | prevista (4) |
| H3 `block_fluid` | hook | `logged-fluid-row` | em uso |
| H4 `screen_tooltip` v2 | hook | `widget_tooltip` (mekanism e mekanismgenerators) | em uso (2) |
| H5 `component_types` v2 | hook | `command_components` | em uso |
| H6 `item_model_node` | hook | `component_item_model` | em uso |
| H7 `item_container_accepts` | helper | módulo `slot-accepts` | em uso |
| H8 helpers de leitura e escrita | helpers | 8 hooks com estrutura privada, os módulos `glue` | em uso (8) |
| H9 `screen_layout` | hook | módulo `spike` em `native_hooks.rs`, escrita em `mod_screen` | spike |
| H10 `screen_slot_at` | hook | `slot_at` (RETURN) | spike |
| H11 `screen_draw_over_slots` | hook | `draw_inventory_slots` (HEAD, cancelável) | spike |
| H12 `menu_hide_slots` | helper | uso de `lan.hub` em `net_from_client` | spike |
| H13 `block_entity_save` | hook | `save_level` | spike |
| H14 `block_entity_removed` | hook | `remove_entity` | spike |
| H15 `visuals` v2 | hook | `native_frame_visuals` | spike |
| H16 leitura do `Kd` do `.mtl` | correção | `obj_quad` (RETURN) | spike |
| H17 ordenação da camada translúcida | correção | `outline_fill` | spike |
| H18 `properties.light` condicional | dado | `override_gameplay` | spike |
| H19 `looping_sound` | helpers | módulo de som | prevista |
| H20 `menu_slot_rules` v2 | hook | alvo ainda sem nome | prevista |
| H21 `screen_slot_highlight` | hook | `slot_highlight` (mekanism) | em uso (1) |

H1 a H6, H13 a H15 e H21 são genéricos: qualquer mod de máquina, de rede, de fluido ou de tela com slot virtual usa. Nada neles cita o Mekanism.

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
| `mods/machinery_basis/horizon-mixin.toml` | 10 | 8 | 2 | 2 |
| `mods/mekanism/native/horizon-mixin.toml` | 22 | 18 | 4 | 2 |
| `mods/mekanismgenerators/native/horizon-mixin.toml` | 9 | 8 | 1 | 2 |
| Total | 41 | 34 | 7 | 6 |

Todas são `kind = "HEAD"`. Seis das sete cruas são `cancellable = true`; a que não é, `logged-fluid-row`. Nenhuma usa `trait_method`: os três crates `[api]` têm só
`impl Hooks for Mod {}`. Os 24 nomes distintos de `hook` existem no catálogo do pin, na versão 1.

Dos 34 hooks, 8 têm estrutura privada (lista em [Hooks do catálogo com estrutura privada](#hooks-do-catálogo-com-estrutura-privada)),
e 26 não: 4 em `machinery_basis`, 15 em `mekanism`, 7 em `mekanismgenerators`.

## Injeções cruas em uso (7)

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
- **Risco:** a função é privada, dentro de um `impl` com `#[allow(dead_code)]` (o comentário diz "Native mods use these; nothing
  in Horizon does"), e `native_energy_transfer`, `native_energy` e `native_energy_insert` dependem dela. Renomear, mudar a
  assinatura ou deixar de passar por ela quebra o build ou, pior, faz a capacidade voltar à do bloco sem aviso.

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
- **Por que é necessária:** cobre três widgets: a aba de avisos, a barra de energia e a barra de químico. `GuiWarningTab.updateTooltip`
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

### `command_components` (mekanism)

- **Estado:** em uso. **Alvo:** `horizon_command::item::ItemState<'_,'_>::component_type`, HEAD, cancelável (`item.rs:160`).
- **Helper:** `mods/mekanism/native/command_components.rs`.
- **Por que é necessária:** o componente `mekanism:upgrade_type` (`MekanismDataComponents.java:226`) é persistente e vale em
  `/give mekanism:upgrade[mekanism:upgrade_type=...]`. `component_type` só aceita o que está em `reg.persistent_components`, a lista
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

## Hooks do catálogo com estrutura privada

Os 8 abaixo usam hooks do catálogo, mas o helper alcança o que não é API. Nenhum é injeção crua, e todos ficam de pé enquanto o
Horizon não renomear o campo. O pedido ao Horizon é de helpers de leitura e escrita (H8), não de hooks novos.

| Injeção (mod) | Hook | O que toca de privado | O que o resolve |
|---|---|---|---|
| `update_shape` (basis) | `update_shape` | `level.registry.state(state).properties` (lê `fluid_logged`) | H8: `state_property(state, key)` no `GameLevel` |
| `state_for_placement` (basis) | `state_for_placement` | `level.world.block(x, y, z)` (o vizinho) | H8: `block_state_at(dimension, pos)` só leitura, no `GameLevel` |
| `use_item_on` (basis) | `use_item_on` | `game.world.game_time`, `game.with_level(...)` para agendar o tick do fluido | `level_game_time` já existe; H8: `schedule_fluid_tick` no `Game` |
| `use_item` (basis) | `use_item` | `game.world.game_time` | `level_game_time`, que já existe |
| `block_entity_tick` (mekanism) | `block_entity_tick` | `game.world.game_time`, e o método do módulo `slot-accepts` no ejetor | `level_game_time` e H7 |
| `menu_quick_move` (mekanism) | `menu_quick_move` | `game.recipes().cooking` (via `machine_recipes`) | H8: `cooking_recipes(kind)` |
| `use_item_on` (mekanism) | `use_item_on` | `game.world.block_entities.get/get_mut`, `game.world.modified.insert`, `game.held_mut()`, `game.recipes()` | H8: `mod_block_entity`, `change_mod_block_entity`, `consume_held` |
| `block_entity_tick` (generators) | `block_entity_tick` | `game.res.providers().burn_time(world_items!)`, `game.world.chunks` e `world_biomes!` | H8: `burn_time(stack)`, `biome_temperature(dimension, pos)` |

Java de cada um: `IStateFluidLoggable.updateFluids` e `BlockStateHelper.getStateForPlacement` (`BlockStateHelper.java:102-115`) para
os dois primeiros; `ItemUpgrade.useOn` (`ItemUpgrade.java:28-45`) para o `use_item_on` do Mekanism; `MekanismRecipeType.java:334` e
`TileEntityEnergizedSmelter.java:26` para a receita de cozimento que vem do `RecipeType.SMELTING` do jogo;
`TileEntityHeatGenerator.java:111`, `MekanismUtils.getBurnTime` (`MekanismUtils.java:469-478`) e `HeatAPI.getAmbientTemp`
(`HeatAPI.java:42-46`, que chama `Biome.getTemperature(pos, seaLevel)`) para o gerador.

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
| `native_container_slots` | API 48 | ejetor, hoppers |
| `native_state_block`, `native_state_with_property` | API 164, 76 | `has_analog_output`, `update_shape`, `state_for_placement` |
| `native_schedule_fluid_tick` | API 32 | `update_shape`, `use_item_on` |

Helpers em fragmento ainda pendente de carimbo (`tools/native-hook-fragments/`), que o README do próprio Horizon manda não
usar antes de ter número de API:

| Helper | Fragmento | Quem usa |
|---|---|---|
| `native_sneaking`, `native_component` | `host_bridge.json` | `use_item_on` do Mekanism |
| `native_draw_model` | `host_bridge.json` | spike do cabo, Fase 4 |
| `native_text_width` | `client_services.json` | `machine_panel`, `menu_tooltip` |
| hook `container_accepts` | `container_accepts.json` | previsto (Fase 2); hoje `hook =` dá `E_HOOK_UNKNOWN` |

Pedido: carimbar esses fragmentos (`python3 tools/stamp_native_hooks.py`) e listar os quatro helpers documentados em `helpers`.

## Módulos (6)

Um `[[module]]` enxerta um arquivo nosso em um arquivo do Horizon e, por ser filho dele, enxerga o que é privado ali.

| Módulo | Arquivo alvo | Para quê | Precisa de |
|---|---|---|---|
| `glue` (machinery_basis) | `game.rs` | liga a lib ao jogo: 1.077 linhas, tudo `pub(in crate::game)` | H8 e o que está em [Dependências a vigiar](#dependências-a-vigiar) |
| `glue` (mekanism) | `game.rs` | liga as máquinas e a tela: 1.596 linhas | idem |
| `glue` (mekanismgenerators) | `game.rs` | liga o Heat Generator: 280 linhas | idem |
| `slot-accepts` (machinery_basis) | `game/machines/native_items.rs` | `isValid` por slot, privado de `machines` | H7 |
| `rules` (mekanism) | `game.rs` | regras puras, sem tipo do jogo | nada do Horizon |
| `rules` (mekanismgenerators) | `game.rs` | regras puras, sem tipo do jogo | nada do Horizon |

- **`slot-accepts`:** o ejetor (`TileComponentEjector.outputItems`, `TileComponentEjector.java:255-279`) pergunta à
  `ResourceHandler` do vizinho o `isValid` de cada slot antes de transacionar, e o Horizon não simula inserção:
  `native_insert_item` não tem `simulate`. O método `machinery_slot_accepts` chama `containers.can_place` e
  `mod_container_accepts`, privados de `machines`. Hook mais próximo: `container_accepts`, que é uma resposta do mod ao Horizon,
  não uma consulta (e é fragmento pendente). Risco: renomear `View`, `containers()` ou `can_place`.
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
| `spike_override_gameplay` | `Plan::override_gameplay` (`mod_content.rs:1319`), HEAD | luz 8 só no Crusher, 13 nos demais | `BlockStateHelper.java:83-94`, `Attributes.java:147-157` | H18 |
| `spike_frame_visuals` | `native_visuals::native_frame_visuals`, HEAD | linhas de várias caixas, só no nosso bloco com a ferramenta | `BlockTransmitter.java:103`, `TileEntityTransmitter.java:234`, `MultipartUtils.java:39` | H15 |
| `spike_obj_kd` | `horizon_assets::bake::obj_quad`, RETURN | cor do quad vezes o `Kd` do `.mtl` (0,8): sem isso tudo sai 20% mais claro | NeoForge `ObjGeometry.makeQuad` | H16 |
| `spike_sort_translucent` | `pvp::Game::outline_fill`, HEAD | ordena os quads de `TRANSLUCENT_ITEM_OVERLAY` do fundo para a frente: manchas sumiram | `MekanismRenderType.java:20,61,69` (`sortOnUpload`) | H17 |

Hooks do catálogo que o spike usou e que são limpos: `standalone_models`, `block_entity_update_tag`. Os que tocam estrutura privada:
`block_highlight` (`game.held()`, `game.world`, `game.res.facts()`, `game.pack.assets.registry`) e `block_entity_submit`, na versão
do spike, com os mesmos campos. Substitutos que já existem: `held`, `block`, `block_property`, e `view_hit` e `camera` no lugar de
`game.pick()` e `game.aim()`. Se `view_hit` dá o mesmo raio que `pick()` não foi conferido.

Riscos: `spike_energy_at`, `spike_save_level` e `spike_remove_entity` quebram em renomeação. `spike_override_gameplay` depende de
`NewBlock` e do formato de linha. `spike_obj_kd` e `spike_sort_translucent` são os mais frágeis: alteram a pipeline de render, e
`spike_sort_translucent` reordena todos os vértices de `outline_fill`, o que muda com qualquer ajuste de camada.

### Aba de upgrades

Diff `~/.cache/rust-mekanism/spike-upgrade/spike-upgrade-A-fim.diff` e a página `gotchas/spike-aba-de-upgrades-viabilidade.md` do
ai-memory. Medido em jogo, no desktop, com dois clientes. Java: `GuiWindow.mouseClicked` (`GuiWindow.java:101`), `onDrag` (`:128`),
`InteractionStrategy` (`:244`), `GuiUpgradeWindow` (`GuiUpgradeWindow.java:34`), `VirtualInventoryContainerSlot`
(`VirtualInventoryContainerSlot.java:11`).

| Injeção | Alvo | Estado | O que provou | Proposta |
|---|---|---|---|---|
| módulo `spike` | `game/native_hooks.rs` | spike | `OpenNativeScreen` tem campos privados de `native_hooks`; o módulo reescreve `mod_screen.screen.slots[i]` (Some, None, posição) no hook `frame` | H9 |
| `spike_draw_slots` | `Game::draw_inventory_slots` (`game.rs:9461`), HEAD cancelável | spike | a ordem do original: itens, janela, itens virtuais, tooltip e item no cursor por cima. `native_screen_draw` fica sob os itens e `screen_render_post` cobre o tooltip | H11 |
| `spike_slot_at` | `Game::slot_at` (`game.rs:9349`), RETURN | spike | devolve o slot virtual sob o cursor e `None` para o slot coberto. Vale para hover, tooltip, clique, teclas 1 a 9, F e Q, e divisão por arrasto | H10 |

Hooks do catálogo que o spike usou com estrutura privada (7): `frame`, `screen_key_pressed_pre`,
`screen_mouse_button_pressed_pre`, `screen_mouse_button_released_pre`, `screen_mouse_dragged_pre`, `screen_render_post` (todos
leem `self.mod_screen`, `self.renderer`, `self.container` ou `self.world.block_entities`) e `net_from_client` (chama
`self.lan.hub.hide_menu_slots(conn, ...)` e zera `play.crafting.machine`). Proposta: H9 a H12.

Riscos: `slot_at` e `draw_inventory_slots` são o núcleo do desenho de inventário e mudam sempre que o Horizon mexe na tela. O
módulo em `native_hooks.rs` depende de um arquivo de 11 mil linhas. `hide_menu_slots` e `play.crafting.machine` são o
protocolo de menu de LAN: mudar quem fala com o convidado quebra a seleção por jogador, sem erro.

Falta decidir no Java: `GuiWindow.mouseClicked` com `InteractionStrategy.ALL` não consome o clique no fundo, e o spike cancela
sempre. A limpeza da seleção se o cliente cair pode usar `player_container_close`, que existe (API 152).

## Previstas

Do plano, sem compilar. O Java está nas fases correspondentes.

| Prevista | Fase | Alvo (do plano) | Proposta |
|---|---|---|---|
| Faces e contêiner por instância | 2, 3 | HEAD cancelável em `native_fluid_at`, `Containers::view`, `can_take` e `native_container_accepts` | H2 |
| Som contínuo da máquina | 5 | `[[module]]` em `crate::game` com `SoundInstance`, `Ticking::Native` e `stop_and_release`; `Ticking::Native` só aparece em teste | H19 |
| Regra de slot que vê o outro slot | decisão 3 aberta | ainda sem alvo. `menu_slot_rules` não tem `Game` e não vê outro slot | H20 |
| Seleção do Configurator com `context_outline` (não conta: o spike já cobriu) | decisão 1 | o spike resolveu por `block_highlight` mais `native_frame_visuals`: `context_outline` devolve uma caixa só | H15 |
| `native_screen_slot` (não conta: o spike já cobriu) | 1 | HEAD, "não testado": o spike da aba cobriu o caso por H9 | H9 |

Java: `TileEntityMekanism.java:253,327,1436` e `client/sound/SoundHandler.java` para o som; `TileEntityCombiner.java:114-117`
(`containsRecipeAB`, `containsRecipeBA`) para a regra de slot.

Sem injeção conhecida ainda: a energia do Mekanism vive em `i64` no NBT próprio porque o `ModEnergyBuffer` é `i32` (decisão 2 do
plano), e o ejetor não alcança a composteira, que `native_container_slots` não expõe.

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
  outra em `mekanismgenerators_native`, ver [Injeções cruas em uso](#injeções-cruas-em-uso-7)). O tooltip da aba de avisos, da
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

### H16 leitura do `Kd` do `.mtl` (correção, sem hook)

O NeoForge multiplica a cor do quad pelo `Kd` do material (`ObjGeometry.makeQuad`), e `parse_mtl` do Horizon lê só `map_Kd`. Os
materiais do cabo têm `Kd 0.8`: sem isso, tudo sai 20% mais claro. O pedido é que o Horizon leia `Kd` (e `neoforge_TintIndex`) ao
assar um `.obj`. Não é hook porque o dado já está no arquivo: qualquer mod com `.obj` ganha.

### H17 ordenação da camada translúcida (correção, sem hook)

O original usa `sortOnUpload` (`MekanismRenderType.java:20,61,69`) e ordena os quads translúcidos do fundo para a frente. O Horizon
não ordena e escreve profundidade, então um conteúdo desenhado antes apaga o de trás. O pedido é que o Horizon ordene a camada de
quads translúcidos das block entities por distância (`VertexSorting.DISTANCE_TO_ORIGIN`), o que beneficia qualquer mod.

### H18 `properties.light` condicional (dado, sem hook)

O Horizon já aceita condição por estado em `random_ticks` (`ticks.applies(combo)`), mas `properties.light` é um inteiro por bloco
(`mod_content.rs:1345`). O Java dá luz por estado (`BlockStateHelper.applyLightLevelAdjustments`, `BlockStateHelper.java:83-94`).
O pedido é aceitar em `light` a mesma condição de `random_ticks`.

### H19 `looping_sound`

- **Lado:** cliente. **Tipo:** helpers do `Host`.
- **O que faz:** toca um som em laço preso a um ponto e o para, e o som cessa sozinho se o chunk descarrega. Hoje só há
  `native_play_local_sound` (uma vez, com atraso).
- **Assinatura:**

```rust
fn looping_sound(&mut self, sound: &str, at: [f64; 3], volume: f32, pitch: f32) -> Option<SoundHandle>
fn stop_sound(&mut self, handle: SoundHandle)
```

- **Exemplo:** a máquina ativa chama `looping_sound` ao ligar e `stop_sound` ao parar (`TileEntityMekanism` e `SoundHandler`).

### H20 `menu_slot_rules` v2

- **Lado:** servidor. **Contrato:** `listen`, com o menu inteiro em vista.
- **Quando dispara:** no mesmo lugar do v1, mas com uma visão dos outros slots do menu e do `Game` em leitura.
- **Assinatura:** `native_menu_slot_rules_v2(menu, slot, stack, may_place, max_stack, slots: &[Option<ItemStack>])`.
- **Exemplo:** o Combiner aceita no segundo slot só o item que forma receita com o que está no primeiro
  (`TileEntityCombiner.java:114-117`).

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

   Hoje: `injection=41 hook=34 target=7 module=6 soma_ok=sim`. Se `target` mudar, uma ficha nova ou removida tem de
   acompanhar. Se `hook` mudar, reclassifique entre limpo e com estrutura privada: 26 + 8 tem de somar `hook`.
6. **Quem lê:** o texto vira a issue no Horizon. Escreva cada ficha para um leitor que não conhece o Mekanism: o comportamento
   do original, e o que um mod qualquer ganha com o hook.

## O que não foi confirmado

- **`rules` como `[[crate]]`.** Presumo que são módulos porque `machinery_basis_core` é crate de outro mod e a documentação
  proíbe dependência de crate por caminho. Não testei um `[[crate]]` dependendo do de outro mod. Se for possível, os dois saem.
- **`view_hit` e `camera` no lugar de `game.pick()` e `game.aim()`.** Existem no catálogo, mas não comparei o raio com o do
  spike do cabo.
- **`properties.light` condicional.** Li `override_gameplay` e vi um inteiro, mas não rodei um bloco com `light` por estado no pin.
  O plano registra injeção em `override_gameplay`: não achei no plano por que o campo comum não bastava, só que "em
  `set_state_properties` não".
- **O hook `item_model_property`** consta no catálogo do pin, mas a documentação ainda o chama de "pending hook". Usei o que o
  catálogo diz.
- **Assinaturas e estruturas propostas** (`NativeScreenTooltip`, `NativeBlockFluid`, `NativeOutlineBox`, `SoundHandle`) são o que
  o port precisa, não o que o Horizon aceitaria: o formato final é do mantenedor.
- **O Java por linha** foi lido onde há número. `ObjGeometry.makeQuad` é do NeoForge e não está no repositório de referência. Não
  localizei no Java a classe de `TileComponentConfig` para a linha de faces por instância (só o nome, pelo plano).
- **A tabela de bloqueios** que a regra 5 do plano cita não existe hoje no `plano-paridade.md`. Usei a tabela da Fase 0, as
  Decisões e as fases 3 e 4.
- **O estado em jogo** das cruas em uso vem dos commits (`9d4d91e`, `ccf5035`, `83f070b`): não rodei o jogo agora.
