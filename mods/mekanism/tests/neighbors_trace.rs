//! Repete, tick a tick, os rastros `neighbors_*.jsonl` do Mekanism original (`dev/referencia/testes/NeighborTraceTest.java`): o campo `redstone`
//! das máquinas só muda quando o jogo avisa o bloco (`BlockTile.neighborChanged`, `onNeighborChange`, `onAdded`), e o `canFunction()`,
//! a atividade e o progresso seguem o `redstone` que ficou, velho ou não.
//!
//! O `SimLevel` dá as notificações de vizinhança do `setBlock` na ordem do jogo (`machinery_basis_core::neighbors`) e o replay
//! (`sim::neighbor_replay`) faz cada operação do rastro e confere o `powered` de todas as máquinas logo depois dela, no meio do tick.
mod common;

use machinery_basis_core::cached_recipe::{ItemIngredient, ItemOutput, ItemToItemRecipe};
use machinery_basis_core::chemical::{ChemicalConversion, ChemicalIngredient, ChemicalResource};
use machinery_basis_core::containers::ItemResource;
use machinery_basis_core::neighbors::{OUTPUT_SIGNAL_ORDER, UPDATE_ORDER};
use machinery_basis_core::sim::SimNeighbours;
use machinery_basis_core::sim::chunks::{BlockPos, ChunkHooks, LevelView, SimLevel};
use machinery_basis_core::sim::neighbor_replay::{
    LEVER_ON, OTHER_MACHINE, REDSTONE_BLOCK, Scene, Template, replay,
};
use machinery_basis_core::tile::{ProgressMachine, RedstoneControl, SavedState};
use machinery_basis_core::two_input_recipe::ItemChemicalToItemRecipe;
use machinery_basis_core::upgrade::UpgradeSaved;
use mekanism_rules::{
    CRUSHER_USAGE, ENRICHMENT_USAGE, INFUSER_USAGE, SMELTER_USAGE, electric_config, infuser_config,
    infusing_cache_of,
};
use serde_json::{Value, json};
use std::path::Path;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Chamber,
    Crusher,
    Smelter,
    Infuser,
}

impl Kind {
    fn block(self) -> &'static str {
        match self {
            Kind::Chamber => OTHER_MACHINE,
            Kind::Crusher => "mekanism:crusher",
            Kind::Smelter => "mekanism:energized_smelter",
            Kind::Infuser => "mekanism:metallurgic_infuser",
        }
    }

    /// O item que `fill` põe na entrada (`NeighborTraceTest.fill`), e o slot dele.
    fn input(self) -> (&'static str, bool) {
        match self {
            Kind::Chamber => ("clay", false),
            Kind::Crusher => ("cobblestone", false),
            Kind::Smelter => ("sand", false),
            Kind::Infuser => ("dirt", true),
        }
    }
}

fn item(id: &str) -> ItemResource {
    ItemResource::new(id, 64)
}

fn recipe(input: &str, output: &str, count: u32) -> Rc<ItemToItemRecipe> {
    Rc::new(ItemToItemRecipe {
        id: format!("mekanism:test/{output}"),
        input: ItemIngredient::item(input),
        input_count: 1,
        output: ItemOutput {
            resource: item(output),
            count,
        },
    })
}

fn infusing(
    input: &str,
    chemical: &str,
    amount: u32,
    output: &str,
) -> Rc<ItemChemicalToItemRecipe> {
    Rc::new(ItemChemicalToItemRecipe {
        id: format!("mekanism:metallurgic_infusing/{output}"),
        item_input: ItemIngredient::item(input),
        item_count: 1,
        chemical_input: ChemicalIngredient::chemical(chemical),
        chemical_amount: amount,
        output: ItemOutput {
            resource: item(output),
            count: 1,
        },
        per_tick_usage: false,
    })
}

fn machine_of(kind: Kind) -> ProgressMachine {
    match kind {
        Kind::Infuser => {
            let recipes = vec![infusing("dirt", "bio", 10, "podzol")];
            let conversions = vec![ChemicalConversion {
                id: String::new(),
                input: ItemIngredient::item("redstone"),
                input_count: 1,
                output: ChemicalResource::new("redstone"),
                amount: 10,
            }];
            let mut config = infuser_config(
                INFUSER_USAGE,
                infusing_cache_of(recipes),
                ChemicalConversion::cache_of(conversions),
                0,
                common::upgrade_data(),
            );
            config.items = common::item_energy();
            ProgressMachine::chemical(config)
        }
        electric => {
            let (usage, recipes) = match electric {
                Kind::Crusher => (CRUSHER_USAGE, vec![recipe("cobblestone", "gravel", 1)]),
                Kind::Smelter => (SMELTER_USAGE, vec![recipe("sand", "glass", 1)]),
                _ => (ENRICHMENT_USAGE, vec![recipe("clay", "clay_ball", 4)]),
            };
            let mut config = electric_config(
                usage,
                ItemToItemRecipe::cache_of(recipes),
                0,
                common::upgrade_data(),
            );
            config.items = common::item_energy();
            ProgressMachine::electric(config)
        }
    }
}

/// O que `saveAdditional` guarda e o `loadStatic` volta a ler: o estado do tile, a energia e os slots.
#[derive(Clone)]
struct Snapshot {
    state: SavedState,
    block_active: bool,
    energy: i64,
    input: (Option<ItemResource>, u32),
    output: (Option<ItemResource>, u32),
}

struct Entity {
    kind: Kind,
    machine: ProgressMachine,
}

struct Mek {
    kind: Kind,
    /// Quem mudou de nível de comparador durante o tick do nível.
    comparators: Vec<BlockPos>,
}

impl Mek {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            comparators: Vec::new(),
        }
    }

    fn input_slot<'a>(
        &self,
        entity: &'a Entity,
    ) -> &'a machinery_basis_core::containers::InventorySlot {
        entity.machine.input()
    }
}

impl ChunkHooks for Mek {
    type BlockEntity = Entity;
    type Saved = Snapshot;

    fn new_block_entity(
        &mut self,
        _: BlockPos,
        block: &str,
        saved: Option<&Snapshot>,
    ) -> Option<Entity> {
        let kind = if block == OTHER_MACHINE {
            Kind::Chamber
        } else if block == self.kind.block() {
            self.kind
        } else {
            return None;
        };
        let mut machine = machine_of(kind);
        if let Some(saved) = saved {
            machine.restore(&saved.state, saved.block_active);
            machine.energy().set_energy(saved.energy, None);
            machine
                .input()
                .set_contents(saved.input.0.clone(), saved.input.1, None);
            machine
                .output()
                .set_contents(saved.output.0.clone(), saved.output.1, None);
        }
        Some(Entity { kind, machine })
    }

    fn save_block_entity(&mut self, _: BlockPos, entity: &mut Entity) -> Snapshot {
        let machine = &entity.machine;
        Snapshot {
            state: machine.saved_state(),
            block_active: machine.tile().block_active(),
            energy: machine.energy().amount(),
            input: (machine.input().resource(), machine.input().amount()),
            output: (machine.output().resource(), machine.output().amount()),
        }
    }

    fn block_entity_ticks(&self, _: &str) -> bool {
        true
    }

    fn block_entity_tick(&mut self, view: &LevelView<Entity>, pos: BlockPos, entity: &mut Entity) {
        if entity
            .machine
            .tick_server(view.game_time(), &SimNeighbours::default())
        {
            self.comparators.push(pos);
        }
    }

    fn take_comparator_changes(&mut self) -> Vec<BlockPos> {
        std::mem::take(&mut self.comparators)
    }

    fn block_signal(&self, block: &str) -> i32 {
        if block == REDSTONE_BLOCK || block == LEVER_ON {
            15
        } else {
            0
        }
    }

    /// `Attributes.COMPARATOR`: toda máquina do mod tem sinal analógico.
    fn has_analog_output(&self, block: &str) -> bool {
        block == OTHER_MACHINE || block == self.kind.block()
    }

    fn neighbor_changed(
        &mut self,
        _: &LevelView<Entity>,
        _: BlockPos,
        entity: &mut Entity,
        powered: bool,
    ) {
        entity.machine.tile_mut().update_power(powered);
    }

    fn on_neighbor_change(
        &mut self,
        _: &LevelView<Entity>,
        _: BlockPos,
        entity: &mut Entity,
        powered: bool,
        _: BlockPos,
    ) {
        entity.machine.tile_mut().update_power(powered);
    }

    /// `BlockMekanism.setPlacedBy`: o item aplica os componentes (`applyImplicitComponents`) e `onAdded` lê o sinal.
    fn placed_by(
        &mut self,
        _: &LevelView<Entity>,
        _: BlockPos,
        entity: &mut Entity,
        powered: bool,
    ) {
        entity
            .machine
            .apply_item_components(&UpgradeSaved::default());
        entity.machine.tile_mut().update_power(powered);
    }
}

impl Scene for Mek {
    fn subject_block(&self) -> &'static str {
        self.kind.block()
    }

    fn powered(&self, entity: &Entity) -> bool {
        entity.machine.tile().is_powered()
    }

    fn row(&self, entity: &Entity) -> Value {
        let machine = &entity.machine;
        let tile = machine.tile();
        json!({
            "powered": tile.is_powered(),
            "was_powered": tile.redstone_sync().was_powered,
            "can_function": tile.is_redstone_activated(),
            "mode": tile.control_type().serialized_name(),
            "active": tile.active(),
            "block_active": tile.block_active(),
            "comparator": tile.current_redstone_level(),
            "progress": machine.recipe_progress(),
            "energy": machine.energy().amount(),
        })
    }

    fn set_mode(&self, entity: &Entity, mode: &str) {
        entity
            .machine
            .tile_mut()
            .set_control_type(RedstoneControl::from_serialized_name(mode));
    }

    fn fill(&self, entity: &Entity) {
        let (id, _) = entity.kind.input();
        self.input_slot(entity)
            .set_contents(Some(item(id)), 64, None);
        entity.machine.energy().set_energy(20_000, None);
    }

    fn set_other_input(&self, entity: &Entity, filled: bool) {
        let slot = entity.machine.input();
        if filled {
            slot.set_contents(Some(item("clay")), 5, None);
        } else {
            slot.set_contents(None, 0, None);
        }
    }
}

fn lines(scenario: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(format!("neighbors_{scenario}.jsonl"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn names(order: [machinery_basis_core::sides::Face; 6]) -> Vec<&'static str> {
    order.iter().map(|face| face.name()).collect()
}

fn run(scenario: &str, kind: Kind, template: Template, minimum: usize) {
    let lines = lines(scenario);
    assert!(
        lines.len() > minimum,
        "{scenario}: só {} linhas",
        lines.len()
    );
    // A primeira linha traz as ordens do jogo, lidas por reflexão: o port as tem iguais.
    let meta = &lines[0];
    let order = |key: &str| -> Vec<String> {
        meta[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|name| name.as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(order("update_order"), names(UPDATE_ORDER), "{scenario}");
    assert_eq!(
        order("output_signal_order"),
        names(OUTPUT_SIGNAL_ORDER),
        "{scenario}"
    );
    let mut level = SimLevel::new(Mek::new(kind));
    let mismatches = replay(&mut level, &template, &lines);
    assert!(
        mismatches.is_empty(),
        "{scenario}: {} diferenças, a primeira em {:?}\n{}",
        mismatches.len(),
        mismatches.first(),
        mismatches
            .iter()
            .take(12)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn the_chamber_follows_the_notifications_of_a_redstone_block_and_a_lever() {
    run(
        "chamber_notified",
        Kind::Chamber,
        Template::new(3, &[], false),
        100,
    );
}

#[test]
fn the_chamber_keeps_the_old_redstone_when_the_signal_changes_without_notice() {
    run(
        "chamber_unnotified",
        Kind::Chamber,
        Template::new(3, &[], false),
        100,
    );
}

#[test]
fn the_chamber_reads_the_signal_when_it_is_notified_in_the_middle_of_the_tick() {
    run(
        "chamber_mid_tick",
        Kind::Chamber,
        Template::new(1, &[], false),
        60,
    );
}

#[test]
fn the_chamber_placed_next_to_a_powered_block_reads_it_only_when_placed_by_a_player() {
    run(
        "chamber_placed",
        Kind::Chamber,
        Template::new(3, &[true, true, true], false),
        80,
    );
}

#[test]
fn the_chamber_comes_back_from_the_save_with_the_old_redstone() {
    run(
        "chamber_reload",
        Kind::Chamber,
        Template::new(3, &[], false),
        100,
    );
}

#[test]
fn a_neighbour_block_entity_that_changes_makes_the_chamber_read_the_signal_again() {
    run(
        "chamber_output_signal",
        Kind::Chamber,
        Template::new(2, &[], true),
        80,
    );
}

#[test]
fn the_crusher_follows_the_same_path() {
    run(
        "crusher_core",
        Kind::Crusher,
        Template::new(3, &[false, false, true], false),
        80,
    );
}

#[test]
fn the_energized_smelter_follows_the_same_path() {
    run(
        "smelter_core",
        Kind::Smelter,
        Template::new(3, &[false, false, true], false),
        80,
    );
}

#[test]
fn the_metallurgic_infuser_follows_the_same_path() {
    run(
        "infuser_core",
        Kind::Infuser,
        Template::new(3, &[false, false, true], false),
        80,
    );
}

#[test]
fn the_five_blocks_get_neighbor_changed_and_on_neighbor_change_from_block_tile() {
    let lines = lines("block_classes");
    assert_eq!(lines.len(), 5);
    for line in lines {
        assert_eq!(
            line["neighbor_changed"],
            "mekanism.common.block.prefab.BlockTile"
        );
        assert_eq!(
            line["on_neighbor_change"],
            "mekanism.common.block.prefab.BlockTile"
        );
    }
}
