// Os cubos de energia no jogo (`TileEntityEnergyCube`). As regras ficam em machinery_basis_core::energy_cube, e o que é genérico (nome, configuração de lados, energia do item,
// faces de item por instância) em machinery_basis.
use crate::game::mekanism_glue as glue;
use crate::game::mekanism_rules as rules;
use crate::game::{Game, machinery_basis as basis};
use crate::game::native_block_entity_draw::{BlockEntityDraw, CubeFace};
use crate::item_render::{ItemRenderState, LayerGeometry, LayerRender};
use horizon_assets::item_bake::BakedItemModel;
use horizon_assets::item_model::FoilType;
use std::sync::Arc;
use horizon_client::containers::{BlockEntity, ModBlockEntity, ModContainer, ModEnergyBuffer};
use horizon_client::items::ItemStack;
use horizon_world::nbt::{JavaString, Tag};
use machinery_basis_core::components::Components;
use machinery_basis_core::config_window::{self, ConfigPacket, ConfigView};
use machinery_basis_core::container_sync::{TrackedSource, Value};
use machinery_basis_core::ejector::TileComponentEjector;
use machinery_basis_core::menu_source;
use machinery_basis_core::slot_rules;
use machinery_basis_core::tile_trackers::source;
use machinery_basis_core::upgrade_window::UpgradeWindowSpec;
use machinery_basis_core::containers::{AutomationType, InventorySlot, ItemResource};
use machinery_basis_core::energy_cube::{self, CubeSideState, EnergyCube, ItemEnergy};
use machinery_basis_core::side_config::{TileComponentConfig, TransmissionType};
use machinery_basis_core::sides::Face;
use machinery_basis_core::tile::RedstoneControl;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Os slots do menu do cubo, na ordem de `getInitialInventory`: o de descarga e o de carga.
const SLOTS: usize = 2;
const REDSTONE_OWNER: &str = "mekanism_cube";
const REDSTONE: &str = "redstone";
const CONTROL_TYPE: &str = "control_type";
const CURRENT_REDSTONE: &str = "current_redstone";
/// Um objeto vivo que não anda por tantos ticks sai da tabela (o bloco foi quebrado ou o chunk descarregado).
const STALE_TICKS: i64 = 100;
const SWEEP_TICKS: i64 = 200;

/// O objeto vivo de um cubo. O Horizon só guarda a `ModBlockEntity`; o que o Java guarda no objeto (os contêineres, o ejetor, a escala) fica aqui, entre os ticks, e o NBT leva o que o
/// Java salva.
struct LiveCube {
    cube: EnergyCube,
    last_seen: i64,
    /// O que a configuração de lados e o ejetor tinham no NBT na última vez que este objeto os viu ou escreveu.
    side_nbt: basis::SideNbt,
    /// A capacidade e as faces de energia que o Horizon já recebeu.
    energy_state: Option<(i32, [bool; 6], [bool; 6])>,
    /// `redstone`, o modo de controle e o nível do comparador que o NBT já tem.
    scalars: Option<(bool, RedstoneControl, i32)>,
    /// A energia que o NBT tem (`energy_amount`, só o criativo), para saber se alguém a mexeu de fora.
    nbt_amount: i64,
    /// O que o cliente já recebeu do desenho: a escala e o `CubeSideState` de cada lado.
    published: Option<(f32, [CubeSideState; 6])>,
}

thread_local! {
    static LIVE_CUBES: RefCell<HashMap<(&'static str, [i32; 3]), LiveCube>> = RefCell::new(HashMap::new());
    static LAST_SWEEP: std::cell::Cell<i64> = const { std::cell::Cell::new(i64::MIN) };
}

pub(in crate::game) fn tier_of(block: &str) -> Option<usize> {
    rules::cube_tier(block)
}

pub(in crate::game) fn is_cube(block: &str) -> bool {
    tier_of(block).is_some()
}

/// Como a configuração de lados do cubo é montada e o padrão do item dele (`ItemBlockEnergyCube.SIDE_CONFIG`).
pub(in crate::game) fn side_spec(block: &str) -> Option<basis::SideSpec> {
    is_cube(block).then(|| basis::SideSpec { spec: energy_cube::side_config_spec(), default_side_config: rules::energy_cube_default() })
}

// ---------------------------------------------------------------------------------------------------------------------------
// O handler de energia dos itens.

/// O recurso da pilha com a energia dela registrada para o handler (`basis::energy_slot_resource`).
fn resource_of(game: &Game<'_>, stack: &ItemStack) -> ItemResource {
    basis::energy_slot_resource(game, stack)
}

fn items(game: &Game<'_>) -> ItemEnergy {
    items_with(glue::energy_index(game))
}

fn items_with(conversions: Rc<machinery_basis_core::energy_conversion::EnergyConversionCache>) -> ItemEnergy {
    basis::item_energy_handler(conversions)
}

// ---------------------------------------------------------------------------------------------------------------------------
// As faces e as regras de slot de quem está de fora (H2, decisão 94): o Horizon pergunta ao contêiner da instância, e o cubo responde pelo que o tile guarda.

/// `ConfigHolder` do cubo: o `component_config` do NBT do tile, montado como o tick o monta, com a frente do bloco. O item posto traz a configuração dele; um cubo posto por
/// `setBlock` fica com tudo em `NONE`.
pub(in crate::game) fn item_faces(block: &str, entity: &ModBlockEntity, front: Face) -> Option<machinery_basis_core::item_instance::InstanceItems> {
    let spec = side_spec(block)?.spec;
    let mut config = TileComponentConfig::new(&spec.types);
    (spec.setup)(&mut config);
    if let Some(data) = &basis::read_side_nbt(entity).config {
        config.deserialize(data, front, false);
    }
    Some(rules::cube_item_instance(&config, front))
}

/// As regras dos dois slots (`EnergyInventorySlot.fillOrConvert` e `drain`) com a energia que o tile guarda e o índice de conversões que `conversions` dá.
fn slot_rules_of(tier: usize, entity: &ModBlockEntity, conversions: Rc<machinery_basis_core::energy_conversion::EnergyConversionCache>) -> energy_cube::SlotRules {
    let stored = if tier == energy_cube::CREATIVE { creative_amount(entity) } else { i64::from(basis::stored_energy(entity)) };
    rules::cube_slot_rules(tier, stored, &glue::server_config(), &items_with(conversions))
}

/// `isValidForExtraction(item, EXTERNAL)` do slot do cubo, sem o jogo (o `can_take` do Horizon): o índice de conversões é o da última pergunta.
pub(in crate::game) fn item_extractable(block: &str, entity: &ModBlockEntity, slot: usize, stack: &ItemStack, limit: u32) -> bool {
    let (Some(tier), Some(conversions)) = (tier_of(block), glue::published_energy_index()) else {
        return false;
    };
    slot_rules_of(tier, entity, conversions).is_valid_for_extraction(slot, &basis::item_resource_with_limit(stack, limit), AutomationType::External)
}

/// `isValidForInsertion(item, EXTERNAL)` do slot do cubo pela face `face`: a face deixa entrar e o slot aceita o item.
pub(in crate::game) fn item_accepts(game: &Game<'_>, pos: [i32; 3], entity: &ModBlockEntity, slot: usize, stack: &ItemStack, face: Face) -> bool {
    let Some(tier) = tier_of(&entity.id) else {
        return false;
    };
    let allowed = item_faces(&entity.id, entity, basis::front(game, pos)).is_some_and(|items| items.accepts_through(face, slot));
    allowed && slot_rules_of(tier, entity, glue::energy_index(game)).is_valid_for_insertion(slot, &resource_of(game, stack), AutomationType::External)
}

// ---------------------------------------------------------------------------------------------------------------------------
// O NBT.

fn read_scalars(entity: &ModBlockEntity) -> (bool, RedstoneControl, i32) {
    (
        glue::nbt_bool(entity, REDSTONE),
        RedstoneControl::from_serialized_name(&glue::nbt_string(entity, CONTROL_TYPE).unwrap_or_default()),
        glue::nbt_int(entity, CURRENT_REDSTONE),
    )
}

fn write_scalars(entity: &mut ModBlockEntity, scalars: (bool, RedstoneControl, i32)) {
    glue::put(entity, REDSTONE, Tag::Byte(i8::from(scalars.0)));
    glue::put(entity, CONTROL_TYPE, Tag::String(JavaString::from(scalars.1.serialized_name())));
    glue::put(entity, CURRENT_REDSTONE, Tag::Int(scalars.2));
}

/// `Cube` nova do que a `ModBlockEntity` guarda.
fn born(game: &Game<'_>, pos: [i32; 3], entity: &ModBlockEntity, tier: usize) -> LiveCube {
    let config = glue::server_config();
    let mut cube = EnergyCube::new(rules::energy_cube_config(tier, &config, items(game)));
    let scalars = read_scalars(entity);
    let mut tile = cube.tile_mut();
    tile.restore(
        &machinery_basis_core::tile::SavedState { redstone: scalars.0, control_type: scalars.1, current_redstone: scalars.2, ..Default::default() },
        false,
    );
    drop(tile);
    cube.set_facing(basis::front(game, pos));
    let side = basis::read_side_nbt(entity);
    cube.restore_side_config(side.config.as_ref(), side.ejector.as_ref(), false);
    // O criativo guarda o valor exato fora do `i32` do Horizon; os outros guardam o `energy`.
    let stored = if tier == energy_cube::CREATIVE { creative_amount(entity) } else { i64::from(basis::stored_energy(entity)) };
    cube.energy().set_energy(stored, None);
    LiveCube { cube, last_seen: i64::MIN, side_nbt: side, energy_state: None, scalars: None, nbt_amount: stored, published: None }
}

/// A energia do cubo criativo no NBT: o valor exato, ou o `energy` inteiro cheio como `Long.MAX_VALUE` (o item cheio do criativo, que o Horizon grava no teto).
fn creative_amount(entity: &ModBlockEntity) -> i64 {
    let stored = basis::stored_energy_long(entity);
    if stored == i64::from(i32::MAX) { i64::MAX } else { stored }
}

// ---------------------------------------------------------------------------------------------------------------------------
// O que o Horizon deixa ver.

pub(in crate::game) fn block_entity(block: &str, entity: &mut Option<(String, horizon_world::nbt::Compound)>) {
    if is_cube(block) {
        // `AttributeRedstone`: o jogo avisa o bloco quando um vizinho muda.
        basis::watch_redstone(REDSTONE_OWNER, is_cube);
        register_windows(block);
        if entity.is_none() {
            *entity = Some((block.to_owned(), horizon_world::nbt::Compound::default()));
        }
    }
}

pub(in crate::game) fn ticks(block: &str, ticks: &mut bool) {
    if is_cube(block) {
        *ticks = true;
    }
}

pub(in crate::game) fn energy(block: &str, energy: &mut Option<ModEnergyBuffer>) {
    let Some(tier) = tier_of(block) else {
        return;
    };
    // O `ModEnergyBuffer` do Horizon é `i32`: o criativo fica no teto. As faces são da instância (H1), e um tile ainda sem configuração não tem capability em nenhuma.
    let capacity = i32::try_from(glue::server_config().cubes[tier]).unwrap_or(i32::MAX);
    *energy = Some(ModEnergyBuffer { capacity, max_insert: capacity, max_extract: capacity, ..Default::default() });
}

pub(in crate::game) fn container(block: &str, container: &mut Option<ModContainer>) {
    if is_cube(block) {
        // Sem interface vanilla: as faces são da instância (`item_faces`, H2). O menu é a tela `GuiEnergyCube` (`screen` do glue).
        *container = Some(ModContainer { slots: SLOTS, menu: Some(block.to_owned()), ..Default::default() });
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// Os slots entre o menu e o objeto.

fn slot_from_entity(game: &Game<'_>, slot: &InventorySlot, stack: Option<&ItemStack>) {
    match stack {
        None => slot.set_contents(None, 0, None),
        Some(stack) => {
            let resource = resource_of(game, stack);
            if slot.resource().as_ref() != Some(&resource) || slot.amount() != stack.count {
                slot.set_contents(Some(resource), stack.count, None);
            }
        }
    }
}

/// O que o tick mudou volta para a `ModBlockEntity`. Um recurso feito pelo handler de energia (a energia nova do item) não passou por uma pilha do jogo: a pilha dele é a que o slot
/// tinha, com a energia nova.
fn slot_to_entity(game: &Game<'_>, entity: &mut ModBlockEntity, index: usize, slot: &InventorySlot) {
    let contents = slot.contents();
    let current = entity.items[index].as_ref();
    let same = match (&contents.resource, current) {
        (None, None) => true,
        (Some(resource), Some(stack)) => *resource == resource_of(game, stack) && contents.amount == stack.count,
        _ => false,
    };
    if same {
        return;
    }
    let new = contents.resource.map(|resource| match current {
        Some(old) if resource_of(game, old) == resource => {
            let mut stack = old.clone();
            stack.count = contents.amount;
            stack
        }
        _ => basis::energy_slot_stack(game, current, &resource, contents.amount),
    });
    entity.items[index] = new;
}

// ---------------------------------------------------------------------------------------------------------------------------
// Os vizinhos de energia (`basis::GameEnergy`, que o Heat Generator também usa).

/// As faces por onde a capability de energia deixa entrar e sair (`ConfigHolder.canInsert`/`canExtract` do `DataType` do lado).
fn energy_faces(cube: &EnergyCube) -> ([bool; 6], [bool; 6]) {
    let (mut insert, mut extract) = ([false; 6], [false; 6]);
    for face in Face::ALL {
        if let Some(info) = cube.side_config().slot_info(TransmissionType::Energy, cube.facing(), Some(face)) {
            insert[face as usize] = info.can_input;
            extract[face as usize] = info.can_output;
        }
    }
    (insert, extract)
}

// ---------------------------------------------------------------------------------------------------------------------------
// O tick.

pub(in crate::game) fn tick(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    let Some(tier) = tier_of(&entity.id) else {
        return;
    };
    let now = game.world.game_time;
    let dimension = game.native_dimension();
    let key = (dimension, pos);
    if entity.items.len() < SLOTS {
        entity.items.resize(SLOTS, None);
    }
    basis::name_apply_pending(entity, pos);
    basis::security_apply_pending(entity, pos);
    basis::side_apply_pending(entity, pos);
    let alive = LIVE_CUBES.with_borrow_mut(|cubes| cubes.remove(&key));
    let mut live = match alive {
        Some(live) if live.last_seen + 1 == now && glue::has_saved(entity) => live,
        _ => born(game, pos, entity, tier),
    };
    let creative = tier == energy_cube::CREATIVE;
    slot_from_entity(game, live.cube.discharge_slot(), entity.items[energy_cube::DISCHARGE_SLOT].as_ref());
    slot_from_entity(game, live.cube.charge_slot(), entity.items[energy_cube::CHARGE_SLOT].as_ref());
    basis::energy_apply_pending(entity, pos, live.cube.energy().capacity());
    if creative {
        // Quem mexeu no valor exato por fora (um comando de dados, o item posto) muda o contêiner.
        let in_nbt = basis::stored_energy_long(entity);
        if in_nbt != live.nbt_amount {
            let stored = if in_nbt == i64::from(i32::MAX) { i64::MAX } else { in_nbt };
            live.cube.energy().set_energy(stored, None);
        }
    } else {
        let stored = i64::from(basis::stored_energy(entity));
        if live.cube.energy().amount() != stored {
            live.cube.energy().set_energy(stored, None);
        }
    }
    if let Some(powered) = basis::take_power(game, pos) {
        live.cube.tile_mut().update_power(powered);
    }
    live.cube.set_facing(basis::front(game, pos));
    let in_nbt = basis::read_side_nbt(entity);
    if in_nbt != live.side_nbt {
        live.cube.restore_side_config(in_nbt.config.as_ref(), in_nbt.ejector.as_ref(), true);
        live.side_nbt = in_nbt;
    }

    let result = {
        let neighbours = basis::GameEnergy::new(game, dimension, pos);
        live.cube.tick_server(now as u64, &neighbours)
    };
    glue::publish_comparator(game, dimension, pos, live.cube.tile().current_redstone_level(), result.comparator_changed);

    slot_to_entity(game, entity, energy_cube::DISCHARGE_SLOT, live.cube.discharge_slot());
    slot_to_entity(game, entity, energy_cube::CHARGE_SLOT, live.cube.charge_slot());
    let amount = live.cube.energy().amount();
    if creative {
        // O Horizon vê um buffer vazio que aceita tudo (o criativo aceita sem guardar); o valor exato fica em `energy_amount`.
        basis::set_stored_energy(entity, 0);
        entity.nbt.insert(JavaString::from(basis::ENERGY_AMOUNT), Tag::Long(amount));
        live.nbt_amount = amount;
    } else if basis::stored_energy(entity) != basis::energy_as_int(amount, "a energia guardada") {
        basis::set_stored_energy(entity, basis::energy_as_int(amount, "a energia guardada"));
    }
    let side_now = basis::SideNbt::of(live.cube.side_config().serialize(), live.cube.ejector().serialize());
    if side_now != live.side_nbt {
        basis::write_side_nbt(entity, &side_now);
        live.side_nbt = basis::read_side_nbt(entity);
    }
    let capacity = i32::try_from(live.cube.energy().capacity()).unwrap_or(i32::MAX);
    let (insert_faces, extract_faces) = energy_faces(&live.cube);
    let energy_state = (capacity, insert_faces, extract_faces);
    if live.energy_state != Some(energy_state) {
        basis::set_energy_capacity(dimension, pos, &entity.id, capacity, insert_faces, extract_faces);
        live.energy_state = Some(energy_state);
    }
    let scalars = {
        let tile = live.cube.tile();
        (tile.is_powered(), tile.control_type(), tile.current_redstone_level())
    };
    if live.scalars != Some(scalars) {
        write_scalars(entity, scalars);
        live.scalars = Some(scalars);
    }
    let drawn = (live.cube.energy_scale(), live.cube.side_states());
    if live.published != Some(drawn) {
        game.mb_view_changed(dimension, pos, drawn.0, drawn.1.map(|state| state as u8));
        live.published = Some(drawn);
    }
    if entity.data.is_empty() {
        entity.data = vec![1];
    }

    live.last_seen = now;
    LIVE_CUBES.with_borrow_mut(|cubes| {
        cubes.insert(key, live);
        if now.saturating_sub(LAST_SWEEP.get()) >= SWEEP_TICKS {
            LAST_SWEEP.set(now);
            cubes.retain(|_, live| now - live.last_seen <= STALE_TICKS);
            basis::retain_energy_capacities(|dimension, pos| cubes.contains_key(&(dimension, pos)) || glue::has_live_machine(dimension, pos));
        }
    });
}

/// `TrackingEnergyHandler.insert` → `LastEnergyTracker.received`: o buffer do cubo em `pos` passou a guardar `energy`.
pub(in crate::game) fn energy_stored(game: &Game<'_>, pos: [i32; 3], energy: i32) {
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(pos[0], pos[1], pos[2])) else {
        return;
    };
    if !is_cube(&entity.id) {
        return;
    }
    let before = basis::stored_energy(entity);
    let Some(received) = energy.checked_sub(before).filter(|received| *received > 0) else {
        return;
    };
    let key = (game.native_dimension(), pos);
    LIVE_CUBES.with(|cubes| {
        if let Ok(cubes) = cubes.try_borrow()
            && let Some(live) = cubes.get(&key)
        {
            live.cube.energy_received(i64::from(received), game.world.game_time as u64);
        }
    });
}

/// `PacketGuiInteract.NEXT_REDSTONE_CONTROL` e `PREVIOUS_REDSTONE_CONTROL`: `setControlType(getControlType().getNext(supportsMode))` no objeto vivo do cubo, e no NBT, de onde um cubo que ainda não andou o lê.
pub(in crate::game) fn step_control(game: &Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, forward: bool) {
    let key = (game.native_dimension(), pos);
    let next = LIVE_CUBES.with_borrow_mut(|cubes| {
        cubes.get_mut(&key).map(|live| {
            let next = machinery_basis_core::redstone_tab::cycle(live.cube.tile().control_type(), forward);
            live.cube.tile_mut().set_control_type(next);
            next
        })
    });
    let next = next.unwrap_or_else(|| machinery_basis_core::redstone_tab::cycle(basis::redstone_read(entity), forward));
    basis::redstone_write(entity, next);
}

/// `TileEntityEnergyCube.writeConfigurationData`: o que o cartão de configuração e o Configurator copiam do cubo.
pub(in crate::game) fn write_configuration_data(game: &Game<'_>, pos: [i32; 3]) -> Option<horizon_world::nbt::Compound> {
    let key = (game.native_dimension(), pos);
    let live = LIVE_CUBES.with_borrow(|cubes| cubes.get(&key).map(|live| live.cube.write_configuration_data()));
    let data = match live {
        Some(data) => data,
        None => {
            let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(pos[0], pos[1], pos[2])) else {
                return None;
            };
            let tier = tier_of(&entity.id)?;
            born(game, pos, entity, tier).cube.write_configuration_data()
        }
    };
    let mut out = horizon_world::nbt::Compound::default();
    if let Some(control) = data.control_type {
        glue::put_compound(&mut out, CONTROL_TYPE, Tag::String(JavaString::from(control.serialized_name())));
    }
    basis::side_nbt_into(&mut out, &basis::SideNbt { config: data.config, ejector: data.ejector });
    Some(out)
}

/// `TileEntityEnergyCube.setConfigurationData`.
pub(in crate::game) fn set_configuration_data(game: &mut Game<'_>, pos: [i32; 3], input: &horizon_world::nbt::Compound) -> bool {
    let key = (game.native_dimension(), pos);
    let side = basis::side_nbt_from(input);
    let control_type = match input.get(&JavaString::from(CONTROL_TYPE)) {
        Some(Tag::String(name)) => Some(RedstoneControl::from_serialized_name(&name.to_string_lossy())),
        _ => None,
    };
    let data = machinery_basis_core::tile::ConfigurationData { control_type, config: side.config.clone(), ejector: side.ejector.clone() };
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&(pos[0], pos[1], pos[2])) else {
        return false;
    };
    let applied = LIVE_CUBES.with_borrow_mut(|cubes| {
        let live = cubes.get_mut(&key)?;
        live.cube.set_configuration_data(&data);
        Some(basis::SideNbt::of(live.cube.side_config().serialize(), live.cube.ejector().serialize()))
    });
    match applied {
        Some(now) => {
            basis::write_side_nbt(entity, &now);
            let seen = basis::read_side_nbt(entity);
            LIVE_CUBES.with_borrow_mut(|cubes| {
                if let Some(live) = cubes.get_mut(&key) {
                    live.side_nbt = seen;
                }
            });
        }
        None => basis::write_side_nbt(entity, &side),
    }
    game.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
    true
}

/// Se há um cubo vivo em `pos` (a varredura das capacidades de energia não pode apagar a dele).
pub(in crate::game) fn is_live(dimension: &'static str, pos: [i32; 3]) -> bool {
    LIVE_CUBES.with(|cubes| cubes.try_borrow().map_or(true, |cubes| cubes.contains_key(&(dimension, pos))))
}

/// Corre `act` no cubo em `pos`: o objeto vivo, ou um feito do NBT que não fica na tabela. O que `act` mudou na configuração de lados e no ejetor vai ao NBT na hora, e as faces de energia
/// são avisadas ao Horizon (H1). Quem usa: o Configurator e o cartão de configuração.
pub(in crate::game) fn with_cube<R>(game: &mut Game<'_>, pos: [i32; 3], act: impl FnOnce(&mut EnergyCube) -> R) -> Option<R> {
    let block = game.native_block(pos);
    let tier = tier_of(&block)?;
    let dimension = game.native_dimension();
    let key = (dimension, pos);
    let at = (pos[0], pos[1], pos[2]);
    let alive = LIVE_CUBES.with_borrow_mut(|cubes| cubes.remove(&key));
    let was_alive = alive.is_some();
    let mut live = match alive {
        Some(live) => live,
        None => {
            let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&at) else {
                return None;
            };
            born(game, pos, entity, tier)
        }
    };
    let result = act(&mut live.cube);
    live.cube.take_side_changes();
    let now = basis::SideNbt::of(live.cube.side_config().serialize(), live.cube.ejector().serialize());
    let capacity = i32::try_from(live.cube.energy().capacity()).unwrap_or(i32::MAX);
    let (insert_faces, extract_faces) = energy_faces(&live.cube);
    if let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&at) {
        basis::write_side_nbt(entity, &now);
        live.side_nbt = basis::read_side_nbt(entity);
        basis::set_energy_capacity(dimension, pos, &entity.id, capacity, insert_faces, extract_faces);
        live.energy_state = Some((capacity, insert_faces, extract_faces));
    }
    game.world.modified.insert((at.0 >> 4, at.2 >> 4));
    if was_alive {
        LIVE_CUBES.with_borrow_mut(|cubes| cubes.insert(key, live));
    }
    Some(result)
}

/// O nome do tile, a configuração de lados e a energia do item que o coloca (`setPlacedBy`/`applyImplicitComponents`) e do que ele solta (`collectImplicitComponents`).
pub(in crate::game) fn place_block(game: &mut Game<'_>, stack: &ItemStack, pos: [i32; 3]) {
    let block = game.native_block(pos);
    if !is_cube(&block) {
        return;
    }
    // `BlockMekanism.setPlacedBy` -> `onAdded` -> `updatePower`.
    basis::on_added(game, pos);
    let tier = tier_of(&block).unwrap_or_default();
    let capacity = glue::server_config().cubes[tier];
    // `copyToTile`: o item traz a energia, limitada à capacidade do cubo.
    basis::energy_placed(game, stack, pos, rules::ENERGY_COMPONENT, capacity);
    let dimension = game.native_dimension();
    let published = match game.world.block_entities.get(&(pos[0], pos[1], pos[2])) {
        Some(BlockEntity::Mod(entity)) => {
            let live = born(game, pos, entity, tier);
            let capacity = i32::try_from(live.cube.energy().capacity()).unwrap_or(i32::MAX);
            let (insert_faces, extract_faces) = energy_faces(&live.cube);
            basis::set_energy_capacity(dimension, pos, &entity.id, capacity, insert_faces, extract_faces);
            true
        }
        _ => false,
    };
    if published {
        // O tile novo guarda `currentRedstoneLevel = 0`.
        glue::publish_comparator(game, dimension, pos, 0, false);
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// O desenho: `EnergyCubeModel` (as portas e os LEDs de cada lado, pelo `CubeSideState`) e `RenderEnergyCube` (o núcleo que gira). O modelo estático do bloco é o quadro e os LEDs
// apagados (o bloco sem block entity, decisão 26); o desenho do tile põe por cima as portas dos lados ativos e o LED aceso dos que saem.

// Gerado de `models/block/energy_cube/base.json` do original: as peças de cada lado (`ports` e `leds`), na ordem de `RelativeSide` (frente, esquerda, direita, costas, alto, base).
/// Uma peça do modelo: a caixa em texels, e por face (`Direction`: baixo, cima, norte, sul, oeste, leste) o sprite (0 a 3: `energy_cube`, `ports`, `ports_led`, `leds`) e o `uv`.
struct Part {
    from: [f32; 3],
    to: [f32; 3],
    faces: [Option<(usize, [f32; 4])>; 6],
    emissive: bool,
}

const PORTS: [&[Part]; 6] = [
    &[
        Part { from: [3.0, 5.0, 1.0], to: [13.0, 11.0, 2.0], faces: [Some((0, [10.0, 5.0, 0.0, 6.0])), Some((0, [10.0, 1.0, 0.0, 0.0])), Some((0, [0.0, 0.0, 10.0, 6.0])), Some((0, [0.0, 0.0, 10.0, 6.0])), None, None], emissive: false },
        Part { from: [4.0, 4.0, 0.0], to: [12.0, 12.0, 1.0], faces: [Some((1, [0.0, 7.0, 8.0, 8.0])), Some((1, [0.0, 0.0, 8.0, 1.0])), Some((1, [0.0, 0.0, 8.0, 8.0])), Some((1, [8.0, 0.0, 16.0, 8.0])), Some((1, [0.0, 0.0, 1.0, 8.0])), Some((1, [7.0, 0.0, 8.0, 8.0]))], emissive: false },
        Part { from: [4.0, 4.0, 0.0], to: [12.0, 12.0, 0.0], faces: [None, None, Some((2, [8.0, 8.0, 16.0, 16.0])), None, None, None], emissive: true },
    ],
    &[
        Part { from: [14.0, 5.0, 3.0], to: [15.0, 11.0, 13.0], faces: [Some((0, [10.0, 5.0, 0.0, 6.0])), Some((0, [10.0, 1.0, 0.0, 0.0])), None, None, Some((0, [0.0, 0.0, 10.0, 6.0])), Some((0, [0.0, 0.0, 10.0, 6.0]))], emissive: false },
        Part { from: [15.0, 4.0, 4.0], to: [16.0, 12.0, 12.0], faces: [Some((1, [0.0, 7.0, 8.0, 8.0])), Some((1, [0.0, 0.0, 8.0, 1.0])), Some((1, [0.0, 0.0, 1.0, 8.0])), Some((1, [7.0, 0.0, 8.0, 8.0])), Some((1, [8.0, 0.0, 16.0, 8.0])), Some((1, [0.0, 0.0, 8.0, 8.0]))], emissive: false },
        Part { from: [16.0, 4.0, 4.0], to: [16.0, 12.0, 12.0], faces: [None, None, None, None, None, Some((2, [8.0, 8.0, 16.0, 16.0]))], emissive: true },
    ],
    &[
        Part { from: [1.0, 5.0, 3.0], to: [2.0, 11.0, 13.0], faces: [Some((0, [10.0, 5.0, 0.0, 6.0])), Some((0, [10.0, 1.0, 0.0, 0.0])), None, None, Some((0, [0.0, 0.0, 10.0, 6.0])), Some((0, [0.0, 0.0, 10.0, 6.0]))], emissive: false },
        Part { from: [0.0, 4.0, 4.0], to: [1.0, 12.0, 12.0], faces: [Some((1, [0.0, 7.0, 8.0, 8.0])), Some((1, [0.0, 0.0, 8.0, 1.0])), Some((1, [0.0, 0.0, 1.0, 8.0])), Some((1, [7.0, 0.0, 8.0, 8.0])), Some((1, [0.0, 0.0, 8.0, 8.0])), Some((1, [8.0, 0.0, 16.0, 8.0]))], emissive: false },
        Part { from: [0.0, 4.0, 4.0], to: [0.0, 12.0, 12.0], faces: [None, None, None, None, Some((2, [8.0, 8.0, 16.0, 16.0])), None], emissive: true },
    ],
    &[
        Part { from: [3.0, 5.0, 14.0], to: [13.0, 11.0, 15.0], faces: [Some((0, [10.0, 5.0, 0.0, 6.0])), Some((0, [10.0, 1.0, 0.0, 0.0])), Some((0, [0.0, 0.0, 10.0, 6.0])), Some((0, [0.0, 0.0, 10.0, 6.0])), None, None], emissive: false },
        Part { from: [4.0, 4.0, 15.0], to: [12.0, 12.0, 16.0], faces: [Some((1, [8.0, 8.0, 0.0, 7.0])), Some((1, [8.0, 1.0, 0.0, 0.0])), Some((1, [8.0, 0.0, 16.0, 8.0])), Some((1, [0.0, 0.0, 8.0, 8.0])), Some((1, [0.0, 0.0, 1.0, 8.0])), Some((1, [7.0, 0.0, 8.0, 8.0]))], emissive: false },
        Part { from: [4.0, 4.0, 16.0], to: [12.0, 12.0, 16.0], faces: [None, None, None, Some((2, [8.0, 8.0, 16.0, 16.0])), None, None], emissive: true },
    ],
    &[
        Part { from: [3.0, 14.0, 5.0], to: [13.0, 15.0, 11.0], faces: [Some((0, [10.0, 0.0, 0.0, 6.0])), Some((0, [10.0, 6.0, 0.0, 0.0])), Some((0, [0.0, 0.0, 10.0, 1.0])), Some((0, [0.0, 5.0, 10.0, 6.0])), None, None], emissive: false },
        Part { from: [4.0, 15.0, 4.0], to: [12.0, 16.0, 12.0], faces: [Some((1, [8.0, 0.0, 16.0, 8.0])), Some((1, [0.0, 0.0, 8.0, 8.0])), Some((1, [0.0, 0.0, 8.0, 1.0])), Some((1, [0.0, 7.0, 8.0, 8.0])), Some((1, [0.0, 0.0, 1.0, 8.0])), Some((1, [7.0, 0.0, 8.0, 8.0]))], emissive: false },
        Part { from: [4.0, 16.0, 4.0], to: [12.0, 16.0, 12.0], faces: [None, Some((2, [8.0, 8.0, 16.0, 16.0])), None, None, None, None], emissive: true },
    ],
    &[
        Part { from: [3.0, 1.0, 5.0], to: [13.0, 2.0, 11.0], faces: [Some((0, [10.0, 0.0, 0.0, 6.0])), Some((0, [10.0, 6.0, 0.0, 0.0])), Some((0, [0.0, 0.0, 10.0, 1.0])), Some((0, [0.0, 5.0, 10.0, 6.0])), None, None], emissive: false },
        Part { from: [4.0, 0.0, 4.0], to: [12.0, 1.0, 12.0], faces: [Some((1, [0.0, 0.0, 8.0, 8.0])), Some((1, [8.0, 0.0, 16.0, 8.0])), Some((1, [0.0, 0.0, 8.0, 1.0])), Some((1, [0.0, 7.0, 8.0, 8.0])), Some((1, [0.0, 0.0, 1.0, 8.0])), Some((1, [7.0, 0.0, 8.0, 8.0]))], emissive: false },
        Part { from: [4.0, 0.0, 4.0], to: [12.0, 0.0, 12.0], faces: [Some((2, [8.0, 8.0, 16.0, 16.0])), None, None, None, None, None], emissive: true },
    ],
];

const LEDS: [&[Part]; 6] = [
    &[
        Part { from: [3.0, 7.5, 0.1], to: [3.5, 8.5, 1.0], faces: [Some((3, [2.0, 0.0, 2.5, 0.9])), Some((3, [2.0, 0.0, 2.5, 0.9])), Some((3, [2.0, 0.0, 2.5, 1.0])), Some((3, [2.0, 0.0, 2.5, 1.0])), None, Some((3, [2.0, 0.0, 2.9, 1.0]))], emissive: false },
        Part { from: [12.5, 7.5, 0.1], to: [13.0, 8.5, 1.0], faces: [Some((3, [2.0, 0.0, 2.5, 0.9])), Some((3, [2.0, 0.0, 2.5, 0.9])), Some((3, [2.0, 0.0, 2.5, 1.0])), Some((3, [2.0, 0.0, 2.5, 1.0])), Some((3, [2.0, 0.0, 2.9, 1.0])), None], emissive: false },
    ],
    &[
        Part { from: [15.0, 7.5, 3.0], to: [15.9, 8.5, 3.5], faces: [Some((3, [2.0, 0.0, 2.5, 0.9])), Some((3, [2.0, 0.0, 2.5, 0.9])), None, Some((3, [2.0, 0.0, 2.9, 1.0])), Some((3, [2.0, 0.0, 2.5, 1.0])), Some((3, [2.0, 0.0, 2.5, 1.0]))], emissive: false },
        Part { from: [15.0, 7.5, 12.5], to: [15.9, 8.5, 13.0], faces: [Some((3, [2.0, 0.0, 2.5, 0.9])), Some((3, [2.0, 0.0, 2.5, 0.9])), Some((3, [2.0, 0.0, 2.9, 1.0])), None, Some((3, [2.0, 0.0, 2.5, 1.0])), Some((3, [2.0, 0.0, 2.5, 1.0]))], emissive: false },
    ],
    &[
        Part { from: [0.1, 7.5, 3.0], to: [1.0, 8.5, 3.5], faces: [Some((3, [2.0, 0.0, 2.5, 0.9])), Some((3, [2.0, 0.0, 2.5, 0.9])), None, Some((3, [2.0, 0.0, 2.9, 1.0])), Some((3, [2.0, 0.0, 2.5, 1.0])), Some((3, [2.0, 0.0, 2.5, 1.0]))], emissive: false },
        Part { from: [0.1, 7.5, 12.5], to: [1.0, 8.5, 13.0], faces: [Some((3, [2.0, 0.0, 2.5, 0.9])), Some((3, [2.0, 0.0, 2.5, 0.9])), Some((3, [2.0, 0.0, 2.9, 1.0])), None, Some((3, [2.0, 0.0, 2.5, 1.0])), Some((3, [2.0, 0.0, 2.5, 1.0]))], emissive: false },
    ],
    &[
        Part { from: [3.0, 7.5, 15.0], to: [3.5, 8.5, 15.9], faces: [Some((3, [2.0, 0.0, 2.5, 0.9])), Some((3, [2.0, 0.0, 2.5, 0.9])), Some((3, [2.0, 0.0, 2.5, 1.0])), Some((3, [2.0, 0.0, 2.5, 1.0])), None, Some((3, [2.0, 0.0, 2.9, 1.0]))], emissive: false },
        Part { from: [12.5, 7.5, 15.0], to: [13.0, 8.5, 15.9], faces: [Some((3, [2.0, 0.0, 2.5, 0.9])), Some((3, [2.0, 0.0, 2.5, 0.9])), Some((3, [2.0, 0.0, 2.5, 1.0])), Some((3, [2.0, 0.0, 2.5, 1.0])), Some((3, [2.0, 0.0, 2.9, 1.0])), None], emissive: false },
    ],
    &[
        Part { from: [3.0, 15.0, 7.5], to: [3.5, 15.9, 8.5], faces: [Some((3, [2.0, 0.0, 2.5, 1.0])), Some((3, [2.0, 0.0, 2.5, 1.0])), Some((3, [2.0, 0.0, 2.5, 0.9])), Some((3, [2.0, 0.0, 2.5, 0.9])), None, Some((3, [2.0, 0.0, 2.9, 1.0]))], emissive: false },
        Part { from: [12.5, 15.0, 7.5], to: [13.0, 15.9, 8.5], faces: [Some((3, [2.0, 0.0, 2.5, 1.0])), Some((3, [2.0, 0.0, 2.5, 1.0])), Some((3, [2.0, 0.0, 2.5, 0.9])), Some((3, [2.0, 0.0, 2.5, 0.9])), Some((3, [2.0, 0.0, 2.9, 1.0])), None], emissive: false },
    ],
    &[
        Part { from: [3.0, 0.1, 7.5], to: [3.5, 1.0, 8.5], faces: [Some((3, [2.0, 0.0, 2.5, 1.0])), Some((3, [2.0, 0.0, 2.5, 1.0])), Some((3, [2.0, 0.0, 2.5, 0.9])), Some((3, [2.0, 0.0, 2.5, 0.9])), None, Some((3, [2.0, 0.0, 2.9, 1.0]))], emissive: false },
        Part { from: [12.5, 0.1, 7.5], to: [13.0, 1.0, 8.5], faces: [Some((3, [2.0, 0.0, 2.5, 1.0])), Some((3, [2.0, 0.0, 2.5, 1.0])), Some((3, [2.0, 0.0, 2.5, 0.9])), Some((3, [2.0, 0.0, 2.5, 0.9])), Some((3, [2.0, 0.0, 2.9, 1.0])), None], emissive: false },
    ],
];

/// Os sprites das peças (`#1` a `#4` do modelo): o quadro, as portas, o LED das portas e os LEDs.
const SPRITES: [&str; 4] = [
    "mekanism:block/models/energy_cube",
    "mekanism:block/models/ports",
    "mekanism:block/models/ports_led",
    "mekanism:block/models/leds",
];
/// `lit_uv_shift` do modelo: o LED aceso é a mesma face com o uv deslocado dois texels.
const LIT_UV_SHIFT: f32 = -2.0;
/// O quanto o LED aceso sai da caixa do apagado, em texels, para não brigar com ele na profundidade.
const LIT_GROW: f32 = 0.02;
/// `LightCoordsUtil.FULL_BRIGHT`, em 16 vezes o céu (bits 0 a 8) e o bloco (bits 8 a 16).
const FULL_BRIGHT: u32 = 0xF0F0;
/// O modelo do núcleo (`MekanismModelCache.ENERGY_CORE`).
const CORE_MODEL: &str = "mekanism:block/energy_core";
/// `BaseTier.getPackedColor` (o pixel de `colormap/tiers.png`) de cada tier.
const TIER_COLORS: [u32; 5] = [0x5F_FF_B8, 0xFF_80_6A, 0x4B_F8_FF, 0xF7_87_FF, 0x58_58_58];

/// Os modelos avulsos que o jogo tem de assar para este bloco.
pub(in crate::game) fn standalone_models(models: &mut Vec<String>) {
    if !models.iter().any(|model| model == CORE_MODEL) {
        models.push(CORE_MODEL.to_owned());
    }
}

fn draw_part(game: &Game<'_>, draw: &mut BlockEntityDraw, part: &Part, shift: f32, grow: f32) {
    let faces: [Option<CubeFace<'_>>; 6] = part.faces.map(|face| {
        face.map(|(sprite, uv)| CubeFace { sprite: SPRITES[sprite], uv: Some([uv[0] + shift, uv[1], uv[2] + shift, uv[3]]) })
    });
    let from = part.from.map(|value| value - grow);
    let to = part.to.map(|value| value + grow);
    game.native_draw_cube(draw, from, to, faces, 0xFF_FF_FF);
}

/// O bloco virado como o `blockstate` o vira (as variantes `x` e `y` do modelo, que está de frente para o norte).
fn face_the_front(draw: &mut BlockEntityDraw, facing: Face) {
    const CENTER: [f32; 3] = [0.5, 0.5, 0.5];
    match facing {
        Face::North => {}
        Face::East => draw.rotate_about([0.0, 1.0, 0.0], -90.0, CENTER),
        Face::South => draw.rotate_about([0.0, 1.0, 0.0], 180.0, CENTER),
        Face::West => draw.rotate_about([0.0, 1.0, 0.0], 90.0, CENTER),
        Face::Up => draw.rotate_about([1.0, 0.0, 0.0], 90.0, CENTER),
        Face::Down => draw.rotate_about([1.0, 0.0, 0.0], -90.0, CENTER),
    }
}

thread_local! {
    /// O modelo do núcleo assado para o item, com as faces translúcidas (`Sheets.translucentBlockItemSheet`), e o modelo de onde saiu: o desenhista de item assa de novo
    /// quando o jogo recarrega os modelos.
    static CORE_ITEM_MODEL: RefCell<Option<(Arc<BakedItemModel>, Arc<BakedItemModel>)>> = const { RefCell::new(None) };
}

fn core_item_model(game: &Game<'_>) -> Arc<BakedItemModel> {
    let source = game.pack.item_renderer.bake(CORE_MODEL);
    CORE_ITEM_MODEL.with_borrow_mut(|cache| {
        if let Some((from, made)) = cache
            && Arc::ptr_eq(from, &source)
        {
            return made.clone();
        }
        let mut model = (*source).clone();
        for quad in &mut model.quads {
            quad.material.force_translucent = true;
        }
        let made = Arc::new(model);
        *cache = Some((source, made.clone()));
        made
    })
}

/// `RenderEnergyCubeItem.submit`: o núcleo do item, translúcido, depois do quadro (o item resolvido do jogo, `resolve_item_in_hand`). A tinta é a cor do tier com o alfa da razão
/// de energia guardada (`ARGB.as8BitChannel(ContainerType.ENERGY.getRatio(stack))`), e sem energia não há núcleo. O tempo é o `gameTime` mais o `partialTick`, como no tile.
pub(in crate::game) fn item_core(game: &Game<'_>, stack: &ItemStack, mut state: ItemRenderState) -> ItemRenderState {
    let Some(tier) = tier_of(&stack.item) else {
        return state;
    };
    let Some(frame) = state.layers.first() else {
        return state;
    };
    let capacity = glue::server_config().cubes[tier];
    let stored = basis::item_energy_of(game, stack, rules::ENERGY_COMPONENT);
    let ratio = machinery_basis_core::item_energy::divide_to_level(stored, capacity) as f32;
    if ratio <= 0.0 {
        return state;
    }
    let pose = energy_cube::core_pose(game.world.game_time as f32 + game.clock.fraction() as f32);
    let axis = glam::Vec3::new(0.0, std::f32::consts::FRAC_1_SQRT_2, std::f32::consts::FRAC_1_SQRT_2);
    let matrix = frame.matrix
        * glam::Mat4::from_translation(glam::Vec3::splat(0.5))
        * glam::Mat4::from_scale(glam::Vec3::splat(0.4))
        * glam::Mat4::from_translation(glam::Vec3::new(0.0, pose.bob, 0.0))
        * glam::Mat4::from_rotation_y(pose.yaw.to_radians())
        * glam::Mat4::from_axis_angle(axis, pose.tilt.to_radians());
    let core = LayerRender {
        geometry: LayerGeometry::Quads(core_item_model(game)),
        tints: vec![((energy_cube::core_alpha(ratio) << 24) | TIER_COLORS[tier]) as i32],
        foil: FoilType::None,
        matrix,
        particle: frame.particle.clone(),
        uses_block_light: frame.uses_block_light,
        extents: frame.extents.clone(),
    };
    state.layers.push(core);
    state
}

/// `BlockEntityRenderer.submit` do cubo (`block_entity_submit`).
pub(in crate::game) fn submit(game: &Game<'_>, pos: [i32; 3], draw: &mut BlockEntityDraw) {
    let block = game.native_block(pos);
    let Some(tier) = tier_of(&block) else {
        return;
    };
    let (scale, sides) = crate::game::machinery_tile_sound::tile_view(game.native_dimension(), pos).unwrap_or((0.0, [0; 6]));
    let light = draw.light;
    draw.push();
    face_the_front(draw, basis::front(game, pos));
    for (side, state) in sides.iter().enumerate() {
        if *state == CubeSideState::Inactive as u8 {
            continue;
        }
        for part in PORTS[side] {
            draw.light = if part.emissive { FULL_BRIGHT } else { light };
            draw_part(game, draw, part, 0.0, 0.0);
        }
        if *state == CubeSideState::ActiveLit as u8 {
            draw.light = FULL_BRIGHT;
            for part in LEDS[side] {
                draw_part(game, draw, part, LIT_UV_SHIFT, LIT_GROW);
            }
        }
    }
    draw.light = light;
    draw.pop();
    // `RenderEnergyCube.shouldRender`: o núcleo só aparece com energia.
    if scale > 0.0 {
        let pose = energy_cube::core_pose(draw.game_time as f32 + draw.partial);
        draw.push();
        draw.translate([0.5, 0.5, 0.5]);
        draw.scale([0.4, 0.4, 0.4]);
        draw.translate([0.0, pose.bob, 0.0]);
        draw.rotate_about([0.0, 1.0, 0.0], pose.yaw, [0.0; 3]);
        draw.rotate_about([0.0, std::f32::consts::FRAC_1_SQRT_2, std::f32::consts::FRAC_1_SQRT_2], pose.tilt, [0.0; 3]);
        draw.light = FULL_BRIGHT;
        let start = draw.vertices_mut().len();
        if game.native_draw_model(draw, CORE_MODEL, TIER_COLORS[tier]) {
            // O núcleo é translúcido: a tinta leva a escala no canal alfa (`ARGB.as8BitChannel(getEnergyScale())`).
            let alpha = energy_cube::core_alpha(scale.clamp(0.0, 1.0)) << 24;
            for vertex in &mut draw.vertices_mut()[start..] {
                vertex.color = alpha | (vertex.color & 0x00FF_FFFF);
            }
        }
        draw.light = light;
        draw.pop();
    }
}

/// `BlockEnergyCube.getShape` do cubo posto (`outline_boxes`): o quadro e as placas dos lados habilitados, o `facing` e os lados que o tile mandou (`CubeSideState` diferente
/// de `INACTIVE` é o `slotInfo.isEnabled()` do original). Sem o tile (o cliente ainda não recebeu a cópia), `getShape` cai no índice "norte, tudo habilitado". A colisão
/// segue o modelo estático do bloco (as seis placas): o Horizon só tem a forma de colisão por estado (H92).
pub(in crate::game) fn outline_boxes(game: &Game<'_>, pos: [i32; 3]) -> Option<Vec<[f64; 6]>> {
    let block = game.native_block(pos);
    tier_of(&block)?;
    let (facing, enabled) = match crate::game::machinery_tile_sound::tile_view(game.native_dimension(), pos) {
        Some((_, sides)) => {
            let facing = basis::front(game, pos);
            let enabled = rules::cube_shape::absolute_enabled(facing, |side| {
                let index = machinery_basis_core::side_config::RelativeSide::VALUES.iter().position(|s| *s == side).unwrap_or(0);
                sides[index] != CubeSideState::Inactive as u8
            });
            (facing, enabled)
        }
        None => (Face::North, [true; 6]),
    };
    let origin = [f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2])];
    Some(rules::cube_shape::boxes(facing, enabled).into_iter().map(|b| std::array::from_fn(|i| origin[i % 3] + b[i])).collect())
}

/// O cliente só tem a block entity do cubo, e então a desenha, se o servidor mandar uma cópia.
pub(in crate::game) fn update_tag(entity: &ModBlockEntity, tag: &mut Option<horizon_world::nbt::Compound>) {
    if is_cube(&entity.id) {
        *tag = Some(horizon_world::nbt::Compound::default());
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// A tela (`GuiEnergyCube`): o menu, as abas de lados e de transporte e os pacotes delas. O cubo não é `IUpgradeTile`: a janela de upgrades não existe.

/// A janela do cubo: só dados. A mecânica (abas, janelas de configuração, sessões do menu) é da machinery_basis; `upgrades: false` tira a aba e os slots virtuais.
const fn window_spec_of(block: &'static str) -> UpgradeWindowSpec {
    UpgradeWindowSpec {
        block,
        upgrades: false,
        recipe_book: false,
        book: None,
        book_slots: [0, 0],
        virtual_slots: [SLOTS, SLOTS + 1],
        trackers: rules::cube_trackers,
        window_trackers: rules::upgrade_window_data,
        ns: "mekanism",
        window_type: rules::UPGRADE_WINDOW,
        window_types: &rules::WINDOW_TYPES,
        base_slice: rules::PANEL_SLICE,
        max_multiplier: machinery_basis_core::settings::max_upgrade_multiplier,
        client_config: rules::CLIENT_CONFIG_FILE,
        data: glue::window_upgrade_data,
        color: rules::enum_color,
        item: rules::UPGRADE_ITEM,
        item_component: rules::UPGRADE_TYPE_COMPONENT,
    }
}

static WINDOW_SPECS: [UpgradeWindowSpec; 5] = [
    window_spec_of(rules::CUBE_TIERS[0]),
    window_spec_of(rules::CUBE_TIERS[1]),
    window_spec_of(rules::CUBE_TIERS[2]),
    window_spec_of(rules::CUBE_TIERS[3]),
    window_spec_of(rules::CUBE_TIERS[4]),
];

fn register_windows(block: &str) {
    if let Some(spec) = WINDOW_SPECS.iter().find(|spec| spec.block == block) {
        Game::mb_window_register(spec, remove_upgrade, add_upgrades, window_source);
        Game::mb_config_register(spec.block, config_view, config_apply, config_snapshot);
    }
}

/// O cubo não tem upgrades: o item de upgrade não entra e nada sai.
fn remove_upgrade(_: &mut Game<'_>, _: [i32; 3], _: &str, _: bool) {}

fn add_upgrades(_: &mut Game<'_>, _: [i32; 3], _: &str, _: i32) -> i32 {
    0
}

/// Os dados do menu do cubo (`addContainerTrackers`), lidos do objeto.
struct CubeSync<'a> {
    cube: &'a EnergyCube,
    security: machinery_basis_core::security::SecuritySync,
}

impl TrackedSource for CubeSync<'_> {
    fn value(&self, id: u16) -> Value {
        let redstone = self.cube.tile().redstone_sync();
        match id {
            source::SECURITY_FREQUENCY => {
                let (hash, bytes) = self.security.frequency();
                Value::Codec { hash, bytes }
            }
            source::SECURITY_MODE => Value::Enum(self.security.mode()),
            source::CONTROL_TYPE => Value::Enum(redstone.control_type.ordinal()),
            source::POWERED => Value::Boolean(redstone.powered),
            source::WAS_POWERED => Value::Boolean(redstone.was_powered),
            source::LAST_ENERGY_RECEIVED => Value::Long(self.cube.last_energy_received()),
            source::ENERGY => Value::Long(self.cube.energy().amount()),
            other => panic!("fonte {other} desconhecida"),
        }
    }
}

/// O valor vivo dos dados do menu do cubo em `pos` (`MekanismContainer.broadcastChanges`): o objeto vivo, ou, até o primeiro tick do tile, o que o NBT dá
/// (`menu_source`). Um aviso de redstone que o tile ainda não leu entra na hora, como o `updatePower` do Java.
fn window_source(game: &mut Game<'_>, pos: [i32; 3], act: &mut dyn FnMut(&dyn TrackedSource)) -> bool {
    let at = (pos[0], pos[1], pos[2]);
    let ticked = match game.world.block_entities.get(&at) {
        Some(BlockEntity::Mod(entity)) if is_cube(&entity.id) => !entity.data.is_empty(),
        _ => return false,
    };
    let power = if ticked { basis::take_power(game, pos) } else { basis::peek_power(game, pos) };
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&at) else {
        return false;
    };
    let Some(tier) = tier_of(&entity.id) else {
        return false;
    };
    let security = basis::security_sync(game, entity);
    let key = (game.native_dimension(), pos);
    let mut loaded = None;
    LIVE_CUBES.with_borrow_mut(|cubes| {
        let live = cubes.get_mut(&key).map(|live| &mut live.cube);
        let Some(cube) = menu_source::object(ticked, live, &mut loaded, || born(game, pos, entity, tier).cube) else {
            return false;
        };
        if let Some(powered) = power {
            cube.tile_mut().update_power(powered);
        }
        act(&CubeSync { cube, security });
        true
    })
}

/// O que as janelas de configuração leem do cubo em `pos`: a configuração e o ejetor do NBT do tile, e a frente de agora.
fn config_view(game: &Game<'_>, pos: [i32; 3]) -> Option<ConfigView> {
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(pos[0], pos[1], pos[2])) else {
        return None;
    };
    tier_of(&entity.id)?;
    let spec = energy_cube::side_config_spec();
    let mut config = TileComponentConfig::new(&spec.types);
    (spec.setup)(&mut config);
    let mut ejector = TileComponentEjector::new();
    ejector.set_output_data(&config, &spec.eject);
    let facing = basis::front(game, pos);
    let nbt = basis::read_side_nbt(entity);
    if let Some(data) = &nbt.config {
        config.deserialize(data, facing, false);
    }
    if let Some(data) = &nbt.ejector {
        ejector.deserialize(data);
    }
    Some(ConfigView { config, ejector, facing })
}

/// O estado vivo da configuração e do ejetor, que o servidor manda a quem tem uma janela de configuração aberta.
fn config_snapshot(game: &mut Game<'_>, pos: [i32; 3]) -> Option<Vec<u8>> {
    let key = (game.native_dimension(), pos);
    let live = LIVE_CUBES.with_borrow(|cubes| cubes.get(&key).map(|live| config_window::encode_snapshot(live.cube.side_config(), live.cube.ejector())));
    if live.is_some() {
        return live;
    }
    let view = config_view(game, pos)?;
    Some(config_window::encode_snapshot(&view.config, &view.ejector))
}

/// O `handle` dos pacotes de configuração no servidor: aplica ao cubo (o objeto vivo, ou um feito do NBT) e grava a configuração no NBT na hora, como `sideChanged`.
fn config_apply(game: &mut Game<'_>, pos: [i32; 3], packet: ConfigPacket) {
    let facing = basis::front(game, pos);
    with_cube(game, pos, |cube| {
        cube.set_facing(facing);
        let (config, ejector) = cube.side_parts_mut();
        packet.apply(config, ejector, facing);
    });
}

/// `InventoryContainerSlot.mayPlace` do cubo aberto: o `isValidForInsertion(item, MANUAL)` do slot, com a energia que a pilha do clique guarda. O hook do Horizon
/// roda dentro do menu, sem o jogo: a regra leva um cubo feito do NBT agora.
pub(in crate::game) fn publish_slot_rules(game: &Game<'_>, pos: [i32; 3], owner: &'static str) {
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(pos[0], pos[1], pos[2])) else {
        return;
    };
    let Some(tier) = tier_of(&entity.id) else {
        return;
    };
    let cube = born(game, pos, entity, tier).cube;
    slot_rules::set(
        owner,
        &entity.id,
        Rc::new(move |slot, item, stack| {
            let energy = stack
                .downcast_ref::<ItemStack>()
                .and_then(|stack| stack.data()?.other(rules::ENERGY_COMPONENT).cloned())
                .map_or(0, |tag| match tag {
                    Tag::Long(value) => value.max(0),
                    Tag::Int(value) => i64::from(value).max(0),
                    _ => 0,
                });
            let resource = if energy == 0 {
                ItemResource::new(item, 64)
            } else {
                let text = energy.to_string();
                ItemResource::with_components(item, 64, Components::new([(rules::ENERGY_COMPONENT, text.as_str())]))
            };
            cube.inventory_slots().get(slot).is_some_and(|held| held.is_valid_for_insertion(&resource, AutomationType::Manual))
        }),
    );
}
