//! Redes de transmissores. Não existe objeto de rede: quem tem algo a entregar percorre as ligações
//! a partir de si, e a divisão é a do `SplitInfo` do Mekanism.
use crate::sides::{Face, Sides};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet, VecDeque};
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
    transmitter: bool,
    /// Por onde a energia entra, na ordem de `SIDE_NAMES`.
    inputs: [bool; 6],
    /// Por onde a energia sai, na ordem de `SIDE_NAMES`.
    outputs: [bool; 6],
}

impl EnergyBlock {
    /// `{"transmitter": true}`, ou `{"inputs": "all"}`, `{"outputs": ["front"]}` e combinações.
    fn parse(json: &Value) -> Self {
        Self {
            transmitter: json.get("transmitter").and_then(Value::as_bool) == Some(true),
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

    pub fn is_transmitter(&self, block: &str) -> bool {
        self.blocks
            .get(block)
            .is_some_and(|block| block.transmitter)
    }

    /// As faces de energia de `block`, que tem a frente em `front`. Um bloco que não se declarou
    /// não tem nenhuma.
    pub fn sides(&self, block: &str, front: Face) -> Sides {
        let declared = self.blocks.get(block);
        declared.map_or_else(Sides::default, |block| block.sides(front))
    }

    /// Se um transmissor se liga à face `face` de `block`: há ligação onde há troca de energia,
    /// em qualquer sentido.
    pub fn joins(&self, block: &str, front: Face, face: Face) -> bool {
        let sides = self.sides(block, front);
        self.is_transmitter(block) || sides.accepts_input(face) || sides.gives_output(face)
    }

    /// Se dá para entregar energia pela face `face` de `block`. Quem não se declarou fica com a
    /// regra do Horizon, porque pode ser de um mod que não conhece a lib.
    pub fn receives(&self, block: &str, front: Face, face: Face) -> bool {
        match self.blocks.get(block) {
            Some(declared) => declared.transmitter || declared.sides(front).accepts_input(face),
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

/// O que uma ligação alcança.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link {
    Transmitter,
    Acceptor,
}

/// Quem a rede alcança a partir de `start`: a posição de cada consumidor e a face dele que a rede
/// toca. Um consumidor tocado por dois transmissores aparece duas vezes, como no original.
pub fn acceptors(
    start: [i32; 3],
    mut links: impl FnMut([i32; 3]) -> [Option<Link>; 6],
) -> Vec<([i32; 3], Face)> {
    let mut found = Vec::new();
    let mut seen = HashSet::from([start]);
    let mut queue = VecDeque::from([start]);
    while let Some(pos) = queue.pop_front() {
        let around = links(pos);
        for face in Face::ALL {
            let [x, y, z] = face.offset();
            let next = [pos[0] + x, pos[1] + y, pos[2] + z];
            match around[face as usize] {
                Some(Link::Acceptor) => found.push((next, face.opposite())),
                Some(Link::Transmitter) if seen.insert(next) => queue.push_back(next),
                _ => {}
            }
        }
    }
    found
}

/// Divide `amount` entre quem pede `wanted`. Quem pede menos que a parte igual leva o que pediu, e
/// o que sobra volta para os outros. O resto da divisão vai aos primeiros, um para cada.
pub fn split_fairly(amount: i32, wanted: &[i32]) -> Vec<i32> {
    let mut shares = vec![0; wanted.len()];
    let mut open: Vec<usize> = (0..wanted.len()).filter(|&i| wanted[i] > 0).collect();
    let mut left = amount;
    loop {
        let Some(count) = i32::try_from(open.len()).ok().filter(|&count| count > 0) else {
            return shares;
        };
        let share = left / count;
        let (small, big): (Vec<usize>, Vec<usize>) =
            open.iter().partition(|&&i| wanted[i] <= share);
        if small.is_empty() {
            let mut extra = left % count;
            for i in big {
                let bonus = i32::from(extra > 0 && wanted[i] > share);
                shares[i] = share + bonus;
                extra -= bonus;
            }
            return shares;
        }
        for i in small {
            shares[i] = wanted[i];
            left -= wanted[i];
        }
        open = big;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    fn blocks() -> EnergyBlocks {
        EnergyBlocks::load(&[
            ("a:cabo".to_owned(), json!({"transmitter": true})),
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
                    blocks.joins("a:gerador", front, face),
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
            assert!(blocks.joins("a:maquina", Face::North, face));
            assert!(blocks.receives("a:maquina", Face::North, face));
        }
        assert_eq!(blocks.sides("a:maquina", Face::North).output, [false; 6]);
    }

    #[test]
    fn a_battery_tells_its_input_sides_from_its_output_side() {
        let blocks = blocks();
        let front = Face::East;
        // A saída liga o cabo, mas a rede não entrega por ela.
        assert!(blocks.joins("a:bateria", front, front));
        assert!(!blocks.receives("a:bateria", front, front));
        assert!(blocks.sides("a:bateria", front).gives_output(front));
        // As entradas ligam e recebem, e não entregam.
        for input in [front.opposite(), front.left_of()] {
            assert!(blocks.joins("a:bateria", front, input));
            assert!(blocks.receives("a:bateria", front, input));
            assert!(!blocks.sides("a:bateria", front).gives_output(input));
        }
        // Lado nenhum: nem liga.
        assert!(!blocks.joins("a:bateria", front, front.right_of()));
    }

    #[test]
    fn a_side_can_go_both_ways() {
        let blocks = EnergyBlocks::load(&[(
            "a:x".to_owned(),
            json!({"inputs": ["top"], "outputs": ["top"]}),
        )]);
        let sides = blocks.sides("a:x", Face::North);
        assert!(sides.accepts_input(Face::Up) && sides.gives_output(Face::Up));
    }

    #[test]
    fn a_transmitter_joins_and_receives_by_every_face() {
        let blocks = blocks();
        assert!(blocks.is_transmitter("a:cabo"));
        assert!(!blocks.is_transmitter("a:maquina"));
        assert!(blocks.joins("a:cabo", Face::North, Face::Down));
        assert!(blocks.receives("a:cabo", Face::North, Face::Down));
    }

    #[test]
    fn a_block_nobody_declared_does_not_join_but_keeps_the_game_rule() {
        assert!(!blocks().joins("a:pedra", Face::North, Face::North));
        assert!(blocks().receives("a:outro_mod", Face::North, Face::North));
    }

    #[test]
    fn relative_sides_follow_the_front() {
        let blocks = EnergyBlocks::load(&[("a:x".to_owned(), json!({"inputs": ["left", "top"]}))]);
        assert!(blocks.joins("a:x", Face::North, Face::North.left_of()));
        assert!(blocks.joins("a:x", Face::East, Face::East.left_of()));
        assert!(blocks.joins("a:x", Face::East, Face::Up));
        assert!(!blocks.joins("a:x", Face::East, Face::Down));
    }

    /// Uma rede de teste: cada posição com o que tem em cada face.
    fn walk(start: [i32; 3], map: &HashMap<[i32; 3], [Option<Link>; 6]>) -> Vec<([i32; 3], Face)> {
        acceptors(start, |pos| map.get(&pos).copied().unwrap_or([None; 6]))
    }

    fn cable(links: &[(Face, Link)]) -> [Option<Link>; 6] {
        let mut around = [None; 6];
        for &(face, link) in links {
            around[face as usize] = Some(link);
        }
        around
    }

    #[test]
    fn a_line_reaches_the_machine_at_its_end() {
        let map = HashMap::from([
            ([0, 0, 0], cable(&[(Face::East, Link::Transmitter)])),
            (
                [1, 0, 0],
                cable(&[
                    (Face::East, Link::Transmitter),
                    (Face::West, Link::Transmitter),
                ]),
            ),
            (
                [2, 0, 0],
                cable(&[
                    (Face::West, Link::Transmitter),
                    (Face::East, Link::Acceptor),
                ]),
            ),
        ]);
        assert_eq!(walk([0, 0, 0], &map), [([3, 0, 0], Face::West)]);
    }

    #[test]
    fn a_loop_ends() {
        let round = cable(&[
            (Face::East, Link::Transmitter),
            (Face::West, Link::Transmitter),
        ]);
        let map = HashMap::from([([0, 0, 0], round), ([1, 0, 0], round), ([-1, 0, 0], round)]);
        assert!(walk([0, 0, 0], &map).len() < 10);
    }

    #[test]
    fn a_machine_touched_twice_counts_twice() {
        let map = HashMap::from([
            (
                [0, 0, 0],
                cable(&[(Face::Up, Link::Acceptor), (Face::East, Link::Transmitter)]),
            ),
            (
                [1, 0, 0],
                cable(&[(Face::West, Link::Transmitter), (Face::Up, Link::Acceptor)]),
            ),
        ]);
        assert_eq!(walk([0, 0, 0], &map).len(), 2);
    }

    #[test]
    fn nothing_connected_is_nobody() {
        assert!(walk([0, 0, 0], &HashMap::new()).is_empty());
    }

    #[test]
    fn splits_evenly() {
        assert_eq!(split_fairly(90, &[100, 100, 100]), [30, 30, 30]);
    }

    #[test]
    fn a_small_need_gives_the_rest_back() {
        assert_eq!(split_fairly(90, &[10, 100, 100]), [10, 40, 40]);
    }

    #[test]
    fn the_remainder_goes_to_the_first_ones() {
        assert_eq!(split_fairly(11, &[100, 100, 100]), [4, 4, 3]);
    }

    #[test]
    fn the_remainder_skips_who_has_enough() {
        assert_eq!(split_fairly(10, &[3, 100, 100]), [3, 4, 3]);
    }

    #[test]
    fn nobody_gets_more_than_it_asked() {
        assert_eq!(split_fairly(1_000, &[5, 7]), [5, 7]);
        assert_eq!(split_fairly(10, &[0, 0]), [0, 0]);
        assert_eq!(split_fairly(0, &[5]), [0]);
    }

    #[test]
    fn nothing_is_lost_or_created() {
        for amount in 0..60 {
            let wanted = [7, 0, 13, 2, 40];
            let shares = split_fairly(amount, &wanted);
            let sent: i32 = shares.iter().sum();
            assert_eq!(sent, amount.min(wanted.iter().sum()), "{amount}");
            assert!(
                shares
                    .iter()
                    .zip(wanted)
                    .all(|(&share, want)| share <= want)
            );
        }
    }
}
