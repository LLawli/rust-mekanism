//! Repete, tick a tick, o rastro `composter_enrichment_chamber.jsonl` (`IdentityTraceTest.chamberComposter`): a Enrichment Chamber
//! ejetando a saída para uma composteira que o `ComposterWrapper` do NeoForge expõe. A saída fica só no lado do cenário, com o
//! ejetor ligado, e a composteira naquele lado no nível do cenário.
//!
//! O que o rastro prova: pelo lado de cima da composteira (a máquina em cima, ejetando para baixo) o item compostável entra, um
//! por disparo do ejetor, e o nível sobe de um em um; no nível 7 e no 8 nada entra; o item sem o componente fica; com a composteira
//! em cima da máquina (a face de baixo dela, que só entrega farelo) e ao lado (face horizontal, sem capacidade) nada entra. O bloco
//! agenda 20 ticks quando o nível chega a 7, e aí vira 8, o que o modelo do bloco abaixo faz.
mod common;

use machinery_basis_core::composter::{ComposterBlock, ComposterWrapper, MAX_LEVEL, READY};
use machinery_basis_core::containers::ItemResource;
use machinery_basis_core::item_handler::{ItemCapabilities, ItemHandler};
use machinery_basis_core::sides::Face;
use machinery_basis_core::tile::ProgressMachine;
use serde_json::Value;
use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

/// Quanto cada fase do cenário em Java dura até a próxima começar (`thenExecuteAfter`), na ordem das fases do rastro. O rastro
/// grava no máximo 60 ticks de cada, e os outros também contam para o ritmo do ejetor.
const DURATIONS: [u32; 8] = [60, 70, 40, 40, 40, 40, 40, 40];

/// Uma composteira de mentira com o que o bloco do Java faz: o item `cake` sempre dá uma camada, a folha só no nível 0 (a outra
/// parte é sorteio e o rastro não a usa), e o resto não é compostável. O tick agendado vira 7 em 8.
struct Block {
    level: Cell<i32>,
    time: Rc<Cell<u64>>,
    scheduled: Cell<Option<u64>>,
}

impl Block {
    /// `ComposterBlock.onPlace` e o `setBlock` do cenário: um bloco novo no nível dado.
    fn place(&self, level: i32) {
        self.level.set(level);
        self.scheduled.set(None);
        if level == MAX_LEVEL {
            self.scheduled.set(Some(self.time.get() + 20));
        }
    }

    /// O tick agendado, que roda no começo do tick do nível.
    fn tick(&self) {
        if self.scheduled.get().is_some_and(|at| at <= self.time.get()) {
            self.scheduled.set(None);
            if self.level.get() == MAX_LEVEL {
                self.level.set(READY);
            }
        }
    }
}

impl ComposterBlock for Block {
    fn exists(&self) -> bool {
        true
    }

    fn level(&self) -> i32 {
        self.level.get()
    }

    fn set_level(&self, level: i32, _notify: bool) {
        self.level.set(level);
    }

    fn layers(&self, resource: &ItemResource) -> Option<i32> {
        match &*resource.id {
            "cake" => Some(1),
            "oak_leaves" => Some(i32::from(self.level.get() == 0)),
            _ => None,
        }
    }

    fn schedule_tick(&self, ticks: u32) {
        self.scheduled.set(Some(self.time.get() + u64::from(ticks)));
    }

    fn level_event(&self, _rose: bool) {}
}

/// A vizinhança da máquina: a composteira só no lado da fase, vista pelo lado oposto.
struct Around {
    side: Face,
    wrapper: Rc<ComposterWrapper>,
}

impl ItemCapabilities for Around {
    fn item_handler(&self, face: Face) -> Option<Rc<dyn ItemHandler>> {
        if face != self.side {
            return None;
        }
        self.wrapper.handler(face.opposite())
    }
}

fn read_trace() -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/composter_enrichment_chamber.jsonl");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// Os lados do cenário em Java, pelo nome da fase.
fn side_of(phase: &str) -> Face {
    match phase.split('_').next().unwrap() {
        "below" => Face::Down,
        "above" => Face::Up,
        "east" => Face::East,
        other => panic!("fase {other}"),
    }
}

/// O que uma linha do rastro diz da saída da máquina: `[id, nome, dados, quantidade]`.
fn output_of(machine: &ProgressMachine) -> Value {
    let slot = machine.output();
    let id = slot
        .resource()
        .map_or(String::new(), |resource| resource.id.to_string());
    serde_json::json!([id, "", "", slot.amount()])
}

fn phases(lines: &[Value]) -> Vec<Vec<&Value>> {
    let mut phases: Vec<Vec<&Value>> = Vec::new();
    for line in lines.iter().filter(|line| line.get("phase").is_some()) {
        if line["t"] == 0 {
            phases.push(Vec::new());
        }
        phases.last_mut().unwrap().push(line);
    }
    phases
}

fn run(lines: &[Value]) -> usize {
    let time = Rc::new(Cell::new(0));
    let block = Rc::new(Block {
        level: Cell::new(0),
        time: time.clone(),
        scheduled: Cell::new(None),
    });
    let wrapper = ComposterWrapper::new(block.clone(), common::item("bone_meal"));
    let mut machine = common::machine("enrichment_chamber");
    let mut compared = 0;
    let phases = phases(lines);
    assert_eq!(phases.len(), DURATIONS.len(), "fases do rastro");
    for (phase, duration) in phases.iter().zip(DURATIONS) {
        let name = phase[0]["phase"].as_str().unwrap();
        let side = side_of(name);
        let around = Around {
            side,
            wrapper: wrapper.clone(),
        };
        // `begin`: a composteira nova no lado, a saída só nele, o ejetor ligado e o item na saída.
        let level = phase[0]["composter"].as_i64().unwrap() as i32;
        block.place(level);
        common::eject_to(&mut machine, side);
        let output = phase[0]["output"].as_array().unwrap();
        machine.output().set_contents(
            Some(common::item(output[0].as_str().unwrap())),
            output[3].as_u64().unwrap() as u32,
            None,
        );
        for tick in 0..=duration {
            if tick > 0 {
                // O tick do nível: os ticks agendados, depois os block entities.
                time.set(time.get() + 1);
                block.tick();
                machine.tick_server(time.get(), &around);
            }
            let Some(expected) = phase.get(tick as usize) else {
                continue;
            };
            assert_eq!(expected["t"], tick, "{name}");
            assert_eq!(
                expected["composter"],
                block.level.get(),
                "{name} t={tick}: nível da composteira"
            );
            assert_eq!(
                expected["output"],
                output_of(&machine),
                "{name} t={tick}: saída da máquina"
            );
            compared += 1;
        }
    }
    compared
}

#[test]
fn the_chamber_ejects_into_a_composter_like_the_original() {
    let lines = read_trace();
    let compared = run(&lines);
    // Menos linhas do que isto é um rastro cortado.
    assert!(compared >= 300, "só {compared} linhas comparadas");
}
