//! A configuração de lados por instância: `RelativeSide`, `DataType`, `TransmissionType`, `ISlotInfo`, `ConfigInfo`,
//! `TileComponentConfig` e `AttachedSideConfig`, do Mekanism. A lib só conhece o mecanismo: o que cada máquina tem (quais tipos de
//! transmissão, quais `DataType`, quais contêineres em cada um) entra como dado, na hora de montar o componente.
//!
//! Um contêiner (slot, tanque, recipiente de energia, capacitor) é um número: o índice dele na tabela de contêineres do tipo, na ordem
//! de `getInitialInventory` e companhia. As variantes `*Proxy` do Java (`IProxiedSlotInfo`), que resolvem o contêiner por um
//! `Supplier` na hora, são o mesmo `SlotInfo`: o índice só vira o contêiner de verdade quando quem pergunta tem a tabela.
use crate::sides::{Face, FaceSlots};
use std::collections::BTreeMap;

/// `RelativeSide`, na ordem do `ordinal` do Java: a frente, a esquerda, a direita, as costas, o alto e a base.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RelativeSide {
    Front,
    Left,
    Right,
    Back,
    Top,
    Bottom,
}

impl RelativeSide {
    pub const VALUES: [RelativeSide; 6] = [
        RelativeSide::Front,
        RelativeSide::Left,
        RelativeSide::Right,
        RelativeSide::Back,
        RelativeSide::Top,
        RelativeSide::Bottom,
    ];

    pub fn ordinal(self) -> usize {
        self as usize
    }

    /// `RelativeSide.BY_ID`: o índice dá a volta (`OutOfBoundsStrategy.WRAP`).
    pub fn by_id(index: i64) -> RelativeSide {
        Self::VALUES[index.rem_euclid(6) as usize]
    }

    pub fn serialized_name(self) -> &'static str {
        match self {
            RelativeSide::Front => "front",
            RelativeSide::Left => "left",
            RelativeSide::Right => "right",
            RelativeSide::Back => "back",
            RelativeSide::Top => "top",
            RelativeSide::Bottom => "bottom",
        }
    }

    pub fn from_serialized_name(name: &str) -> Option<RelativeSide> {
        Self::VALUES
            .into_iter()
            .find(|side| side.serialized_name() == name)
    }

    /// `RelativeSide.getDirection(facing)`: a face do mundo que este lado é, para um bloco virado para `facing`.
    pub fn direction(self, facing: Face) -> Face {
        let vertical = matches!(facing, Face::Down | Face::Up);
        match self {
            RelativeSide::Front => facing,
            RelativeSide::Back => facing.opposite(),
            // `getClockWise` e `getCounterClockWise`; de cima ou de baixo, o bloco é visto como se olhasse para o norte e girasse no eixo X.
            RelativeSide::Left if vertical => Face::East,
            RelativeSide::Left => facing.left_of(),
            RelativeSide::Right if vertical => Face::West,
            RelativeSide::Right => facing.right_of(),
            RelativeSide::Top => match facing {
                Face::Down => Face::North,
                Face::Up => Face::South,
                _ => Face::Up,
            },
            RelativeSide::Bottom => match facing {
                Face::Down => Face::South,
                Face::Up => Face::North,
                _ => Face::Down,
            },
        }
    }

    /// `RelativeSide.fromDirections(facing, side)`.
    pub fn from_directions(facing: Face, side: Face) -> RelativeSide {
        if side == facing {
            return RelativeSide::Front;
        }
        if side == facing.opposite() {
            return RelativeSide::Back;
        }
        if matches!(facing, Face::Down | Face::Up) {
            let down = facing == Face::Down;
            return match side {
                Face::North if down => RelativeSide::Top,
                Face::North => RelativeSide::Bottom,
                Face::South if down => RelativeSide::Bottom,
                Face::South => RelativeSide::Top,
                Face::West => RelativeSide::Right,
                // `Face::East`. O Java lança `IllegalStateException` para o resto, que já saiu acima (a frente e as costas).
                _ => RelativeSide::Left,
            };
        }
        match side {
            Face::Down => RelativeSide::Bottom,
            Face::Up => RelativeSide::Top,
            _ if side == facing.right_of() => RelativeSide::Right,
            _ if side == facing.left_of() => RelativeSide::Left,
            // "Fall back to front, should never get here".
            _ => RelativeSide::Front,
        }
    }
}

/// `TransmissionType`, na ordem do `ordinal`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TransmissionType {
    Energy,
    Fluid,
    Chemical,
    Item,
    Heat,
}

impl TransmissionType {
    /// `TransmissionType.VALUES`.
    pub const VALUES: [TransmissionType; 5] = [
        TransmissionType::Energy,
        TransmissionType::Fluid,
        TransmissionType::Chemical,
        TransmissionType::Item,
        TransmissionType::Heat,
    ];

    pub fn ordinal(self) -> usize {
        self as usize
    }

    /// `getSerializedName`: o `transmission` do Java (`energy`, `fluids`, `chemicals`, `items`, `heat`).
    pub fn serialized_name(self) -> &'static str {
        match self {
            TransmissionType::Energy => "energy",
            TransmissionType::Fluid => "fluids",
            TransmissionType::Chemical => "chemicals",
            TransmissionType::Item => "items",
            TransmissionType::Heat => "heat",
        }
    }

    pub fn from_serialized_name(name: &str) -> Option<TransmissionType> {
        Self::VALUES
            .into_iter()
            .find(|kind| kind.serialized_name() == name)
    }
}

/// Os nomes de `EnumColor` (o `getSerializedName`, que é o prefixo do registro), na ordem do `ordinal`.
pub const ENUM_COLOR_NAMES: [&str; 18] = [
    "black",
    "blue",
    "green",
    "cyan",
    "dark_red",
    "purple",
    "orange",
    "light_gray",
    "gray",
    "light_blue",
    "lime",
    "aqua",
    "red",
    "magenta",
    "yellow",
    "white",
    "brown",
    "pink",
];

/// `EnumColor.DARK_RED` e as outras que `DataType` usa, pelo `ordinal`.
const COLOR_DARK_BLUE: u8 = 1;
const COLOR_DARK_GREEN: u8 = 2;
const COLOR_DARK_AQUA: u8 = 3;
const COLOR_DARK_RED: u8 = 4;
const COLOR_PURPLE: u8 = 5;
const COLOR_ORANGE: u8 = 6;
const COLOR_GRAY: u8 = 7;
const COLOR_YELLOW: u8 = 14;

/// `DataType`, na ordem do `ordinal` (que é o que o NBT guarda).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DataType {
    None,
    Input,
    Input1,
    Input2,
    Output,
    Output1,
    Output2,
    InputOutput,
    Energy,
    Extra,
}

impl DataType {
    pub const VALUES: [DataType; 10] = [
        DataType::None,
        DataType::Input,
        DataType::Input1,
        DataType::Input2,
        DataType::Output,
        DataType::Output1,
        DataType::Output2,
        DataType::InputOutput,
        DataType::Energy,
        DataType::Extra,
    ];

    pub fn ordinal(self) -> usize {
        self as usize
    }

    /// `DataType.BY_ID`: fora do intervalo vale `NONE` (`OutOfBoundsStrategy.ZERO`).
    pub fn by_id(id: i64) -> DataType {
        usize::try_from(id)
            .ok()
            .and_then(|id| Self::VALUES.get(id).copied())
            .unwrap_or(DataType::None)
    }

    /// `byIndexWrapping`.
    fn by_index_wrapping(index: i64) -> DataType {
        Self::VALUES[index.rem_euclid(Self::VALUES.len() as i64) as usize]
    }

    /// `getColor`: o `ordinal` do `EnumColor`.
    pub fn color(self) -> u8 {
        match self {
            DataType::None => COLOR_GRAY,
            DataType::Input | DataType::Input1 => COLOR_DARK_RED,
            DataType::Input2 => COLOR_ORANGE,
            DataType::Output | DataType::Output1 => COLOR_DARK_BLUE,
            DataType::Output2 => COLOR_DARK_AQUA,
            DataType::InputOutput => COLOR_PURPLE,
            DataType::Energy => COLOR_DARK_GREEN,
            DataType::Extra => COLOR_YELLOW,
        }
    }

    /// `DataType.canOutput`.
    pub fn can_output(self) -> bool {
        matches!(
            self,
            DataType::Output | DataType::InputOutput | DataType::Output1 | DataType::Output2
        )
    }

    pub fn serialized_name(self) -> &'static str {
        match self {
            DataType::None => "none",
            DataType::Input => "input",
            DataType::Input1 => "input_1",
            DataType::Input2 => "input_2",
            DataType::Output => "output",
            DataType::Output1 => "output_1",
            DataType::Output2 => "output_2",
            DataType::InputOutput => "input_output",
            DataType::Energy => "energy",
            DataType::Extra => "extra",
        }
    }

    pub fn from_serialized_name(name: &str) -> Option<DataType> {
        Self::VALUES
            .into_iter()
            .find(|kind| kind.serialized_name() == name)
    }

    /// `IIncrementalEnum.getNext(isValid)`: o próximo válido, dando a volta; se der a volta inteira, ele mesmo.
    pub fn next(self, is_valid: impl Fn(DataType) -> bool) -> DataType {
        let mut next = Self::by_index_wrapping(self.ordinal() as i64 + 1);
        while !is_valid(next) {
            if next == self {
                return next;
            }
            next = Self::by_index_wrapping(next.ordinal() as i64 + 1);
        }
        next
    }

    /// `IIncrementalEnum.getPrevious(isValid)`.
    pub fn previous(self, is_valid: impl Fn(DataType) -> bool) -> DataType {
        let mut previous = Self::by_index_wrapping(self.ordinal() as i64 - 1);
        while !is_valid(previous) {
            if previous == self {
                return previous;
            }
            previous = Self::by_index_wrapping(previous.ordinal() as i64 - 1);
        }
        previous
    }
}

/// `ISlotInfo` e suas variantes (`InventorySlotInfo`, `FluidSlotInfo`, `ChemicalSlotInfo`, `EnergySlotInfo`, `HeatSlotInfo`): se o
/// `DataType` deixa entrar e sair, e quais contêineres do tipo ele expõe, na ordem em que foram dados.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotInfo {
    pub kind: TransmissionType,
    pub can_input: bool,
    pub can_output: bool,
    pub containers: Vec<usize>,
}

impl SlotInfo {
    pub fn new(
        kind: TransmissionType,
        can_input: bool,
        can_output: bool,
        containers: Vec<usize>,
    ) -> Self {
        Self {
            kind,
            can_input,
            can_output,
            containers,
        }
    }

    /// `ISlotInfo.isEnabled`.
    pub fn is_enabled(&self) -> bool {
        self.can_input || self.can_output
    }

    /// `ISlotInfo.isEmpty`: nada a tirar. `is_empty(kind, container)` diz se o contêiner está vazio; o capacitor de calor nunca está
    /// (`HeatSlotInfo.isEmpty`), e um recipiente de energia que falta (`container == null`) conta como vazio.
    pub fn is_empty(&self, is_empty: &dyn Fn(TransmissionType, usize) -> bool) -> bool {
        match self.kind {
            TransmissionType::Heat => false,
            TransmissionType::Energy => self
                .containers
                .first()
                .is_none_or(|&container| is_empty(self.kind, container)),
            kind => self
                .containers
                .iter()
                .all(|&container| is_empty(kind, container)),
        }
    }
}

/// `ConfigInfo`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigInfo {
    can_eject: bool,
    ejecting: bool,
    side_config: [DataType; 6],
    slot_info: BTreeMap<DataType, SlotInfo>,
    /// `containerTypeMapping`: por contêiner, os `DataType` que o têm, na ordem em que `addSlotInfo` os viu.
    container_types: BTreeMap<usize, Vec<DataType>>,
    disabled_sides: Option<[bool; 6]>,
}

impl Default for ConfigInfo {
    /// `new ConfigInfo()`: cada lado em `NONE`, e pode ejetar mas não ejeta.
    fn default() -> Self {
        Self {
            can_eject: true,
            ejecting: false,
            side_config: [DataType::None; 6],
            slot_info: BTreeMap::new(),
            container_types: BTreeMap::new(),
            disabled_sides: None,
        }
    }
}

impl ConfigInfo {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn can_eject(&self) -> bool {
        self.can_eject
    }

    pub fn set_can_eject(&mut self, can_eject: bool) {
        self.can_eject = can_eject;
    }

    pub fn is_ejecting(&self) -> bool {
        self.ejecting
    }

    pub fn set_ejecting(&mut self, ejecting: bool) {
        self.ejecting = ejecting;
    }

    /// `addDisabledSides`: o lado volta a `NONE` e não muda mais.
    pub fn add_disabled_sides(&mut self, sides: &[RelativeSide]) {
        let disabled = self.disabled_sides.get_or_insert([false; 6]);
        for side in sides {
            disabled[side.ordinal()] = true;
            self.side_config[side.ordinal()] = DataType::None;
        }
    }

    pub fn is_side_enabled(&self, side: RelativeSide) -> bool {
        self.disabled_sides
            .is_none_or(|disabled| !disabled[side.ordinal()])
    }

    pub fn data_type(&self, side: RelativeSide) -> DataType {
        self.side_config[side.ordinal()]
    }

    /// `getSideConfig`, na ordem do `EnumMap`: a ordem de `RelativeSide`.
    pub fn side_config(&self) -> impl Iterator<Item = (RelativeSide, DataType)> + '_ {
        RelativeSide::VALUES
            .into_iter()
            .map(|side| (side, self.side_config[side.ordinal()]))
    }

    /// `setDataType`: se o lado mudou. Não confere se o `DataType` é um dos que a máquina tem, como o Java; um lado em um tipo sem
    /// `SlotInfo` fica sem capability (`getSlotInfo` é nulo).
    pub fn set_data_type(&mut self, data_type: DataType, side: RelativeSide) -> bool {
        if !self.is_side_enabled(side) {
            return false;
        }
        let old = std::mem::replace(&mut self.side_config[side.ordinal()], data_type);
        old != data_type
    }

    /// `getSupportedDataTypes`: `NONE` e os que têm `SlotInfo`, na ordem do `ordinal` (um `EnumSet`).
    pub fn supported_data_types(&self) -> Vec<DataType> {
        let mut types = vec![DataType::None];
        types.extend(
            self.slot_info
                .keys()
                .copied()
                .filter(|&kind| kind != DataType::None),
        );
        types
    }

    pub fn supports(&self, data_type: DataType) -> bool {
        data_type == DataType::None || self.slot_info.contains_key(&data_type)
    }

    pub fn slot_info_of(&self, data_type: DataType) -> Option<&SlotInfo> {
        self.slot_info.get(&data_type)
    }

    /// `getSlotInfo(side)`.
    pub fn slot_info(&self, side: RelativeSide) -> Option<&SlotInfo> {
        self.slot_info_of(self.data_type(side))
    }

    /// `addSlotInfo`: troca o do `DataType`, e o mapa de contêineres só cresce, como no Java.
    pub fn add_slot_info(&mut self, data_type: DataType, info: SlotInfo) {
        for &container in &info.containers {
            self.container_types
                .entry(container)
                .or_default()
                .push(data_type);
        }
        self.slot_info.insert(data_type, info);
    }

    /// `getDataTypeForContainer`.
    pub fn data_types_for_container(&self, container: usize) -> Vec<DataType> {
        self.container_types
            .get(&container)
            .cloned()
            .unwrap_or_default()
    }

    /// `incrementDataType`: devolve o novo.
    pub fn increment_data_type(&mut self, side: RelativeSide) -> DataType {
        let current = self.data_type(side);
        if self.is_side_enabled(side) {
            let next = current.next(|kind| self.supports(kind));
            self.side_config[side.ordinal()] = next;
            return next;
        }
        current
    }

    /// `decrementDataType`.
    pub fn decrement_data_type(&mut self, side: RelativeSide) -> DataType {
        let current = self.data_type(side);
        if self.is_side_enabled(side) {
            let previous = current.previous(|kind| self.supports(kind));
            self.side_config[side.ordinal()] = previous;
            return previous;
        }
        current
    }
}

/// `LightConfigInfo`: o que o item guarda de um `ConfigInfo`. Só entram os lados que não são `NONE`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LightConfigInfo {
    pub side_config: BTreeMap<RelativeSide, DataType>,
    pub ejecting: bool,
}

impl LightConfigInfo {
    pub fn new(sides: &[(RelativeSide, DataType)], ejecting: bool) -> Self {
        Self {
            side_config: sides.iter().copied().collect(),
            ejecting,
        }
    }

    /// `LightConfigInfo.create(output, extra, energy, ejecting)`: todos os lados em `INPUT`, e os três dados em outros tipos.
    pub fn create(
        output: Option<RelativeSide>,
        extra: Option<RelativeSide>,
        energy: Option<RelativeSide>,
        ejecting: bool,
    ) -> Self {
        let mut side_config: BTreeMap<_, _> = RelativeSide::VALUES
            .into_iter()
            .map(|side| (side, DataType::Input))
            .collect();
        for (side, data_type) in [
            (output, DataType::Output),
            (extra, DataType::Extra),
            (energy, DataType::Energy),
        ] {
            if let Some(side) = side {
                side_config.insert(side, data_type);
            }
        }
        Self {
            side_config,
            ejecting,
        }
    }

    /// `LightConfigInfo.without`.
    pub fn without(&self, sides: &[RelativeSide]) -> Self {
        let mut side_config = self.side_config.clone();
        for side in sides {
            side_config.remove(side);
        }
        Self {
            side_config,
            ejecting: self.ejecting,
        }
    }

    /// `getDataType`: o que não está no mapa é `NONE`.
    pub fn data_type(&self, side: RelativeSide) -> DataType {
        self.side_config
            .get(&side)
            .copied()
            .unwrap_or(DataType::None)
    }
}

/// `AttachedSideConfig`: o componente `mekanism:side_config` do item.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AttachedSideConfig {
    pub config_info: BTreeMap<TransmissionType, LightConfigInfo>,
}

impl AttachedSideConfig {
    pub fn new(entries: &[(TransmissionType, LightConfigInfo)]) -> Self {
        Self {
            config_info: entries.iter().cloned().collect(),
        }
    }
}

/// Uma mudança de lado: o que `sideChanged` e `sideChangedBasic` avisam (invalidar a capability da face, o pacote de atualização,
/// os ouvintes e salvar).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SideChange {
    pub transmission: TransmissionType,
    pub direction: Face,
}

/// O que `serialize` grava e `deserialize` lê de `component_config`: `eject<ordinal>` (booleano) e `config<ordinal>` (`int[]` de
/// `RelativeSide.VALUES`, o `ordinal` do `DataType`). Na leitura, o que falta não muda nada.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConfigData {
    pub eject: [Option<bool>; 5],
    pub config: [Option<Vec<i32>>; 5],
}

/// `TileComponentConfig`.
#[derive(Clone, Debug)]
pub struct TileComponentConfig {
    config_info: BTreeMap<TransmissionType, ConfigInfo>,
    /// `transmissionTypes`: na ordem em que a máquina os declarou (a tela os mostra assim).
    transmission_types: Vec<TransmissionType>,
    changes: Vec<SideChange>,
}

impl TileComponentConfig {
    /// `new TileComponentConfig(tile, types)`: um `ConfigInfo` novo, com tudo em `NONE`, por tipo.
    pub fn new(types: &[TransmissionType]) -> Self {
        let mut config_info = BTreeMap::new();
        let mut transmission_types = Vec::new();
        for &kind in types {
            if config_info.insert(kind, ConfigInfo::new()).is_none() {
                transmission_types.push(kind);
            }
        }
        Self {
            config_info,
            transmission_types,
            changes: Vec::new(),
        }
    }

    /// `getTransmissions`.
    pub fn transmissions(&self) -> &[TransmissionType] {
        &self.transmission_types
    }

    pub fn supports(&self, kind: TransmissionType) -> bool {
        self.config_info.contains_key(&kind)
    }

    pub fn config(&self, kind: TransmissionType) -> Option<&ConfigInfo> {
        self.config_info.get(&kind)
    }

    pub fn config_mut(&mut self, kind: TransmissionType) -> Option<&mut ConfigInfo> {
        self.config_info.get_mut(&kind)
    }

    fn config_nn(&mut self, kind: TransmissionType) -> &mut ConfigInfo {
        self.config_info
            .get_mut(&kind)
            .unwrap_or_else(|| panic!("Config of {} item not present", kind.serialized_name()))
    }

    pub fn add_disabled_sides(&mut self, sides: &[RelativeSide]) {
        for config in self.config_info.values_mut() {
            config.add_disabled_sides(sides);
        }
    }

    /// `setupInputConfig`: o `DataType` `INPUT`, que deixa entrar mas não sair, e a máquina não ejeta esse tipo.
    pub fn setup_input_config(&mut self, kind: TransmissionType, container: usize) {
        let config = self.config_nn(kind);
        config.add_slot_info(
            DataType::Input,
            SlotInfo::new(kind, true, false, vec![container]),
        );
        config.set_can_eject(false);
    }

    /// `setupOutputConfig`.
    pub fn setup_output_config(&mut self, kind: TransmissionType, container: usize) {
        let config = self.config_nn(kind);
        config.add_slot_info(
            DataType::Output,
            SlotInfo::new(kind, false, true, vec![container]),
        );
    }

    /// `setupIOConfig(type, inputContainer, outputContainer, alwaysAllowInput, alwaysAllowOutput)`.
    pub fn setup_io_config_two(
        &mut self,
        kind: TransmissionType,
        input: usize,
        output: usize,
        always_allow_input: bool,
        always_allow_output: bool,
    ) {
        let config = self.config_nn(kind);
        config.add_slot_info(
            DataType::Input,
            SlotInfo::new(kind, true, always_allow_output, vec![input]),
        );
        config.add_slot_info(
            DataType::Output,
            SlotInfo::new(kind, always_allow_input, true, vec![output]),
        );
        config.add_slot_info(
            DataType::InputOutput,
            SlotInfo::new(kind, true, true, vec![input, output]),
        );
    }

    /// `setupIOConfig(type, info, alwaysAllow)`: um só contêiner nos três.
    pub fn setup_io_config(
        &mut self,
        kind: TransmissionType,
        container: usize,
        always_allow: bool,
    ) {
        let config = self.config_nn(kind);
        config.add_slot_info(
            DataType::Input,
            SlotInfo::new(kind, true, always_allow, vec![container]),
        );
        config.add_slot_info(
            DataType::Output,
            SlotInfo::new(kind, always_allow, true, vec![container]),
        );
        config.add_slot_info(
            DataType::InputOutput,
            SlotInfo::new(kind, true, true, vec![container]),
        );
    }

    /// `setupItemIOConfig(inputSlots, outputSlots, energySlot, alwaysAllow)`.
    pub fn setup_item_io_config(
        &mut self,
        input_slots: &[usize],
        output_slots: &[usize],
        energy_slot: usize,
        always_allow: bool,
    ) {
        let kind = TransmissionType::Item;
        let config = self.config_nn(kind);
        config.add_slot_info(
            DataType::Input,
            SlotInfo::new(kind, true, always_allow, input_slots.to_vec()),
        );
        config.add_slot_info(
            DataType::Output,
            SlotInfo::new(kind, always_allow, true, output_slots.to_vec()),
        );
        let io: Vec<usize> = input_slots.iter().chain(output_slots).copied().collect();
        config.add_slot_info(DataType::InputOutput, SlotInfo::new(kind, true, true, io));
        config.add_slot_info(
            DataType::Energy,
            SlotInfo::new(kind, true, true, vec![energy_slot]),
        );
    }

    /// `setupItemIOExtraConfig`.
    pub fn setup_item_io_extra_config(
        &mut self,
        input_slot: usize,
        output_slot: usize,
        extra_slot: usize,
        energy_slot: usize,
    ) {
        let kind = TransmissionType::Item;
        let config = self.config_nn(kind);
        config.add_slot_info(
            DataType::Input,
            SlotInfo::new(kind, true, false, vec![input_slot]),
        );
        config.add_slot_info(
            DataType::Output,
            SlotInfo::new(kind, false, true, vec![output_slot]),
        );
        config.add_slot_info(
            DataType::InputOutput,
            SlotInfo::new(kind, true, true, vec![input_slot, output_slot]),
        );
        config.add_slot_info(
            DataType::Extra,
            SlotInfo::new(kind, true, true, vec![extra_slot]),
        );
        config.add_slot_info(
            DataType::Energy,
            SlotInfo::new(kind, true, true, vec![energy_slot]),
        );
    }

    /// `getDataType(type, side)`.
    pub fn data_type(&self, kind: TransmissionType, side: RelativeSide) -> Option<DataType> {
        self.config(kind).map(|info| info.data_type(side))
    }

    /// `getSlotInfo(type, direction)`: a do lado `direction` de uma máquina virada para `facing`. Sem face (a consulta interna), nada.
    pub fn slot_info(
        &self,
        kind: TransmissionType,
        facing: Face,
        direction: Option<Face>,
    ) -> Option<&SlotInfo> {
        let direction = direction?;
        self.config(kind)?
            .slot_info(RelativeSide::from_directions(facing, direction))
    }

    /// `isCapabilityDisabled`: a capability do tipo está desligada nesta face. Sem face (a leitura interna), ou sem configuração do
    /// tipo, nunca.
    pub fn is_capability_disabled(
        &self,
        kind: TransmissionType,
        facing: Face,
        side: Option<Face>,
    ) -> bool {
        let Some(info) = self.config(kind) else {
            return false;
        };
        let Some(side) = side else {
            return false;
        };
        match info.slot_info(RelativeSide::from_directions(facing, side)) {
            None => true,
            Some(slot_info) => !slot_info.is_enabled(),
        }
    }

    /// `ISideConfiguration.getActiveDataType(container)`: o tipo que pinta o contêiner na tela, se ele aparece em alguns tipos e não em
    /// todos.
    pub fn active_data_type(&self, kind: TransmissionType, container: usize) -> Option<DataType> {
        let info = self.config(kind)?;
        let types = info.data_types_for_container(container);
        let count = types.len();
        if count > 0 && count < info.supported_data_types().len() {
            return types.first().copied();
        }
        None
    }

    /// `ConfigHolder.getContainers(side)`, `canInsert` e `canExtract` dos itens, para quem está fora: os slots do `DataType` da face,
    /// para entrar se ele deixa entrar e para sair se deixa sair. Um `DataType` sem `SlotInfo`, ou com um desligado, não alcança
    /// nada (`defaultValue()`). Sem configuração de itens (`NO_CONFIG`), todos os `all_slots` entram e saem.
    pub fn item_face(&self, facing: Face, face: Face, all_slots: &[usize]) -> FaceSlots {
        let Some(info) = self.config(TransmissionType::Item) else {
            return FaceSlots {
                inputs: all_slots.to_vec(),
                outputs: all_slots.to_vec(),
            };
        };
        match info.slot_info(RelativeSide::from_directions(facing, face)) {
            Some(slot_info) if slot_info.is_enabled() => FaceSlots {
                inputs: if slot_info.can_input {
                    slot_info.containers.clone()
                } else {
                    Vec::new()
                },
                outputs: if slot_info.can_output {
                    slot_info.containers.clone()
                } else {
                    Vec::new()
                },
            },
            _ => FaceSlots::default(),
        }
    }

    fn side_changed_basic(&mut self, kind: TransmissionType, direction: Face) {
        self.changes.push(SideChange {
            transmission: kind,
            direction,
        });
    }

    /// Quem foi mudando desde a última chamada, para a máquina invalidar as capabilities, salvar e avisar o cliente.
    pub fn take_changes(&mut self) -> Vec<SideChange> {
        std::mem::take(&mut self.changes)
    }

    /// `setMode`: o modo do lado (o computador de Mekanism); só vale se o tipo o tem.
    pub fn set_mode(
        &mut self,
        kind: TransmissionType,
        side: RelativeSide,
        mode: DataType,
        facing: Face,
    ) -> bool {
        let Some(config) = self.config_info.get_mut(&kind) else {
            return false;
        };
        if !config.supports(mode) || config.data_type(side) == mode {
            return false;
        }
        config.set_data_type(mode, side);
        self.side_changed_basic(kind, side.direction(facing));
        true
    }

    /// `incrementMode`: o clique do Configurator.
    pub fn increment_mode(
        &mut self,
        kind: TransmissionType,
        side: RelativeSide,
        facing: Face,
    ) -> bool {
        let Some(config) = self.config_info.get_mut(&kind) else {
            return false;
        };
        let before = config.data_type(side);
        if before != config.increment_data_type(side) {
            self.side_changed_basic(kind, side.direction(facing));
            return true;
        }
        false
    }

    /// `decrementMode`.
    pub fn decrement_mode(
        &mut self,
        kind: TransmissionType,
        side: RelativeSide,
        facing: Face,
    ) -> bool {
        let Some(config) = self.config_info.get_mut(&kind) else {
            return false;
        };
        let before = config.data_type(side);
        if before != config.decrement_data_type(side) {
            self.side_changed_basic(kind, side.direction(facing));
            return true;
        }
        false
    }

    /// `applyImplicitComponents`: o item colocado traz a configuração. Só os lados que o item lista são tocados, e `facing` diz
    /// qual face do mundo cada um é (avisada só quando `loaded`: "if we aren't already loaded yet don't do any updates").
    pub fn apply_implicit_components(
        &mut self,
        attached: Option<&AttachedSideConfig>,
        facing: Face,
        loaded: bool,
    ) {
        let Some(attached) = attached else {
            return;
        };
        for (&kind, light) in &attached.config_info {
            let Some(info) = self.config_info.get_mut(&kind) else {
                continue;
            };
            info.set_ejecting(light.ejecting);
            let mut changed = Vec::new();
            for (&side, &data_type) in &light.side_config {
                if info.set_data_type(data_type, side) {
                    changed.push(side);
                }
            }
            if loaded {
                for side in changed {
                    self.side_changed_basic(kind, side.direction(facing));
                }
            }
        }
    }

    /// `collectImplicitComponents`: `AttachedSideConfig.create(configInfo)`. Os lados `NONE` não entram.
    pub fn collect_implicit_components(&self) -> AttachedSideConfig {
        let mut config_info = BTreeMap::new();
        for (&kind, info) in &self.config_info {
            let side_config = info
                .side_config()
                .filter(|&(_, data_type)| data_type != DataType::None)
                .collect();
            config_info.insert(
                kind,
                LightConfigInfo {
                    side_config,
                    ejecting: info.is_ejecting(),
                },
            );
        }
        AttachedSideConfig { config_info }
    }

    /// `deserialize` (a leitura de `component_config`): `eject<ordinal>` e `config<ordinal>`. `loaded` é como acima.
    pub fn deserialize(&mut self, data: &ConfigData, facing: Face, loaded: bool) {
        for (&kind, info) in &mut self.config_info {
            let ordinal = kind.ordinal();
            if let Some(ejecting) = data.eject[ordinal] {
                info.set_ejecting(ejecting);
            }
            let Some(sides) = &data.config[ordinal] else {
                continue;
            };
            for (side, &id) in RelativeSide::VALUES.into_iter().zip(sides) {
                if info.set_data_type(DataType::by_id(i64::from(id)), side) && loaded {
                    self.changes.push(SideChange {
                        transmission: kind,
                        direction: side.direction(facing),
                    });
                }
            }
        }
    }

    /// `serialize` (a escrita de `component_config`, `full`): o ejetar e os seis lados de cada tipo.
    pub fn serialize(&self) -> ConfigData {
        let mut data = ConfigData::default();
        for (&kind, info) in &self.config_info {
            let ordinal = kind.ordinal();
            data.eject[ordinal] = Some(info.is_ejecting());
            data.config[ordinal] = Some(
                RelativeSide::VALUES
                    .into_iter()
                    .map(|side| info.data_type(side).ordinal() as i32)
                    .collect(),
            );
        }
        data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HORIZONTAL: [Face; 4] = [Face::North, Face::South, Face::West, Face::East];

    #[test]
    fn a_relative_side_maps_to_one_direction_and_back() {
        for facing in Face::ALL {
            let mut seen = Vec::new();
            for side in RelativeSide::VALUES {
                let direction = side.direction(facing);
                assert_eq!(
                    RelativeSide::from_directions(facing, direction),
                    side,
                    "{facing:?} {side:?}"
                );
                seen.push(direction);
            }
            seen.sort_by_key(|face| *face as usize);
            seen.dedup();
            assert_eq!(seen.len(), 6, "{facing:?} deve cobrir as seis faces");
        }
    }

    #[test]
    fn the_horizontal_sides_follow_the_facing() {
        // `getClockWise` de norte é leste, e a esquerda é a do bloco olhando para fora.
        assert_eq!(RelativeSide::Left.direction(Face::North), Face::East);
        assert_eq!(RelativeSide::Right.direction(Face::North), Face::West);
        assert_eq!(RelativeSide::Right.direction(Face::East), Face::North);
        assert_eq!(RelativeSide::Left.direction(Face::South), Face::West);
        for facing in HORIZONTAL {
            assert_eq!(RelativeSide::Top.direction(facing), Face::Up);
            assert_eq!(RelativeSide::Bottom.direction(facing), Face::Down);
            assert_eq!(RelativeSide::Front.direction(facing), facing);
            assert_eq!(RelativeSide::Back.direction(facing), facing.opposite());
        }
    }

    #[test]
    fn a_vertical_facing_turns_around_the_x_axis() {
        assert_eq!(RelativeSide::Left.direction(Face::Up), Face::East);
        assert_eq!(RelativeSide::Right.direction(Face::Down), Face::West);
        assert_eq!(RelativeSide::Top.direction(Face::Down), Face::North);
        assert_eq!(RelativeSide::Top.direction(Face::Up), Face::South);
        assert_eq!(RelativeSide::Bottom.direction(Face::Down), Face::South);
        assert_eq!(RelativeSide::Bottom.direction(Face::Up), Face::North);
    }

    #[test]
    fn the_enum_orders_are_the_java_ordinals() {
        let names: Vec<_> = DataType::VALUES
            .iter()
            .map(|kind| kind.serialized_name())
            .collect();
        assert_eq!(
            names,
            [
                "none",
                "input",
                "input_1",
                "input_2",
                "output",
                "output_1",
                "output_2",
                "input_output",
                "energy",
                "extra"
            ]
        );
        assert_eq!(DataType::by_id(99), DataType::None);
        assert_eq!(DataType::by_id(-1), DataType::None);
        assert_eq!(RelativeSide::by_id(6), RelativeSide::Front);
        assert_eq!(RelativeSide::by_id(-1), RelativeSide::Bottom);
        assert_eq!(TransmissionType::Item.ordinal(), 3);
        assert_eq!(TransmissionType::Item.serialized_name(), "items");
    }

    #[test]
    fn the_next_data_type_skips_what_the_machine_lacks() {
        let supports = |kind| matches!(kind, DataType::None | DataType::Input | DataType::Output);
        assert_eq!(DataType::None.next(supports), DataType::Input);
        assert_eq!(DataType::Input.next(supports), DataType::Output);
        assert_eq!(DataType::Output.next(supports), DataType::None);
        assert_eq!(DataType::None.previous(supports), DataType::Output);
        assert_eq!(DataType::Input.previous(supports), DataType::None);
        // Nada válido além dele mesmo: não gira para sempre.
        assert_eq!(
            DataType::Input.next(|kind| kind == DataType::Input),
            DataType::Input
        );
    }

    fn electric() -> TileComponentConfig {
        let mut config =
            TileComponentConfig::new(&[TransmissionType::Item, TransmissionType::Energy]);
        config.setup_item_io_config(&[0], &[1], 2, false);
        config.setup_input_config(TransmissionType::Energy, 0);
        config
    }

    #[test]
    fn a_new_config_starts_in_none_and_ships_no_capability() {
        let config = electric();
        for side in RelativeSide::VALUES {
            assert_eq!(
                config.data_type(TransmissionType::Item, side),
                Some(DataType::None)
            );
        }
        for face in Face::ALL {
            assert!(config.is_capability_disabled(TransmissionType::Item, Face::North, Some(face)));
        }
        // A leitura interna (sem face) nunca é desligada.
        assert!(!config.is_capability_disabled(TransmissionType::Item, Face::North, None));
        assert!(!config.is_capability_disabled(
            TransmissionType::Heat,
            Face::North,
            Some(Face::Up)
        ));
    }

    #[test]
    fn the_item_setup_has_the_java_data_types_and_flags() {
        let config = electric();
        let item = config.config(TransmissionType::Item).unwrap();
        assert_eq!(
            item.supported_data_types(),
            [
                DataType::None,
                DataType::Input,
                DataType::Output,
                DataType::InputOutput,
                DataType::Energy
            ]
        );
        let input = item.slot_info_of(DataType::Input).unwrap();
        assert!(input.can_input && !input.can_output);
        let output = item.slot_info_of(DataType::Output).unwrap();
        assert!(!output.can_input && output.can_output);
        assert_eq!(
            item.slot_info_of(DataType::InputOutput).unwrap().containers,
            [0, 1]
        );
        let energy = config.config(TransmissionType::Energy).unwrap();
        assert_eq!(
            energy.supported_data_types(),
            [DataType::None, DataType::Input]
        );
        assert!(!energy.can_eject());
        assert!(item.can_eject());
    }

    #[test]
    fn the_extra_setup_adds_the_extra_type() {
        let mut config = TileComponentConfig::new(&[TransmissionType::Item]);
        config.setup_item_io_extra_config(1, 2, 0, 3);
        let item = config.config(TransmissionType::Item).unwrap();
        assert_eq!(item.slot_info_of(DataType::Extra).unwrap().containers, [0]);
        assert_eq!(item.slot_info_of(DataType::Input).unwrap().containers, [1]);
        assert_eq!(
            item.slot_info_of(DataType::InputOutput).unwrap().containers,
            [1, 2]
        );
        assert_eq!(
            item.data_types_for_container(1),
            [DataType::Input, DataType::InputOutput]
        );
    }

    #[test]
    fn the_active_type_is_the_first_unless_every_type_has_the_container() {
        let config = electric();
        // O slot de energia só está no `ENERGY`: o tipo dele pinta o slot.
        assert_eq!(
            config.active_data_type(TransmissionType::Item, 2),
            Some(DataType::Energy)
        );
        // O de entrada está em `INPUT` e `INPUT_OUTPUT`: dois dos cinco.
        assert_eq!(
            config.active_data_type(TransmissionType::Item, 0),
            Some(DataType::Input)
        );
        assert_eq!(config.active_data_type(TransmissionType::Item, 9), None);
    }

    #[test]
    fn a_chosen_data_type_the_machine_lacks_has_no_slot_info() {
        let mut config = electric();
        let info = config.config_mut(TransmissionType::Item).unwrap();
        // `setDataType` não confere o suporte.
        assert!(info.set_data_type(DataType::Extra, RelativeSide::Right));
        assert!(!info.set_data_type(DataType::Extra, RelativeSide::Right));
        assert!(config.is_capability_disabled(
            TransmissionType::Item,
            Face::North,
            Some(Face::West)
        ));
    }

    #[test]
    fn a_disabled_side_stays_in_none() {
        let mut config = electric();
        config.add_disabled_sides(&[RelativeSide::Top]);
        let info = config.config_mut(TransmissionType::Item).unwrap();
        assert!(!info.set_data_type(DataType::Input, RelativeSide::Top));
        assert_eq!(info.increment_data_type(RelativeSide::Top), DataType::None);
        assert!(info.set_data_type(DataType::Input, RelativeSide::Front));
    }

    #[test]
    fn increment_walks_the_supported_types_and_reports_the_direction() {
        let mut config = electric();
        assert!(config.increment_mode(TransmissionType::Item, RelativeSide::Right, Face::North));
        assert_eq!(
            config.data_type(TransmissionType::Item, RelativeSide::Right),
            Some(DataType::Input)
        );
        assert_eq!(
            config.take_changes(),
            [SideChange {
                transmission: TransmissionType::Item,
                direction: Face::West
            }]
        );
        assert!(config.take_changes().is_empty());
        for _ in 0..4 {
            assert!(config.increment_mode(
                TransmissionType::Item,
                RelativeSide::Right,
                Face::North
            ));
        }
        // Deu a volta: NONE, INPUT, OUTPUT, INPUT_OUTPUT, ENERGY, NONE.
        assert_eq!(
            config.data_type(TransmissionType::Item, RelativeSide::Right),
            Some(DataType::None)
        );
        assert!(config.decrement_mode(TransmissionType::Item, RelativeSide::Right, Face::North));
        assert_eq!(
            config.data_type(TransmissionType::Item, RelativeSide::Right),
            Some(DataType::Energy)
        );
    }

    #[test]
    fn set_mode_refuses_a_type_the_machine_lacks() {
        let mut config = electric();
        assert!(!config.set_mode(
            TransmissionType::Item,
            RelativeSide::Front,
            DataType::Extra,
            Face::East
        ));
        assert!(config.set_mode(
            TransmissionType::Item,
            RelativeSide::Front,
            DataType::Output,
            Face::East
        ));
        assert!(!config.set_mode(
            TransmissionType::Item,
            RelativeSide::Front,
            DataType::Output,
            Face::East
        ));
        assert!(!config.set_mode(
            TransmissionType::Fluid,
            RelativeSide::Front,
            DataType::Output,
            Face::East
        ));
        assert_eq!(config.take_changes().len(), 1);
    }

    #[test]
    fn the_capability_follows_the_facing() {
        let mut config = electric();
        config
            .config_mut(TransmissionType::Item)
            .unwrap()
            .set_data_type(DataType::Output, RelativeSide::Right);
        for (facing, output) in [
            (Face::North, Face::West),
            (Face::East, Face::North),
            (Face::South, Face::East),
            (Face::West, Face::South),
        ] {
            for face in Face::ALL {
                let info = config.slot_info(TransmissionType::Item, facing, Some(face));
                assert_eq!(info.is_some(), face == output, "{facing:?} {face:?}");
            }
        }
        assert!(
            config
                .slot_info(TransmissionType::Item, Face::North, None)
                .is_none()
        );
    }

    #[test]
    fn the_item_default_applies_to_the_sides_it_lists() {
        let mut config = electric();
        let machine = LightConfigInfo::create(
            Some(RelativeSide::Right),
            None,
            Some(RelativeSide::Back),
            false,
        );
        let attached = AttachedSideConfig::new(&[
            (TransmissionType::Item, machine),
            (
                TransmissionType::Energy,
                LightConfigInfo::create(None, None, None, false),
            ),
        ]);
        config.apply_implicit_components(Some(&attached), Face::North, false);
        assert!(
            config.take_changes().is_empty(),
            "sem nível carregado, ninguém é avisado"
        );
        let item = config.config(TransmissionType::Item).unwrap();
        assert_eq!(item.data_type(RelativeSide::Right), DataType::Output);
        assert_eq!(item.data_type(RelativeSide::Back), DataType::Energy);
        assert_eq!(item.data_type(RelativeSide::Front), DataType::Input);
        assert_eq!(item.data_type(RelativeSide::Bottom), DataType::Input);
        let energy = config.config(TransmissionType::Energy).unwrap();
        assert!(
            RelativeSide::VALUES
                .iter()
                .all(|&side| energy.data_type(side) == DataType::Input)
        );
        // E o que a máquina coleta de volta é o mesmo item.
        assert_eq!(config.collect_implicit_components(), attached);
    }

    #[test]
    fn collecting_leaves_none_out_and_keeps_the_ejecting_flag() {
        let mut config = electric();
        config
            .config_mut(TransmissionType::Item)
            .unwrap()
            .set_ejecting(true);
        config
            .config_mut(TransmissionType::Item)
            .unwrap()
            .set_data_type(DataType::Output, RelativeSide::Left);
        let collected = config.collect_implicit_components();
        let item = &collected.config_info[&TransmissionType::Item];
        assert!(item.ejecting);
        assert_eq!(item.side_config.len(), 1);
        assert_eq!(item.data_type(RelativeSide::Left), DataType::Output);
        assert_eq!(item.data_type(RelativeSide::Right), DataType::None);
        assert!(
            collected.config_info[&TransmissionType::Energy]
                .side_config
                .is_empty()
        );
    }

    #[test]
    fn the_saved_form_is_eject_and_config_by_ordinal() {
        let mut config = electric();
        let info = config.config_mut(TransmissionType::Item).unwrap();
        info.set_ejecting(true);
        info.set_data_type(DataType::Output, RelativeSide::Right);
        info.set_data_type(DataType::Energy, RelativeSide::Back);
        let data = config.serialize();
        assert_eq!(data.eject[TransmissionType::Item.ordinal()], Some(true));
        assert_eq!(data.eject[TransmissionType::Energy.ordinal()], Some(false));
        assert_eq!(
            data.config[TransmissionType::Item.ordinal()],
            Some(vec![0, 0, 4, 8, 0, 0])
        );
        assert_eq!(data.eject[TransmissionType::Fluid.ordinal()], None);

        let mut other = electric();
        other.deserialize(&data, Face::North, true);
        assert_eq!(other.serialize(), data);
        assert_eq!(other.take_changes().len(), 2);
    }

    #[test]
    fn reading_a_partial_or_oversized_array_keeps_the_rest() {
        let mut config = electric();
        let mut data = ConfigData::default();
        // Curto: só os dois primeiros lados.
        data.config[TransmissionType::Item.ordinal()] = Some(vec![1, 4]);
        config.deserialize(&data, Face::North, false);
        let item = config.config(TransmissionType::Item).unwrap();
        assert_eq!(item.data_type(RelativeSide::Front), DataType::Input);
        assert_eq!(item.data_type(RelativeSide::Left), DataType::Output);
        assert_eq!(item.data_type(RelativeSide::Right), DataType::None);
        // Comprido, com um id fora do intervalo: o excesso é ignorado e o id vira NONE.
        data.config[TransmissionType::Item.ordinal()] = Some(vec![1, 99, 4, 7, 8, 9, 1, 1]);
        data.eject[TransmissionType::Item.ordinal()] = Some(true);
        config.deserialize(&data, Face::North, false);
        let item = config.config(TransmissionType::Item).unwrap();
        assert_eq!(item.data_type(RelativeSide::Left), DataType::None);
        assert_eq!(item.data_type(RelativeSide::Bottom), DataType::Extra);
        assert!(item.is_ejecting());
        // O tipo que a máquina não tem é ignorado.
        data.config[TransmissionType::Fluid.ordinal()] = Some(vec![1; 6]);
        config.deserialize(&data, Face::North, false);
        assert!(config.config(TransmissionType::Fluid).is_none());
    }

    #[test]
    fn slot_info_emptiness_follows_the_kind() {
        let probe = |_: TransmissionType, container: usize| container != 1;
        let items = SlotInfo::new(TransmissionType::Item, true, true, vec![0, 2]);
        assert!(items.is_empty(&probe));
        assert!(!SlotInfo::new(TransmissionType::Item, true, true, vec![0, 1]).is_empty(&probe));
        assert!(!SlotInfo::new(TransmissionType::Heat, true, true, vec![0]).is_empty(&probe));
        assert!(SlotInfo::new(TransmissionType::Energy, true, true, vec![]).is_empty(&probe));
        assert!(!SlotInfo::new(TransmissionType::Energy, true, true, vec![1]).is_empty(&probe));
    }

    #[test]
    fn light_config_without_and_create_match_the_java_constants() {
        let machine = LightConfigInfo::create(
            Some(RelativeSide::Right),
            None,
            Some(RelativeSide::Back),
            false,
        );
        assert_eq!(machine.side_config.len(), 6);
        assert_eq!(machine.data_type(RelativeSide::Right), DataType::Output);
        let no_top = machine.without(&[RelativeSide::Top]);
        assert_eq!(no_top.data_type(RelativeSide::Top), DataType::None);
        assert_eq!(no_top.side_config.len(), 5);
    }
}
