//! A parede entre duas células: o `OCCLUSION_CACHE` de `canPassThroughWall` (`FlowingFluid.java:37-45,215-240`) e a conta de
//! `Shapes.mergedFaceOccludes` (`Shapes.java:260-282`) para formas alinhadas a 1/16.
use crate::sides::Face;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};

/// `CACHE_SIZE` (`FlowingFluid.java:36`).
pub const CACHE_SIZE: usize = 200;

/// `FlowingFluid.BlockStatePairKey` (`FlowingFluid.java:469-484`): os dois estados por identidade e a direção.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OcclusionKey {
    pub first: u32,
    pub second: u32,
    pub direction: Face,
}

impl Hash for OcclusionKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.first, self.second, self.direction as u8).hash(state);
    }
}

/// O `Object2ByteLinkedOpenHashMap` de 200 entradas sem `rehash`: o acerto vai para o começo, e o cheio perde o último antes de pôr o
/// novo no começo. Num estado sem forma dinâmica, o resultado só depende do par, então o cache não muda o que o fluido decide; ele está
/// aqui porque é o que o Java faz.
#[derive(Debug, Default)]
pub struct OcclusionCache {
    inner: RefCell<Lru>,
}

#[derive(Debug, Default)]
struct Lru {
    entries: HashMap<OcclusionKey, (bool, u64)>,
    order: BTreeMap<u64, OcclusionKey>,
    clock: u64,
}

impl Lru {
    fn stamp(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }
}

impl OcclusionCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// `getAndMoveToFirst` (`FlowingFluid.java:225`).
    pub fn get_and_move_to_first(&self, key: OcclusionKey) -> Option<bool> {
        let mut lru = self.inner.borrow_mut();
        let (value, old) = *lru.entries.get(&key)?;
        let stamp = lru.stamp();
        lru.order.remove(&old);
        lru.order.insert(stamp, key);
        lru.entries.insert(key, (value, stamp));
        Some(value)
    }

    /// `removeLastByte` quando está cheio e `putAndMoveToFirst` (`FlowingFluid.java:235-239`).
    pub fn put(&self, key: OcclusionKey, value: bool) {
        let mut lru = self.inner.borrow_mut();
        if lru.entries.len() == CACHE_SIZE
            && let Some((_, last)) = lru.order.pop_first()
        {
            lru.entries.remove(&last);
        }
        let stamp = lru.stamp();
        if let Some((_, old)) = lru.entries.insert(key, (value, stamp)) {
            lru.order.remove(&old);
        }
        lru.order.insert(stamp, key);
    }

    pub fn len(&self) -> usize {
        self.inner.borrow().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Uma forma de colisão feita de caixas em pixels (0 a 16 em cada eixo), como `Block.box`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PixelShape {
    /// `[min_x, min_y, min_z, max_x, max_y, max_z]`.
    pub boxes: Vec<[u8; 6]>,
}

const FULL: u8 = 16;

impl PixelShape {
    pub fn new(boxes: &[[u8; 6]]) -> Self {
        Self {
            boxes: boxes.to_vec(),
        }
    }

    /// As caixas que encostam na face `face` do bloco, projetadas nela: a fatia `SliceShape` da última camada (lado positivo) ou da
    /// primeira (lado negativo). Para formas em pixels a última camada é a das caixas que chegam a 16, e a primeira a das que saem de 0.
    fn face_cells(&self, face: Face) -> [[bool; 16]; 16] {
        let mut cells = [[false; 16]; 16];
        let (axis, positive) = axis_of(face);
        for b in &self.boxes {
            let touches = if positive {
                b[axis + 3] == FULL
            } else {
                b[axis] == 0
            };
            if !touches {
                continue;
            }
            let (u, v) = match axis {
                0 => (1, 2),
                1 => (0, 2),
                _ => (0, 1),
            };
            for row in cells.iter_mut().take(b[u + 3] as usize).skip(b[u] as usize) {
                for cell in row.iter_mut().take(b[v + 3] as usize).skip(b[v] as usize) {
                    *cell = true;
                }
            }
        }
        cells
    }

    /// `Shapes.mergedFaceOccludes(shape, occluder, direction)` (`Shapes.java:260-282`): a face de `shape` do lado `direction` e a face
    /// de `occluder` do lado oposto, juntas, cobrem a face inteira.
    pub fn merged_face_occludes(
        shape: &PixelShape,
        occluder: &PixelShape,
        direction: Face,
    ) -> bool {
        let first = shape.face_cells(direction);
        let second = occluder.face_cells(direction.opposite());
        first
            .iter()
            .zip(second.iter())
            .all(|(a, b)| a.iter().zip(b.iter()).all(|(x, y)| *x || *y))
    }
}

/// O eixo (0 = x, 1 = y, 2 = z) e se a face é do lado positivo.
fn axis_of(face: Face) -> (usize, bool) {
    match face {
        Face::West => (0, false),
        Face::East => (0, true),
        Face::Down => (1, false),
        Face::Up => (1, true),
        Face::North => (2, false),
        Face::South => (2, true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(n: u32) -> OcclusionKey {
        OcclusionKey {
            first: n,
            second: n + 1,
            direction: Face::North,
        }
    }

    #[test]
    fn the_cache_forgets_the_least_recently_used_pair_when_full() {
        let cache = OcclusionCache::new();
        for n in 0..CACHE_SIZE as u32 {
            cache.put(key(n), n % 2 == 0);
        }
        assert_eq!(cache.get_and_move_to_first(key(0)), Some(true));
        cache.put(key(1000), false);
        assert_eq!(cache.len(), CACHE_SIZE);
        assert_eq!(cache.get_and_move_to_first(key(1)), None);
        assert_eq!(cache.get_and_move_to_first(key(0)), Some(true));
        assert_eq!(cache.get_and_move_to_first(key(1000)), Some(false));
    }

    #[test]
    fn a_bottom_slab_and_a_top_slab_close_the_side_between_them_only_together() {
        let bottom = PixelShape::new(&[[0, 0, 0, 16, 8, 16]]);
        let top = PixelShape::new(&[[0, 8, 0, 16, 16, 16]]);
        assert!(PixelShape::merged_face_occludes(&bottom, &top, Face::East));
        assert!(!PixelShape::merged_face_occludes(
            &bottom,
            &bottom,
            Face::East
        ));
        // A face de cima da laje de baixo não encosta em 16: só a de cima do vizinho conta.
        assert!(!PixelShape::merged_face_occludes(
            &bottom,
            &PixelShape::default(),
            Face::Up
        ));
        assert!(PixelShape::merged_face_occludes(
            &top,
            &PixelShape::default(),
            Face::Up
        ));
        assert!(PixelShape::merged_face_occludes(
            &PixelShape::default(),
            &top,
            Face::Down
        ));
    }

    #[test]
    fn a_thin_panel_on_the_north_side_closes_only_that_side() {
        let panel = PixelShape::new(&[[0, 0, 0, 16, 16, 3]]);
        assert!(PixelShape::merged_face_occludes(
            &panel,
            &PixelShape::default(),
            Face::North
        ));
        assert!(!PixelShape::merged_face_occludes(
            &panel,
            &PixelShape::default(),
            Face::South
        ));
        assert!(!PixelShape::merged_face_occludes(
            &panel,
            &PixelShape::default(),
            Face::East
        ));
    }
}
