// Entra em `crates/horizon-client-render/src/collisions.rs` como `pub mod machinery_transmitter_collision` (`visibility = "pub"`). A forma de colisão de um transmissor por instância
// (`BlockTransmitter.getCollisionShape` = `getRealShape`: o centro e um braço por lado ligado, cada um com a forma do tipo dele, o `PUSH` e o `PULL` de três caixas): o Horizon tem uma forma
// de colisão por estado de bloco (`pipe_shape`: o centro e o braço normal de cada lado ligado), e o estado não sabe o tipo de cada lado. Quem publica o transmissor (o dono, no servidor)
// guarda aqui as caixas da forma dele quando algum lado é `PUSH` ou `PULL` (as outras combinações são a forma do estado, a mesma volume a volume), por dimensão e posição, e o HEAD de
// `SceneCollisions::context_shape` (injeção `transmitter_collision`) entrega a forma guardada no lugar da do estado. A tabela é do processo: o cliente e o servidor integrado rodam em threads
// diferentes e os dois varrem colisão (o jogador e o item solto); a dimensão em que a varredura roda é a do nível no lugar da thread (`enter`).
use horizon_core::geometry::{Aabb, ShapeOp, Vec3, VoxelShape};
use std::{
    cell::Cell,
    collections::HashMap,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

/// As caixas de cada transmissor com forma guardada, por dimensão e posição.
type Table = HashMap<(&'static str, [i32; 3]), Vec<[f64; 6]>>;

static SHAPES: Mutex<Option<Table>> = Mutex::new(None);
/// O número de formas guardadas: sem nenhuma, a varredura de colisão não toca a trava.
static COUNT: AtomicUsize = AtomicUsize::new(0);
/// A sonda da cópia de teste (`enviar_fontes.sh` não a liga: o roteiro `h2_cabo_forma.py` a liga na cópia): `MBC` a thread, o nível dela e se a forma guardada serviu, nas primeiras buscas de uma posição que tem forma guardada.
const COLLISION_PROBE: bool = false;
static PROBED: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static LEVEL: Cell<&'static str> = const { Cell::new("") };
}

fn table() -> std::sync::MutexGuard<'static, Option<Table>> {
    SHAPES.lock().unwrap_or_else(|error| error.into_inner())
}

/// O nível no lugar desta thread mudou (`Game::enter_level`) ou é este (o publicador e o tique do cliente dizem qual é).
pub fn enter(dimension: &'static str) {
    LEVEL.with(|level| level.set(dimension));
}

/// A forma de colisão do transmissor em `pos` na dimensão `dimension` passa a ser a união das `boxes` (em coordenadas do bloco). Sem caixas, a forma do estado volta.
pub fn set(dimension: &'static str, pos: [i32; 3], boxes: Option<Vec<[f64; 6]>>) {
    let mut shapes = table();
    let map = shapes.get_or_insert_with(HashMap::new);
    match boxes {
        Some(boxes) => {
            map.insert((dimension, pos), boxes);
        }
        None => {
            map.remove(&(dimension, pos));
        }
    }
    COUNT.store(map.len(), Ordering::Relaxed);
}

/// O mundo fechou: nenhuma forma guardada vale mais.
pub fn clear() {
    let mut shapes = table();
    if let Some(map) = shapes.as_mut() {
        map.clear();
    }
    COUNT.store(0, Ordering::Relaxed);
}

/// A forma guardada do transmissor em `pos`, no nível no lugar desta thread, se há; `None` deixa o Horizon com a forma do estado.
pub fn shape_at(pos: [i32; 3]) -> Option<VoxelShape> {
    if COUNT.load(Ordering::Relaxed) == 0 {
        return None;
    }
    let level = LEVEL.with(Cell::get);
    if COLLISION_PROBE && PROBED.load(Ordering::Relaxed) < 400 {
        let guard = table();
        let known = guard.as_ref().is_some_and(|map| map.keys().any(|(_, at)| *at == pos));
        if known {
            PROBED.fetch_add(1, Ordering::Relaxed);
            let hit = guard.as_ref().is_some_and(|map| map.contains_key(&(level, pos)));
            eprintln!("MBC {{\"thread\":\"{}\",\"level\":\"{level}\",\"p\":{pos:?},\"hit\":{hit}}}", std::thread::current().name().unwrap_or("?"));
        }
    }
    let boxes = table().as_ref()?.get(&(level, pos))?.clone();
    let mut shape = VoxelShape::empty();
    for part in boxes {
        let part = VoxelShape::from_box(Aabb::new(Vec3::new(part[0], part[1], part[2]), Vec3::new(part[3], part[4], part[5]))).ok()?;
        shape = shape.join(&part, ShapeOp::OR).ok()?;
    }
    shape.optimized().ok()
}

/// As caixas da forma guardada (`shape_at`), em coordenadas do bloco: o que a flecha lê (`ArrowScene::collision_boxes`).
pub fn boxes_at(pos: [i32; 3]) -> Option<Vec<Aabb>> {
    shape_at(pos).map(|shape| shape.boxes().to_vec())
}
