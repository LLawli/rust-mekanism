//! Repete, tick a tick, os rastros `comparator_order.jsonl` e `comparator_first_tick.jsonl` do Mekanism original
//! (`dev/referencia/testes/ComparatorTraceTest.java`): o aviso do comparador de uma máquina sai dentro do tick dela
//! (`TileEntityMekanism.tickServer`, `:645-652`), então uma máquina vizinha que tica depois, no mesmo tick, já vê o `redstone` que ele corrige, e a que
//! tica antes só o vê no tick seguinte. Cada linha leva o estado da máquina S (a que tem o `redstone` velho), o nível da vizinha N e a saída dos dois
//! comparadores (`ComparatorBlockEntity.getOutputSignal`), um ao lado de cada uma.
//!
//! O comparador é o do jogo: `neighborChanged` -> `checkTickOnNeighbor` agenda um tick de 2 `gameTime` se a saída mudou, e o tick (no começo do tick do
//! nível, antes dos block entities) guarda a saída nova (`ComparatorBlock.refreshOutputState`).
mod common;

use machinery_basis_core::containers::ItemResource;
use machinery_basis_core::neighbors::flags;
use machinery_basis_core::sim::SimNeighbours;
use machinery_basis_core::sim::chunks::{
    BlockPos, ChunkHooks, ChunkPos, ENTITY_TICKING_LEVEL, LevelView, SimLevel,
};
use machinery_basis_core::tile::{ProgressMachine, RedstoneControl};
use serde_json::{Value, json};
use std::cell::Cell;
use std::path::Path;

const COMPARATOR: &str = "minecraft:comparator";
const REDSTONE_BLOCK: &str = "minecraft:redstone_block";
const STRIDE: i32 = 6;
const Y: i32 = 65;
/// `ComparatorBlock.getDelay`.
const COMPARATOR_DELAY: u64 = 2;

fn north(row: usize) -> BlockPos {
    BlockPos::new(3, Y, STRIDE * row as i32 + 2)
}

fn south(row: usize) -> BlockPos {
    BlockPos::new(3, Y, STRIDE * row as i32 + 3)
}

fn source(row: usize) -> BlockPos {
    BlockPos::new(2, Y, STRIDE * row as i32 + 3)
}

fn comparator_north(row: usize) -> BlockPos {
    BlockPos::new(4, Y, STRIDE * row as i32 + 2)
}

fn comparator_south(row: usize) -> BlockPos {
    BlockPos::new(4, Y, STRIDE * row as i32 + 3)
}

enum Entity {
    Machine(Box<ProgressMachine>),
    /// `ComparatorBlockEntity`: a saída, e o estado `powered` do bloco.
    Comparator {
        output: Cell<i32>,
        powered: Cell<bool>,
    },
}

impl Entity {
    fn machine(&self) -> Option<&ProgressMachine> {
        match self {
            Entity::Machine(machine) => Some(machine),
            Entity::Comparator { .. } => None,
        }
    }
}

struct World {
    /// O bloco e o nome da máquina do cenário.
    block: String,
    name: String,
    /// O item que a receita da máquina aceita na entrada: só o slot de entrada conta a capacidade de um item válido.
    input: String,
    /// Quem mudou de nível de comparador no tick que acabou de rodar.
    changes: Vec<BlockPos>,
    /// Os ticks agendados dos comparadores: `gameTime` e posição.
    scheduled: Vec<(u64, BlockPos)>,
    /// Quando os avisos do comparador só saem no fim do tick do nível (a mutação de teste): ficam aqui até `finish`.
    deferred: bool,
    game_time: u64,
}

impl World {
    fn new(name: &str, input: &str) -> Self {
        Self {
            block: format!("mekanism:{name}"),
            name: name.to_owned(),
            input: input.to_owned(),
            changes: Vec::new(),
            scheduled: Vec::new(),
            deferred: false,
            game_time: 0,
        }
    }

    /// `ComparatorBlock.calculateOutputSignal` com a entrada a oeste: o `getAnalogOutputSignal` de uma máquina é o `currentRedstoneLevel` do tile.
    fn input(view: &LevelView<Entity>, comparator: BlockPos) -> i32 {
        let behind = BlockPos::new(comparator.x - 1, comparator.y, comparator.z);
        view.block_entity(behind)
            .and_then(Entity::machine)
            .map_or(0, |machine| machine.tile().current_redstone_level())
    }

    /// `ComparatorBlock.checkTickOnNeighbor`.
    fn check(&mut self, view: &LevelView<Entity>, pos: BlockPos, entity: &Entity) {
        let Entity::Comparator { output, powered } = entity else {
            return;
        };
        if self.scheduled.iter().any(|(_, at)| *at == pos) {
            return;
        }
        let wanted = Self::input(view, pos);
        if wanted != output.get() || powered.get() != (wanted > 0) {
            self.scheduled
                .push((self.game_time + COMPARATOR_DELAY, pos));
        }
    }
}

impl ChunkHooks for World {
    type BlockEntity = Entity;
    type Saved = ();

    fn new_block_entity(&mut self, _: BlockPos, block: &str, _: Option<&()>) -> Option<Entity> {
        match block {
            block if block == self.block => Some(Entity::Machine(Box::new(
                if self.name == "metallurgic_infuser" {
                    common::infuser_from_content()
                } else {
                    common::chamber(&self.input)
                },
            ))),
            COMPARATOR => Some(Entity::Comparator {
                output: Cell::new(0),
                powered: Cell::new(false),
            }),
            _ => None,
        }
    }

    fn save_block_entity(&mut self, _: BlockPos, _: &mut Entity) {}

    fn block_entity_ticks(&self, block: &str) -> bool {
        block == self.block
    }

    fn block_entity_tick(&mut self, _: &LevelView<Entity>, pos: BlockPos, entity: &mut Entity) {
        if let Entity::Machine(machine) = entity
            && machine.tick_server(self.game_time, &SimNeighbours::default())
        {
            self.changes.push(pos);
        }
    }

    fn take_comparator_changes(&mut self) -> Vec<BlockPos> {
        if self.deferred {
            Vec::new()
        } else {
            std::mem::take(&mut self.changes)
        }
    }

    fn block_signal(&self, block: &str) -> i32 {
        if block == REDSTONE_BLOCK { 15 } else { 0 }
    }

    fn has_analog_output(&self, block: &str) -> bool {
        block == self.block
    }

    fn neighbor_changed(
        &mut self,
        view: &LevelView<Entity>,
        pos: BlockPos,
        entity: &mut Entity,
        powered: bool,
    ) {
        match entity {
            Entity::Machine(machine) => machine.tile_mut().update_power(powered),
            Entity::Comparator { .. } => self.check(view, pos, entity),
        }
    }

    fn on_neighbor_change(
        &mut self,
        view: &LevelView<Entity>,
        pos: BlockPos,
        entity: &mut Entity,
        powered: bool,
        _: BlockPos,
    ) {
        match entity {
            Entity::Machine(machine) => machine.tile_mut().update_power(powered),
            Entity::Comparator { .. } => self.check(view, pos, entity),
        }
    }
}

struct Run {
    level: SimLevel<World>,
    mismatches: Vec<String>,
}

impl Run {
    fn new(name: &str, input: &str, rows: usize) -> Self {
        let mut level = SimLevel::new(World::new(name, input));
        level.set_ticket_level(ChunkPos::new(0, 0), ENTITY_TICKING_LEVEL);
        level.tick();
        for row in 0..rows {
            for pos in [comparator_north(row), comparator_south(row)] {
                level.set_block_flags(pos, Some(COMPARATOR), flags::UPDATE_CLIENTS);
            }
        }
        level.take_neighbor_calls();
        Self {
            level,
            mismatches: Vec::new(),
        }
    }

    fn machine(&self, pos: BlockPos) -> Option<&ProgressMachine> {
        self.level
            .view()
            .block_entity(pos)
            .and_then(Entity::machine)
    }

    fn apply(&mut self, op: &str) {
        let words: Vec<&str> = op.split(' ').collect();
        let row: usize = words[1].parse().unwrap();
        let int = |index: usize| -> i32 { words[index].parse().unwrap() };
        let who = |index: usize| {
            if words[index] == "N" {
                north(row)
            } else {
                south(row)
            }
        };
        match words[0] {
            "put" => {
                let block = self.level.hooks.block.clone();
                self.level.set_block_flags(who(2), Some(&block), int(3));
            }
            "clear" => {
                self.level.set_block_flags(who(2), None, int(3));
            }
            "mode" => {
                if let Some(machine) = self.machine(south(row)) {
                    machine
                        .tile_mut()
                        .set_control_type(RedstoneControl::from_serialized_name(
                            &words[2].to_lowercase(),
                        ));
                }
            }
            "fill" => {
                if let Some(machine) = self.machine(south(row)) {
                    machine
                        .input()
                        .set_contents(Some(common::item("clay")), 64, None);
                    machine.energy().set_energy(20_000, None);
                }
            }
            "src" => {
                let block = (words[2] == "on").then_some(REDSTONE_BLOCK);
                self.level.set_block_flags(source(row), block, int(3));
            }
            "ninput" => {
                if let Some(machine) = self.machine(north(row)) {
                    machine
                        .input()
                        .set_contents(Some(common::item("clay")), 64, None);
                }
            }
            "nclear" => {
                if let Some(machine) = self.machine(north(row)) {
                    machine.input().set_contents(None, 0, None);
                }
            }
            other => panic!("operação desconhecida {other:?}"),
        }
    }

    /// `ServerLevel.tickBlocks`: os ticks agendados que venceram, no começo do tick do nível. `ComparatorBlock.tick` -> `refreshOutputState`.
    fn run_scheduled(&mut self) {
        let t = self.level.hooks.game_time;
        let due: Vec<BlockPos> = self
            .level
            .hooks
            .scheduled
            .iter()
            .filter(|(at, _)| *at <= t)
            .map(|(_, pos)| *pos)
            .collect();
        self.level.hooks.scheduled.retain(|(at, _)| *at > t);
        for pos in due {
            let wanted = World::input(self.level.view(), pos);
            if let Some(Entity::Comparator { output, powered }) =
                self.level.view().block_entity(pos)
            {
                output.set(wanted);
                powered.set(wanted > 0);
            }
        }
    }

    fn rows(&self, rows: usize) -> Value {
        let mut out = Vec::new();
        for row in 0..rows {
            let south_row = self.machine(south(row)).map(|machine| {
                let tile = machine.tile();
                json!({
                    "powered": tile.is_powered(),
                    "can_function": tile.is_redstone_activated(),
                    "active": tile.active(),
                    "comparator": tile.current_redstone_level(),
                    "progress": machine.recipe_progress(),
                    "energy": machine.energy().amount(),
                })
            });
            let north_row = self.machine(north(row)).map(|machine| {
                json!({
                    "comparator": machine.tile().current_redstone_level(),
                    "level": machine.redstone_level(),
                })
            });
            let output = |pos| match self.level.view().block_entity(pos) {
                Some(Entity::Comparator { output, .. }) => output.get(),
                _ => -1,
            };
            out.push(json!({
                "s": south_row,
                "n": north_row,
                "cn": output(comparator_north(row)),
                "cs": output(comparator_south(row)),
            }));
        }
        Value::Array(out)
    }
}

fn lines(name: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(format!("{name}.jsonl"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// Repete o rastro e devolve o que diverge. `deferred` avisa o comparador só depois dos block entities, como o port fazia.
fn replay(name: &str, rows: usize, deferred: bool) -> Vec<String> {
    let lines = lines(name);
    let mut run = Run::new("enrichment_chamber", "clay", rows);
    run.level.hooks.deferred = deferred;
    for line in &lines {
        let t = line["t"].as_u64().unwrap();
        run.level.hooks.game_time = run.level.view().game_time();
        let ops = line["ops"].as_array().unwrap();
        for op in ops.iter().filter(|op| op["phase"] == "pre") {
            run.apply(op["op"].as_str().unwrap());
        }
        run.run_scheduled();
        run.level.tick_level();
        if deferred {
            // O port antigo: o aviso saía em `level_tick_post`, depois de todos os block entities.
            let changed = std::mem::take(&mut run.level.hooks.changes);
            for pos in changed {
                run.level.update_neighbour_for_output_signal(pos);
            }
        }
        for op in ops.iter().filter(|op| op["phase"] == "step") {
            run.apply(op["op"].as_str().unwrap());
        }
        run.level.finish_tick();
        let got = run.rows(rows);
        let want = &line["rows"];
        if &got != want {
            for row in 0..rows {
                if got[row] != want[row] {
                    run.mismatches.push(format!(
                        "t={t} linha {row}: {}, o rastro diz {}",
                        got[row], want[row]
                    ));
                }
            }
        }
    }
    run.mismatches
}

fn check(name: &str, rows: usize, minimum: usize) {
    assert!(lines(name).len() > minimum, "{name}: rastro curto");
    let mismatches = replay(name, rows, false);
    assert!(
        mismatches.is_empty(),
        "{name}: {} diferenças\n{}",
        mismatches.len(),
        mismatches
            .iter()
            .take(12)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn a_machine_that_ticks_after_the_changed_one_sees_the_comparator_notice_in_the_same_tick() {
    check("comparator_order", 2, 90);
}

#[test]
fn a_new_machine_does_not_notify_its_neighbours_in_its_first_tick() {
    check("comparator_first_tick", 2, 50);
}

#[test]
fn notifying_after_the_block_entities_gives_a_different_trace() {
    // A mutação que o port tinha: o aviso sai depois de todos os block entities. O rastro tem de pegar.
    let mismatches = replay("comparator_order", 2, true);
    assert!(
        !mismatches.is_empty(),
        "o rastro não distingue o aviso no tick do aviso no fim do tick"
    );
}

// ---- o valor analógico das quatro máquinas, vazias, com itens e cheias ----

/// Os slots da máquina na ordem de `getInventorySlots()`: infusão (só o Infuser), entrada, saída e energia.
fn slots_of(machine: &ProgressMachine) -> Vec<&machinery_basis_core::containers::InventorySlot> {
    let mut slots = Vec::new();
    if let Some(side) = machine.chemical_side() {
        slots.push(&side.slot);
    }
    slots.push(machine.input());
    slots.push(machine.output());
    slots.push(machine.energy_slot());
    slots
}

/// O item (e a pilha dele) que o Java pôs em cada slot na marca `all_full`: o que `candidate` achou válido, que o rastro guarda.
fn full_items(marks: &[Value]) -> Vec<ItemResource> {
    let full = marks
        .iter()
        .find(|mark| mark["mark"] == "all_full")
        .expect("a marca all_full");
    full["slots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|slot| {
            ItemResource::new(
                slot["item"].as_str().unwrap(),
                u32::try_from(slot["capacity"].as_u64().unwrap()).unwrap(),
            )
        })
        .collect()
}

/// Uma operação de `comparator_levels_*`: `slot 0 N` e `slot 1 N` mexem na entrada e na saída, `all` enche todos os slots, `none` esvazia.
fn apply_level_op(run: &mut Run, op: &str, marks: &[Value], mismatches: &mut Vec<String>, t: u64) {
    let words: Vec<&str> = op.split(' ').collect();
    let Some(machine) = run.machine(north(0)) else {
        if words[0] == "put" {
            let block = run.level.hooks.block.clone();
            run.level.set_block_flags(north(0), Some(&block), 3);
        }
        return;
    };
    let items = full_items(marks);
    // A posição de cada slot na lista do Java: o Infuser tem o slot de infusão antes.
    let shift = usize::from(machine.chemical_side().is_some());
    match words[0] {
        "slot" => {
            let index: usize = words[1].parse().unwrap();
            let slot = if index == 0 {
                machine.input()
            } else {
                machine.output()
            };
            slot.set_contents(
                Some(items[index + shift].clone()),
                words[2].parse().unwrap(),
                None,
            );
        }
        "all" => {
            for (slot, item) in slots_of(machine).into_iter().zip(items) {
                slot.set_contents(Some(item), 64, None);
            }
        }
        "none" => {
            for slot in slots_of(machine) {
                slot.set_contents(None, 0, None);
            }
        }
        "mark" => {
            // O que o Java contou em cada slot (item, quantidade, capacidade) e o nível que saiu disso.
            let mark = marks
                .iter()
                .find(|mark| mark["mark"] == words[1])
                .expect("a marca");
            let got: Vec<Value> = slots_of(machine)
                .iter()
                .map(|slot| {
                    json!({
                        "item": slot.resource().map_or(String::new(), |resource| resource.id.to_string()),
                        "count": slot.amount(),
                        "capacity": slot.capacity(),
                    })
                })
                .collect();
            if Value::Array(got.clone()) != mark["slots"] {
                mismatches.push(format!(
                    "t={t} {}: slots {}, o rastro diz {}",
                    words[1],
                    Value::Array(got),
                    mark["slots"]
                ));
            }
            if i64::from(machine.redstone_level()) != mark["level"].as_i64().unwrap() {
                mismatches.push(format!(
                    "t={t} {}: nível {}, o rastro diz {}",
                    words[1],
                    machine.redstone_level(),
                    mark["level"]
                ));
            }
        }
        other => panic!("operação desconhecida {other:?}"),
    }
}

fn replay_levels(name: &str, deferred: bool) -> Vec<String> {
    let all = lines(&format!("comparator_levels_{name}"));
    let marks: Vec<Value> = all
        .iter()
        .filter(|line| line.get("mark").is_some())
        .cloned()
        .collect();
    let ticks: Vec<&Value> = all
        .iter()
        .filter(|line| line.get("ops").is_some())
        .collect();
    let input = full_items(&marks)[usize::from(name == "metallurgic_infuser")]
        .id
        .to_string();
    let mut run = Run::new(name, &input, 1);
    run.level.hooks.deferred = deferred;
    let mut mismatches = Vec::new();
    for line in ticks {
        let t = line["t"].as_u64().unwrap();
        run.level.hooks.game_time = run.level.view().game_time();
        run.run_scheduled();
        run.level.tick_level();
        if deferred {
            let changed = std::mem::take(&mut run.level.hooks.changes);
            for pos in changed {
                run.level.update_neighbour_for_output_signal(pos);
            }
        }
        for op in line["ops"].as_array().unwrap() {
            apply_level_op(
                &mut run,
                op["op"].as_str().unwrap(),
                &marks,
                &mut mismatches,
                t,
            );
        }
        run.level.finish_tick();
        let got = match run.machine(north(0)) {
            Some(machine) => json!({
                "level": machine.redstone_level(),
                "current": machine.tile().current_redstone_level(),
                "cn": match run.level.view().block_entity(comparator_north(0)) {
                    Some(Entity::Comparator { output, .. }) => output.get(),
                    _ => -1,
                },
            }),
            None => Value::Null,
        };
        if got != line["rows"][0] {
            mismatches.push(format!("t={t}: {got}, o rastro diz {}", line["rows"][0]));
        }
    }
    mismatches
}

fn check_levels(name: &str) {
    let mismatches = replay_levels(name, false);
    assert!(
        mismatches.is_empty(),
        "{name}: {} diferenças\n{}",
        mismatches.len(),
        mismatches
            .iter()
            .take(12)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn the_comparator_reads_the_enrichment_chamber_empty_partly_full_and_full() {
    check_levels("enrichment_chamber");
}

#[test]
fn the_comparator_reads_the_crusher_empty_partly_full_and_full() {
    check_levels("crusher");
}

#[test]
fn the_comparator_reads_the_energized_smelter_empty_partly_full_and_full() {
    check_levels("energized_smelter");
}

#[test]
fn the_comparator_reads_the_metallurgic_infuser_empty_partly_full_and_full() {
    check_levels("metallurgic_infuser");
}
