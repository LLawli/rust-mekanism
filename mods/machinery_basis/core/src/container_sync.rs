//! `MekanismContainer.trackedData`: a lista de dados sincronizados de um menu e o que cada jogador já viu deles. Cada dado é um
//! `ISyncableData` (`mekanism.common.inventory.container.sync`): guarda o último valor que mandou ao jogador e, a cada
//! `broadcastChanges`, diz se mudou (`isDirty`) e o que mandar (`getPropertyData`). Um menu é de um jogador (o Java cria um objeto
//! por jogador), então "o que o jogador viu" é o último valor de cada dado da lista do menu dele.
//!
//! A lib não conhece nenhum campo: quem usa dá a lista (a ordem de `addContainerTrackers`) e um [`TrackedSource`] que lê o valor
//! vivo de cada dado. O rastro `*_trackers` do Java confere a ordem, a classe, a regra de sujeira e o valor mandado.
use crate::window_session::Tracking;
use std::fmt;

/// `ISyncableData.DirtyType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirtyType {
    Clean,
    /// Só a quantidade de uma pilha mudou: o `SyncableLargeResourceStack` manda só o `long`.
    Size,
    Dirty,
}

/// A classe do `Syncable*`: o que decide como se compara com o último valor e o que se manda.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `SyncableBoolean`.
    Boolean,
    /// `SyncableInt`.
    Int,
    /// `SyncableLong`.
    Long,
    /// `SyncableDouble`.
    Double,
    /// `SyncableEnum`: o último valor começa no padrão que a criação dá (`defaultValue`), não no que o getter devolve. Manda o
    /// ordinal, como um `IntPropertyData`.
    Enum { default: i32 },
    /// `SyncableLargeResourceStack`: o último valor começa na pilha vazia.
    LargeResourceStack,
    /// `SyncableStreamCodec`: compara o `hashCode` do valor com o último (que começa em zero) e manda os bytes do codec.
    StreamCodec,
}

impl Kind {
    /// O nome da classe Java, sem o sufixo de classe anônima (`$1` é a versão de vetor, `$3` a de getter e setter, e assim por diante):
    /// o que o rastro do Java grava, para a comparação.
    pub fn class(self) -> &'static str {
        match self {
            Self::Boolean => "SyncableBoolean",
            Self::Int => "SyncableInt",
            Self::Long => "SyncableLong",
            Self::Double => "SyncableDouble",
            Self::Enum { .. } => "SyncableEnum",
            Self::LargeResourceStack => "SyncableLargeResourceStack",
            Self::StreamCodec => "SyncableStreamCodec",
        }
    }
}

/// Um dado da lista: de onde vem o valor (um identificador que o [`TrackedSource`] entende) e a classe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tracked {
    pub source: u16,
    pub kind: Kind,
}

impl Tracked {
    pub const fn new(source: u16, kind: Kind) -> Self {
        Self { source, kind }
    }
}

/// Uma pilha grande (`LargeResourceStack`): o recurso pelo id (vazio é `""`) e a quantidade.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StackValue {
    pub resource: String,
    pub amount: u64,
}

/// O valor vivo de um dado, como o getter dele o devolve.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Boolean(bool),
    Int(i32),
    Long(i64),
    Double(f64),
    /// O ordinal do enum.
    Enum(i32),
    Stack(StackValue),
    /// `hashCode()` do objeto (zero para `null`) e os bytes que o codec escreve para ele.
    Codec {
        hash: i32,
        bytes: Vec<u8>,
    },
}

impl Value {
    fn shape(&self) -> &'static str {
        match self {
            Self::Boolean(_) => "Boolean",
            Self::Int(_) => "Int",
            Self::Long(_) => "Long",
            Self::Double(_) => "Double",
            Self::Enum(_) => "Enum",
            Self::Stack(_) => "Stack",
            Self::Codec { .. } => "Codec",
        }
    }
}

/// O que um `PropertyData` leva.
#[derive(Clone, Debug, PartialEq)]
pub enum PropertyData {
    Boolean(bool),
    Int(i32),
    /// `LongPropertyData`: um `long`, e também só o tamanho de uma pilha.
    Long(i64),
    Double(f64),
    Stack(StackValue),
    Bytes(Vec<u8>),
}

/// Um item de `PacketUpdateContainer`: a posição na lista, a classe do dado e o que ele manda.
#[derive(Clone, Debug, PartialEq)]
pub struct Property {
    pub index: u16,
    pub class: &'static str,
    pub dirty: DirtyType,
    pub data: PropertyData,
}

/// Quem lê o valor vivo de cada dado. A lista só guarda de onde ele vem.
pub trait TrackedSource {
    fn value(&self, source: u16) -> Value;
}

impl<F: Fn(u16) -> Value> TrackedSource for F {
    fn value(&self, source: u16) -> Value {
        self(source)
    }
}

/// O último valor que um dado mandou.
#[derive(Clone, Debug)]
enum Last {
    Boolean(bool),
    Int(i32),
    Long(i64),
    Double(f64),
    Enum(i32),
    Stack(StackValue),
    Hash(i32),
}

#[derive(Clone, Debug)]
struct Syncable {
    tracked: Tracked,
    last: Last,
}

impl Syncable {
    fn new(tracked: Tracked) -> Self {
        let last = match tracked.kind {
            Kind::Boolean => Last::Boolean(false),
            Kind::Int => Last::Int(0),
            Kind::Long => Last::Long(0),
            Kind::Double => Last::Double(0.0),
            Kind::Enum { default } => Last::Enum(default),
            Kind::LargeResourceStack => Last::Stack(StackValue::default()),
            Kind::StreamCodec => Last::Hash(0),
        };
        Self { tracked, last }
    }

    fn mismatch(&self, live: &Value) -> ! {
        panic!(
            "o dado {} (fonte {}) recebeu um valor {}",
            self.tracked.kind.class(),
            self.tracked.source,
            live.shape()
        )
    }

    /// `isDirty`: compara o valor vivo com o último que mandou e guarda o vivo. Os primitivos usam o `!=` do Java, então um `NaN`
    /// nunca é igual a si mesmo. A pilha distingue o tipo (o recurso) do tamanho.
    fn is_dirty(&mut self, live: &Value) -> DirtyType {
        fn dirty(changed: bool) -> DirtyType {
            if changed {
                DirtyType::Dirty
            } else {
                DirtyType::Clean
            }
        }
        match (&mut self.last, live) {
            (Last::Boolean(last), Value::Boolean(now)) => {
                let changed = now != last;
                *last = *now;
                dirty(changed)
            }
            (Last::Int(last), Value::Int(now)) => {
                let changed = now != last;
                *last = *now;
                dirty(changed)
            }
            (Last::Long(last), Value::Long(now)) => {
                let changed = now != last;
                *last = *now;
                dirty(changed)
            }
            (Last::Double(last), Value::Double(now)) => {
                #[allow(clippy::float_cmp)]
                let changed = now != last;
                *last = *now;
                dirty(changed)
            }
            (Last::Enum(last), Value::Enum(now)) => {
                let changed = now != last;
                *last = *now;
                dirty(changed)
            }
            (Last::Stack(last), Value::Stack(now)) => {
                let same_type = now.resource == last.resource;
                if !same_type || now.amount != last.amount {
                    *last = now.clone();
                    if same_type {
                        DirtyType::Size
                    } else {
                        DirtyType::Dirty
                    }
                } else {
                    DirtyType::Clean
                }
            }
            (Last::Hash(last), Value::Codec { hash, .. }) => {
                if last == hash {
                    DirtyType::Clean
                } else {
                    *last = *hash;
                    DirtyType::Dirty
                }
            }
            _ => self.mismatch(live),
        }
    }

    /// `getPropertyData`: lê o valor vivo, não o que `isDirty` guardou. `dirty` é `Dirty` ou `Size`.
    fn property_data(&self, live: &Value, dirty: DirtyType) -> PropertyData {
        match (self.tracked.kind, live) {
            (Kind::Boolean, Value::Boolean(value)) => PropertyData::Boolean(*value),
            (Kind::Int, Value::Int(value)) | (Kind::Enum { .. }, Value::Enum(value)) => {
                PropertyData::Int(*value)
            }
            (Kind::Long, Value::Long(value)) => PropertyData::Long(*value),
            (Kind::Double, Value::Double(value)) => PropertyData::Double(*value),
            (Kind::LargeResourceStack, Value::Stack(stack)) => {
                if dirty == DirtyType::Size {
                    // Só o tamanho mudou: não se reenvia o tipo.
                    PropertyData::Long(i64::try_from(stack.amount).unwrap_or(i64::MAX))
                } else {
                    PropertyData::Stack(stack.clone())
                }
            }
            (Kind::StreamCodec, Value::Codec { bytes, .. }) => PropertyData::Bytes(bytes.clone()),
            _ => self.mismatch(live),
        }
    }
}

/// O espelho de um menu no cliente, com a chave de `startTracking` que o Mekanism usa (`MekanismContainer.UPGRADE_WINDOW`).
pub type Mirror = ContainerMirror<u8>;

/// O menu de um jogador: `trackedData` e `specificTrackedData`, com o último valor de cada dado. A chave `K` identifica uma lista
/// específica (`startTracking(key, tracker)`: a janela de upgrades).
#[derive(Clone)]
pub struct ContainerSync<K: Ord> {
    tracking: Tracking<K>,
    /// Indexado pelo id do dado em `tracking`: os ids nascem em ordem, então o vetor só cresce.
    syncables: Vec<Syncable>,
}

impl<K: Ord> fmt::Debug for ContainerSync<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ContainerSync({} dados)", self.count())
    }
}

impl<K: Ord> ContainerSync<K> {
    /// O menu recém-criado: os `track` do construtor (`addContainerTrackers`), com o último valor de cada um no padrão do dado.
    pub fn new(base: &[Tracked]) -> Self {
        Self {
            tracking: Tracking::new(base.len()),
            syncables: base.iter().copied().map(Syncable::new).collect(),
        }
    }

    /// `trackedData.size()`.
    pub fn count(&self) -> usize {
        self.tracking.count()
    }

    /// A lista de dados, na ordem de `trackedData`.
    pub fn tracked(&self) -> Vec<Tracked> {
        self.tracking
            .ids()
            .iter()
            .map(|id| self.syncables[*id as usize].tracked)
            .collect()
    }

    /// `startTracking(key, tracker)`: os dados entram no fim, e a lista da chave é trocada (a antiga continua em `trackedData`).
    /// Não manda nada. Devolve as posições dos que entraram.
    pub fn start_tracking(&mut self, key: K, list: &[Tracked]) -> Vec<usize> {
        let positions = self.tracking.start(key, list.len());
        self.syncables
            .extend(list.iter().copied().map(Syncable::new));
        positions
    }

    /// `startTrackingServer`: `startTracking` e o envio inicial dos dados novos, com a posição deles na lista inteira.
    pub fn start_tracking_server(
        &mut self,
        key: K,
        list: &[Tracked],
        source: &dyn TrackedSource,
    ) -> Vec<Property> {
        let positions = self.start_tracking(key, list);
        positions
            .into_iter()
            .map(|position| self.send_initial(position, source))
            .collect()
    }

    /// `stopTracking(key)`: tira de `trackedData` a lista que a chave tem agora. Devolve as posições que ocupavam.
    pub fn stop_tracking(&mut self, key: &K) -> Vec<usize> {
        self.tracking.stop(key)
    }

    fn syncable_at(&mut self, position: usize) -> &mut Syncable {
        let id = self.tracking.ids()[position] as usize;
        &mut self.syncables[id]
    }

    fn send_initial(&mut self, position: usize, source: &dyn TrackedSource) -> Property {
        let syncable = self.syncable_at(position);
        let live = source.value(syncable.tracked.source);
        // Pergunta se sujou só para o último valor passar a ser o inicial, e manda como sujo de qualquer jeito.
        syncable.is_dirty(&live);
        Property {
            index: position as u16,
            class: syncable.tracked.kind.class(),
            dirty: DirtyType::Dirty,
            data: syncable.property_data(&live, DirtyType::Dirty),
        }
    }

    /// `sendAllDataToRemote` (a parte do `MekanismContainer`): todos os dados, ao abrir.
    pub fn send_all(&mut self, source: &dyn TrackedSource) -> Vec<Property> {
        (0..self.count())
            .map(|position| self.send_initial(position, source))
            .collect()
    }

    /// `broadcastChanges` (a parte do `MekanismContainer`): só os dados sujos, na ordem da lista.
    pub fn broadcast_changes(&mut self, source: &dyn TrackedSource) -> Vec<Property> {
        let mut dirty_data = Vec::new();
        for position in 0..self.count() {
            let syncable = self.syncable_at(position);
            let live = source.value(syncable.tracked.source);
            let dirty = syncable.is_dirty(&live);
            if dirty != DirtyType::Clean {
                dirty_data.push(Property {
                    index: position as u16,
                    class: syncable.tracked.kind.class(),
                    dirty,
                    data: syncable.property_data(&live, dirty),
                });
            }
        }
        dirty_data
    }
}

impl<K: Ord> ContainerSync<K> {
    /// Se há dado acompanhado além dos do construtor (`startTracking`): é quando o servidor manda o que só quem abriu a janela vê.
    pub fn has_extra(&self) -> bool {
        self.tracking.has_extra()
    }
}

/// `StackValue` de uma pilha sem recurso: o que `LargeResourceStack` vazia é.
impl StackValue {
    /// `LargeResourceStack.isEmpty`.
    pub fn is_empty(&self) -> bool {
        self.resource.is_empty() || self.amount == 0
    }
}

#[derive(Clone, Debug)]
struct Mirrored {
    kind: Kind,
    value: Option<PropertyData>,
}

/// O menu do lado do cliente: `trackedData` com o valor que cada `Syncable*` do cliente guarda. O servidor manda um [`Property`] por dado
/// sujo e o cliente o entrega à posição dele (`MekanismContainer.handleWindowProperty`, `MekanismContainer.java:560-645`); a classe do dado
/// que mora na posição (não a do pacote) decide como o valor entra, como o `instanceof` do Java. Um pacote de classe errada, de posição
/// que não existe ou de uma classe que o cliente não conhece é ignorado.
#[derive(Clone)]
pub struct ContainerMirror<K: Ord> {
    tracking: Tracking<K>,
    /// Indexado pelo id do dado em `tracking`, como em [`ContainerSync`].
    slots: Vec<Mirrored>,
}

impl<K: Ord> fmt::Debug for ContainerMirror<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ContainerMirror({} dados)", self.count())
    }
}

impl<K: Ord> ContainerMirror<K> {
    /// O menu recém-aberto no cliente: os dados do construtor (`addContainerTrackers`), ainda sem nenhum valor do servidor.
    pub fn new(base: &[Tracked]) -> Self {
        Self {
            tracking: Tracking::new(base.len()),
            slots: base
                .iter()
                .map(|tracked| Mirrored {
                    kind: tracked.kind,
                    value: None,
                })
                .collect(),
        }
    }

    /// `trackedData.size()`.
    pub fn count(&self) -> usize {
        self.tracking.count()
    }

    /// `startTracking(key, tracker)` no cliente: a janela acrescenta os dados dela no fim, como no servidor. Devolve as posições.
    pub fn start_tracking(&mut self, key: K, list: &[Tracked]) -> Vec<usize> {
        let positions = self.tracking.start(key, list.len());
        self.slots.extend(list.iter().map(|tracked| Mirrored {
            kind: tracked.kind,
            value: None,
        }));
        positions
    }

    /// `stopTracking(key)` no cliente.
    pub fn stop_tracking(&mut self, key: &K) -> Vec<usize> {
        self.tracking.stop(key)
    }

    fn slot(&self, position: usize) -> Option<&Mirrored> {
        let id = *self.tracking.ids().get(position)? as usize;
        self.slots.get(id)
    }

    fn slot_mut(&mut self, position: usize) -> Option<&mut Mirrored> {
        let id = *self.tracking.ids().get(position)? as usize;
        self.slots.get_mut(id)
    }

    /// `handleWindowProperty`: entrega o valor ao dado da posição. Devolve se algum dado o aceitou.
    pub fn apply(&mut self, index: u16, data: &PropertyData) -> bool {
        let Some(slot) = self.slot_mut(usize::from(index)) else {
            return false;
        };
        match (slot.kind, data) {
            (Kind::Boolean, PropertyData::Boolean(_))
            | (Kind::Int | Kind::Enum { .. }, PropertyData::Int(_))
            | (Kind::Long, PropertyData::Long(_))
            | (Kind::Double, PropertyData::Double(_))
            | (Kind::StreamCodec, PropertyData::Bytes(_))
            | (Kind::LargeResourceStack, PropertyData::Stack(_)) => {
                slot.value = Some(data.clone());
                true
            }
            // `SyncableLargeResourceStack.set(long)`: só a quantidade, sobre o recurso que o cliente já tem.
            (Kind::LargeResourceStack, PropertyData::Long(amount)) => {
                let current = match &slot.value {
                    Some(PropertyData::Stack(stack)) => stack.clone(),
                    _ => StackValue::default(),
                };
                if !current.is_empty() {
                    let amount = u64::try_from(*amount).unwrap_or(0);
                    slot.value = Some(PropertyData::Stack(if amount == 0 {
                        StackValue::default()
                    } else {
                        StackValue {
                            resource: current.resource,
                            amount,
                        }
                    }));
                }
                true
            }
            _ => false,
        }
    }

    /// O valor de um `SyncableBoolean`; `false` antes de o servidor mandar, ou se o dado da posição é de outra classe.
    pub fn boolean(&self, position: usize) -> bool {
        match self.slot(position) {
            Some(Mirrored {
                kind: Kind::Boolean,
                value: Some(PropertyData::Boolean(value)),
            }) => *value,
            _ => false,
        }
    }

    /// O valor de um `SyncableInt`.
    pub fn int(&self, position: usize) -> i32 {
        match self.slot(position) {
            Some(Mirrored {
                kind: Kind::Int,
                value: Some(PropertyData::Int(value)),
            }) => *value,
            _ => 0,
        }
    }

    /// O ordinal de um `SyncableEnum`; `default` antes de o servidor mandar (o padrão que a criação do dado deu).
    pub fn enum_ordinal(&self, position: usize) -> i32 {
        match self.slot(position) {
            Some(Mirrored {
                kind: Kind::Enum { default },
                value,
            }) => match value {
                Some(PropertyData::Int(value)) => *value,
                _ => *default,
            },
            _ => 0,
        }
    }

    /// O valor de um `SyncableLong`.
    pub fn long(&self, position: usize) -> i64 {
        match self.slot(position) {
            Some(Mirrored {
                kind: Kind::Long,
                value: Some(PropertyData::Long(value)),
            }) => *value,
            _ => 0,
        }
    }

    /// O valor de um `SyncableDouble`.
    pub fn double(&self, position: usize) -> f64 {
        match self.slot(position) {
            Some(Mirrored {
                kind: Kind::Double,
                value: Some(PropertyData::Double(value)),
            }) => *value,
            _ => 0.0,
        }
    }

    /// A pilha de um `SyncableLargeResourceStack`; vazia antes de o servidor mandar.
    pub fn stack(&self, position: usize) -> StackValue {
        match self.slot(position) {
            Some(Mirrored {
                kind: Kind::LargeResourceStack,
                value: Some(PropertyData::Stack(stack)),
            }) => stack.clone(),
            _ => StackValue::default(),
        }
    }

    /// Os bytes de um `SyncableStreamCodec`; vazios antes de o servidor mandar.
    pub fn bytes(&self, position: usize) -> &[u8] {
        match self.slot(position) {
            Some(Mirrored {
                kind: Kind::StreamCodec,
                value: Some(PropertyData::Bytes(bytes)),
            }) => bytes,
            _ => &[],
        }
    }
}

/// O que `PacketUpdateContainer` leva, no formato do canal de mod do Horizon. As classes de `PropertyData` seguem o `PropertyType` do
/// Java (o ordinal vai num byte); o resto troca o id da janela (`windowId`) pela posição do bloco e, na abertura, pelo id do bloco, que
/// diz ao cliente a lista de dados do menu. O recurso de uma pilha vai só como id, sem o `ResourceType`: o tipo do tanque é o da lista.
pub mod wire {
    use super::PropertyData;
    use super::{Property, StackValue};

    /// `PropertyType.ordinal()`.
    const BOOLEAN: u8 = 0;
    const DOUBLE: u8 = 2;
    const INT: u8 = 4;
    const LONG: u8 = 5;
    const BYTE_ARRAY: u8 = 7;
    const LARGE_RESOURCE_STACK: u8 = 12;

    /// O pacote: de que bloco, se abre um menu novo (`sendAllDataToRemote` do menu recém-criado, e então o id do bloco) ou só atualiza
    /// (`broadcastChanges`), e os dados.
    #[derive(Clone, Debug, PartialEq)]
    pub struct Packet {
        pub pos: [i32; 3],
        /// O id do bloco do menu, na abertura.
        pub opens: Option<String>,
        pub data: Vec<(u16, PropertyData)>,
    }

    impl Packet {
        /// Os dados `properties` do `PacketUpdateContainer` de `pos`.
        pub fn new(pos: [i32; 3], opens: Option<String>, properties: &[Property]) -> Self {
            Self {
                pos,
                opens,
                data: properties
                    .iter()
                    .map(|property| (property.index, property.data.clone()))
                    .collect(),
            }
        }
    }

    /// `VarInt.write`.
    pub fn write_var_int(bytes: &mut Vec<u8>, value: i32) {
        write_var_long(bytes, i64::from(value as u32));
    }

    /// `VarLong.write`.
    pub fn write_var_long(bytes: &mut Vec<u8>, value: i64) {
        let mut rest = value as u64;
        while rest & !0x7F != 0 {
            bytes.push((rest & 0x7F | 0x80) as u8);
            rest >>= 7;
        }
        bytes.push(rest as u8);
    }

    fn read_var_long(bytes: &mut &[u8], max: usize) -> Option<u64> {
        let mut value = 0u64;
        for shift in 0..max {
            let (&byte, rest) = bytes.split_first()?;
            *bytes = rest;
            value |= u64::from(byte & 0x7F) << (7 * shift);
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }

    fn read_bytes<'a>(bytes: &mut &'a [u8], length: usize) -> Option<&'a [u8]> {
        let (head, rest) = bytes.split_at_checked(length)?;
        *bytes = rest;
        Some(head)
    }

    fn write_text(bytes: &mut Vec<u8>, text: &str) {
        write_var_int(bytes, i32::try_from(text.len()).unwrap_or(i32::MAX));
        bytes.extend_from_slice(text.as_bytes());
    }

    fn read_text(bytes: &mut &[u8]) -> Option<String> {
        let length = usize::try_from(read_var_long(bytes, 5)?).ok()?;
        String::from_utf8(read_bytes(bytes, length)?.to_vec()).ok()
    }

    /// O pacote em bytes.
    pub fn encode(packet: &Packet) -> Vec<u8> {
        let mut bytes = vec![u8::from(packet.opens.is_some())];
        for value in packet.pos {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        if let Some(block) = &packet.opens {
            write_text(&mut bytes, block);
        }
        write_var_int(
            &mut bytes,
            i32::try_from(packet.data.len()).unwrap_or(i32::MAX),
        );
        for (index, data) in &packet.data {
            match data {
                PropertyData::Boolean(value) => {
                    bytes.push(BOOLEAN);
                    bytes.extend_from_slice(&index.to_be_bytes());
                    bytes.push(u8::from(*value));
                }
                PropertyData::Int(value) => {
                    bytes.push(INT);
                    bytes.extend_from_slice(&index.to_be_bytes());
                    write_var_int(&mut bytes, *value);
                }
                PropertyData::Long(value) => {
                    bytes.push(LONG);
                    bytes.extend_from_slice(&index.to_be_bytes());
                    write_var_long(&mut bytes, *value);
                }
                PropertyData::Double(value) => {
                    bytes.push(DOUBLE);
                    bytes.extend_from_slice(&index.to_be_bytes());
                    bytes.extend_from_slice(&value.to_be_bytes());
                }
                PropertyData::Bytes(value) => {
                    bytes.push(BYTE_ARRAY);
                    bytes.extend_from_slice(&index.to_be_bytes());
                    write_var_int(&mut bytes, i32::try_from(value.len()).unwrap_or(i32::MAX));
                    bytes.extend_from_slice(value);
                }
                PropertyData::Stack(StackValue { resource, amount }) => {
                    bytes.push(LARGE_RESOURCE_STACK);
                    bytes.extend_from_slice(&index.to_be_bytes());
                    write_text(&mut bytes, resource);
                    write_var_long(&mut bytes, i64::try_from(*amount).unwrap_or(i64::MAX));
                }
            }
        }
        bytes
    }

    /// O inverso. Um pacote cortado ou de classe desconhecida não é nada.
    pub fn decode(mut bytes: &[u8]) -> Option<Packet> {
        let bytes = &mut bytes;
        let opens = read_bytes(bytes, 1)?[0] != 0;
        let mut pos = [0; 3];
        for value in &mut pos {
            *value = i32::from_be_bytes(read_bytes(bytes, 4)?.try_into().ok()?);
        }
        let opens = if opens { Some(read_text(bytes)?) } else { None };
        let count = usize::try_from(read_var_long(bytes, 5)?).ok()?;
        let mut data = Vec::new();
        for _ in 0..count {
            let kind = read_bytes(bytes, 1)?[0];
            let index = u16::from_be_bytes(read_bytes(bytes, 2)?.try_into().ok()?);
            let value = match kind {
                BOOLEAN => PropertyData::Boolean(read_bytes(bytes, 1)?[0] != 0),
                INT => PropertyData::Int(read_var_long(bytes, 5)? as u32 as i32),
                LONG => PropertyData::Long(read_var_long(bytes, 10)? as i64),
                DOUBLE => {
                    PropertyData::Double(f64::from_be_bytes(read_bytes(bytes, 8)?.try_into().ok()?))
                }
                BYTE_ARRAY => {
                    let length = usize::try_from(read_var_long(bytes, 5)?).ok()?;
                    PropertyData::Bytes(read_bytes(bytes, length)?.to_vec())
                }
                LARGE_RESOURCE_STACK => PropertyData::Stack(StackValue {
                    resource: read_text(bytes)?,
                    amount: u64::try_from(read_var_long(bytes, 10)? as i64).unwrap_or(0),
                }),
                _ => return None,
            };
            data.push((index, value));
        }
        bytes.is_empty().then_some(Packet { pos, opens, data })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// Uma fonte que o teste muda à vontade.
    #[derive(Default)]
    struct Values(RefCell<HashMap<u16, Value>>);

    impl Values {
        fn set(&self, source: u16, value: Value) {
            self.0.borrow_mut().insert(source, value);
        }
    }

    impl TrackedSource for Values {
        fn value(&self, source: u16) -> Value {
            self.0
                .borrow()
                .get(&source)
                .cloned()
                .unwrap_or(Value::Int(0))
        }
    }

    fn stack(resource: &str, amount: u64) -> Value {
        Value::Stack(StackValue {
            resource: resource.to_owned(),
            amount,
        })
    }

    fn one(kind: Kind) -> ContainerSync<u8> {
        ContainerSync::new(&[Tracked::new(0, kind)])
    }

    #[test]
    fn a_primitive_is_dirty_when_it_differs_from_the_last_value_it_sent() {
        let values = Values::default();
        let mut sync = one(Kind::Long);
        values.set(0, Value::Long(0));
        assert_eq!(sync.broadcast_changes(&values), []);
        values.set(0, Value::Long(5));
        let sent = sync.broadcast_changes(&values);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].data, PropertyData::Long(5));
        assert_eq!(sent[0].class, "SyncableLong");
        // Mandou: o mesmo valor não sujo de novo.
        assert_eq!(sync.broadcast_changes(&values), []);
        values.set(0, Value::Long(0));
        assert_eq!(sync.broadcast_changes(&values).len(), 1);
    }

    #[test]
    fn the_last_value_starts_at_the_default_so_a_default_is_never_sent_until_it_changes() {
        // `SyncableBoolean` começa em `false`, `SyncableInt` em 0, `SyncableDouble` em 0.0: o valor inicial não suja.
        let values = Values::default();
        let mut sync = ContainerSync::<u8>::new(&[
            Tracked::new(0, Kind::Boolean),
            Tracked::new(1, Kind::Int),
            Tracked::new(2, Kind::Double),
        ]);
        values.set(0, Value::Boolean(false));
        values.set(1, Value::Int(0));
        values.set(2, Value::Double(0.0));
        assert_eq!(sync.broadcast_changes(&values), []);
        values.set(2, Value::Double(-0.0));
        assert_eq!(
            sync.broadcast_changes(&values),
            [],
            "-0.0 == 0.0 no `!=` do Java"
        );
    }

    #[test]
    fn a_nan_double_is_dirty_on_every_broadcast() {
        let values = Values::default();
        let mut sync = one(Kind::Double);
        values.set(0, Value::Double(f64::NAN));
        assert_eq!(sync.broadcast_changes(&values).len(), 1);
        assert_eq!(sync.broadcast_changes(&values).len(), 1);
    }

    #[test]
    fn an_enum_starts_at_its_default_and_sends_the_ordinal() {
        let values = Values::default();
        let mut sync = one(Kind::Enum { default: 0 });
        values.set(0, Value::Enum(0));
        assert_eq!(sync.broadcast_changes(&values), []);
        values.set(0, Value::Enum(2));
        let sent = sync.broadcast_changes(&values);
        assert_eq!(sent[0].data, PropertyData::Int(2));
        assert_eq!(sent[0].class, "SyncableEnum");
    }

    #[test]
    fn a_stack_sends_only_the_size_when_the_resource_is_the_same() {
        let values = Values::default();
        let mut sync = one(Kind::LargeResourceStack);
        values.set(0, stack("", 0));
        assert_eq!(
            sync.broadcast_changes(&values),
            [],
            "a pilha vazia é o padrão"
        );
        values.set(0, stack("mekanism:redstone", 30));
        let sent = sync.broadcast_changes(&values);
        assert_eq!(sent[0].dirty, DirtyType::Dirty);
        assert_eq!(
            sent[0].data,
            PropertyData::Stack(StackValue {
                resource: "mekanism:redstone".into(),
                amount: 30
            })
        );
        values.set(0, stack("mekanism:redstone", 29));
        let sent = sync.broadcast_changes(&values);
        assert_eq!(sent[0].dirty, DirtyType::Size);
        assert_eq!(sent[0].data, PropertyData::Long(29));
        // Outro recurso com a mesma quantidade: o tipo mudou.
        values.set(0, stack("mekanism:carbon", 29));
        assert_eq!(sync.broadcast_changes(&values)[0].dirty, DirtyType::Dirty);
        // Esvaziar: o recurso vazio é outro tipo.
        values.set(0, stack("", 0));
        let sent = sync.broadcast_changes(&values);
        assert_eq!(sent[0].dirty, DirtyType::Dirty);
        assert_eq!(sent[0].data, PropertyData::Stack(StackValue::default()));
    }

    #[test]
    fn a_stream_codec_compares_hash_codes_and_starts_from_zero() {
        let values = Values::default();
        let mut sync = one(Kind::StreamCodec);
        values.set(
            0,
            Value::Codec {
                hash: 0,
                bytes: vec![0],
            },
        );
        assert_eq!(
            sync.broadcast_changes(&values),
            [],
            "hash zero é o padrão: não sai até mudar"
        );
        values.set(
            0,
            Value::Codec {
                hash: 7,
                bytes: vec![1, 5, 2],
            },
        );
        let sent = sync.broadcast_changes(&values);
        assert_eq!(sent[0].data, PropertyData::Bytes(vec![1, 5, 2]));
        // Mesmo hash, bytes diferentes: o Java não percebe.
        values.set(
            0,
            Value::Codec {
                hash: 7,
                bytes: vec![9],
            },
        );
        assert_eq!(sync.broadcast_changes(&values), []);
        values.set(
            0,
            Value::Codec {
                hash: 0,
                bytes: vec![0],
            },
        );
        assert_eq!(sync.broadcast_changes(&values).len(), 1);
    }

    #[test]
    fn send_all_sends_every_data_as_dirty_and_makes_the_current_value_the_last_one() {
        let values = Values::default();
        let mut sync = ContainerSync::<u8>::new(&[
            Tracked::new(0, Kind::Int),
            Tracked::new(1, Kind::LargeResourceStack),
        ]);
        values.set(0, Value::Int(0));
        values.set(1, stack("a", 5));
        let sent = sync.send_all(&values);
        assert_eq!(sent.len(), 2, "o valor padrão também vai");
        assert_eq!(sent[1].dirty, DirtyType::Dirty);
        assert_eq!(
            sent[1].data,
            PropertyData::Stack(StackValue {
                resource: "a".into(),
                amount: 5
            })
        );
        assert_eq!(sync.broadcast_changes(&values), []);
    }

    #[test]
    fn specific_tracking_sends_the_new_data_with_their_position_and_keeps_the_leak() {
        let values = Values::default();
        let mut sync = ContainerSync::<u8>::new(&[Tracked::new(0, Kind::Int)]);
        values.set(0, Value::Int(3));
        values.set(10, Value::Int(4));
        values.set(
            11,
            Value::Codec {
                hash: 1,
                bytes: vec![1],
            },
        );
        let window = [
            Tracked::new(10, Kind::Int),
            Tracked::new(11, Kind::StreamCodec),
        ];
        let sent = sync.start_tracking_server(2, &window, &values);
        assert_eq!(sync.count(), 3);
        assert_eq!(sent.iter().map(|p| p.index).collect::<Vec<_>>(), [1, 2]);
        // O primeiro `broadcast` manda o dado do construtor, que nunca foi enviado, e nada dos novos.
        let sent = sync.broadcast_changes(&values);
        assert_eq!(sent.iter().map(|p| p.index).collect::<Vec<_>>(), [0]);
        // Pedir de novo sem parar: os antigos ficam, e os novos entram no fim.
        let sent = sync.start_tracking_server(2, &window, &values);
        assert_eq!(sync.count(), 5);
        assert_eq!(sent.iter().map(|p| p.index).collect::<Vec<_>>(), [3, 4]);
        values.set(10, Value::Int(5));
        // Os quatro dados da janela leem a mesma fonte: os antigos e os novos sujam juntos.
        let sent = sync.broadcast_changes(&values);
        assert_eq!(sent.iter().map(|p| p.index).collect::<Vec<_>>(), [1, 3]);
        sync.stop_tracking(&2);
        assert_eq!(sync.count(), 3);
        values.set(10, Value::Int(6));
        let sent = sync.broadcast_changes(&values);
        assert_eq!(sent.iter().map(|p| p.index).collect::<Vec<_>>(), [1]);
        assert_eq!(
            sync.tracked(),
            [
                Tracked::new(0, Kind::Int),
                Tracked::new(10, Kind::Int),
                Tracked::new(11, Kind::StreamCodec)
            ]
        );
    }

    fn mirror_of(list: &[Tracked]) -> ContainerMirror<u8> {
        ContainerMirror::new(list)
    }

    #[test]
    fn the_mirror_hands_each_value_to_the_class_of_the_data_in_that_position() {
        let mut mirror = mirror_of(&[
            Tracked::new(0, Kind::Boolean),
            Tracked::new(1, Kind::Int),
            Tracked::new(2, Kind::Long),
            Tracked::new(3, Kind::Double),
            Tracked::new(4, Kind::Enum { default: 2 }),
            Tracked::new(5, Kind::LargeResourceStack),
            Tracked::new(6, Kind::StreamCodec),
        ]);
        // Antes de o servidor mandar: os padrões.
        assert!(!mirror.boolean(0));
        assert_eq!(mirror.int(1), 0);
        assert_eq!(mirror.long(2), 0);
        assert_eq!(
            mirror.enum_ordinal(4),
            2,
            "o padrão que a criação do dado deu"
        );
        assert_eq!(mirror.stack(5), StackValue::default());
        assert!(mirror.bytes(6).is_empty());
        assert!(mirror.apply(0, &PropertyData::Boolean(true)));
        assert!(mirror.apply(1, &PropertyData::Int(-7)));
        assert!(mirror.apply(2, &PropertyData::Long(1 << 40)));
        assert!(mirror.apply(3, &PropertyData::Double(2.5)));
        assert!(mirror.apply(4, &PropertyData::Int(3)));
        assert!(mirror.apply(6, &PropertyData::Bytes(vec![1, 5, 2])));
        assert!(mirror.boolean(0));
        assert_eq!(mirror.int(1), -7);
        assert_eq!(mirror.long(2), 1 << 40);
        assert!((mirror.double(3) - 2.5).abs() < 1e-12);
        assert_eq!(mirror.enum_ordinal(4), 3);
        assert_eq!(mirror.bytes(6), [1, 5, 2]);
    }

    #[test]
    fn a_property_of_another_class_or_of_a_position_that_does_not_exist_is_ignored() {
        let mut mirror = mirror_of(&[Tracked::new(0, Kind::Int), Tracked::new(1, Kind::Long)]);
        // `handleWindowProperty(short, long)` num `SyncableInt`: o `instanceof` do Java não casa.
        assert!(!mirror.apply(0, &PropertyData::Long(5)));
        assert!(!mirror.apply(1, &PropertyData::Int(5)));
        assert!(!mirror.apply(9, &PropertyData::Int(5)));
        assert_eq!(mirror.int(0), 0);
        assert_eq!(mirror.long(1), 0);
        // E um dado lido como outra classe devolve o padrão, não o valor de outra classe.
        assert!(mirror.apply(1, &PropertyData::Long(5)));
        assert_eq!(mirror.int(1), 0);
        assert_eq!(mirror.long(0), 0);
    }

    #[test]
    fn a_size_only_stack_changes_the_amount_of_the_stack_the_client_has() {
        let mut mirror = mirror_of(&[Tracked::new(0, Kind::LargeResourceStack)]);
        // Sem pilha no cliente, `set(long)` não faz nada (`if (!stack.isEmpty())`).
        assert!(mirror.apply(0, &PropertyData::Long(40)));
        assert_eq!(mirror.stack(0), StackValue::default());
        mirror.apply(
            0,
            &PropertyData::Stack(StackValue {
                resource: "mekanism:carbon".into(),
                amount: 100,
            }),
        );
        mirror.apply(0, &PropertyData::Long(40));
        assert_eq!(
            mirror.stack(0),
            StackValue {
                resource: "mekanism:carbon".into(),
                amount: 40
            }
        );
        // Quantidade zero esvazia a pilha inteira, o recurso também.
        mirror.apply(0, &PropertyData::Long(0));
        assert_eq!(mirror.stack(0), StackValue::default());
    }

    #[test]
    fn the_mirror_tracks_the_window_data_at_the_same_positions_as_the_server() {
        let window = [
            Tracked::new(10, Kind::Int),
            Tracked::new(11, Kind::StreamCodec),
        ];
        let mut mirror = mirror_of(&[Tracked::new(0, Kind::Int)]);
        assert_eq!(mirror.start_tracking(2, &window), [1, 2]);
        assert!(mirror.apply(1, &PropertyData::Int(9)));
        assert!(mirror.apply(2, &PropertyData::Bytes(vec![0])));
        assert_eq!(mirror.int(1), 9);
        assert_eq!(mirror.stop_tracking(&2), [1, 2]);
        assert_eq!(mirror.count(), 1);
        assert!(!mirror.apply(1, &PropertyData::Int(9)), "a janela fechou");
        assert_eq!(mirror.int(1), 0);
    }

    #[test]
    fn the_wire_packet_round_trips_every_class() {
        let packet = wire::Packet {
            pos: [-3, 64, 1_000_000],
            opens: Some("mekanism:enrichment_chamber".into()),
            data: vec![
                (0, PropertyData::Boolean(true)),
                (1, PropertyData::Int(-1)),
                (2, PropertyData::Int(300)),
                (3, PropertyData::Long(i64::MAX)),
                (4, PropertyData::Long(0)),
                (5, PropertyData::Double(-0.5)),
                (6, PropertyData::Bytes(vec![1, 5, 2])),
                (
                    7,
                    PropertyData::Stack(StackValue {
                        resource: "mekanism:carbon".into(),
                        amount: 1000,
                    }),
                ),
                (8, PropertyData::Stack(StackValue::default())),
            ],
        };
        assert_eq!(wire::decode(&wire::encode(&packet)), Some(packet));
        let update = wire::Packet {
            pos: [0, 0, 0],
            opens: None,
            data: Vec::new(),
        };
        assert_eq!(wire::decode(&wire::encode(&update)), Some(update));
    }

    #[test]
    fn a_cut_or_padded_packet_is_nothing() {
        let packet = wire::Packet {
            pos: [1, 2, 3],
            opens: None,
            data: vec![(1, PropertyData::Long(300))],
        };
        let bytes = wire::encode(&packet);
        for cut in 0..bytes.len() {
            assert_eq!(wire::decode(&bytes[..cut]), None, "cortado em {cut}");
        }
        let mut longer = bytes.clone();
        longer.push(0);
        assert_eq!(wire::decode(&longer), None);
        // Uma classe que o Java tem e a lista não (`BYTE`, 1) não é lida.
        let mut other = bytes;
        other[14] = 1;
        assert_eq!(wire::decode(&other), None);
    }

    #[test]
    #[should_panic(expected = "recebeu um valor Long")]
    fn a_value_of_another_class_is_an_error() {
        let values = Values::default();
        let mut sync = one(Kind::Int);
        values.set(0, Value::Long(1));
        sync.broadcast_changes(&values);
    }
}
