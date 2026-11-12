//! O que a busca lê do mundo e dos transportadores: [`Transporter`] (`LogisticalTransporterBase`, o que a busca consulta dele),
//! [`InventoryNetworkView`] (a `InventoryNetwork`: os transportadores por posição e os aceitadores) e [`TransporterWorld`] (o `Level`:
//! a configuração de lados e o ejetor avançado de um bloco, e o que ele expõe de itens). Tudo só leitura.
use crate::containers::ItemResource;
use crate::item_handler::ItemHandler;
use crate::sides::Face;
use crate::transaction::Transaction;
use crate::transmission::Uuid;
use crate::transmission::connection::ConnectionType;
use std::rc::Rc;

/// O que a busca lê de um `LogisticalTransporterBase`.
pub trait Transporter {
    /// `getColor`.
    fn color(&self) -> Option<u8>;
    /// `getCost`.
    fn cost(&self) -> f64;
    /// `canConnect(side)` (o do desviador soma o modo e o sinal).
    fn can_connect(&self, side: Face) -> bool;
    /// `getConnectionType(side)`.
    fn connection_type(&self, side: Face) -> ConnectionType;
    /// `canEmitTo(side)`: `canConnect(side) && getConnectionType(side).canSendTo()`.
    fn can_emit_to(&self, side: Face) -> bool {
        self.can_connect(side) && self.connection_type(side).can_send_to()
    }
    /// `canReceiveFrom(side)`.
    fn can_receive_from(&self, side: Face) -> bool {
        self.can_connect(side) && self.connection_type(side).can_accept()
    }
    /// `canConnectMutual(side, transmitterFrom)`: o vizinho nulo (outro bloco ali) conta como se ligasse.
    fn can_connect_mutual(&self, side: Face, other: Option<&dyn Transporter>) -> bool {
        self.can_connect(side) && other.is_none_or(|other| other.can_connect(side.opposite()))
    }
}

/// A `InventoryNetwork`, no que a busca lê.
pub trait InventoryNetworkView {
    /// `getUUID`: a chave da cache de caminhos.
    fn uuid(&self) -> Uuid;
    /// `getTransmitter(pos)`.
    fn transmitter(&self, pos: i64) -> Option<&dyn Transporter>;
    /// `acceptorCache.getAcceptorFastIterator()`: as posições (`BlockPos.asLong`) na ordem da tabela, cada uma com os aceitadores dos
    /// seus lados na ordem do `Direction` (o lado do aceitador voltado para a rede).
    fn acceptors(&self) -> Vec<(i64, Vec<(Face, Rc<dyn ItemHandler>)>)>;
}

/// `ISideConfiguration`, no que a busca lê: se a entrada é estrita e a cor de entrada de cada lado do mundo (já passada por
/// `RelativeSide.fromDirections(direction, side)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SideConfigView {
    /// `getEjector().hasStrictInput()`.
    pub strict_input: bool,
    /// `getEjector().getInputColor(RelativeSide.fromDirections(getDirection(), side))`, por lado do mundo (índice do `Direction`).
    pub input_colors: [Option<u8>; 6],
}

impl SideConfigView {
    pub fn input_color(&self, side: Face) -> Option<u8> {
        self.input_colors[side_index(side)]
    }
}

/// `IAdvancedTransportEjector` num bloco de destino (o Logistical Sorter): o que a busca pergunta a ele quando a pilha volta para casa.
pub trait TransportSorter {
    /// `canSendHome(level, itemType, amount, transaction)`.
    fn can_send_home(
        &self,
        item: &ItemResource,
        amount: u32,
        transaction: Option<&Transaction>,
    ) -> bool;
}

/// O `Level` no que a busca lê de um bloco de destino.
pub trait TransporterWorld {
    /// O bloco em `pos` é um `ISideConfiguration`?
    fn side_configuration(&self, pos: i64) -> Option<SideConfigView>;
    /// O bloco em `pos` é um `IAdvancedTransportEjector`?
    fn sorter(&self, pos: i64) -> Option<&dyn TransportSorter>;
    /// `Capabilities.ITEM.getCapabilityIfLoaded(level, pos, null, tile, side)`.
    fn item_handler(&self, pos: i64, side: Face) -> Option<Rc<dyn ItemHandler>>;
}

/// `TransporterUtils.canInsert(level, pos, tile, color, itemType, itemAmount, side, force, transaction)`.
#[allow(clippy::too_many_arguments)]
pub fn can_insert(
    world: &dyn TransporterWorld,
    pos: i64,
    color: Option<u8>,
    item: &ItemResource,
    amount: u32,
    side: Face,
    force: bool,
    transaction: Option<&Transaction>,
) -> bool {
    if amount == 0 {
        // Não devia chegar zerado, mas se chegar não há nada a inserir.
        return true;
    }
    if force && let Some(sorter) = world.sorter(pos) {
        return sorter.can_send_home(item, amount, transaction);
    }
    if !force
        && let Some(config) = world.side_configuration(pos)
        && config.strict_input
    {
        let config_color = config.input_color(side.opposite());
        if config_color.is_some() && config_color != color {
            return false;
        }
    }
    let Some(inventory) = world.item_handler(pos, side.opposite()) else {
        return false;
    };
    // Simula a inserção: ela valida o item e vê se ao menos parte cabe.
    let simulation = Transaction::open(transaction);
    inventory.insert(item, amount, &simulation) > 0
}

/// O índice de um lado no `Direction`.
pub fn side_index(side: Face) -> usize {
    Face::ALL
        .iter()
        .position(|face| *face == side)
        .expect("Face::ALL tem os seis lados")
}
