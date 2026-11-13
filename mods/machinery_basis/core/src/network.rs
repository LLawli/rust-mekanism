//! As faces de energia que um bloco declara (`data/<ns>/machinery_basis/energy/<bloco>.json`): por onde a energia entra e sai de uma máquina ou de um gerador que a lib não conhece.
//! A rede de transmissores (o cabo) é `transmission`: o cabo não se declara aqui.
use crate::sides::{Face, Sides};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

/// A pasta que declara quais blocos entram na rede de energia, dentro de `data/<ns>/`. O arquivo
/// tem o nome do bloco: `data/<ns>/machinery_basis/energy/<bloco>.json`.
pub const ENERGY_FOLDER: &str = "machinery_basis/energy";

// Os lados relativos à frente do bloco, na ordem de `relative`.
const SIDE_NAMES: [&str; 6] = ["front", "back", "left", "right", "top", "bottom"];
const ALL_SIDES: &str = "all";

fn relative(front: Face) -> [Face; 6] {
    [
        front,
        front.opposite(),
        front.left_of(),
        front.right_of(),
        Face::Up,
        Face::Down,
    ]
}

/// Uma lista de lados do arquivo: nomes, ou `"all"`. Ausente, nenhum.
fn parse_sides(json: Option<&Value>) -> [bool; 6] {
    match json {
        Some(Value::String(all)) if all == ALL_SIDES => [true; 6],
        Some(Value::Array(listed)) => {
            SIDE_NAMES.map(|name| listed.iter().any(|side| side.as_str() == Some(name)))
        }
        _ => [false; 6],
    }
}

/// Como um bloco participa da rede de energia. Um lado pode receber, entregar ou os dois, como
/// a capability de energia do Mekanism, que responde por lado.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EnergyBlock {
    /// Por onde a energia entra, na ordem de `SIDE_NAMES`.
    inputs: [bool; 6],
    /// Por onde a energia sai, na ordem de `SIDE_NAMES`.
    outputs: [bool; 6],
}

impl EnergyBlock {
    /// `{"inputs": "all"}`, `{"outputs": ["front"]}` e combinações.
    fn parse(json: &Value) -> Self {
        Self {
            inputs: parse_sides(json.get("inputs")),
            outputs: parse_sides(json.get("outputs")),
        }
    }

    /// Os lados em faces do mundo, para um bloco com a frente em `front`.
    fn sides(&self, front: Face) -> Sides {
        let mut sides = Sides::default();
        for (i, face) in relative(front).into_iter().enumerate() {
            sides.input[face as usize] = self.inputs[i];
            sides.output[face as usize] = self.outputs[i];
        }
        sides
    }
}

/// Os blocos da rede de energia de uma carga de dados.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnergyBlocks {
    blocks: BTreeMap<String, EnergyBlock>,
}

impl EnergyBlocks {
    pub fn load(files: &[(String, Value)]) -> Self {
        let blocks = files
            .iter()
            .map(|(id, json)| (id.clone(), EnergyBlock::parse(json)));
        Self {
            blocks: blocks.collect(),
        }
    }

    /// Se dá para entregar energia pela face `face` de `block`. Quem não se declarou fica com a
    /// regra do Horizon, porque pode ser de um mod que não conhece a lib.
    pub fn receives(&self, block: &str, front: Face, face: Face) -> bool {
        match self.blocks.get(block) {
            Some(declared) => declared.sides(front).accepts_input(face),
            None => true,
        }
    }
}

static CURRENT: RwLock<Option<Arc<EnergyBlocks>>> = RwLock::new(None);

/// Troca os blocos em uso: a cada carga de dados, inclusive no `/reload`.
pub fn set_current(blocks: EnergyBlocks) {
    let mut current = CURRENT.write().unwrap_or_else(|e| e.into_inner());
    *current = Some(Arc::new(blocks));
}

/// Os blocos em uso: nenhum antes da primeira carga.
pub fn current() -> Arc<EnergyBlocks> {
    let current = CURRENT.read().unwrap_or_else(|e| e.into_inner());
    current.clone().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// As faces de energia de `block`, que tem a frente em `front`. Um bloco que não se declarou não tem nenhuma.
    fn sides(blocks: &EnergyBlocks, block: &str, front: Face) -> Sides {
        blocks
            .blocks
            .get(block)
            .map_or_else(Sides::default, |block| block.sides(front))
    }

    /// Se um bloco troca energia pela face `face`, em qualquer sentido.
    fn joins(blocks: &EnergyBlocks, block: &str, front: Face, face: Face) -> bool {
        let sides = sides(blocks, block, front);
        sides.accepts_input(face) || sides.gives_output(face)
    }

    fn blocks() -> EnergyBlocks {
        EnergyBlocks::load(&[
            ("a:gerador".to_owned(), json!({"outputs": ["front"]})),
            ("a:maquina".to_owned(), json!({"inputs": "all"})),
            (
                "a:bateria".to_owned(),
                json!({"inputs": ["back", "left"], "outputs": ["front"]}),
            ),
        ])
    }

    #[test]
    fn a_generator_joins_only_by_its_front_and_takes_nothing() {
        let blocks = blocks();
        for front in [Face::North, Face::South, Face::East, Face::West] {
            for face in Face::ALL {
                assert_eq!(
                    joins(&blocks, "a:gerador", front, face),
                    face == front,
                    "{front:?} {face:?}"
                );
                assert!(!blocks.receives("a:gerador", front, face));
            }
        }
    }

    #[test]
    fn a_machine_takes_by_every_face_and_gives_by_none() {
        let blocks = blocks();
        for face in Face::ALL {
            assert!(joins(&blocks, "a:maquina", Face::North, face));
            assert!(blocks.receives("a:maquina", Face::North, face));
        }
        assert_eq!(sides(&blocks, "a:maquina", Face::North).output, [false; 6]);
    }

    #[test]
    fn a_battery_tells_its_input_sides_from_its_output_side() {
        let blocks = blocks();
        let front = Face::East;
        // A saída liga o cabo, mas a rede não entrega por ela.
        assert!(joins(&blocks, "a:bateria", front, front));
        assert!(!blocks.receives("a:bateria", front, front));
        assert!(sides(&blocks, "a:bateria", front).gives_output(front));
        // As entradas ligam e recebem, e não entregam.
        for input in [front.opposite(), front.left_of()] {
            assert!(joins(&blocks, "a:bateria", front, input));
            assert!(blocks.receives("a:bateria", front, input));
            assert!(!sides(&blocks, "a:bateria", front).gives_output(input));
        }
        // Lado nenhum: nem liga.
        assert!(!joins(&blocks, "a:bateria", front, front.right_of()));
    }

    #[test]
    fn a_side_can_go_both_ways() {
        let blocks = EnergyBlocks::load(&[(
            "a:x".to_owned(),
            json!({"inputs": ["top"], "outputs": ["top"]}),
        )]);
        let sides = sides(&blocks, "a:x", Face::North);
        assert!(sides.accepts_input(Face::Up) && sides.gives_output(Face::Up));
    }

    #[test]
    fn a_block_nobody_declared_does_not_join_but_keeps_the_game_rule() {
        assert!(!joins(&blocks(), "a:pedra", Face::North, Face::North));
        assert!(blocks().receives("a:outro_mod", Face::North, Face::North));
    }

    #[test]
    fn relative_sides_follow_the_front() {
        let blocks = EnergyBlocks::load(&[("a:x".to_owned(), json!({"inputs": ["left", "top"]}))]);
        assert!(joins(&blocks, "a:x", Face::North, Face::North.left_of()));
        assert!(joins(&blocks, "a:x", Face::East, Face::East.left_of()));
        assert!(joins(&blocks, "a:x", Face::East, Face::Up));
        assert!(!joins(&blocks, "a:x", Face::East, Face::Down));
    }
}
