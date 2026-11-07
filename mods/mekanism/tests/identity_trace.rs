//! Repete os rastros `identity_<máquina>.jsonl` (`IdentityTraceTest`): dois itens do mesmo id e componentes diferentes (o nome e os
//! dados) diante do slot de entrada das quatro máquinas. O `ItemResource` do NeoForge compara o item e os componentes, então um
//! slot com `clay` de nome A não aceita `clay` de nome B nem `clay` sem nome, e aceita outro de nome A.
//!
//! - **Hopper de cima**, tick a tick: o hopper entrega um item por vez (`Cooldown`), escolhendo o primeiro item dele, na ordem dos
//!   slots, que a entrada aceita e tem lugar para ele (`hopper::first_taker`, a escolha que o glue faz), e a entrada recebe pelo
//!   `InventorySlot::insert` de verdade.
//! - **Shift-clique** (`quickMoveStack`), nos dois sentidos: da barra de atalhos e do inventário para a entrada, e da entrada para o
//!   jogador. O menu usa os slots da lib e o `room_for` para o inventário do jogador.
mod common;

use machinery_basis_core::components::Components;
use machinery_basis_core::containers::{
    Access, AutomationType, InventorySlot, ItemResource, SlotChecks, SlotContents, room_for,
};
use machinery_basis_core::hopper::{Cooldown, Offer, first_taker};
use machinery_basis_core::quick_move::{QuickMenu, Region, quick_move};
use machinery_basis_core::tile::{ProgressMachine, energy_can_drain};
use machinery_basis_core::transaction::Transaction;
use serde_json::{Value, json};
use std::path::Path;

const MACHINES: [&str; 4] = [
    "enrichment_chamber",
    "crusher",
    "energized_smelter",
    "metallurgic_infuser",
];
/// Menos linhas do que isto é um rastro cortado: 5 fases de hopper com 40 ticks, mais 8 de shift-clique, mais a linha da máquina.
const MINIMUM_LINES: usize = 200 + 8;
/// O hopper entrega um item a cada 8 ticks.
const HOPPER_PHASES: usize = 5;

fn read_trace(machine: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(format!("identity_{machine}.jsonl"));
    let lines: Vec<Value> = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(
        lines.len() >= MINIMUM_LINES,
        "rastro curto: {}",
        lines.len()
    );
    lines
}

/// `["id", "nome", "dados", quantidade]` do rastro.
fn stack(json: &Value) -> Option<(ItemResource, u32)> {
    let [id, name, data, count] = json.as_array().unwrap().as_slice() else {
        panic!("item {json}");
    };
    let count = count.as_u64().unwrap() as u32;
    if count == 0 {
        return None;
    }
    Some((
        resource(
            id.as_str().unwrap(),
            name.as_str().unwrap(),
            data.as_str().unwrap(),
        ),
        count,
    ))
}

/// O item com o nome e os dados que o cenário em Java lhe deu (`custom_name` e `custom_data`), cada um um componente.
fn resource(id: &str, name: &str, data: &str) -> ItemResource {
    let mut components = Vec::new();
    let name_value = Components::text_value(name);
    if !name.is_empty() {
        components.push(("minecraft:custom_name", name_value.as_str()));
    }
    let data_value = format!("{{d:{data}}}");
    if !data.is_empty() {
        components.push(("minecraft:custom_data", data_value.as_str()));
    }
    ItemResource::with_components(id, 64, Components::new(components))
}

/// O que o rastro diz de um conteúdo: `[id, nome, dados, quantidade]`.
fn json_of(resource: &Option<ItemResource>, amount: u32) -> Value {
    let Some(resource) = resource else {
        return json!(["", "", "", 0]);
    };
    let name = resource
        .components
        .get("minecraft:custom_name")
        .and_then(Components::parse_text_value)
        .unwrap_or_default();
    let data = resource
        .components
        .get("minecraft:custom_data")
        .map_or(String::new(), |value| {
            value
                .trim_start_matches("{d:")
                .trim_end_matches('}')
                .to_owned()
        });
    json!([resource.id.to_string(), name, data, amount])
}

fn slot_json(slot: &InventorySlot) -> Value {
    json_of(&slot.resource(), slot.amount())
}

/// O hopper de cima: cinco slots e o ritmo de oito ticks.
struct Hopper {
    slots: Vec<Option<(ItemResource, u32)>>,
    cooldown: Cooldown,
}

impl Hopper {
    fn json(&self) -> Value {
        Value::Array(
            self.slots
                .iter()
                .map(|slot| match slot {
                    Some((resource, amount)) => json_of(&Some(resource.clone()), *amount),
                    None => json!(["", "", "", 0]),
                })
                .collect(),
        )
    }

    /// O que o glue faz com um hopper apontado para a máquina (`take_one`): se o ritmo deixa, leva um item para a entrada.
    fn tick(&mut self, input: &InventorySlot) {
        if !self.cooldown.tick() {
            return;
        }
        let offers: Vec<Offer<usize>> = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| {
                let (resource, _) = slot.as_ref()?;
                Some(Offer {
                    slot: index,
                    resource: resource.clone(),
                    payload: index,
                })
            })
            .collect();
        let accepts = |_: usize, offer: &Offer<usize>| {
            input.is_valid_for_insertion(&offer.resource, AutomationType::External)
        };
        let Some((offer, _)) = first_taker(&offers, &[(0, input.contents())], accepts) else {
            return;
        };
        let offer = &offers[offer];
        let transaction = Transaction::open_root();
        let moved = input.insert(&offer.resource, 1, &transaction, AutomationType::External);
        transaction.commit();
        assert_eq!(moved, 1, "o slot recusou o que o hopper escolheu");
        let slot = &mut self.slots[offer.payload];
        let (resource, amount) = slot.take().unwrap();
        *slot = (amount > 1).then_some((resource, amount - 1));
        self.cooldown.moved();
    }
}

fn set_input(machine: &ProgressMachine, contents: &Value) {
    match stack(contents) {
        Some((resource, amount)) => machine.input().set_contents(Some(resource), amount, None),
        None => machine.input().set_contents(None, 0, None),
    }
}

/// O Heat Generator: o slot de combustível é um `BasicInventorySlot` que só aceita o que queima (`coal`, no cenário). O rastro
/// mantém o tanque de lava cheio, então o gerador não queima o que o hopper põe.
#[test]
fn a_hopper_above_the_heat_generator_stacks_only_the_same_item_and_components() {
    let lines = read_trace("heat_generator");
    let fuel = InventorySlot::with_checks(
        machinery_basis_core::containers::Access::Any,
        machinery_basis_core::containers::Access::Any,
        machinery_basis_core::containers::SlotChecks {
            validator: Some(std::rc::Rc::new(|resource: &ItemResource| {
                &*resource.id == "coal"
            })),
            ..Default::default()
        },
        None,
        None,
    );
    let mut hopper = Hopper {
        slots: Vec::new(),
        cooldown: Cooldown::default(),
    };
    let mut compared = 0;
    for line in lines.iter().filter(|line| {
        line["phase"]
            .as_str()
            .is_some_and(|p| p.starts_with("hopper_"))
    }) {
        let phase = line["phase"].as_str().unwrap();
        let t = line["t"].as_u64().unwrap();
        if t == 0 {
            match stack(&line["slots"][0]) {
                Some((resource, amount)) => fuel.set_contents(Some(resource), amount, None),
                None => fuel.set_contents(None, 0, None),
            }
            hopper = Hopper {
                slots: line["hopper"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(stack)
                    .collect(),
                cooldown: Cooldown::default(),
            };
        } else {
            hopper.tick(&fuel);
        }
        assert_eq!(
            line["hopper"],
            hopper.json(),
            "heat_generator {phase} t={t}: hopper"
        );
        assert_eq!(
            line["slots"][0],
            slot_json(&fuel),
            "heat_generator {phase} t={t}: combustível"
        );
        compared += 1;
    }
    assert_eq!(compared, HOPPER_PHASES * 40);
}

/// O número do slot de entrada na lista `slots` do rastro, por máquina.
fn input_index(name: &str) -> usize {
    if name == "metallurgic_infuser" { 1 } else { 0 }
}

#[test]
fn a_hopper_above_stacks_only_the_same_item_and_components() {
    for name in MACHINES {
        let lines = read_trace(name);
        let machine = common::machine(name);
        let mut hopper = Hopper {
            slots: Vec::new(),
            cooldown: Cooldown::default(),
        };
        let mut compared = 0;
        for line in lines.iter().filter(|line| {
            line["phase"]
                .as_str()
                .is_some_and(|p| p.starts_with("hopper_"))
        }) {
            let phase = line["phase"].as_str().unwrap();
            let t = line["t"].as_u64().unwrap();
            let slots = line["slots"].as_array().unwrap();
            if t == 0 {
                // O cenário em Java põe a entrada e o hopper e só então grava: nada se move neste tick.
                set_input(&machine, &slots[input_index(name)]);
                hopper = Hopper {
                    slots: line["hopper"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(stack)
                        .collect(),
                    cooldown: Cooldown::default(),
                };
            } else {
                hopper.tick(machine.input());
            }
            assert_eq!(
                line["hopper"],
                hopper.json(),
                "{name} {phase} t={t}: hopper"
            );
            assert_eq!(
                slots[input_index(name)],
                slot_json(machine.input()),
                "{name} {phase} t={t}: entrada"
            );
            compared += 1;
        }
        assert_eq!(compared, HOPPER_PHASES * 40, "{name}: linhas de hopper");
    }
}

/// O menu do shift-clique com os índices do Java: dois slots virtuais, a infusão no Infuser, a entrada, a saída e a energia, depois
/// o inventário (27) e a barra (9).
struct Menu {
    machine: ProgressMachine,
    infuser: bool,
    player: Vec<SlotContents>,
}

impl Menu {
    fn first_main(&self) -> usize {
        self.container_slots()
    }

    fn container_slots(&self) -> usize {
        if self.infuser { 6 } else { 5 }
    }

    fn first_hotbar(&self) -> usize {
        self.container_slots() + 27
    }

    /// O slot da máquina com este número, ou `None` para os do jogador.
    fn machine_slot(&self, slot: usize) -> Option<&InventorySlot> {
        let infusion = usize::from(self.infuser);
        match slot {
            0 => Some(self.machine.upgrades().upgrade_slot()),
            1 => Some(self.machine.upgrades().upgrade_output_slot()),
            2 if self.infuser => Some(&self.machine.chemical_side().unwrap().slot),
            n if n == 2 + infusion => Some(self.machine.input()),
            n if n == 3 + infusion => Some(self.machine.output()),
            n if n == 4 + infusion => Some(self.machine.energy_slot()),
            _ => None,
        }
    }

    fn player_index(&self, slot: usize) -> usize {
        // O inventário do jogador: a barra é 0 a 8 e o principal 9 a 35.
        if slot >= self.first_hotbar() {
            slot - self.first_hotbar()
        } else {
            9 + slot - self.first_main()
        }
    }
}

impl QuickMenu for Menu {
    type Item = ItemResource;

    fn slots_of(&self, region: Region) -> Vec<usize> {
        match region {
            Region::Container => (0..self.container_slots()).collect(),
            Region::Main => (self.first_main()..self.first_hotbar()).collect(),
            Region::Hotbar => (self.first_hotbar()..self.first_hotbar() + 9).collect(),
            Region::Armor | Region::Offhand => Vec::new(),
        }
    }

    fn region(&self, slot: usize) -> Region {
        if slot < self.container_slots() {
            Region::Container
        } else if slot < self.first_hotbar() {
            Region::Main
        } else {
            Region::Hotbar
        }
    }

    fn exists(&self, slot: usize) -> bool {
        // Os slots virtuais só existem com a janela de upgrades escolhida, e o cenário não a escolhe.
        slot >= 2
    }

    fn stack(&self, slot: usize) -> Option<(ItemResource, u32)> {
        let contents = match self.machine_slot(slot) {
            Some(machine) => machine.contents(),
            None => self.player[self.player_index(slot)].clone(),
        };
        contents
            .resource
            .map(|resource| (resource, contents.amount))
    }

    fn insert(&mut self, slot: usize, resource: &ItemResource, amount: u32) -> u32 {
        if let Some(machine) = self.machine_slot(slot) {
            let transaction = Transaction::open_root();
            let inserted = machine.insert(resource, amount, &transaction, AutomationType::Manual);
            transaction.commit();
            return inserted;
        }
        let index = self.player_index(slot);
        let held = &mut self.player[index];
        let moved = amount.min(room_for(held, resource, resource.slot_capacity()));
        if moved > 0 {
            *held = SlotContents::new(Some(resource.clone()), held.amount + moved);
        }
        moved
    }

    fn remove(&mut self, slot: usize, amount: u32) {
        if let Some(machine) = self.machine_slot(slot) {
            if let Some(resource) = machine.resource() {
                let transaction = Transaction::open_root();
                machine.extract(&resource, amount, &transaction, AutomationType::Manual);
                transaction.commit();
            }
            return;
        }
        let index = self.player_index(slot);
        let held = &mut self.player[index];
        *held = SlotContents::new(held.resource.clone(), held.amount.saturating_sub(amount));
    }
}

/// Os slots com algo, como o rastro os grava: `[índice, item]`, sem a classe do slot.
fn menu_json(menu: &Menu) -> Value {
    let mut slots = Vec::new();
    for slot in 0..menu.first_hotbar() + 9 {
        if let Some((resource, amount)) = menu.stack(slot) {
            slots.push(json!([slot, json_of(&Some(resource), amount)]));
        }
    }
    Value::Array(slots)
}

fn expected_menu(line: &Value) -> Value {
    Value::Array(
        line["menu"]
            .as_array()
            .unwrap()
            .iter()
            .map(|slot| json!([slot[0], slot[2]]))
            .collect(),
    )
}

/// A cena de cada fase do shift-clique, como `IdentityTraceTest` a monta: o que a entrada guarda, o que o jogador tem e de onde o
/// clique parte. `from` é o slot do inventário do jogador (barra 0 a 8, principal 9 a 35), ou `None` para a entrada da máquina.
struct Quick {
    label: &'static str,
    input: Option<Given>,
    player: Vec<(usize, Given)>,
    from: Option<usize>,
}

/// Um item do cenário: id, nome, dados e quantidade.
type Given = (&'static str, &'static str, &'static str, u32);

fn quick_scenes(valid: &'static str) -> Vec<Quick> {
    let item = |name, data, count| (valid, name, data, count);
    vec![
        Quick {
            label: "slot_a_player_b",
            input: Some(item("A", "", 3)),
            player: vec![(9, item("B", "", 5))],
            from: Some(9),
        },
        Quick {
            label: "slot_a_player_a",
            input: Some(item("A", "", 3)),
            player: vec![(10, item("A", "", 5))],
            from: Some(10),
        },
        Quick {
            label: "slot_a_player_plain",
            input: Some(item("A", "", 3)),
            player: vec![(11, item("", "", 5))],
            from: Some(11),
        },
        Quick {
            label: "empty_slot_player_b",
            input: None,
            player: vec![(12, item("B", "", 5))],
            from: Some(12),
        },
        Quick {
            label: "slot_data_player_data2",
            input: Some(item("", "1", 3)),
            player: vec![(13, item("", "2", 5))],
            from: Some(13),
        },
        Quick {
            label: "slot_data_player_data1",
            input: Some(item("", "1", 3)),
            player: vec![(14, item("", "1", 5))],
            from: Some(14),
        },
        Quick {
            label: "slot_a_out_hotbar_b_a",
            input: Some(item("A", "", 8)),
            player: vec![(0, item("B", "", 10)), (1, item("A", "", 4))],
            from: None,
        },
        Quick {
            label: "slot_a_out_hotbar_b_only",
            input: Some(item("A", "", 8)),
            player: vec![(0, item("B", "", 64))],
            from: None,
        },
    ]
}

#[test]
fn a_shift_click_stacks_only_the_same_item_and_components() {
    for name in MACHINES {
        let lines = read_trace(name);
        let infuser = name == "metallurgic_infuser";
        let scenes = quick_scenes(common::valid_input(name));
        let mut compared = 0;
        for scene in scenes {
            let line = lines
                .iter()
                .find(|line| line["phase"].as_str() == Some(&format!("quick_{}", scene.label)))
                .unwrap_or_else(|| panic!("{name}: sem a fase {}", scene.label));
            let machine = common::machine(name);
            let mut menu = Menu {
                machine,
                infuser,
                player: vec![SlotContents::default(); 36],
            };
            if let Some((id, item_name, data, count)) = scene.input {
                menu.machine
                    .input()
                    .set_contents(Some(resource(id, item_name, data)), count, None);
            }
            for (index, (id, item_name, data, count)) in &scene.player {
                menu.player[*index] =
                    SlotContents::new(Some(resource(id, item_name, data)), *count);
            }
            let from = match scene.from {
                Some(inventory) => {
                    let inventory_slot = inventory;
                    if inventory_slot < 9 {
                        menu.first_hotbar() + inventory_slot
                    } else {
                        menu.first_main() + inventory_slot - 9
                    }
                }
                None => 2 + usize::from(infuser),
            };
            let before = menu.stack(from);
            let moved = quick_move(&mut menu, from);
            // `quickMoveStack` devolve uma cópia do que havia no slot se algo se moveu, e vazio se nada.
            let returned = match (moved, before) {
                (Some(moved), Some((resource, amount))) if moved > 0 => {
                    json_of(&Some(resource), amount)
                }
                _ => json!(["", "", "", 0]),
            };
            assert_eq!(
                line["returned"], returned,
                "{name} {}: o que voltou",
                scene.label
            );
            assert_eq!(
                expected_menu(line),
                menu_json(&menu),
                "{name} {}: os slots",
                scene.label
            );
            compared += 1;
        }
        assert_eq!(compared, 8, "{name}: fases de shift-clique");
    }
}

/// O menu do Heat Generator com os índices do Java: dois slots virtuais da janela de upgrades (que o gerador tem no original, e que
/// só existem com a janela escolhida, o que o cenário não faz), o combustível (2), a energia (3), o inventário (4 a 30) e a barra
/// (31 a 39). O slot de combustível aceita o que queima (`coal`, `stick` e o balde de lava) e o de energia o que `energy_can_drain`: nenhum item
/// portado tem handler de energia.
struct HeatMenu {
    fuel: InventorySlot,
    energy: InventorySlot,
    player: Vec<SlotContents>,
}

const HEAT_FUEL: usize = 2;
const HEAT_ENERGY: usize = 3;
const HEAT_MAIN: usize = 4;
const HEAT_HOTBAR: usize = 31;

impl HeatMenu {
    fn new() -> Self {
        let burns =
            |resource: &ItemResource| matches!(&*resource.id, "coal" | "lava_bucket" | "stick");
        let fuel = InventorySlot::with_checks(
            Access::Any,
            Access::Any,
            SlotChecks {
                validator: Some(std::rc::Rc::new(burns)),
                ..Default::default()
            },
            None,
            None,
        );
        let energy = InventorySlot::with_checks(
            Access::Any,
            Access::Any,
            SlotChecks {
                can_insert: Some(std::rc::Rc::new(|_: &ItemResource| {
                    energy_can_drain(0, true, None)
                })),
                ..Default::default()
            },
            None,
            None,
        );
        Self {
            fuel,
            energy,
            player: vec![SlotContents::default(); 36],
        }
    }

    fn machine_slot(&self, slot: usize) -> Option<&InventorySlot> {
        match slot {
            HEAT_FUEL => Some(&self.fuel),
            HEAT_ENERGY => Some(&self.energy),
            _ => None,
        }
    }

    fn player_index(&self, slot: usize) -> usize {
        if slot >= HEAT_HOTBAR {
            slot - HEAT_HOTBAR
        } else {
            9 + slot - HEAT_MAIN
        }
    }
}

impl QuickMenu for HeatMenu {
    type Item = ItemResource;

    fn slots_of(&self, region: Region) -> Vec<usize> {
        match region {
            Region::Container => (0..HEAT_MAIN).collect(),
            Region::Main => (HEAT_MAIN..HEAT_HOTBAR).collect(),
            Region::Hotbar => (HEAT_HOTBAR..HEAT_HOTBAR + 9).collect(),
            Region::Armor | Region::Offhand => Vec::new(),
        }
    }

    fn region(&self, slot: usize) -> Region {
        if slot < HEAT_MAIN {
            Region::Container
        } else if slot < HEAT_HOTBAR {
            Region::Main
        } else {
            Region::Hotbar
        }
    }

    fn exists(&self, slot: usize) -> bool {
        slot >= HEAT_FUEL
    }

    fn stack(&self, slot: usize) -> Option<(ItemResource, u32)> {
        let contents = match self.machine_slot(slot) {
            Some(machine) => machine.contents(),
            None if slot < HEAT_FUEL => SlotContents::default(),
            None => self.player[self.player_index(slot)].clone(),
        };
        contents
            .resource
            .map(|resource| (resource, contents.amount))
    }

    fn insert(&mut self, slot: usize, resource: &ItemResource, amount: u32) -> u32 {
        if let Some(machine) = self.machine_slot(slot) {
            let transaction = Transaction::open_root();
            let inserted = machine.insert(resource, amount, &transaction, AutomationType::Manual);
            transaction.commit();
            return inserted;
        }
        let index = self.player_index(slot);
        let held = &mut self.player[index];
        let moved = amount.min(room_for(held, resource, resource.slot_capacity()));
        if moved > 0 {
            *held = SlotContents::new(Some(resource.clone()), held.amount + moved);
        }
        moved
    }

    fn remove(&mut self, slot: usize, amount: u32) {
        if let Some(machine) = self.machine_slot(slot) {
            if let Some(resource) = machine.resource() {
                let transaction = Transaction::open_root();
                machine.extract(&resource, amount, &transaction, AutomationType::Manual);
                transaction.commit();
            }
            return;
        }
        let index = self.player_index(slot);
        let held = &mut self.player[index];
        *held = SlotContents::new(held.resource.clone(), held.amount.saturating_sub(amount));
    }
}

fn heat_menu_json(menu: &HeatMenu) -> Value {
    let mut slots = Vec::new();
    for slot in 0..HEAT_HOTBAR + 9 {
        if let Some((resource, amount)) = menu.stack(slot) {
            slots.push(json!([slot, json_of(&Some(resource), amount)]));
        }
    }
    Value::Array(slots)
}

/// Uma cena do shift-clique do Heat Generator: o nome, o combustível, os itens do jogador e o slot de onde parte o clique.
type HeatScene = (
    &'static str,
    Option<Given>,
    Vec<(usize, Given)>,
    Option<usize>,
);

/// O rastro do Heat Generator tem, além das oito cenas das máquinas, o que o gerador recusa e o que o balde faz. As duas cenas do
/// `energy_tablet` não se repetem: o item não é portado (e deixa um tablet no slot de energia nas cenas seguintes, que se tira
/// do que se espera).
#[test]
fn a_shift_click_in_the_heat_generator_follows_the_original_trace() {
    let lines = read_trace("heat_generator");
    let item = |id, name, data, count| (id, name, data, count);
    let scenes: Vec<HeatScene> = vec![
        (
            "slot_a_player_b",
            Some(item("coal", "A", "", 3)),
            vec![(9, item("coal", "B", "", 5))],
            Some(9),
        ),
        (
            "slot_a_player_a",
            Some(item("coal", "A", "", 3)),
            vec![(10, item("coal", "A", "", 5))],
            Some(10),
        ),
        (
            "slot_a_player_plain",
            Some(item("coal", "A", "", 3)),
            vec![(11, item("coal", "", "", 5))],
            Some(11),
        ),
        (
            "empty_slot_player_b",
            None,
            vec![(12, item("coal", "B", "", 5))],
            Some(12),
        ),
        (
            "slot_data_player_data2",
            Some(item("coal", "", "1", 3)),
            vec![(13, item("coal", "", "2", 5))],
            Some(13),
        ),
        (
            "slot_data_player_data1",
            Some(item("coal", "", "1", 3)),
            vec![(14, item("coal", "", "1", 5))],
            Some(14),
        ),
        (
            "slot_a_out_hotbar_b_a",
            Some(item("coal", "A", "", 8)),
            vec![
                (0, item("coal", "B", "", 10)),
                (1, item("coal", "A", "", 4)),
            ],
            None,
        ),
        (
            "slot_a_out_hotbar_b_only",
            Some(item("coal", "A", "", 8)),
            vec![(0, item("coal", "B", "", 64))],
            None,
        ),
        (
            "stick_player",
            Some(item("coal", "A", "", 3)),
            vec![(9, item("stick", "", "", 5))],
            Some(9),
        ),
        (
            "stick_player_fuel_empty",
            None,
            vec![(14, item("stick", "", "", 5))],
            Some(14),
        ),
        (
            "coal_player_fuel_empty",
            None,
            vec![(12, item("coal", "", "", 5))],
            Some(12),
        ),
        (
            "lava_bucket_player",
            None,
            vec![(13, item("lava_bucket", "", "", 1))],
            Some(13),
        ),
    ];
    let mut compared = 0;
    for (label, input, player, from) in scenes {
        let line = lines
            .iter()
            .find(|line| line["phase"].as_str() == Some(&format!("quick_{label}")))
            .unwrap_or_else(|| panic!("sem a fase {label}"));
        let mut menu = HeatMenu::new();
        if let Some((id, name, data, count)) = input {
            menu.fuel
                .set_contents(Some(resource(id, name, data)), count, None);
        }
        for (index, (id, name, data, count)) in &player {
            menu.player[*index] = SlotContents::new(Some(resource(id, name, data)), *count);
        }
        let from = match from {
            Some(inventory) if inventory < 9 => HEAT_HOTBAR + inventory,
            Some(inventory) => HEAT_MAIN + inventory - 9,
            None => HEAT_FUEL,
        };
        let before = menu.stack(from);
        let moved = quick_move(&mut menu, from);
        let returned = match (moved, before) {
            (Some(moved), Some((resource, amount))) if moved > 0 => {
                json_of(&Some(resource), amount)
            }
            _ => json!(["", "", "", 0]),
        };
        assert_eq!(
            line["returned"], returned,
            "heat_generator {label}: o que voltou"
        );
        // As cenas depois do tablet têm um no slot de energia, que o gerador não porta.
        let expected: Vec<Value> = line["menu"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|slot| slot[2][0] != "energy_tablet")
            .map(|slot| json!([slot[0], slot[2]]))
            .collect();
        assert_eq!(
            Value::Array(expected),
            heat_menu_json(&menu),
            "heat_generator {label}: os slots"
        );
        compared += 1;
    }
    assert_eq!(compared, 12);
}
