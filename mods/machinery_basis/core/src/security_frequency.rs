//! A frequência de segurança de um dono (`SecurityFrequency`): o modo dela, a sobreposição (`override`) e a lista dos jogadores de confiança, que a Security Desk
//! edita. Uma por dono, todas na tabela pública do tipo `security` (`FrequencyTypes.SECURITY`, `FrequencyController.Type.PUBLIC_ONLY`).
//!
//! Cada conta vem de `dev/referencia/rastros/security_desk.jsonl` (`testes/SecurityDeskTraceTest.java`): o NBT do `CODEC`, os bytes do `STREAM_CODEC`, o `hashCode`
//! e o `getSyncHash`, a lista e a matriz de acesso a cada passo.
//!
//! O que parece erro e é do original (cada item está em "Comportamento do original que parece bug e fica"):
//! - o `hashCode` de uma frequência de segurança só leva o nome e o dono, e é ele que o menu usa para saber se a frequência mudou
//!   (`SyncableStreamCodec.getValueHashCode`): a lista, o modo e a sobreposição só chegam à tela quando o menu abre. O `getSyncHash`, que os leva, não é chamado;
//! - `trustedCacheHash` nasce zero e só é calculado ao pôr ou tirar um nome; a frequência que o cliente lê do pacote fica com zero;
//! - a lista de nomes (`trustedCache`) é um `HashList`: dois jogadores de confiança com o mesmo nome guardado ocupam uma linha só, e `removeTrusted(i)` confere o
//!   índice em cada lista por conta própria;
//! - nada disso suja a tabela (ver [`crate::frequency`]).

use crate::frequency::{ControllerType, Frequency, FrequencyController, FrequencyIdentity, FrequencyLookup};
use crate::recipe_data::SecurityMode;
use crate::security::{OWNER_UUID, SECURITY_MODE, SecurityData, java_string_hash, owner_of, uuid_hash, uuid_ints};
use serde_json::{Map, Value, json};

/// `FrequencyTypes.SECURITY`: o nome do tipo (a chave no `component_frequency` do tile e a pasta do arquivo).
pub const TYPE_NAME: &str = "security";
/// `SecurityFrequency.SECURITY`: o nome de toda frequência de segurança.
pub const NAME: &str = "Security";
/// `SerializationConstants.OVERRIDE` e `TRUSTED`.
pub const OVERRIDE: &str = "override";
pub const TRUSTED: &str = "trusted";
/// `FrequencyController.Type.PUBLIC_ONLY`.
pub const CONTROLLER_TYPE: ControllerType = ControllerType::PublicOnly;
/// `PacketUtils.LAST_USERNAME_LENGTH` não limita o que se grava; `MekanismUtils.getLastKnownUsername(null)`.
pub const UNKNOWN_OWNER_NAME: &str = "<???>";

/// `SecurityFrequency`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecurityFrequency {
    owner: Option<u128>,
    owner_name: String,
    security_mode: SecurityMode,
    overridden: bool,
    trusted: Vec<u128>,
    trusted_cache: Vec<String>,
    trusted_cache_hash: i32,
    valid: bool,
    removed: bool,
}

/// A tabela das frequências de segurança e o controlador dela.
pub type SecurityLookup = FrequencyLookup<SecurityFrequency>;
pub type SecurityController = FrequencyController<SecurityFrequency>;

fn put_var_int(out: &mut Vec<u8>, mut value: u32) {
    while value & !0x7F != 0 {
        out.push((value & 0x7F) as u8 | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn put_string(out: &mut Vec<u8>, text: &str) {
    put_var_int(out, text.len() as u32);
    out.extend_from_slice(text.as_bytes());
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn byte(&mut self) -> Option<u8> {
        let byte = *self.bytes.get(self.at)?;
        self.at += 1;
        Some(byte)
    }

    fn var_int(&mut self) -> Option<u32> {
        let mut value = 0u32;
        for shift in (0..35).step_by(7) {
            let byte = self.byte()?;
            value |= u32::from(byte & 0x7F) << shift;
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }

    fn string(&mut self) -> Option<String> {
        let len = self.var_int()? as usize;
        let text = std::str::from_utf8(self.bytes.get(self.at..self.at.checked_add(len)?)?).ok()?.to_owned();
        self.at += len;
        Some(text)
    }

    fn uuid(&mut self) -> Option<u128> {
        let value = u128::from_be_bytes(self.bytes.get(self.at..self.at + 16)?.try_into().ok()?);
        self.at += 16;
        Some(value)
    }
}

/// `List.hashCode` de uma lista de `String`.
fn names_hash(names: &[String]) -> i32 {
    names.iter().fold(1i32, |hash, name| hash.wrapping_mul(31).wrapping_add(java_string_hash(name)))
}

impl SecurityFrequency {
    /// `new SecurityFrequency(uuid, securityMode)`: o nome do dono é o último que o servidor viu (`<???>` sem dono).
    pub fn new(owner: Option<u128>, security_mode: SecurityMode, names: &dyn Fn(u128) -> String) -> Self {
        let owner_name = owner.map_or_else(|| UNKNOWN_OWNER_NAME.to_owned(), names);
        Self::with_owner_name(owner, owner_name, security_mode)
    }

    fn with_owner_name(owner: Option<u128>, owner_name: String, security_mode: SecurityMode) -> Self {
        Self { owner, owner_name, security_mode, overridden: false, trusted: Vec::new(), trusted_cache: Vec::new(), trusted_cache_hash: 0, valid: true, removed: false }
    }

    pub fn owner(&self) -> Option<u128> {
        self.owner
    }

    /// `Frequency.getOwnerName`.
    pub fn owner_name(&self) -> &str {
        &self.owner_name
    }

    /// `Frequency.getSecurity`: o modo que a mesa escolhe (não o da identidade, que é sempre `PUBLIC`).
    pub fn security(&self) -> SecurityMode {
        self.security_mode
    }

    pub fn is_overridden(&self) -> bool {
        self.overridden
    }

    pub fn is_valid(&self) -> bool {
        self.valid
    }

    pub fn is_removed(&self) -> bool {
        self.removed
    }

    /// `new SecurityData(frequency)`.
    pub fn data(&self) -> SecurityData {
        SecurityData { mode: self.security_mode, overridden: self.overridden }
    }

    /// `Frequency.setSecurityMode`.
    pub fn set_security_mode(&mut self, mode: SecurityMode) {
        self.security_mode = mode;
    }

    /// `setOverridden`.
    pub fn set_overridden(&mut self, overridden: bool) {
        self.overridden = overridden;
    }

    /// `isTrusted(subject)`.
    pub fn is_trusted(&self, subject: u128) -> bool {
        self.trusted.contains(&subject)
    }

    /// Os jogadores de confiança, na ordem em que entraram.
    pub fn trusted(&self) -> &[u128] {
        &self.trusted
    }

    /// `getTrustedUsernameCache`: os nomes que a tela lista.
    pub fn trusted_names(&self) -> &[String] {
        &self.trusted_cache
    }

    /// `addTrusted(uuid, name)`: quem já está na lista não entra de novo. Devolve se entrou.
    pub fn add_trusted(&mut self, uuid: u128, name: &str) -> bool {
        if self.trusted.contains(&uuid) {
            return false;
        }
        self.add_trusted_raw(uuid, name);
        true
    }

    /// `addTrustedRaw`: as duas listas são `HashList`, então um nome que já está na lista de nomes não entra de novo nela.
    fn add_trusted_raw(&mut self, uuid: u128, name: &str) {
        if !self.trusted.contains(&uuid) {
            self.trusted.push(uuid);
        }
        if !self.trusted_cache.iter().any(|known| known == name) {
            self.trusted_cache.push(name.to_owned());
        }
        self.trusted_cache_hash = names_hash(&self.trusted_cache);
    }

    /// `removeTrusted(index)`: tira o jogador e o nome daquela posição, cada lista conferindo o índice por si. Devolve quem saiu.
    pub fn remove_trusted(&mut self, index: i32) -> Option<u128> {
        let at = usize::try_from(index).ok();
        let removed = at.filter(|at| *at < self.trusted.len()).map(|at| self.trusted.remove(at));
        if let Some(at) = at.filter(|at| *at < self.trusted_cache.len()) {
            self.trusted_cache.remove(at);
            self.trusted_cache_hash = names_hash(&self.trusted_cache);
        }
        removed
    }

    /// `Frequency.hashCode` de uma frequência de segurança: o nome e o dono (o modo só entra nos outros tipos).
    pub fn hash_code(&self) -> i32 {
        let mut code = 31i32.wrapping_add(java_string_hash(NAME));
        if let Some(owner) = self.owner {
            code = code.wrapping_mul(31).wrapping_add(uuid_hash(owner));
        }
        code
    }

    /// `SecurityFrequency.getSyncHash`: o `hashCode`, a sobreposição, o modo e o hash da lista de nomes. Nenhum código do original o chama.
    pub fn sync_hash(&self) -> i32 {
        self.hash_code()
            .wrapping_mul(31)
            .wrapping_add(i32::from(self.overridden))
            .wrapping_mul(31)
            .wrapping_add(self.security_mode.ordinal() as i32)
            .wrapping_mul(31)
            .wrapping_add(self.trusted_cache_hash)
    }

    /// `SecurityFrequency.getIdentity`: a chave e o dono são o dono, e o modo é sempre `PUBLIC`.
    pub fn identity(&self) -> FrequencyIdentity<Option<u128>> {
        FrequencyIdentity { key: self.owner, security_mode: SecurityMode::Public, owner: self.owner }
    }

    /// `IdentitySerializer.UUID`, gravando: o que o tile guarda em `component_frequency.security`.
    pub fn identity_value(owner: u128) -> Value {
        json!({ OWNER_UUID: uuid_ints(owner), SECURITY_MODE: SecurityMode::Public.serialized_name() })
    }

    /// `IdentitySerializer.UUID`, lendo: o dono (o modo é lido e a identidade fica com ele, mas a frequência de segurança é sempre da tabela pública).
    pub fn identity_owner(value: &Value) -> Option<(u128, SecurityMode)> {
        let owner = owner_of(value.get(OWNER_UUID)?)?;
        let mode = SecurityMode::from_name(value.get(SECURITY_MODE)?.as_str()?)?;
        Some((owner, mode))
    }

    /// `SecurityFrequency.STREAM_CODEC`, gravando: o dono opcional, o nome dele, o modo, a sobreposição e os nomes de confiança.
    pub fn stream_encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match self.owner {
            Some(owner) => {
                out.push(1);
                out.extend_from_slice(&owner.to_be_bytes());
            }
            None => out.push(0),
        }
        put_string(&mut out, &self.owner_name);
        put_var_int(&mut out, self.security_mode.ordinal() as u32);
        out.push(u8::from(self.overridden));
        put_var_int(&mut out, self.trusted_cache.len() as u32);
        for name in &self.trusted_cache {
            put_string(&mut out, name);
        }
        out
    }

    /// `SecurityFrequency.STREAM_CODEC`, lendo: a frequência que o cliente tem, sem os UUIDs de confiança e com o hash da lista em zero. A lista lida é um
    /// `HashList`: um nome repetido no pacote entra uma vez.
    pub fn stream_decode(bytes: &[u8]) -> Option<Self> {
        let mut reader = Reader { bytes, at: 0 };
        let owner = if reader.byte()? != 0 { Some(reader.uuid()?) } else { None };
        let owner_name = reader.string()?;
        let mode = SecurityMode::by_id(reader.var_int()? as i32);
        let overridden = reader.byte()? != 0;
        let count = reader.var_int()?;
        let mut frequency = Self::with_owner_name(owner, owner_name, mode);
        frequency.overridden = overridden;
        for _ in 0..count {
            let name = reader.string()?;
            if !frequency.trusted_cache.contains(&name) {
                frequency.trusted_cache.push(name);
            }
        }
        Some(frequency)
    }

    /// O dado do menu (`SyncableStreamCodec.frequency(SECURITY, ...)`): o `hashCode` que diz se mudou (zero sem frequência) e os bytes (`false`, ou `true` e o
    /// `STREAM_CODEC`).
    pub fn menu_sync(frequency: Option<&Self>) -> (i32, Vec<u8>) {
        match frequency {
            None => (0, vec![0]),
            Some(frequency) => {
                let mut bytes = vec![1];
                bytes.extend(frequency.stream_encode());
                (frequency.hash_code(), bytes)
            }
        }
    }

    /// O dado do menu, lendo.
    pub fn menu_read(bytes: &[u8]) -> Option<Self> {
        match bytes.split_first() {
            Some((present, rest)) if *present != 0 => Self::stream_decode(rest),
            _ => None,
        }
    }
}

impl Frequency for SecurityFrequency {
    type Key = Option<u128>;

    fn key(&self) -> Option<u128> {
        self.owner
    }

    fn owner(&self) -> Option<u128> {
        self.owner
    }

    fn set_valid(&mut self, valid: bool) {
        self.valid = valid;
    }

    fn set_removed(&mut self) {
        self.removed = true;
    }

    fn create(_key: Option<u128>, owner: Option<u128>, security_mode: SecurityMode, names: &dyn Fn(u128) -> String) -> Self {
        Self::new(owner, security_mode, names)
    }

    /// `SecurityFrequency.CODEC`, gravando: o dono (se há), o modo, a sobreposição e, se a lista não é vazia, os jogadores de confiança.
    fn encode(&self) -> Value {
        let mut out = Map::new();
        if let Some(owner) = self.owner {
            out.insert(OWNER_UUID.to_owned(), json!(uuid_ints(owner)));
        }
        if !self.trusted.is_empty() {
            out.insert(TRUSTED.to_owned(), Value::Array(self.trusted.iter().map(|uuid| json!(uuid_ints(*uuid))).collect()));
        }
        out.insert(SECURITY_MODE.to_owned(), json!(self.security_mode.serialized_name()));
        out.insert(OVERRIDE.to_owned(), json!(u8::from(self.overridden)));
        Value::Object(out)
    }

    /// `SecurityFrequency.CODEC`, lendo: o nome de cada jogador de confiança é o último que o servidor viu (`addTrustedRaw(trusted, getLastKnownUsername(trusted))`).
    fn decode(value: &Value, names: &dyn Fn(u128) -> String) -> Option<Self> {
        let owner = match value.get(OWNER_UUID) {
            Some(owner) => Some(owner_of(owner)?),
            None => None,
        };
        let mode = SecurityMode::from_name(value.get(SECURITY_MODE)?.as_str()?)?;
        let overridden = match value.get(OVERRIDE)? {
            Value::Bool(flag) => *flag,
            other => other.as_i64()? != 0,
        };
        let mut frequency = Self::new(owner, mode, names);
        frequency.overridden = overridden;
        if let Some(trusted) = value.get(TRUSTED) {
            for entry in trusted.as_array()? {
                let uuid = owner_of(entry)?;
                frequency.add_trusted_raw(uuid, &names(uuid));
            }
        }
        Some(frequency)
    }
}

/// `SecurityUtils.getData(uuid, false)`: a `SecurityData` da frequência do dono na tabela, ou `DUMMY`.
pub fn server_data(lookup: Option<&SecurityLookup>, owner: Option<u128>) -> SecurityData {
    owner
        .and_then(|owner| lookup?.get(&Some(owner)))
        .map_or(SecurityData::DUMMY, SecurityFrequency::data)
}

/// O `isTrusted` que `canAccessObject` pergunta no servidor: sem frequência, como `PRIVATE`.
pub fn server_trusted(lookup: Option<&SecurityLookup>, owner: u128, player: u128) -> bool {
    lookup.and_then(|lookup| lookup.get(&Some(owner))).is_some_and(|frequency| frequency.is_trusted(player))
}

/// `TileComponentSecurity.setOwnerUUID` no servidor (`tile.setFrequency(SECURITY, FrequencyIdentity(owner, PUBLIC, owner), owner)`): a frequência do dono, criada se
/// ainda não existe (a tabela fica suja).
pub fn frequency_of_owner<'a>(lookup: &'a mut SecurityLookup, owner: u128, names: &dyn Fn(u128) -> String) -> &'a mut SecurityFrequency {
    let identity = FrequencyIdentity { key: Some(owner), security_mode: SecurityMode::Public, owner: Some(owner) };
    lookup.get_or_create(&identity, Some(owner), names)
}

/// `TileComponentFrequency.updateFrequency` de um tile que leu a frequência do NBT (inválida): a tabela a valida. Se não a tinha, entra uma nova do dono (`PUBLIC`,
/// sem lista) e a tabela fica suja.
pub fn validate_owner(lookup: &mut SecurityLookup, owner: u128, names: &dyn Fn(u128) -> String) {
    let mut fresh = SecurityFrequency::new(Some(owner), SecurityMode::Public, names);
    fresh.valid = false;
    lookup.validate_and_update(fresh);
}

/// `PacketSyncSecurity`: o dono, o nome dele e, se veio de uma frequência, a `SecurityData`. É o que enche `clientUUIDMap` e `clientSecurityMap`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncSecurity {
    pub player: u128,
    pub name: String,
    pub data: Option<SecurityData>,
}

impl SyncSecurity {
    /// `new PacketSyncSecurity(frequency)`. Uma frequência sem dono não tem pacote (o original daria erro ao gravar o UUID nulo).
    pub fn of_frequency(frequency: &SecurityFrequency) -> Option<Self> {
        Some(Self { player: frequency.owner()?, name: frequency.owner_name().to_owned(), data: Some(frequency.data()) })
    }

    /// `new PacketSyncSecurity(uuid)`: só o nome, quando alguém põe um bloco ou reclama um item.
    pub fn of_player(player: u128, names: &dyn Fn(u128) -> String) -> Self {
        Self { player, name: names(player), data: None }
    }

    /// `PacketSyncSecurity.STREAM_CODEC`, gravando.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = self.player.to_be_bytes().to_vec();
        put_string(&mut out, &self.name);
        match self.data {
            Some(data) => {
                out.push(1);
                put_var_int(&mut out, data.mode.ordinal() as u32);
                out.push(u8::from(data.overridden));
            }
            None => out.push(0),
        }
        out
    }

    /// `PacketSyncSecurity.STREAM_CODEC`, lendo.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let mut reader = Reader { bytes, at: 0 };
        let player = reader.uuid()?;
        let name = reader.string()?;
        let data = if reader.byte()? != 0 {
            let mode = SecurityMode::by_id(reader.var_int()? as i32);
            Some(SecurityData { mode, overridden: reader.byte()? != 0 })
        } else {
            None
        };
        Some(Self { player, name, data })
    }
}

/// `PacketBatchSecurityUpdate`: todas as frequências com dono, para quem entra no servidor (`SyncAllSecurityData`).
pub fn batch(lookup: &SecurityLookup) -> Vec<SyncSecurity> {
    lookup.frequencies().filter_map(SyncSecurity::of_frequency).collect()
}

/// `MekanismClient.clientSecurityMap` e `clientUUIDMap`: o que o cliente sabe da segurança de cada dono.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClientSecurity {
    data: std::collections::HashMap<u128, SecurityData>,
    names: std::collections::HashMap<u128, String>,
}

impl ClientSecurity {
    /// `PacketSyncSecurity.handle`: o nome sempre, a `SecurityData` se veio.
    pub fn sync(&mut self, packet: SyncSecurity) {
        self.names.insert(packet.player, packet.name);
        if let Some(data) = packet.data {
            self.data.insert(packet.player, data);
        }
    }

    /// `PacketBatchSecurityUpdate.handle`: o mapa de segurança é trocado inteiro e os nomes entram por cima dos que havia.
    pub fn batch(&mut self, packets: Vec<SyncSecurity>) {
        self.data.clear();
        for packet in packets {
            self.names.insert(packet.player, packet.name);
            if let Some(data) = packet.data {
                self.data.insert(packet.player, data);
            }
        }
    }

    /// `MekanismClient.reset`: ao sair do servidor.
    pub fn reset(&mut self) {
        self.data.clear();
        self.names.clear();
    }

    /// `SecurityUtils.getData(uuid, true)`.
    pub fn data(&self, owner: Option<u128>) -> SecurityData {
        owner.and_then(|owner| self.data.get(&owner).copied()).unwrap_or(SecurityData::DUMMY)
    }

    /// `clientUUIDMap.get(uuid)`.
    pub fn name(&self, owner: u128) -> Option<&str> {
        self.names.get(&owner).map(String::as_str)
    }
}
