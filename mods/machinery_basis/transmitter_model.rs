// Entra em `crates/horizon-client-render/src/models/catalog.rs` como `pub mod machinery_transmitter_model` (`visibility = "pub"`). As partes do modelo de um transmissor por instância: o
// `TransmitterBlockStateModel` do original escolhe, para cada um dos seis lados, uma parte do OBJ pelo `ModelData` da block entity (`TransmitterModelData`), e o mesher do Horizon só
// escolhe o modelo pelo estado do bloco. A conta do dado do modelo (`getVisualStatus`, o que o `hideContiguousJoin` esconde), a tabela dos estados que são de transmissor e o dado de cada
// posição ficam aqui; o HEAD de `ModelCatalog::models_at` pede as partes a `fill`. O código é puro e só de `std`: o mesmo arquivo entra no teste de `machinery_basis`
// (`core/tests/transmitter_model_module.rs`), que o confere contra o original (`rastros/transmitter_model_visual.jsonl`, `testes/TransmitterModelTraceTest.java`).
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

/// `ConnectionType.ordinal()`: `NORMAL`, `PUSH`, `PULL` e `NONE`.
pub const CONNECTION_NONE: u8 = 3;

/// `VisualConnectionStatus.ordinal()`.
pub const NORMAL: u8 = 0;
pub const PUSH: u8 = 1;
pub const PULL: u8 = 2;
pub const NONE: u8 = 3;
pub const NONE_CONTIGUOUS: u8 = 4;
pub const NONE_CONTIGUOUS_ROTATED: u8 = 5;

/// O lado, na ordem de `EnumUtils.DIRECTIONS` (`Direction.ordinal()`), em `Direction.getSerializedName`.
pub const SIDES: [&str; 6] = ["down", "up", "north", "south", "west", "east"];
/// O fim do nome do modelo de uma parte, por `VisualConnectionStatus.ordinal()`.
pub const STATUS_NAMES: [&str; 6] = [
    "normal",
    "push",
    "pull",
    "none",
    "none_contiguous",
    "none_contiguous_rotated",
];

/// `TransmitterModelData.getStatus`: o lado sem conexão com ligação em um par de lados perpendiculares e em outro não. `a` e `b` são um par de lados opostos e `c` e `d` o outro.
fn status(a: u8, b: u8, c: u8, d: u8) -> u8 {
    let has = |connection: u8| connection != CONNECTION_NONE;
    let (has_a, has_b, has_c, has_d) = (has(a), has(b), has(c), has(d));
    if (has_a || has_b) != (has_c || has_d) {
        if has_a && has_b {
            return NONE_CONTIGUOUS;
        } else if has_c && has_d {
            return NONE_CONTIGUOUS_ROTATED;
        }
    }
    NONE
}

/// `TransmitterModelData.setConnectionData` (`getVisualStatus`): o estado visual de cada lado, dados os `ConnectionType` dos seis (`connections`, em `Direction.ordinal()`). Um lado ligado
/// vale o próprio tipo; o `Diversion` (o desviador) nunca junta os trechos retos.
pub fn visual_statuses(connections: [u8; 6], diversion: bool) -> [u8; 6] {
    let [down, up, north, south, west, east] = connections;
    std::array::from_fn(|side| {
        if connections[side] != CONNECTION_NONE || diversion {
            return connections[side];
        }
        match side {
            0 | 1 => status(north, south, east, west),
            2 | 3 => status(up, down, east, west),
            _ => status(up, down, north, south),
        }
    })
}

/// A família de um transmissor: o nome dos modelos das partes (`<prefixo>/<lado>_<estado>`) e se o dado do modelo dela é o do desviador (`TransmitterModelData.Diversion`, que nunca junta
/// os trechos retos).
pub struct Family {
    names: Vec<String>,
    diversion: bool,
}

impl Family {
    /// Os nomes dos 36 modelos (seis lados, seis estados), na ordem lado e estado.
    pub fn new(prefix: &str, diversion: bool) -> Family {
        Family {
            diversion,
            names: SIDES
                .iter()
                .flat_map(|side| {
                    STATUS_NAMES
                        .iter()
                        .map(move |name| format!("{prefix}/{side}_{name}"))
                })
                .collect(),
        }
    }

    /// O modelo da parte de um lado e estado.
    pub fn part(&self, side: usize, status: u8) -> &str {
        &self.names[side * STATUS_NAMES.len() + usize::from(status)]
    }
}

struct Table {
    /// `states[estado]` é o índice em `families`, ou `u16::MAX` se o estado não é de transmissor.
    states: Vec<u16>,
    families: Vec<Family>,
}

static TABLE: RwLock<Option<Arc<Table>>> = RwLock::new(None);
static GENERATION: AtomicU64 = AtomicU64::new(1);
static DATA: RwLock<Option<HashMap<[i32; 3], [u8; 6]>>> = RwLock::new(None);

thread_local! {
    static LOCAL: RefCell<(u64, Option<Arc<Table>>)> = const { RefCell::new((0, None)) };
}

/// A tabela dos estados de transmissor: `states[i]` é o `(estado, família)` de cada um, refeita a cada carga de dados.
pub fn set_families(families: Vec<Family>, states: Vec<(u32, u16)>) {
    let size = states
        .iter()
        .map(|(state, _)| *state as usize + 1)
        .max()
        .unwrap_or(0);
    let mut by_state = vec![u16::MAX; size];
    for (state, family) in states {
        by_state[state as usize] = family;
    }
    *TABLE.write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(Table {
        states: by_state,
        families,
    }));
    GENERATION.fetch_add(1, Ordering::Release);
}

/// O dado do modelo da block entity em `pos`: o `ConnectionType` de cada lado (`updateModelData`, `myTransmitter.getConnectionType(side)`; o estado visual sai disso em `fill`, pela família).
/// `None` tira o dado (a block entity saiu, ou o chunk). Devolve se mudou algo.
pub fn set_data(pos: [i32; 3], connections: Option<[u8; 6]>) -> bool {
    let mut data = DATA.write().unwrap_or_else(|e| e.into_inner());
    let data = data.get_or_insert_with(HashMap::new);
    match connections {
        Some(connections) => data.insert(pos, connections) != Some(connections),
        None => data.remove(&pos).is_some(),
    }
}

/// O dado de `pos`, se há.
pub fn data_at(pos: [i32; 3]) -> Option<[u8; 6]> {
    DATA.read()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()?
        .get(&pos)
        .copied()
}

/// Esvazia o dado de todas as posições (outra dimensão, ou o mundo fechou).
pub fn clear_data() {
    *DATA.write().unwrap_or_else(|e| e.into_inner()) = None;
}

/// O número de posições com dado.
pub fn data_count() -> usize {
    DATA.read()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map_or(0, HashMap::len)
}

/// `TransmitterBlockStateModel.collectParts`: as partes do transmissor em `pos`, uma por lado, na ordem de `EnumUtils.DIRECTIONS`. `lookup` dá o modelo cozido de um nome. Devolve `false`,
/// sem tocar em `out`, se o estado não é de transmissor ou a block entity ainda não deu o dado do modelo: o modelo do blockstate (o `NONE` de todo lado, o que o original faz sem dado)
/// segue. Uma parte sem modelo cozido é pulada, como o `partTable.get` nulo do original.
pub fn fill<'a, M>(
    state: u32,
    pos: [i32; 3],
    lookup: impl Fn(&str) -> Option<&'a M>,
    out: &mut Vec<&'a M>,
) -> bool {
    let generation = GENERATION.load(Ordering::Acquire);
    LOCAL.with(|local| {
        let mut local = local.borrow_mut();
        if local.0 != generation {
            *local = (
                generation,
                TABLE.read().unwrap_or_else(|e| e.into_inner()).clone(),
            );
        }
        let Some(table) = local.1.as_ref() else {
            return false;
        };
        let Some(family) = table
            .states
            .get(state as usize)
            .copied()
            .filter(|family| *family != u16::MAX)
        else {
            return false;
        };
        let Some(connections) = data_at(pos) else {
            return false;
        };
        let family = &table.families[usize::from(family)];
        let statuses = visual_statuses(connections, family.diversion);
        let parts: Vec<&'a M> = (0..SIDES.len())
            .filter_map(|side| lookup(family.part(side, statuses[side])))
            .collect();
        if parts.is_empty() {
            return false;
        }
        out.clear();
        out.extend(parts);
        true
    })
}
