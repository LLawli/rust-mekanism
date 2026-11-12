//! `LogisticalTransporterBase` e as três classes de transportador (`LogisticalTransporter`, `RestrictiveTransporter`,
//! `DiversionTransporter`), no que a busca de caminho lê: a cor, o custo, o que cada lado deixa passar e o modo do desviador contra o
//! sinal de redstone. O estado de conexão é o [`Transmitter`] da `rede-conexao`.
use super::world::Transporter;
use crate::item_handler::ItemHandler;
use crate::sides::Face;
use crate::sim::chunks::BlockPos;
use crate::transmission::connection::{ConnectionType, TransmissionSet, TransmissionType};
use crate::transmission::transmitter::{NeighborTransmitter, Transmitter, TransmitterWorld};
use super::movement::{DroppedItems, TransitState, WorldDrop};
use crate::transaction::JournalCell;
use std::cell::RefCell;
use std::rc::Rc;

/// `TransporterTier`: a velocidade e a quantidade que puxa por vez (os valores de fábrica, sem a configuração).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransporterTier {
    Basic,
    Advanced,
    Elite,
    Ultimate,
}

impl TransporterTier {
    pub const VALUES: [TransporterTier; 4] = [
        TransporterTier::Basic,
        TransporterTier::Advanced,
        TransporterTier::Elite,
        TransporterTier::Ultimate,
    ];

    /// `BY_ID` (`OutOfBoundsStrategy.ZERO`).
    pub fn by_id(id: i32) -> TransporterTier {
        usize::try_from(id)
            .ok()
            .and_then(|index| Self::VALUES.get(index).copied())
            .unwrap_or(TransporterTier::Basic)
    }

    pub fn ordinal(self) -> i32 {
        self as i32
    }

    /// `getSpeed`.
    pub fn speed(self) -> i32 {
        match self {
            TransporterTier::Basic => 5,
            TransporterTier::Advanced => 10,
            TransporterTier::Elite => 20,
            TransporterTier::Ultimate => 50,
        }
    }

    /// `getPullAmount`.
    pub fn pull_amount(self) -> i32 {
        match self {
            TransporterTier::Basic => 1,
            TransporterTier::Advanced => 16,
            TransporterTier::Elite => 32,
            TransporterTier::Ultimate => 64,
        }
    }
}

/// `DiversionTransporter.DiversionControl`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiversionControl {
    Disabled,
    High,
    Low,
}

impl DiversionControl {
    pub fn by_id(id: i32) -> DiversionControl {
        match id {
            1 => DiversionControl::High,
            2 => DiversionControl::Low,
            _ => DiversionControl::Disabled,
        }
    }

    pub fn ordinal(self) -> i32 {
        self as i32
    }
}

/// O que muda de uma classe de transportador para outra.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransporterKind {
    /// `LogisticalTransporter`: o tier e a cor.
    Logistical { color: Option<u8> },
    /// `RestrictiveTransporter`: tier básico e custo 1000.
    Restrictive,
    /// `DiversionTransporter`: tier básico e um modo por lado.
    Diversion { modes: [DiversionControl; 6] },
}

/// Um mundo que nunca é consultado: o transportador não trata redstone (`handlesRedstone() == false`), então o `Transmitter` não lê o
/// mundo para saber se pode ligar.
struct NoRedstone;

impl TransmitterWorld for NoRedstone {
    type Acceptor = Rc<dyn ItemHandler>;

    fn is_block_loaded(&self, _pos: [i32; 3]) -> bool {
        unreachable!("o transportador não lê o mundo para ligar")
    }

    fn is_getting_powered(&self, _pos: [i32; 3]) -> bool {
        unreachable!("o transportador não lê o redstone")
    }

    fn transmitter_at(&self, _pos: [i32; 3], _side: Face) -> Option<NeighborTransmitter> {
        unreachable!("o transportador não lê o mundo para ligar")
    }

    fn acceptor(&self, _pos: [i32; 3], _from_side: Face) -> Option<Self::Acceptor> {
        unreachable!("o transportador não lê o mundo para ligar")
    }
}

/// `LogisticalTransporterBase`.
pub struct LogisticalTransporterBase {
    pub transmitter: Transmitter<Rc<dyn ItemHandler>>,
    pub tier: TransporterTier,
    pub kind: TransporterKind,
    /// `WorldUtils.isGettingPowered(level, pos)`: lido a cada vez, como o do original.
    powered: Rc<dyn Fn() -> bool>,
    pos: BlockPos,
    /// `transit`, `needsSync`, `nextId`, `delay` e `delayCount` (`movement.rs`).
    pub(super) transit: Rc<RefCell<TransitState>>,
    pub(super) dropped: Rc<JournalCell<DroppedItems>>,
    pub(super) world_drops: Rc<RefCell<Vec<WorldDrop>>>,
}

impl LogisticalTransporterBase {
    /// `LogisticalTransporter`, `RestrictiveTransporter` (tier básico) ou `DiversionTransporter` (tier básico).
    pub fn new(
        pos: BlockPos,
        tier: TransporterTier,
        kind: TransporterKind,
        powered: Rc<dyn Fn() -> bool>,
    ) -> Self {
        let tier = match kind {
            TransporterKind::Logistical { .. } => tier,
            TransporterKind::Restrictive | TransporterKind::Diversion { .. } => {
                TransporterTier::Basic
            }
        };
        let world_drops = Rc::new(RefCell::new(Vec::new()));
        Self {
            dropped: DroppedItems::new(pos, world_drops.clone()),
            world_drops,
            transit: Rc::new(RefCell::new(TransitState::default())),
            pos,
            transmitter: Transmitter::new(
                [pos.x, pos.y, pos.z],
                TransmissionSet::of(&[TransmissionType::Item]),
                false,
            ),
            tier,
            kind,
            powered,
        }
    }

    /// `getBlockPos()`.
    pub fn block_pos(&self) -> BlockPos {
        self.pos
    }

    /// `getColor`.
    pub fn get_color(&self) -> Option<u8> {
        match &self.kind {
            TransporterKind::Logistical { color } => *color,
            _ => None,
        }
    }

    /// `LogisticalTransporter.setColor`. Quem muda a cor esvazia a cache de caminhos da rede (`PathfinderCache.onChanged`).
    pub fn set_color(&mut self, color: Option<u8>) {
        if let TransporterKind::Logistical { color: current } = &mut self.kind {
            *current = color;
        }
    }

    /// `getCost`: `ULTIMATE.getSpeed() / (double) tier.getSpeed()`, e 1000 no restritivo.
    pub fn get_cost(&self) -> f64 {
        if self.kind == TransporterKind::Restrictive {
            return 1_000.0;
        }
        f64::from(TransporterTier::Ultimate.speed()) / f64::from(self.tier.speed())
    }

    /// `modeReqsMet(side)`: o modo do lado contra o sinal. Só o desviador tem modo.
    pub fn mode_reqs_met(&self, side: Face) -> bool {
        match &self.kind {
            TransporterKind::Diversion { modes } => match modes[super::world::side_index(side)] {
                DiversionControl::High => (self.powered)(),
                DiversionControl::Low => !(self.powered)(),
                DiversionControl::Disabled => true,
            },
            _ => true,
        }
    }

    /// `exposesInsertCap(side)`: `getConnectionTypeRaw(side).canAccept()`, e no desviador o modo também.
    pub fn exposes_insert_cap(&self, side: Face) -> bool {
        self.transmitter.get_connection_type_raw(side).can_accept() && self.mode_reqs_met(side)
    }

    /// `isValidTransmitterBasic(transmitter, side)` com um transportador do outro lado: as cores não se contradizem e os dois lados ligam
    /// (`canConnectMutual`, com o modo do desviador).
    pub fn is_valid_transmitter_basic(
        &self,
        other: &LogisticalTransporterBase,
        side: Face,
    ) -> bool {
        self.colors_allow(true, other.get_color())
            && Transporter::can_connect_mutual(self, side, Some(other))
    }

    /// `isValidTransmitterBasic(transmitter, side)`, a parte das cores: o outro é um transportador e as cores não se contradizem (uma
    /// delas nula, ou iguais). O resto (tipos e ligação dos dois lados) é a regra básica do [`Transmitter`].
    pub fn colors_allow(&self, other_is_transporter: bool, other_color: Option<u8>) -> bool {
        other_is_transporter
            && (self.get_color().is_none()
                || other_color.is_none()
                || self.get_color() == other_color)
    }
}

impl Transporter for LogisticalTransporterBase {
    fn as_logistical(&self) -> Option<&LogisticalTransporterBase> {
        Some(self)
    }

    fn color(&self) -> Option<u8> {
        self.get_color()
    }

    fn cost(&self) -> f64 {
        self.get_cost()
    }

    fn can_connect(&self, side: Face) -> bool {
        self.transmitter.can_connect(side, &NoRedstone) && self.mode_reqs_met(side)
    }

    fn connection_type(&self, side: Face) -> ConnectionType {
        self.transmitter.get_connection_type(side)
    }
}
