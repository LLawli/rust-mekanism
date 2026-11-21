//! As frequências do original (`mekanism.common.lib.frequency`): `Frequency`, `FrequencyIdentity`, `FrequencyLookup` (o `SavedData` que guarda as frequências de um dono e
//! de um modo) e `FrequencyController` (as tabelas pública, privada e de confiança de um tipo). O original usa a mesma estrutura para a segurança, o teletransporte, o
//! Quantum Entangloporter e o QIO; a lib não conhece nenhum tipo: quem a usa diz o nome do tipo (`FrequencyType.getName`), o espaço de nomes do arquivo e implementa
//! [`Frequency`]. A frequência de segurança está em [`crate::security_frequency`].
//!
//! O que vai para o disco é o que o original grava (`FrequencyLookup.codec`: o dono opcional e a lista `freq_list`), no arquivo que [`lookup_id`] diz
//! (`<espaço>:frequency/<tipo>[/<dono>]/<modo>`, que o `SavedDataStorage` grava em `data/<espaço>/frequency/<tipo>[/<dono>]/<modo>.dat`).
//!
//! **A tabela só fica suja pelo que o original suja** (`FrequencyLookup.setDirty`): criar ou tirar uma frequência, validar a de um tile que a tabela não tinha, e o
//! `update` ou o `onDeactivate` que devolve `true`. O campo `dirty` do `Frequency` do original é só escrito (nenhum código o lê), então mudar uma frequência que já
//! existe não suja a tabela: a mudança só chega ao disco se outra coisa a sujar depois. Está em "Comportamento do original que parece bug e fica".
//!
//! A ordem das tabelas privadas e de confiança (`Object2ObjectOpenHashMap<UUID, FrequencyLookup>`, que `getTrustedLookups` percorre) ainda não é exposta: nenhum tipo
//! portado a percorre, e ela entra pela tabela do fastutil (`sim::open_hash`) quando o primeiro tipo `PUBLIC_PRIVATE_TRUSTED` chegar, nunca pela ordem de inserção.

use crate::recipe_data::SecurityMode;
use crate::security::{owner_of, uuid_ints};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// `SerializationConstants.OWNER_UUID` e `FREQUENCY_LIST`.
pub const OWNER_UUID: &str = "owner";
pub const FREQUENCY_LIST: &str = "freq_list";

/// `FrequencyIdentity`: a chave da frequência, o modo e o dono.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrequencyIdentity<K> {
    pub key: K,
    pub security_mode: SecurityMode,
    pub owner: Option<u128>,
}

/// O que todo `Frequency` do original tem e a tabela usa.
pub trait Frequency: Clone + std::fmt::Debug {
    /// `Frequency.getKey`: o nome, ou o dono na frequência de segurança.
    type Key: Clone + PartialEq + std::fmt::Debug;

    fn key(&self) -> Self::Key;
    fn owner(&self) -> Option<u128>;
    /// `setValid`: a frequência criada do NBT de um tile nasce inválida, até a tabela a validar.
    fn set_valid(&mut self, valid: bool);
    /// `onRemove`.
    fn set_removed(&mut self);
    /// `Frequency.update(level, tile)`: `true` se algo que se grava mudou. O padrão do original é `false`.
    fn update(&mut self) -> bool {
        false
    }
    /// `Frequency.onDeactivate(level, tile)`. O padrão do original é `false`.
    fn on_deactivate(&mut self) -> bool {
        false
    }
    /// `FrequencyType.create(key, owner, securityMode)`; `names` é `MekanismUtils.getLastKnownUsername`, que o construtor do `Frequency` chama para o nome do dono.
    fn create(
        key: Self::Key,
        owner: Option<u128>,
        security_mode: SecurityMode,
        names: &dyn Fn(u128) -> String,
    ) -> Self;
    /// O `CODEC` do tipo.
    fn encode(&self) -> Value;
    /// O `CODEC` do tipo, lendo; `names` é `MekanismUtils.getLastKnownUsername`.
    fn decode(value: &Value, names: &dyn Fn(u128) -> String) -> Option<Self>;
}

/// `FrequencyController.Type`: que tabelas o tipo tem.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControllerType {
    PublicOnly,
    PrivateOnly,
    PublicPrivateTrusted,
}

impl ControllerType {
    /// `Type.supports`.
    pub fn supports(self, mode: SecurityMode) -> bool {
        match mode {
            SecurityMode::Public => matches!(self, Self::PublicOnly | Self::PublicPrivateTrusted),
            SecurityMode::Private => matches!(self, Self::PrivateOnly | Self::PublicPrivateTrusted),
            SecurityMode::Trusted => self == Self::PublicPrivateTrusted,
        }
    }
}

/// `FrequencyLookup.getId`: `frequency/<tipo>/[<dono>/]<modo>` no espaço de nomes de quem usa.
pub fn lookup_id(
    namespace: &str,
    type_name: &str,
    owner: Option<u128>,
    mode: SecurityMode,
) -> String {
    let mut path = format!("{namespace}:frequency/{type_name}/");
    if let Some(owner) = owner {
        path.push_str(&uuid_text(owner));
        path.push('/');
    }
    path.push_str(mode.serialized_name());
    path
}

/// O caminho do arquivo do `SavedData` de [`lookup_id`], relativo à pasta do mundo (`SavedDataStorage`: `data/<espaço>/<caminho>.dat`).
pub fn lookup_file(id: &str) -> String {
    let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", id));
    format!("data/{namespace}/{path}.dat")
}

/// `UUID.toString`: os 32 dígitos em minúsculas, com os quatro hífens.
pub fn uuid_text(uuid: u128) -> String {
    let hex = format!("{uuid:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// `FrequencyLookup`: as frequências de um dono e de um modo, na ordem em que entraram (`LinkedHashMap`), e se há algo a gravar.
#[derive(Clone, Debug)]
pub struct FrequencyLookup<F: Frequency> {
    frequencies: Vec<(F::Key, F)>,
    owner: Option<u128>,
    security_mode: SecurityMode,
    dirty: bool,
}

impl<F: Frequency> FrequencyLookup<F> {
    pub fn new(owner: Option<u128>, security_mode: SecurityMode) -> Self {
        Self {
            frequencies: Vec::new(),
            owner,
            security_mode,
            dirty: false,
        }
    }

    pub fn owner(&self) -> Option<u128> {
        self.owner
    }

    pub fn security_mode(&self) -> SecurityMode {
        self.security_mode
    }

    /// `SavedData.isDirty`.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// `SavedData.setDirty(boolean)`.
    pub fn set_dirty(&mut self, dirty: bool) {
        self.dirty = dirty;
    }

    fn index(&self, key: &F::Key) -> Option<usize> {
        self.frequencies
            .iter()
            .position(|(stored, _)| stored == key)
    }

    /// `getFrequency(key)`.
    pub fn get(&self, key: &F::Key) -> Option<&F> {
        self.index(key).map(|at| &self.frequencies[at].1)
    }

    /// A frequência guardada, para quem a muda no lugar (a tabela não fica suja por isso: o original também não a suja).
    pub fn get_mut(&mut self, key: &F::Key) -> Option<&mut F> {
        self.index(key).map(|at| &mut self.frequencies[at].1)
    }

    /// `frequencies.values()`, na ordem de entrada.
    pub fn frequencies(&self) -> impl Iterator<Item = &F> {
        self.frequencies.iter().map(|(_, frequency)| frequency)
    }

    pub fn len(&self) -> usize {
        self.frequencies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frequencies.is_empty()
    }

    /// `remove(key, ownerUUID)`: só o dono tira a frequência.
    pub fn remove(&mut self, key: &F::Key, owner: u128) -> bool {
        let Some(at) = self.index(key) else {
            return false;
        };
        if self.frequencies[at].1.owner() != Some(owner) {
            return false;
        }
        self.frequencies[at].1.set_removed();
        self.frequencies.remove(at);
        self.dirty = true;
        true
    }

    /// `deactivate(freq, level, tile)`.
    pub fn deactivate(&mut self, key: &F::Key) {
        if let Some(frequency) = self.get_mut(key)
            && frequency.on_deactivate()
        {
            self.dirty = true;
        }
    }

    /// `validateAndUpdate(level, tile, freq)`: a frequência que um tile leu do NBT vira a que a tabela guarda; se a tabela não a tem, a do tile entra (válida) e a
    /// tabela fica suja. Devolve a chave da guardada.
    pub fn validate_and_update(&mut self, mut frequency: F) -> F::Key {
        let key = frequency.key();
        let at = match self.index(&key) {
            Some(at) => at,
            None => {
                frequency.set_valid(true);
                self.dirty = true;
                self.frequencies.push((key.clone(), frequency));
                self.frequencies.len() - 1
            }
        };
        if self.frequencies[at].1.update() {
            self.dirty = true;
        }
        key
    }

    /// `getOrCreateFrequency(identity, ownerUUID)`: a frequência da chave, criada (e a tabela suja) se não existe.
    pub fn get_or_create(
        &mut self,
        identity: &FrequencyIdentity<F::Key>,
        owner: Option<u128>,
        names: &dyn Fn(u128) -> String,
    ) -> &mut F {
        let at = match self.index(&identity.key) {
            Some(at) => at,
            None => {
                self.frequencies.push((
                    identity.key.clone(),
                    F::create(identity.key.clone(), owner, identity.security_mode, names),
                ));
                self.dirty = true;
                self.frequencies.len() - 1
            }
        };
        &mut self.frequencies[at].1
    }

    /// `addFrequency(freq)`.
    pub fn add(&mut self, frequency: F) {
        let key = frequency.key();
        match self.index(&key) {
            Some(at) => self.frequencies[at].1 = frequency,
            None => self.frequencies.push((key, frequency)),
        }
        self.dirty = true;
    }

    /// `FrequencyLookup.codec`, gravando: o dono da tabela (se há) e `freq_list`.
    pub fn encode(&self) -> Value {
        let mut out = Map::new();
        if let Some(owner) = self.owner {
            out.insert(OWNER_UUID.to_owned(), json!(uuid_ints(owner)));
        }
        out.insert(
            FREQUENCY_LIST.to_owned(),
            Value::Array(self.frequencies().map(Frequency::encode).collect()),
        );
        Value::Object(out)
    }

    /// `FrequencyLookup.codec`, lendo: a frequência que não decodifica fica de fora (`promotePartial`), e a tabela lida não está suja.
    pub fn decode(
        value: &Value,
        security_mode: SecurityMode,
        names: &dyn Fn(u128) -> String,
    ) -> Option<Self> {
        let list = value.get(FREQUENCY_LIST)?.as_array()?;
        let mut lookup = Self::new(value.get(OWNER_UUID).and_then(owner_of), security_mode);
        for frequency in list.iter().filter_map(|entry| F::decode(entry, names)) {
            let key = frequency.key();
            match lookup.index(&key) {
                Some(at) => lookup.frequencies[at].1 = frequency,
                None => lookup.frequencies.push((key, frequency)),
            }
        }
        Some(lookup)
    }
}

/// `FrequencyController`: as tabelas de um tipo. A pública nasce com o servidor; as privadas e as de confiança nascem quando alguém pede a de um dono.
#[derive(Clone, Debug)]
pub struct FrequencyController<F: Frequency> {
    controller_type: ControllerType,
    public: Option<FrequencyLookup<F>>,
    private: HashMap<u128, FrequencyLookup<F>>,
    trusted: HashMap<u128, FrequencyLookup<F>>,
}

impl<F: Frequency> FrequencyController<F> {
    /// `FrequencyController.create`, com a tabela pública que o disco tinha (`SavedDataStorage.computeIfAbsent`), ou vazia.
    pub fn new(controller_type: ControllerType, public: Option<FrequencyLookup<F>>) -> Self {
        let public = controller_type
            .supports(SecurityMode::Public)
            .then(|| public.unwrap_or_else(|| FrequencyLookup::new(None, SecurityMode::Public)));
        Self {
            controller_type,
            public,
            private: HashMap::new(),
            trusted: HashMap::new(),
        }
    }

    pub fn controller_type(&self) -> ControllerType {
        self.controller_type
    }

    /// `getPublicLookup`: `None` se o tipo não tem tabela pública (o original registra um erro).
    pub fn public(&self) -> Option<&FrequencyLookup<F>> {
        self.public.as_ref()
    }

    pub fn public_mut(&mut self) -> Option<&mut FrequencyLookup<F>> {
        self.public.as_mut()
    }

    /// `FrequencyType.getLookup(owner, securityMode)`: a tabela do modo, criada se é a primeira vez (`loaded` é a que o disco tinha para esse dono). Sem dono, ou num
    /// modo que o tipo não tem, `None` (o original registra um erro).
    pub fn lookup(
        &mut self,
        owner: Option<u128>,
        mode: SecurityMode,
        loaded: impl FnOnce(u128, SecurityMode) -> Option<FrequencyLookup<F>>,
    ) -> Option<&mut FrequencyLookup<F>> {
        if !self.controller_type.supports(mode) {
            return None;
        }
        let table = match mode {
            SecurityMode::Public => return self.public.as_mut(),
            SecurityMode::Private => &mut self.private,
            SecurityMode::Trusted => &mut self.trusted,
        };
        let owner = owner?;
        Some(table.entry(owner).or_insert_with(|| {
            loaded(owner, mode).unwrap_or_else(|| FrequencyLookup::new(Some(owner), mode))
        }))
    }

    /// `FrequencyController.clear`: as tabelas privadas e de confiança somem (a pública fica).
    pub fn clear(&mut self) {
        self.private.clear();
        self.trusted.clear();
    }
}
