// As máquinas do Mekanism no jogo. As regras ficam em mekanism_rules, e o que é genérico em
// machinery_basis.
use crate::game::mekanism_rules as rules;
use crate::game::{Game, machinery_basis as basis};
use horizon_client::containers::{ModBlockEntity, ModContainer, ModEnergyBuffer};
use horizon_client::hud::HudBatch;
use horizon_client::items::ItemStack;
use horizon_client::lang;
use horizon_client::mod_api::NativeScreen;
use horizon_world::nbt::Compound;
use machinery_basis_core::data_slots;
use machinery_basis_core::resource;
use machinery_basis_core::tank::Action;
use machinery_basis_core::units::format_energy;
use basis::RecipeSource::{Cooking, Custom};

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

fn inside(rect: [f32; 4], at: [f32; 2]) -> bool {
    let [x, y, w, h] = rect;
    (x..x + w).contains(&at[0]) && (y..y + h).contains(&at[1])
}

fn sprite_rect(game: &Game<'_>, batch: &mut HudBatch, sprite: &str, rect: [f32; 4]) {
    let [_, _, w, h] = rect;
    game.native_sprite(batch, sprite, [0.0, 0.0, w, h], rect);
}

pub(in crate::game) fn block_entity(block: &str, entity: &mut Option<(String, Compound)>) {
    if is_machine(block) && entity.is_none() {
        *entity = Some((block.to_owned(), Compound::default()));
    }
}

pub(in crate::game) fn ticks(block: &str, ticks: &mut bool) {
    if is_machine(block) {
        *ticks = true;
    }
}

pub(in crate::game) fn energy(block: &str, energy: &mut Option<ModEnergyBuffer>) {
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
    if is_infuser(&entity.id) {
        tick_infuser(game, pos, entity);
    } else if let Some(electric) = electric_machine(&entity.id) {
        tick_electric(game, pos, entity, electric);
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
