// O Heat Generator no jogo. As regras ficam em mekanismgenerators_rules, e o que é genérico em
// machinery_basis.
use crate::game::mekanismgenerators_rules as rules;
use crate::game::{Game, machinery_basis as basis};
use horizon_client::containers::{ModBlockEntity, ModContainer, ModEnergyBuffer};
use horizon_client::hud::HudBatch;
use horizon_client::items::ItemStack;
use horizon_client::lang;
use horizon_client::mod_api::NativeScreen;
use horizon_world::nbt::{Compound, JavaString, Tag};
use machinery_basis_core::data_slots;
use machinery_basis_core::gui;
use machinery_basis_core::fluid_logged::{EMPTY_BUCKET, LAVA_BUCKET};
use machinery_basis_core::heat::Capacitor;
use machinery_basis_core::sides::{Face, FaceSlots, Sides};
use machinery_basis_core::slot_view::{SlotOverlay, SlotType, slot_type};
use machinery_basis_core::tank::{Action, Tank};

const FUEL_SLOT: usize = 0;
const SLOTS: usize = rules::SLOTS_SPEC.len();
const FUEL_SLOTS: &[usize] = &[FUEL_SLOT];
const LAVA_TANK: [u64; 1] = [rules::TANK_CAPACITY];
const NETHER: &str = "minecraft:the_nether";
const LAVA_BLOCK: &str = "minecraft:lava";
const HEAT_KEY: &str = "heat";

// Os data slots da tela.
const DATA_ENERGY: usize = 0;
const DATA_LAVA: usize = 2;
const DATA_TEMPERATURE: usize = 3;
const DATA_PRODUCTION: usize = 5;
const DATA_SLOTS: usize = 6;

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

pub(in crate::game) fn block_entity(block: &str, entity: &mut Option<(String, Compound)>) {
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
        // Sem interface vanilla: hoppers passam pela regra da machinery_basis.
        *container = Some(ModContainer {
            slots: SLOTS,
            menu: Some(rules::MENU.to_owned()),
            ..Default::default()
        });
    }
}

pub(in crate::game) fn screen(block: &str, screen: &mut Option<NativeScreen>) {
    if is_generator(block) {
        *screen = Some(NativeScreen {
            width: WIDTH,
            height: HEIGHT,
            slots: rules::SLOTS_SPEC.iter().map(|slot| Some(slot.item)).collect(),
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
    let needed = tank.room();
    if needed == 0 {
        return;
    }
    if &*stack.item == LAVA_BUCKET {
        if needed >= rules::BUCKET_VOLUME {
            tank.insert(rules::LAVA, rules::BUCKET_VOLUME, Action::Execute);
            let mut empty = stack;
            empty.item = EMPTY_BUCKET.into();
            entity.items[FUEL_SLOT] = Some(empty);
        }
        return;
    }
    let fuel = rules::lava_from_fuel(burn_time(game, &stack), lava_bucket_burn_time(game));
    if fuel > 0 && fuel <= needed {
        tank.insert(rules::LAVA, fuel, Action::Execute);
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

pub(in crate::game) fn tick(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    if !is_generator(&entity.id) {
        return;
    }
    let mut tanks = basis::load_tanks(entity, &LAVA_TANK);
    let tank = &mut tanks[0];
    fill_or_burn(game, entity, tank);

    let ambient = basis::ambient_temperature(game, pos);
    let around = rules::Surroundings {
        lava_faces: lava_faces(game, pos) + u32::from(basis::has_logged_lava(game, pos)),
        in_nether: game.native_dimension() == NETHER,
        ambient,
    };
    let mut heat = load_heat(entity, ambient);
    let stored = basis::stored_energy(entity);
    let full = stored >= rules::ENERGY_CAPACITY;
    let produced = rules::tick(&mut heat, &around, tank.amount(), full);
    tank.extract(produced.lava_used, Action::Execute);
    let filled = (stored + produced.energy).min(rules::ENERGY_CAPACITY);
    basis::set_stored_energy(entity, filled);
    entity.nbt.insert(JavaString::from(HEAT_KEY), Tag::Double(heat.heat.to_bits()));

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
    basis::tick_hoppers(game, pos, entity, &sides, &|_| slots, &accepts);
    let lit = if produced.active { "true" } else { "false" };
    game.native_set_block_property(pos, "lit", lit);

    let mut data = vec![0; DATA_SLOTS];
    let energy = data_slots::split(basis::stored_energy(entity) as u32);
    data[DATA_ENERGY] = energy[0];
    data[DATA_ENERGY + 1] = energy[1];
    data[DATA_LAVA] = tank.amount() as i32;
    let temperature = data_slots::split(heat.temperature() as u32);
    data[DATA_TEMPERATURE] = temperature[0];
    data[DATA_TEMPERATURE + 1] = temperature[1];
    data[DATA_PRODUCTION] = filled - stored;
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

    let title = lang::get("container.mekanismgenerators.heat_generator");
    basis::machine_panel(game, batch, &PANEL, [x, y, WIDTH, HEIGHT], &title);
    for (index, spec) in rules::SLOTS_SPEC.iter().enumerate() {
        let frame = slot_type(None, index, spec.container).map(slot_sprite);
        basis::slot(game, batch, origin, spec.item, frame, spec.overlay.and_then(overlay_sprite));
    }
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
    let get = |i: usize| data.get(i).copied().unwrap_or(0);
    let translate = |key: &str| lang::get(key);
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
