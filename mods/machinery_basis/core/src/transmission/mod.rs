//! A topologia das redes de transmissores (Mekanism `lib/transmitter`): o registro, a rede genérica e o que a fusão adota.
//!
//! Porta, no servidor, `TransmitterNetworkRegistry` (e o `OrphanPathFinder` e o `DimData` dele), a parte de topologia de `DynamicNetwork`,
//! `NetworkAcceptorCache` e `CompatibleTransmitterValidator`. O que fica de fora:
//!
//! - o cliente (`clientNetworks`, `addClientNetwork`, `createEmptyNetworkWithID`, as ramificações `isClientSide`), que não tem o registro;
//! - o `Transmitter`, as conexões por lado, o redstone e o `AcceptorCache` do transmissor (`transmitter`, `connection` e `acceptor_cache`,
//!   frente `rede-conexao`, decisão 104). O dono dos transmissores liga os dois: ver a decisão 102;
//! - a energia, o fluido, o químico, o item e o calor: buffer, shares, `onUpdate` de cada família (segunda leva). A rede genérica chama
//!   o que elas pedem por [`TransmitterHost`] e [`NetworkEvent`].
//!
//! A lib não conhece id nem tipo do Mekanism. O dono dos transmissores (o jogo, ou o mundo simulado de um teste) implementa
//! [`TransmitterHost`]: o que a rede pergunta ao `Transmitter` (posição, `isValid`, `isOrphan`, rede atual, lados que ligam a outro
//! transmissor, tipo de transmissão) e o que ela manda fazer (`requestsUpdate`, `chunkAccessibilityChange`, `takeShare`).
//!
//! # A ordem de iteração
//!
//! `networks`, `invalidTransmitters`, `networksToChange`, `networksFound`, `connectedTransmitters` e `transmittersToAdd` são
//! `ObjectOpenHashSet` e `positionedTransmitters` é um `Long2ObjectOpenHashMap`: quem os percorre vê a ordem da tabela, não a da
//! inserção (`sim::open_hash`). A ordem decide a ordem dos `onUpdate`, dos `requestsUpdate` e dos `commit`. As chaves são
//! `DynamicNetwork.hashCode()` (o `UUID.hashCode()`, sorteado a cada rede) e o `hashCode` de `Transmitter`, que não o sobrescreve e vale
//! o `System.identityHashCode` (sorteado a cada JVM). Os dois vêm de fora ([`Uuid`], [`TransmitterHost::identity_hash`]): em produção
//! são sorteados como no original, e um teste dá os que o rastro gravou e reproduz a ordem daquela execução (decisão 101).
//! `DimData` guarda os órfãos e os chunks em árvores (`Long2ObjectRBTreeMap`, `Long2BooleanRBTreeMap`, `Long2ObjectAVLTreeMap`), cuja
//! ordem é a da chave `long` com sinal: determinística.
pub mod acceptor_cache;
pub mod buffered_network;
pub mod buffered_transmitter;
pub mod connection;
pub mod dynamic_network;
pub mod energy_container;
pub mod energy_network;
pub mod heat_network;
pub mod inventory_network;
pub mod network_acceptors;
pub mod registry;
pub mod thermodynamic_conductor;
pub mod transmitter;
pub mod universal_cable;
pub mod validator;

use crate::sides::Face;
use crate::sim::chunks::BlockPos;
use crate::sim::open_hash::{OpenKey, mix_int, mix_long};
use registry::TransmitterNetworkRegistry;
use validator::CompatibleTransmitterValidator;

/// Um `Transmitter`, pelo índice que o dono dos transmissores lhe deu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TransmitterId(pub u32);

/// Uma `DynamicNetwork`, pelo índice no registro. Nunca é reaproveitado: uma rede desfeita continua existindo, sem transmissores.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NetworkId(pub u32);

/// Um `ResourceKey<Level>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DimensionId(pub u32);

/// Um `java.util.UUID`, só com o que a rede usa dele: o `hashCode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Uuid {
    pub most_significant: i64,
    pub least_significant: i64,
}

impl Uuid {
    /// `UUID.hashCode`: `(int) (hilo >> 32) ^ (int) hilo`, com `hilo = most ^ least`.
    pub fn hash_code(self) -> i32 {
        let hilo = self.most_significant ^ self.least_significant;
        ((hilo >> 32) as i32) ^ (hilo as i32)
    }

    /// Um UUID com o `hashCode` pedido, para o teste que reproduz a ordem de uma execução.
    pub fn with_hash_code(hash: i32) -> Self {
        Self {
            most_significant: 0,
            least_significant: i64::from(hash as u32),
        }
    }

    /// `UUID.randomUUID`: 122 bits sorteados, versão 4 e variante 2.
    pub fn random() -> Self {
        use crate::java_random::{JavaRandom, entropy_seed};
        use std::sync::Mutex;
        static RANDOM: Mutex<Option<JavaRandom>> = Mutex::new(None);
        let mut guard = RANDOM.lock().unwrap_or_else(|e| e.into_inner());
        let random = guard.get_or_insert_with(|| JavaRandom::new(entropy_seed()));
        let mut long = || (i64::from(random.next(32)) << 32) | i64::from(random.next(32) as u32);
        let most = (long() & !0xF000) | 0x4000;
        let least = (long() & 0x3FFF_FFFF_FFFF_FFFF) | i64::MIN;
        Self {
            most_significant: most,
            least_significant: least,
        }
    }
}

/// Uma chave de tabela do fastutil cujo `hashCode` vem de fora (o de identidade de um objeto, o `UUID` de uma rede).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Hashed<T> {
    pub id: T,
    pub hash: i32,
}

impl<T: Copy + Eq> OpenKey for Hashed<T> {
    fn mixed(self) -> i32 {
        mix_int(self.hash)
    }
}

/// `BlockPos.asLong`, a chave dos mapas de `long`. A posição 0 é a chave nula do fastutil.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LongKey(pub i64);

impl OpenKey for LongKey {
    fn mixed(self) -> i32 {
        mix_long(self.0)
    }

    const NULL_KEY: Option<Self> = Some(LongKey(0));
}

/// O que acontece numa rede e que as famílias (energia, fluido...) acompanham. Cada um é chamado no ponto do Java em que a subclasse
/// sobrescreve o método: depois do `super`, a menos que a documentação diga outra coisa.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetworkEvent {
    /// `new Network(Collection)`: a rede nasceu, antes de adotar as outras (o construtor da subclasse roda antes de `adoptAllAndRegister`).
    Created { uuid: Uuid },
    /// O fim de `new Network(Collection)`: a rede adotou as dadas e se registrou.
    Merged { from: Vec<NetworkId> },
    /// `adoptTransmittersAndAcceptorsFrom(net)`, depois do `super`.
    Adopted { from: NetworkId },
    /// O começo de `commit()`, antes de qualquer mudança.
    Committing,
    /// O começo de `invalidate(trigger)`, antes de qualquer mudança.
    Invalidating(Option<TransmitterId>),
    /// `addNewTransmitters`, depois do `super`.
    NewTransmitters,
    /// `addTransmitterFromCommit(transmitter)`, depois de pôr o transmissor em `positionedTransmitters`.
    TransmitterCommitted(TransmitterId),
    /// `validTransmittersAdded()`, no `commit`, antes dos `requestsUpdate`.
    ValidTransmittersAdded,
    /// O fim de `commit()`.
    Committed,
    /// `onLastTransmitterRemoved(trigger)`, em `invalidate`, antes de `removeInvalid`.
    LastTransmitterRemoved(TransmitterId),
    /// `removeInvalid(trigger)`, depois de tirar os inválidos.
    InvalidRemoved(Option<TransmitterId>),
    /// `deregister()`, depois do `super`.
    Deregistered,
}

/// Tudo o que a rede genérica pede ao dono dos transmissores (`Transmitter`, `TileEntityTransmitter`) e às famílias de rede.
///
/// `A` é o aceitador (`EnergyHandler`, `ResourceHandler<FluidResource>`...), que a lib só guarda e copia. Os métodos de consulta não
/// mudam nada; os que mudam são os que o Java chama no transmissor.
pub trait TransmitterHost<A> {
    /// `Transmitter.getBlockPos`.
    fn block_pos(&self, transmitter: TransmitterId) -> BlockPos;
    /// `Transmitter.getLevel().dimension()`.
    fn dimension(&self, transmitter: TransmitterId) -> DimensionId;
    /// `Transmitter.isValid`: `!tile.isRemoved() && tile.isLoaded()`.
    fn is_valid(&self, transmitter: TransmitterId) -> bool;
    /// `Transmitter.isOrphan`.
    fn is_orphan(&self, transmitter: TransmitterId) -> bool;
    /// `Transmitter.setOrphan`.
    fn set_orphan(&mut self, transmitter: TransmitterId, orphan: bool);
    /// `Transmitter.getTransmitterNetwork`.
    fn transmitter_network(&self, transmitter: TransmitterId) -> Option<NetworkId>;
    /// Só a atribuição `theNetwork = network` de `setTransmitterNetwork`. O resto (`orphaned`, o `requestsUpdate`) é de
    /// [`registry::set_transmitter_network`].
    fn store_transmitter_network(&mut self, transmitter: TransmitterId, network: Option<NetworkId>);
    /// O `hashCode` do `Transmitter`, que é o `System.identityHashCode` (a classe não sobrescreve `hashCode`).
    fn identity_hash(&self, transmitter: TransmitterId) -> i32;
    /// `Transmitter.requestsUpdate` (`sendUpdatePacket` do tile).
    fn request_update(&mut self, transmitter: TransmitterId);
    /// `Transmitter.supportsTransmissionType(other)`: algum tipo de transmissão em comum.
    fn supports_transmission_type(&self, transmitter: TransmitterId, other: TransmitterId) -> bool;
    /// `Transmitter.isValidTransmitterBasic(tile, side)`: `supportsTransmissionType(tile) && canConnectMutual(side, tile)`.
    fn is_valid_transmitter_basic(
        &self,
        transmitter: TransmitterId,
        other: TransmitterId,
        side: Face,
    ) -> bool;
    /// `WorldUtils.getTileEntity(TileEntityTransmitter.class, world, chunkMap, pos)`: o transmissor do bloco em `pos`, ou `None` se o
    /// chunk não está carregado (`getChunk(FULL, false)` nulo) ou não há transmissor.
    fn transmitter_at(&self, dimension: DimensionId, pos: BlockPos) -> Option<TransmitterId>;
    /// `Transmitter.getNewOrphanValidator`.
    fn new_orphan_validator(
        &self,
        _transmitter: TransmitterId,
    ) -> Box<dyn CompatibleTransmitterValidator<A>> {
        Box::new(validator::AlwaysCompatible)
    }
    /// `TileEntityTransmitter.chunkAccessibilityChange(loaded)`. Quem o implementa volta ao registro (`register_orphan_transmitter`,
    /// `invalidate_transmitter`), por isso recebe o registro.
    fn chunk_accessibility_change(
        &mut self,
        registry: &mut TransmitterNetworkRegistry<A>,
        transmitter: TransmitterId,
        loaded: bool,
    );
    /// `Transmitter.refreshAcceptorConnections(side)`.
    fn refresh_acceptor_connections(&mut self, transmitter: TransmitterId, side: Face);
    /// `transmitter.canConnectToAcceptor(side) ? transmitter.getAcceptor(side) : null`.
    fn acceptor(&self, transmitter: TransmitterId, side: Face) -> Option<A>;
    /// `Transmitter.takeShare(transaction)`, em `invalidate`, na ordem de `positionedTransmitters`. Dentro de uma transação raiz no
    /// original; a energia (segunda leva) a abre por conta própria.
    fn take_share(&mut self, _transmitter: TransmitterId) {}
    /// `DynamicNetwork.onUpdate`, no `ServerTickEvent.Post` quando o tick roda normalmente, na ordem do `networks`.
    fn on_update(&mut self, _registry: &mut TransmitterNetworkRegistry<A>, _network: NetworkId) {}
    /// O que uma subclasse acrescenta a um método da rede ([`NetworkEvent`]).
    ///
    /// Recebe o registro para ler (os transmissores da rede, em ordem) o que a subclasse lê de `getTransmitters()` ou de `isEmpty()`.
    fn network_event(
        &mut self,
        _registry: &TransmitterNetworkRegistry<A>,
        _network: NetworkId,
        _event: NetworkEvent,
    ) {
    }
}
