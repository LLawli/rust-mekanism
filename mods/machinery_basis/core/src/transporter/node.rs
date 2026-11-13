//! `LogisticalTransporterBase` e as três classes de transportador (`LogisticalTransporter`, `RestrictiveTransporter`,
//! `DiversionTransporter`), no que a busca de caminho lê: a cor, o custo, o que cada lado deixa passar e o modo do desviador contra o
//! sinal de redstone. O estado de conexão é o [`Transmitter`] da `rede-conexao`.
use super::movement::{DroppedItems, TransitState, WorldDrop};
use super::world::Transporter;
use crate::item_handler::ItemHandler;
use crate::sides::Face;
use crate::sim::chunks::BlockPos;
use crate::transaction::JournalCell;
use crate::transmission::connection::{ConnectionType, TransmissionSet, TransmissionType};
use crate::transmission::transmitter::{NeighborTransmitter, Transmitter, TransmitterWorld};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicI32, Ordering};

/// `TransporterTier`: a velocidade e a quantidade que puxa por vez (os valores de fábrica, sem a configuração).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransporterTier {
    Basic,
    Advanced,
    Elite,
    Ultimate,
}

/// `TransporterTier.getSpeed` e `getPullAmount` de cada tier: o valor do arquivo `tiers.toml` (`TierConfig`), que a cola põe quando o arquivo carrega; os padrões antes disso.
static SPEEDS: [AtomicI32; 4] = [
    AtomicI32::new(5),
    AtomicI32::new(10),
    AtomicI32::new(20),
    AtomicI32::new(50),
];
static PULL_AMOUNTS: [AtomicI32; 4] = [
    AtomicI32::new(1),
    AtomicI32::new(16),
    AtomicI32::new(32),
    AtomicI32::new(64),
];

impl TransporterTier {
    /// Os valores do arquivo de configuração, na ordem dos tiers: a velocidade e a quantidade que puxa por vez.
    pub fn configure(speeds: [i32; 4], pull_amounts: [i32; 4]) {
        for (slot, value) in SPEEDS.iter().zip(speeds) {
            slot.store(value, Ordering::Relaxed);
        }
        for (slot, value) in PULL_AMOUNTS.iter().zip(pull_amounts) {
            slot.store(value, Ordering::Relaxed);
        }
    }

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
        SPEEDS[self as usize].load(Ordering::Relaxed)
    }

    /// `getPullAmount`.
    pub fn pull_amount(self) -> i32 {
        PULL_AMOUNTS[self as usize].load(Ordering::Relaxed)
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

    /// `getNext()` do `IIncrementalEnum`: o seguinte, dando a volta.
    pub fn next(self) -> DiversionControl {
        DiversionControl::by_id((self.ordinal() + 1) % 3)
    }
}

/// `TransporterUtils.increment(color)`: da cor nenhuma à primeira, de cada uma à seguinte e da última a nenhuma.
pub fn increment_color(color: Option<u8>) -> Option<u8> {
    match color {
        None => Some(0),
        Some(color) if usize::from(color) == crate::side_config::ENUM_COLOR_NAMES.len() - 1 => None,
        Some(color) => Some(color + 1),
    }
}

/// `TransporterUtils.decrement(color)`.
pub fn decrement_color(color: Option<u8>) -> Option<u8> {
    match color {
        None => Some((crate::side_config::ENUM_COLOR_NAMES.len() - 1) as u8),
        Some(0) => None,
        Some(color) => Some(color - 1),
    }
}

/// A capability de item que o bloco passa a expor ou deixa de expor depois de uma mudança do desviador.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityChange {
    /// `invalidateCapabilities()`: passou a expor (avisa quem escuta a posição).
    Exposed,
    /// `invalidateCapability(ITEM, side)`: deixou de expor o lado.
    Hidden(Face),
}

/// O que o `onNeighborBlockChange` do desviador pede a quem guarda o bloco.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiversionNeighbor {
    /// O sinal mudou: `current = getAllCurrentConnections(); refreshConnections();` e, se as ligações mudaram, `markDirtyTransmitters()`;
    /// depois as mudanças de [`LogisticalTransporterBase::capability_changes_after_power`].
    PowerChanged,
    /// O sinal é o mesmo: `refreshConnections()` (sem lado) ou `refreshConnections(side)`.
    Refresh(Option<Face>),
}

/// O que o `updateMode` do desviador pede depois de trocar o modo de um lado.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModeUpdate {
    /// A capability do lado mudou de exposta para escondida ou o contrário.
    pub capability: Option<CapabilityChange>,
    // Depois: `markForSave`, `refreshConnections()`, `notifyTileChange()` e `sendUpdatePacket()`, nessa ordem.
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
    /// `DiversionTransporter.wasGettingPower`: nulo até o primeiro `onNeighborBlockChange`.
    was_getting_power: std::cell::Cell<Option<bool>>,
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
            was_getting_power: std::cell::Cell::new(None),
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

    /// `DiversionTransporter.onNeighborBlockChange(side)`: lê o sinal e diz o que o dono do bloco faz em seguida. Só vale para o desviador.
    pub fn diversion_neighbor_change(&self, side: Option<Face>) -> DiversionNeighbor {
        let receiving_power = (self.powered)();
        // Inicia o `wasGettingPower` só aqui: na primeira vez conta como mudança, mesmo que o sinal seja o mesmo.
        if self.was_getting_power.get() != Some(receiving_power) {
            self.was_getting_power.set(Some(receiving_power));
            DiversionNeighbor::PowerChanged
        } else {
            DiversionNeighbor::Refresh(side)
        }
    }

    /// As mudanças de capability de item depois do sinal mudar e as ligações serem refeitas, lado a lado: o que a base exporia
    /// (`super.exposesInsertCap`) e o modo do lado agora aceita é anunciado; o que não aceita mais é escondido.
    pub fn capability_changes_after_power(&self) -> Vec<CapabilityChange> {
        Face::ALL
            .into_iter()
            .filter(|side| self.transmitter.get_connection_type_raw(*side).can_accept())
            .map(|side| {
                if self.mode_reqs_met(side) {
                    CapabilityChange::Exposed
                } else {
                    CapabilityChange::Hidden(side)
                }
            })
            .collect()
    }

    /// `DiversionTransporter.updateMode(side, mode)`: troca o modo do lado. Devolve nada se já era esse.
    pub fn update_mode(&mut self, side: Face, mode: DiversionControl) -> Option<ModeUpdate> {
        let powered = (self.powered)();
        let met = |mode: DiversionControl| match mode {
            DiversionControl::High => powered,
            DiversionControl::Low => !powered,
            DiversionControl::Disabled => true,
        };
        let exposes = self.transmitter.get_connection_type_raw(side).can_accept();
        let TransporterKind::Diversion { modes } = &mut self.kind else {
            return None;
        };
        let index = super::world::side_index(side);
        let old = modes[index];
        if old == mode {
            return None;
        }
        modes[index] = mode;
        let mut capability = None;
        if exposes {
            let now_exposes = met(mode);
            if now_exposes != met(old) {
                capability = Some(if now_exposes {
                    CapabilityChange::Exposed
                } else {
                    CapabilityChange::Hidden(side)
                });
            }
        }
        Some(ModeUpdate { capability })
    }

    /// `DiversionTransporter.onRightClick`: o próximo modo do lado.
    pub fn next_mode(&self, side: Face) -> Option<DiversionControl> {
        match &self.kind {
            TransporterKind::Diversion { modes } => {
                Some(modes[super::world::side_index(side)].next())
            }
            _ => None,
        }
    }

    /// `LogisticalTransporter.onConfigure`: a cor seguinte. O dono do bloco esvazia a cache de caminhos da rede
    /// (`PathfinderCache.onChanged`), marca para salvar, avisa os vizinhos e manda o pacote.
    pub fn configure_color(&mut self) -> Option<Option<u8>> {
        match &mut self.kind {
            TransporterKind::Logistical { color } => {
                *color = increment_color(*color);
                Some(*color)
            }
            _ => None,
        }
    }

    /// `output.storeNullable(COLOR, EnumColor.CODEC, getColor())`: o nome da cor, ou nada.
    pub fn write_color(&self) -> Option<super::nbt::Nbt> {
        self.get_color().map(|color| {
            super::nbt::Nbt::String(crate::side_config::ENUM_COLOR_NAMES[usize::from(color)].into())
        })
    }

    /// `setColor(input.read(COLOR, EnumColor.CODEC).orElse(null))`. Devolve se a cor mudou (o `handleUpdateTag` pede novo modelo).
    pub fn read_color(&mut self, tag: Option<&super::nbt::Nbt>) -> bool {
        let color = tag
            .and_then(super::nbt::Nbt::as_str)
            .and_then(|name| {
                crate::side_config::ENUM_COLOR_NAMES
                    .iter()
                    .position(|candidate| *candidate == name)
            })
            .map(|index| index as u8);
        match &mut self.kind {
            TransporterKind::Logistical { color: current } => {
                let changed = *current != color;
                *current = color;
                changed
            }
            _ => false,
        }
    }

    /// `writeModes(output)`: o `mode` do desviador, um inteiro por lado.
    pub fn write_modes(&self) -> Option<super::nbt::Nbt> {
        match &self.kind {
            TransporterKind::Diversion { modes } => Some(super::nbt::Nbt::IntArray(
                modes.iter().map(|mode| mode.ordinal()).collect(),
            )),
            _ => None,
        }
    }

    /// `readModes(input)`: sem a chave todos voltam a desligados; com ela, um por lado até onde o vetor vai.
    pub fn read_modes(&mut self, tag: Option<&super::nbt::Nbt>) {
        let TransporterKind::Diversion { modes } = &mut self.kind else {
            return;
        };
        match tag.and_then(super::nbt::Nbt::as_int_array) {
            Some(indices) => {
                for (mode, index) in modes.iter_mut().zip(indices) {
                    *mode = DiversionControl::by_id(*index);
                }
            }
            None => modes.fill(DiversionControl::Disabled),
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
