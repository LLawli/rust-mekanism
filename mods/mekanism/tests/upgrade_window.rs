//! A aba e a janela de upgrades sem o jogo: repete, passo a passo, os rastros que o Mekanism original gravou em
//! `dev/referencia/rastros` (instalação pela janela, remoção por pacote, shift-clique com e sem janela, e o que o contêiner
//! sincroniza), com a lógica pura da `machinery_basis` por baixo.
mod common;

use machinery_basis_core::cached_recipe::{ItemIngredient, ItemOutput, ItemToItemRecipe};
use machinery_basis_core::containers::{AutomationType, InventorySlot, ItemResource};
use machinery_basis_core::item_handler::ItemCapabilities;
use machinery_basis_core::operation_tracker::RecipeError;
use machinery_basis_core::quick_move::{QuickMenu, Region, quick_move};
use machinery_basis_core::sim::{SimBlockEntity, SimWorld};
use machinery_basis_core::tile::ProgressMachine;
use machinery_basis_core::transaction::Transaction;
use machinery_basis_core::upgrade_window::{
    UpgradeSync, decode_remove, encode_remove, type_at, type_index,
};
use machinery_basis_core::window::{SelectedWindow, WindowType};
use machinery_basis_core::window_session::Sessions;
use mekanism_rules::{
    ELECTRIC_TRACKED_BASE, ENERGY_UPGRADE, ENRICHMENT_USAGE, INFUSER_TRACKED_BASE, SPEED_UPGRADE,
    UPGRADE_TRACKED_ITEMS, UPGRADE_TRACKING_KEY, UPGRADE_TYPES, UPGRADE_WINDOW, WINDOW_TYPES,
    electric_config, upgrade_type_of,
};
use serde_json::{Value, json};
use std::path::Path;
use std::rc::Rc;

const MINIMUM_LINES: usize = 10;

/// O contêiner e o jogador do cenário: o rastro tem um só.
const CONTAINER: u32 = 1;
const PLAYER: u128 = 1;

struct Chamber(ProgressMachine);

impl SimBlockEntity for Chamber {
    fn tick_server(&mut self, game_time: u64, neighbours: &dyn ItemCapabilities) {
        self.0.tick_server(game_time, neighbours);
    }

    fn neighbor_changed(&mut self, powered: bool) {
        self.0.tile_mut().update_power(powered);
    }
}

fn item(id: &str) -> ItemResource {
    ItemResource::new(id, 64)
}

fn chamber() -> SimWorld<Chamber> {
    let recipe = |input: &str, count: u32, output: &str, out: u32| {
        Rc::new(ItemToItemRecipe {
            id: format!("mekanism:enriching/{output}"),
            input: ItemIngredient::item(input),
            input_count: count,
            output: ItemOutput {
                resource: item(output),
                count: out,
            },
        })
    };
    SimWorld::new(Chamber(ProgressMachine::electric(electric_config(
        ENRICHMENT_USAGE,
        ItemToItemRecipe::cache_of(vec![
            recipe("clay", 1, "clay_ball", 4),
            recipe("stone", 1, "cracked_stone_bricks", 1),
            recipe("hdpe_pellet", 3, "hdpe_sheet", 1),
        ]),
        0,
        common::upgrade_data(),
    ))))
}

fn lines(scenario: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(format!("{scenario}.jsonl"));
    let lines: Vec<Value> = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(
        lines.len() >= MINIMUM_LINES,
        "{scenario}: rastro curto demais: {}",
        lines.len()
    );
    lines
}

fn marks(line: &Value) -> Vec<&str> {
    line["marks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|mark| mark.as_str().unwrap())
        .collect()
}

/// Uma marca que junta várias ações separadas por vírgula (`select upgrade window, track upgrades`).
fn actions(line: &Value) -> Vec<&str> {
    marks(line)
        .into_iter()
        .flat_map(|mark| mark.split(", "))
        .collect()
}

fn slot_json(slot: &InventorySlot) -> Value {
    json!([
        slot.resource().map_or(String::new(), |r| r.id.to_string()),
        slot.amount()
    ])
}

/// Um slot de upgrade como o rastro o grava: o tipo sem o namespace e a quantidade.
fn upgrade_slot_json(slot: &InventorySlot) -> Value {
    let kind = slot
        .resource()
        .and_then(|resource| upgrade_type_of(&resource))
        .map_or(String::new(), |kind| {
            kind.trim_start_matches("mekanism:").to_owned()
        });
    json!([kind, slot.amount()])
}

fn errors_json(machine: &ProgressMachine) -> Value {
    [
        (RecipeError::NOT_ENOUGH_ENERGY, "energy"),
        (RecipeError::NOT_ENOUGH_INPUT, "input"),
        (RecipeError::NOT_ENOUGH_OUTPUT_SPACE, "output_space"),
        (RecipeError::INPUT_DOESNT_PRODUCE_OUTPUT, "wrong_output"),
    ]
    .iter()
    .filter(|(error, _)| machine.has_error(*error))
    .map(|(_, name)| Value::from(*name))
    .collect()
}

/// Os campos que os três rastros de upgrade têm em comum, calculados da máquina.
fn machine_state(machine: &ProgressMachine, t: &Value, marks: &Value) -> Value {
    let upgrades = machine.upgrades();
    json!({
        "t": t,
        "marks": marks,
        "progress": machine.recipe_progress(),
        "required": machine.ticks_required(),
        "energy": machine.energy().amount(),
        "capacity": machine.energy().capacity(),
        "per_tick": machine.energy().energy_per_tick(),
        "active": machine.tile().active(),
        "block_active": machine.tile().block_active(),
        "powered": machine.tile().is_powered(),
        "comparator": machine.tile().current_redstone_level(),
        "input": slot_json(machine.input()),
        "output": slot_json(machine.output()),
        "errors": errors_json(machine),
        "speed": upgrades.get_upgrades(SPEED_UPGRADE),
        "energy_upgrades": upgrades.get_upgrades(ENERGY_UPGRADE),
        // O Java grava o `float` pelo texto mais curto que o lê de volta, que o `{:?}` do `f32` também dá.
        "upgrade_progress": serde_json::from_str::<Value>(&format!("{:?}", upgrades.scaled_upgrade_progress())).unwrap(),
        "upgrade_in": upgrade_slot_json(upgrades.upgrade_slot()),
        "upgrade_out": upgrade_slot_json(upgrades.upgrade_output_slot()),
    })
}

/// O tipo de upgrade de uma marca (`speed`, `energy`, `xp_containment`) com o namespace do registro.
fn upgrade_id(name: &str) -> String {
    format!("mekanism:{name}")
}

/// `addUpgrades` até o total que a marca diz.
fn install(machine: &ProgressMachine, name: &str, total: i32) {
    let upgrades = machine.upgrades();
    let id = upgrade_id(name);
    upgrades.add_upgrades(&id, total - upgrades.get_upgrades(&id));
}

fn set_slot(slot: &InventorySlot, machine: &ProgressMachine, name: &str, count: u32) {
    let resource = machine.upgrades().data().resource_of(&upgrade_id(name));
    slot.set_contents(Some(resource), count, None);
}

/// `PacketRemoveUpgrade` pelo fio: o tipo vira índice no registro, os bytes saem e voltam, e o servidor remove.
fn remove_by_packet(machine: &ProgressMachine, name: &str, remove_all: bool) {
    let order = machine.upgrades().data().type_ids();
    let index = type_index(&order, &upgrade_id(name)).expect("tipo do registro");
    let (index, remove_all) = decode_remove(&encode_remove(index, remove_all)).unwrap();
    if let Some(upgrade) = type_at(&order, index) {
        machine.upgrades().remove_upgrade(upgrade, remove_all);
    }
}

#[test]
fn the_remove_packet_follows_the_original_trace() {
    let mut world = chamber();
    for expected in lines("chamber_upgrade_remove_by_packet") {
        world.tick();
        let machine = &world.block_entity.0;
        for mark in marks(&expected) {
            let words: Vec<&str> = mark
                .split_whitespace()
                .map(|w| w.trim_end_matches(','))
                .collect();
            match words.as_slice() {
                ["install", "speed", speed, "energy", energy] => {
                    install(machine, "speed", speed.parse().unwrap());
                    install(machine, "energy", energy.parse().unwrap());
                }
                ["install", "speed", speed, "output", "speed", count] => {
                    install(machine, "speed", speed.parse().unwrap());
                    set_slot(
                        machine.upgrades().upgrade_output_slot(),
                        machine,
                        "speed",
                        count.parse().unwrap(),
                    );
                }
                ["output", "energy", count, "with", "room", "for", _] => set_slot(
                    machine.upgrades().upgrade_output_slot(),
                    machine,
                    "energy",
                    count.parse().unwrap(),
                ),
                ["output", "cleared", "packet", "remove", name, "all"] => {
                    machine
                        .upgrades()
                        .upgrade_output_slot()
                        .set_contents(None, 0, None);
                    remove_by_packet(machine, name, true);
                }
                ["packet", "remove", name, "all", ..] => remove_by_packet(machine, name, true),
                ["packet", "remove", name, "one", ..] => remove_by_packet(machine, name, false),
                ["packet", "remove", name, "not", "installed"] => {
                    remove_by_packet(machine, name, false);
                }
                other => panic!("marca desconhecida: {other:?}"),
            }
        }
        // O rastro desta cena grava só os campos de upgrade e o estado da máquina.
        let actual = machine_state(machine, &expected["t"], &expected["marks"]);
        assert_eq!(actual, expected, "tick {}", expected["t"]);
    }
}

/// Um clique com o cursor num slot de upgrade, como o `Slot` do vanilla trata `PICKUP` com o botão esquerdo: o que cabe entra, o
/// que sobra fica no cursor; um item que não empilha com o do slot troca de lugar com ele.
fn click_slot(slot: &InventorySlot, carried: &mut Option<(ItemResource, u32)>) {
    let Some((resource, amount)) = carried.clone() else {
        return;
    };
    let transaction = Transaction::open_root();
    let inserted = slot.insert(&resource, amount, &transaction, AutomationType::Manual);
    if inserted > 0 {
        transaction.commit();
        *carried = (amount > inserted).then(|| (resource, amount - inserted));
        return;
    }
    drop(transaction);
    // Não coube: se o slot tem outro item e aceita o do cursor, trocam.
    if let Some(held) = slot.resource() {
        let held_amount = slot.amount();
        if held != resource && slot.is_valid(&resource) {
            slot.set_contents(Some(resource), amount, None);
            *carried = Some((held, held_amount));
        }
    }
}

fn carried_json(carried: &Option<(ItemResource, u32)>) -> Value {
    match carried {
        None => json!(["", 0]),
        Some((resource, amount)) => {
            let kind = upgrade_type_of(resource)
                .map(|kind| kind.trim_start_matches("mekanism:").to_owned());
            let id = match kind {
                Some(kind) => format!("{}:{kind}", resource.id.trim_start_matches("mekanism:")),
                None => resource.id.to_string(),
            };
            json!([id, amount])
        }
    }
}

#[test]
fn installing_through_the_window_follows_the_original_trace() {
    let mut world = chamber();
    let mut sessions = Sessions::<u32, u128, u8>::default();
    let mut carried: Option<(ItemResource, u32)> = None;
    // O que o contêiner tinha ao fechar: o rastro continua gravando a contagem depois disso.
    let mut last_tracked = 0;
    let window = SelectedWindow::new(UPGRADE_WINDOW, 0);
    for expected in lines("chamber_upgrade_window_install") {
        world.tick();
        let machine = &world.block_entity.0;
        for mark in actions(&expected) {
            let words: Vec<&str> = mark.split_whitespace().collect();
            match words.as_slice() {
                ["open", "container"] => sessions.open(CONTAINER, PLAYER, ELECTRIC_TRACKED_BASE),
                ["select", "upgrade", "window"] => {
                    sessions.select(CONTAINER, PLAYER, Some(window));
                }
                ["track", "upgrades"] => {
                    let session = sessions.session_mut(CONTAINER, PLAYER).unwrap();
                    session
                        .tracking
                        .start(UPGRADE_TRACKING_KEY, UPGRADE_TRACKED_ITEMS);
                }
                ["stop", "tracking"] => {
                    let session = sessions.session_mut(CONTAINER, PLAYER).unwrap();
                    session.tracking.stop(&UPGRADE_TRACKING_KEY);
                }
                ["deselect", "window"] => {
                    sessions.select(CONTAINER, PLAYER, None);
                }
                ["close", "container"] => {
                    last_tracked = sessions
                        .session(CONTAINER, PLAYER)
                        .unwrap()
                        .tracking
                        .count();
                    sessions.close(CONTAINER, PLAYER);
                }
                ["click", "upgrade", "slot", "with", name, count, ..] => {
                    let resource = machine.upgrades().data().resource_of(&upgrade_id(name));
                    carried = Some((resource, count.parse().unwrap()));
                    click_slot(machine.upgrades().upgrade_slot(), &mut carried);
                }
                ["drop", "what", "is", "carried"] => carried = None,
                // Só descreve o cenário: o clique ignora a janela, como o `clicked` do contêiner em Java.
                ["no", "window"] => {}
                other => panic!("marca desconhecida: {other:?}"),
            }
        }
        let mut actual = machine_state(machine, &expected["t"], &expected["marks"]);
        let sync = UpgradeSync::of(machine.upgrades());
        // O que o cliente lê dos data slots é o que o servidor tem, depois dos 16 bits.
        let wire = mekanism_rules::ELECTRIC_DATA;
        let mut data = vec![0; wire.len];
        mekanism_rules::write_upgrade_sync(&mut data, &wire, &sync);
        let read = mekanism_rules::read_upgrade_sync(&wire, &data);
        assert_eq!(read, sync);
        actual["upgrade_ticks"] = sync.ticks.into();
        let map: serde_json::Map<String, Value> = machine
            .upgrades()
            .data()
            .type_ids()
            .iter()
            .zip(&sync.counts)
            .filter(|(_, count)| **count > 0)
            .map(|(id, count)| (id.trim_start_matches("mekanism:").to_owned(), json!(count)))
            .collect();
        actual["upgrade_map"] = Value::Object(map);
        // Os campos do contêiner só existem no rastro depois que ele abriu.
        if expected.get("window").is_some() {
            let session = sessions.session(CONTAINER, PLAYER);
            let selected = sessions.selected(CONTAINER, PLAYER);
            actual["window"] = json!(if selected == Some(window) {
                "upgrade"
            } else {
                "none"
            });
            actual["tracked"] = json!(session.map_or(last_tracked, |s| s.tracking.count()));
            actual["carried"] = carried_json(&carried);
            actual["players"] = json!(sessions.players(CONTAINER));
        }
        assert_eq!(actual, expected, "tick {}", expected["t"]);
    }
}

/// O contêiner dos rastros `*_container_tracking`: uma sessão, e as posições que cada pedido acrescentou ou tirou.
fn replay_tracking(scenario: &str, base: usize) {
    let mut sessions = Sessions::<u32, u128, u8>::default();
    let window = SelectedWindow::new(UPGRADE_WINDOW, 0);
    let mut count = base;
    for expected in lines_unbounded(scenario) {
        let mut added: Vec<usize> = Vec::new();
        let mut removed: Vec<usize> = Vec::new();
        for mark in marks(&expected) {
            match mark {
                "container created" => {
                    sessions.open(CONTAINER, PLAYER, base);
                    added = (0..base).collect();
                }
                "PacketWindowSelect upgrade" => {
                    sessions.select(CONTAINER, PLAYER, Some(window));
                }
                "PacketWindowSelect none" => {
                    sessions.select(CONTAINER, PLAYER, None);
                }
                mark if mark.starts_with("PacketGuiInteract CONTAINER_TRACK_UPGRADES") => {
                    let tracking = &mut sessions.session_mut(CONTAINER, PLAYER).unwrap().tracking;
                    added = tracking.start(UPGRADE_TRACKING_KEY, UPGRADE_TRACKED_ITEMS);
                }
                "PacketGuiInteract CONTAINER_STOP_TRACKING" => {
                    let tracking = &mut sessions.session_mut(CONTAINER, PLAYER).unwrap().tracking;
                    removed = tracking.stop(&UPGRADE_TRACKING_KEY);
                }
                "container closed" => {
                    count = sessions
                        .session(CONTAINER, PLAYER)
                        .unwrap()
                        .tracking
                        .count();
                    sessions.close(CONTAINER, PLAYER);
                }
                other => panic!("marca desconhecida: {other:?}"),
            }
        }
        if let Some(session) = sessions.session(CONTAINER, PLAYER) {
            count = session.tracking.count();
        }
        let t = &expected["t"];
        assert_eq!(
            json!(count),
            expected["count"],
            "{scenario}, tick {t}: count"
        );
        assert_eq!(
            expected["tracked"].as_array().unwrap().len(),
            count,
            "{scenario}, tick {t}: tracked"
        );
        let positions = |key: &str| -> Vec<usize> {
            expected[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| entry[0].as_u64().unwrap() as usize)
                .collect()
        };
        assert_eq!(added, positions("added"), "{scenario}, tick {t}: added");
        assert_eq!(
            removed,
            positions("removed"),
            "{scenario}, tick {t}: removed"
        );
        let selected = sessions.selected(CONTAINER, PLAYER);
        assert_eq!(
            json!(if selected == Some(window) {
                "upgrade"
            } else {
                "none"
            }),
            expected["window"],
            "{scenario}, tick {t}: window"
        );
        assert_eq!(
            json!(sessions.players(CONTAINER)),
            expected["players"],
            "{scenario}, tick {t}: players"
        );
    }
}

/// Os rastros de contêiner têm só oito linhas.
fn lines_unbounded(scenario: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(format!("{scenario}.jsonl"));
    let lines: Vec<Value> = std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 8, "{scenario}");
    lines
}

#[test]
fn the_chamber_container_tracking_follows_the_original_trace() {
    replay_tracking("chamber_container_tracking", ELECTRIC_TRACKED_BASE);
}

#[test]
fn the_infuser_container_tracking_follows_the_original_trace() {
    replay_tracking("infuser_container_tracking", INFUSER_TRACKED_BASE);
}

/// O menu do rastro de shift-clique, com os índices do Java: 0 e 1 são os slots virtuais, 2 a 4 os da máquina, 5 a 31 o
/// inventário e 32 a 40 a barra. O port põe os virtuais no fim, mas a ordem de `slots_of` é a do Java.
struct ChamberMenu {
    machine: ProgressMachine,
    window: Option<SelectedWindow>,
    player: Vec<Option<(ItemResource, u32)>>,
}

const JAVA_VIRTUAL_INPUT: usize = 0;
const JAVA_VIRTUAL_OUTPUT: usize = 1;
const JAVA_MAIN: usize = 5;
const JAVA_HOTBAR: usize = 32;

impl ChamberMenu {
    fn machine_slot(&self, slot: usize) -> &InventorySlot {
        match slot {
            JAVA_VIRTUAL_INPUT => self.machine.upgrades().upgrade_slot(),
            JAVA_VIRTUAL_OUTPUT => self.machine.upgrades().upgrade_output_slot(),
            2 => self.machine.input(),
            3 => self.machine.output(),
            4 => self.machine.energy_slot(),
            _ => unreachable!("não é slot da máquina"),
        }
    }
}

impl QuickMenu for ChamberMenu {
    type Item = ItemResource;

    fn slots_of(&self, region: Region) -> Vec<usize> {
        match region {
            Region::Container => (0..5).collect(),
            Region::Main => (JAVA_MAIN..JAVA_HOTBAR).collect(),
            Region::Hotbar => (JAVA_HOTBAR..JAVA_HOTBAR + 9).collect(),
            Region::Armor | Region::Offhand => Vec::new(),
        }
    }

    fn region(&self, slot: usize) -> Region {
        match slot {
            0..5 => Region::Container,
            JAVA_MAIN..JAVA_HOTBAR => Region::Main,
            _ => Region::Hotbar,
        }
    }

    fn exists(&self, slot: usize) -> bool {
        // `VirtualInventoryContainerSlot.exists`: só com a janela dele escolhida.
        match slot {
            JAVA_VIRTUAL_INPUT | JAVA_VIRTUAL_OUTPUT => {
                SelectedWindow::exists(SelectedWindow::new(UPGRADE_WINDOW, 0), self.window)
            }
            _ => true,
        }
    }

    fn stack(&self, slot: usize) -> Option<(ItemResource, u32)> {
        if slot < 5 {
            let machine = self.machine_slot(slot);
            return machine
                .resource()
                .map(|resource| (resource, machine.amount()));
        }
        self.player[slot - JAVA_MAIN].clone()
    }

    fn insert(&mut self, slot: usize, resource: &ItemResource, amount: u32) -> u32 {
        if slot < 5 {
            // As regras de quem pode entrar nos slots da máquina (`RecipeInputInventorySlot` e o slot de energia) são do glue, não
            // do slot da lib: a entrada só aceita o que uma receita usa, e o slot de energia nada que este cenário tenha.
            let accepted = match slot {
                2 => matches!(&*resource.id, "clay" | "stone" | "hdpe_pellet"),
                4 => false,
                _ => true,
            };
            if !accepted {
                return 0;
            }
            let transaction = Transaction::open_root();
            let inserted = self.machine_slot(slot).insert(
                resource,
                amount,
                &transaction,
                AutomationType::Manual,
            );
            transaction.commit();
            return inserted;
        }
        match &mut self.player[slot - JAVA_MAIN] {
            Some((held, count)) if held == resource => {
                let moved = amount.min(resource.max_stack_size - *count);
                *count += moved;
                moved
            }
            Some(_) => 0,
            empty => {
                let moved = amount.min(resource.max_stack_size);
                *empty = Some((resource.clone(), moved));
                moved
            }
        }
    }

    fn remove(&mut self, slot: usize, amount: u32) {
        if slot < 5 {
            let machine = self.machine_slot(slot);
            if let Some(resource) = machine.resource() {
                let transaction = Transaction::open_root();
                machine.extract(&resource, amount, &transaction, AutomationType::Manual);
                transaction.commit();
            }
            return;
        }
        let entry = &mut self.player[slot - JAVA_MAIN];
        if let Some((resource, count)) = entry.take() {
            *entry = (count > amount).then_some((resource, count - amount));
        }
    }
}

/// O rastro grava só os slots com algo: `[índice, classe, tipo, quantidade]`, com o tipo `upgrade:speed` para um upgrade.
fn menu_slots_json(menu: &ChamberMenu) -> Value {
    let class = menu_slot_class;
    let name = |resource: &ItemResource| match upgrade_type_of(resource) {
        Some(kind) => format!(
            "{}:{}",
            resource.id.trim_start_matches("mekanism:"),
            kind.trim_start_matches("mekanism:")
        ),
        None => resource.id.to_string(),
    };
    let mut slots = Vec::new();
    // O rastro lista os virtuais mesmo escondidos: `getSlot(i).getItem()` não olha a janela.
    for slot in 0..JAVA_MAIN + 36 {
        if let Some((resource, amount)) = menu.stack(slot) {
            slots.push(json!([slot, class(slot), name(&resource), amount]));
        }
    }
    Value::Array(slots)
}

#[test]
fn the_quick_move_follows_the_original_trace() {
    let world = chamber();
    let mut menu = ChamberMenu {
        machine: world.block_entity.0,
        window: None,
        player: vec![None; 36],
    };
    let window = SelectedWindow::new(UPGRADE_WINDOW, 0);
    let upgrade = |menu: &ChamberMenu, name: &str| {
        menu.machine
            .upgrades()
            .data()
            .resource_of(&upgrade_id(name))
    };
    let put = |menu: &mut ChamberMenu, slot: usize, resource: ItemResource, count: u32| {
        menu.player[slot - JAVA_MAIN] = Some((resource, count));
    };
    let rows = lines_unbounded_any("chamber_upgrade_slot_quick_move");
    assert_eq!(rows.len(), 14);
    // O primeiro passo só descreve o menu: a classe de cada slot, na ordem do Java, e nada dentro.
    let layout = rows[0]["layout"].as_array().unwrap();
    assert_eq!(layout.len(), 5 + 36);
    for entry in layout {
        let slot = entry[0].as_u64().unwrap() as usize;
        assert_eq!(entry[1], menu_slot_class(slot), "classe do slot {slot}");
    }
    for (step, expected) in rows.iter().enumerate().skip(1) {
        let t = expected["t"].as_u64().unwrap();
        assert_eq!(t as usize, step);
        // A marca diz o que o cenário em Java fez antes do shift-clique deste passo.
        let mark = marks(expected).join(", ");
        match t {
            1 => {
                assert_eq!(mark, "no window: upgrade speed 3 from main slot 9");
                let resource = upgrade(&menu, "speed");
                put(&mut menu, 5, resource, 3);
            }
            2 => {
                assert_eq!(mark, "no window: clay 10 from main slot 10");
                put(&mut menu, 6, item("clay"), 10);
            }
            3 => {
                assert_eq!(
                    mark,
                    "select upgrade window, window: upgrade speed 3 from main slot 11"
                );
                menu.window = Some(window);
                let resource = upgrade(&menu, "speed");
                put(&mut menu, 7, resource, 3);
            }
            4 => {
                assert_eq!(
                    mark,
                    "window: upgrade energy 2 from main slot 12, input slot already has speed"
                );
                let resource = upgrade(&menu, "energy");
                put(&mut menu, 8, resource, 2);
            }
            5 => {
                assert_eq!(
                    mark,
                    "window: upgrade the machine does not support from main slot 13, unsupported upgrade is mekanism:anchor"
                );
                let resource = upgrade(&menu, "anchor");
                put(&mut menu, 9, resource, 1);
            }
            6 => {
                assert_eq!(mark, "window: clay 10 from main slot 14");
                put(&mut menu, 10, item("clay"), 10);
            }
            7 => assert_eq!(mark, "window: upgrade speed 3 from hotbar slot 0"),
            8 => assert_eq!(
                mark,
                "window: from the virtual input slot back to the inventory"
            ),
            9 => {
                assert_eq!(
                    mark,
                    "window: output slot holds energy 3, quick move it out"
                );
                set_slot(
                    menu.machine.upgrades().upgrade_output_slot(),
                    &menu.machine,
                    "energy",
                    3,
                );
            }
            10 => {
                assert_eq!(mark, "deselect window, input slot holds speed 2");
                menu.window = None;
                set_slot(
                    menu.machine.upgrades().upgrade_slot(),
                    &menu.machine,
                    "speed",
                    2,
                );
            }
            11 => assert_eq!(mark, "no window: quick move the virtual input slot"),
            12 => {
                assert_eq!(mark, "no window: output slot holds energy 3, quick move it");
                set_slot(
                    menu.machine.upgrades().upgrade_output_slot(),
                    &menu.machine,
                    "energy",
                    3,
                );
            }
            13 => assert_eq!(
                mark,
                "machine slots: quick move the clay in the input slot out"
            ),
            other => panic!("passo desconhecido: {other}"),
        }
        let returned = match expected.get("from").and_then(Value::as_u64) {
            Some(from) => {
                let before = menu.stack(from as usize);
                match quick_move(&mut menu, from as usize) {
                    Some(moved) => {
                        let resource = before.expect("o slot tinha algo").0;
                        carried_json(&Some((resource, moved)))
                    }
                    None => json!(["", 0]),
                }
            }
            None => Value::Null,
        };
        if expected.get("returned").is_some() {
            assert_eq!(returned, expected["returned"], "passo {t}: o que voltou");
        }
        assert_eq!(
            expected["window"],
            json!(if menu.window.is_some() {
                "upgrade"
            } else {
                "none"
            }),
            "passo {t}: janela"
        );
        assert_eq!(
            menu_slots_json(&menu),
            expected["slots"],
            "passo {t}: {mark}"
        );
        // O `upgrade_ticks` gravado no passo 10 é o do componente, parado porque o slot de entrada ainda não passou por um tick.
        if let Some(ticks) = expected.get("upgrade_ticks") {
            assert_eq!(
                json!(menu.machine.upgrades().upgrade_ticks()),
                *ticks,
                "passo {t}: upgradeTicks"
            );
        }
    }
}

fn menu_slot_class(slot: usize) -> &'static str {
    match slot {
        0 | 1 => "VirtualInventoryContainerSlot",
        2 | 4 => "InventoryContainerSlot",
        3 => "InventoryContainerResultSlot",
        JAVA_MAIN..JAVA_HOTBAR => "MainInventorySlot",
        _ => "HotBarSlot",
    }
}

fn lines_unbounded_any(scenario: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(format!("{scenario}.jsonl"));
    std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn the_window_type_table_matches_the_java_enum() {
    assert_eq!(WINDOW_TYPES.len(), 11);
    for (index, window) in WINDOW_TYPES.iter().enumerate() {
        assert_eq!(usize::from(window.ordinal), index);
    }
    assert_eq!(UPGRADE_WINDOW.save_name, Some("upgrade"));
    assert!(std::hint::black_box(UPGRADE_WINDOW).can_pin);
    let _: WindowType = UPGRADE_WINDOW;
}

#[test]
fn the_data_declares_as_many_upgrade_types_as_the_menu_has_slots_for() {
    assert_eq!(common::upgrade_data().type_ids().len(), UPGRADE_TYPES);
}

fn content() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("content/assets/mekanism")
}

/// Os sprites que a aba e a janela desenham (`windows.rs`).
const WINDOW_SPRITES: [&str; 33] = [
    "base",
    "shadow",
    "blur",
    "holder_right",
    "element_holder",
    "inner_screen",
    "button",
    "button_focus",
    "button_inactive",
    "button_digital",
    "button_digital_focus",
    "button_digital_inactive",
    "button/close",
    "button/pinned",
    "button/unpinned",
    "button/upgrade",
    "list/base",
    "list/hovered",
    "list/selected",
    "list/slot",
    "list/slot_hovered",
    "list/slot_selected",
    "scroll/list_border",
    "scroll/list_bar",
    "progress/installing/empty",
    "progress/installing/full",
    "progress/uninstalling/empty",
    "progress/uninstalling/full",
    "slot/normal",
    "slot/overlay/upgrade",
    "arrow/up",
    "arrow/down",
    "arrow/right",
];

#[test]
fn every_sprite_the_window_draws_exists() {
    for name in WINDOW_SPRITES {
        let path = content().join(format!("textures/gui/sprites/{name}.png"));
        assert!(path.is_file(), "falta o sprite {name}");
    }
}

#[test]
fn the_nine_slice_constants_match_the_mcmeta_files() {
    use machinery_basis_core::upgrade_window as uw;
    let pairs = [
        ("holder_right", uw::HOLDER_SLICE),
        ("shadow", uw::SHADOW_SLICE),
        ("button", uw::BUTTON_SLICE),
        ("button_digital", uw::DIGITAL_SLICE),
        ("element_holder", uw::ELEMENT_HOLDER_SLICE),
        ("inner_screen", uw::INNER_SCREEN_SLICE),
        ("scroll/list_border", uw::LIST_BORDER_SLICE),
        ("list/base", uw::LIST_ROW_SLICE),
        ("list/hovered", uw::LIST_ROW_SLICE),
        ("list/selected", uw::LIST_ROW_SLICE),
        ("base", mekanism_rules::PANEL_SLICE),
    ];
    for (name, slice) in pairs {
        let text = std::fs::read_to_string(
            content().join(format!("textures/gui/sprites/{name}.png.mcmeta")),
        )
        .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(
            machinery_basis_core::gui::NineSlice::from_mcmeta(&text),
            Some(slice),
            "{name}"
        );
    }
}

#[test]
fn the_window_texts_and_the_upgrade_icons_exist_for_every_declared_type() {
    let lang: serde_json::Map<String, Value> =
        serde_json::from_str(&std::fs::read_to_string(content().join("lang/en_us.json")).unwrap())
            .unwrap();
    for key in [
        "gui.mekanism.upgrades",
        "gui.mekanism.upgrades.no_selection",
        "gui.mekanism.upgrades.supported",
        "gui.mekanism.upgrades.amount",
        "gui.mekanism.upgrades.effect",
        "gui.mekanism.upgrade",
        "gui.mekanism.upgrade.not_supported",
        "gui.mekanism.upgrade.uninstall",
        "gui.mekanism.upgrade.uninstall.tooltip",
        "gui.mekanism.close",
        "gui.mekanism.pin",
        "gui.mekanism.unpin",
    ] {
        assert!(lang.contains_key(key), "falta o texto {key}");
    }
    let types = common::upgrade_data().type_ids();
    assert_eq!(types.len(), UPGRADE_TYPES);
    for id in types {
        let name = id.trim_start_matches("mekanism:");
        for key in [
            format!("upgrade.mekanism.{name}"),
            format!("upgrade.mekanism.{name}.description"),
        ] {
            assert!(lang.contains_key(&key), "falta o texto {key}");
        }
        for file in [
            format!("textures/item/upgrade/{name}.png"),
            format!("models/item/upgrade/{name}.json"),
        ] {
            assert!(content().join(&file).is_file(), "falta {file}");
        }
    }
}

#[test]
fn every_declared_upgrade_type_has_a_known_color() {
    let folder =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("content/data/mekanism/mekanism/upgrade");
    for entry in std::fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        let json: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let color = json["color"].as_str().unwrap();
        assert_ne!(
            mekanism_rules::enum_color(color),
            0xFFFF_FFFF,
            "{}: cor {color}",
            path.display()
        );
    }
}
