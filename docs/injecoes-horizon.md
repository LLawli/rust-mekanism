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

Contagem em 2026-11-08, sobre os `horizon-mixin.toml` (conferência na [próxima seção](#contagem-e-conferência)):

| Categoria | Quantas | Assunto deste documento |
|---|---|---|
| `[[injection]]` no total | 86 | |
| Hook do catálogo, usado só com o contrato dele | 28 | não |
| Hook do catálogo, usado com estrutura privada do jogo | 30 | sim |
| Injeção crua (`target = ...`) | 28 | sim |
| `[[module]]` (fora da contagem de `[[injection]]`) | 17 | sim |

28 + 30 + 28 = 86. Os 17 módulos são outra coisa: código nosso compilado dentro do `horizon-client`. O assunto do documento são, então,
58 injeções e 17 módulos em uso, mais o que já foi provado em spike e o que as fases futuras vão pedir:

| Estado | Cruas | Módulos | Hook com estrutura privada |
|---|---|---|---|
| Em uso | 28 | 17 | 30 |
| Provada em spike | 5 | 0 | 1 |
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
| H5 `component_types` v2 | hook | `command_components` | em uso |
| H6 `item_model_node` | hook | `component_item_model` | em uso |
| H7 `item_container_accepts` | helper | módulo `slot-accepts` | em uso |
| H8 helpers de leitura e escrita | helpers | 7 hooks com estrutura privada, os módulos `glue` | em uso (7) |
| H9 `screen_layout` | hook | módulo `windows` em `native_hooks.rs`, escrita em `mod_screen` | em uso (módulo) |
| H10 `screen_slot_at` | hook | `slot_at` (RETURN) | em uso |
| H11 `screen_draw_over_slots` | hook | `draw_inventory_slots` (HEAD, cancelável) | em uso |
| H12 `menu_hide_slots` | helper | uso de `lan.hub` em `net_from_client` | em uso |
| H13 `block_entity_save` | hook | `save_level` | spike |
| H14 `block_entity_removed` | hook | `remove_entity` | spike |
| H15 `visuals` v2 | hook | `native_frame_visuals` | spike |
| H16 leitura do `Kd` do `.mtl` | correção | `obj_quad` (RETURN) | spike |
| H17 ordenação da camada translúcida | correção | `outline_fill` | spike |
| H18 `properties.light` condicional | dado | `active_light` (mekanism) | em uso |
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
| H38 `ingredient` v2 | hook | `components-ingredient-stash` (cru, HEAD de `Ingredient::parse_object`), o hook `ingredient` v1 `components-ingredient-items` e o módulo `components-ingredient` | em uso (1 cru, 1 hook, 1 módulo) |

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
| `mods/machinery_basis/horizon-mixin.toml` | 39 | 20 | 19 | 13 |
| `mods/mekanism/native/horizon-mixin.toml` | 29 | 22 | 7 | 2 |
| `mods/mekanismgenerators/native/horizon-mixin.toml` | 18 | 16 | 2 | 2 |
| Total | 86 | 58 | 28 | 17 |

Todas são `kind = "HEAD"`, menos `window_slot_at`, `bar_slot_position`, `bar_slots` e `bar_slot_count` (`RETURN`). Onze das 28 cruas são `cancellable = true` (a oitava é `container_dim`; as três últimas, `bar_menu_slot`, `bar_clicked_outside` e `bar_quick_move`); as que não são: `energy_store`, `container_dim_flag`, `logged-fluid-row`, `window_slot_at`, `sprite_ties`, `flush_signals`, `container_changed`, `result_take_before`, `menu_click_before`, `active_light`, `bar_slot_position`, `bar_slots`, `bar_slot_count`, `bar_machine_menu`, `bar_answer_click` e `bar_resend_menu`. Nenhuma usa `trait_method`: os crates `[api]` do Mekanism e do Generators têm só
`impl Hooks for Mod {}`, e o da `machinery_basis` implementa um hook do catálogo, `menu_slot_rules` v1 (ver [A regra de slot do clique](#a-regra-de-slot-do-clique)), que não passa pelo `horizon-mixin.toml` e por isso não entra na contagem. Os 42 nomes distintos de `hook` (eram 41 antes do `neighbor_changed`, 40 antes do `server_tick`; a contagem anterior, 36, estava velha) existem no catálogo do pin, na versão 1.

Dos 57 hooks, 30 têm estrutura privada (lista em [Hooks do catálogo com estrutura privada](#hooks-do-catálogo-com-estrutura-privada)),
e 27 não: 4 em `machinery_basis`, 13 em `mekanism`, 10 em `mekanismgenerators` (um deles, `heat_tab_press`, é `screen_mouse_button_pressed_pre`, o mesmo hook de `window_press`, usado só com o contrato dele: a aba de calor toma o clique sem o som e o pacote que o `screen_click` mandaria). Os 30 com estrutura privada são os 7 de antes, os 9 da janela de upgrades (`window_*`; o `window_viewers` saiu em 2026-11-08, veja `container_viewers`), os 2 do som e da experiência (`client_tick` e `destroy_block`), os 3 do nome (`place_block`, `loot_drops` e `container_opened`), o `menu_quick_move` do Heat Generator, 1 da aba de xp (`menu_button`), 1 da experiência de uma explosão (`explosion_detonate`) os 4 do nome do Heat Generator (`place_block`, `loot_drops`, `container_opened` e `destroy_block` de `mekanismgenerators`) o `server_tick` da `machinery_basis` (o `broadcastChanges` no começo do tick do servidor, 2026-11-08; era do `mekanism`) e o `neighbor_changed` da `machinery_basis` (2026-11-08).

## Injeções cruas em uso (27)

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

### `active_light` (mekanism)

- **Estado:** em uso desde 2026-11-07. **Alvo:** `horizon_client_lib::mod_content::Plan<'_>::override_gameplay`, HEAD
  (`mod_content.rs:1319`, método privado do plano de conteúdo, que recebe o `NewBlock` e a linha `J` da tabela de jogo de cada estado).
- **Helper:** `mods/mekanism/native/active_light.rs`. Só age no Crusher, na Enrichment Chamber, no Energized Smelter e no Metallurgic
  Infuser, e só nos estados em que a linha já emite luz (o `lit=true` do gabarito `minecraft:furnace`, 13): troca por 8.
- **Por que é necessária:** as quatro são `Machine` (`Machine.java:26`), com `Attributes.ACTIVE_LIGHT`, `AttributeStateActive(8)`
  (`Attributes.java:27`): luz 8 quando ativas e 0 quando não (`AttributeStateActive.java:49-51`, que soma ao 0 do bloco pela
  `BlockStateHelper.applyLightLevelAdjustments`, `BlockStateHelper.java:83-94`). O gabarito dá 13 ao estado aceso, e
  `properties.light` é um inteiro só, aplicado a todos os estados (`mod_content.rs:1345`): com `light: 8`, a máquina desligada
  também ficaria acesa. O Heat Generator usa `ACTIVE_MELT_LIGHT`, 13 (`GeneratorsBlockTypes.java:68`), o mesmo do gabarito, e não
  passa por aqui.
- **Hook mais próximo:** nenhum. `random_ticks` já aceita condição por estado, `light` não.
- **Hook proposto:** H18 (dado, sem hook).
- **Risco:** depende de `NewBlock` (`decl.id`) e do formato de linha (`J`, chave `emission`), privados de `mod_content.rs`. Renomear
  o método ou a chave quebra o build, ou deixa a máquina com a luz do gabarito, sem erro.
- **Conferido em jogo** (2026-11-07, `horizon`, `horizon-ctl light` no bloco e no vizinho): as quatro máquinas acesas dão 8 e 7, a
  Enrichment Chamber apagada 0 e 0, o Heat Generator aceso 13 e 12, e o `minecraft:furnace` aceso 13 e 12.

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

Os 31 abaixo usam hooks do catálogo, mas o helper alcança o que não é API. Nenhum é injeção crua, e todos ficam de pé enquanto o
Horizon não renomear o campo. O pedido ao Horizon é de helpers de leitura e escrita (H8), não de hooks novos.

| Injeção (mod) | Hook | O que toca de privado | O que o resolve |
|---|---|---|---|
| `update_shape` (basis) | `update_shape` | `level.registry.state(state).properties` (lê `fluid_logged`) | H8: `state_property(state, key)` no `GameLevel` |
| `state_for_placement` (basis) | `state_for_placement` | `level.world.block(x, y, z)` (o vizinho) | H8: `block_state_at(dimension, pos)` só leitura, no `GameLevel` |
| `use_item_on` (basis) | `use_item_on` | `game.world.game_time`, `game.with_level(...)` para agendar o tick do fluido, e, para o `ItemUpgrade.useOn` (que o Mekanism e o Heat Generator usam por aqui), `game.held_mut()` e, nas funções de cada mod, `game.world.block_entities.get/get_mut` e `game.world.modified.insert` | `level_game_time` já existe; H8: `schedule_fluid_tick` no `Game`, `mod_block_entity`, `change_mod_block_entity`, `consume_held` |
| `use_item` (basis) | `use_item` | `game.world.game_time` | `level_game_time`, que já existe |
| `block_entity_tick` (mekanism) | `block_entity_tick` | `game.world.game_time`, o método do módulo `slot-accepts` no ejetor, e, para premiar as receitas ao jogador que tirou o resultado (o tick não tem jogador agindo), `game.lan.take()` e `game.as_guest` com o estado dele no lugar do anfitrião | `level_game_time`, H7 e H27 entregando o jogador |
| `menu_quick_move` (mekanismgenerators) | `menu_quick_move` | `game.inventory` (a barra de atalhos e o inventário, que o shift-clique do Heat Generator enche), `game.armor` e `game.offhand` (a barra de armadura que ele também enche e esvazia), e `game.items()` (em que slot cada peça se veste, `canEquip`) | o mesmo helper de escrita no inventário de quem age, e um de leitura de `equip_slot` |
| `menu_quick_move` (mekanism) | `menu_quick_move` | `game.recipes().cooking` (via `machine_recipes`), `game.inventory` (a barra de atalhos e o inventário, que o shift-clique do Mekanism enche) | H8: `cooking_recipes(kind)`; falta um helper de escrita no inventário de quem age |
| `block_entity_tick` (generators) | `block_entity_tick` | `game.res.providers().burn_time(world_items!)`, `game.world.chunks` e `world_biomes!`; a temperatura do ar (módulo `glue` da `machinery_basis`, `ambient_temperature`) lê também `game.streaming.biome_zoom` (o `BiomeManager.getBiome`) e `game.world_flat` (o nível do mar de um mundo plano, -63) | H8: `burn_time(stack)`, `biome_temperature(dimension, pos)`, que devolva a temperatura de `Biome.getTemperature(pos, seaLevel)` do bioma que `Level.getBiome(pos)` dá, com o nível do mar do mundo (o `BiomeInfo::temperature_at` do Horizon fixa 63, 32 e 0 por dimensão) |
| `window_frame` (basis) | `frame` | `game.mod_screen` (escreve `screen.slots`), `game.renderer`, `game.remote`, `game.world.block_entities` | H9 |
| `window_key` (basis) | `screen_key_pressed_pre` | `game.mod_screen` (o bloco da tela aberta) | H9 |
| `window_press` (basis) | `screen_mouse_button_pressed_pre` | `game.mod_screen`, `game.renderer` (o tamanho da tela em pixels de GUI) | H9 |
| `window_release` (basis) | `screen_mouse_button_released_pre` | `game.mod_screen` | H9 |
| `window_drag` (basis) | `screen_mouse_dragged_pre` | `game.mod_screen`, `game.renderer` | H9 |
| `window_scroll` (basis) | `screen_mouse_scrolled_pre` | `game.mod_screen`, `game.renderer` | H9 |
| `window_net` (basis) | `net_from_client` | `game.lan.hub.guests`, `hide_menu_slots` e `play.crafting.machine` do menu de LAN, `game.container`, `game.world.block_entities` | H12 |
| `window_player_in` (basis) | `player_logged_in` | `native_net_send_to_player` (`pub(super)` do `game`), pelo módulo `machinery_basis_windows` | H12 |
| `client_tick` (mekanism) | `tick` | `game.world.block_entities` (os tiles que o cliente vê), `game.native_window`, e pelo módulo `tile-sound` o motor de som (`game.audio`), o relógio (`game.clock`) e a câmera | H19 e H8 |
| `place_block` (mekanism) | `place_block` | `game.world.block_entities.get_mut`, `game.world.modified` | H8: `mod_block_entity`, `change_mod_block_entity` |
| `loot_drops` (mekanism) | `loot_drops` | `game.world.block_entities.get` (o `CustomName` do tile que quebrou: o Horizon já o tirou do mundo quando rola a tabela de um jogador, então o nome vem do que `destroy_block` guardou) | H8: `mod_block_entity`; `NativeLootDrops` sem o block entity é o buraco: `block_entity: Option<&Tag>` resolve |
| `container_opened` (mekanism) | `container_opened` | `game.world.block_entities.get` | H8: `mod_block_entity` |
| `destroy_block` (mekanism) | `destroy_block` | `game.spawn_orbs` (`ExperienceOrb.award`, privado de `game.rs`), e `game.world.block_entities.get` (o nome do tile, guardado antes de o Horizon o tirar do mundo, para `loot_drops`) | H8: `award_experience(at, points)`, `mod_block_entity` e `block_entity` em `NativeLootDrops` |
| `window_net_server` (basis) | `net_from_server` | o módulo `machinery_basis_windows`, que guarda os tipos de upgrade nos estáticos da lib | H12 |
| `explosion_detonate` (mekanism) | `explosion_detonate` | `game.spawn_orbs` (`ExperienceOrb.award`, privado de `game.rs`) | H8: `award_experience(at, points)` |
| `place_block`, `loot_drops`, `container_opened` e `destroy_block` (mekanismgenerators) | os mesmos | os da linha de `place_block` etc. do `mekanism`: `game.world.block_entities` e `game.world.modified`, pelas funções `name_*` da `machinery_basis` | H8: `mod_block_entity`, `change_mod_block_entity` e `block_entity` em `NativeLootDrops` |
| `menu_button` (mekanism) | `menu_button` | `game.spawn_orbs` (`ExperienceOrb.award`, privado de `game.rs`), para a aba de xp (`COLLECT_XP`) dar a experiência ao jogador que clicou; e, para `ServerPlayer.awardRecipes` + `triggerRecipeCrafted`, `game.level.recipe_book.add_recipes`, `game.recipe_unlocked`, `game.trigger_recipe_crafted` e a macro `world_stations!` (o livro de receitas e os critérios de `advancements.rs`) | H8: `award_experience(at, points)` e `award_recipes(player, recipes, items)` |
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

## Módulos (16)

Um `[[module]]` enxerta um arquivo nosso em um arquivo do Horizon e, por ser filho dele, enxerga o que é privado ali.

| Módulo | Arquivo alvo | Para quê | Precisa de |
|---|---|---|---|
| `glue` (machinery_basis) | `game.rs` | liga a lib ao jogo: tudo `pub(in crate::game)`. `Game::mb_in_levels` (2026-11-08) roda uma ação em cada nível ticado com o nível no lugar, para o `ServerTickStart`: usa `levels_to_tick`, `enter_level` e `Dim`, privados de `levels.rs` e `game.rs` | H8, H33 e o que está em [Dependências a vigiar](#dependências-a-vigiar) |
| `glue` (mekanism) | `game.rs` | liga as máquinas e a tela: 1.596 linhas | idem |
| `glue` (mekanismgenerators) | `game.rs` | liga o Heat Generator: 280 linhas | idem |
| `windows` (machinery_basis) | `game/native_hooks.rs` | a mecânica de janela e a janela de upgrades: `OpenNativeScreen` tem campos privados de lá | H9 |
| `slot-accepts` (machinery_basis) | `game/machines/native_items.rs` | `isValid` por slot, privado de `machines` | H7 |
| `tile-sound` (machinery_basis) | `game.rs` | o som contínuo de um tile (`updateSound` e `TileTickableSound`): usa `Game.audio` (`play_instance`, `stop_and_release`, `instance_mut`), `Game.clock` e a câmera, privados | H19 |
| `composter-wrapper` (machinery_basis) | `game/composter.rs` | o sorteio das camadas de um item e a subida de um nível da composteira, como o `ComposterWrapper` do NeoForge faz | H29 |
| `container-dim` (machinery_basis) | `horizon-client-render/src/hud.rs` | o estado "a tela aberta é de uma máquina nossa" (`machinery_container_dim`), que o HEAD de `HudBatch::fill` lê e o `Game` escreve | H22 |
| `sprite-ties` (machinery_basis) | `horizon-client-render/src/hud.rs` | os retângulos do atlas que as telas de mod desenham, para a injeção `sprite_ties` | H26 |
| `armor-bar` (machinery_basis) | `game.rs` | a barra de armadura do lado do cliente: a lista de slots, a posição, o "fora" da tela e o mapa dos índices do menu | H31, H32 |
| `bar-menu` (machinery_basis) | `menu.rs` (crate `horizon_client_lib`, `pub`) | os cinco slots no fim do menu de um tipo com barra, e o registro dele, que o crate de baixo enxerga (a `machinery-basis-core` só liga ao `horizon-client`) | H31 |
| `bar-machine` (machinery_basis) | `game/lan_crafting.rs` | deixa pronto o menu da conexão com os cinco slots antes do clique (`Menu::new` devolve `Self`) | H31 |
| `bar-hub` (machinery_basis) | `lan/containers.rs` | `Hub::machinery_sync_bar`: a armadura e a mão que o clique mudou vão ao cliente com a resposta dele | H31 |
| `bar-quick-move` (machinery_basis) | `game/native_quick_move.rs` | o shift-clique que parte de um slot da barra, com as guardas de `lan_mod_quick_move` e o `mayPickup` | H31 |
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
| `spike_obj_kd` | `horizon_assets::bake::obj_quad`, RETURN | cor do quad vezes o `Kd` do `.mtl` (0,8): sem isso tudo sai 20% mais claro | NeoForge `ObjGeometry.makeQuad` | H16 |
| `spike_sort_translucent` | `pvp::Game::outline_fill`, HEAD | ordena os quads de `TRANSLUCENT_ITEM_OVERLAY` do fundo para a frente: manchas sumiram | `MekanismRenderType.java:20,61,69` (`sortOnUpload`) | H17 |

Hooks do catálogo que o spike usou e que são limpos: `standalone_models`, `block_entity_update_tag`. Os que tocam estrutura privada:
`block_highlight` (`game.held()`, `game.world`, `game.res.facts()`, `game.pack.assets.registry`) e `block_entity_submit`, na versão
do spike, com os mesmos campos. Substitutos que já existem: `held`, `block`, `block_property`, e `view_hit` e `camera` no lugar de
`game.pick()` e `game.aim()`. Se `view_hit` dá o mesmo raio que `pick()` não foi conferido.

Riscos: `spike_energy_at`, `spike_save_level` e `spike_remove_entity` quebram em renomeação. `spike_override_gameplay` depende de
`NewBlock` e do formato de linha. `spike_obj_kd` e `spike_sort_translucent` são os mais frágeis: alteram a pipeline de render, e
`spike_sort_translucent` reordena todos os vértices de `outline_fill`, o que muda com qualquer ajuste de camada.

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
- **Posição.** Fica no cliente, em `config/<ns>/client.toml`, tabela `[window.upgrade]` com `x`, `y` e `pinned`, gravada em `close()`
  e lida na primeira janela: persiste entre sessões como o `lastWindowPositions` do `ClientConfig` (`ClientConfig.java:59,145-158`),
  que o NeoForge grava em `config/mekanism/client.toml`.

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
  outra em `mekanismgenerators_native`, ver [Injeções cruas em uso](#injeções-cruas-em-uso-24)). O tooltip da aba de avisos, da
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
O pedido é aceitar em `light` a mesma condição de `random_ticks`. Elimina `active_light` (mekanism).

### H19 `looping_sound`

- **Estado:** em uso, pelo módulo `tile-sound` (machinery_basis, `tile_sound.rs`), conferido em jogo em 2026-11-06: o som da Enrichment
  Chamber toca ao ligar, para ao parar e fica mudo com o upgrade de silêncio no máximo (`MEKSOUND` no stderr e vídeo com áudio).
  O módulo guarda o `InstanceId` por tile e chama `Audio::play_instance` (com `Ticking::Native`), `stop_and_release` e
  `engine().instance_mut` a cada tick do cliente, e a regra (cooldown de 20 ticks, `isFullyMuffled`, alcance, verificação a cada 20 a 39
  ticks) vem de `machinery_basis_core::sound`, repetida do Java. A câmera do último quadro faz as vezes da posição do jogador.
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
  ainda toca (`is_sound_active`) e dar a posição do jogador local no tick do cliente (`native_camera` só existe depois de um quadro).

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

## Correções pedidas ao renderer do Horizon

Estas não são injeções. São diferenças de pixel que vêm do próprio renderer do Horizon, então afetam o jogo inteiro, não só o
Mekanism. Por decisão do usuário, não são corrigidas por injeção: entram na issue do Horizon.

Os valores são em `guiScale` 2, medidos com `dev/referencia/comparar_tela.py`. Em escala 3 os números são outros, e aparecem
entre parênteses.

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

   Hoje: `injection=84 hook=57 target=27 module=16 soma_ok=sim`. Se `target` mudar, uma ficha nova ou removida tem de
   acompanhar. Se `hook` mudar, reclassifique entre limpo e com estrutura privada: 27 + 30 tem de somar `hook`.
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
