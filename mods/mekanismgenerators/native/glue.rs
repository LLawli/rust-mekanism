// O Heat Generator no jogo. As regras ficam em mekanismgenerators_rules, e o que é genérico em
// machinery_basis.
use crate::game::mekanismgenerators_rules as rules;
use crate::game::machinery_armor_bar::declare as declare_armor_bar;
use crate::game::{Game, machinery_basis as basis};
use horizon_client::containers::{BlockEntity, ModBlockEntity, ModContainer, ModEnergyBuffer};
use horizon_client::hud::HudBatch;
use horizon_client::items::ItemStack;
use horizon_client::lang;
use horizon_client::mod_api::{NativeScreen, NativeScreenView};
use horizon_world::nbt::{Compound, JavaString, Tag};
use machinery_basis_core::armor_bar;
use machinery_basis_core::armor_slots::{self, EquipmentSlot, SlotKind};
use machinery_basis_core::data_slots;
use machinery_basis_core::gui;
use machinery_basis_core::fluid_logged::{EMPTY_BUCKET, LAVA_BUCKET};
use machinery_basis_core::heat::Capacitor;
use machinery_basis_core::quick_move::{self, QuickMenu, Region};
use machinery_basis_core::slot_rules;
use machinery_basis_core::side_tab;
use machinery_basis_core::sides::{Face, FaceSlots, Sides};
use machinery_basis_core::slot_view::{SlotOverlay, SlotType, slot_type};
use machinery_basis_core::tank::{Action, Tank};
use machinery_basis_core::tile::{ItemEnergy, RedstoneControl, SavedState, TileBase, energy_can_drain};
use machinery_basis_core::upgrade::{self, UpgradeComponent, UpgradeTransient};
use machinery_basis_core::upgrade_window::{UpgradeSync, UpgradeWindowSpec};
use machinery_basis_core::window::SelectedWindow;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

const FUEL_SLOT: usize = rules::FUEL_SLOT;
const ENERGY_SLOT: usize = rules::ENERGY_SLOT;
const FUEL_SLOTS: &[usize] = &[FUEL_SLOT];
const LAVA_TANK: [u64; 1] = [rules::TANK_CAPACITY];
const NETHER: &str = "minecraft:the_nether";
const LAVA_BLOCK: &str = "minecraft:lava";
const HEAT_KEY: &str = "heat";
/// `SerializationConstants.ACTIVE_STATE` e `UPDATE_DELAY`: o `currentActive` e o `updateDelay` que `TileEntityMekanism` salva.
const ACTIVE_STATE: &str = "active_state";
/// `SerializationConstants.REDSTONE` e `CONTROL_TYPE`: o `redstone` que o último aviso dos vizinhos deixou (`TileEntityMekanism.java:775`) e o
/// modo de controle (`writeSustainedData`).
const REDSTONE: &str = "redstone";
const CONTROL_TYPE: &str = "control_type";
/// O nome com que o mod diz à `machinery_basis` quais blocos têm `redstone`.
const REDSTONE_OWNER: &str = "mekanismgenerators";
const UPDATE_DELAY: &str = "update_delay";
const LIT: &str = "lit";

// Os data slots da tela. Os 3 e 4 ficaram livres: eram a temperatura inteira, que a aba de calor lê agora como o calor exato (`DATA_HEAT`).
// Um `double` ocupa quatro slots de 16 bits.
const DATA_ENERGY: usize = 0;
const DATA_LAVA: usize = 2;
const DATA_PRODUCTION: usize = 5;
/// `SyncableDouble` de `lastTransferLoss` e de `lastEnvironmentLoss`, nesta ordem (`addContainerTrackers`).
const DATA_TRANSFER_LOSS: usize = 6;
const DATA_ENVIRONMENT_LOSS: usize = 10;
/// O calor do capacitor (`BasicHeatCapacitor.getHeat`, que `TileEntityMekanism.addContainerTrackers` sincroniza): a aba mostra o calor
/// sobre a capacidade, a temperatura do tile.
const DATA_HEAT: usize = rules::DATA_HEAT;
const DATA_SLOTS: usize = rules::DATA_SLOTS;

// A tela, nas posições de GuiHeatGenerator. O Mekanism Generators usa os sprites do Mekanism, como o original.
const WIDTH: f32 = 176.0;
const HEIGHT: f32 = 166.0;
const SLOT: &str = "mekanism:gui/sprites/slot/normal";
const PANEL: basis::PanelStyle = basis::PanelStyle {
    base: "mekanism:gui/sprites/base",
    base_slice: rules::PANEL_SLICE,
    slot: SLOT,
};
const BAR_STYLE: basis::BarStyle = basis::BarStyle {
    frame: "mekanism:gui/sprites/bar/base",
    frame_slice: rules::BAR_FRAME_SLICE,
};
const POWER_BAR_SPRITE: &str = "mekanism:gui/sprites/bar/vertical_power";
/// `GaugeType.WIDE` com `GaugeInfo.STANDARD`: a moldura `gauge/normal` e o overlay `gauge/overlay/wide`.
const GAUGE_STYLE: basis::GaugeStyle = basis::GaugeStyle {
    frame: "mekanism:gui/sprites/gauge/normal",
    frame_slice: rules::GAUGE_FRAME_SLICE,
    overlay: "mekanism:gui/sprites/gauge/overlay/wide",
};
/// Os elementos do tamanho do `GuiElement`: o medidor largo é 64 por 48 mais a moldura, e a barra soma dois ao que o Java passa.
const LAVA_GAUGE: [f32; 4] = [55.0, 18.0, 66.0, 50.0];
/// `Tooltip.MAX_WIDTH`: onde `Tooltip.create` quebra o texto.
const TOOLTIP_WRAP: i32 = 170;
const ENERGY_BAR: [f32; 4] = [164.0, 15.0, 6.0, 54.0];
/// `MekanismRenderer.color(lava)`: a lava não tem cor de fluido, e o sprite sai como é.
const LAVA_TINT: u32 = 0xFF_FF_FF_FF;

/// O sprite da moldura de um slot (`SlotType.texture`). Só os tipos que esta tela usa têm arquivo.
fn slot_sprite(slot: SlotType) -> &'static str {
    match slot {
        SlotType::Power => "mekanism:gui/sprites/slot/power",
        _ => SLOT,
    }
}

/// O sprite que vai por cima da moldura (`SlotOverlay.texture`). Só o do slot de energia tem arquivo.
fn overlay_sprite(overlay: SlotOverlay) -> Option<&'static str> {
    (overlay == SlotOverlay::Power).then_some("mekanism:gui/sprites/slot/overlay/power")
}

fn is_generator(block: &str) -> bool {
    block == rules::HEAT_GENERATOR
}

/// O nome do tile é o da `machinery_basis` (`basis::name_*`): o componente inteiro, só para o bloco daqui.
fn is_generator_id(id: &str) -> bool {
    is_generator(id)
}

pub(in crate::game) fn place_block(game: &mut Game<'_>, stack: &ItemStack, pos: [i32; 3]) {
    basis::name_placed(game, stack, pos, &is_generator_id);
    if is_generator(&game.native_block(pos)) {
        // `BlockMekanism.setPlacedBy` -> `onAdded` -> `updatePower`: só o bloco posto com um item lê o sinal ao nascer.
        basis::on_added(game, pos);
    }
}

pub(in crate::game) fn loot_drops(game: &Game<'_>, context: &horizon_client::mod_api::NativeLootDrops<'_>, drops: &mut [ItemStack]) {
    basis::name_loot(game, context, drops, &is_generator_id);
}

pub(in crate::game) fn container_opened(game: &mut Game<'_>, player: u128, pos: Option<[i32; 3]>) {
    basis::name_container_opened(game, player, pos, &is_generator_id);
}

pub(in crate::game) fn destroy_block(game: &mut Game<'_>, pos: [i32; 3], canceled: bool) {
    if !canceled && is_generator(&game.native_block(pos)) {
        basis::name_destroying(game, pos, &is_generator_id);
    }
}

pub(in crate::game) fn block_entity(block: &str, entity: &mut Option<(String, Compound)>) {
    if is_generator(block) {
        // Os blocos do `TileEntityMekanism` com `supportsRedstone()`: o jogo os avisa quando um vizinho muda (`basis::neighbor_changed`).
        basis::watch_redstone(REDSTONE_OWNER, is_generator);
    }
    if is_generator(block) && entity.is_none() {
        *entity = Some((block.to_owned(), Compound::default()));
    }
}

pub(in crate::game) fn ticks(block: &str, ticks: &mut bool) {
    if is_generator(block) {
        *ticks = true;
    }
}

pub(in crate::game) fn energy(block: &str, energy: &mut Option<ModEnergyBuffer>) {
    if is_generator(block) {
        *energy = Some(ModEnergyBuffer {
            capacity: rules::ENERGY_CAPACITY,
            max_extract: rules::ENERGY_CAPACITY,
            extract_faces: [true; 6],
            ..Default::default()
        });
    }
}

pub(in crate::game) fn container(block: &str, container: &mut Option<ModContainer>) {
    if is_generator(block) {
        // Todo caminho que abre o menu (o servidor que monta o menu com os cinco slots a mais, o cliente que os põe na tela) passa por aqui antes.
        declare_armor_bar(rules::MENU, rules::armor_bar_declaration);
        machinery_basis_core::screen_elements::declare_once(rules::MENU, rules::screen_elements);
        // Sem interface vanilla: hoppers passam pela regra da machinery_basis.
        *container = Some(ModContainer {
            slots: rules::MENU_SLOTS,
            menu: Some(rules::MENU.to_owned()),
            ..Default::default()
        });
    }
}

pub(in crate::game) fn screen(block: &str, screen: &mut Option<NativeScreen>) {
    if is_generator(block) {
        declare_armor_bar(rules::MENU, rules::armor_bar_declaration);
        machinery_basis_core::screen_elements::declare_once(rules::MENU, rules::screen_elements);
        // Os slots virtuais da janela de upgrades vêm depois dos do gerador e ficam escondidos até a janela abrir.
        let mut slots: Vec<Option<[f32; 2]>> = rules::SLOTS_SPEC.iter().map(|slot| Some(slot.item)).collect();
        slots.extend([None, None]);
        Game::mb_window_register(&WINDOW_SPEC, remove_upgrade, add_upgrades);
        *screen = Some(NativeScreen {
            width: WIDTH,
            height: HEIGHT,
            slots,
            inventory_y: 84.0,
            inventory_x: 8.0,
            own_labels: true,
            own_background: true,
        });
    }
}

fn burn_time(game: &Game<'_>, stack: &ItemStack) -> i32 {
    game.res.providers().burn_time(world_items!(game), stack, false)
}

fn is_fuel(game: &Game<'_>, stack: &ItemStack) -> bool {
    burn_time(game, stack) > 0
}

fn lava_bucket_burn_time(game: &Game<'_>) -> i32 {
    let bucket = ItemStack {
        item: LAVA_BUCKET.into(),
        count: 1,
        damage: 0,
        extra: None,
    };
    burn_time(game, &bucket)
}

/// `FluidFuelInventorySlot.fillOrBurn`: um balde enche o tanque, e o resto queima em lava.
fn fill_or_burn(game: &Game<'_>, entity: &mut ModBlockEntity, tank: &mut Tank) {
    let Some(stack) = entity.items[FUEL_SLOT].clone() else {
        return;
    };
    if &*stack.item == LAVA_BUCKET {
        if let Some(lava) = rules::fuel_to_tank(rules::FuelItem::LavaBucket, tank.room()) {
            tank.insert(rules::LAVA, lava, Action::Execute);
            let mut empty = stack;
            empty.item = EMPTY_BUCKET.into();
            entity.items[FUEL_SLOT] = Some(empty);
        }
        return;
    }
    let solid = rules::FuelItem::Solid {
        burn_time: burn_time(game, &stack),
        lava_bucket_burn_time: lava_bucket_burn_time(game),
    };
    if let Some(lava) = rules::fuel_to_tank(solid, tank.room()) {
        tank.insert(rules::LAVA, lava, Action::Execute);
        let mut left = stack;
        left.count -= 1;
        entity.items[FUEL_SLOT] = (left.count > 0).then_some(left);
    }
}

fn lava_faces(game: &Game<'_>, pos: [i32; 3]) -> u32 {
    Face::ALL
        .iter()
        .filter(|face| {
            let [x, y, z] = face.offset();
            game.native_block([pos[0] + x, pos[1] + y, pos[2] + z]) == LAVA_BLOCK
        })
        .count() as u32
}

/// O cliente só tem a block entity, e o que ela desenha, se o servidor mandar uma cópia.
pub(in crate::game) fn update_tag(entity: &ModBlockEntity, tag: &mut Option<Compound>) {
    if is_generator(&entity.id) {
        *tag = Some(Compound::default());
    }
}

/// Combustível entra por todas as faces menos a direita, e o balde vazio sai por baixo.
fn sides(front: Face) -> Sides {
    let mut sides = Sides {
        input: [true; 6],
        output: [false; 6],
    };
    sides.input[front.right_of() as usize] = false;
    sides.output[Face::Down as usize] = true;
    sides
}

fn load_heat(entity: &ModBlockEntity, ambient: f64) -> Capacitor {
    let mut capacitor = rules::new_capacitor(ambient);
    if let Some(Tag::Double(bits)) = entity.nbt.get(&JavaString::from(HEAT_KEY)) {
        capacitor.heat = f64::from_bits(*bits);
    }
    capacitor
}

/// O que `TileEntityMekanism.loadAdditional` lê (`redstone`, o modo de controle, `currentActive` e `updateDelay`), com o estado `lit` que o
/// bloco tem no mundo.
fn load_tile(entity: &ModBlockEntity, block_active: bool) -> TileBase {
    let redstone = matches!(entity.nbt.get(&JavaString::from(REDSTONE)), Some(Tag::Byte(value)) if *value != 0);
    let control_type = match entity.nbt.get(&JavaString::from(CONTROL_TYPE)) {
        Some(Tag::String(name)) => RedstoneControl::from_serialized_name(&name.to_string_lossy()),
        _ => RedstoneControl::Disabled,
    };
    let active = matches!(entity.nbt.get(&JavaString::from(ACTIVE_STATE)), Some(Tag::Byte(value)) if *value != 0);
    let update_delay = match entity.nbt.get(&JavaString::from(UPDATE_DELAY)) {
        Some(Tag::Int(value)) => *value,
        _ => 0,
    };
    let mut tile = TileBase::new(rules::BLOCK_DEACTIVATION_DELAY);
    let saved = SavedState {
        redstone,
        control_type,
        active,
        update_delay,
        ..Default::default()
    };
    tile.restore(&saved, block_active);
    tile
}

/// `saveAdditional` do `TileEntityMekanism`: `redstone`, o modo de controle, `currentActive` e `updateDelay`.
fn save_tile(entity: &mut ModBlockEntity, tile: &TileBase) {
    entity.nbt.insert(JavaString::from(REDSTONE), Tag::Byte(i8::from(tile.is_powered())));
    entity.nbt.insert(JavaString::from(CONTROL_TYPE), Tag::String(JavaString::from(tile.control_type().serialized_name())));
    entity.nbt.insert(JavaString::from(ACTIVE_STATE), Tag::Byte(i8::from(tile.active())));
    entity.nbt.insert(JavaString::from(UPDATE_DELAY), Tag::Int(tile.update_delay()));
}

// ---------------------------------------------------------------------------------------------------------------------------
// Os upgrades (`TileComponentUpgrade`). O gerador não tem objeto vivo: o componente é refeito do NBT (`component_upgrade`) e dos dois
// slots virtuais do menu a cada tick, avança, e volta para os dois. O que o Java guarda só na memória (`upgradeTicks` e
// `canCheckUpgrades`) fica numa tabela por thread, uma entrada por gerador.

/// Quando o item não é conhecido, a pilha dele.
const UNKNOWN_STACK: u32 = 64;
/// De quanto em quanto tempo a tabela é varrida, e quanto sem andar uma entrada vive.
const SWEEP_TICKS: i64 = 200;
const STALE_TICKS: i64 = 100;

/// A janela de upgrades do gerador: só dados. A mecânica (aba, janela, slots virtuais, o item de upgrade na mão) é da machinery_basis.
static WINDOW_SPEC: UpgradeWindowSpec = UpgradeWindowSpec {
    block: rules::HEAT_GENERATOR,
    virtual_slots: [rules::UPGRADE_SLOT, rules::UPGRADE_OUTPUT_SLOT],
    tracked_base: rules::HEAT_TRACKED_BASE,
    tracking_key: rules::UPGRADE_TRACKING_KEY,
    tracked_items: rules::UPGRADE_TRACKED_ITEMS,
    sync_offset: rules::UPGRADE_DATA,
    sync_len: rules::UPGRADE_SYNC_LEN,
    ns: "mekanism",
    window_type: rules::UPGRADE_WINDOW,
    window_types: &rules::WINDOW_TYPES,
    base_slice: rules::PANEL_SLICE,
    max_multiplier: rules::UPGRADE_MULTIPLIER as i32,
    data: window_upgrade_data,
    color: rules::enum_color,
    item: rules::UPGRADE_ITEM,
    item_component: rules::UPGRADE_TYPE_COMPONENT,
};

fn window_upgrade_data() -> Rc<upgrade::UpgradeData> {
    rules::upgrade_data(&upgrade::current_types(), &upgrade::current_tags(), UNKNOWN_STACK)
}

/// Os dados de upgrade desta carga: os tipos, as tags e o limite de pilha do item.
fn upgrade_data(game: &Game<'_>) -> Rc<upgrade::UpgradeData> {
    let stack = game.native_max_stack_size(rules::UPGRADE_ITEM).unwrap_or(UNKNOWN_STACK);
    rules::upgrade_data(&upgrade::current_types(), &upgrade::current_tags(), stack)
}

/// O que o gerador guarda do componente só na memória.
struct UpgradeMemory {
    transient: UpgradeTransient,
    /// O último tick em que o gerador andou: um tick que não é o seguinte é um objeto novo no Java (o chunk saiu, o mundo foi reaberto).
    last_seen: i64,
    /// O que a janela de upgrades viu no tick anterior. O Java sincroniza o `upgradeTicks` e o mapa depois do tick do bloco, na batida
    /// seguinte da tela: o cliente mostra o valor de um tick atrás.
    sync_seen: Option<UpgradeSync>,
    /// Quantos upgrades de silêncio o cliente já sabe que o tile tem (`MUFFLING_COUNT` do pacote de atualização).
    muffling_sent: i32,
}

impl UpgradeMemory {
    fn new() -> Self {
        Self {
            transient: UpgradeTransient::default(),
            last_seen: i64::MIN,
            sync_seen: None,
            // Um tile novo avisa o cliente na primeira vez, mesmo com zero: o `getUpdateTag` do Java sempre leva a contagem.
            muffling_sent: -1,
        }
    }
}

thread_local! {
    static UPGRADES: RefCell<HashMap<(&'static str, [i32; 3]), UpgradeMemory>> = RefCell::new(HashMap::new());
    static LAST_SWEEP: Cell<i64> = const { Cell::new(i64::MIN) };
}

/// Um mundo antigo tem menos slots salvos que o menu de agora (os dois virtuais vêm depois dos do gerador).
fn ensure_slots(entity: &mut ModBlockEntity) {
    if entity.items.len() < rules::MENU_SLOTS {
        entity.items.resize(rules::MENU_SLOTS, None);
    }
}

/// Os dois slots virtuais como o menu os tem, a entrada e a saída.
fn menu_contents(game: &Game<'_>, entity: &ModBlockEntity) -> [machinery_basis_core::containers::SlotContents; 2] {
    [rules::UPGRADE_SLOT, rules::UPGRADE_OUTPUT_SLOT].map(|index| basis::slot_contents(game, entity.items.get(index).and_then(Option::as_ref)))
}

/// `TileComponentUpgrade.setUpgrades` → `sendUpdatePacket`: o cliente só quer saber quantos upgrades de silêncio há.
fn sync_muffling(game: &mut Game<'_>, dimension: &str, pos: [i32; 3], memory: &mut UpgradeMemory, component: &UpgradeComponent) {
    let count = component.get_upgrades(rules::MUFFLING_UPGRADE);
    if count != memory.muffling_sent {
        memory.muffling_sent = count;
        game.mb_muffling_changed(dimension, pos, count);
    }
}

/// O primeiro passo do tick do tile (`TileEntityMekanism.tickServer`: o componente anda antes de tudo). Devolve o que a janela de upgrades
/// mostra neste tick.
fn tick_upgrades(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> UpgradeSync {
    ensure_slots(entity);
    let now = game.world.game_time;
    let dimension = game.native_dimension();
    let key = (dimension, pos);
    // Um bloco novo na mesma posição não herda nada: sem a chave `active_state` (o Java a grava sempre), nasce outra entrada.
    let saved_before = entity.nbt.contains_key(&JavaString::from(ACTIVE_STATE));
    let remembered = UPGRADES
        .with_borrow_mut(|tiles| tiles.remove(&key))
        .filter(|memory| memory.last_seen + 1 == now && saved_before);
    // No tick em que o objeto nasce o NBT do componente manda; depois, o que o menu tem nos dois slots virtuais.
    let fresh = remembered.is_none();
    let mut memory = remembered.unwrap_or_else(UpgradeMemory::new);
    let nbt = basis::read_upgrades(game, entity);
    let menu = (!fresh).then(|| menu_contents(game, entity));
    let component = upgrade::rebuild(upgrade_data(game), &nbt, menu.as_ref(), memory.transient);
    component.tick_server();
    let saved = component.saved();
    if saved != nbt {
        basis::write_upgrades(entity, &saved);
    }
    basis::upgrade_to_cell(game, rules::UPGRADE_TYPE_COMPONENT, &mut entity.items[rules::UPGRADE_SLOT], &saved.input);
    basis::upgrade_to_cell(game, rules::UPGRADE_TYPE_COMPONENT, &mut entity.items[rules::UPGRADE_OUTPUT_SLOT], &saved.output);
    memory.transient = component.transient();
    sync_muffling(game, dimension, pos, &mut memory, &component);
    let sync = UpgradeSync::of(&component);
    let shown = memory.sync_seen.replace(sync.clone()).unwrap_or(sync);
    memory.last_seen = now;
    UPGRADES.with_borrow_mut(|tiles| {
        tiles.insert(key, memory);
        if now.saturating_sub(LAST_SWEEP.get()) >= SWEEP_TICKS {
            LAST_SWEEP.set(now);
            tiles.retain(|_, memory| now - memory.last_seen <= STALE_TICKS);
        }
    });
    shown
}

/// Faz `act` no componente de upgrades do gerador em `pos` fora do tick (o pacote de remoção, o upgrade na mão), e guarda o que ele
/// deixou no NBT, nos dois slots do menu e na memória. O menu manda sobre o NBT: o tick seguinte o lê primeiro.
fn act_upgrades<R>(game: &mut Game<'_>, pos: [i32; 3], act: impl FnOnce(&UpgradeComponent) -> R) -> Option<R> {
    let dimension = game.native_dimension();
    let key = (dimension, pos);
    let at = (pos[0], pos[1], pos[2]);
    let (nbt, menu) = match game.world.block_entities.get(&at) {
        Some(BlockEntity::Mod(entity)) if is_generator(&entity.id) => (basis::read_upgrades(game, entity), menu_contents(game, entity)),
        _ => return None,
    };
    let mut memory = UPGRADES.with_borrow_mut(|tiles| tiles.remove(&key));
    let transient = memory.as_ref().map_or_else(UpgradeTransient::default, |memory| memory.transient);
    let component = upgrade::rebuild(upgrade_data(game), &nbt, Some(&menu), transient);
    let result = act(&component);
    let saved = component.saved();
    // As células novas dos dois slots, calculadas com o jogo ainda emprestado só para leitura.
    let cells = match game.world.block_entities.get(&at) {
        Some(BlockEntity::Mod(entity)) => [(rules::UPGRADE_SLOT, &saved.input), (rules::UPGRADE_OUTPUT_SLOT, &saved.output)].map(|(index, contents)| {
            let cell = entity.items.get(index).and_then(Option::as_ref);
            basis::upgrade_cell_after(game, rules::UPGRADE_TYPE_COMPONENT, cell, contents)
        }),
        _ => [None, None],
    };
    if let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&at) {
        ensure_slots(entity);
        if saved != nbt {
            basis::write_upgrades(entity, &saved);
        }
        for (index, cell) in [rules::UPGRADE_SLOT, rules::UPGRADE_OUTPUT_SLOT].into_iter().zip(cells) {
            if let Some(cell) = cell {
                entity.items[index] = cell;
            }
        }
    }
    game.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
    if let Some(memory) = memory.as_mut() {
        memory.transient = component.transient();
        sync_muffling(game, dimension, pos, memory, &component);
    }
    if let Some(memory) = memory {
        UPGRADES.with_borrow_mut(|tiles| tiles.insert(key, memory));
    }
    Some(result)
}

/// `PacketRemoveUpgrade.handle`: o servidor tira um ou todos os upgrades do tipo do gerador em `pos`.
fn remove_upgrade(game: &mut Game<'_>, pos: [i32; 3], upgrade: &str, all: bool) {
    act_upgrades(game, pos, |component| component.remove_upgrade(upgrade, all));
}

/// `TileComponentUpgrade.addUpgrades`, que `ItemUpgrade.useOn` (na `machinery_basis`) chama depois de conferir o suporte e o shift:
/// quantos upgrades do tipo entraram no gerador em `pos`, de até `available`.
fn add_upgrades(game: &mut Game<'_>, pos: [i32; 3], upgrade: &str, available: i32) -> i32 {
    act_upgrades(game, pos, |component| component.add_upgrades(upgrade, available)).unwrap_or(0)
}

pub(in crate::game) fn tick(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    if !is_generator(&entity.id) {
        return;
    }
    basis::name_apply_pending(entity, pos);
    // `TileEntityMekanism.tickServer`: o componente de upgrades anda antes do atraso do estado ativo e do `onUpdateServer`.
    let upgrade_sync = tick_upgrades(game, pos, entity);
    let mut tanks = basis::load_tanks(entity, &LAVA_TANK);
    let tank = &mut tanks[0];
    fill_or_burn(game, entity, tank);

    let ambient = basis::ambient_temperature(game, pos);
    let around = rules::Surroundings {
        lava_faces: lava_faces(game, pos) + u32::from(basis::has_logged_lava(game, pos)),
        in_nether: game.native_dimension() == NETHER,
        ambient,
        // `CachedAmbientTemperature.getTemperature(side)`: o ar do bloco vizinho, face por face.
        face_ambient: Face::ALL.map(|face| {
            let [x, y, z] = face.offset();
            basis::ambient_temperature(game, [pos[0] + x, pos[1] + y, pos[2] + z])
        }),
    };
    let block_active = game.native_block_property(pos, LIT).as_deref() == Some("true");
    let mut state = rules::State {
        heat: load_heat(entity, ambient),
        lava: tank.amount(),
        energy: basis::stored_energy(entity),
        tile: load_tile(entity, block_active),
    };
    // `updatePower` só roda quando o jogo avisa o bloco (`neighborChanged`, `onNeighborChange`, `onAdded`): o `redstone` não é lido a cada
    // tick. O que o aviso deixou chega aqui, antes de `canFunction()`.
    if let Some(powered) = basis::take_power(game, pos) {
        state.tile.update_power(powered);
    }
    let produced = rules::step(&mut state, &around);
    if basis::MEDIR {
        let fuel = entity.items[FUEL_SLOT].as_ref().map_or(("", 0), |stack| {
            (stack.item.split_once(':').map_or(&*stack.item, |(_, path)| path), stack.count.max(0) as u64)
        });
        basis::trace(&rules::trace_line(game.world.game_time, &state, &around, &produced, fuel));
    }
    tank.extract(produced.lava_used, Action::Execute);
    basis::set_stored_energy(entity, state.energy);
    let heat = state.heat;
    entity.nbt.insert(JavaString::from(HEAT_KEY), Tag::Double(heat.heat.to_bits()));
    save_tile(entity, &state.tile);

    let front = basis::front(game, pos);
    let sides = sides(front);
    basis::distribute_energy(game, pos, entity);
    // Só o balde vazio sai: combustível fica no slot.
    let empty_bucket = entity.items[FUEL_SLOT]
        .as_ref()
        .is_some_and(|stack| &*stack.item == EMPTY_BUCKET);
    let slots = FaceSlots {
        inputs: FUEL_SLOTS,
        outputs: if empty_bucket { FUEL_SLOTS } else { &[] },
    };
    let accepts = |game: &Game<'_>, _: usize, stack: &ItemStack| is_fuel(game, stack);
    basis::tick_hoppers(game, pos, entity, &sides, &|_| slots, &accepts, &|_, _, _| true);
    // O bloco só muda quando o tile manda: o atraso de `blockDeactivationDelay` o segura aceso depois que o gerador para.
    let lit = state.tile.block_active();
    if lit != block_active {
        game.native_set_block_property(pos, LIT, if lit { "true" } else { "false" });
    }

    let mut data = vec![0; DATA_SLOTS];
    let energy = data_slots::split(basis::stored_energy(entity) as u32);
    data[DATA_ENERGY] = energy[0];
    data[DATA_ENERGY + 1] = energy[1];
    data[DATA_LAVA] = tank.amount() as i32;
    data[DATA_PRODUCTION] = produced.energy;
    data[DATA_TRANSFER_LOSS..DATA_TRANSFER_LOSS + 4].copy_from_slice(&data_slots::split_f64(produced.transfer_loss));
    data[DATA_ENVIRONMENT_LOSS..DATA_ENVIRONMENT_LOSS + 4].copy_from_slice(&data_slots::split_f64(produced.environment_loss));
    data[DATA_HEAT..DATA_HEAT + 4].copy_from_slice(&data_slots::split_f64(heat.heat));
    // O Java só sincroniza `upgradeTicks` e o mapa com a janela aberta, por jogador; aqui os data slots são do bloco, então eles só levam o
    // valor enquanto alguém acompanha, e ficam em zero (sem tráfego) no resto do tempo.
    if Game::mb_window_tracking(pos) {
        rules::write_upgrade_sync(&mut data, &upgrade_sync);
    }
    entity.data = data;
    basis::save_tanks(entity, &tanks);
}

pub(in crate::game) fn screen_draw(
    game: &Game<'_>,
    block: &str,
    data: &[i32],
    batch: &mut HudBatch,
    origin: [f32; 2],
) {
    if !is_generator(block) {
        return;
    }
    let [x, y] = origin;
    let at = |rect: [f32; 4]| [x + rect[0], y + rect[1], rect[2], rect[3]];
    let get = |i: usize| data.get(i).copied().unwrap_or(0);

    let title = basis::screen_title(game, "container.mekanismgenerators.heat_generator");
    basis::machine_panel(game, batch, &PANEL, [x, y, WIDTH, HEIGHT], &title);
    draw_armor_bar(game, batch, origin);
    for (index, spec) in rules::SLOTS_SPEC.iter().enumerate() {
        let frame = slot_type(None, index, spec.container).map(slot_sprite);
        basis::slot(game, batch, origin, spec.item, frame, spec.overlay.and_then(overlay_sprite));
    }
    // `GuiMekanismTile.addGenericTabs`: a aba de upgrades vem de `super.addGuiElements()`, antes do medidor e da barra.
    game.mb_window_draw_tab(batch, block, origin);
    // A ordem dos `addRenderableWidget` de `GuiHeatGenerator`: o medidor de lava e depois a barra de energia.
    let lava = get(DATA_LAVA).max(0) as u64;
    let scaled = gui::gauge_level(lava, rules::TANK_CAPACITY, LAVA_GAUGE[3] as i32);
    let fill = (lava > 0).then_some((rules::LAVA_SPRITE, rules::LAVA_FRAME, LAVA_TINT));
    basis::gauge(game, batch, &GAUGE_STYLE, at(LAVA_GAUGE), scaled, fill);
    let energy = data_slots::join(get(DATA_ENERGY), get(DATA_ENERGY + 1));
    let level = gui::divide_to_level(f64::from(energy), f64::from(rules::ENERGY_CAPACITY));
    basis::vertical_bar(game, batch, &BAR_STYLE, at(ENERGY_BAR), level, None, &|game, batch, dst| {
        basis::power_bar_contents(game, batch, POWER_BAR_SPRITE, rules::POWER_BAR_TILE, dst);
    });
    // `GuiEnergyTab`, antes do medidor e da barra na ordem do Java; não se sobrepõe a nada, então a ordem não aparece.
    let [tab_x, tab_y, tab_w, tab_h] = rules::ENERGY_TAB.rect;
    game.native_sprite(batch, rules::ENERGY_TAB_SPRITE, [0.0, 0.0, tab_w, tab_h], [x + tab_x, y + tab_y, tab_w, tab_h]);
    // `GuiHeatTab`, o último `addRenderableWidget`: o sprite muda com a unidade de temperatura do cliente (`getResource`).
    let sprite = rules::heat_tab_sprite(basis::temperature_unit(rules::LOCAL_CONFIG_NAMESPACE));
    let [tab_x, tab_y, tab_w, tab_h] = rules::HEAT_TAB.rect;
    game.native_sprite(batch, &sprite, [0.0, 0.0, tab_w, tab_h], [x + tab_x, y + tab_y, tab_w, tab_h]);
}

/// `GuiHeatGenerator.addGuiElements`: o suporte (`GuiSideHolder`) vem antes de `super.addGuiElements()`, que põe os `GuiSlot` de todos os slots do
/// menu, os cinco da armadura e da mão entre eles (`SlotType.NORMAL`, um pixel antes do item). Os itens e os fundos vazios o jogo desenha depois.
fn draw_armor_bar(game: &Game<'_>, batch: &mut HudBatch, origin: [f32; 2]) {
    let [x, y] = origin;
    let [holder_x, holder_y, width, height] = rules::ARMOR_HOLDER;
    for cell in gui::nine_slice_cells(side_tab::HOLDER_SLICE, [x + holder_x, y + holder_y, width, height]) {
        basis::sprite_tinted(game, batch, rules::ARMOR_HOLDER_SPRITE, cell.src, cell.dst, rules::ARMOR_HOLDER_TINT);
    }
    for slot in armor_bar::slots(rules::MENU) {
        basis::slot(game, batch, origin, [slot.x as f32, slot.y as f32], Some(SLOT), None);
    }
}

/// O clique na aba de calor (`GuiHeatTab.onClick`), antes de a tela fazer qualquer coisa com ele (hook
/// `screen_mouse_button_pressed_pre`): o esquerdo avança a unidade de temperatura do cliente e o direito volta. A aba toma o clique
/// (`canceled`): não vai ao slot, ao item no cursor nem ao botão, e não toca som nem manda nada ao servidor, porque `GuiHeatTab` não
/// tem `clickSound` (só `GuiInsetElement` tem) e a unidade é do cliente. Uma janela por cima do ponto fica com o clique.
pub(in crate::game) fn heat_tab_press(game: &Game<'_>, screen: &NativeScreenView<'_>, button: i32, canceled: &mut bool) {
    if screen.menu != Some(rules::MENU) {
        return;
    }
    let at = [screen.mouse[0] - screen.rect[0], screen.mouse[1] - screen.rect[1]];
    let unit = basis::temperature_unit(rules::LOCAL_CONFIG_NAMESPACE);
    if let Some(updated) = rules::heat_tab_click(unit, at, button, game.mb_window_covers(at)) {
        basis::set_temperature_unit(rules::LOCAL_CONFIG_NAMESPACE, updated);
        *canceled = true;
    }
}

/// `Tooltip.create` de uma lista de linhas: cada uma quebrada em 170 pixels, sem cor própria.
fn wrapped(game: &Game<'_>, lines: &[String]) -> Vec<(String, Option<u32>)> {
    lines
        .iter()
        .flat_map(|line| machinery_basis_core::upgrade_window::wrap_lines(line, TOOLTIP_WRAP, &|piece: &str| game.native_text_width(piece)))
        .map(|line| (line, None))
        .collect()
}

/// Os tooltips dos widgets da tela (`GuiElement.renderToolTip`): a barra de energia (`GuiVerticalPowerBar`) e o medidor de lava
/// (`GuiFluidGauge`). Um tooltip de widget fica por cima ou por baixo dele, não sobre o cursor como o de `native_screen_tooltip`, então
/// sai daqui, pela injeção em `draw_native_screen_tooltip`. Devolve se desenhou. `at` é o cursor relativo ao canto da tela, e `cursor`
/// o mesmo em pixels de GUI da janela.
pub(in crate::game) fn widget_tooltip(
    game: &Game<'_>,
    batch: &mut HudBatch,
    block: &str,
    data: &[i32],
    at: [f32; 2],
    cursor: (f32, f32),
) -> bool {
    if !is_generator(block) {
        return false;
    }
    // A janela de upgrades, quando aberta, e a aba ficam por cima do resto da tela.
    if game.mb_window_tooltip(batch, block, at, cursor) {
        return true;
    }
    let get = |i: usize| data.get(i).copied().unwrap_or(0);
    let translate = |key: &str| lang::get(key);
    // `GuiEnergyTab.updateTooltip`: `Tooltip.create` de uma lista de linhas, que quebra cada uma em 170 pixels.
    if rules::ENERGY_TAB.is_mouse_over(at, false) {
        let production = get(DATA_PRODUCTION).max(0) as u64;
        let lines = wrapped(game, &rules::energy_tab_tooltip(production, &translate));
        let [x, y, w, h] = rules::ENERGY_TAB.rect;
        basis::menu_tooltip_flat(game, batch, &lines, cursor, [cursor.0 - at[0] + x, cursor.1 - at[1] + y, w, h]);
        return true;
    }
    // `GuiHeatTab.updateTooltip`: as três linhas do `GuiHeatGenerator` na unidade do cliente, e a da unidade, quebradas do mesmo jeito.
    // Uma janela por cima do ponto esconde a aba (`isMouseOverCheckWindows`).
    if rules::HEAT_TAB.is_mouse_over(at, game.mb_window_covers(at)) {
        let slice = |first: usize| data.get(first..first + 4).unwrap_or(&[]);
        let unit = basis::temperature_unit(rules::LOCAL_CONFIG_NAMESPACE);
        let info = rules::heat_tab_tooltip(
            data_slots::join_f64(slice(DATA_HEAT)),
            data_slots::join_f64(slice(DATA_TRANSFER_LOSS)),
            data_slots::join_f64(slice(DATA_ENVIRONMENT_LOSS)),
            unit,
            &translate,
        );
        let lines = wrapped(game, &info);
        let [x, y, w, h] = rules::HEAT_TAB.rect;
        basis::menu_tooltip_flat(game, batch, &lines, cursor, [cursor.0 - at[0] + x, cursor.1 - at[1] + y, w, h]);
        return true;
    }
    let (widget, text) = if gui::contains(ENERGY_BAR, at) {
        let energy = data_slots::join(get(DATA_ENERGY), get(DATA_ENERGY + 1));
        let capacity = rules::ENERGY_CAPACITY as u64;
        (ENERGY_BAR, rules::energy_tooltip(u64::from(energy), capacity, &translate))
    } else if gui::contains(LAVA_GAUGE, at) {
        (LAVA_GAUGE, rules::lava_tooltip(get(DATA_LAVA).max(0) as u64, &translate))
    } else {
        return false;
    };
    // O retângulo do widget na janela: o canto da tela é o cursor menos a posição relativa dele.
    let [x, y, w, h] = widget;
    let rect = [cursor.0 - at[0] + x, cursor.1 - at[1] + y, w, h];
    basis::menu_tooltip(game, batch, &[(text, None)], cursor, rect);
    true
}

/// O dono das regras de slot que este mod deixa na `slot_rules` do núcleo.
const RULES_OWNER: &str = "mekanismgenerators";

/// Antes de um clique do jogador no menu de uma máquina (`lan_machine_click`, que o servidor roda para o anfitrião e para os
/// convidados): deixa a regra de `InventoryContainerSlot.mayPlace` do Heat Generator aberto, que o hook `menu_slot_rules` do Horizon
/// consulta dentro do menu, onde não há o jogo. A regra do slot de combustível pergunta o tempo de queima, que só o jogo sabe: ela
/// responde para cada item que o clique pode perguntar, que são os que o menu tem (o inventário de quem clica, o cursor e os slots do
/// bloco).
pub(in crate::game) fn menu_click_before(game: &Game<'_>, host: &crate::game::lan::LanHost, conn: u32) {
    slot_rules::clear(RULES_OWNER);
    let Some(guest) = host.hub.guests.iter().find(|guest| guest.conn == conn) else {
        return;
    };
    let Some(horizon_client::lan::menu::Slots::Block((x, y, z))) = guest.menu.as_ref().map(|menu| menu.slots) else {
        return;
    };
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(x, y, z)) else {
        return;
    };
    if entity.id != rules::HEAT_GENERATOR {
        return;
    }
    let stored = stored_energy(entity);
    let mut answers: std::collections::HashMap<String, (bool, bool)> = std::collections::HashMap::new();
    let stacks = guest.inventory.iter().flatten().chain(guest.carried.as_ref()).chain(entity.items.iter().flatten());
    for stack in stacks {
        answers
            .entry(stack.item.to_string())
            .or_insert_with(|| (is_fuel(game, stack), energy_can_drain(stored, true, item_energy(stack))));
    }
    slot_rules::set(
        RULES_OWNER,
        rules::HEAT_GENERATOR,
        std::rc::Rc::new(move |slot, item| {
            answers.get(item).is_some_and(|&(fuel, drain)| match slot {
                FUEL_SLOT => fuel,
                ENERGY_SLOT => drain,
                _ => false,
            })
        }),
    );
}

/// O menu do Heat Generator visto pelo `quickMoveStack` do Mekanism: os slots do bloco (`entity.items`: combustível e energia, na ordem
/// do Java, e os dois slots virtuais da janela de upgrades no fim), o inventário do jogador e a barra de armadura. Um slot do bloco tem o
/// índice dele; o do jogador, `items.len() + índice no inventário` (a barra de atalhos é 0 a 8, o inventário 9 a 35); e os da barra
/// `items.len() + 36 + k` (a armadura da cabeça aos pés e a mão), a ordem do menu do Java e dos índices que o cliente manda.
struct GeneratorMenu<'a, 'g> {
    game: &'a Game<'g>,
    entity: &'a mut ModBlockEntity,
    inventory: &'a mut [Option<ItemStack>],
    /// `Inventory.armor`, da cabeça aos pés, e `Inventory.offhand`: os slots 40 a 44 do menu de quem clica.
    armor: &'a mut [Option<ItemStack>; 4],
    offhand: &'a mut Option<ItemStack>,
    /// Os slots da barra que o tipo de menu declarou, na ordem do menu.
    bar: Vec<armor_slots::PlacedSlot>,
    /// A janela que o jogador que clica tem escolhida (`getSelectedWindow(player)`).
    window: Option<SelectedWindow>,
    upgrades: Rc<upgrade::UpgradeData>,
}

/// `EquipmentSlot.getName()`, como o catálogo de itens do jogo o escreve em `equip_slot`.
fn equipment_name(slot: EquipmentSlot) -> &'static str {
    match slot {
        EquipmentSlot::Feet => "feet",
        EquipmentSlot::Legs => "legs",
        EquipmentSlot::Chest => "chest",
        EquipmentSlot::Head => "head",
    }
}

/// `ItemStack.canEquip(slot, player)`: o item se veste nesse slot e o jogador está entre os que o `equippable` dele permite. É a conta
/// que o motor de menus do Horizon faz para o `ArmorSlot` (`Menu::may_place`).
fn can_equip(game: &Game<'_>, stack: &ItemStack, slot: EquipmentSlot) -> bool {
    game.items().get(&stack.item).is_some_and(|info| {
        info.equip_slot.as_deref() == Some(equipment_name(slot))
            && info.equip_allowed.as_ref().is_none_or(|allowed| allowed.contains("minecraft:player"))
    })
}

impl GeneratorMenu<'_, '_> {
    fn first_player(&self) -> usize {
        self.entity.items.len()
    }

    /// O slot da barra (`0` a `bar.len() - 1`) que um índice do menu é.
    fn bar_index(&self, slot: usize) -> Option<usize> {
        let k = slot.checked_sub(self.first_player() + armor_bar::PLAYER_SLOTS)?;
        (k < self.bar.len()).then_some(k)
    }

    /// `isValidForInsertion(.., MANUAL)` do slot: o de combustível (`FluidFuelInventorySlot.forFuel`) aceita o que queima ou o que enche
    /// o tanque de lava (o balde, que também queima), o de energia (`EnergyInventorySlot.drain`) o que `canDrain`: nenhum item portado
    /// tem handler de energia, então nada; e o virtual de entrada de upgrades (`UpgradeInventorySlot.input`) o upgrade da tag do gerador.
    /// A saída de upgrades não aceita nada de quem clica.
    fn accepts(&self, slot: usize, stack: &ItemStack) -> bool {
        match slot {
            FUEL_SLOT => is_fuel(self.game, stack),
            ENERGY_SLOT => energy_can_drain(stored_energy(self.entity), true, item_energy(stack)),
            rules::UPGRADE_SLOT => rules::accepts_upgrade(&self.upgrades, &stack.item, self.upgrade_type(stack).as_deref()),
            _ => false,
        }
    }

    fn upgrade_type(&self, stack: &ItemStack) -> Option<String> {
        match self.game.native_component(stack, rules::UPGRADE_TYPE_COMPONENT) {
            Ok(Some(Tag::String(kind))) => Some(kind.to_string_lossy()),
            _ => None,
        }
    }

    fn cell(&mut self, slot: usize) -> &mut Option<ItemStack> {
        let first = self.first_player();
        if slot < first {
            &mut self.entity.items[slot]
        } else if let Some(k) = self.bar_index(slot) {
            if k < self.armor.len() { &mut self.armor[k] } else { &mut *self.offhand }
        } else {
            &mut self.inventory[slot - first]
        }
    }

    /// `TransactionalSlot.insert` do `ArmorSlot` e do `OffhandSlot` da barra: a regra é de `armor_slots::insert`, o jogo só diz o que sabe
    /// do item (em que slot se veste, o tamanho da pilha).
    fn insert_into_bar(&mut self, k: usize, item: &ItemStack, amount: u32) -> u32 {
        let kind = self.bar[k].kind;
        let item_max = basis::stack_limit(self.game, item);
        let game = self.game;
        let cell = self.cell_of_bar(k);
        let held = cell.as_ref().map_or(0, |held| held.count);
        let same = cell.as_ref().is_some_and(|held| held.same_item_same_components(item));
        let moved = armor_slots::insert(kind, held, same, amount, item_max, |slot| can_equip(game, item, slot));
        if moved == 0 {
            return 0;
        }
        match cell {
            Some(held) => held.count += moved,
            empty => {
                let mut placed = item.clone();
                placed.count = moved;
                *empty = Some(placed);
            }
        }
        moved
    }

    fn cell_of_bar(&mut self, k: usize) -> &mut Option<ItemStack> {
        if k < self.armor.len() { &mut self.armor[k] } else { &mut *self.offhand }
    }
}

/// O handler de energia de um item: nenhum item portado tem um (`energy_tablet` e os outros ainda não existem).
fn item_energy(_stack: &ItemStack) -> Option<ItemEnergy> {
    None
}

fn stored_energy(entity: &ModBlockEntity) -> i64 {
    i64::from(basis::stored_energy(entity))
}

impl QuickMenu for GeneratorMenu<'_, '_> {
    type Item = ItemStack;

    fn slots_of(&self, region: Region) -> Vec<usize> {
        let first = self.first_player();
        let bar = |wanted: fn(SlotKind) -> bool| -> Vec<usize> {
            let base = first + armor_bar::PLAYER_SLOTS;
            self.bar.iter().enumerate().filter(|(_, slot)| wanted(slot.kind)).map(|(k, _)| base + k).collect()
        };
        match region {
            // A ordem do Java: os dois virtuais primeiro, depois os do gerador em `getInitialInventory`.
            Region::Container => rules::CONTAINER_ORDER.to_vec(),
            Region::Hotbar => (first..first + 9).collect(),
            Region::Main => (first + 9..first + 36).collect(),
            Region::Armor => bar(|kind| matches!(kind, SlotKind::Armor(_))),
            Region::Offhand => bar(|kind| kind == SlotKind::Offhand),
        }
    }

    fn region(&self, slot: usize) -> Region {
        let first = self.first_player();
        if slot < first {
            Region::Container
        } else if let Some(k) = self.bar_index(slot) {
            match self.bar[k].kind {
                SlotKind::Armor(_) => Region::Armor,
                SlotKind::Offhand => Region::Offhand,
            }
        } else if slot - first < 9 {
            Region::Hotbar
        } else {
            Region::Main
        }
    }

    /// `ITransactionalSlot.exists(selectedWindow)`: um slot virtual só existe com a janela dele escolhida por quem clica.
    fn exists(&self, slot: usize) -> bool {
        rules::slot_exists(slot, self.window)
    }

    fn stack(&self, slot: usize) -> Option<(ItemStack, u32)> {
        let first = self.first_player();
        let held = if slot < first {
            self.entity.items[slot].as_ref()
        } else if let Some(k) = self.bar_index(slot) {
            if k < self.armor.len() { self.armor[k].as_ref() } else { self.offhand.as_ref() }
        } else {
            self.inventory[slot - first].as_ref()
        };
        held.filter(|stack| stack.count > 0).map(|stack| (stack.clone(), stack.count))
    }

    fn insert(&mut self, slot: usize, item: &ItemStack, amount: u32) -> u32 {
        if let Some(k) = self.bar_index(slot) {
            return self.insert_into_bar(k, item, amount);
        }
        if slot < self.first_player() && !self.accepts(slot, item) {
            return 0;
        }
        basis::insert_into_cell(self.game, self.cell(slot), item, amount)
    }

    fn remove(&mut self, slot: usize, amount: u32) {
        basis::remove_from_cell(self.cell(slot), amount);
    }
}

/// Shift-click do menu do Heat Generator: `MekanismContainer.quickMoveStack`, como nas quatro máquinas. Do inventário vai para o slot do
/// gerador que aceita o item (o combustível, e o de upgrades quando a janela do jogador que clica está aberta) e, não cabendo, para a
/// outra região do inventário; do gerador vai para a barra de atalhos e depois o inventário.
pub(in crate::game) fn quick_move(
    game: &mut Game<'_>,
    pos: [i32; 3],
    entity: &mut ModBlockEntity,
    container_slots: usize,
    slot: usize,
    stack: &mut ItemStack,
    handled: &mut bool,
) {
    if !is_generator(&entity.id) {
        return;
    }
    ensure_slots(entity);
    let window = game.mb_window_selected(pos);
    let upgrades = window_upgrade_data();
    let first = entity.items.len();
    // O menu numera o inventário do jogador depois dos slots do bloco: o principal (9 a 35), depois a barra de atalhos (0 a 8), e então a
    // barra de armadura (a armadura da cabeça aos pés e a mão).
    let from = if slot < container_slots {
        slot
    } else {
        let n = slot - container_slots;
        first + match n {
            0..27 => 9 + n,
            27..36 => n - 27,
            _ => n,
        }
    };
    let mut inventory = std::mem::take(&mut game.inventory);
    let mut armor = std::mem::take(&mut game.armor);
    let mut offhand = game.offhand.take();
    let left = {
        let bar = armor_bar::slots(rules::MENU);
        let mut menu = GeneratorMenu {
            game: &*game,
            entity,
            inventory: &mut inventory,
            armor: &mut armor,
            offhand: &mut offhand,
            bar,
            window,
            upgrades,
        };
        quick_move::quick_move(&mut menu, from);
        menu.stack(from)
    };
    game.inventory = inventory;
    game.armor = armor;
    game.offhand = offhand;
    // O slot clicado guarda o que sobrou; `handled` impede o roteamento do jogo, que levaria o item a outros slots.
    match left {
        Some((remaining, count)) => {
            *stack = remaining;
            stack.count = count;
        }
        None => stack.count = 0,
    }
    *handled = true;
}
