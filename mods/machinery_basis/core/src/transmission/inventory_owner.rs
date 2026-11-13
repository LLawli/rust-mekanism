//! O dono dos transportadores logísticos, irmão de [`super::owner::EnergyTransmitters`] e de [`super::resource_owner::ResourceTransmitters`]:
//! liga o `LogisticalTransporterBase` (o `Transmitter` de conexões por lado, a cor, o modo do desviador e as pilhas em trânsito), o registro de
//! redes (a `InventoryNetwork`) e o gerenciador das pilhas em voo ao que o jogo entrega (`TileEntityLogisticalTransporterBase`,
//! `BlockTransmitter.neighborChanged`, o `RefreshListener` do `AcceptorCache`, o `ServerTickEvent.Post`). Os eventos são os mesmos do dono de energia
//! ([`Event`], decisão 107) e a lib continua sem conhecer id: o jogo (ou o mundo simulado de um teste) entra por [`InventoryOwnerWorld`].
//!
//! O que difere dos outros donos, e que vem do Java:
//!
//! - `TileEntityLogisticalTransporterBase.onUpdateServer` é `super.onUpdateServer` (entrar na rede, `forceUpdate`) e depois o movimento das pilhas
//!   (`LogisticalTransporterBase.onUpdateServer`: puxar a cada tantos ticks e andar as pilhas em trânsito);
//! - `canConnect(side)` do desviador soma o modo do lado contra o sinal de redstone (o gancho [`TransmitterWorld::extra_can_connect`]) e
//!   `isValidTransmitterBasic` a cor (as cores não se contradizem);
//! - `preRemoveSideEffects` solta as pilhas em trânsito quando o bloco é quebrado (não quando o chunk descarrega) e `remove()` as tira das em voo;
//! - o NBT leva o `Transmitter`, a cor (ou o `mode` do desviador) e a lista `items` das pilhas em trânsito;
//! - a capability de item é o `TransporterItemHandler`, que existe nos lados em que `exposesInsertCap` vale; inserir por ele é [`InventoryTransmitters::insert`].
use super::alloy::{AlloyRun, can_upgrade, order_by_distance};
use super::connection::{
    ConnectionType, TransmissionSet, TransmissionType, connection_map_contains_side,
};
use super::inventory_network::{RegistryNetworkView, TransporterLookup, on_network_event};
use super::owner::{CableView, Event};
use super::registry::{TransmitterNetworkRegistry, set_transmitter_network};
use super::transmitter::{
    Compound, Effect, InteractionResult, NeighborTransmitter, Overlay, Tag, Transmitter,
    TransmitterWorld,
};
use super::validator::{AlwaysCompatible, CompatibleTransmitterValidator};
use super::{DimensionId, NetworkEvent, NetworkId, TransmitterHost, TransmitterId, Uuid};
use crate::containers::ItemResource;
use crate::item_handler::ItemHandler;
use crate::sides::Face;
use crate::sim::chunks::{BlockPos, MAX_VIEW_DISTANCE};
use crate::transaction::Transaction;
use crate::transporter::movement::{MoveContext, TransitEvent, TransitUpgrade, WorldDrop};
use crate::transporter::nbt::Nbt;
use crate::transporter::node::{
    CapabilityChange, DiversionControl, DiversionNeighbor, LogisticalTransporterBase,
    TransporterKind, TransporterTier,
};
use crate::transporter::pathfinder::PathContext;
use crate::transporter::pos::{block_pos_of, side_difference};
use crate::transporter::stack::{ItemDecode, ItemEncode, TransporterStack, plain_item_encode};
use crate::transporter::world::{
    InventoryNetworkView, SideConfigView, TransportSorter, Transporter, TransporterWorld,
};
use crate::transporter::{PathfinderCache, TransporterManager};
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

/// A capability de item de um bloco vizinho (`ResourceHandler<ItemResource>`).
pub type ItemAcceptor = Rc<dyn ItemHandler>;
type Reg = TransmitterNetworkRegistry<ItemAcceptor>;

/// As chaves de NBT do transportador (`SerializationConstants.COLOR`, `MODE` e `ITEMS`).
pub mod keys {
    pub const COLOR: &str = "color";
    pub const MODE: &str = "mode";
    pub const ITEMS: &str = "items";
}

/// O NBT do transportador (`transporter::nbt`) no `Tag` do pacote de atualização do `Transmitter`.
fn nbt_to_tag(nbt: &Nbt) -> Tag {
    match nbt {
        Nbt::Byte(v) => Tag::Byte(*v),
        Nbt::Short(v) => Tag::Short(*v),
        Nbt::Int(v) => Tag::Int(*v),
        Nbt::Long(v) => Tag::Long(*v),
        Nbt::Float(bits) => Tag::Float(f32::from_bits(*bits)),
        Nbt::Double(bits) => Tag::Double(f64::from_bits(*bits)),
        Nbt::String(v) => Tag::String(v.clone()),
        Nbt::ByteArray(v) => Tag::ByteArray(v.iter().map(|byte| *byte as i8).collect()),
        Nbt::IntArray(v) => Tag::IntArray(v.clone()),
        Nbt::LongArray(v) => Tag::LongArray(v.clone()),
        Nbt::List(items) => Tag::List(items.iter().map(nbt_to_tag).collect()),
        Nbt::Compound(entries) => Tag::Compound(
            entries
                .iter()
                .map(|(key, value)| (key.clone(), nbt_to_tag(value)))
                .collect(),
        ),
    }
}

fn arr(pos: BlockPos) -> [i32; 3] {
    [pos.x, pos.y, pos.z]
}

fn bp(pos: [i32; 3]) -> BlockPos {
    BlockPos::new(pos[0], pos[1], pos[2])
}

/// O que `Game::move_in_item` recebe de volta de um transportador (`tryMoveInItem`): o que sobra da pilha oferecida e se algo entrou.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HopperOutcome {
    /// Quanto da pilha oferecida ficou com quem a pôs (um funil, um dropper, o ejetor de outra máquina).
    pub rest: u32,
    /// Se `TransporterItemHandler.insert` aceitou alguma coisa (`inserted > 0`).
    pub moved: bool,
}

/// A conta de `hopper_insert` (o `TransporterItemHandler.insert` para quem oferece `offered` itens): `inserted` é o que `InventoryTransmitters::insert` devolveu, `None` se o lado não expõe a
/// capability (ou o funil não diz de que lado vem). O que não entra volta inteiro, e nunca volta mais do que foi oferecido.
pub fn hopper_outcome(offered: u32, inserted: Option<u32>) -> HopperOutcome {
    let inserted = inserted.unwrap_or(0).min(offered);
    HopperOutcome {
        rest: offered - inserted,
        moved: inserted > 0,
    }
}

/// O que o block entity guardou de um transportador (`loadAdditional`), se veio de um save.
#[derive(Default)]
pub struct SavedTransporter {
    /// O `Transmitter` (`TileEntityTransmitter.saveAdditional`): redstone, ligações e tipos de conexão.
    pub tag: Compound,
    /// A cor (`COLOR`), se tem.
    pub color: Option<Nbt>,
    /// O `mode` do desviador.
    pub modes: Option<Nbt>,
    /// A lista `items` das pilhas em trânsito.
    pub items: Option<Nbt>,
}

/// `LogisticalTransporterUpgradeData`: o que o transportador velho leva ao novo (o redstone, os tipos de conexão crus, a cor e as pilhas em trânsito com os contadores).
pub struct TransporterUpgrade {
    pub redstone_reactive: bool,
    pub connection_types: [ConnectionType; 6],
    pub color: Option<u8>,
    pub transit: TransitUpgrade,
}

/// O que o jogo diz de um transportador novo.
pub struct TransporterInfo {
    pub tier: TransporterTier,
    pub kind: TransporterKind,
    pub saved: Option<SavedTransporter>,
}

/// O mundo, como o dono o vê. O que o `Transmitter` e o movimento leem mais o que o dono manda de volta.
pub trait InventoryOwnerWorld {
    /// `WorldUtils.isBlockLoaded`.
    fn is_block_loaded(&self, pos: BlockPos) -> bool;
    /// `WorldUtils.isGettingPowered`.
    fn is_getting_powered(&self, pos: BlockPos) -> bool;
    /// `Capabilities.ITEM` do bloco em `pos` pelo lado `side` dele (`BlockCapabilityCache.getCapability`, `getCapabilityIfLoaded`), `None` se não há. O dono nunca pergunta por um
    /// transportador.
    fn item_handler(&self, pos: BlockPos, side: Face) -> Option<ItemAcceptor>;
    /// O bloco em `pos` é um `ISideConfiguration`?
    fn side_configuration(&self, _pos: i64) -> Option<SideConfigView> {
        None
    }
    /// O bloco em `pos` é um `IAdvancedTransportEjector`?
    fn sorter(&self, _pos: i64) -> Option<&dyn TransportSorter> {
        None
    }
    /// O transportador que o bloco em `pos` é, ou `None` se o bloco não é um dos nossos.
    fn transporter_info(&mut self, pos: BlockPos) -> Option<TransporterInfo>;
    /// `ItemResource.CODEC` ao ler.
    fn decode_item(&self, tag: &crate::transporter::nbt::Compound) -> Option<ItemResource>;
    /// `ItemResource.CODEC` ao escrever.
    fn encode_item(&self, item: &ItemResource) -> crate::transporter::nbt::Compound {
        plain_item_encode(item)
    }
    /// `sendUpdatePacket`: as conexões do transportador em `pos` mudaram.
    fn publish(&mut self, pos: BlockPos, view: &CableView);
    /// `BlockEntity.setChanged` / `markForSave`.
    fn set_changed(&mut self, _pos: BlockPos) {}
    /// `player.sendOverlayMessage`.
    fn overlay(&mut self, _pos: BlockPos, _overlay: Overlay) {}
    /// `invalidateCapabilities()` / `invalidateCapability(ITEM, side)` de um desviador.
    fn capability_changed(&mut self, _pos: BlockPos, _change: CapabilityChange) {}
    /// Um `Block.popResource`: o que o `drop` do transportador soltou no mundo.
    fn world_drop(&mut self, _drop: WorldDrop) {}
    /// `PacketUtils.sendToAllTracking(PacketTransporterSync | PacketTransporterBatch, tile)`: o pacote de movimento das pilhas do transportador em `pos`, com o valor da hora em que o
    /// original o mandou.
    fn transporter_packet(&mut self, _pos: BlockPos, _packet: TransporterPacket) {}
}

/// `PacketTransporterSync` e `PacketTransporterBatch`, como o cliente os lê: cada pilha é o compound do `writeToUpdateTag` (`color`, `progress`, `original_location`, `item`,
/// `path_type`, `next`, `previous`), que é o conteúdo do `TransporterStack.STREAM_CODEC`.
#[derive(Clone, Debug, PartialEq)]
pub enum TransporterPacket {
    /// Uma pilha nova em trânsito (`emitStack`).
    Sync { id: i32, stack: Compound },
    /// As pilhas que saíram do transportador e as que mudaram, no fim do tick dele.
    Batch {
        deletes: Vec<i32>,
        updates: Vec<(i32, Compound)>,
    },
}

/// O que a rede fez, na ordem, para um teste comparar (só se o dono grava: [`InventoryTransmitters::record_events`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TraceEvent {
    Commit(NetworkId),
    Invalidate(NetworkId),
    Created {
        network: NetworkId,
        hash: i32,
        from: Vec<NetworkId>,
    },
}

struct Slot {
    pos: BlockPos,
    base: LogisticalTransporterBase,
    /// `System.identityHashCode`.
    hash: i32,
    /// `TileEntityTransmitter.markJoined`.
    mark_joined: bool,
    /// A rede do registro (o `Transmitter` guarda só o UUID dela) e o órfão: cópias, para ler com o transmissor fora do lugar.
    network: Option<NetworkId>,
    orphan: bool,
    /// O `Transmitter` está fora do lugar (uma operação em andamento): o mundo vê os outros.
    out: bool,
    /// O transmissor saiu do mundo (`setRemoved`): o registro ainda o conhece até o próximo tick.
    dead: bool,
    /// `onChunkUnloaded` já passou: o `setRemoved` que vem é do chunk, e não de um bloco quebrado.
    unloaded: bool,
    /// `WorldUtils.isGettingPowered(level, pos)` do desviador, relido do jogo a cada evento.
    power: Rc<Cell<bool>>,
}

impl Slot {
    /// `hasTransmitterNetwork()`.
    fn net(&self) -> Option<NetworkId> {
        self.network.filter(|_| !self.orphan)
    }
}

struct State {
    dimension: DimensionId,
    slots: Vec<Slot>,
    by_pos: HashMap<BlockPos, TransmitterId>,
    manager: TransporterManager,
    cache: PathfinderCache,
    hashes: Box<dyn FnMut(BlockPos) -> i32>,
    pending_uuid: Option<Uuid>,
    uuids: HashMap<NetworkId, Uuid>,
    /// Efeitos que um método do `TransmitterHost` pediu e que só o dono, com o registro, pode aplicar.
    deferred: Vec<(TransmitterId, Effect)>,
    record: bool,
    events: Vec<TraceEvent>,
}

impl State {
    fn new(dimension: DimensionId, hashes: Box<dyn FnMut(BlockPos) -> i32>) -> Self {
        Self {
            dimension,
            slots: Vec::new(),
            by_pos: HashMap::new(),
            manager: TransporterManager::new(),
            cache: PathfinderCache::new(),
            hashes,
            pending_uuid: None,
            uuids: HashMap::new(),
            deferred: Vec::new(),
            record: false,
            events: Vec::new(),
        }
    }

    fn live(&self, pos: BlockPos) -> Option<TransmitterId> {
        self.by_pos.get(&pos).copied()
    }

    fn tx(&self, id: TransmitterId) -> &Transmitter<ItemAcceptor> {
        &self.slots[id.0 as usize].base.transmitter
    }

    fn uuid_value(&self, network: NetworkId) -> u128 {
        let uuid = self.uuids.get(&network).copied().unwrap_or(Uuid {
            most_significant: 0,
            least_significant: i64::from(network.0),
        });
        (u128::from(uuid.most_significant as u64) << 64) | u128::from(uuid.least_significant as u64)
    }

    fn event(&mut self, event: TraceEvent) {
        if self.record {
            self.events.push(event);
        }
    }
}

/// O que o `Transmitter` enxerga do mundo: os outros transportadores do dono e o jogo.
struct View<'a, W: InventoryOwnerWorld> {
    st: &'a State,
    world: &'a W,
}

impl<W: InventoryOwnerWorld> View<'_, W> {
    fn id_at(&self, pos: BlockPos) -> Option<TransmitterId> {
        if !self.world.is_block_loaded(pos) {
            return None;
        }
        self.st.live(pos)
    }

    /// O transportador no lugar (não o que uma operação em andamento tirou).
    fn slot_at(&self, pos: BlockPos) -> Option<&Slot> {
        let id = self.id_at(pos)?;
        let slot = &self.st.slots[id.0 as usize];
        (!slot.out).then_some(slot)
    }
}

impl<W: InventoryOwnerWorld> TransmitterWorld for View<'_, W> {
    type Acceptor = ItemAcceptor;

    fn is_block_loaded(&self, pos: [i32; 3]) -> bool {
        self.world.is_block_loaded(bp(pos))
    }

    fn is_getting_powered(&self, pos: [i32; 3]) -> bool {
        self.world.is_getting_powered(bp(pos))
    }

    fn transmitter_at(&self, pos: [i32; 3], side: Face) -> Option<NeighborTransmitter> {
        let other = self.slot_at(bp(pos))?;
        Some(NeighborTransmitter {
            supported: other.base.transmitter.supported_transmission_types(),
            can_connect_opposite: other.base.transmitter.can_connect(side.opposite(), self),
        })
    }

    fn acceptor(&self, pos: [i32; 3], from_side: Face) -> Option<ItemAcceptor> {
        let at = bp(pos);
        if self.st.by_pos.contains_key(&at) {
            return None;
        }
        self.world.item_handler(at, from_side)
    }

    /// `LogisticalTransporterBase.isValidTransmitterBasic`, a parte das cores: as cores não se contradizem.
    fn extra_valid_transmitter(&self, me: [i32; 3], other: [i32; 3], _side: Face) -> bool {
        let (Some(a), Some(b)) = (self.id_at(bp(me)), self.id_at(bp(other))) else {
            return true;
        };
        let (a, b) = (&self.st.slots[a.0 as usize], &self.st.slots[b.0 as usize]);
        a.base.colors_allow(true, b.base.get_color())
    }

    /// `DiversionTransporter.canConnect`: o modo do lado contra o sinal.
    fn extra_can_connect(&self, me: [i32; 3], side: Face) -> bool {
        match self.st.live(bp(me)) {
            Some(id) => self.st.slots[id.0 as usize].base.mode_reqs_met(side),
            None => true,
        }
    }
}

/// O `Level` como a busca de caminho o lê: o jogo, mais os transportadores do dono (`neighbor_transmitter_can_connect`).
struct Adaptor<'a, W: InventoryOwnerWorld> {
    st: &'a State,
    world: &'a W,
}

impl<W: InventoryOwnerWorld> TransporterWorld for Adaptor<'_, W> {
    fn side_configuration(&self, pos: i64) -> Option<SideConfigView> {
        self.world.side_configuration(pos)
    }

    fn sorter(&self, pos: i64) -> Option<&dyn TransportSorter> {
        self.world.sorter(pos)
    }

    fn item_handler(&self, pos: i64, side: Face) -> Option<ItemAcceptor> {
        self.world.item_handler(block_pos_of(pos), side)
    }

    fn neighbor_transmitter_can_connect(&self, pos: i64, side: Face) -> Option<bool> {
        let id = self.st.by_pos.get(&block_pos_of(pos))?;
        Some(Transporter::can_connect(
            &self.st.slots[id.0 as usize].base,
            side,
        ))
    }
}

struct Lookup<'a>(&'a State);

impl TransporterLookup for Lookup<'_> {
    fn transporter(&self, id: TransmitterId) -> Option<&dyn Transporter> {
        Some(&self.0.slots[id.0 as usize].base)
    }
}

/// O que o registro enxerga num instante: os transportadores e o jogo.
struct Ctx<'a, W: InventoryOwnerWorld> {
    st: &'a mut State,
    world: &'a mut W,
}

impl<W: InventoryOwnerWorld> Ctx<'_, W> {
    fn view(&self) -> View<'_, W> {
        View {
            st: &*self.st,
            world: &*self.world,
        }
    }

    fn slot(&mut self, id: TransmitterId) -> &mut Slot {
        &mut self.st.slots[id.0 as usize]
    }

    /// Roda uma operação do `Transmitter` com ele fora do lugar (ela lê o mundo, e o mundo guarda o transmissor), e aplica os efeitos que ela
    /// pediu.
    fn with_tr<T>(
        &mut self,
        registry: &mut Reg,
        id: TransmitterId,
        f: impl FnOnce(&mut Transmitter<ItemAcceptor>, &View<'_, W>, &mut Vec<Effect>) -> T,
    ) -> Option<T> {
        let slot = self.slot(id);
        if slot.out {
            return None;
        }
        let placeholder = Transmitter::new(
            arr(slot.pos),
            TransmissionSet::of(&[TransmissionType::Item]),
            false,
        );
        let mut tx = std::mem::replace(&mut slot.base.transmitter, placeholder);
        slot.out = true;
        let mut out = Vec::new();
        let result = f(&mut tx, &self.view(), &mut out);
        let slot = self.slot(id);
        slot.base.transmitter = tx;
        slot.out = false;
        self.apply(registry, id, out);
        Some(result)
    }

    fn apply(&mut self, registry: &mut Reg, id: TransmitterId, effects: Vec<Effect>) {
        let pos = self.st.slots[id.0 as usize].pos;
        for effect in effects {
            match effect {
                Effect::NotifyTileChange => self.notify_neighbors(registry, pos),
                Effect::RefreshNeighbor { pos: other, side } => {
                    if let Some(other) = self.view().id_at(bp(other)) {
                        self.with_tr(registry, other, |t, w, out| {
                            t.refresh_connections_side(side, w, out)
                        });
                    }
                }
                Effect::RecheckConnections(mask) => {
                    // `Transmitter.recheckConnections` (a base, sem fusão manual: o transportador não é um `BufferedTransmitter`).
                    self.with_tr(registry, id, |t, w, out| {
                        t.base_recheck_connections(mask, w, out)
                    });
                }
                Effect::RecheckConnection(side) => {
                    // `Transmitter.recheckConnection(side)` da base: só a rede com o vizinho relê o lado de volta.
                    let pos = self.st.slots[id.0 as usize].pos;
                    if self.st.slots[id.0 as usize].net().is_none()
                        && let Some(other) = self.view().id_at(pos.relative(side))
                    {
                        self.with_tr(registry, other, |t, w, out| {
                            t.refresh_connections_side(side.opposite(), w, out)
                        });
                    }
                }
                Effect::RequestsUpdate => self.publish(id),
                Effect::SendUpdatePacket => self.publish(id),
                Effect::InvalidateTransmitter => {
                    registry.invalidate_transmitter(&*self, id);
                }
                Effect::AcceptorChanged(side) => {
                    if let Some(network) = self.st.slots[id.0 as usize].network {
                        registry.network_acceptor_changed(&*self, network, id, side);
                    }
                }
                Effect::SetForceUpdate => {
                    if self.st.slots[id.0 as usize].out {
                        continue;
                    }
                    self.slot(id).base.transmitter.set_force_update();
                }
                Effect::SetChanged => self.world.set_changed(pos),
                Effect::SideChanged { .. } => {
                    self.publish(id);
                }
                Effect::RedstoneChanged(_) => self.publish(id),
                Effect::Overlay(overlay) => self.world.overlay(pos, overlay),
                Effect::ClientNetworkRemove(_) | Effect::ClientNetworkAdd(_) => {}
            }
        }
    }

    /// `sendUpdatePacket`: o jogo recebe a vista do transmissor. Um transmissor que saiu, ou que está tirado do lugar por uma operação em
    /// andamento, não publica.
    fn publish(&mut self, id: TransmitterId) {
        let slot = &self.st.slots[id.0 as usize];
        if !slot.out && !slot.dead {
            let pos = slot.pos;
            let view = view_of(&self.view(), id);
            self.world.publish(pos, &view);
        }
    }

    /// `Transmitter.notifyTileChange` -> `WorldUtils.notifyLoadedNeighborsOfTileChange`: os vizinhos carregados recebem `neighborChanged` sem orientação, na ordem de
    /// `EnumUtils.DIRECTIONS` (`Direction.values()`).
    fn notify_neighbors(&mut self, registry: &mut Reg, pos: BlockPos) {
        for side in Face::ALL {
            if let Some(id) = self.view().id_at(pos.relative(side)) {
                self.neighbor_changed(registry, id);
            }
        }
    }

    /// `onNeighborBlockChange(null)`: o do desviador relê tudo se o sinal mudou (e avisa a capability); o dos outros relê tudo (sem lado).
    fn neighbor_changed(&mut self, registry: &mut Reg, id: TransmitterId) {
        let is_diversion = matches!(
            self.st.slots[id.0 as usize].base.kind,
            TransporterKind::Diversion { .. }
        );
        if !is_diversion {
            self.with_tr(registry, id, |t, w, out| {
                t.on_neighbor_block_change(None, w, out)
            });
            return;
        }
        let pos = self.st.slots[id.0 as usize].pos;
        self.refresh_power(id);
        match self.st.slots[id.0 as usize]
            .base
            .diversion_neighbor_change(None)
        {
            DiversionNeighbor::PowerChanged => {
                let before = self.st.tx(id).get_all_current_connections();
                self.with_tr(registry, id, |t, w, out| t.refresh_connections(w, out));
                let after = self.st.tx(id).get_all_current_connections();
                if before != after {
                    self.with_tr(registry, id, |t, _, out| t.mark_dirty_transmitters(out));
                }
                let changes = self.st.slots[id.0 as usize]
                    .base
                    .capability_changes_after_power();
                for change in changes {
                    self.world.capability_changed(pos, change);
                }
            }
            DiversionNeighbor::Refresh(side) => {
                self.with_tr(registry, id, |t, w, out| match side {
                    None => t.refresh_connections(w, out),
                    Some(side) => t.refresh_connections_side(side, w, out),
                });
            }
        }
    }

    /// Lê o sinal de redstone do desviador no jogo (`WorldUtils.isGettingPowered`, que o original lê a cada uso).
    fn refresh_power(&mut self, id: TransmitterId) {
        let slot = &self.st.slots[id.0 as usize];
        if matches!(slot.base.kind, TransporterKind::Diversion { .. }) {
            slot.power.set(self.world.is_getting_powered(slot.pos));
        }
    }

    /// `TileEntityTransmitter.onWorldJoin`.
    fn world_join(&mut self, registry: &mut Reg, id: TransmitterId, was_present: bool) {
        if !was_present {
            registry.track_transmitter(&*self, id);
        }
        if !self.st.tx(id).loaded {
            self.slot(id).base.transmitter.loaded = true;
            registry.register_orphan_transmitter(&*self, id);
        }
    }

    /// `TileEntityTransmitter.onWorldSeparate`.
    fn world_separate(&mut self, registry: &mut Reg, id: TransmitterId, still_present: bool) {
        if !still_present {
            registry.untrack_transmitter(&*self, id);
        }
        if self.st.tx(id).loaded {
            self.slot(id).base.transmitter.loaded = false;
            registry.invalidate_transmitter(&*self, id);
        }
    }
}

/// A vista do transportador `id`: o que o modelo, a capability e o Configurator leem.
fn view_of<W: InventoryOwnerWorld>(world: &View<'_, W>, id: TransmitterId) -> CableView {
    let base = &world.st.slots[id.0 as usize].base;
    let tx = &base.transmitter;
    let all = tx.get_all_current_connections();
    let mut view = CableView {
        connected: [false; 6],
        kind: [ConnectionType::None; 6],
        raw: [ConnectionType::None; 6],
        exists: [false; 6],
        insert: [false; 6],
        has_network: tx.has_transmitter_network(),
        redstone_reactive: false,
        redstone_activated: false,
    };
    for side in Face::ALL {
        let index = side as usize;
        view.connected[index] = connection_map_contains_side(all, side);
        view.kind[index] = tx.get_connection_type(side);
        view.raw[index] = tx.get_connection_type_raw(side);
        // `TransporterCapabilityResolver`: o `TransporterItemHandler` existe onde `exposesInsertCap(side)` vale.
        view.exists[index] = base.exposes_insert_cap(side);
        view.insert[index] = view.exists[index];
    }
    view
}

impl<W: InventoryOwnerWorld> TransmitterHost<ItemAcceptor> for Ctx<'_, W> {
    fn block_pos(&self, transmitter: TransmitterId) -> BlockPos {
        self.st.slots[transmitter.0 as usize].pos
    }

    fn dimension(&self, _: TransmitterId) -> DimensionId {
        self.st.dimension
    }

    fn is_valid(&self, transmitter: TransmitterId) -> bool {
        self.st.tx(transmitter).is_valid()
    }

    fn is_orphan(&self, transmitter: TransmitterId) -> bool {
        self.st.slots[transmitter.0 as usize].orphan
    }

    fn set_orphan(&mut self, transmitter: TransmitterId, orphan: bool) {
        let slot = self.slot(transmitter);
        slot.orphan = orphan;
        if !slot.out {
            slot.base.transmitter.set_orphan(orphan);
        }
    }

    fn transmitter_network(&self, transmitter: TransmitterId) -> Option<NetworkId> {
        self.st.slots[transmitter.0 as usize].network
    }

    fn store_transmitter_network(
        &mut self,
        transmitter: TransmitterId,
        network: Option<NetworkId>,
    ) {
        let uuid = network.map(|network| self.st.uuid_value(network));
        let slot = self.slot(transmitter);
        slot.network = network;
        // Só a atribuição: o `requestsUpdate` é do registro (`request_update_in`).
        if !slot.out {
            slot.base
                .transmitter
                .set_transmitter_network(uuid, false, &mut Vec::new());
        }
    }

    fn identity_hash(&self, transmitter: TransmitterId) -> i32 {
        self.st.slots[transmitter.0 as usize].hash
    }

    fn request_update(&mut self, transmitter: TransmitterId) {
        self.st.deferred.push((transmitter, Effect::RequestsUpdate));
    }

    fn request_update_in(&mut self, _registry: &mut Reg, transmitter: TransmitterId) {
        self.publish(transmitter);
    }

    fn supports_transmission_type(&self, transmitter: TransmitterId, other: TransmitterId) -> bool {
        self.st
            .tx(transmitter)
            .supports_transmission_type(self.st.tx(other).supported_transmission_types())
    }

    fn is_valid_transmitter_basic(
        &self,
        transmitter: TransmitterId,
        other: TransmitterId,
        side: Face,
    ) -> bool {
        self.st.slots[transmitter.0 as usize]
            .base
            .is_valid_transmitter_basic(&self.st.slots[other.0 as usize].base, side)
    }

    fn transmitter_at(&self, _: DimensionId, pos: BlockPos) -> Option<TransmitterId> {
        self.view().id_at(pos)
    }

    fn new_orphan_validator(
        &self,
        _: TransmitterId,
    ) -> Box<dyn CompatibleTransmitterValidator<ItemAcceptor>> {
        Box::new(AlwaysCompatible)
    }

    fn chunk_accessibility_change(
        &mut self,
        registry: &mut Reg,
        transmitter: TransmitterId,
        loaded: bool,
    ) {
        if loaded {
            self.world_join(registry, transmitter, true);
        } else {
            self.world_separate(registry, transmitter, true);
        }
    }

    fn refresh_acceptor_connections(&mut self, transmitter: TransmitterId, side: Face) {
        let slot = &mut self.st.slots[transmitter.0 as usize];
        if slot.out {
            return;
        }
        let placeholder = Transmitter::new(
            arr(slot.pos),
            TransmissionSet::of(&[TransmissionType::Item]),
            false,
        );
        let mut tx = std::mem::replace(&mut slot.base.transmitter, placeholder);
        slot.out = true;
        let mut out = Vec::new();
        tx.refresh_acceptor_connections(side, &self.view(), &mut out);
        let slot = self.slot(transmitter);
        slot.base.transmitter = tx;
        slot.out = false;
        self.st
            .deferred
            .extend(out.into_iter().map(|effect| (transmitter, effect)));
    }

    fn acceptor(&self, transmitter: TransmitterId, side: Face) -> Option<ItemAcceptor> {
        let tx = self.st.tx(transmitter);
        if tx.can_connect_to_acceptor(side) {
            tx.get_acceptor(side, &self.view())
        } else {
            None
        }
    }

    fn network_event(&mut self, registry: &Reg, network: NetworkId, event: NetworkEvent) {
        on_network_event(&self.st.cache, registry.network(network).uuid(), &event);
        match event {
            NetworkEvent::Created { uuid } => self.st.pending_uuid = Some(uuid),
            NetworkEvent::Merged { from } => {
                if let Some(uuid) = self.st.pending_uuid.take() {
                    self.st.uuids.insert(network, uuid);
                    self.st.event(TraceEvent::Created {
                        network,
                        hash: uuid.hash_code(),
                        from,
                    });
                }
            }
            NetworkEvent::Committing => self.st.event(TraceEvent::Commit(network)),
            NetworkEvent::Invalidating(_) => self.st.event(TraceEvent::Invalidate(network)),
            _ => {}
        }
    }
}

/// Os transportadores logísticos de uma dimensão.
pub struct InventoryTransmitters {
    registry: Reg,
    st: State,
}

impl InventoryTransmitters {
    /// `uuids` dá o UUID de cada rede nova (`UUID.randomUUID`) e `hashes` o `identityHashCode` de cada transportador novo (decisão 101).
    pub fn new(
        dimension: DimensionId,
        uuids: impl FnMut() -> Uuid + 'static,
        hashes: impl FnMut(BlockPos) -> i32 + 'static,
    ) -> Self {
        Self {
            registry: TransmitterNetworkRegistry::new(uuids),
            st: State::new(dimension, Box::new(hashes)),
        }
    }

    /// Liga ou desliga o registro dos eventos da rede ([`TraceEvent`]), para os testes.
    pub fn record_events(&mut self, on: bool) {
        self.st.record = on;
    }

    pub fn take_events(&mut self) -> Vec<TraceEvent> {
        std::mem::take(&mut self.st.events)
    }

    pub fn registry(&self) -> &Reg {
        &self.registry
    }

    pub fn registry_mut(&mut self) -> &mut Reg {
        &mut self.registry
    }

    /// O transportador vivo em `pos`.
    pub fn transporter_at(&self, pos: BlockPos) -> Option<&LogisticalTransporterBase> {
        let id = self.st.live(pos)?;
        Some(&self.st.slots[id.0 as usize].base)
    }

    /// `hasTransmitterNetwork()` do transportador em `pos`: a rede, se não é órfão.
    pub fn network_of(&self, pos: BlockPos) -> Option<NetworkId> {
        self.st.slots[self.st.live(pos)?.0 as usize].net()
    }

    /// O tipo de cada pilha em trânsito de cada transportador vivo, sem ordem prometida e sem ordenar as posições (`positions` ordena a cada chamada, e quem pergunta a cada evento paga
    /// por isso: decisão 202).
    pub fn for_each_transit_item(&self, mut visit: impl FnMut(&ItemResource)) {
        for slot in self.st.slots.iter().filter(|slot| !slot.dead) {
            for (_, stack) in slot.base.transit_stacks() {
                if let Some(item) = stack.borrow().item_type() {
                    visit(item);
                }
            }
        }
    }

    /// As posições dos transportadores vivos, em ordem.
    pub fn positions(&self) -> Vec<BlockPos> {
        let mut list: Vec<BlockPos> = self.st.by_pos.keys().copied().collect();
        list.sort_by_key(|pos| (pos.x, pos.y, pos.z));
        list
    }

    /// Quantos caminhos a cache guarda para a rede do transportador em `pos` (a invalidação por cor ou por mudança da rede os esvazia).
    pub fn cached_paths(&self, pos: BlockPos) -> Option<usize> {
        let network = self.network_of(pos)?;
        Some(
            self.st
                .cache
                .cached_count(self.registry.network(network).uuid()),
        )
    }

    /// As pilhas em voo do gerenciador, para o teste conferir o que o `remove` tirou.
    pub fn manager(&self) -> &TransporterManager {
        &self.st.manager
    }

    // ------------------------------------------------------------------------------------------------------------------------
    // Os eventos

    pub fn handle<W: InventoryOwnerWorld>(&mut self, world: &mut W, event: Event) {
        self.refresh_signals(&*world);
        match event {
            Event::ClearRemoved(pos) => self.clear_removed(world, pos),
            Event::PlacedByItem(pos) => {
                if let Some(id) = self.st.live(pos) {
                    // `onAdded`: `onWorldJoin(level, false)` e `refreshConnections()`.
                    let mut ctx = Ctx {
                        st: &mut self.st,
                        world,
                    };
                    ctx.refresh_power(id);
                    ctx.world_join(&mut self.registry, id, false);
                    ctx.with_tr(&mut self.registry, id, |tx, view, out| {
                        tx.refresh_connections(view, out)
                    });
                }
            }
            Event::ChunkUnloaded(pos) => {
                if let Some(id) = self.st.live(pos) {
                    self.st.slots[id.0 as usize].unloaded = true;
                }
            }
            Event::Removed(pos) => self.removed(world, pos),
            Event::Update(pos) => self.update(world, pos),
            Event::Neighbor(pos) => {
                if let Some(id) = self.st.live(pos) {
                    let mut ctx = Ctx {
                        st: &mut self.st,
                        world,
                    };
                    ctx.neighbor_changed(&mut self.registry, id);
                }
            }
            Event::Capability(pos, side) => {
                if let Some(id) = self.st.live(pos) {
                    let mut ctx = Ctx {
                        st: &mut self.st,
                        world,
                    };
                    ctx.with_tr(&mut self.registry, id, |tx, _, out| {
                        if let Some(dirty) =
                            tx.acceptor_cache.refresh_listener_run(side, tx.is_valid())
                        {
                            tx.mark_dirty_acceptor(dirty, out);
                        }
                    });
                }
            }
            Event::Ticket { chunk, loaded } => {
                let (old, new) = if loaded {
                    (MAX_VIEW_DISTANCE + 1, MAX_VIEW_DISTANCE)
                } else {
                    (MAX_VIEW_DISTANCE, MAX_VIEW_DISTANCE + 1)
                };
                self.registry
                    .on_ticket_level_change(self.st.dimension, chunk.pack(), old, new);
            }
            Event::ServerTickPost { runs_normally } => {
                let mut ctx = Ctx {
                    st: &mut self.st,
                    world,
                };
                self.registry.on_tick(&mut ctx, runs_normally);
            }
            Event::Reset => {
                self.registry.reset();
                let dimension = self.st.dimension;
                let hashes = std::mem::replace(&mut self.st.hashes, Box::new(|_| 0));
                let record = self.st.record;
                self.st = State::new(dimension, hashes);
                self.st.record = record;
            }
        }
        self.flush_deferred(world);
    }

    /// `WorldUtils.isGettingPowered(level, pos)` de cada desviador: o original o lê a cada uso, e nada muda o sinal no meio de um evento, então basta lê-lo no começo
    /// de cada um.
    fn refresh_signals<W: InventoryOwnerWorld>(&self, world: &W) {
        for slot in &self.st.slots {
            if !slot.dead && matches!(slot.base.kind, TransporterKind::Diversion { .. }) {
                slot.power.set(world.is_getting_powered(slot.pos));
            }
        }
    }

    fn clear_removed<W: InventoryOwnerWorld>(&mut self, world: &mut W, pos: BlockPos) {
        if let Some(id) = self.st.live(pos) {
            // O mesmo block entity entrando de novo: só marca para entrar.
            self.st.slots[id.0 as usize].mark_joined = true;
            return;
        }
        let Some(info) = world.transporter_info(pos) else {
            return;
        };
        let hash = (self.st.hashes)(pos);
        let id = TransmitterId(self.st.slots.len() as u32);
        let power = Rc::new(Cell::new(false));
        let signal = power.clone();
        let mut base = LogisticalTransporterBase::new(
            pos,
            info.tier,
            info.kind,
            Rc::new(move || signal.get()),
        );
        if let Some(saved) = info.saved {
            // `loadAdditional`, antes de `setLevel`: o `Transmitter`, a cor ou o modo, e as pilhas em trânsito (com ids novos).
            base.transmitter.read(&saved.tag, &mut Vec::new());
            base.read_color(saved.color.as_ref());
            base.read_modes(saved.modes.as_ref());
            base.read_items(saved.items.as_ref(), &|tag| world.decode_item(tag));
        }
        base.transmitter.set_level(false);
        power.set(world.is_getting_powered(pos));
        self.st.slots.push(Slot {
            pos,
            base,
            hash,
            mark_joined: true,
            network: None,
            orphan: true,
            out: false,
            dead: false,
            unloaded: false,
            power,
        });
        self.st.by_pos.insert(pos, id);
    }

    fn removed<W: InventoryOwnerWorld>(&mut self, world: &mut W, pos: BlockPos) {
        let Some(id) = self.st.live(pos) else {
            return;
        };
        // `preRemoveSideEffects`: um bloco quebrado solta o que leva (o chunk que descarrega, não).
        if !self.st.slots[id.0 as usize].unloaded {
            self.move_with(world, id, |base, ctx| {
                base.pre_remove_side_effects(ctx);
            });
            self.drain_transit(world, id);
        }
        {
            let slot = &mut self.st.slots[id.0 as usize];
            slot.base.transmitter.removed = true;
            slot.dead = true;
        }
        // `setRemoved`: `onWorldSeparate(level, false)` e `remove()`.
        let mut ctx = Ctx {
            st: &mut self.st,
            world,
        };
        ctx.world_separate(&mut self.registry, id, false);
        let path = PathContext {
            dimension: self.st.dimension,
            manager: &self.st.manager,
            cache: &self.st.cache,
            world: &Adaptor {
                st: &self.st,
                world: &*world,
            },
        };
        self.st.slots[id.0 as usize].base.remove_transit(&path);
        self.st.by_pos.remove(&pos);
    }

    fn update<W: InventoryOwnerWorld>(&mut self, world: &mut W, pos: BlockPos) {
        let Some(id) = self.st.live(pos) else {
            return;
        };
        {
            let mut ctx = Ctx {
                st: &mut self.st,
                world: &mut *world,
            };
            ctx.refresh_power(id);
            // `TileEntityTransmitter.onUpdateServer`.
            if ctx.st.slots[id.0 as usize].mark_joined {
                ctx.world_join(&mut self.registry, id, false);
                ctx.slot(id).mark_joined = false;
            }
            ctx.with_tr(&mut self.registry, id, |tx, view, out| {
                tx.on_update_server(view, out)
            });
        }
        // `LogisticalTransporterBase.onUpdateServer`.
        self.move_with(world, id, |base, ctx| base.on_update_server(ctx));
        self.drain_transit(world, id);
    }

    /// Roda `body` com o transportador `id`, o contexto de busca dele e a rede (`None` sem rede).
    fn move_with<W: InventoryOwnerWorld>(
        &self,
        world: &W,
        id: TransmitterId,
        body: impl FnOnce(&LogisticalTransporterBase, &MoveContext),
    ) {
        let slot = &self.st.slots[id.0 as usize];
        let adaptor = Adaptor {
            st: &self.st,
            world,
        };
        let path = PathContext {
            dimension: self.st.dimension,
            manager: &self.st.manager,
            cache: &self.st.cache,
            world: &adaptor,
        };
        let lookup = Lookup(&self.st);
        match slot.net() {
            Some(network) => {
                let view = RegistryNetworkView {
                    registry: &self.registry,
                    network,
                    transporters: &lookup,
                };
                let view: &dyn InventoryNetworkView = &view;
                body(
                    &slot.base,
                    &MoveContext {
                        path: &path,
                        network: Some(view),
                    },
                );
            }
            None => body(
                &slot.base,
                &MoveContext {
                    path: &path,
                    network: None,
                },
            ),
        }
    }

    /// O que o movimento mandou ao disco e ao mundo: o aviso de salvar e o que caiu.
    fn drain_transit<W: InventoryOwnerWorld>(&mut self, world: &mut W, id: TransmitterId) {
        let slot = &self.st.slots[id.0 as usize];
        let pos = slot.pos;
        let events = slot.base.take_events();
        let drops = slot.base.take_world_drops();
        let packed = |stack: &TransporterStack, world: &W| -> Compound {
            let tag =
                stack.write_to_update_tag_with(pos.as_long(), &|item| world.encode_item(item));
            match nbt_to_tag(&Nbt::Compound(tag)) {
                Tag::Compound(compound) => compound,
                _ => Compound::new(),
            }
        };
        for event in events {
            match event {
                TransitEvent::MarkForSave => world.set_changed(pos),
                TransitEvent::Sync { id, stack } => {
                    let stack = packed(&stack, &*world);
                    world.transporter_packet(pos, TransporterPacket::Sync { id, stack });
                }
                TransitEvent::Batch { deletes, updates } => {
                    let updates = updates
                        .iter()
                        .map(|(id, stack)| (*id, packed(stack, &*world)))
                        .collect();
                    world.transporter_packet(pos, TransporterPacket::Batch { deletes, updates });
                }
            }
        }
        for drop in drops {
            world.world_drop(drop);
        }
    }

    fn flush_deferred<W: InventoryOwnerWorld>(&mut self, world: &mut W) {
        while !self.st.deferred.is_empty() {
            for (id, effect) in std::mem::take(&mut self.st.deferred) {
                let mut ctx = Ctx {
                    st: &mut self.st,
                    world: &mut *world,
                };
                ctx.apply(&mut self.registry, id, vec![effect]);
            }
        }
    }

    // ------------------------------------------------------------------------------------------------------------------------
    // O que o jogo pergunta

    /// A vista do transportador em `pos`.
    pub fn view_at<W: InventoryOwnerWorld>(&self, world: &W, pos: BlockPos) -> Option<CableView> {
        let id = self.st.live(pos)?;
        Some(view_of(
            &View {
                st: &self.st,
                world,
            },
            id,
        ))
    }

    /// Roda `body` com o transportador em `pos` e o contexto dele (`TransporterEnv::with_transporter`).
    pub fn with_transporter<W: InventoryOwnerWorld>(
        &self,
        world: &W,
        pos: BlockPos,
        body: impl FnOnce(&LogisticalTransporterBase, &MoveContext),
    ) {
        if let Some(id) = self.st.live(pos) {
            self.move_with(world, id, body);
        }
    }

    /// `TransporterItemHandler.insert` pelo lado `side` do transportador em `pos`: o que um funil ou o ejetor de outra máquina põe nele. Devolve quanto entrou, ou
    /// `None` se o transportador não expõe a capability naquele lado.
    pub fn insert<W: InventoryOwnerWorld>(
        &mut self,
        world: &mut W,
        pos: BlockPos,
        side: Face,
        resource: &ItemResource,
        amount: u32,
    ) -> Option<u32> {
        let id = self.st.live(pos)?;
        {
            let mut ctx = Ctx {
                st: &mut self.st,
                world: &mut *world,
            };
            ctx.refresh_power(id);
        }
        let base = &self.st.slots[id.0 as usize].base;
        if !base.exposes_insert_cap(side) {
            return None;
        }
        let limit = base.tier.pull_amount() as u32;
        let from = crate::transporter::pos::relative_pos(pos.as_long(), side);
        let mut inserted = 0;
        self.move_with(&*world, id, |base, ctx| {
            // Sem rede, nada entra (`hasTransmitterNetwork`).
            if ctx.network.is_some() {
                let transaction = Transaction::open_root();
                inserted = base.insert_unchecked_from(
                    ctx,
                    from,
                    resource,
                    amount.min(limit),
                    Some(&transaction),
                );
                transaction.commit();
            }
        });
        self.drain_transit(world, id);
        Some(inserted)
    }

    /// `LogisticalTransporterBase.insert(level, outputter, outputterPos, request, color, min, transaction)` com `color = outputColor == null ? transporter.getColor() : outputColor`
    /// (`TransitRequest.eject` para um `TransporterItemHandler`): o que o ejetor de uma máquina chama. `outputter_pos` é o bloco que ejeta. `None` se não há transportador em `pos` ou
    /// o lado não expõe a capability (`exposesInsertCap`); senão a resposta, vazia quando nenhum caminho serve (ou não há rede, que o original nem chega a tentar).
    #[allow(clippy::too_many_arguments)]
    pub fn insert_request<W: InventoryOwnerWorld>(
        &mut self,
        world: &mut W,
        pos: BlockPos,
        outputter_pos: BlockPos,
        request: &crate::transit::TransitRequest,
        output_color: Option<u8>,
        min: u32,
        transaction: &Transaction,
    ) -> Option<crate::transit::TransitResponse> {
        let id = self.st.live(pos)?;
        {
            let mut ctx = Ctx {
                st: &mut self.st,
                world: &mut *world,
            };
            ctx.refresh_power(id);
        }
        let from = crate::sides::Face::ALL.into_iter().find(|side| {
            crate::transporter::pos::relative_pos(pos.as_long(), *side) == outputter_pos.as_long()
        })?;
        if !self.st.slots[id.0 as usize].base.exposes_insert_cap(from) {
            return None;
        }
        let mut response = crate::transit::TransitResponse::EMPTY;
        self.move_with(&*world, id, |base, ctx| {
            if ctx.network.is_some() {
                let color = output_color.or_else(|| base.get_color());
                response = base.insert(
                    ctx,
                    crate::transporter::movement::Outputter::Tile(
                        crate::transporter::world::OutputterTile::Other,
                    ),
                    outputter_pos.as_long(),
                    request,
                    color,
                    min,
                    Some(transaction),
                );
            }
        });
        self.drain_transit(world, id);
        Some(response)
    }

    /// O que o transportador guarda no block entity (`TileEntityTransmitter.saveAdditional`, `LogisticalTransporterBase.write`, a cor e o modo). `encode` é o
    /// `ItemResource.CODEC` do jogo.
    pub fn write(&self, pos: BlockPos, encode: ItemEncode) -> Option<SavedTransporter> {
        let id = self.st.live(pos)?;
        let base = &self.st.slots[id.0 as usize].base;
        Some(SavedTransporter {
            tag: base.transmitter.write(),
            color: base.write_color(),
            modes: base.write_modes(),
            items: base.write_items_with(encode),
        })
    }

    /// O pacote de atualização do transportador (`writeReducedUpdatedTag`) e o `getCapacity()` dele (a rede de itens não tem buffer: zero): o do `Transmitter`, as pilhas em
    /// trânsito com o `index` de cada uma (`LogisticalTransporterBase`), a cor (`LogisticalTransporter`) e os modos do desviador (`DiversionTransporter`). `encode` é o
    /// `ItemResource.CODEC` do jogo.
    pub fn client_update(&self, pos: BlockPos, encode: ItemEncode) -> Option<(Compound, i64)> {
        let id = self.st.live(pos)?;
        let base = &self.st.slots[id.0 as usize].base;
        let mut tag = base.transmitter.write_reduced_updated_tag();
        if let Some(items) = base.write_reduced_updated_items_with(encode) {
            tag.insert(keys::ITEMS.into(), nbt_to_tag(&items));
        }
        if let Some(color) = base.write_color() {
            tag.insert(keys::COLOR.into(), nbt_to_tag(&color));
        }
        if let Some(modes) = base.write_modes() {
            tag.insert(keys::MODE.into(), nbt_to_tag(&modes));
        }
        Some((tag, 0))
    }

    /// `TileEntityTransmitter.onAlloyInteraction`, até a lista: o transportador clicado em `pos` tem rede, e os da rede que a liga de nível `alloy_level` sobe (`canUpgrade`) vêm do
    /// mais perto para o mais longe. `tier_of` dá o nível do tier do bloco em cada posição, e nada onde o bloco não é um `LogisticalTransporter` (o restritivo e o desviador não são
    /// `IUpgradeableTransmitter`).
    pub fn alloy_start(
        &self,
        pos: BlockPos,
        alloy_level: i32,
        tier_of: &dyn Fn(BlockPos) -> Option<i32>,
    ) -> Option<AlloyRun> {
        let id = self.st.live(pos)?;
        let slot = &self.st.slots[id.0 as usize];
        if !slot.base.transmitter.has_transmitter_network() {
            return None;
        }
        let network = slot.network?;
        let members: Vec<BlockPos> = self
            .registry
            .network(network)
            .transmitters()
            .into_iter()
            .map(|member| &self.st.slots[member.0 as usize])
            .filter(|member| !member.dead)
            .map(|member| member.pos)
            .filter(|at| tier_of(*at).is_some_and(|level| can_upgrade(alloy_level, level)))
            .collect();
        Some(AlloyRun {
            network,
            targets: order_by_distance(pos, members),
            shares_set: false,
            upgraded: 0,
        })
    }

    /// O começo da troca de um transportador: `startUpgrading` (`isUpgrading`, que impede o `preRemoveSideEffects` de soltar as pilhas, `takeShare(null)`, que ele não tem, e
    /// `setTransmitterNetwork(null)`) e o dado de upgrade (`getUpgradeData`), antes de o bloco mudar. A rede de itens não é em buffer: não há shares a validar.
    pub fn alloy_take<W: InventoryOwnerWorld>(
        &mut self,
        world: &mut W,
        run: &mut AlloyRun,
        pos: BlockPos,
    ) -> Option<TransporterUpgrade> {
        let id = self.st.live(pos)?;
        run.shares_set = true;
        self.st.slots[id.0 as usize]
            .base
            .transmitter
            .set_upgrading(true);
        {
            let mut ctx = Ctx {
                st: &mut self.st,
                world,
            };
            set_transmitter_network(&mut ctx, id, None, true);
        }
        let base = &self.st.slots[id.0 as usize].base;
        let data = TransporterUpgrade {
            redstone_reactive: base.transmitter.redstone_reactive,
            connection_types: *base.transmitter.connection_types_raw(),
            color: base.get_color(),
            transit: base.upgrade_transit(),
        };
        self.flush_deferred(world);
        Some(data)
    }

    /// O fim da troca: o transportador novo em `pos` recebe o dado (`parseUpgradeData`: o redstone, os tipos crus, a cor, as pilhas e os contadores). `false` se o bloco novo não
    /// tem transportador ("Error upgrading transmitter"): o laço segue sem contar.
    pub fn alloy_give(
        &mut self,
        run: &mut AlloyRun,
        pos: BlockPos,
        data: TransporterUpgrade,
    ) -> bool {
        let Some(id) = self.st.live(pos) else {
            return false;
        };
        let base = &mut self.st.slots[id.0 as usize].base;
        base.transmitter.redstone_reactive = data.redstone_reactive;
        base.transmitter
            .set_connection_types_raw(data.connection_types);
        base.set_color(data.color);
        base.parse_upgrade_transit(data.transit);
        run.upgraded += 1;
        true
    }

    /// `transmitterNetwork.invalidate(null)` ao fim do laço, se algum subiu.
    pub fn alloy_finish<W: InventoryOwnerWorld>(&mut self, world: &mut W, run: &AlloyRun) {
        if !run.did_upgrade() {
            return;
        }
        let mut ctx = Ctx {
            st: &mut self.st,
            world,
        };
        self.registry
            .invalidate_network(&mut ctx, run.network, None);
        self.flush_deferred(world);
    }

    /// Lê as pilhas que o pacote de atualização traz (`handleUpdateTag`); só para os testes e para o cliente.
    pub fn handle_update_items(&self, pos: BlockPos, items: Option<&Nbt>, decode: ItemDecode) {
        if let Some(base) = self.transporter_at(pos) {
            base.handle_update_items(items, decode);
        }
    }

    /// Os cliques do Configurator (`TileEntityTransmitter.onSneakRightClick`): `hit_side` é o lado do braço que o olhar toca. Devolve se o
    /// clique foi do transportador.
    pub fn sneak_right_click<W: InventoryOwnerWorld>(
        &mut self,
        world: &mut W,
        pos: BlockPos,
        side: Face,
        hit_side: Option<Face>,
    ) -> bool {
        let Some(id) = self.st.live(pos) else {
            return false;
        };
        // `onConfigure` do `LogisticalTransporter`, que corre antes do resto do clique: a cor seguinte, a cache de caminhos da rede, salvar, avisar os vizinhos
        // e mandar o pacote. Só vale se o clique não pegou um braço e o lado tem conexão; os outros tipos devolvem `PASS`.
        let configures = hit_side.is_none()
            && self.st.tx(id).get_connection_type_raw(side) != ConnectionType::None
            && matches!(
                self.st.slots[id.0 as usize].base.kind,
                TransporterKind::Logistical { .. }
            );
        let mut result = InteractionResult::Pass;
        if configures {
            self.configure_color(world, id);
            result = InteractionResult::Success;
        }
        let consumed = Ctx {
            st: &mut self.st,
            world: &mut *world,
        }
        .with_tr(&mut self.registry, id, |tx, view, out| {
            tx.on_sneak_right_click(side, hit_side, |_, _| result, view, out)
        });
        self.flush_deferred(world);
        consumed.is_some_and(|result| result.consumes_action())
    }

    /// `LogisticalTransporter.onConfigure`.
    fn configure_color<W: InventoryOwnerWorld>(&mut self, world: &mut W, id: TransmitterId) {
        let pos = self.st.slots[id.0 as usize].pos;
        let color = self.st.slots[id.0 as usize]
            .base
            .configure_color()
            .flatten();
        self.color_changed(world, id);
        world.overlay(pos, Overlay::ToggleColor { color });
    }

    /// O que `onConfigure` faz depois de trocar a cor: a cache de caminhos da rede, salvar, avisar os vizinhos e mandar o pacote.
    fn color_changed<W: InventoryOwnerWorld>(&mut self, world: &mut W, id: TransmitterId) {
        let pos = self.st.slots[id.0 as usize].pos;
        if let Some(network) = self.st.slots[id.0 as usize].net() {
            self.st
                .cache
                .on_changed(self.registry.network(network).uuid());
        }
        world.set_changed(pos);
        let mut ctx = Ctx {
            st: &mut self.st,
            world: &mut *world,
        };
        ctx.notify_neighbors(&mut self.registry, pos);
        ctx.publish(id);
    }

    /// Põe a cor do transportador em `pos` (`setColor` e o que `onConfigure` faz depois), sem o aviso: para a sonda do teste em jogo.
    pub fn set_color<W: InventoryOwnerWorld>(
        &mut self,
        world: &mut W,
        pos: BlockPos,
        color: Option<u8>,
    ) {
        let Some(id) = self.st.live(pos) else {
            return;
        };
        self.st.slots[id.0 as usize].base.set_color(color);
        self.color_changed(world, id);
        self.flush_deferred(world);
    }

    /// O que a chave inglesa faz num lado (`setConnectionTypeRaw`, `onModeChange`, `refreshConnections` e `notifyTileChange`): para os testes e a sonda do teste em jogo.
    pub fn set_mode<W: InventoryOwnerWorld>(
        &mut self,
        world: &mut W,
        pos: BlockPos,
        side: Face,
        kind: ConnectionType,
    ) {
        let Some(id) = self.st.live(pos) else {
            return;
        };
        {
            let mut ctx = Ctx {
                st: &mut self.st,
                world: &mut *world,
            };
            ctx.with_tr(&mut self.registry, id, |t, _, out| {
                t.set_connection_type_raw(side, kind, out)
            });
            ctx.with_tr(&mut self.registry, id, |t, w, out| {
                t.on_mode_change(side, w, out)
            });
            ctx.with_tr(&mut self.registry, id, |t, w, out| {
                t.refresh_connections(w, out)
            });
            ctx.with_tr(&mut self.registry, id, |t, _, out| {
                t.notify_tile_change(out)
            });
        }
        self.flush_deferred(world);
    }

    /// Põe o modo de um lado do desviador em `pos` (`updateMode`): para a sonda do teste em jogo.
    pub fn set_diversion_mode<W: InventoryOwnerWorld>(
        &mut self,
        world: &mut W,
        pos: BlockPos,
        side: Face,
        mode: DiversionControl,
    ) {
        if let Some(id) = self.st.live(pos) {
            {
                let mut ctx = Ctx {
                    st: &mut self.st,
                    world: &mut *world,
                };
                ctx.refresh_power(id);
            }
            self.update_mode(world, id, side, mode);
        }
    }

    /// `onRightClick`: o transportador logístico mostra a cor; o desviador troca o modo do lado (`hit_side` é o lado do braço que o olhar toca, `side` a face clicada).
    pub fn right_click<W: InventoryOwnerWorld>(
        &mut self,
        world: &mut W,
        pos: BlockPos,
        side: Face,
        hit_side: Option<Face>,
    ) -> bool {
        let Some(id) = self.st.live(pos) else {
            return false;
        };
        match self.st.slots[id.0 as usize].base.kind.clone() {
            TransporterKind::Logistical { color } => {
                world.overlay(pos, Overlay::CurrentColor { color });
            }
            TransporterKind::Diversion { .. } => {
                let side = hit_side.unwrap_or(side);
                {
                    let mut ctx = Ctx {
                        st: &mut self.st,
                        world: &mut *world,
                    };
                    ctx.refresh_power(id);
                }
                let Some(mode) = self.st.slots[id.0 as usize].base.next_mode(side) else {
                    return true;
                };
                self.update_mode(world, id, side, mode);
                world.overlay(pos, Overlay::ToggleDiverter { mode });
            }
            TransporterKind::Restrictive => {}
        }
        true
    }

    /// `DiversionTransporter.updateMode(side, mode)`.
    pub fn update_mode<W: InventoryOwnerWorld>(
        &mut self,
        world: &mut W,
        id: TransmitterId,
        side: Face,
        mode: DiversionControl,
    ) {
        let pos = self.st.slots[id.0 as usize].pos;
        let Some(update) = self.st.slots[id.0 as usize].base.update_mode(side, mode) else {
            return;
        };
        if let Some(change) = update.capability {
            world.capability_changed(pos, change);
        }
        world.set_changed(pos);
        let mut ctx = Ctx {
            st: &mut self.st,
            world: &mut *world,
        };
        ctx.with_tr(&mut self.registry, id, |t, w, out| {
            t.refresh_connections(w, out)
        });
        ctx.notify_neighbors(&mut self.registry, pos);
        ctx.publish(id);
        self.flush_deferred(world);
    }

    /// O lado do braço que o olhar toca (`getSideLookingAt`), pelo índice da caixa tocada.
    pub fn side_looking_at(&self, pos: BlockPos, box_index: Option<usize>) -> Option<Face> {
        self.transporter_at(pos)?
            .transmitter
            .side_looking_at(box_index)
    }

    /// O lado de onde `from` toca o transportador em `pos`, se encosta.
    pub fn side_of(&self, pos: BlockPos, from: BlockPos) -> Option<Face> {
        side_difference(pos.as_long(), from.as_long())
    }
}

impl super::reading::NetworkReader for InventoryTransmitters {
    type Resource = ();

    /// `ItemNetworkReader.displayTransmitterInfo` para o transportador em `pos`: a rede de itens não responde nenhuma das quatro informações (`INetworkDataHandler`).
    fn read_network(&self, pos: BlockPos) -> super::reading::TransmitterReading<()> {
        use super::reading::{NetworkReading, ReadingDetail, TransmitterReading};
        if self.st.live(pos).is_none() {
            return TransmitterReading::NoTransmitter;
        }
        let Some(network) = self.network_of(pos) else {
            return TransmitterReading::NoNetwork;
        };
        let dynamic = self.registry.network(network);
        TransmitterReading::Network(NetworkReading {
            transmitters: dynamic.transmitters_size(),
            acceptors: dynamic.acceptor_count(),
            detail: ReadingDetail::Inventory,
        })
    }

    fn acceptor_sides(
        &self,
        transmitter: BlockPos,
        acceptor: BlockPos,
    ) -> Option<super::reading::AcceptorSides> {
        let network = self.st.slots[self.st.live(transmitter)?.0 as usize].network?;
        let dynamic = self.registry.network(network);
        dynamic
            .has_acceptor(acceptor)
            .then(|| super::reading::AcceptorSides {
                network,
                sides: dynamic.acceptor_directions(acceptor),
            })
    }
}
