//! `PathfinderCache`: o caminho já achado de um transportador a um lado de um destino, por rede, até a rede mudar (`commit` e
//! `deregister` da `InventoryNetwork`) ou a cor de um transportador mudar (`LogisticalTransporter.onConfigure`).
//!
//! No original é todo estático; aqui é um objeto que o dono das redes guarda. O mapa de redes nunca é percorrido, então a ordem das
//! tabelas do Java não aparece.
use crate::sides::Face;
use crate::sim::chunks::BlockPos;
use crate::transmission::Uuid;
use std::cell::RefCell;
use std::collections::HashMap;

/// `CachedPath`: o caminho (do destino ao transportador de partida) e o custo dele.
#[derive(Clone, Debug, PartialEq)]
pub struct CachedPath {
    pub path: Vec<i64>,
    pub cost: f64,
}

/// `PathData`: de que transportador, até que destino e por que lado dele.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PathData {
    start: BlockPos,
    end: BlockPos,
    end_side: Option<u8>,
}

fn key(uuid: Uuid) -> (i64, i64) {
    (uuid.most_significant, uuid.least_significant)
}

#[derive(Default)]
pub struct PathfinderCache {
    cached_paths: RefCell<HashMap<(i64, i64), HashMap<PathData, CachedPath>>>,
}

impl PathfinderCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// `onChanged(networks)`: esquece os caminhos da rede.
    pub fn on_changed(&self, network: Uuid) {
        self.cached_paths.borrow_mut().remove(&key(network));
    }

    /// `addCachedPath(start, destination, pathfinder)`: guarda o caminho que a busca achou e o devolve.
    pub fn add_cached_path(
        &self,
        network: Uuid,
        start: BlockPos,
        destination: BlockPos,
        side: Option<Face>,
        path: Vec<i64>,
        final_score: f64,
    ) -> CachedPath {
        let cached = CachedPath {
            path,
            cost: final_score,
        };
        let data = PathData {
            start,
            end: destination,
            end_side: side.map(|side| super::world::side_index(side) as u8),
        };
        self.cached_paths
            .borrow_mut()
            .entry(key(network))
            .or_default()
            .insert(data, cached.clone());
        cached
    }

    /// `getCache(network, start, end, sides)`: o mais barato dos caminhos guardados para os lados dados (o primeiro, num empate).
    pub fn get_cache(
        &self,
        network: Uuid,
        start: BlockPos,
        end: BlockPos,
        sides: &[Face],
    ) -> Option<CachedPath> {
        let paths = self.cached_paths.borrow();
        let path_map = paths.get(&key(network))?;
        let mut ret: Option<&CachedPath> = None;
        for side in sides {
            let data = PathData {
                start,
                end,
                end_side: Some(super::world::side_index(*side) as u8),
            };
            if let Some(test) = path_map.get(&data)
                && ret.is_none_or(|best| test.cost < best.cost)
            {
                ret = Some(test);
            }
        }
        ret.cloned()
    }

    /// `getSingleCache(start, end, side)`.
    pub fn get_single_cache(
        &self,
        network: Uuid,
        start: BlockPos,
        end: BlockPos,
        side: Face,
    ) -> Option<CachedPath> {
        let data = PathData {
            start,
            end,
            end_side: Some(super::world::side_index(side) as u8),
        };
        self.cached_paths
            .borrow()
            .get(&key(network))
            .and_then(|paths| paths.get(&data))
            .cloned()
    }

    /// `reset()`.
    pub fn reset(&self) {
        self.cached_paths.borrow_mut().clear();
    }
}
