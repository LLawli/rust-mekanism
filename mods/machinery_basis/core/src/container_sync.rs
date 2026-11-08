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

    #[test]
    #[should_panic(expected = "recebeu um valor Long")]
    fn a_value_of_another_class_is_an_error() {
        let values = Values::default();
        let mut sync = one(Kind::Int);
        values.set(0, Value::Long(1));
        sync.broadcast_changes(&values);
    }
}
