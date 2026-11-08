//! Repete, tick a tick, os rastros `neighbors_heat_*.jsonl` do Heat Generator original (`dev/referencia/testes/NeighborTraceTest.java`): o
//! `redstone` só muda quando o jogo avisa o bloco (`BlockTile.neighborChanged`, `onNeighborChange`, `onAdded`), e o `canFunction()` de
//! `TileEntityHeatGenerator.java:140` segue o que ficou, velho ou não.
//!
//! A energia que o gerador produz depende do ambiente, que o rastro não grava: depois de cada tick a energia do rastro volta ao tile
//! (`Scene::seed`), e o que se compara é o `redstone`, o `canFunction`, a atividade, o bloco aceso e a lava.
use machinery_basis_core::neighbors::{OUTPUT_SIGNAL_ORDER, UPDATE_ORDER};
use machinery_basis_core::sim::chunks::{BlockPos, ChunkHooks, LevelView, SimLevel};
use machinery_basis_core::sim::neighbor_replay::{
    LEVER_ON, REDSTONE_BLOCK, Scene, Template, replay,
};
use machinery_basis_core::tile::{RedstoneControl, SavedState, TileBase};
use mekanismgenerators_rules::{
    BLOCK_DEACTIVATION_DELAY, HEAT_GENERATOR, State, Surroundings, step,
};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::path::Path;

/// O que o port guarda de um Heat Generator no mundo: o estado do tile e o que o NBT leva.
type Entity = RefCell<State>;

const AMBIENT: f64 = 300.0;
/// A lava que `fill` põe no tanque (`NeighborTraceTest.fill`).
const LAVA: u64 = 10_000;

struct Heat;

impl ChunkHooks for Heat {
    type BlockEntity = Entity;
    type Saved = State;

    fn new_block_entity(
        &mut self,
        _: BlockPos,
        block: &str,
        saved: Option<&State>,
    ) -> Option<Entity> {
        if block != HEAT_GENERATOR {
            return None;
        }
        let mut state = State::new(AMBIENT);
        if let Some(saved) = saved {
            // `load_tile` do glue: o tile volta do NBT com `redstone`, o modo, `currentActive` e `updateDelay`.
            let mut tile = TileBase::new(BLOCK_DEACTIVATION_DELAY);
            tile.restore(
                &SavedState {
                    redstone: saved.tile.is_powered(),
                    control_type: saved.tile.control_type(),
                    active: saved.tile.active(),
                    update_delay: saved.tile.update_delay(),
                    ..SavedState::default()
                },
                saved.tile.block_active(),
            );
            state = State {
                tile,
                ..saved.clone()
            };
        }
        Some(RefCell::new(state))
    }

    fn save_block_entity(&mut self, _: BlockPos, entity: &mut Entity) -> State {
        entity.borrow().clone()
    }

    fn block_entity_ticks(&self, _: &str) -> bool {
        true
    }

    fn block_entity_tick(&mut self, _: &LevelView<Entity>, _: BlockPos, entity: &mut Entity) {
        let around = Surroundings {
            lava_faces: 0,
            in_nether: false,
            ambient: AMBIENT,
            face_ambient: [AMBIENT; 6],
        };
        step(&mut entity.borrow_mut(), &around);
    }

    fn block_signal(&self, block: &str) -> i32 {
        if block == REDSTONE_BLOCK || block == LEVER_ON {
            15
        } else {
            0
        }
    }

    /// `Generator` põe `Attributes.COMPARATOR` em todo gerador.
    fn has_analog_output(&self, block: &str) -> bool {
        block == HEAT_GENERATOR
    }

    fn neighbor_changed(
        &mut self,
        _: &LevelView<Entity>,
        _: BlockPos,
        entity: &mut Entity,
        powered: bool,
    ) {
        entity.borrow_mut().tile.update_power(powered);
    }

    fn on_neighbor_change(
        &mut self,
        _: &LevelView<Entity>,
        _: BlockPos,
        entity: &mut Entity,
        powered: bool,
        _: BlockPos,
    ) {
        entity.borrow_mut().tile.update_power(powered);
    }

    fn placed_by(
        &mut self,
        _: &LevelView<Entity>,
        _: BlockPos,
        entity: &mut Entity,
        powered: bool,
    ) {
        entity.borrow_mut().tile.update_power(powered);
    }
}

impl Scene for Heat {
    fn subject_block(&self) -> &'static str {
        HEAT_GENERATOR
    }

    fn powered(&self, entity: &Entity) -> bool {
        entity.borrow().tile.is_powered()
    }

    fn row(&self, entity: &Entity) -> Value {
        let state = entity.borrow();
        json!({
            "powered": state.tile.is_powered(),
            "can_function": state.tile.is_redstone_activated(),
            "mode": state.tile.control_type().serialized_name(),
            "active": state.tile.active(),
            "block_active": state.tile.block_active(),
            "lava": state.lava,
        })
    }

    fn set_mode(&self, entity: &Entity, mode: &str) {
        entity
            .borrow_mut()
            .tile
            .set_control_type(RedstoneControl::from_serialized_name(mode));
    }

    fn fill(&self, entity: &Entity) {
        entity.borrow_mut().lava = LAVA;
    }

    fn set_other_input(&self, _: &Entity, _: bool) {}

    fn seed(&self, entity: &Entity, row: &Value) {
        entity.borrow_mut().energy = row["energy"].as_i64().unwrap() as i32;
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

fn run(scenario: &str, template: Template) {
    let lines = lines(scenario);
    assert!(lines.len() > 100, "{scenario}: só {} linhas", lines.len());
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
    let mut level = SimLevel::new(Heat);
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
fn the_heat_generator_follows_the_notifications_of_a_redstone_block_and_a_lever() {
    run("heat_notified", Template::new(3, &[], false));
}

#[test]
fn the_heat_generator_keeps_the_old_redstone_when_the_signal_changes_without_notice() {
    run("heat_unnotified", Template::new(3, &[], false));
}

#[test]
fn the_heat_generator_placed_next_to_a_powered_block_reads_it_only_when_placed_by_a_player() {
    run("heat_placed", Template::new(3, &[true, true, true], false));
}

#[test]
fn the_heat_generator_comes_back_from_the_save_with_the_old_redstone() {
    run("heat_reload", Template::new(3, &[], false));
}

#[test]
fn a_neighbour_block_entity_that_changes_makes_the_heat_generator_read_the_signal_again() {
    run("heat_output_signal", Template::new(2, &[], true));
}
