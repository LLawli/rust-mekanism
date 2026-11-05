// As máquinas do Mekanism no jogo. As regras ficam em mekanism_rules, e o que é genérico em
// machinery_basis.
use crate::game::mekanism_rules as rules;
use crate::game::{Game, machinery_basis as basis};
use horizon_client::containers::{ModBlockEntity, ModContainer, ModEnergyBuffer};
use horizon_client::hud::HudBatch;
use horizon_client::items::ItemStack;
use horizon_client::lang;
use horizon_client::mod_api::NativeScreen;
use horizon_world::nbt::{Compound, JavaString, Tag};
use machinery_basis_core::cached_recipe::{ItemIngredient, ItemOutput, ItemToItemRecipe};
use machinery_basis_core::containers::{InventorySlot, ItemResource};
use machinery_basis_core::data_slots;
use machinery_basis_core::item_handler::NoNeighbours;
use machinery_basis_core::resource;
use machinery_basis_core::tank::Action;
use machinery_basis_core::tile::{ElectricMachine, RECIPE_CHECK_FREQUENCY};
use machinery_basis_core::units::format_energy;
use basis::RecipeSource::{Cooking, Custom};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const TANKS: [u64; 1] = [rules::TANK_CAPACITY];
const DUMP_BUTTON: i32 = 0;
const INFUSING: basis::RecipeSource = Custom(rules::INFUSING, rules::INFUSING_FIELDS);
const INFUSER: basis::RecipeMachine = basis::RecipeMachine {
    recipes: INFUSING,
    ticks: rules::TICKS_REQUIRED,
    energy_per_tick: rules::INFUSER_USAGE,
    input: rules::INPUT_SLOT,
    output: rules::OUTPUT_SLOT,
};

/// Uma máquina elétrica: as três só mudam no id, nas receitas e no consumo.
struct Electric {
    block: &'static str,
    title: &'static str,
    machine: basis::RecipeMachine,
}

const fn electric(
    block: &'static str,
    title: &'static str,
    recipes: basis::RecipeSource,
    energy_per_tick: i32,
) -> Electric {
    Electric {
        block,
        title,
        machine: basis::RecipeMachine {
            recipes,
            ticks: rules::TICKS_REQUIRED,
            energy_per_tick,
            input: rules::ELECTRIC_INPUT_SLOT,
            output: rules::ELECTRIC_OUTPUT_SLOT,
        },
    }
}

static ELECTRIC_MACHINES: [Electric; 3] = [
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

fn electric_machine(block: &str) -> Option<&'static Electric> {
    ELECTRIC_MACHINES.iter().find(|machine| machine.block == block)
}

fn is_machine(block: &str) -> bool {
    is_infuser(block) || electric_machine(block).is_some()
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
    let usage = match electric_machine(block) {
        Some(electric) => electric.machine.energy_per_tick,
        None if is_infuser(block) => rules::INFUSER_USAGE,
        None => return,
    };
    // `LightConfigInfo.INPUT_ONLY`: energia entra por todas as faces.
    *energy = Some(ModEnergyBuffer {
        capacity: rules::energy_capacity(usage),
        max_insert: rules::energy_capacity(usage),
        insert_faces: [true; 6],
        ..Default::default()
    });
}

pub(in crate::game) fn container(block: &str, container: &mut Option<ModContainer>) {
    let slots = match electric_machine(block) {
        Some(_) => rules::ELECTRIC_SLOTS,
        None if is_infuser(block) => rules::SLOTS,
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

fn tick_infuser(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    let recipes = basis::machine_recipes(game, &INFUSING);
    let conversions = basis::conversions(game, rules::CHEMICAL_CONVERSION, &rules::CONVERSION_FIELDS);
    let mut tanks = basis::load_tanks(entity, &TANKS);
    let tank = &mut tanks[0];

    let input = entity.items[rules::INPUT_SLOT].clone();
    let usable = |infusion: &str| basis::is_recipe_resource(&recipes, infusion, input.as_ref());
    basis::convert_into_tank(entity, rules::INFUSION_SLOT, tank, &conversions, &usable);
    let (step, progress) = basis::run_machine(game, entity, Some(&mut *tank), &INFUSER);

    let front = basis::front(game, pos);
    let held = tank.resource().map(str::to_owned);
    let accepts = |_: &Game<'_>, slot: usize, stack: &ItemStack| match slot {
        rules::INPUT_SLOT => basis::is_recipe_input(&recipes, stack, held.as_deref()),
        rules::INFUSION_SLOT => conversions.iter().any(|conversion| conversion.item.test(stack)),
        _ => false,
    };
    let slots = |face| rules::item_slots(front, face);
    let sides = rules::sides(front, rules::item_slots);
    basis::tick_hoppers(game, pos, entity, &sides, &slots, &accepts);
    let lit = if step.active { "true" } else { "false" };
    game.native_set_block_property(pos, "lit", lit);

    let color = held.and_then(|id| resource::current().get(&id).map(|infusion| infusion.color));
    let mut data = vec![0; DATA_SLOTS];
    let energy = data_slots::split(basis::stored_energy(entity) as u32);
    data[DATA_ENERGY] = energy[0];
    data[DATA_ENERGY + 1] = energy[1];
    data[DATA_INFUSION] = tank.amount() as i32;
    let color = data_slots::split(color.unwrap_or(0));
    data[DATA_COLOR] = color[0];
    data[DATA_COLOR + 1] = color[1];
    data[DATA_PROGRESS] = progress as i32;
    entity.data = data;
    basis::save_tanks(entity, &tanks);
}

// Os objetos vivos da Enrichment Chamber (a opção B: o desenho do Java, com contêineres e receita em cache como objetos).
//
// Moram aqui, numa tabela por thread, uma por block entity, na chave (dimensão, posição). O Horizon entrega a block entity
// por `&mut` só durante o tick, e `ModBlockEntity` guarda `Items`, `nbt` e `data`, sem lugar para um objeto vivo: então o
// objeto é um cache, e a verdade que sobrevive é a `ModBlockEntity`.
//
// - Nasce no primeiro tick do bloco, ou no primeiro depois de uma lacuna (o chunk saiu, o mundo foi salvo e reaberto, o bloco
//   foi trocado): refeito dos itens, da energia e do progresso que a `ModBlockEntity` tem. É o `deserialize` do Java.
// - Vive enquanto o tick seguinte for o do tick seguinte do jogo. Todo tick começa lendo da `ModBlockEntity` o que mudou
//   fora dele (a tela, um hopper, um cabo), como um `setContents` do Java, que dispara os listeners; e termina escrevendo o
//   que mudou. Salvar, abrir a tela e mandar energia funcionam sem o objeto saber.
// - Morre pela varredura: sem andar por `STALE_TICKS`, o que acontece com bloco quebrado ou chunk descarregado, o objeto sai
//   da tabela. Um bloco novo na mesma posição não herda nada: sem a chave `TILE_KEY` na `ModBlockEntity`, nasce outro.
thread_local! {
    static CHAMBERS: RefCell<HashMap<(&'static str, [i32; 3]), Live>> = RefCell::new(HashMap::new());
    static LAST_SWEEP: Cell<i64> = const { Cell::new(i64::MIN) };
}

/// O compound do tile na `ModBlockEntity`: o que o Java salva em `loadAdditional` do monitor, só o progresso.
const TILE_KEY: &str = "mekanism_tile";
const PROGRESS_KEY: &str = "progress";
/// `progress` que ninguém gravou ainda: força a primeira escrita, que cria `TILE_KEY`.
const UNSAVED: i32 = -1;
/// De quanto em quanto tempo a tabela é varrida, e quanto sem andar um objeto vive.
const SWEEP_TICKS: i64 = 200;
const STALE_TICKS: i64 = 100;
/// Quando o item não é conhecido, a pilha dele.
const UNKNOWN_STACK: u32 = 64;

struct Live {
    machine: ElectricMachine,
    /// As receitas de onde as da máquina saíram: uma carga nova de dados as troca.
    recipes: Arc<Vec<basis::MachineRecipe>>,
    last_seen: i64,
    saved_progress: i32,
    /// O `lit` que o bloco tem, para só escrever quando muda.
    lit: Option<bool>,
}

fn resource_of(game: &Game<'_>, stack: &ItemStack) -> ItemResource {
    let limit = game.native_max_stack_size(&stack.item).unwrap_or(UNKNOWN_STACK);
    ItemResource::new(&stack.item, limit)
}

/// `ItemToItemRecipe` das receitas do jogo. O ingrediente do Horizon (item ou tag) vira o predicado da lib, e guarda a
/// última resposta, porque a receita em cache pergunta todo tick pelo mesmo item.
fn core_recipes(game: &Game<'_>, recipes: &Arc<Vec<basis::MachineRecipe>>) -> Vec<Rc<ItemToItemRecipe>> {
    let mut converted = Vec::with_capacity(recipes.len());
    for (index, recipe) in recipes.iter().enumerate() {
        let source = recipes.clone();
        let last: RefCell<Option<(Rc<str>, bool)>> = RefCell::new(None);
        let input = ItemIngredient::matching(move |resource| {
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
            let answer = source[index].item.test(&stack);
            *last.borrow_mut() = Some((resource.id.clone(), answer));
            answer
        });
        converted.push(Rc::new(ItemToItemRecipe {
            id: recipe.id.clone(),
            input,
            input_count: recipe.count,
            output: ItemOutput {
                resource: resource_of(game, &recipe.output),
                count: recipe.output.count,
            },
        }));
    }
    converted
}

fn has_tile_key(entity: &ModBlockEntity) -> bool {
    entity.nbt.contains_key(&JavaString::from(TILE_KEY))
}

fn saved_chamber_progress(entity: &ModBlockEntity) -> i32 {
    let saved = entity.nbt.get(&JavaString::from(TILE_KEY));
    match saved {
        Some(Tag::Compound(tile)) => match tile.get(&JavaString::from(PROGRESS_KEY)) {
            Some(Tag::Int(progress)) => *progress,
            _ => 0,
        },
        _ => 0,
    }
}

fn save_chamber_progress(entity: &mut ModBlockEntity, progress: i32) {
    let mut tile = Compound::default();
    tile.insert(JavaString::from(PROGRESS_KEY), Tag::Int(progress));
    entity.nbt.insert(JavaString::from(TILE_KEY), Tag::Compound(tile));
}

/// `ElectricMachine` nova, do que a `ModBlockEntity` guarda.
fn born(game: &Game<'_>, entity: &ModBlockEntity, recipes: &Arc<Vec<basis::MachineRecipe>>) -> Live {
    // `ThreadLocalRandom.nextInt(RECIPE_CHECK_FREQUENCY)`: o sorteio de `shouldRecheckAllErrors`.
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| u64::from(since.subsec_nanos()));
    let config = rules::chamber_config(core_recipes(game, recipes), nanos % RECIPE_CHECK_FREQUENCY);
    let machine = ElectricMachine::new(config);
    machine.load_operating_ticks(saved_chamber_progress(entity));
    Live {
        machine,
        recipes: recipes.clone(),
        last_seen: i64::MIN,
        saved_progress: UNSAVED,
        lit: None,
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

/// Um tick da Enrichment Chamber sobre o objeto vivo: a base nova, no lugar de `run_machine`.
fn tick_chamber(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, electric: &Electric) {
    let now = game.world.game_time;
    let key = (game.native_dimension(), pos);
    let recipes = basis::machine_recipes(game, &electric.machine.recipes);

    let started = basis::MEDIR.then(Instant::now);
    let alive = CHAMBERS.with_borrow_mut(|chambers| chambers.remove(&key));
    let mut live = match alive {
        Some(live) if live.last_seen + 1 == now && has_tile_key(entity) => live,
        _ => born(game, entity, &recipes),
    };
    if !Arc::ptr_eq(&live.recipes, &recipes) {
        live.machine.set_recipes(core_recipes(game, &recipes));
        live.recipes = recipes.clone();
    }
    let machine = &live.machine;
    slot_from_entity(game, machine.input(), entity.items[rules::ELECTRIC_INPUT_SLOT].as_ref());
    slot_from_entity(game, machine.output(), entity.items[rules::ELECTRIC_OUTPUT_SLOT].as_ref());
    slot_from_entity(game, machine.energy_slot(), entity.items[rules::ELECTRIC_ENERGY_SLOT].as_ref());
    let stored = i64::from(basis::stored_energy(entity));
    if machine.energy().amount() != stored {
        machine.energy().set_energy(stored, None);
    }

    live.machine.tick_server(now as u64, &NoNeighbours);

    let machine = &live.machine;
    slot_to_entity(entity, rules::ELECTRIC_INPUT_SLOT, machine.input(), &recipes);
    slot_to_entity(entity, rules::ELECTRIC_OUTPUT_SLOT, machine.output(), &recipes);
    slot_to_entity(entity, rules::ELECTRIC_ENERGY_SLOT, machine.energy_slot(), &recipes);
    let energy = machine.energy().amount_as_int();
    if basis::stored_energy(entity) != energy {
        basis::set_stored_energy(entity, energy);
    }
    let progress = machine.recipe_progress();
    if progress != live.saved_progress {
        save_chamber_progress(entity, progress);
        live.saved_progress = progress;
    }
    if let Some(started) = started {
        basis::bench_record(now, u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX));
        basis::trace(&rules::trace_line(now, machine));
    }

    let lit = machine.tile().block_active();
    if live.lit != Some(lit) {
        game.native_set_block_property(pos, "lit", if lit { "true" } else { "false" });
        live.lit = Some(lit);
    }
    let split = data_slots::split(energy as u32);
    let data = [split[0], split[1], progress];
    if entity.data != data {
        entity.data = data.to_vec();
    }

    let front = basis::front(game, pos);
    let accepts = |_: &Game<'_>, _: usize, stack: &ItemStack| basis::is_recipe_input(&recipes, stack, None);
    let slots = |face| rules::electric_item_slots(front, face);
    let sides = rules::sides(front, rules::electric_item_slots);
    basis::tick_hoppers(game, pos, entity, &sides, &slots, &accepts);

    live.last_seen = now;
    CHAMBERS.with_borrow_mut(|chambers| {
        chambers.insert(key, live);
        if now.saturating_sub(LAST_SWEEP.get()) >= SWEEP_TICKS {
            LAST_SWEEP.set(now);
            chambers.retain(|_, live| now - live.last_seen <= STALE_TICKS);
        }
    });
}

fn tick_electric(
    game: &mut Game<'_>,
    pos: [i32; 3],
    entity: &mut ModBlockEntity,
    electric: &Electric,
) {
    let (step, progress) = basis::run_machine(game, entity, None, &electric.machine);
    let recipes = basis::machine_recipes(game, &electric.machine.recipes);
    let front = basis::front(game, pos);
    let accepts = |_: &Game<'_>, _: usize, stack: &ItemStack| basis::is_recipe_input(&recipes, stack, None);
    let slots = |face| rules::electric_item_slots(front, face);
    let sides = rules::sides(front, rules::electric_item_slots);
    basis::tick_hoppers(game, pos, entity, &sides, &slots, &accepts);
    let lit = if step.active { "true" } else { "false" };
    game.native_set_block_property(pos, "lit", lit);

    let mut data = vec![0; ELECTRIC_DATA_SLOTS];
    let energy = data_slots::split(basis::stored_energy(entity) as u32);
    data[DATA_ENERGY] = energy[0];
    data[DATA_ENERGY + 1] = energy[1];
    data[ELECTRIC_DATA_PROGRESS] = progress as i32;
    entity.data = data;
}

pub(in crate::game) fn tick(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    if is_cable(&entity.id) {
        basis::transmit_energy(game, pos, entity);
    } else if is_infuser(&entity.id) {
        tick_infuser(game, pos, entity);
    } else if let Some(electric) = electric_machine(&entity.id) {
        if electric.block == rules::ENRICHMENT_CHAMBER {
            tick_chamber(game, pos, entity, electric);
        } else {
            tick_electric(game, pos, entity, electric);
        }
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
        let recipes = basis::machine_recipes(game, &electric.machine.recipes);
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
    let tanks = basis::load_tanks(entity, &TANKS);
    if basis::is_recipe_input(&recipes, stack, tanks[0].resource()) {
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
    let mut tanks = basis::load_tanks(entity, &TANKS);
    let amount = tanks[0].amount();
    tanks[0].extract(amount, Action::Execute);
    basis::save_tanks(entity, &tanks);
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
    electric: &Electric,
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
    let capacity = rules::energy_capacity(electric.machine.energy_per_tick);
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
