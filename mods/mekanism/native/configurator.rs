// O Configurator (`ItemConfigurator`) e o cartão de configuração (`ItemConfigurationCard`) no jogo. As regras (os modos, as mensagens, o que o clique faz
// numa face) são de `mekanism_rules` e da `machinery_basis`; aqui fica a cola: ler e escrever o componente da pilha, achar a máquina, e o que o jogo
// expõe (o clique, o bloco, a rede, o teclado). Cada função cita o Java.
use crate::game::mekanism_glue as glue;
use crate::game::mekanism_rules as rules;
use crate::game::mekanism_rules::{ConfiguratorMode, DisplayChange};
use crate::game::{Game, machinery_basis as basis};
use horizon_client::containers::BlockEntity;
use horizon_client::first_person::Hand;
use horizon_core::random::RandomSource;
use horizon_client::items::ItemStack;
use horizon_client::mod_api::Text;
use horizon_world::nbt::{Compound, JavaString, Tag};
use machinery_basis_core::side_config::SideClick;
use machinery_basis_core::sides::Face;

/// O Heat Generator não é uma máquina de progresso daqui, mas é um `TileEntityMekanism` direcional, com inventário e com o cartão (só o modo de
/// redstone). O estado dele vive no NBT do bloco (o gerador é refeito dele a cada tick).
const HEAT_GENERATOR: &str = "mekanismgenerators:heat_generator";
const HEAT_SLOTS: [usize; 2] = [0, 1];
const CONTROL_TYPE: &str = "control_type";

/// O canal do `PacketModeChange` (cliente para servidor) e o do `showModeChange` (servidor para cliente).
const MODE_CHANNEL: &str = "mekanism:mode";
const SHOW_CHANNEL: &str = "mekanism:show_mode_change";
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
    glue::is_machine_block(block) || block == HEAT_GENERATOR
}

/// `TileEntityMekanism.hasInventory` e os slots de `getInventorySlots`.
fn inventory_slots(block: &str) -> Vec<usize> {
    if block == HEAT_GENERATOR { HEAT_SLOTS.to_vec() } else { glue::inventory_slots(block) }
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

/// Se a pilha é um item de modo na mão principal (`IModeItem.isModeItem`).
fn is_mode_item(stack: &ItemStack) -> bool {
    &*stack.item == rules::CONFIGURATOR_ITEM
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

/// `MekanismKeyHandler.handModeSwitchKey`: a tecla troca o modo (para trás com o jogador agachado) e mostra o texto.
pub(in crate::game) fn key(game: &mut Game<'_>, key: &str, pressed: bool) {
    if !pressed || !game.native_playing() || key != game.native_mod_key("mekanism", MODE_KEY_NAME, MODE_KEY_DEFAULT) {
        return;
    }
    if !game.held().is_some_and(is_mode_item) {
        return;
    }
    let shift = if game.native_sneaking() { -1 } else { 1 };
    send_mode(game, shift, DisplayChange::MainHand);
}

thread_local! {
    /// `ScrollIncrementer`: o quanto da rolagem ainda não virou um passo, e o tique do último passo.
    static SCROLL: std::cell::Cell<(f64, i64)> = const { std::cell::Cell::new((0.0, -1)) };
}

/// `ClientTickHandler.onMouseEvent`: agachado, a rolagem com um item de modo na mão troca o modo (`allowModeScroll`, padrão ligado) em vez de trocar o
/// slot da barra.
pub(in crate::game) fn scroll(game: &mut Game<'_>, delta: [f64; 2], canceled: &mut bool) {
    if !game.native_sneaking() || delta[1] == 0.0 || !game.held().is_some_and(is_mode_item) {
        return;
    }
    let tick = game.visual_ticks() as i64;
    let (mut sum, last) = SCROLL.get();
    if last >= 0 && tick - last > 20 {
        sum = 0.0;
    }
    sum += delta[1];
    let whole = sum.trunc();
    sum -= whole;
    SCROLL.set((sum, tick));
    let shift = (whole as i32).clamp(-1, 1);
    if shift != 0 {
        send_mode(game, shift, DisplayChange::None);
        mode_timer_start();
    }
    *canceled = true;
}

/// `PacketModeChange.handle`: o servidor troca o modo da pilha na mão do jogador, e conforme o `DisplayChange` avisa.
pub(in crate::game) fn net_from_client(game: &mut Game<'_>, player: u128, channel: &str, data: &[u8]) {
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
    if !is_mode_item(&stack) {
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

/// `MekanismStatusOverlay.BASE_TIMER`: cinco segundos de tiques.
const MODE_TIMER_TICKS: i32 = 100;

static MODE_TIMER: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

/// `MekanismStatusOverlay.setTimer`.
fn mode_timer_start() {
    MODE_TIMER.store(MODE_TIMER_TICKS, std::sync::atomic::Ordering::Relaxed);
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
