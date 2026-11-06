// As máquinas do Mekanism no jogo. As regras ficam em mekanism_rules, e o que é genérico em
// machinery_basis.
use crate::game::mekanism_rules as rules;
use crate::game::{Game, GameLevel, machinery_basis as basis};
use horizon_client::containers::{ModBlockEntity, ModContainer, ModEnergyBuffer};
use horizon_client::crafting::Ingredient;
use horizon_client::hud::HudBatch;
use horizon_client::items::ItemStack;
use horizon_client::lang;
use horizon_client::mod_api::NativeScreen;
use horizon_world::nbt::{Compound, JavaString, Tag};
use horizon_world::registry::BlockStateId;
use machinery_basis_core::cached_recipe::{ItemIngredient, ItemOutput, ItemToItemRecipe};
use machinery_basis_core::chemical::{ChemicalConversion, ChemicalIngredient, ChemicalResource, TankContents};
use machinery_basis_core::containers::{InventorySlot, ItemResource};
use machinery_basis_core::data_slots;
use machinery_basis_core::item_handler::{ItemCapabilities, ItemHandler, RuledHandler, VanillaContainer};
use machinery_basis_core::sides::Face;
use machinery_basis_core::resource;
use machinery_basis_core::tile::{ProgressMachine, RECIPE_CHECK_FREQUENCY, RedstoneControl, SavedState};
use machinery_basis_core::two_input_recipe::ItemChemicalToItemRecipe;
use machinery_basis_core::units::format_energy;
use basis::RecipeSource::{Cooking, Custom};
use std::cell::{Cell, RefCell};
use horizon_core::geometry::Direction;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const DUMP_BUTTON: i32 = 0;
const INFUSING: basis::RecipeSource = Custom(rules::INFUSING, rules::INFUSING_FIELDS);

/// Onde cada slot fica no `ModContainer`, na ordem de `getInitialInventory` de cada máquina.
struct Slots {
    input: usize,
    output: usize,
    energy: usize,
    /// O slot que enche o tanque de químico, nas máquinas que têm tanque.
    infusion: Option<usize>,
}

/// Uma máquina de progresso: as quatro só mudam no id, nas receitas, no consumo e nos slots.
struct Machine {
    block: &'static str,
    title: &'static str,
    recipes: basis::RecipeSource,
    energy_per_tick: i32,
    slots: Slots,
}

const ELECTRIC_SLOT_LAYOUT: Slots = Slots {
    input: rules::ELECTRIC_INPUT_SLOT,
    output: rules::ELECTRIC_OUTPUT_SLOT,
    energy: rules::ELECTRIC_ENERGY_SLOT,
    infusion: None,
};

const fn electric(
    block: &'static str,
    title: &'static str,
    recipes: basis::RecipeSource,
    energy_per_tick: i32,
) -> Machine {
    Machine {
        block,
        title,
        recipes,
        energy_per_tick,
        slots: ELECTRIC_SLOT_LAYOUT,
    }
}

static MACHINES: [Machine; 4] = [
    electric(
        rules::ENRICHMENT_CHAMBER,
        "container.mekanism.enrichment_chamber",
        Custom(rules::ENRICHING, rules::ELECTRIC_FIELDS),
        rules::ENRICHMENT_USAGE,
    ),
    electric(
        rules::CRUSHER,
        "container.mekanism.crusher",
        Custom(rules::CRUSHING, rules::ELECTRIC_FIELDS),
        rules::CRUSHER_USAGE,
    ),
    electric(
        rules::ENERGIZED_SMELTER,
        "container.mekanism.energized_smelter",
        Cooking(rules::SMELTING),
        rules::SMELTER_USAGE,
    ),
    Machine {
        block: rules::INFUSER,
        title: "container.mekanism.metallurgic_infuser",
        recipes: INFUSING,
        energy_per_tick: rules::INFUSER_USAGE,
        slots: Slots {
            input: rules::INPUT_SLOT,
            output: rules::OUTPUT_SLOT,
            energy: rules::ENERGY_SLOT,
            infusion: Some(rules::INFUSION_SLOT),
        },
    },
];

// Os data slots das telas.
const DATA_ENERGY: usize = 0;
const DATA_INFUSION: usize = 2;
const DATA_COLOR: usize = 3;
const DATA_PROGRESS: usize = 5;
const DATA_SLOTS: usize = 6;
const ELECTRIC_DATA_PROGRESS: usize = 2;
const ELECTRIC_DATA_SLOTS: usize = 3;

// As telas, nas posições de GuiMetallurgicInfuser e GuiElectricMachine. Um slot guarda a posição do
// item, e a moldura fica um pixel antes.
const WIDTH: f32 = 176.0;
const HEIGHT: f32 = 166.0;
const SLOT: &str = "mekanism:gui/sprites/slot/normal";
const ENERGY_SLOT_FRAME: &str = "mekanism:gui/sprites/slot/power";
const PROGRESS_EMPTY: &str = "mekanism:gui/sprites/progress/right/empty";
const PROGRESS_FULL: &str = "mekanism:gui/sprites/progress/right/full";
const BAR_EMPTY: &str = "mekanism:gui/sprites/progress/bar/empty";
const BAR_FULL: &str = "mekanism:gui/sprites/progress/bar/full";
const UP_ARROW: &str = "mekanism:gui/sprites/arrow/up";
const DUMP_SPRITE: &str = "mekanism:gui/sprites/button/dump";
const PANEL: basis::PanelStyle = basis::PanelStyle {
    base: "mekanism:gui/sprites/base",
    base_size: 16.0,
    base_border: 4.0,
    slot: SLOT,
};
const SLOT_SIZE: f32 = 18.0;
const SLOT_POSITIONS: [[f32; 2]; rules::SLOTS] = [[17.0, 35.0], [51.0, 43.0], [109.0, 43.0], [143.0, 35.0]];
const ELECTRIC_SLOT_POSITIONS: [[f32; 2]; rules::ELECTRIC_SLOTS] = [[64.0, 17.0], [116.0, 35.0], [64.0, 53.0]];
const INFUSION_BAR: [f32; 4] = [7.0, 15.0, 6.0, 54.0];
const ENERGY_BAR: [f32; 4] = [164.0, 15.0, 6.0, 54.0];
const ELECTRIC_ENERGY_BAR: [f32; 4] = [164.0, 16.0, 6.0, 54.0];
const PROGRESS_BAR: [f32; 4] = [72.0, 47.0, 32.0, 8.0];
const ELECTRIC_PROGRESS_BAR: [f32; 4] = [86.0, 38.0, 25.0, 9.0];
const ELECTRIC_ARROW: [f32; 4] = [68.0, 38.0, 8.0, 10.0];
const DUMP_RECT: [f32; 4] = [16.0, 59.0, 21.0, 10.0];
const ENERGY_COLOR: u32 = 0xFF_1E_B4_32;
const OPAQUE: u32 = 0xFF_00_00_00;

fn is_infuser(block: &str) -> bool {
    block == rules::INFUSER
}

fn machine_of(block: &str) -> Option<&'static Machine> {
    MACHINES.iter().find(|machine| machine.block == block)
}

/// As máquinas de um item só, com a tela de `GuiElectricMachine`.
fn electric_machine(block: &str) -> Option<&'static Machine> {
    machine_of(block).filter(|machine| machine.slots.infusion.is_none())
}

fn is_machine(block: &str) -> bool {
    machine_of(block).is_some()
}

fn is_cable(block: &str) -> bool {
    rules::cable_capacity(block).is_some()
}

fn inside(rect: [f32; 4], at: [f32; 2]) -> bool {
    let [x, y, w, h] = rect;
    (x..x + w).contains(&at[0]) && (y..y + h).contains(&at[1])
}

fn sprite_rect(game: &Game<'_>, batch: &mut HudBatch, sprite: &str, rect: [f32; 4]) {
    let [_, _, w, h] = rect;
    game.native_sprite(batch, sprite, [0.0, 0.0, w, h], rect);
}

pub(in crate::game) fn block_entity(block: &str, entity: &mut Option<(String, Compound)>) {
    if (is_machine(block) || is_cable(block)) && entity.is_none() {
        *entity = Some((block.to_owned(), Compound::default()));
    }
}

pub(in crate::game) fn ticks(block: &str, ticks: &mut bool) {
    if is_machine(block) || is_cable(block) {
        *ticks = true;
    }
}

pub(in crate::game) fn energy(block: &str, energy: &mut Option<ModEnergyBuffer>) {
    if let Some(capacity) = rules::cable_capacity(block) {
        *energy = Some(ModEnergyBuffer {
            capacity,
            max_insert: capacity,
            insert_faces: [true; 6],
            ..Default::default()
        });
        return;
    }
    let Some(machine) = machine_of(block) else {
        return;
    };
    let usage = machine.energy_per_tick;
    // `LightConfigInfo.INPUT_ONLY`: energia entra por todas as faces.
    *energy = Some(ModEnergyBuffer {
        capacity: rules::energy_capacity(usage),
        max_insert: rules::energy_capacity(usage),
        insert_faces: [true; 6],
        ..Default::default()
    });
}

pub(in crate::game) fn container(block: &str, container: &mut Option<ModContainer>) {
    let slots = match machine_of(block) {
        Some(machine) if machine.slots.infusion.is_some() => rules::SLOTS,
        Some(_) => rules::ELECTRIC_SLOTS,
        None => return,
    };
    // Sem interface vanilla: hoppers passam pela regra da machinery_basis.
    *container = Some(ModContainer {
        slots,
        menu: Some(block.to_owned()),
        ..Default::default()
    });
}

fn native_screen(positions: &[[f32; 2]]) -> NativeScreen {
    NativeScreen {
        width: WIDTH,
        height: HEIGHT,
        slots: positions.iter().map(|&position| Some(position)).collect(),
        inventory_y: 84.0,
        inventory_x: 8.0,
        own_labels: true,
        own_background: true,
    }
}

pub(in crate::game) fn screen(block: &str, screen: &mut Option<NativeScreen>) {
    if is_infuser(block) {
        *screen = Some(native_screen(&SLOT_POSITIONS));
    } else if electric_machine(block).is_some() {
        *screen = Some(native_screen(&ELECTRIC_SLOT_POSITIONS));
    }
}

// Os objetos vivos das máquinas de progresso (o desenho do Java, com contêineres e receita em cache como objetos).
//
// Moram aqui, numa tabela por thread, uma por block entity, na chave (dimensão, posição). O Horizon entrega a block entity
// por `&mut` só durante o tick, e `ModBlockEntity` guarda `Items`, `nbt` e `data`, sem lugar para um objeto vivo: então o
// objeto é um cache, e a verdade que sobrevive é a `ModBlockEntity`.
//
// - Nasce no primeiro tick do bloco, ou no primeiro depois de uma lacuna (o chunk saiu, o mundo foi salvo e reaberto, o bloco
//   foi trocado): refeito dos itens, da energia e do progresso que a `ModBlockEntity` tem. É o `deserialize` do Java.
// - Vive enquanto o tick seguinte for o do tick seguinte do jogo. Todo tick começa lendo da `ModBlockEntity` o que mudou
//   fora dele (a tela, um hopper, um cabo), como um `setContents` do Java, que dispara os listeners; e termina escrevendo o
//   que mudou, em tags simples com os nomes que o Java grava. Salvar, abrir a tela e mandar energia funcionam sem o objeto saber.
// - Morre pela varredura: sem andar por `STALE_TICKS`, o que acontece com bloco quebrado ou chunk descarregado, o objeto sai
//   da tabela. Um bloco novo na mesma posição não herda nada: sem a chave `redstone` na `ModBlockEntity` (o Java a grava sempre), nasce outro.
thread_local! {
    static LIVE_MACHINES: RefCell<HashMap<(&'static str, [i32; 3]), Live>> = RefCell::new(HashMap::new());
    static COMPARATOR_LEVELS: RefCell<HashMap<(&'static str, [i32; 3]), i32>> = RefCell::new(HashMap::new());
    static COMPARATOR_QUEUE: RefCell<Vec<(&'static str, [i32; 3])>> = const { RefCell::new(Vec::new()) };
    static LAST_SWEEP: Cell<i64> = const { Cell::new(i64::MIN) };
}

/// Os nomes de `SerializationConstants` que o Java grava, em tags simples no NBT da `ModBlockEntity`. `REDSTONE` é a
/// marca de que o tile já foi salvo: o Java a escreve sempre.
const REDSTONE: &str = "redstone";
const CONTROL_TYPE: &str = "control_type";
const ACTIVE_STATE: &str = "active_state";
const UPDATE_DELAY: &str = "update_delay";
const CURRENT_REDSTONE: &str = "current_redstone";
const PROGRESS: &str = "progress";
const USED_SO_FAR: &str = "used_so_far";
const RECIPES_USED: &str = "recipes_used";
const LAST_RECIPE_USED: &str = "last_recipe_used";
const LAST_RECIPE_AMOUNT: &str = "last_recipe_amount";
/// `SerializationConstants.EJECT + TransmissionType.ITEM.ordinal()`: ENERGY, FLUID, CHEMICAL, ITEM.
const EJECT_ITEM: &str = "eject3";
/// A chave que liga o ejetor por comando: `/data merge block x y z {mekanism_eject:1}`.
const EJECT_SWITCH: &str = "mekanism_eject";
/// `ChemicalContainerType.getTag()` e o que `IListContainerType.saveTo` e `LargeResourceStack.codec` gravam: uma lista de
/// `{container: byte, stored: {type, amount}}`, sem o item da lista quando o tanque está vazio, e sem a lista inteira.
const CHEMICAL_TANKS: &str = "chemical_tanks";
const CONTAINER: &str = "container";
const STORED: &str = "stored";
const TYPE: &str = "type";
const AMOUNT: &str = "amount";
/// De quanto em quanto tempo a tabela é varrida, e quanto sem andar um objeto vive.
const SWEEP_TICKS: i64 = 200;
const STALE_TICKS: i64 = 100;
/// Quando o item não é conhecido, a pilha dele.
const UNKNOWN_STACK: u32 = 64;

struct Live {
    machine: ProgressMachine,
    /// As receitas de onde as da máquina saíram: uma carga nova de dados as troca.
    recipes: Arc<Vec<basis::MachineRecipe>>,
    /// As conversões de item em químico, de onde as da máquina saíram, nas máquinas com tanque.
    conversions: Option<Arc<Vec<basis::ItemConversion>>>,
    last_seen: i64,
    /// O que já está no NBT, para só escrever quando muda.
    saved: Option<SavedState>,
    used_version: Option<u32>,
    /// O `lit` que o bloco tem, para só escrever quando muda.
    /// O nível que os comparadores já viram.
    reported_level: Option<i32>,
    lit: Option<bool>,
    /// A face de saída do ejetor, a direita da frente.
    eject_side: Option<Face>,
}

fn resource_of(game: &Game<'_>, stack: &ItemStack) -> ItemResource {
    let limit = game.native_max_stack_size(&stack.item).unwrap_or(UNKNOWN_STACK);
    ItemResource::new(&stack.item, limit)
}

fn recipe_ingredient(recipe: &basis::MachineRecipe) -> &Ingredient {
    &recipe.item
}

fn conversion_ingredient(conversion: &basis::ItemConversion) -> &Ingredient {
    &conversion.item
}

/// O ingrediente do Horizon (item ou tag) vira o predicado da lib, e guarda a última resposta, porque a receita em cache
/// pergunta todo tick pelo mesmo item. `source` e `index` dizem de qual entrada da lista ele sai.
fn item_ingredient<T: 'static>(source: &Arc<Vec<T>>, index: usize, ingredient: fn(&T) -> &Ingredient) -> ItemIngredient {
    let source = source.clone();
    let last: RefCell<Option<(Rc<str>, bool)>> = RefCell::new(None);
    ItemIngredient::matching(move |resource| {
        if let Some((id, answer)) = &*last.borrow()
            && *id == resource.id
        {
            return *answer;
        }
        let stack = ItemStack {
            item: Arc::from(&*resource.id),
            count: 1,
            damage: 0,
            extra: None,
        };
        let answer = ingredient(&source[index]).test(&stack);
        *last.borrow_mut() = Some((resource.id.clone(), answer));
        answer
    })
}

/// `ItemToItemRecipe` das receitas do jogo.
fn core_recipes(game: &Game<'_>, recipes: &Arc<Vec<basis::MachineRecipe>>) -> Vec<Rc<ItemToItemRecipe>> {
    let mut converted = Vec::with_capacity(recipes.len());
    for (index, recipe) in recipes.iter().enumerate() {
        converted.push(Rc::new(ItemToItemRecipe {
            id: recipe.id.clone(),
            input: item_ingredient(recipes, index, recipe_ingredient),
            input_count: recipe.count,
            output: ItemOutput {
                resource: resource_of(game, &recipe.output),
                count: recipe.output.count,
            },
        }));
    }
    converted
}

/// `ItemChemicalToItemRecipe` das receitas do jogo. O químico de uma receita é um id ou uma tag, e a tag vale como estava na
/// carga de dados em que a receita foi lida: uma carga nova refaz as receitas. A receita que gasta o químico a cada tick
/// (`per_tick_usage`) fica de fora, porque pede `ItemStackConstantChemicalToObjectCachedRecipe`, que a lib não porta.
fn core_chemical_recipes(game: &Game<'_>, recipes: &Arc<Vec<basis::MachineRecipe>>) -> Vec<Rc<ItemChemicalToItemRecipe>> {
    let tags = resource::current_tags();
    let mut converted = Vec::with_capacity(recipes.len());
    for (index, recipe) in recipes.iter().enumerate() {
        let Some(chemical) = recipe.resource.clone() else {
            continue;
        };
        if recipe.per_tick_usage {
            eprintln!("mekanism: {} gasta o químico a cada tick, o que o Metallurgic Infuser ainda não porta", recipe.id);
            continue;
        }
        let amount = u32::try_from(chemical.amount).unwrap_or(u32::MAX);
        let tags = tags.clone();
        converted.push(Rc::new(ItemChemicalToItemRecipe {
            id: recipe.id.clone(),
            item_input: item_ingredient(recipes, index, recipe_ingredient),
            item_count: recipe.count,
            chemical_input: ChemicalIngredient::matching(move |resource| chemical.accepts(&resource.id, &tags)),
            chemical_amount: amount,
            output: ItemOutput {
                resource: resource_of(game, &recipe.output),
                count: recipe.output.count,
            },
        }));
    }
    converted
}

/// `ChemicalConversion` das conversões de item em químico do jogo.
fn core_conversions(conversions: &Arc<Vec<basis::ItemConversion>>) -> Vec<ChemicalConversion> {
    let mut converted = Vec::with_capacity(conversions.len());
    for (index, conversion) in conversions.iter().enumerate() {
        converted.push(ChemicalConversion {
            input: item_ingredient(conversions, index, conversion_ingredient),
            input_count: conversion.count,
            output: ChemicalResource::new(&conversion.resource),
            amount: i32::try_from(conversion.amount).unwrap_or(i32::MAX),
        });
    }
    converted
}

fn has_saved(entity: &ModBlockEntity) -> bool {
    entity.nbt.contains_key(&JavaString::from(REDSTONE))
}

fn nbt_int(entity: &ModBlockEntity, key: &str) -> i32 {
    match entity.nbt.get(&JavaString::from(key)) {
        Some(Tag::Int(value)) => *value,
        _ => 0,
    }
}

fn nbt_bool(entity: &ModBlockEntity, key: &str) -> bool {
    matches!(entity.nbt.get(&JavaString::from(key)), Some(Tag::Byte(value)) if *value != 0)
}

fn nbt_string(entity: &ModBlockEntity, key: &str) -> Option<String> {
    match entity.nbt.get(&JavaString::from(key)) {
        Some(Tag::String(value)) => Some(value.to_string_lossy()),
        _ => None,
    }
}

/// `loadAdditional` do `TileEntityMekanism` e do monitor, do que a `ModBlockEntity` guarda.
fn read_saved(entity: &ModBlockEntity) -> SavedState {
    let mut recipes_used = BTreeMap::new();
    if let Some(Tag::Compound(used)) = entity.nbt.get(&JavaString::from(RECIPES_USED)) {
        for (id, amount) in used {
            if let Tag::Int(amount) = amount {
                recipes_used.insert(id.to_string_lossy(), *amount);
            }
        }
    }
    SavedState {
        redstone: nbt_bool(entity, REDSTONE),
        control_type: RedstoneControl::from_serialized_name(
            &nbt_string(entity, CONTROL_TYPE).unwrap_or_default(),
        ),
        active: nbt_bool(entity, ACTIVE_STATE),
        update_delay: nbt_int(entity, UPDATE_DELAY),
        current_redstone: nbt_int(entity, CURRENT_REDSTONE),
        progress: nbt_int(entity, PROGRESS),
        used_so_far: nbt_int(entity, USED_SO_FAR),
        recipes_used,
        last_recipe_used: nbt_string(entity, LAST_RECIPE_USED),
        last_recipe_amount: nbt_int(entity, LAST_RECIPE_AMOUNT),
        ejecting: nbt_bool(entity, EJECT_ITEM),
    }
}

fn put(entity: &mut ModBlockEntity, key: &str, tag: Tag) {
    entity.nbt.insert(JavaString::from(key), tag);
}

/// `saveAdditional` do que muda a cada tick, só dos campos que mudaram desde `old`. `used_so_far` só o Metallurgic Infuser grava.
fn write_scalars(entity: &mut ModBlockEntity, old: Option<&SavedState>, new: &SavedState, with_used_so_far: bool) {
    let changed = |same: fn(&SavedState, &SavedState) -> bool| old.is_none_or(|old| !same(old, new));
    if changed(|a, b| a.redstone == b.redstone) {
        put(entity, REDSTONE, Tag::Byte(i8::from(new.redstone)));
    }
    if changed(|a, b| a.control_type == b.control_type) {
        let name = JavaString::from(new.control_type.serialized_name());
        put(entity, CONTROL_TYPE, Tag::String(name));
    }
    if changed(|a, b| a.active == b.active) {
        put(entity, ACTIVE_STATE, Tag::Byte(i8::from(new.active)));
    }
    if changed(|a, b| a.update_delay == b.update_delay) {
        put(entity, UPDATE_DELAY, Tag::Int(new.update_delay));
    }
    if changed(|a, b| a.current_redstone == b.current_redstone) {
        put(entity, CURRENT_REDSTONE, Tag::Int(new.current_redstone));
    }
    if changed(|a, b| a.progress == b.progress) {
        put(entity, PROGRESS, Tag::Int(new.progress));
    }
    if with_used_so_far && changed(|a, b| a.used_so_far == b.used_so_far) {
        put(entity, USED_SO_FAR, Tag::Int(new.used_so_far));
    }
    if changed(|a, b| a.ejecting == b.ejecting) {
        put(entity, EJECT_ITEM, Tag::Byte(i8::from(new.ejecting)));
    }
}

/// `readFrom` do `IListContainerType` para o tanque 0: as entradas com `container` 0, sem checar a capacidade (o Java lê o
/// tanque "sem checar", para um conteúdo que deixou de ser válido não derrubar o carregamento). Uma entrada que não se lê vale vazio.
fn read_tank(entity: &ModBlockEntity) -> TankContents {
    let Some(Tag::List(list)) = entity.nbt.get(&JavaString::from(CHEMICAL_TANKS)) else {
        return TankContents::default();
    };
    let mut contents = TankContents::default();
    for entry in list {
        let Tag::Compound(entry) = entry else {
            continue;
        };
        if !matches!(entry.get(&JavaString::from(CONTAINER)), Some(Tag::Byte(0))) {
            continue;
        }
        contents = match entry.get(&JavaString::from(STORED)) {
            Some(Tag::Compound(stored)) => stored_contents(stored),
            _ => TankContents::default(),
        };
    }
    contents
}

/// `LargeResourceStack.codec`: `type` (o id do químico) e `amount` (um `long` que não é negativo).
fn stored_contents(stored: &Compound) -> TankContents {
    let id = match stored.get(&JavaString::from(TYPE)) {
        Some(Tag::String(id)) => id.to_string_lossy(),
        _ => return TankContents::default(),
    };
    let amount = match stored.get(&JavaString::from(AMOUNT)) {
        Some(Tag::Long(amount)) => u64::try_from(*amount).ok(),
        Some(Tag::Int(amount)) => u64::try_from(*amount).ok(),
        _ => None,
    };
    match amount {
        Some(amount) => TankContents::new(Some(ChemicalResource::new(&id)), amount),
        None => TankContents::default(),
    }
}

/// `saveTo` do `IListContainerType` para o tanque 0, que só grava o que não está vazio.
fn write_tank(entity: &mut ModBlockEntity, contents: &TankContents) {
    let key = JavaString::from(CHEMICAL_TANKS);
    let Some(resource) = &contents.resource else {
        entity.nbt.remove(&key);
        return;
    };
    let mut stored = Compound::default();
    stored.insert(JavaString::from(TYPE), Tag::String(JavaString::from(&*resource.id)));
    stored.insert(JavaString::from(AMOUNT), Tag::Long(i64::try_from(contents.amount).unwrap_or(i64::MAX)));
    let mut entry = Compound::default();
    entry.insert(JavaString::from(CONTAINER), Tag::Byte(0));
    entry.insert(JavaString::from(STORED), Tag::Compound(stored));
    entity.nbt.insert(key, Tag::List(vec![Tag::Compound(entry)]));
}

/// O tanque da versão anterior ficava em `machinery_basis.tanks`. Num bloco que ainda só tem esse, leva o conteúdo para o
/// formato do Java, e tira o antigo, para um tanque esvaziado não voltar a encher.
fn migrate_old_tank(entity: &mut ModBlockEntity) {
    if entity.nbt.contains_key(&JavaString::from(CHEMICAL_TANKS)) {
        return;
    }
    let old = basis::load_tanks(entity, &[rules::TANK_CAPACITY]);
    if let Some(resource) = old[0].resource() {
        let contents = TankContents::new(Some(ChemicalResource::new(resource)), old[0].amount());
        write_tank(entity, &contents);
    }
    basis::discard_tanks(entity);
}

/// `saveAdditional` do monitor: as receitas usadas, a última e quanto dela saiu. Como no Java, a quantidade só vai se
/// for maior que zero, e a última só se existe.
fn write_used(entity: &mut ModBlockEntity, new: &SavedState) {
    let mut used = Compound::default();
    for (id, amount) in &new.recipes_used {
        used.insert(JavaString::from(id.as_str()), Tag::Int(*amount));
    }
    put(entity, RECIPES_USED, Tag::Compound(used));
    match &new.last_recipe_used {
        Some(id) => put(entity, LAST_RECIPE_USED, Tag::String(JavaString::from(id.as_str()))),
        None => {
            entity.nbt.remove(&JavaString::from(LAST_RECIPE_USED));
        }
    }
    if new.last_recipe_amount > 0 {
        put(entity, LAST_RECIPE_AMOUNT, Tag::Int(new.last_recipe_amount));
    } else {
        entity.nbt.remove(&JavaString::from(LAST_RECIPE_AMOUNT));
    }
}

/// `ProgressMachine` nova, do que a `ModBlockEntity` guarda. Com `conversions` é o Metallurgic Infuser.
fn born(
    game: &Game<'_>,
    pos: [i32; 3],
    entity: &ModBlockEntity,
    machine: &Machine,
    recipes: &Arc<Vec<basis::MachineRecipe>>,
    conversions: Option<&Arc<Vec<basis::ItemConversion>>>,
) -> Live {
    // `ThreadLocalRandom.nextInt(RECIPE_CHECK_FREQUENCY)`: o sorteio de `shouldRecheckAllErrors`.
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| u64::from(since.subsec_nanos()));
    let check_offset = nanos % RECIPE_CHECK_FREQUENCY;
    let mut progress = match conversions {
        None => ProgressMachine::electric(rules::electric_config(
            machine.energy_per_tick,
            core_recipes(game, recipes),
            check_offset,
        )),
        Some(conversions) => ProgressMachine::chemical(rules::infuser_config(
            machine.energy_per_tick,
            core_chemical_recipes(game, recipes),
            core_conversions(conversions),
            check_offset,
        )),
    };
    let block_active = game.native_block_property(pos, "lit").as_deref() == Some("true");
    progress.restore(&read_saved(entity), block_active);
    Live {
        machine: progress,
        recipes: recipes.clone(),
        conversions: conversions.cloned(),
        last_seen: i64::MIN,
        saved: None,
        used_version: None,
        reported_level: None,
        lit: None,
        eject_side: None,
    }
}

/// O que a `ModBlockEntity` mudou fora do tick vira `setContents` no slot, que dispara os listeners como o Java.
fn slot_from_entity(game: &Game<'_>, slot: &InventorySlot, stack: Option<&ItemStack>) {
    match stack {
        None => slot.set_contents(None, 0, None),
        Some(stack) => {
            let same = slot.resource().is_some_and(|held| *held.id == *stack.item) && slot.amount() == stack.count;
            if !same {
                slot.set_contents(Some(resource_of(game, stack)), stack.count, None);
            }
        }
    }
}

/// O que o tick mudou vai para a `ModBlockEntity`. Uma pilha do mesmo item guarda o que tinha (dano, componentes); um item
/// novo é o da saída de uma receita.
fn slot_to_entity(entity: &mut ModBlockEntity, index: usize, slot: &InventorySlot, recipes: &[basis::MachineRecipe]) {
    let contents = slot.contents();
    let current = entity.items[index].as_ref();
    let same = match (&contents.resource, current) {
        (None, None) => true,
        (Some(resource), Some(stack)) => *resource.id == *stack.item && contents.amount == stack.count,
        _ => false,
    };
    if same {
        return;
    }
    let new = contents.resource.map(|resource| {
        let mut stack = match current {
            Some(old) if *old.item == *resource.id => old.clone(),
            _ => recipes
                .iter()
                .find(|recipe| *recipe.output.item == *resource.id)
                .map_or_else(
                    || ItemStack {
                        item: Arc::from(&*resource.id),
                        count: 1,
                        damage: 0,
                        extra: None,
                    },
                    |recipe| recipe.output.clone(),
                ),
        };
        stack.count = contents.amount;
        stack
    });
    entity.items[index] = new;
}

/// Os vizinhos como o ejetor os vê: o inventário do outro lado, copiado do jogo, com a regra real de cada slot para o item que
/// está saindo. O Horizon não simula inserção, então o diário guarda a cópia, e o que entrou nela vai ao vizinho de verdade, por
/// `native_insert_item`, depois que a transação commitou. O ejetor roda antes da receita: só o item da saída pode sair no tick.
struct GameNeighbours<'a, 'g> {
    game: &'a Game<'g>,
    dimension: &'static str,
    pos: [i32; 3],
    offered: Option<(Rc<str>, ItemStack)>,
    created: RefCell<Vec<CopiedContainer>>,
}

struct CopiedContainer {
    at: [i32; 3],
    reached: Direction,
    container: Rc<VanillaContainer>,
    original: Vec<(usize, ItemResource, u32)>,
}

impl ItemCapabilities for GameNeighbours<'_, '_> {
    fn item_handler(&self, face: Face) -> Option<Rc<dyn ItemHandler>> {
        let [x, y, z] = face.offset();
        let at = [self.pos[0] + x, self.pos[1] + y, self.pos[2] + z];
        let reached = basis::direction(face.opposite());
        let slots = self.game.native_container_slots(self.dimension, at, reached)?;
        let container = VanillaContainer::new(slots.len(), Rc::new(|| {}));
        for (index, slot) in slots.iter().enumerate() {
            if let Some(stack) = &slot.stack {
                container.set_item(index, Some(resource_of(self.game, stack)), stack.count);
            }
        }
        let original = container.stacks();
        self.created.borrow_mut().push(CopiedContainer {
            at,
            reached,
            container: container.clone(),
            original,
        });
        let (offered, accepted) = match &self.offered {
            Some((id, stack)) => (
                Some(id.clone()),
                slots
                    .iter()
                    .map(|slot| self.game.machinery_slot_accepts(at, slot.slot, stack, reached))
                    .collect(),
            ),
            None => (None, Vec::new()),
        };
        Some(Rc::new(RuledHandler::new(container, move |index, resource: &ItemResource| {
            offered.as_ref() == Some(&resource.id) && accepted.get(index).copied().unwrap_or(false)
        })))
    }
}

/// O que entrou nas cópias durante o tick: por vizinho, quanto de cada item.
struct Ejected {
    at: [i32; 3],
    reached: Direction,
    id: Rc<str>,
    count: u32,
}

impl GameNeighbours<'_, '_> {
    fn ejected(self) -> Vec<Ejected> {
        let mut ejected = Vec::new();
        for copied in self.created.into_inner() {
            for (index, resource, amount) in copied.container.stacks() {
                let before = copied
                    .original
                    .iter()
                    .find(|(slot, held, _)| *slot == index && *held == resource)
                    .map_or(0, |(_, _, amount)| *amount);
                if amount > before {
                    ejected.push(Ejected {
                        at: copied.at,
                        reached: copied.reached,
                        id: resource.id.clone(),
                        count: amount - before,
                    });
                }
            }
        }
        ejected
    }
}

/// A pilha de um item que sai da máquina: a da receita, com dano e componentes, ou a do id.
fn stack_of(recipes: &[basis::MachineRecipe], id: &str) -> ItemStack {
    let template = recipes.iter().find(|recipe| *recipe.output.item == *id);
    template.map_or_else(
        || ItemStack {
            item: Arc::from(id),
            count: 1,
            damage: 0,
            extra: None,
        },
        |recipe| recipe.output.clone(),
    )
}

/// Entrega ao vizinho de verdade o que o diário mandou. A regra de cada slot já foi perguntada e nada mudou no vizinho desde
/// então, então ele aceita tudo. O resto, se houver, volta ao slot de saída da `ModBlockEntity`.
fn deliver(game: &mut Game<'_>, entity: &mut ModBlockEntity, recipes: &[basis::MachineRecipe], output_slot: usize, ejected: Vec<Ejected>) {
    for sent in ejected {
        let mut stack = stack_of(recipes, &sent.id);
        stack.count = sent.count;
        let Some(rest) = game.native_insert_item(sent.at, sent.reached, stack) else {
            continue;
        };
        let slot = &mut entity.items[output_slot];
        match slot.as_mut() {
            None => *slot = Some(rest),
            Some(held) if held.item == rest.item => held.count += rest.count,
            Some(_) => eprintln!(
                "mekanism: {} x{} recusados pelo vizinho se perderam",
                rest.item, rest.count
            ),
        }
    }
}

/// O baú como o rastro o grava, lido do jogo: `[slot, id, quantidade]` dos slots com algo.
fn chest_of(game: &Game<'_>, dimension: &'static str, pos: [i32; 3], face: Face) -> Vec<(usize, String, u32)> {
    let [x, y, z] = face.offset();
    let at = [pos[0] + x, pos[1] + y, pos[2] + z];
    let reached = basis::direction(face.opposite());
    let slots = game.native_container_slots(dimension, at, reached).unwrap_or_default();
    slots
        .into_iter()
        .filter_map(|slot| {
            let stack = slot.stack?;
            let path = stack.item.split_once(':').map_or(&*stack.item, |(_, path)| path).to_owned();
            Some((slot.slot, path, stack.count))
        })
        .collect()
}

/// Um tick de uma máquina de progresso sobre o objeto vivo.
fn tick_machine(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, spec: &Machine) {
    let now = game.world.game_time;
    let dimension = game.native_dimension();
    let key = (dimension, pos);
    let slots = &spec.slots;
    let recipes = basis::machine_recipes(game, &spec.recipes);
    let conversions = slots
        .infusion
        .map(|_| basis::conversions(game, rules::CHEMICAL_CONVERSION, &rules::CONVERSION_FIELDS));

    let started = basis::MEDIR.then(Instant::now);
    let alive = LIVE_MACHINES.with_borrow_mut(|machines| machines.remove(&key));
    let mut live = match alive {
        Some(live) if live.last_seen + 1 == now && has_saved(entity) => live,
        _ => {
            if slots.infusion.is_some() {
                migrate_old_tank(entity);
            }
            born(game, pos, entity, spec, &recipes, conversions.as_ref())
        }
    };
    let conversions_changed = match (&live.conversions, &conversions) {
        (Some(old), Some(new)) => !Arc::ptr_eq(old, new),
        _ => false,
    };
    if !Arc::ptr_eq(&live.recipes, &recipes) || conversions_changed {
        match &conversions {
            None => live.machine.set_recipes(core_recipes(game, &recipes)),
            Some(conversions) => live
                .machine
                .set_chemical_recipes(core_chemical_recipes(game, &recipes), core_conversions(conversions)),
        }
        live.recipes = recipes.clone();
        live.conversions = conversions.clone();
    }
    let machine = &live.machine;
    slot_from_entity(game, machine.input(), entity.items[slots.input].as_ref());
    slot_from_entity(game, machine.output(), entity.items[slots.output].as_ref());
    slot_from_entity(game, machine.energy_slot(), entity.items[slots.energy].as_ref());
    if let (Some(side), Some(index)) = (machine.chemical_side(), slots.infusion) {
        slot_from_entity(game, &side.slot, entity.items[index].as_ref());
        // O tanque também muda fora do tick: o botão de esvaziar e o que a tela faz.
        let contents = read_tank(entity);
        if side.tank.contents() != contents {
            side.tank.set_contents(contents.resource, contents.amount, None);
        }
    }
    let stored = i64::from(basis::stored_energy(entity));
    if machine.energy().amount() != stored {
        machine.energy().set_energy(stored, None);
    }
    // `updatePower`: o sinal dos vizinhos, lido a cada tick.
    let powered = game.native_neighbor_signal(dimension, pos) > 0;
    if machine.tile().is_powered() != powered {
        machine.tile_mut().update_power(powered);
    }
    // O ejetor liga e desliga por `mekanism_eject`; sai pela direita da frente.
    let switch = match entity.nbt.get(&JavaString::from(EJECT_SWITCH)) {
        Some(Tag::Byte(on)) => Some(*on != 0),
        Some(Tag::Int(on)) => Some(*on != 0),
        _ => None,
    };
    if let Some(on) = switch
        && live.machine.ejector().is_ejecting() != on
    {
        live.machine.ejector_mut().set_ejecting(on);
    }
    let front = basis::front(game, pos);
    let right = front.right_of();
    if live.machine.ejector().is_ejecting() && live.eject_side != Some(right) {
        live.machine.ejector_mut().set_sides(0, vec![right]);
        live.eject_side = Some(right);
    }

    let offered = live.machine.output().resource().map(|resource| {
        let stack = stack_of(&recipes, &resource.id);
        (resource.id, stack)
    });
    let neighbours = GameNeighbours {
        game: &*game,
        dimension,
        pos,
        offered,
        created: RefCell::new(Vec::new()),
    };
    live.machine.tick_server(now as u64, &neighbours);
    let ejected = neighbours.ejected();

    let machine = &live.machine;
    slot_to_entity(entity, slots.input, machine.input(), &recipes);
    slot_to_entity(entity, slots.output, machine.output(), &recipes);
    slot_to_entity(entity, slots.energy, machine.energy_slot(), &recipes);
    if let (Some(side), Some(index)) = (machine.chemical_side(), slots.infusion) {
        slot_to_entity(entity, index, &side.slot, &recipes);
        let contents = side.tank.contents();
        if read_tank(entity) != contents {
            write_tank(entity, &contents);
        }
    }
    let energy = machine.energy().amount_as_int();
    if basis::stored_energy(entity) != energy {
        basis::set_stored_energy(entity, energy);
    }
    let scalars = machine.saved_scalars();
    if live.saved.as_ref() != Some(&scalars) {
        write_scalars(entity, live.saved.as_ref(), &scalars, slots.infusion.is_some());
        live.saved = Some(scalars);
    }
    let version = machine.used_version();
    if live.used_version != Some(version) {
        write_used(entity, &machine.saved_state());
        live.used_version = Some(version);
    }
    deliver(game, entity, &recipes, slots.output, ejected);
    let progress = machine.recipe_progress();
    if let Some(started) = started {
        basis::bench_record(now, u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX));
        let chest = machine
            .ejector()
            .is_ejecting()
            .then(|| chest_of(game, dimension, pos, right));
        basis::trace(&rules::trace_line(now, machine, chest.as_deref()));
    }

    let lit = machine.tile().block_active();
    if live.lit != Some(lit) {
        game.native_set_block_property(pos, "lit", if lit { "true" } else { "false" });
        live.lit = Some(lit);
    }
    let split = data_slots::split(energy as u32);
    let data = match machine.chemical_side() {
        None => vec![split[0], split[1], progress],
        Some(side) => {
            let held = side.tank.resource();
            let color = held.and_then(|held| resource::current().get(&held.id).map(|infusion| infusion.color));
            let color = data_slots::split(color.unwrap_or(0));
            let mut data = vec![0; DATA_SLOTS];
            data[DATA_ENERGY] = split[0];
            data[DATA_ENERGY + 1] = split[1];
            data[DATA_INFUSION] = side.tank.amount() as i32;
            data[DATA_COLOR] = color[0];
            data[DATA_COLOR + 1] = color[1];
            data[DATA_PROGRESS] = progress;
            data
        }
    };
    if entity.data != data {
        entity.data = data;
    }

    match live.machine.chemical_side() {
        None => {
            let accepts = |_: &Game<'_>, _: usize, stack: &ItemStack| basis::is_recipe_input(&recipes, stack, None);
            let slots = |face| rules::electric_item_slots(front, face);
            let sides = rules::sides(front, rules::electric_item_slots);
            basis::tick_hoppers(game, pos, entity, &sides, &slots, &accepts);
        }
        Some(side) => {
            let held = side.tank.resource().map(|resource| resource.id.to_string());
            let accepts = |_: &Game<'_>, slot: usize, stack: &ItemStack| match slot {
                rules::INPUT_SLOT => basis::is_recipe_input(&recipes, stack, held.as_deref()),
                rules::INFUSION_SLOT => conversions
                    .as_ref()
                    .is_some_and(|conversions| conversions.iter().any(|conversion| conversion.item.test(stack))),
                _ => false,
            };
            let slots = |face| rules::item_slots(front, face);
            let sides = rules::sides(front, rules::item_slots);
            basis::tick_hoppers(game, pos, entity, &sides, &slots, &accepts);
        }
    }

    report_comparator(&mut live, key);
    live.last_seen = now;
    LIVE_MACHINES.with_borrow_mut(|machines| {
        machines.insert(key, live);
        if now.saturating_sub(LAST_SWEEP.get()) >= SWEEP_TICKS {
            LAST_SWEEP.set(now);
            machines.retain(|_, live| now - live.last_seen <= STALE_TICKS);
            COMPARATOR_LEVELS.with_borrow_mut(|levels| levels.retain(|key, _| machines.contains_key(key)));
        }
    });
}

/// `notifyComparatorChange`, mais o nível que `getAnalogOutputSignal` devolve. O objeto vivo sai da tabela durante o próprio
/// tick, e um comparador pode perguntar nessa hora, por isso o nível fica em outra tabela.
fn report_comparator(live: &mut Live, key: (&'static str, [i32; 3])) {
    let level = live.machine.tile().current_redstone_level();
    if live.reported_level != Some(level) {
        live.reported_level = Some(level);
        COMPARATOR_LEVELS.with_borrow_mut(|levels| levels.insert(key, level));
        COMPARATOR_QUEUE.with_borrow_mut(|queue| queue.push(key));
    }
}

/// `hasAnalogOutputSignal`: toda máquina do mod tem `AttributeComparator`.
pub(in crate::game) fn has_analog_output(level: &GameLevel<'_>, state: BlockStateId, output: &mut bool) {
    if level.native_state_block(state).is_some_and(|block| machine_of(block).is_some()) {
        *output = true;
    }
}

/// `getAnalogOutputSignal`: o `currentRedstoneLevel` do tile.
pub(in crate::game) fn analog_output(dimension: &'static str, pos: [i32; 3], output: &mut i32) {
    if let Some(level) = COMPARATOR_LEVELS.with_borrow(|levels| levels.get(&(dimension, pos)).copied()) {
        *output = level;
    }
}

/// Avisa os comparadores depois do tick do nível: dentro do `block_entity_tick` a máquina está fora do mundo.
pub(in crate::game) fn notify_comparators(game: &mut Game<'_>, dimension: &str) {
    let pending = COMPARATOR_QUEUE.with_borrow_mut(std::mem::take);
    for (at, pos) in pending {
        if at == dimension {
            game.native_update_output_neighbors(dimension, pos);
        }
    }
}

pub(in crate::game) fn tick(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    if is_cable(&entity.id) {
        basis::transmit_energy(game, pos, entity);
    } else if let Some(machine) = machine_of(&entity.id) {
        tick_machine(game, pos, entity, machine);
    }
}

/// Shift-click do inventário: vai para o slot que aceita o item, e o que nenhum aceita não sai.
pub(in crate::game) fn quick_move(
    game: &Game<'_>,
    entity: &mut ModBlockEntity,
    container_slots: usize,
    slot: usize,
    stack: &mut ItemStack,
    handled: &mut bool,
) {
    if slot < container_slots {
        return;
    }
    if let Some(electric) = electric_machine(&entity.id) {
        let recipes = basis::machine_recipes(game, &electric.recipes);
        if basis::is_recipe_input(&recipes, stack, None) {
            basis::move_into_slot(game, entity, rules::ELECTRIC_INPUT_SLOT, stack);
        }
        *handled = true;
        return;
    }
    if !is_infuser(&entity.id) {
        return;
    }
    let recipes = basis::machine_recipes(game, &INFUSING);
    let conversions = basis::conversions(game, rules::CHEMICAL_CONVERSION, &rules::CONVERSION_FIELDS);
    let held = read_tank(entity).resource.map(|resource| resource.id.to_string());
    if basis::is_recipe_input(&recipes, stack, held.as_deref()) {
        basis::move_into_slot(game, entity, rules::INPUT_SLOT, stack);
    } else if conversions.iter().any(|conversion| conversion.item.test(stack)) {
        basis::move_into_slot(game, entity, rules::INFUSION_SLOT, stack);
    }
    *handled = true;
}

pub(in crate::game) fn menu_button(entity: &mut ModBlockEntity, id: i32) {
    if !is_infuser(&entity.id) || id != DUMP_BUTTON {
        return;
    }
    // `dump()`: `dumpOrClearContents` no tanque de infusão, que fora a radiação (que não existe aqui) só o esvazia. O objeto
    // vivo o relê no tick seguinte, como um `setContents` de fora.
    if !read_tank(entity).is_empty() {
        write_tank(entity, &TankContents::default());
    }
}

pub(in crate::game) fn screen_click(block: &str, at: [f32; 2], right: bool, button: &mut Option<i32>) {
    if is_infuser(block) && !right && inside(DUMP_RECT, at) {
        *button = Some(DUMP_BUTTON);
    }
}

pub(in crate::game) fn screen_tooltip(block: &str, data: &[i32], at: [f32; 2], lines: &mut Vec<String>) {
    let get = |i: usize| data.get(i).copied().unwrap_or(0);
    let energy = || format_energy(data_slots::join(get(DATA_ENERGY), get(DATA_ENERGY + 1)));
    if electric_machine(block).is_some() {
        if inside(ELECTRIC_ENERGY_BAR, at) {
            lines.push(energy());
        }
    } else if is_infuser(block) {
        if inside(ENERGY_BAR, at) {
            lines.push(energy());
        } else if inside(INFUSION_BAR, at) {
            lines.push(format!("{} / {} mB", get(DATA_INFUSION), rules::TANK_CAPACITY));
        }
    }
}

fn draw_slots(
    game: &Game<'_>,
    batch: &mut HudBatch,
    origin: [f32; 2],
    positions: &[[f32; 2]],
    energy_slot: usize,
) {
    let [x, y] = origin;
    for (i, [slot_x, slot_y]) in positions.iter().copied().enumerate() {
        let sprite = if i == energy_slot { ENERGY_SLOT_FRAME } else { SLOT };
        let dst = [x + slot_x - 1.0, y + slot_y - 1.0, SLOT_SIZE, SLOT_SIZE];
        game.native_sprite(batch, sprite, [0.0, 0.0, SLOT_SIZE, SLOT_SIZE], dst);
    }
}

fn draw_infuser(game: &Game<'_>, data: &[i32], batch: &mut HudBatch, origin: [f32; 2]) {
    let [x, y] = origin;
    let at = |rect: [f32; 4]| [x + rect[0], y + rect[1], rect[2], rect[3]];
    let get = |i: usize| data.get(i).copied().unwrap_or(0);

    let title = lang::get("container.mekanism.metallurgic_infuser");
    basis::machine_panel(game, batch, &PANEL, [x, y, WIDTH, HEIGHT], &title);
    draw_slots(game, batch, origin, &SLOT_POSITIONS, rules::ENERGY_SLOT);
    let infusion = get(DATA_INFUSION) as f32 / rules::TANK_CAPACITY as f32;
    let color = OPAQUE | data_slots::join(get(DATA_COLOR), get(DATA_COLOR + 1));
    basis::gauge(game, batch, at(INFUSION_BAR), infusion, color);
    let energy = data_slots::join(get(DATA_ENERGY), get(DATA_ENERGY + 1));
    let fill = energy as f32 / rules::energy_capacity(rules::INFUSER_USAGE) as f32;
    basis::gauge(game, batch, at(ENERGY_BAR), fill, ENERGY_COLOR);
    let progress = get(DATA_PROGRESS) as f32 / rules::TICKS_REQUIRED as f32;
    basis::progress_bar(game, batch, PROGRESS_EMPTY, PROGRESS_FULL, at(PROGRESS_BAR), progress);
    sprite_rect(game, batch, DUMP_SPRITE, at(DUMP_RECT));
}

fn draw_electric(
    game: &Game<'_>,
    electric: &Machine,
    data: &[i32],
    batch: &mut HudBatch,
    origin: [f32; 2],
) {
    let [x, y] = origin;
    let at = |rect: [f32; 4]| [x + rect[0], y + rect[1], rect[2], rect[3]];
    let get = |i: usize| data.get(i).copied().unwrap_or(0);

    let title = lang::get(electric.title);
    basis::machine_panel(game, batch, &PANEL, [x, y, WIDTH, HEIGHT], &title);
    draw_slots(game, batch, origin, &ELECTRIC_SLOT_POSITIONS, rules::ELECTRIC_ENERGY_SLOT);
    sprite_rect(game, batch, UP_ARROW, at(ELECTRIC_ARROW));
    let energy = data_slots::join(get(DATA_ENERGY), get(DATA_ENERGY + 1));
    let capacity = rules::energy_capacity(electric.energy_per_tick);
    basis::gauge(game, batch, at(ELECTRIC_ENERGY_BAR), energy as f32 / capacity as f32, ENERGY_COLOR);
    let progress = get(ELECTRIC_DATA_PROGRESS) as f32 / rules::TICKS_REQUIRED as f32;
    basis::progress_bar(game, batch, BAR_EMPTY, BAR_FULL, at(ELECTRIC_PROGRESS_BAR), progress);
}

pub(in crate::game) fn screen_draw(
    game: &Game<'_>,
    block: &str,
    data: &[i32],
    batch: &mut HudBatch,
    origin: [f32; 2],
) {
    if is_infuser(block) {
        draw_infuser(game, data, batch, origin);
    } else if let Some(electric) = electric_machine(block) {
        draw_electric(game, electric, data, batch, origin);
    }
}
