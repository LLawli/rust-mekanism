//! `ConnectionType` e `TransmissionType`, e as contas de byte de conexão do `Transmitter`.

use crate::sides::Face;

/// `mekanism.common.lib.transmitter.ConnectionType`, na ordem do `ordinal` (que é o que o NBT e o
/// pacote guardam).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConnectionType {
    Normal,
    Push,
    Pull,
    None,
}

impl ConnectionType {
    /// `ConnectionType.VALUES`.
    pub const VALUES: [ConnectionType; 4] = [
        ConnectionType::Normal,
        ConnectionType::Push,
        ConnectionType::Pull,
        ConnectionType::None,
    ];

    pub fn ordinal(self) -> usize {
        match self {
            ConnectionType::Normal => 0,
            ConnectionType::Push => 1,
            ConnectionType::Pull => 2,
            ConnectionType::None => 3,
        }
    }

    /// `ConnectionType.BY_ID` (`ByIdMap.continuous` com `OutOfBoundsStrategy.ZERO`): fora de 0..4
    /// dá o primeiro, `NORMAL`.
    pub fn by_id(id: i32) -> ConnectionType {
        usize::try_from(id)
            .ok()
            .and_then(|index| Self::VALUES.get(index).copied())
            .unwrap_or(ConnectionType::Normal)
    }

    /// `getSerializedName`: o nome em minúsculas.
    pub fn serialized_name(self) -> &'static str {
        match self {
            ConnectionType::Normal => "normal",
            ConnectionType::Push => "push",
            ConnectionType::Pull => "pull",
            ConnectionType::None => "none",
        }
    }

    /// O nome do `EnumColor` do texto (`langEntry.translateColored(color)`).
    pub fn color_name(self) -> &'static str {
        match self {
            ConnectionType::Normal => "ORANGE",
            ConnectionType::Push => "BRIGHT_GREEN",
            ConnectionType::Pull => "YELLOW",
            ConnectionType::None => "WHITE",
        }
    }

    /// O fim da chave de tradução (`connection.<mod>.<este>`): o mod entra de fora.
    pub fn lang_name(self) -> &'static str {
        self.serialized_name()
    }

    /// `canAccept`: deixa outras coisas inserirem nele.
    pub fn can_accept(self) -> bool {
        matches!(self, ConnectionType::Normal | ConnectionType::Pull)
    }

    /// `canSendTo`: pode mandar para outras coisas ou deixar que extraiam dele.
    pub fn can_send_to(self) -> bool {
        matches!(self, ConnectionType::Normal | ConnectionType::Push)
    }

    /// `IIncrementalEnum.byIndexWrapping`.
    pub fn by_index_wrapping(index: i32) -> ConnectionType {
        Self::VALUES[index.rem_euclid(Self::VALUES.len() as i32) as usize]
    }

    /// `getNext()`: o próximo do ciclo do Configurator.
    pub fn get_next(self) -> ConnectionType {
        self.get_next_valid(|_| true)
    }

    /// `getNext(isValid)`: o próximo que a regra aceita; se der a volta sem achar, devolve este.
    pub fn get_next_valid(self, is_valid: impl Fn(ConnectionType) -> bool) -> ConnectionType {
        let mut next = Self::by_index_wrapping(self.ordinal() as i32 + 1);
        while !is_valid(next) {
            if next == self {
                return next;
            }
            next = Self::by_index_wrapping(next.ordinal() as i32 + 1);
        }
        next
    }

    /// `getPrevious()`.
    pub fn get_previous(self) -> ConnectionType {
        self.get_previous_valid(|_| true)
    }

    /// `getPrevious(isValid)`.
    pub fn get_previous_valid(self, is_valid: impl Fn(ConnectionType) -> bool) -> ConnectionType {
        let mut previous = Self::by_index_wrapping(self.ordinal() as i32 - 1);
        while !is_valid(previous) {
            if previous == self {
                return previous;
            }
            previous = Self::by_index_wrapping(previous.ordinal() as i32 - 1);
        }
        previous
    }

    /// `adjust(shift)`: o elemento deslocado, para os dois lados.
    pub fn adjust(self, shift: i32) -> ConnectionType {
        if shift == 0 {
            self
        } else {
            Self::by_index_wrapping(self.ordinal() as i32 + shift)
        }
    }
}

/// `mekanism.common.lib.transmitter.TransmissionType`, na ordem do `ordinal`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TransmissionType {
    Energy,
    Fluid,
    Chemical,
    Item,
    Heat,
}

impl TransmissionType {
    pub const VALUES: [TransmissionType; 5] = [
        TransmissionType::Energy,
        TransmissionType::Fluid,
        TransmissionType::Chemical,
        TransmissionType::Item,
        TransmissionType::Heat,
    ];

    pub fn ordinal(self) -> usize {
        match self {
            TransmissionType::Energy => 0,
            TransmissionType::Fluid => 1,
            TransmissionType::Chemical => 2,
            TransmissionType::Item => 3,
            TransmissionType::Heat => 4,
        }
    }

    /// `TransmissionType.BY_ID` (`OutOfBoundsStrategy.WRAP`).
    pub fn by_id(id: i32) -> TransmissionType {
        Self::VALUES[id.rem_euclid(Self::VALUES.len() as i32) as usize]
    }

    /// `name()`, o nome do enum em maiúsculas.
    pub fn enum_name(self) -> &'static str {
        match self {
            TransmissionType::Energy => "ENERGY",
            TransmissionType::Fluid => "FLUID",
            TransmissionType::Chemical => "CHEMICAL",
            TransmissionType::Item => "ITEM",
            TransmissionType::Heat => "HEAT",
        }
    }

    /// `getName()`: o nome da classe da rede.
    pub fn network_name(self) -> &'static str {
        match self {
            TransmissionType::Energy => "EnergyNetwork",
            TransmissionType::Fluid => "FluidNetwork",
            TransmissionType::Chemical => "ChemicalNetwork",
            TransmissionType::Item => "InventoryNetwork",
            TransmissionType::Heat => "HeatNetwork",
        }
    }

    /// `getTransmission()`, que também é o `getSerializedName()`.
    pub fn transmission(self) -> &'static str {
        match self {
            TransmissionType::Energy => "energy",
            TransmissionType::Fluid => "fluids",
            TransmissionType::Chemical => "chemicals",
            TransmissionType::Item => "items",
            TransmissionType::Heat => "heat",
        }
    }

    /// O caminho de `guiTexture()` (`transmission/<tipo>`), sem o namespace do mod.
    pub fn gui_texture_path(self) -> String {
        format!("transmission/{}", self.transmission())
    }

    pub fn bit(self) -> u8 {
        1 << self.ordinal()
    }
}

/// Um `EnumSet<TransmissionType>` (`supportedTransmissionTypes`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransmissionSet(u8);

impl TransmissionSet {
    pub fn of(types: &[TransmissionType]) -> TransmissionSet {
        TransmissionSet(types.iter().fold(0, |bits, kind| bits | kind.bit()))
    }

    pub fn contains(self, kind: TransmissionType) -> bool {
        self.0 & kind.bit() != 0
    }

    /// `supportsTransmissionType(other)`: algum tipo em comum.
    pub fn intersects(self, other: TransmissionSet) -> bool {
        self.0 & other.0 != 0
    }

    pub fn iter(self) -> impl Iterator<Item = TransmissionType> {
        TransmissionType::VALUES
            .into_iter()
            .filter(move |kind| self.contains(*kind))
    }
}

/// `Transmitter.connectionMapContainsSide`.
pub fn connection_map_contains_side(connections: u8, side: Face) -> bool {
    connections & side_bit(side) != 0
}

/// O bit de um lado: `1 << ordinal`.
pub fn side_bit(side: Face) -> u8 {
    1 << face_ordinal(side)
}

/// O `ordinal` do `Direction`.
pub fn face_ordinal(side: Face) -> usize {
    Face::ALL.iter().position(|face| *face == side).unwrap_or(0)
}

/// `Transmitter.setConnectionBit`.
pub fn set_connection_bit(connections: u8, to_set: bool, side: Face) -> u8 {
    let bit = side_bit(side);
    (connections & !bit) | if to_set { bit } else { 0 }
}

/// `Transmitter.getConnectionType(side, all, transmitters, types)`: o que o lado mostra. Sem bit em
/// `all` é `NONE`; com um transmissor do outro lado é `NORMAL`, seja qual for o tipo cru; no resto
/// vale o tipo cru.
pub fn get_connection_type(
    side: Face,
    all_connections: u8,
    transmitter_connections: u8,
    types: &[ConnectionType; 6],
) -> ConnectionType {
    if !connection_map_contains_side(all_connections, side) {
        ConnectionType::None
    } else if connection_map_contains_side(transmitter_connections, side) {
        ConnectionType::Normal
    } else {
        types[face_ordinal(side)]
    }
}

/// `Transmitter.isConnectionType`.
pub fn is_connection_type(
    side: Face,
    all_connections: u8,
    transmitter_connections: u8,
    types: &[ConnectionType; 6],
    type_to_check: ConnectionType,
) -> bool {
    get_connection_type(side, all_connections, transmitter_connections, types) == type_to_check
}
