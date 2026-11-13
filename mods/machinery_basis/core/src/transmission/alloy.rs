//! O upgrade de tier no lugar (`ItemAlloy` e `TileEntityTransmitter.onAlloyInteraction`): quais transmissores da rede sobem, em que ordem e quantos.
//!
//! A liga clicada num transmissor com rede sobe os da rede que o `IUpgradeableTransmitter.canUpgrade` aceita (o nível da liga é o do
//! transmissor mais um), do mais perto do clique para o mais longe (`Comparator.comparingDouble(distSqr)`, estável), até 8. Cada um leva o
//! dado de upgrade (`getUpgradeData`) ao bloco novo (`parseUpgradeData`), e no fim a rede velha se invalida. Os donos de transmissores
//! montam a lista ([`AlloyRun`]) e a cola do jogo troca os blocos entre `alloy_take` e `alloy_give`.
use super::NetworkId;
use super::connection::ConnectionType;
use crate::sim::chunks::BlockPos;

/// `upgraded == 8` quebra o laço de `onAlloyInteraction`.
pub const MAX_UPGRADES: u32 = 8;

/// `IUpgradeableTransmitter.canUpgrade(alloyTier)`: `alloyTier.getBaseTierLevel() == getTier().getBaseTierLevel() + 1`.
pub fn can_upgrade(alloy_level: i32, tier_level: i32) -> bool {
    alloy_level == tier_level + 1
}

/// `BlockPos.distSqr(Vec3i)`: a distância ao quadrado, em `double`.
pub fn dist_sqr(a: BlockPos, b: BlockPos) -> f64 {
    let dx = f64::from(a.x) - f64::from(b.x);
    let dy = f64::from(a.y) - f64::from(b.y);
    let dz = f64::from(a.z) - f64::from(b.z);
    dx * dx + dy * dy + dz * dz
}

/// `list.sort(Comparator.comparingDouble(transmitter -> transmitter.getBlockPos().distSqr(pos)))`: a ordenação do Java é estável.
pub fn order_by_distance(origin: BlockPos, mut list: Vec<BlockPos>) -> Vec<BlockPos> {
    list.sort_by(|a, b| dist_sqr(*a, origin).total_cmp(&dist_sqr(*b, origin)));
    list
}

/// Um clique de liga em andamento: a rede, os transmissores que sobem (em ordem) e o que o laço já fez.
#[derive(Clone, Debug)]
pub struct AlloyRun {
    pub network: NetworkId,
    pub targets: Vec<BlockPos>,
    /// `sharesSet`: a primeira troca valida as shares da rede.
    pub shares_set: bool,
    /// `upgraded`.
    pub upgraded: u32,
}

impl AlloyRun {
    /// O laço parou em `upgraded == 8`.
    pub fn full(&self) -> bool {
        self.upgraded >= MAX_UPGRADES
    }

    /// `upgraded > 0`: a rede se invalida, a liga se gasta e o avanço dispara.
    pub fn did_upgrade(&self) -> bool {
        self.upgraded > 0
    }
}

/// O que o cabo velho leva ao novo (`UniversalCableUpgradeData`): o redstone, os tipos de conexão crus e o buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CableUpgrade {
    pub redstone_reactive: bool,
    pub connection_types: [ConnectionType; 6],
    pub buffer: i64,
}

/// O que o cano ou o tubo velho leva ao novo (`ResourceTransmitterUpgradeData`).
#[derive(Clone, Debug, PartialEq)]
pub struct ResourceUpgrade<C> {
    pub redstone_reactive: bool,
    pub connection_types: [ConnectionType; 6],
    pub buffer: C,
}
