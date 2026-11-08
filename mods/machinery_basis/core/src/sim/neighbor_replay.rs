//! Repete, tick a tick, um rastro de `NeighborTraceTest` (`dev/referencia/testes/NeighborTraceTest.java`): o campo `redstone` de máquinas
//! do Mekanism quando o sinal dos vizinhos muda de jeitos que avisam o bloco e de jeitos que não avisam.
//!
//! O rastro tem uma linha por tick (`ServerTickEvent.Post`) e, em cada uma, as operações do tick: `pre` (`ServerTickEvent.Pre`, antes do tick
//! da máquina) e `step` (o passo do gameTest, entre os block entities e o `Post`), cada uma com o `redstone` de todas as máquinas logo depois
//! dela. Aqui cada operação vira uma chamada de `SimLevel` (`set_block_flags`, `update_neighbour_for_output_signal`, `place_by_player`,
//! `detach_block_entity`) e o `redstone` lido depois dela, no meio do tick, tem de ser o do rastro.
//!
//! A geometria é a do Java: a linha `r` fica em `z = 4r + 2`, com a máquina em `x = 3`, a fonte de sinal a oeste, o "cutucão" a leste e o
//! vizinho com block entity ao norte.
use super::chunks::{BlockPos, ChunkHooks, ChunkPos, ENTITY_TICKING_LEVEL, SimLevel};
use crate::neighbors::flags;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub const ROW_STRIDE: i32 = 4;
const Y: i32 = 65;
pub const REDSTONE_BLOCK: &str = "minecraft:redstone_block";
pub const LEVER_OFF: &str = "minecraft:lever";
pub const LEVER_ON: &str = "minecraft:lever[powered=true]";
pub const STONE: &str = "minecraft:stone";
pub const CHEST: &str = "minecraft:chest";
/// A máquina que o rastro põe ao norte (`other 1 chamber`): sempre a Enrichment Chamber.
pub const OTHER_MACHINE: &str = "mekanism:enrichment_chamber";

pub fn subject(row: usize) -> BlockPos {
    BlockPos::new(3, Y, ROW_STRIDE * row as i32 + 2)
}

pub fn source(row: usize) -> BlockPos {
    BlockPos::new(2, Y, ROW_STRIDE * row as i32 + 2)
}

pub fn poke(row: usize) -> BlockPos {
    BlockPos::new(4, Y, ROW_STRIDE * row as i32 + 2)
}

pub fn other(row: usize) -> BlockPos {
    BlockPos::new(3, Y, ROW_STRIDE * row as i32 + 1)
}

/// O que o `template` do Java põe antes do primeiro tick: as máquinas, as fontes que já estão ligadas e os baús, sem avisar ninguém.
#[derive(Clone, Debug)]
pub struct Template {
    pub rows: usize,
    pub powered: Vec<bool>,
    pub chest: bool,
}

impl Template {
    pub fn new(rows: usize, powered: &[bool], chest: bool) -> Self {
        let mut powered = powered.to_vec();
        powered.resize(rows, false);
        Self {
            rows,
            powered,
            chest,
        }
    }
}

/// O mod por trás do mundo: o tipo de máquina do cenário e o que se lê dela.
pub trait Scene: ChunkHooks {
    /// O bloco da máquina do cenário.
    fn subject_block(&self) -> &'static str;
    /// `isPowered()`.
    fn powered(&self, entity: &Self::BlockEntity) -> bool;
    /// A linha do rastro: as chaves que o port reproduz, com os nomes e os valores do Java.
    fn row(&self, entity: &Self::BlockEntity) -> Value;
    /// `setControlType`: `mode` em minúsculas (`high`, `low`, `disabled`).
    fn set_mode(&self, entity: &Self::BlockEntity, mode: &str);
    /// Entrada e energia (ou lava), como `fill` do Java.
    fn fill(&self, entity: &Self::BlockEntity);
    /// Põe (`true`) ou tira a entrada da máquina do norte.
    fn set_other_input(&self, entity: &Self::BlockEntity, filled: bool);
    /// Os comparadores a avisar depois do tick do nível (`level_tick_post` no port): quem mudou de nível.
    fn take_comparator_queue(&mut self) -> Vec<BlockPos> {
        Vec::new()
    }
    /// Depois de cada tick, o que o rastro grava e o port não calcula (a energia que o gerador produz depende do ambiente, que o rastro não
    /// grava) volta ao tile: o cenário semeia o tile com a linha do rastro.
    fn seed(&self, _entity: &Self::BlockEntity, _row: &Value) {}
}

/// Onde o rastro e o port divergem.
pub type Mismatches = Vec<String>;

pub fn setup<S: Scene>(level: &mut SimLevel<S>, template: &Template) {
    let chunk = ChunkPos::new(0, 0);
    level.set_ticket_level(chunk, ENTITY_TICKING_LEVEL);
    level.tick();
    let block = level.hooks.subject_block();
    for row in 0..template.rows {
        level.set_block_flags(subject(row), Some(block), flags::UPDATE_CLIENTS);
        if template.powered[row] {
            level.set_block_flags(source(row), Some(REDSTONE_BLOCK), flags::UPDATE_CLIENTS);
        }
        if template.chest {
            level.set_block_flags(other(row), Some(CHEST), flags::UPDATE_CLIENTS);
        }
    }
    level.take_neighbor_calls();
}

struct Replay<'a, S: Scene> {
    level: &'a mut SimLevel<S>,
    rows: usize,
    pokes: HashMap<usize, bool>,
    saved: HashMap<usize, S::Saved>,
    detached: HashSet<usize>,
    mismatches: Mismatches,
}

impl<S: Scene> Replay<'_, S> {
    fn with_subject<R>(&self, row: usize, f: impl FnOnce(&S, &S::BlockEntity) -> R) -> Option<R> {
        let entity = self.level.view().block_entity(subject(row))?;
        Some(f(&self.level.hooks, entity))
    }

    fn powered_all(&self) -> Vec<Option<bool>> {
        (0..self.rows)
            .map(|row| self.with_subject(row, |scene, entity| scene.powered(entity)))
            .collect()
    }

    fn apply(&mut self, t: u64, op: &Value) {
        let text = op["op"].as_str().unwrap();
        let words: Vec<&str> = text.split(' ').collect();
        let row: usize = words.get(1).map_or(0, |word| word.parse().unwrap_or(0));
        let int = |index: usize| -> i32 { words[index].parse().unwrap() };
        match words[0] {
            "mode" => {
                self.with_subject(row, |scene, entity| {
                    scene.set_mode(entity, &words[2].to_lowercase());
                });
            }
            "fill" => {
                self.with_subject(row, |scene, entity| scene.fill(entity));
            }
            "src" => {
                let block = (words[2] == "on").then_some(REDSTONE_BLOCK);
                self.level.set_block_flags(source(row), block, int(3));
            }
            "lever" => self.lever(row, words[2], int(3)),
            "poke" => {
                let placed = self.pokes.entry(row).or_insert(false);
                *placed = !*placed;
                let block = (*placed).then_some(STONE);
                self.level.set_block_flags(poke(row), block, int(2));
            }
            "chest" => self.level.update_neighbour_for_output_signal(other(row)),
            "other" => {
                self.level.set_block(other(row), Some(OTHER_MACHINE));
            }
            "ninput" | "nclear" => {
                let filled = words[0] == "ninput";
                if let Some(entity) = self.level.view().block_entity(other(row)) {
                    self.level.hooks.set_other_input(entity, filled);
                }
            }
            "clear" => {
                self.level.set_block(subject(row), None);
            }
            "setblock" => {
                let block = self.level.hooks.subject_block();
                self.level
                    .set_block_flags(subject(row), Some(block), int(2));
            }
            "place" => {
                let block = self.level.hooks.subject_block();
                self.level.place_by_player(subject(row), block);
            }
            "unload" => self.unload(row),
            "load" => self.load(row),
            "reload" => {
                self.unload(row);
                self.load(row);
            }
            other => panic!("t={t}: operação desconhecida {other:?}"),
        }
    }

    fn lever(&mut self, row: usize, action: &str, update_flags: i32) {
        let at = source(row);
        match action {
            "place" => {
                self.level
                    .set_block_flags(at, Some(LEVER_OFF), update_flags);
            }
            "pull" => {
                let now = self.level.view().block_if_loaded(at).map(str::to_owned);
                let next = if now.as_deref() == Some(LEVER_ON) {
                    LEVER_OFF
                } else {
                    LEVER_ON
                };
                // `LeverBlock.pull`: `setBlock` com 3 e `updateNeighbours`, que avisa o bloco de onde a alavanca pende (o chão).
                self.level
                    .set_block_flags(at, Some(next), flags::UPDATE_ALL);
                self.level.update_neighbors_at(at);
                self.level
                    .update_neighbors_at(BlockPos::new(at.x, at.y - 1, at.z));
            }
            "remove" => {
                self.level.set_block_flags(at, None, update_flags);
            }
            other => panic!("alavanca: {other}"),
        }
    }

    fn unload(&mut self, row: usize) {
        if let Some(saved) = self.level.detach_block_entity(subject(row)) {
            self.saved.insert(row, saved);
            self.detached.insert(row);
        }
    }

    fn load(&mut self, row: usize) {
        if let Some(saved) = self.saved.remove(&row) {
            self.level.attach_block_entity(subject(row), &saved);
        }
        self.detached.remove(&row);
    }

    /// A leitura de `powered` no meio do tick, logo depois da operação, contra a do rastro. Uma linha descarregada fica de fora: o `getBlockEntity`
    /// do jogo cria um tile novo no lugar do que saiu, e o rastro lê esse.
    fn check_ops(&mut self, t: u64, op: &Value) {
        let expected = op["powered"].as_array().unwrap();
        let got = self.powered_all();
        for row in 0..self.rows {
            if self.detached.contains(&row) {
                continue;
            }
            let want = expected[row].as_bool();
            if got[row] != want {
                self.mismatches.push(format!(
                    "t={t} depois de `{}`: linha {row} powered {:?}, o rastro diz {:?}",
                    op["op"].as_str().unwrap(),
                    got[row],
                    want
                ));
            }
        }
    }

    fn check_rows(&mut self, t: u64, line: &Value) {
        let rows = line["rows"].as_array().unwrap();
        for (row, want) in rows.iter().enumerate().take(self.rows) {
            if self.detached.contains(&row) {
                continue;
            }
            let got = self.with_subject(row, |scene, entity| scene.row(entity));
            match (got, want) {
                (None, Value::Null) => {}
                (Some(got), want) if !want.is_null() => {
                    for (key, value) in got.as_object().unwrap() {
                        if want.get(key) != Some(value) {
                            self.mismatches.push(format!(
                                "t={t} linha {row} `{key}`: {value}, o rastro diz {}",
                                want.get(key).map_or("nada".to_owned(), Value::to_string)
                            ));
                        }
                    }
                }
                (got, want) => self.mismatches.push(format!(
                    "t={t} linha {row}: tile {got:?}, o rastro diz {want}"
                )),
            }
        }
    }
}

/// Repete o rastro `lines` (a primeira linha é a das ordens do jogo) e devolve o que diverge.
pub fn replay<S: Scene>(
    level: &mut SimLevel<S>,
    template: &Template,
    lines: &[Value],
) -> Mismatches {
    setup(level, template);
    let mut run = Replay {
        level,
        rows: template.rows,
        pokes: HashMap::new(),
        saved: HashMap::new(),
        detached: HashSet::new(),
        mismatches: Vec::new(),
    };
    for line in &lines[1..] {
        let t = line["t"].as_u64().unwrap();
        let ops = line["ops"].as_array().unwrap();
        // `ServerTickEvent.Pre`: as operações `pre` rodam antes do tick da máquina.
        for op in ops.iter().filter(|op| op["phase"] == "pre") {
            run.apply(t, op);
            run.check_ops(t, op);
        }
        run.level.tick_level();
        // `level_tick_post`: os comparadores que mudaram avisam os vizinhos depois de todos os block entities.
        for pos in run.level.hooks.take_comparator_queue() {
            run.level.update_neighbour_for_output_signal(pos);
        }
        for op in ops.iter().filter(|op| op["phase"] == "step") {
            run.apply(t, op);
            run.check_ops(t, op);
        }
        run.level.finish_tick();
        run.check_rows(t, line);
        run.seed_from(line);
    }
    run.mismatches
}

impl<S: Scene> Replay<'_, S> {
    fn seed_from(&self, line: &Value) {
        let rows = line["rows"].as_array().unwrap();
        for (row, want) in rows.iter().enumerate().take(self.rows) {
            if !want.is_null() {
                self.with_subject(row, |scene, entity| scene.seed(entity, want));
            }
        }
    }
}
