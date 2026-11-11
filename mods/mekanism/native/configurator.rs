// O Configurator (`ItemConfigurator`) e o cartão de configuração (`ItemConfigurationCard`) no jogo. As regras (os modos, as mensagens, o que o clique faz
// numa face) são de `mekanism_rules` e da `machinery_basis`; aqui fica a cola: ler e escrever o componente da pilha, achar a máquina, e o que o jogo
// expõe (o clique, o bloco, a rede, o teclado). Cada função cita o Java.
use crate::game::mekanism_cube as cube;
use crate::game::mekanism_glue as glue;
use crate::game::mekanism_rules as rules;
use crate::game::mekanism_rules::{ConfiguratorMode, DisplayChange};
use crate::game::{Game, machinery_basis as basis};
use horizon_client::containers::BlockEntity;
use horizon_client::first_person::Hand;
use horizon_client::hud::HudBatch;
use horizon_core::random::RandomSource;
use horizon_client::items::ItemStack;
use horizon_client::mod_api::Text;
use horizon_world::nbt::{Compound, JavaString, Tag};
use machinery_basis_core::side_config::SideClick;
use machinery_basis_core::sides::Face;
use machinery_basis_core::{face_overlay, item_hud, radial, status_overlay};

/// O Heat Generator não é uma máquina de progresso daqui, mas é um `TileEntityMekanism` direcional, com inventário e com o cartão (só o modo de
/// redstone). O estado dele vive no NBT do bloco (o gerador é refeito dele a cada tick).
const HEAT_GENERATOR: &str = "mekanismgenerators:heat_generator";
const HEAT_SLOTS: [usize; 2] = [0, 1];
const CONTROL_TYPE: &str = "control_type";

/// O canal do `PacketModeChange` (cliente para servidor) e o do `showModeChange` (servidor para cliente).
const MODE_CHANNEL: &str = "mekanism:mode";
const SHOW_CHANNEL: &str = "mekanism:show_mode_change";
/// O canal do `PacketRadialModeChange` (cliente para servidor): o slot, a quantidade de nomes do caminho (sempre 0: nenhum modo portado é aninhado) e a representação
/// de rede do modo (o `ordinal`).
const RADIAL_CHANNEL: &str = "mekanism:radial_mode";
/// O id do mod nas telas que ele abre (`native_mod_screen_open`).
const MOD_ID: &str = "mekanism";
/// `Mekanism.rl("mode")`: a tecla `N` (`MekanismKeyHandler.handModeSwitchKey`), que o jogador pode trocar.
const MODE_KEY_NAME: &str = "mode";
const MODE_KEY_DEFAULT: &str = "key.keyboard.n";

fn text(chat: &rules::Chat) -> Text {
    let args: Vec<Text> = chat.with.iter().map(text).collect();
    let mut out = Text::translate(&chat.translate, &args);
    if let Some(rgb) = chat.color {
        out = out.with_style(&horizon_client::text::Style::color(rgb));
    }
    out
}

/// `player.sendOverlayMessage`, ao jogador que age.
fn overlay(game: &mut Game<'_>, chat: &rules::Chat) {
    if let Some(player) = game.native_acting_player() {
        game.native_action_bar(player.uuid, text(chat));
    }
}

fn face_of(normal: [i32; 3]) -> Face {
    match normal {
        [0, 1, 0] => Face::Up,
        [0, -1, 0] => Face::Down,
        [0, 0, -1] => Face::North,
        [0, 0, 1] => Face::South,
        [-1, 0, 0] => Face::West,
        _ => Face::East,
    }
}

/// `ItemConfigurator.getMode`: o componente, ou o padrão (a pilha nova traz o padrão; uma sem o componente também o responde).
fn mode_of(game: &Game<'_>, stack: &ItemStack) -> ConfiguratorMode {
    match game.native_component(stack, rules::CONFIGURATOR_MODE_COMPONENT) {
        Ok(Some(Tag::String(name))) => ConfiguratorMode::from_serialized_name(&name.to_string_lossy()).unwrap_or(ConfiguratorMode::DEFAULT),
        _ => ConfiguratorMode::DEFAULT,
    }
}

/// `setMode`: o modo padrão não deixa remendo (o protótipo do item já o tem), qualquer outro grava o nome.
fn set_mode(game: &Game<'_>, stack: &mut ItemStack, mode: ConfiguratorMode) {
    let _ = game.native_set_component(stack, rules::CONFIGURATOR_MODE_COMPONENT, Tag::String(JavaString::from(mode.serialized_name())));
}

/// Um `TileEntityMekanism` daqui: a máquina de progresso ou o Heat Generator.
fn is_mekanism_tile(block: &str) -> bool {
    glue::is_machine_block(block) || block == HEAT_GENERATOR || cube::is_cube(block)
}

/// `TileEntityMekanism.hasInventory` e os slots de `getInventorySlots`.
fn inventory_slots(block: &str) -> Vec<usize> {
    if block == HEAT_GENERATOR {
        HEAT_SLOTS.to_vec()
    } else if cube::is_cube(block) {
        vec![machinery_basis_core::energy_cube::DISCHARGE_SLOT, machinery_basis_core::energy_cube::CHARGE_SLOT]
    } else {
        glue::inventory_slots(block)
    }
}

fn held_item(game: &Game<'_>) -> Option<(ItemStack, bool)> {
    let stack = game.held()?.clone();
    Some((stack, matches!(game.hand, Hand::Main)))
}

fn swing(game: &mut Game<'_>) {
    // Um convidado balança o braço no cliente dele, e não no servidor.
    if game.lan_acting.is_none() {
        game.swing_arm();
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// O clique do item (`Item.useOn`), depois de o bloco deixar passar.

/// `ItemConfigurator.useOn` e `ItemConfigurationCard.useOn`.
pub(in crate::game) fn use_item_on(game: &mut Game<'_>, stack: &ItemStack, pos: [i32; 3], face: [i32; 3], claimed: &mut bool) {
    if *claimed {
        return;
    }
    let face = face_of(face);
    let result = match &*stack.item {
        rules::CONFIGURATOR_ITEM => configurator_use_on(game, stack, pos, face),
        rules::CONFIGURATION_CARD_ITEM => {
            let used = card_use_on(game, stack, pos);
            if used {
                note_card_use(game);
            }
            used
        }
        _ => return,
    };
    *claimed = result;
}

/// Devolve se o clique foi do item (`SUCCESS` ou `FAIL`); `PASS` segue o caminho normal.
fn configurator_use_on(game: &mut Game<'_>, stack: &ItemStack, pos: [i32; 3], face: Face) -> bool {
    let mode = mode_of(game, stack);
    let sneaking = game.native_sneaking();
    let block = game.native_block(pos);
    if let Some(kind) = mode.transmission() {
        if cube::is_cube(&block) {
            let click = cube::with_cube(game, pos, |cube| {
                let facing = cube.facing();
                cube.side_config_mut().configurator_click(kind, facing, face, sneaking)
            })
            .flatten();
            return match click {
                Some(SideClick::View(data_type)) => {
                    overlay(game, &rules::configurator_side_message(false, kind, data_type));
                    true
                }
                Some(SideClick::Toggled { old, new }) => {
                    if old != new {
                        overlay(game, &rules::configurator_side_message(true, kind, new));
                    }
                    true
                }
                None => false,
            };
        }
        if !glue::is_machine_block(&block) {
            // `IConfigurable`: nenhum bloco daqui a expõe (a lixeira, o tanque e a bomba não existem ainda), então o clique passa.
            return false;
        }
        let click = glue::with_machine(game, pos, |machine| {
            let facing = machine.facing();
            machine.side_config_mut().configurator_click(kind, facing, face, sneaking)
        })
        .flatten();
        return match click {
            Some(SideClick::View(data_type)) => {
                overlay(game, &rules::configurator_side_message(false, kind, data_type));
                true
            }
            Some(SideClick::Toggled { old, new }) => {
                if old != new {
                    overlay(game, &rules::configurator_side_message(true, kind, new));
                }
                true
            }
            None => false,
        };
    }
    match mode {
        ConfiguratorMode::Empty => {
            if is_mekanism_tile(&block) {
                empty(game, pos, &block, face);
                return true;
            }
            false
        }
        // Qualquer outro bloco responde `SUCCESS` (só um `TileEntityMekanism` que não gira passa), e o daqui gira para a face clicada, ou para a oposta
        // se o jogador está agachado (`ItemConfigurator.java:174`).
        ConfiguratorMode::Rotate => {
            if is_mekanism_tile(&block) {
                set_facing(game, pos, &block, if sneaking { face.opposite() } else { face });
            }
            true
        }
        ConfiguratorMode::Wrench => {
            // `doesSneakBypassUse`: com o jogador agachado o bloco age antes (`BlockTile.useItemOn` -> `tryWrench` -> `tryWrenchDismantle`).
            if sneaking && is_mekanism_tile(&block) {
                dismantle(game, pos);
                return true;
            }
            false
        }
        _ => false,
    }
}

/// `TileEntityMekanism.setFacing`: o bloco muda a propriedade `facing`, e a máquina avisa as faces dela no mesmo instante.
fn set_facing(game: &mut Game<'_>, pos: [i32; 3], block: &str, facing: Face) {
    if basis::front(game, pos) == facing {
        return;
    }
    game.native_set_block_property(pos, "facing", facing.name());
    if glue::is_machine_block(block) {
        glue::with_machine(game, pos, |machine| machine.set_facing(facing));
    } else if cube::is_cube(block) {
        cube::with_cube(game, pos, |cube| cube.set_facing(facing));
    }
}

/// `ItemConfigurator.useOn`, o ramo `EMPTY`: cada slot solta a pilha pela face clicada (`InventoryUtils.dropStack` e `Block.popResourceFromFace`) e fica vazio.
fn empty(game: &mut Game<'_>, pos: [i32; 3], block: &str, face: Face) {
    let at = (pos[0], pos[1], pos[2]);
    let slots = inventory_slots(block);
    let mut stacks = Vec::new();
    if let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&at) {
        for index in slots {
            if let Some(slot) = entity.items.get_mut(index)
                && let Some(stack) = slot.take()
                && stack.count > 0
            {
                stacks.push(stack);
            }
        }
    }
    if stacks.is_empty() {
        return;
    }
    game.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
    let step = face.offset();
    for stack in stacks {
        let max = game.res.items().get(&stack.item).map_or(64, |item| item.max_stack).max(1);
        let mut left = stack.count;
        while left > 0 {
            let count = left.min(max);
            left -= count;
            let mut one = stack.clone();
            one.count = count;
            // `Block.popResourceFromFace`: 0,65 para fora na face, e um sorteio de -0,25 a 0,25 nos outros eixos.
            let random = &mut game.world.random;
            let mut at = [0.0; 3];
            for axis in 0..3 {
                at[axis] = if step[axis] == 0 { f64::from(pos[axis]) + 0.5 + (random.next_double() * 0.5 - 0.25) } else { f64::from(pos[axis]) + 0.5 + f64::from(step[axis]) * 0.65 };
            }
            let motion = [f64::from(step[0]) * 0.1, f64::from(step[1]) * 0.1, f64::from(step[2]) * 0.1];
            game.spawn_item(one, at, motion, 10);
        }
    }
}

/// `WorldUtils.dismantleBlock`: os drops do bloco (a loot table com a configuração) vão para o inventário de quem usa (o que não cabe cai), sem
/// experiência, e o bloco sai sem partícula nem som (`Level.removeBlock`). Vale em criativo também.
fn dismantle(game: &mut Game<'_>, pos: [i32; 3]) {
    let key = (pos[0], pos[1], pos[2]);
    let state = game.world.block(pos[0], pos[1], pos[2]);
    let name = game.res.facts().get(state).name.clone();
    let held = game.held().cloned();
    let loot_entity = game.loot_block_entity(pos, &name);
    if let Some(entity) = game.world.block_entities.get(&key).cloned() {
        let custom = game.world.block_names.get(&key).cloned();
        game.pending.record(pos, &entity, &name, custom);
    }
    let drops = game.block_loot_at(pos, state, held.as_ref(), loot_entity);
    let removed = game.with_level(|b, level, ticks| {
        b.player_destroy_block(level, ticks, horizon_core::position::BlockPos::new(pos[0], pos[1], pos[2]), true, true)
    });
    let _ = removed;
    let player = game.native_acting_player().map(|player| player.uuid);
    for mut drop in drops {
        if &*drop.item == "minecraft:air" {
            continue;
        }
        game.pending.attach(pos, &mut drop);
        let left = match player {
            Some(uuid) => game.native_give_stack(uuid, drop),
            None => Some(drop),
        };
        if let Some(left) = left {
            let at = [f64::from(pos[0]) + 0.5, f64::from(pos[1]) + 0.5, f64::from(pos[2]) + 0.5];
            game.spawn_item(left, at, [0.0; 3], 10);
        }
    }
    game.pending.clear(pos);
}

// ---------------------------------------------------------------------------------------------------------------------------
// O bloco, antes de abrir a tela (`BlockTile.useItemOn` e `useWithoutItem`).

/// O que o Java resolve no bloco antes do item: o Configurator em `ROTATE` gira (`TileEntityMekanism.tryWrenchRotate`) e o cartão em mão passa a vez da
/// tela (`TileEntityMekanism.openGui` devolve `TRY_WITH_EMPTY_HAND`) e age em seguida. O Horizon abriria a tela primeiro, então o clique é feito aqui e o
/// resto é cancelado. Só a mão principal e só sem agachar: agachado o bloco nem chega a agir (e a chave inglesa age no `use_item_on`).
pub(in crate::game) fn right_click_block(game: &mut Game<'_>, pos: [i32; 3], face: [i32; 3], canceled: &mut bool) {
    if *canceled || game.native_sneaking() {
        return;
    }
    let Some((stack, main)) = held_item(game) else {
        return;
    };
    if !main {
        return;
    }
    let block = game.native_block(pos);
    match &*stack.item {
        rules::CONFIGURATOR_ITEM if mode_of(game, &stack) == ConfiguratorMode::Rotate && is_mekanism_tile(&block) => {
            let facing = rules::rotate_facing(basis::front(game, pos), true);
            set_facing(game, pos, &block, facing);
        }
        rules::CONFIGURATION_CARD_ITEM if is_mekanism_tile(&block) => {
            card_use_on(game, &stack, pos);
        }
        _ => return,
    }
    let _ = face;
    *canceled = true;
    swing(game);
}

// ---------------------------------------------------------------------------------------------------------------------------
// O cartão de configuração.

/// `ConfigurationData` na pilha: o compound, se o cartão tem algum.
fn card_data(game: &Game<'_>, stack: &ItemStack) -> Option<Compound> {
    match game.native_component(stack, rules::CONFIGURATION_DATA_COMPONENT) {
        Ok(Some(Tag::Compound(data))) if !data.is_empty() => Some(data.clone()),
        _ => None,
    }
}

fn string_of(compound: &Compound, key: &str) -> Option<String> {
    match compound.get(&JavaString::from(key)) {
        Some(Tag::String(value)) => Some(value.to_string_lossy()),
        _ => None,
    }
}

/// `writeConfigurationData` do bloco em `pos`: o modo de redstone, a configuração de lados e o ejetor (a máquina), ou só o modo de redstone (o
/// Heat Generator, cujo estado é o NBT).
fn card_write(game: &Game<'_>, pos: [i32; 3], block: &str) -> Option<Compound> {
    if block == HEAT_GENERATOR {
        let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(pos[0], pos[1], pos[2])) else {
            return None;
        };
        let mut out = Compound::default();
        let control = string_of(&entity.nbt, CONTROL_TYPE).unwrap_or_else(|| "disabled".to_owned());
        out.insert(JavaString::from(CONTROL_TYPE), Tag::String(JavaString::from(control.as_str())));
        return Some(out);
    }
    if cube::is_cube(block) {
        return cube::write_configuration_data(game, pos);
    }
    glue::write_configuration_data(game, pos)
}

fn card_read(game: &mut Game<'_>, pos: [i32; 3], block: &str, input: &Compound) {
    if block == HEAT_GENERATOR {
        // `TileEntityMekanism.setConfigurationData` -> `readSustainedData`: o modo de redstone volta a `DISABLED` se faltava.
        let control = string_of(input, CONTROL_TYPE).unwrap_or_else(|| "disabled".to_owned());
        if let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&(pos[0], pos[1], pos[2])) {
            entity.nbt.insert(JavaString::from(CONTROL_TYPE), Tag::String(JavaString::from(control.as_str())));
        }
        game.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
        return;
    }
    if cube::is_cube(block) {
        cube::set_configuration_data(game, pos, input);
        cube::with_cube(game, pos, |_| ());
        return;
    }
    glue::set_configuration_data(game, pos, input);
    // As faces de energia seguem a configuração nova na hora (`configurationDataSet` -> `invalidateCapabilitiesFull`).
    glue::with_machine(game, pos, |_| ());
}

/// `ItemConfigurationCard.useOn`. Devolve se o clique foi do cartão (`SUCCESS_SERVER`).
fn card_use_on(game: &mut Game<'_>, stack: &ItemStack, pos: [i32; 3]) -> bool {
    let block = game.native_block(pos);
    if !is_mekanism_tile(&block) {
        return false;
    }
    if game.native_sneaking() {
        let Some(config) = card_write(game, pos, &block) else {
            return false;
        };
        let name = rules::block_description_id(&block);
        let mut data = Compound::default();
        data.insert(JavaString::from(rules::CARD_DATA_NAME), Tag::String(JavaString::from(name.as_str())));
        data.insert(JavaString::from(rules::CARD_DATA_TYPE), Tag::String(JavaString::from(block.as_str())));
        // "We store the child data in a separate value output": o filho `config` só existe se tem algo.
        if !config.is_empty() {
            data.insert(JavaString::from(rules::CARD_CONFIG), Tag::Compound(config));
        }
        let mut card = stack.clone();
        if let Err(error) = game.native_set_component(&mut card, rules::CONFIGURATION_DATA_COMPONENT, Tag::Compound(data)) {
            eprintln!("mekanism: o cartão não guardou os dados: {error}");
        }
        *game.held_mut() = Some(card);
        overlay(game, &rules::card_got_message(&name));
        return true;
    }
    let Some(data) = card_data(game, stack) else {
        return false;
    };
    // O tipo guardado tem de ser um bloco que existe (`BLOCK_CODEC`); sem ele o cartão passa.
    let Some(stored) = string_of(&data, rules::CARD_DATA_TYPE) else {
        return false;
    };
    if stored == block {
        if let Some(Tag::Compound(config)) = data.get(&JavaString::from(rules::CARD_CONFIG)) {
            let config = config.clone();
            card_read(game, pos, &block, &config);
        }
        let name = string_of(&data, rules::CARD_DATA_NAME);
        overlay(game, &rules::card_set_message(name.as_deref()));
    } else {
        overlay(game, &rules::card_unequal_message());
    }
    true
}

/// `ItemConfigurationCard.use`: agachado no ar (ou sobre um bloco que não age), o cartão esvazia.
pub(in crate::game) fn use_item(game: &mut Game<'_>, stack: &ItemStack, claimed: &mut bool) {
    if *claimed || &*stack.item != rules::CONFIGURATION_CARD_ITEM || !game.native_sneaking() || card_just_used(game) {
        return;
    }
    let mut card = stack.clone();
    let _ = game.native_set_component(&mut card, rules::CONFIGURATION_DATA_COMPONENT, Tag::Compound(Compound::default()));
    *game.held_mut() = Some(card);
    overlay(game, &rules::card_cleared_message());
    *claimed = true;
}

// ---------------------------------------------------------------------------------------------------------------------------
// O modelo e o tooltip.

/// `ConfigCardProperty` (`mekanism:config_card`): o cartão tem dados (`ItemConfigurationCard.hasData`: o compound traz `data_name`).
pub(in crate::game) fn item_model_property(property: &str, stack: &horizon_assets::item_model::StackView, value: &mut bool) {
    if property != "mekanism:config_card" || stack.item != rules::CONFIGURATION_CARD_ITEM {
        return;
    }
    if let Some(Some(data)) = stack.patch.get(rules::CONFIGURATION_DATA_COMPONENT) {
        *value = data.get(rules::CARD_DATA_NAME).is_some();
    }
}

/// As linhas dos appenders de `CONFIGURATOR_MODE` e `CONFIGURATION_DATA`, depois do nome.
pub(in crate::game) fn tooltip(game: &Game<'_>, stack: &ItemStack, lines: &mut Vec<(Text, u32)>) {
    match &*stack.item {
        rules::CONFIGURATOR_ITEM => {
            // O appender só roda com o componente: uma pilha nova o traz, e uma a que se tirou o componente não.
            if matches!(game.native_component(stack, rules::CONFIGURATOR_MODE_COMPONENT), Err(_)) {
                return;
            }
            let chat = mode_of(game, stack).tooltip_line();
            lines.push((text(&chat), u32::MAX));
        }
        rules::CONFIGURATION_CARD_ITEM => {
            let name = card_data(game, stack).and_then(|data| string_of(&data, rules::CARD_DATA_NAME));
            let chat = rules::card_tooltip_line(name.as_deref());
            lines.push((text(&chat), u32::MAX));
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// A troca de modo (`IModeItem`): a tecla, a rolagem agachado e o pacote.

/// `IModeItem.isModeItem(instance, slotType, allowRadial)`: o Configurator é um item de modo, e tem menu radial (`getRadialData` não é nulo).
fn is_mode_item(stack: &ItemStack, allow_radial: bool) -> bool {
    &*stack.item == rules::CONFIGURATOR_ITEM && (allow_radial || !has_radial(stack))
}

/// `IGenericRadialModeItem.getRadialData(instance) != null`.
fn has_radial(stack: &ItemStack) -> bool {
    &*stack.item == rules::CONFIGURATOR_ITEM
}

/// `player.getMainHandItem()`: o slot selecionado da barra.
fn main_hand<'a>(game: &'a Game<'_>) -> Option<&'a ItemStack> {
    game.inventory.get(game.selected)?.as_ref()
}

fn send_mode(game: &mut Game<'_>, shift: i32, display: DisplayChange) {
    let mut body = vec![0u8, match display {
        DisplayChange::None => 0,
        DisplayChange::MainHand => 1,
        DisplayChange::Other => 2,
    }];
    body.extend_from_slice(&shift.to_be_bytes());
    let _ = game.native_net_send_to_server(MODE_CHANNEL, &body);
}

/// `MekanismKeyHandler.handModeSwitchKey`: a tecla do modo é também a do menu radial (`MekKeyHandler.isRadialPressed`), então o estado dela vale sempre; o
/// `onKeyDown` só manda a troca de modo para um item sem menu radial (`IModeItem.isModeItem(player, MAINHAND, false)`), e o Configurator tem um: a tecla só abre o menu.
pub(in crate::game) fn key(game: &mut Game<'_>, key: &str, pressed: bool) {
    if key != game.native_mod_key("mekanism", MODE_KEY_NAME, MODE_KEY_DEFAULT) {
        return;
    }
    effects().key_held = pressed;
    if !pressed || !game.native_playing() {
        return;
    }
    if !main_hand(game).is_some_and(|stack| is_mode_item(stack, false)) {
        return;
    }
    let shift = if game.native_sneaking() { -1 } else { 1 };
    send_mode(game, shift, DisplayChange::MainHand);
}

/// `ClientTickHandler.onMouseEvent`: agachado, a rolagem com um item de modo na mão troca o modo (`allowModeScroll`, padrão ligado) em vez de trocar o
/// slot da barra.
pub(in crate::game) fn scroll(game: &mut Game<'_>, delta: [f64; 2], canceled: &mut bool) {
    if !glue::effects_config().allow_mode_scroll || !game.native_sneaking() || delta[1] == 0.0 || !main_hand(game).is_some_and(|stack| is_mode_item(stack, true)) {
        return;
    }
    let shift = effects().mode_scroll.scroll(delta[1], game.visual_ticks() as i64);
    if shift != 0 {
        send_mode(game, shift, DisplayChange::None);
        mode_timer_start();
    }
    *canceled = true;
}

/// `PacketModeChange.handle`: o servidor troca o modo da pilha na mão do jogador, e conforme o `DisplayChange` avisa.
pub(in crate::game) fn net_from_client(game: &mut Game<'_>, player: u128, channel: &str, data: &[u8]) {
    if channel == RADIAL_CHANNEL {
        return radial_net_from_client(game, player, data);
    }
    if channel != MODE_CHANNEL || data.len() != 6 {
        return;
    }
    let display = match data[1] {
        1 => DisplayChange::MainHand,
        2 => DisplayChange::Other,
        _ => DisplayChange::None,
    };
    let shift = i32::from_be_bytes([data[2], data[3], data[4], data[5]]);
    let Some(mut host) = game.lan.take() else {
        return;
    };
    let conn = host.hub.guests.iter().find(|guest| guest.uuid == player).map(|guest| guest.conn);
    if let Some(conn) = conn {
        game.as_guest(&mut host, conn, |game| change_mode(game, player, shift, display));
    }
    game.lan = Some(host);
}

fn change_mode(game: &mut Game<'_>, player: u128, shift: i32, display: DisplayChange) {
    let Some(stack) = game.held().cloned() else {
        return;
    };
    if !is_mode_item(&stack, true) {
        return;
    }
    let change = rules::change_mode(mode_of(game, &stack), shift, display);
    if !change.changed {
        return;
    }
    let mut next = stack;
    set_mode(game, &mut next, change.mode);
    *game.held_mut() = Some(next);
    if let Some(chat) = &change.overlay {
        game.native_action_bar(player, text(chat));
    }
    if change.show {
        let _ = game.native_net_send_to_player(player, SHOW_CHANNEL, &[]);
    }
}

/// `PacketShowModeChange` no cliente: o texto do modo aparece sobre a barra.
pub(in crate::game) fn net_from_server(channel: &str) {
    if channel == SHOW_CHANNEL {
        mode_timer_start();
    }
}

/// `MekanismStatusOverlay.setTimer`.
fn mode_timer_start() {
    effects().status.set_timer();
}

// ---------------------------------------------------------------------------------------------------------------------------
// A seleção parcial dos transmissores com o Configurator na mão (decisão 1).

/// `BlockTransmitter.getShape` com o Configurator em uma das mãos (`CollisionContext.isHoldingItem`): a caixa que o olhar toca
/// (`MultipartUtils.collisionRayTrace` sobre `getCollisionBoxes`, o centro e uma por lado ligado), e o centro se nenhuma. Sem o Configurator, ou em outro bloco, o
/// Horizonte segue com a forma inteira. As caixas são as da forma do estado, em coordenadas do bloco.
pub(in crate::game) fn partial_outline(
    game: &Game<'_>,
    pos: [i32; 3],
    state: horizon_world::registry::BlockStateId,
) -> core::ops::ControlFlow<Option<[f64; 6]>> {
    let holding = [game.held(), game.offhand.as_ref()].into_iter().flatten().any(|stack| &*stack.item == rules::CONFIGURATOR_ITEM);
    if !holding {
        return core::ops::ControlFlow::Continue(());
    }
    let facts = game.res.facts().get(state);
    let Some(center) = rules::transmitter_center(&facts.name) else {
        return core::ops::ControlFlow::Continue(());
    };
    let Some((origin, direction)) = game.aim() else {
        return core::ops::ControlFlow::Break(Some(center));
    };
    let reach = game.reach();
    let local = [origin[0] - f64::from(pos[0]), origin[1] - f64::from(pos[1]), origin[2] - f64::from(pos[2])];
    let end: [f64; 3] = std::array::from_fn(|axis| local[axis] + direction[axis] * reach);
    core::ops::ControlFlow::Break(Some(rules::select_box(&facts.outline, local, end, center)))
}

thread_local! {
    /// O jogador e o tique do último clique do cartão que agiu sobre um bloco (`useOn` devolveu `SUCCESS_SERVER`).
    static CARD_USE: std::cell::Cell<Option<(u128, i64)>> = const { std::cell::Cell::new(None) };
}

fn note_card_use(game: &Game<'_>) {
    if let Some(player) = game.native_acting_player() {
        CARD_USE.set(Some((player.uuid, game.world.game_time)));
    }
}

/// No original o cliente já sabe que o clique foi do cartão (`SUCCESS_SERVER` conta como consumido) e não manda o uso no ar do mesmo clique. O cliente do
/// Horizon manda os dois, então o `use` que chega no mesmo clique, logo depois de um `useOn` do cartão, é descartado (H72).
fn card_just_used(game: &Game<'_>) -> bool {
    let Some(player) = game.native_acting_player() else {
        return false;
    };
    CARD_USE.get().is_some_and(|(uuid, tick)| uuid == player.uuid && game.world.game_time - tick <= 2)
}

// ---------------------------------------------------------------------------------------------------------------------------
// Os efeitos de tela (decisão 48): o texto do modo sobre a barra (`MekanismStatusOverlay`), o texto do HUD (`MekanismHUD`), o menu radial (`GuiRadialSelector`) e a
// sobreposição da face (`ConfiguratorOverlayHandler`). As contas são da `machinery_basis_core`; aqui ficam o estado e o que o jogo expõe.

/// O que os efeitos guardam de um quadro para o outro. O `Game` do cliente e o do servidor integrado correm em threads diferentes e os ganchos de tela só leem o `Game`
/// (`&self`), então o estado é do processo, como o contador do modo era.
struct Effects {
    /// `MekanismStatusOverlay.INSTANCE`.
    status: status_overlay::StatusOverlay,
    /// `ClientTickHandler.scrollIncrementer` (`new ScrollIncrementer(true)`).
    mode_scroll: radial::ScrollIncrementer,
    /// A tecla do modo está apertada (`MekKeyHandler.isRadialPressed`).
    key_held: bool,
    /// O menu radial aberto (`minecraft.gui.screen() instanceof GuiRadialSelector`).
    radial: Option<RadialUi>,
    /// Os modos que o menu escolheu, a mandar no próximo tique (os ganchos de tela não têm o `Game` mutável).
    pending: Vec<(radial::Slot, usize)>,
    /// O bloco que a sobreposição cobre neste quadro, para o contorno padrão sair (`showingConfiguratorOverlay`).
    overlay: Option<[i32; 3]>,
}

/// `GuiRadialSelector`: o slot do item e a fatia sob o mouse no último quadro.
struct RadialUi {
    slot: radial::Slot,
    selection: Option<usize>,
    /// `GuiRadialSelector.scrollIncrementer` (`new ScrollIncrementer(true)`).
    scroll: radial::ScrollIncrementer,
}

static EFFECTS: std::sync::Mutex<Effects> = std::sync::Mutex::new(Effects {
    status: status_overlay::StatusOverlay::new(),
    mode_scroll: radial::ScrollIncrementer::new(),
    key_held: false,
    radial: None,
    pending: Vec::new(),
    overlay: None,
});

fn effects() -> std::sync::MutexGuard<'static, Effects> {
    EFFECTS.lock().unwrap_or_else(|error| error.into_inner())
}

/// `ARGB` do Java para a cor de vértice do HUD do Horizon.
fn hud_color(argb: u32) -> u32 {
    horizon_client::hud::rgba((argb >> 16) as u8, (argb >> 8) as u8, argb as u8, (argb >> 24) as u8)
}

/// `ARGB.multiplyAlpha(color, multiplier)`.
fn multiply_alpha(color: u32, multiplier: f32) -> u32 {
    if color == 0 || multiplier <= 0.0 {
        0
    } else if multiplier >= 1.0 {
        color
    } else {
        let alpha = (color >> 24) as f32 / 255.0 * multiplier;
        (((alpha * 255.0).floor() as u32) << 24) | (color & 0x00FF_FFFF)
    }
}

/// `EnumColor.getColor(i)`: o canal do RGB em `[0, 1]`.
fn color_channels(color: u8) -> [f32; 3] {
    let rgb = rules::enum_color_rgb(color);
    [(rgb >> 16 & 0xFF) as f32 / 255.0, (rgb >> 8 & 0xFF) as f32 / 255.0, (rgb & 0xFF) as f32 / 255.0]
}

fn is_spectator(game: &Game<'_>) -> bool {
    matches!(game.mode, horizon_client::command::args::GameMode::Spectator)
}

/// `Hud.leftHeight` e `rightHeight` depois das camadas de vida: no criativo e no espectador elas não rodam e ficam em 39.
fn bar_heights(game: &Game<'_>) -> (i32, i32) {
    if game.creative || is_spectator(game) { (39, 39) } else { (game.health_hud.left_height, game.health_hud.right_height) }
}

/// As camadas do `RegisterGuiLayersEvent` (`ClientRegistration.registerOverlays`): o status sobre a camada do nome do item selecionado e o HUD sobre a das legendas.
pub(in crate::game) fn gui_layer_post(game: &mut Game<'_>, layer: &str, batch: &mut HudBatch) {
    match layer {
        "minecraft:selected_item_name" => draw_status(game, batch),
        "minecraft:subtitle_overlay" => draw_hud(game, batch),
        _ => {}
    }
}

/// `MekanismStatusOverlay.render`.
fn draw_status(game: &mut Game<'_>, batch: &mut HudBatch) {
    use horizon_client::command::args::GameMode;
    let (left_height, right_height) = bar_heights(game);
    let around = status_overlay::Surroundings {
        gui_width: batch.width as i32,
        gui_height: batch.height as i32,
        left_height,
        right_height,
        can_hurt: matches!(game.mode, GameMode::Survival | GameMode::Adventure),
        overlay_message: game.titles.overlay_time > 0,
        gui_ticks: game.visual_ticks() as i32,
        active: game.player.is_some() && !is_spectator(game) && !game.ui.hide_gui,
    };
    // `IModeItem.isModeItem(stack, MAINHAND)` e `getScrollTextComponent`.
    let scroll_text = main_hand(game).filter(|stack| is_mode_item(stack, true)).map(|stack| text(&mode_of(game, stack).scroll_text()));
    let width = scroll_text.as_ref().map(|text| game.gui.atlas.font.width_text(text) as i32);
    let Some(draw) = effects().status.render(&around, width) else {
        return;
    };
    let Some(scroll_text) = scroll_text else {
        return;
    };
    // `GuiUtils.drawBackdrop(.., 0, 0, componentWidth, color.af())`: `options.getBackgroundColor(0.0F)` com o alfa do texto.
    let background = multiply_alpha(game.settings.text_background(0.0), draw.alpha);
    if background != 0 {
        batch.fill(&game.gui.atlas, draw.x - 2.0, draw.y as f32 - 2.0, draw.width as f32 + 4.0, 13.0, hud_color(background));
    }
    let argb = (u32::from(status_overlay::alpha_byte(draw.alpha)) << 24) | 0x00FF_FFFF;
    batch.text_styled(&game.gui.atlas, &scroll_text, draw.x, draw.y as f32, hud_color(argb), true);
}

/// `IItemHUDProvider.addHUDStrings` do item numa pilha: o Configurator dá o modo.
fn hud_strings(game: &Game<'_>, stack: &ItemStack) -> Vec<rules::Chat> {
    if &*stack.item == rules::CONFIGURATOR_ITEM {
        return vec![mode_of(game, stack).hud_line()];
    }
    Vec::new()
}

/// `MekanismHUD.render`, a parte do texto.
fn draw_hud(game: &mut Game<'_>, batch: &mut HudBatch) {
    let config = glue::effects_config();
    if game.player.is_none() || is_spectator(game) || game.ui.hide_gui || !config.hud_enabled {
        return;
    }
    // `EQUIPMENT_ORDER`: a mão secundária, a principal, a cabeça, o peito, as pernas e os pés.
    let stacks: [Option<&ItemStack>; 6] = [game.offhand.as_ref(), main_hand(game), game.armor[0].as_ref(), game.armor[1].as_ref(), game.armor[2].as_ref(), game.armor[3].as_ref()];
    let groups: Vec<Vec<Text>> = stacks
        .into_iter()
        .flatten()
        .map(|stack| hud_strings(game, stack).iter().map(text).collect::<Vec<Text>>())
        .filter(|lines| !lines.is_empty())
        .collect();
    let widths: Vec<Vec<i32>> = groups.iter().map(|lines| lines.iter().map(|line| game.gui.atlas.font.width_text(line) as i32).collect()).collect();
    let background = game.settings.text_background(0.0);
    let layout = item_hud::layout(&widths, batch.width as i32, batch.height as i32, config.hud_scale, config.hud_reverse, background != 0);
    if layout.lines.is_empty() {
        return;
    }
    let start = batch.vertices.len();
    if let Some(backdrop) = layout.backdrop {
        batch.fill(
            &game.gui.atlas,
            backdrop.x0 as f32,
            backdrop.y0 as f32,
            (backdrop.x1 - backdrop.x0) as f32,
            (backdrop.y1 - backdrop.y0) as f32,
            hud_color(background),
        );
    }
    for line in &layout.lines {
        batch.text_styled(&game.gui.atlas, &groups[line.group][line.index], line.x as f32, line.y as f32, hud_color(item_hud::TEXT_ARGB), true);
    }
    // `pose.scale(hudScale, hudScale)` a partir do canto de cima à esquerda.
    batch.scale_since(start, 0.0, 0.0, layout.scale, layout.scale);
}

// ---------------------------------------------------------------------------------------------------------------------------
// O menu radial.

/// `IGenericRadialModeItem` no slot: o Configurator.
fn radial_stack<'a>(game: &'a Game<'_>, slot: radial::Slot) -> Option<&'a ItemStack> {
    match slot {
        radial::Slot::MainHand => main_hand(game),
        radial::Slot::OffHand => game.offhand.as_ref(),
    }
    .filter(|stack| has_radial(stack))
}

/// `ClientTickHandler.tickEnd`, a parte do menu radial: abre com a tecla apertada e um item com menu, fecha sem ela.
pub(in crate::game) fn tick(game: &mut Game<'_>) {
    if !game.native_window() {
        return;
    }
    use crate::game::Screen;
    let ours = game.native_mod_screen() == Some(MOD_ID);
    // O menu fechou por fora (a tecla de sair da tela): `removed()`.
    let closed_outside = !ours && effects().radial.is_some();
    if closed_outside {
        radial_removed();
    }
    let no_screen = game.screen == Screen::Playing && game.native_playing();
    let key_held = effects().key_held;
    let tick = radial::tick(
        no_screen || ours,
        key_held,
        radial_stack(game, radial::Slot::MainHand).is_some(),
        radial_stack(game, radial::Slot::OffHand).is_some(),
        ours,
    );
    match tick {
        radial::Tick::Idle => {}
        radial::Tick::Open(slot) => {
            let same = ours && effects().radial.as_ref().is_some_and(|ui| ui.slot == slot);
            if !same && game.native_mod_screen_open(MOD_ID) {
                effects().radial = Some(RadialUi { slot, selection: None, scroll: radial::ScrollIncrementer::new() });
            }
        }
        radial::Tick::Close => {
            game.native_mod_screen_close(MOD_ID);
            radial_removed();
        }
    }
    flush_pending(game);
}

/// `GuiRadialSelector.removed`: o modo sob o mouse na hora de fechar é o escolhido.
fn radial_removed() {
    let mut effects = effects();
    if let Some(ui) = effects.radial.take()
        && let Some(index) = ui.selection
    {
        effects.pending.push((ui.slot, index));
    }
}

/// Manda os modos que o menu escolheu (`updateSelection`): um que difere do atual (`selection_to_send` já conferiu na hora de pôr na fila).
fn flush_pending(game: &mut Game<'_>) {
    let pending = std::mem::take(&mut effects().pending);
    for (slot, index) in pending {
        let current = radial_stack(game, slot).map(|stack| mode_of(game, stack).ordinal());
        if radial::selection_to_send(Some(index), current).is_none() {
            continue;
        }
        let mut body = vec![match slot {
            radial::Slot::MainHand => 0u8,
            radial::Slot::OffHand => 1,
        }];
        body.push(0);
        body.extend_from_slice(&(index as i32).to_be_bytes());
        let _ = game.native_net_send_to_server(RADIAL_CHANNEL, &body);
    }
}

/// `PacketRadialModeChange.handle`: o servidor põe o modo do menu (`fromNetworkRepresentation`, por módulo) no item do slot, sem mensagem.
fn radial_net_from_client(game: &mut Game<'_>, player: u128, data: &[u8]) {
    if data.len() != 6 || data[1] != 0 {
        return;
    }
    let slot = if data[0] == 0 { radial::Slot::MainHand } else { radial::Slot::OffHand };
    let representation = i32::from_be_bytes([data[2], data[3], data[4], data[5]]);
    let Some(mut host) = game.lan.take() else {
        return;
    };
    let conn = host.hub.guests.iter().find(|guest| guest.uuid == player).map(|guest| guest.conn);
    if let Some(conn) = conn {
        game.as_guest(&mut host, conn, |game| {
            let Some(mut stack) = (match slot {
                radial::Slot::MainHand => game.held().cloned(),
                radial::Slot::OffHand => game.offhand.clone(),
            }) else {
                return;
            };
            if !has_radial(&stack) {
                return;
            }
            let mode = ConfiguratorMode::VALUES[(i64::from(representation)).rem_euclid(ConfiguratorMode::VALUES.len() as i64) as usize];
            set_mode(game, &mut stack, mode);
            match slot {
                radial::Slot::MainHand => *game.held_mut() = Some(stack),
                radial::Slot::OffHand => game.offhand = Some(stack),
            }
        });
    }
    game.lan = Some(host);
}

/// `GuiRadialSelector.extractRenderState`, no gancho de pós-desenho da tela do mod.
pub(in crate::game) fn screen_render_post(game: &Game<'_>, batch: &mut HudBatch) {
    if game.native_mod_screen() != Some(MOD_ID) {
        return;
    }
    let mut effects = effects();
    let Some(ui) = effects.radial.as_mut() else {
        return;
    };
    let Some(stack) = radial_stack(game, ui.slot) else {
        return;
    };
    let current = mode_of(game, stack);
    let center = [batch.width / 2.0, batch.height / 2.0];
    let cursor = game.native_cursor();
    let frame = radial::frame([cursor[0] as i32, cursor[1] as i32], center, ConfiguratorMode::VALUES.len(), Some(current.ordinal()), Some(color_channels(current.color())));
    ui.selection = frame.selection;
    drop(effects);
    let atlas = &game.gui.atlas;
    for torus in &frame.toruses {
        let channel = |value: f32| (value * 255.0).floor() as u8;
        let color = horizon_client::hud::rgba(channel(torus.rgba[0]), channel(torus.rgba[1]), channel(torus.rgba[2]), channel(torus.rgba[3]));
        let pairs = radial::torus_pairs(center, torus);
        for pair in pairs.windows(2) {
            let ((o0, i0), (o1, i1)) = (pair[0], pair[1]);
            batch.fill_quad(atlas, [o0, i0, i1, o1], color);
        }
    }
    for (mode, slice) in ConfiguratorMode::VALUES.iter().zip(&frame.slices) {
        batch.sprite_scaled(
            atlas,
            &mode.radial_icon(),
            center[0] + slice.icon[0] as f32,
            center[1] + slice.icon[1] as f32,
            radial::ICON_SIZE as f32,
            radial::ICON_SIZE as f32,
            u32::MAX,
        );
    }
    for (mode, slice) in ConfiguratorMode::VALUES.iter().zip(&frame.slices) {
        let mut name = mode.slice_name();
        if glue::effects_config().white_radial_text {
            name.color = None;
        }
        let label = text(&name);
        let (pivot_x, pivot_y) = (center[0] + slice.label[0], center[1] + slice.label[1]);
        let start = batch.vertices.len();
        let left = (-(atlas.font.width_text(&label) as f32) / 2.0) as i32;
        batch.text_styled(atlas, &label, pivot_x + left as f32, pivot_y + radial::LABEL_DOWN as f32, hud_color(radial::LABEL_ARGB), true);
        batch.scale_since(start, pivot_x, pivot_y, radial::LABEL_SCALE, radial::LABEL_SCALE);
    }
}

/// Se a tela aberta é o menu radial.
fn radial_open(game: &Game<'_>) -> bool {
    game.native_mod_screen() == Some(MOD_ID) && effects().radial.is_some()
}

/// `GuiRadialSelector.keyPressed`: o menu consome toda tecla.
pub(in crate::game) fn screen_key_pressed(game: &Game<'_>, canceled: &mut bool) {
    if radial_open(game) {
        *canceled = true;
    }
}

/// `GuiRadialSelector.mouseClicked`: o clique escolhe o modo sob o mouse.
pub(in crate::game) fn screen_mouse_pressed(game: &Game<'_>, canceled: &mut bool) {
    if !radial_open(game) {
        return;
    }
    let mut effects = effects();
    if let Some(ui) = effects.radial.as_ref()
        && let Some(index) = ui.selection
    {
        let slot = ui.slot;
        effects.pending.push((slot, index));
    }
    *canceled = true;
}

/// `GuiRadialSelector.mouseScrolled`: a rolagem anda uma fatia a partir do modo atual e já o escolhe.
pub(in crate::game) fn screen_mouse_scrolled(game: &Game<'_>, scroll: [f64; 2], canceled: &mut bool) {
    if !radial_open(game) || scroll[1] == 0.0 {
        return;
    }
    let ticks = game.visual_ticks() as i64;
    let mut effects = effects();
    let Some(ui) = effects.radial.as_mut() else {
        return;
    };
    let shift = ui.scroll.scroll(scroll[1], ticks);
    let slot = ui.slot;
    let current = radial_stack(game, slot).map(|stack| mode_of(game, stack).ordinal());
    match radial::scrolled(ConfiguratorMode::VALUES.len(), current, shift) {
        radial::Scroll::Consumed => *canceled = true,
        radial::Scroll::Select(index) => {
            if let Some(ui) = effects.radial.as_mut() {
                ui.selection = Some(index);
            }
            effects.pending.push((slot, index));
            *canceled = true;
        }
        radial::Scroll::Passed => {}
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// A sobreposição da face (`RenderTickHandler.onBlockHover` + `ConfiguratorOverlayHandler`).

/// O que a sobreposição de um quadro precisa: o bloco, a face, o sprite, a cor e a rotação da câmera.
pub(in crate::game) struct OverlayRequest {
    pub pos: [i32; 3],
    pub face: Face,
    pub sprite: String,
    pub argb: u32,
    pub y_rot: f64,
}

/// `RenderTickHandler.onBlockHover`: com o Configurator na mão principal (ou na secundária), num modo de configurar, olhando para uma máquina que aceita o tipo e tem
/// o lado ligado. Guarda o bloco para o contorno padrão sair.
pub(in crate::game) fn overlay_request(game: &Game<'_>) -> Option<OverlayRequest> {
    let found = overlay_find(game);
    effects().overlay = found.as_ref().map(|request| request.pos);
    found
}

fn overlay_find(game: &Game<'_>) -> Option<OverlayRequest> {
    use horizon_client::command::args::GameMode;
    let stack = main_hand(game).filter(|stack| &*stack.item == rules::CONFIGURATOR_ITEM).or_else(|| game.offhand.as_ref().filter(|stack| &*stack.item == rules::CONFIGURATOR_ITEM))?;
    // As portas do contorno do bloco: sem F1, e o jogador pode construir (ou está em aventura).
    if game.ui.hide_gui || !(game.may_build() || game.mode == GameMode::Adventure) {
        return None;
    }
    let mode = mode_of(game, stack);
    let hit = game.pick()?;
    let config = glue::peek_side_config(game, hit.pos)?;
    let face = face_of(hit.face);
    let (kind, data_type) = rules::configurator_overlay(mode, &config, basis::front(game, hit.pos), face)?;
    Some(OverlayRequest { pos: hit.pos, face, sprite: rules::overlay_sprite(kind), argb: rules::overlay_argb(data_type), y_rot: f64::from(game.yaw) })
}

/// Os vértices da sobreposição, relativos à câmera, no formato dos primitivos do mundo: um quadrilátero do sprite, com a cor do tipo de dado.
pub(in crate::game) fn overlay_vertices(game: &Game<'_>, request: &OverlayRequest, camera: [f64; 3]) -> Option<[[f32; 3]; 4]> {
    let _ = game.gui.atlas.sprites.get(request.sprite.as_str())?;
    let corners = face_overlay::vertices(request.face, request.y_rot);
    Some(std::array::from_fn(|index| {
        let position = corners[index].position;
        [
            (f64::from(request.pos[0]) - camera[0]) as f32 + position[0],
            (f64::from(request.pos[1]) - camera[1]) as f32 + position[1],
            (f64::from(request.pos[2]) - camera[2]) as f32 + position[2],
        ]
    }))
}

/// `RenderPipelines.EYES`: o contorno padrão não sai enquanto a sobreposição está na tela.
pub(in crate::game) fn block_highlight(pos: [i32; 3], canceled: &mut bool) {
    if effects().overlay == Some(pos) {
        *canceled = true;
    }
}
