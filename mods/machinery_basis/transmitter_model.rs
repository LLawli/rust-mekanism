// Entra em `crates/horizon-client-render/src/mesher.rs` como `pub mod machinery_transmitter_model` (`visibility = "pub"`). As partes do modelo de um transmissor por instância: o
// `TransmitterBlockStateModel` do original escolhe, para cada um dos seis lados, uma parte do OBJ pelo `ModelData` da block entity (`TransmitterModelData`), e o mesher do Horizon só
// escolhe o modelo pelo estado do bloco. O blockstate do port é um `multipart` com as 36 partes (um modelo por lado e estado visual; só as `NONE` valem por padrão, o que o original
// mostra sem dado), e a escolha por instância é a do `StateModels::state`: o HEAD dele pergunta a `select` quais partes valem para o bloco que o mesher está desenhando. O `StateModels`
// não recebe a posição, que o mesher anota por thread (`note_section` no começo de `mesh_reusing` e `note_block` em `hidden`, que o laço chama com as coordenadas de cada bloco logo antes de
// pedir o modelo). A conta do dado do modelo (`getVisualStatus`, o que o `hideContiguousJoin` esconde), a tabela dos estados que são de transmissor e o dado de cada posição ficam aqui.
// O código é puro e só de `std`: o mesmo arquivo entra no teste de `machinery_basis` (`core/tests/transmitter_model_module.rs`), que o confere contra o original
// (`rastros/transmitter_model_visual.jsonl`, `testes/TransmitterModelTraceTest.java`).
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

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

/// Quantas partes o blockstate de uma família tem por lado, e o índice de uma delas na ordem do `multipart` (lado, depois estado visual): sem o `hideContiguousJoin` (o desviador) o
/// modelo não tem as duas partes do trecho reto.
pub fn parts_per_side(diversion: bool) -> usize {
    if diversion { 4 } else { STATUS_NAMES.len() }
}

/// As 18 cores do vidro (`EnumColor.VALUES`): cada uma tem as partes dele no `multipart` do blockstate (o Horizon não tem tinta de bloco por posição, H237: o `Kd` do vidro de cada cor
/// leva o produto da tinta pelo `Kd` do original).
pub const GLASS_COLORS: usize = 18;

/// O índice, no `multipart` do blockstate, da parte de cada lado com o estado visual dado. Com o vidro (`glass`, a cor do transportador logístico pintado, `EnumColor.ordinal()`) cada lado
/// leva também a parte do vidro (`TransmitterBlockStateModel.collectParts`: `addPart(baseParts, ...)` e, com `needsGlass`, `addPart(glassParts, ...)`, por lado): depois das partes do corpo
/// vêm as do vidro de cada cor, de cor em cor, na mesma ordem de lado e estado.
pub fn selection(statuses: [u8; 6], diversion: bool, glass: Option<u8>) -> Vec<usize> {
    let per_side = parts_per_side(diversion);
    let body = SIDES.len() * per_side;
    let mut parts = Vec::with_capacity(if glass.is_some() {
        2 * SIDES.len()
    } else {
        SIDES.len()
    });
    for (side, status) in statuses.iter().enumerate() {
        let index = side * per_side + usize::from(*status);
        parts.push(index);
        if let Some(color) = glass {
            parts.push(body + usize::from(color).min(GLASS_COLORS - 1) * body + index);
        }
    }
    parts
}

/// O que a block entity dá ao desenho de uma posição: o `ConnectionType` de cada lado e, se o transmissor tem cor (`TransmitterModelData.getHasColor`, o transportador logístico pintado),
/// a cor dele (`EnumColor.ordinal()`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelData {
    pub connections: [u8; 6],
    pub color: Option<u8>,
}

struct Table {
    /// `states[estado]` é o índice em `diversion`, ou `u16::MAX` se o estado não é de transmissor.
    states: Vec<u16>,
    /// Por família: se a block entity dá o dado do desviador (`TransmitterModelData.Diversion`, que nunca junta os trechos retos).
    diversion: Vec<bool>,
    /// Por família: se o blockstate tem o vidro (`glass`, só o transportador logístico).
    glass: Vec<bool>,
}

static TABLE: RwLock<Option<Arc<Table>>> = RwLock::new(None);
static GENERATION: AtomicU64 = AtomicU64::new(1);
static DATA: RwLock<Option<HashMap<[i32; 3], ModelData>>> = RwLock::new(None);
/// Os conjuntos de partes já pedidos (cada um vira uma seleção do blockstate que vive até o fim do processo: poucas centenas).
type Leaked = HashMap<(usize, Vec<usize>), &'static (dyn Any + Send + Sync)>;
static SELECTIONS: Mutex<Option<Leaked>> = Mutex::new(None);

thread_local! {
    static LOCAL: RefCell<(u64, Option<Arc<Table>>)> = const { RefCell::new((0, None)) };
    /// A origem da seção que o mesher desenha nesta thread e as coordenadas locais do último bloco que ele examinou.
    static SECTION: Cell<[i32; 3]> = const { Cell::new([0; 3]) };
    static BLOCK: Cell<Option<[i32; 3]>> = const { Cell::new(None) };
}

/// A tabela dos estados de transmissor: `states` é o `(estado, família)` de cada um, `diversion` diz, por família, se ela dá o dado do desviador e `glass` se o blockstate dela tem o
/// vidro. Refeita a cada carga de dados.
pub fn set_families(diversion: Vec<bool>, glass: Vec<bool>, states: Vec<(u32, u16)>) {
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
        diversion,
        glass,
    }));
    GENERATION.fetch_add(1, Ordering::Release);
}

/// Se a tabela dos estados de transmissor já existe: o convidado de LAN não carrega os dados do servidor (`server_data_loaded` só roda em quem simula o mundo), então o cliente dele a monta
/// sozinho do registro que recebeu (`transmitter_client::model_tick`), e sem ela toda seleção cai no `multipart` padrão, só o centro de cada bloco.
pub fn has_table() -> bool {
    TABLE.read().unwrap_or_else(|e| e.into_inner()).is_some()
}

/// A tabela some (o mundo fechou): a próxima conexão monta a dela do registro novo.
pub fn clear_table() {
    *TABLE.write().unwrap_or_else(|e| e.into_inner()) = None;
    GENERATION.fetch_add(1, Ordering::Release);
}

/// O dado do modelo da block entity em `pos`: o `ConnectionType` de cada lado (`updateModelData`, `myTransmitter.getConnectionType(side)`; o estado visual sai disso em `select`, pela
/// família) e a cor do transmissor (`setHasColor`). `None` tira o dado (a block entity saiu, ou o chunk). Devolve se mudou algo.
pub fn set_data(pos: [i32; 3], data: Option<ModelData>) -> bool {
    let mut all = DATA.write().unwrap_or_else(|e| e.into_inner());
    let all = all.get_or_insert_with(HashMap::new);
    match data {
        Some(data) => all.insert(pos, data) != Some(data),
        None => all.remove(&pos).is_some(),
    }
}

/// O dado de `pos`, se há.
pub fn data_at(pos: [i32; 3]) -> Option<ModelData> {
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

/// O começo de `Mesher::mesh_reusing`: a origem da seção (`chunk.x * 16`, o `y` da base da seção e `chunk.z * 16`) que o mesher vai desenhar nesta thread.
pub fn note_section(origin: [i32; 3]) {
    SECTION.with(|section| section.set(origin));
}

/// `hidden(rows, x, y, z)`, que o laço do mesher chama com as coordenadas locais de cada bloco antes de pedir o modelo dele.
pub fn note_block(x: i32, y: i32, z: i32) {
    BLOCK.with(|block| block.set(Some([x, y, z])));
}

/// O HEAD de `StateModels::state`: as partes do bloco que o mesher está desenhando, se o estado é de transmissor e a block entity já deu o dado do modelo. `choice` é a escolha de
/// blockstate do estado (o primeiro campo do par que `state` devolve), e `make` monta o par com as partes (o `multipart` do blockstate). Cada conjunto de partes é montado uma vez e fica.
/// `None` deixa o modelo do blockstate (o `NONE` de todo lado, o que o original mostra sem dado).
pub fn select<T: Any + Send + Sync>(
    state: u32,
    choice: Option<usize>,
    make: impl FnOnce(usize, Vec<usize>) -> T,
) -> Option<&'static T> {
    let choice = choice?;
    let local = BLOCK.with(Cell::get)?;
    let generation = GENERATION.load(Ordering::Acquire);
    let (diversion, glass) = LOCAL.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.0 != generation {
            *cache = (
                generation,
                TABLE.read().unwrap_or_else(|e| e.into_inner()).clone(),
            );
        }
        let table = cache.1.as_ref()?;
        let family = table
            .states
            .get(state as usize)
            .copied()
            .filter(|family| *family != u16::MAX)?;
        Some((
            table.diversion[usize::from(family)],
            table.glass[usize::from(family)],
        ))
    })?;
    let origin = SECTION.with(Cell::get);
    let pos = [
        origin[0] + local[0],
        origin[1] + local[1],
        origin[2] + local[2],
    ];
    let data = data_at(pos)?;
    // Sem cor o transportador não tem vidro (`needsGlass = glassParts != null && getHasColor()`).
    let glass = data.color.filter(|_| glass);
    let parts = selection(
        visual_statuses(data.connections, diversion),
        diversion,
        glass,
    );
    let mut selections = SELECTIONS.lock().unwrap_or_else(|e| e.into_inner());
    let selections = selections.get_or_insert_with(HashMap::new);
    let key = (choice, parts);
    if let Some(known) = selections.get(&key) {
        return known.downcast_ref::<T>();
    }
    let value: &'static (dyn Any + Send + Sync) = Box::leak(Box::new(make(choice, key.1.clone())));
    selections.insert(key, value);
    value.downcast_ref::<T>()
}
