// Entra em `crates/horizon-client-render/src/light.rs` como `pub mod machinery_light` (`visibility = "pub"`). A emissão de luz de um bloco por posição
// (`IBlockExtension.getLightEmission(state, level, pos)` do NeoForge): o motor de luz do Horizon lê a emissão de uma tabela por estado de bloco, e um bloco de mod pode emitir
// conforme o que a block entity dele guarda (o Fluid Tank do Mekanism com lava, `BlockFluidTank.getLightEmission`). Aqui ficam os estados que perguntam a posição e a emissão de
// cada posição, partilhados entre o jogo, que os escreve, e as threads de luz do `streaming.rs`, que os leem pelas cópias de `machinery_light_engine`.
//
// SÓ O MUNDO DO CLIENTE LÊ A POSIÇÃO. No original a block entity só responde na thread do nível (`Level.getBlockEntity`, `Level.java:665-673`: no servidor, fora da thread dele,
// devolve `null`), e o motor de luz do servidor roda em outra thread (`ThreadedLevelLightEngine`): lá `getLightEmission` do tanque dá sempre a luz do estado, e o mundo do servidor
// nunca ganha a luz do fluido. O do cliente roda na thread do cliente e a ganha. No Horizon o `IntegratedServer` e o `Client` são dois `Game` no mesmo processo, e esta tabela é
// uma só: a thread do cliente se marca ([`mark_client_thread`]) e diz o nível em que está ([`set_client_level`]), os trabalhos de luz de chunk inteiro do `Streaming` do cliente
// são anotados quando nascem ([`note_job`]), e todo o resto (o mundo do servidor) segue a tabela por estado.
// O código é puro e só de `std`: o mesmo arquivo entra no teste de `machinery_basis` (`core/tests/position_light_module.rs`).
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, RwLock};

/// Uma posição de bloco, na ordem do motor de luz (`light::Pos`).
pub type Pos = (i32, i32, i32);

#[derive(Default)]
struct Table {
    /// Por estado de bloco: se a emissão dele pergunta a posição.
    states: Vec<bool>,
    /// A emissão de cada posição, por nível (o nome da dimensão). Só o que emite fica guardado.
    cells: HashMap<Pos, Vec<(String, u8)>>,
    /// Quantas posições guardadas cada coluna de chunk tem.
    columns: HashMap<(i32, i32), u32>,
    /// O nível em que o cliente está: só as posições dele valem.
    current: String,
}

thread_local! {
    /// Se esta thread é a do `Game` do cliente.
    static CLIENT_THREAD: Cell<bool> = const { Cell::new(false) };
}
/// Os trabalhos de luz de chunk inteiro que o `Streaming` do cliente criou e ainda não rodaram, pelo endereço do chunk do centro.
static CLIENT_JOBS: Mutex<Option<HashSet<usize>>> = Mutex::new(None);
/// Acima disto a lista de trabalhos é esvaziada (os trabalhos abortados nunca saem dela).
const CLIENT_JOBS_MAX: usize = 4096;

/// A thread do `Game` do cliente se marca a cada tick: as trocas de luz que rodam nela (`relight_layer`) são as do mundo do cliente.
pub fn mark_client_thread() {
    CLIENT_THREAD.with(|flag| flag.set(true));
}

/// Se esta thread é a do cliente.
pub fn client_thread() -> bool {
    CLIENT_THREAD.with(Cell::get)
}

/// O nível em que o cliente está (o nome que ele passa a [`set`]).
pub fn set_client_level(level: &str) {
    if read(true, |table| table.current != level) {
        write(|table| table.current = level.to_owned());
    }
}

/// O `Streaming` criou um trabalho de luz de chunk inteiro: `centre` é o endereço do chunk do centro e `remote` diz se o `Streaming` é o do cliente de um servidor (o do
/// jogo de um jogador também). Só os do cliente ficam anotados, e só enquanto alguma posição emite.
pub fn note_job(centre: usize, remote: bool) {
    if !remote || !any() {
        return;
    }
    let mut guard = CLIENT_JOBS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let jobs = guard.get_or_insert_with(HashSet::new);
    if jobs.len() >= CLIENT_JOBS_MAX {
        jobs.clear();
    }
    jobs.insert(centre);
}

/// O trabalho de luz do chunk em `centre` vai rodar: devolve se ele é do cliente, e o tira da lista.
pub fn take_job(centre: usize) -> bool {
    if !any() {
        return false;
    }
    CLIENT_JOBS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .as_mut()
        .is_some_and(|jobs| jobs.remove(&centre))
}

static TABLE: RwLock<Option<Table>> = RwLock::new(None);
/// O tamanho de `cells`, para as threads de luz saírem sem pegar a trava quando nada emite por posição.
static CELLS: AtomicUsize = AtomicUsize::new(0);
/// Quantos estados perguntam a posição.
static STATES: AtomicUsize = AtomicUsize::new(0);

fn write<R>(act: impl FnOnce(&mut Table) -> R) -> R {
    let mut guard = TABLE.write().unwrap_or_else(|error| error.into_inner());
    let table = guard.get_or_insert_with(Table::default);
    let result = act(table);
    CELLS.store(table.cells.len(), Ordering::Release);
    STATES.store(
        table.states.iter().filter(|&&asks| asks).count(),
        Ordering::Release,
    );
    result
}

fn read<R>(none: R, act: impl FnOnce(&Table) -> R) -> R {
    let guard = TABLE.read().unwrap_or_else(|error| error.into_inner());
    match guard.as_ref() {
        Some(table) => act(table),
        None => none,
    }
}

/// Os estados de bloco cuja emissão pergunta a posição (`true` no índice do estado). Refeita a cada carga de dados.
pub fn set_states(states: Vec<bool>) {
    write(|table| table.states = states);
}

/// A emissão da block entity em `pos` do nível `level` (0 a 15). Zero tira a posição da tabela. Devolve se o valor guardado mudou.
pub fn set(level: &str, pos: Pos, emission: u8) -> bool {
    let emission = emission.min(15);
    write(|table| {
        let column = (pos.0 >> 4, pos.2 >> 4);
        let Some(levels) = table.cells.get_mut(&pos) else {
            if emission == 0 {
                return false;
            }
            table.cells.insert(pos, vec![(level.to_owned(), emission)]);
            *table.columns.entry(column).or_default() += 1;
            return true;
        };
        let at = levels.iter().position(|(name, _)| name == level);
        match (at, emission) {
            (None, 0) => false,
            (None, _) => {
                levels.push((level.to_owned(), emission));
                true
            }
            (Some(at), 0) => {
                levels.swap_remove(at);
                if levels.is_empty() {
                    table.cells.remove(&pos);
                    match table.columns.get_mut(&column) {
                        Some(count) if *count > 1 => *count -= 1,
                        _ => {
                            table.columns.remove(&column);
                        }
                    }
                }
                true
            }
            (Some(at), _) => {
                let changed = levels[at].1 != emission;
                levels[at].1 = emission;
                changed
            }
        }
    })
}

/// A emissão guardada para `pos` no nível `level`.
pub fn get(level: &str, pos: Pos) -> u8 {
    read(0, |table| {
        table
            .cells
            .get(&pos)
            .and_then(|levels| levels.iter().find(|(name, _)| name == level))
            .map_or(0, |(_, emission)| *emission)
    })
}

/// Tira tudo de um nível (o nível fechou).
pub fn clear_level(level: &str) {
    let positions: Vec<Pos> = read(Vec::new(), |table| {
        table
            .cells
            .iter()
            .filter(|(_, levels)| levels.iter().any(|(name, _)| name == level))
            .map(|(pos, _)| *pos)
            .collect()
    });
    for pos in positions {
        set(level, pos, 0);
    }
}

/// Se alguma posição emite (as threads de luz saem aqui quando não).
#[inline]
pub fn any() -> bool {
    CELLS.load(Ordering::Acquire) != 0
}

/// Se algum estado pergunta a posição.
#[inline]
pub fn any_state() -> bool {
    STATES.load(Ordering::Acquire) != 0
}

/// Se a emissão do estado pergunta a posição.
pub fn positional(state: u32) -> bool {
    any_state()
        && read(false, |table| {
            table.states.get(state as usize).copied().unwrap_or(false)
        })
}

/// Se alguma posição das colunas de chunk de `(cx - 1, cz - 1)` a `(cx + 1, cz + 1)` emite: o que um trabalho de luz do chunk `(cx, cz)` lê.
pub fn any_around(cx: i32, cz: i32) -> bool {
    any()
        && read(false, |table| {
            (-1..=1).any(|dz| (-1..=1).any(|dx| table.columns.contains_key(&(cx + dx, cz + dz))))
        })
}

/// As posições que emitem numa coluna de chunk no nível em que o cliente está, com a emissão: a foto que um trabalho de luz do cliente tira antes de varrer os blocos.
pub fn column(cx: i32, cz: i32) -> Vec<(Pos, u8)> {
    if !any() {
        return Vec::new();
    }
    read(Vec::new(), |table| {
        if !table.columns.contains_key(&(cx, cz)) {
            return Vec::new();
        }
        table
            .cells
            .iter()
            .filter(|(pos, _)| (pos.0 >> 4, pos.2 >> 4) == (cx, cz))
            .filter_map(|(pos, levels)| Some((*pos, here(levels, &table.current)?)))
            .collect()
    })
}

/// A emissão guardada para o nível `current` numa posição.
fn here(levels: &[(String, u8)], current: &str) -> Option<u8> {
    levels
        .iter()
        .find(|(name, _)| name == current)
        .map(|(_, emission)| *emission)
}

/// `getLightEmission(state, level, pos)` no mundo do cliente, no nível em que ele está: o maior entre a emissão do estado (`by_state`) e a da posição, que só vale para um estado que pergunta a posição (a emissão que sobrou
/// de um bloco que saiu não acende o que veio no lugar).
pub fn emission(state: u32, by_state: u8, pos: Pos) -> u8 {
    if !any() {
        return by_state;
    }
    read(by_state, |table| {
        if !table.states.get(state as usize).copied().unwrap_or(false) {
            return by_state;
        }
        let here = table
            .cells
            .get(&pos)
            .and_then(|levels| here(levels, &table.current))
            .unwrap_or(0);
        by_state.max(here)
    })
}

/// [`emission`] com a foto de [`column`] já tirada (`cells`) e o estado já conferido por [`positional`].
pub fn emission_in(cells: &[(Pos, u8)], by_state: u8, pos: Pos) -> u8 {
    cells
        .iter()
        .find(|(at, _)| *at == pos)
        .map_or(by_state, |(_, emission)| by_state.max(*emission))
}

/// `LightEngine.hasDifferentLightProperties` para um estado que pergunta a posição: a troca de estado nessa posição pode mudar a luz mesmo com a tabela por estado igual (o
/// tanque com lava que sai), e o `checkBlock` pedido pelo tile chega como uma troca do estado por ele mesmo.
pub fn differs(old: u32, new: u32) -> bool {
    // Só no mundo do cliente: no do servidor a emissão do tanque é sempre a do estado.
    client_thread()
        && any_state()
        && read(false, |table| {
            let asks = |state: u32| table.states.get(state as usize).copied().unwrap_or(false);
            asks(old) || asks(new)
        })
}
