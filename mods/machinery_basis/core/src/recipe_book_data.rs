//! Os dados do livro de receitas de um mod: o que o servidor sabe do jogador (as receitas desbloqueadas e o `RecipeBookSettings` de
//! cada tipo de livro) e os pacotes que levam isso ao cliente, como o vanilla faz com `ClientboundRecipeBookAddPacket`,
//! `ClientboundRecipeBookRemovePacket`, `ClientboundRecipeBookSettingsPacket`, `ServerboundRecipeBookChangeSettingsPacket` e
//! `ServerboundRecipeBookSeenRecipePacket`.
//!
//! O Horizon tem o mesmo para as quatro telas do vanilla (`recipe_book.rs`: `Book`, `Settings`, `RecipeIndex`), mas o `RecipeIndex` lê só
//! `data/minecraft/recipe` e as `Settings` só têm os quatro tipos do vanilla: as receitas do Mekanism e os tipos `MEKANISM_*` ficam aqui.
//! Nada conhece um id de receita, de químico ou de máquina: a tabela dos displays entra por parâmetro.
use std::collections::{BTreeMap, BTreeSet};

use crate::recipe_book::Entry;
use crate::sim::java_hash::{JavaIntMap, identity_set_order};

/// `RecipeBookSettings.TypeSettings`: o livro aberto e o filtro de receitas que dão para fazer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TypeSettings {
    pub open: bool,
    pub filtering: bool,
}

/// `RecipeBookSettings` dos tipos de mod: um `TypeSettings` por `RecipeBookType`, e o padrão (`DEFAULT`, tudo falso) para o que falta.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    types: BTreeMap<String, TypeSettings>,
}

/// `CaseFormat.UPPER_UNDERSCORE.to(UPPER_CAMEL, name)`: `MEKANISM_ENRICHING` vira `MekanismEnriching`.
pub fn upper_camel(name: &str) -> String {
    name.split('_')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
            })
        })
        .collect()
}

/// Os dois campos que `CommonHooks.makeModdedRecipeBookTypesSettingsCodec` grava no NBT do jogador para um tipo de livro de mod:
/// `is<Nome>GuiOpen` e `is<Nome>FilteringCraftable`.
pub fn settings_fields(book_type: &str) -> (String, String) {
    let name = upper_camel(book_type);
    (
        format!("is{name}GuiOpen"),
        format!("is{name}FilteringCraftable"),
    )
}

impl Settings {
    /// `getSettings(type)`: o padrão para um tipo que ninguém mexeu.
    pub fn get(&self, book_type: &str) -> TypeSettings {
        self.types.get(book_type).copied().unwrap_or_default()
    }

    pub fn is_open(&self, book_type: &str) -> bool {
        self.get(book_type).open
    }

    pub fn is_filtering(&self, book_type: &str) -> bool {
        self.get(book_type).filtering
    }

    pub fn set_open(&mut self, book_type: &str, open: bool) {
        self.types.entry(book_type.to_owned()).or_default().open = open;
    }

    pub fn set_filtering(&mut self, book_type: &str, filtering: bool) {
        self.types
            .entry(book_type.to_owned())
            .or_default()
            .filtering = filtering;
    }

    /// O `TypeSettings` de um tipo, inteiro.
    pub fn set(&mut self, book_type: &str, settings: TypeSettings) {
        self.types.insert(book_type.to_owned(), settings);
    }

    /// As configurações dos tipos `book_types` com o nome de cada um, para o pacote ao cliente: os tipos de livro de mod são registrados
    /// quando o bloco deles aparece, então o servidor e o cliente não têm a mesma lista nem a mesma ordem, e o nome vai junto.
    pub fn encode_named(&self, book_types: &[&str]) -> Vec<u8> {
        let mut bytes = vec![book_types.len() as u8];
        for book_type in book_types {
            let settings = self.get(book_type);
            put_str(&mut bytes, book_type);
            bytes.push(u8::from(settings.open));
            bytes.push(u8::from(settings.filtering));
        }
        bytes
    }

    pub fn decode_named(bytes: &[u8]) -> Option<Vec<(String, TypeSettings)>> {
        let mut reader = Reader(bytes);
        let count = reader.u8()?;
        let mut out = Vec::new();
        for _ in 0..count {
            let name = reader.str()?;
            let open = reader.u8()? != 0;
            let filtering = reader.u8()? != 0;
            out.push((name, TypeSettings { open, filtering }));
        }
        reader.0.is_empty().then_some(out)
    }

    /// `replaceFrom(other)`.
    pub fn replace_from(&mut self, other: &Settings) {
        self.types = other.types.clone();
    }

    /// `TypeSettings.codec(openName, filteringName)` na leitura: cada campo ausente vale falso (`optionalFieldOf(..., false)`).
    pub fn read(&mut self, book_types: &[&str], field: impl Fn(&str) -> Option<bool>) {
        for book_type in book_types {
            let (open, filtering) = settings_fields(book_type);
            self.types.insert(
                (*book_type).to_owned(),
                TypeSettings {
                    open: field(&open).unwrap_or(false),
                    filtering: field(&filtering).unwrap_or(false),
                },
            );
        }
    }

    /// A escrita: os dois campos de cada tipo, na ordem pedida.
    pub fn write(&self, book_types: &[&str]) -> Vec<(String, bool)> {
        let mut fields = Vec::new();
        for book_type in book_types {
            let (open, filtering) = settings_fields(book_type);
            let settings = self.get(book_type);
            fields.push((open, settings.open));
            fields.push((filtering, settings.filtering));
        }
        fields
    }

    /// `MODDED_RECIPE_BOOK_TYPES_SETTINGS_STREAM_CODEC`: um par de booleanos por tipo de mod, na ordem do enum.
    pub fn encode(&self, book_types: &[&str]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(2 * book_types.len());
        for book_type in book_types {
            let settings = self.get(book_type);
            bytes.push(u8::from(settings.open));
            bytes.push(u8::from(settings.filtering));
        }
        bytes
    }

    pub fn decode(book_types: &[&str], bytes: &[u8]) -> Option<Settings> {
        if bytes.len() != 2 * book_types.len() {
            return None;
        }
        let mut settings = Settings::default();
        for (index, book_type) in book_types.iter().enumerate() {
            settings.types.insert(
                (*book_type).to_owned(),
                TypeSettings {
                    open: bytes[2 * index] != 0,
                    filtering: bytes[2 * index + 1] != 0,
                },
            );
        }
        Some(settings)
    }
}

/// `ServerboundRecipeBookChangeSettingsPacket(bookType, isOpen, isFiltering)`: o cliente avisa o servidor quando o livro abre ou fecha e
/// quando o filtro muda (`RecipeBookComponent.sendUpdateSettings`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeSettings {
    pub book_type: String,
    pub open: bool,
    pub filtering: bool,
}

/// Um `RecipeDisplayEntry` mais as flags do `ClientboundRecipeBookAddPacket.Entry`: 1 avisa (o toast de receita nova) e 2 destaca.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddedEntry {
    pub entry: Entry,
    pub notification: bool,
    pub highlight: bool,
}

/// O que o servidor manda ao cliente do livro.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToClient {
    /// `ClientboundRecipeBookAddPacket(entries, replace)`.
    Add {
        entries: Vec<AddedEntry>,
        replace: bool,
    },
    /// `ClientboundRecipeBookRemovePacket(ids)`.
    Remove(Vec<i32>),
    /// `ClientboundRecipeBookSettingsPacket(settings)`.
    Settings(Vec<u8>),
    /// `ClientboundPlaceGhostRecipePacket(containerId, display)`: a receita não coube nos slots e o fantasma dela aparece na tela
    /// aberta. O display vai pelo id (o cliente tem a tabela).
    PlaceGhost(i32),
}

/// O que o cliente manda ao servidor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToServer {
    ChangeSettings(ChangeSettings),
    /// `ServerboundRecipeBookSeenRecipePacket(recipe)`: o jogador viu a receita e o destaque dela cai.
    SeenRecipe(i32),
}

const ADD: u8 = 0;
const REMOVE: u8 = 1;
const SETTINGS: u8 = 2;
const GHOST: u8 = 3;
const CHANGE: u8 = 0;
const SEEN: u8 = 1;

fn put_str(out: &mut Vec<u8>, text: &str) {
    out.extend_from_slice(&(text.len() as u16).to_be_bytes());
    out.extend_from_slice(text.as_bytes());
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.0.len() < n {
            return None;
        }
        let (head, tail) = self.0.split_at(n);
        self.0 = tail;
        Some(head)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }
    fn i32(&mut self) -> Option<i32> {
        self.take(4)
            .map(|b| i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn str(&mut self) -> Option<String> {
        let len = self.take(2).map(|b| u16::from_be_bytes([b[0], b[1]]))? as usize;
        String::from_utf8(self.take(len)?.to_vec()).ok()
    }
}

impl ToClient {
    /// O pacote em pedaços de até `max` bytes codificados, para um canal com teto de tamanho: o primeiro pedaço de um `Add` leva o
    /// `replace` e os seguintes não, e a ordem das entradas é a mesma (o cliente que aplica os pedaços em ordem fica como ficaria com
    /// o pacote inteiro). O livro de quem conhece tudo tem milhares de displays.
    pub fn split(self, max: usize) -> Vec<ToClient> {
        match self {
            ToClient::Add { entries, replace } => {
                let mut out = Vec::new();
                let mut chunk: Vec<AddedEntry> = Vec::new();
                let mut size = 6;
                for added in entries {
                    let bytes = 11 + added.entry.category.len();
                    if size + bytes > max && !chunk.is_empty() {
                        out.push(ToClient::Add {
                            entries: std::mem::take(&mut chunk),
                            replace: replace && out.is_empty(),
                        });
                        size = 6;
                    }
                    size += bytes;
                    chunk.push(added);
                }
                if !chunk.is_empty() || out.is_empty() {
                    out.push(ToClient::Add {
                        entries: chunk,
                        replace: replace && out.is_empty(),
                    });
                }
                out
            }
            ToClient::Remove(ids) => {
                let per = (max.saturating_sub(5) / 4).max(1);
                if ids.len() <= per {
                    return vec![ToClient::Remove(ids)];
                }
                ids.chunks(per)
                    .map(|chunk| ToClient::Remove(chunk.to_vec()))
                    .collect()
            }
            other => vec![other],
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            ToClient::Add { entries, replace } => {
                out.push(ADD);
                out.push(u8::from(*replace));
                out.extend_from_slice(&(entries.len() as i32).to_be_bytes());
                for added in entries {
                    out.extend_from_slice(&added.entry.id.to_be_bytes());
                    put_str(&mut out, &added.entry.category);
                    out.extend_from_slice(&added.entry.group.unwrap_or(-1).to_be_bytes());
                    out.push(u8::from(added.notification) | (u8::from(added.highlight) << 1));
                }
            }
            ToClient::Remove(ids) => {
                out.push(REMOVE);
                out.extend_from_slice(&(ids.len() as i32).to_be_bytes());
                for id in ids {
                    out.extend_from_slice(&id.to_be_bytes());
                }
            }
            ToClient::Settings(bytes) => {
                out.push(SETTINGS);
                out.extend_from_slice(bytes);
            }
            ToClient::PlaceGhost(id) => {
                out.push(GHOST);
                out.extend_from_slice(&id.to_be_bytes());
            }
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Option<ToClient> {
        let mut reader = Reader(bytes);
        match reader.u8()? {
            ADD => {
                let replace = reader.u8()? != 0;
                let count = reader.i32()?;
                let mut entries = Vec::new();
                for _ in 0..count {
                    let id = reader.i32()?;
                    let category = reader.str()?;
                    let group = reader.i32()?;
                    let flags = reader.u8()?;
                    entries.push(AddedEntry {
                        entry: Entry {
                            id,
                            category,
                            group: (group >= 0).then_some(group),
                        },
                        notification: flags & 1 != 0,
                        highlight: flags & 2 != 0,
                    });
                }
                Some(ToClient::Add { entries, replace })
            }
            REMOVE => {
                let count = reader.i32()?;
                let mut ids = Vec::new();
                for _ in 0..count {
                    ids.push(reader.i32()?);
                }
                Some(ToClient::Remove(ids))
            }
            SETTINGS => Some(ToClient::Settings(reader.0.to_vec())),
            GHOST => Some(ToClient::PlaceGhost(reader.i32()?)),
            _ => None,
        }
    }
}

impl ToServer {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            ToServer::ChangeSettings(change) => {
                out.push(CHANGE);
                put_str(&mut out, &change.book_type);
                out.push(u8::from(change.open));
                out.push(u8::from(change.filtering));
            }
            ToServer::SeenRecipe(id) => {
                out.push(SEEN);
                out.extend_from_slice(&id.to_be_bytes());
            }
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Option<ToServer> {
        let mut reader = Reader(bytes);
        match reader.u8()? {
            CHANGE => Some(ToServer::ChangeSettings(ChangeSettings {
                book_type: reader.str()?,
                open: reader.u8()? != 0,
                filtering: reader.u8()? != 0,
            })),
            SEEN => Some(ToServer::SeenRecipe(reader.i32()?)),
            _ => None,
        }
    }
}

/// Uma receita do jogo e os displays dela: `(receita, id do primeiro display, quantos)`. Os ids de uma receita são seguidos
/// (`RecipeManager.unpackRecipeInfo` numera os displays na ordem das receitas).
pub type DisplayRow = (&'static str, i32, u8);

/// Todos os displays do jogo, por receita, em ordem de id: a numeração do `RecipeManager` do original, de qualquer receita (as do
/// vanilla e as de todo mod), que o servidor usa para dizer ao cliente tudo o que o jogador conhece.
#[derive(Clone, Copy, Debug)]
pub struct Displays(pub &'static [DisplayRow]);

impl Displays {
    /// Cada display com a receita dele, em ordem de id.
    pub fn ids(&self) -> impl Iterator<Item = (i32, &'static str)> + '_ {
        self.0
            .iter()
            .flat_map(|row| (0..i32::from(row.2)).map(move |n| (row.1 + n, row.0)))
    }

    /// A linha da receita do display `id`.
    fn row_of(&self, id: i32) -> Option<usize> {
        let index = self.0.partition_point(|row| row.1 <= id).checked_sub(1)?;
        let row = self.0[index];
        (id < row.1 + i32::from(row.2)).then_some(index)
    }

    /// As linhas das receitas de `ids`, sem repetir, na ordem do nome (a de `recipes` no que o Horizon grava do jogador), e os ids
    /// que não são de receita nenhuma.
    fn rows_of(&self, ids: impl Iterator<Item = i32>) -> (Vec<usize>, Vec<i32>) {
        let mut rows = BTreeSet::new();
        let mut strays = Vec::new();
        for id in ids {
            match self.row_of(id) {
                Some(row) => {
                    rows.insert((self.0[row].0, row));
                }
                None => strays.push(id),
            }
        }
        (rows.into_iter().map(|(_, row)| row).collect(), strays)
    }

    fn expand(&self, rows: Vec<usize>, strays: Vec<i32>) -> Vec<i32> {
        rows.into_iter()
            .flat_map(|row| {
                let (_, first, count) = self.0[row];
                first..first + i32::from(count)
            })
            .chain(strays)
            .collect()
    }
}

/// A ordem em que os displays vão num pacote, que é a ordem em que entram no `HashMap` do `ClientRecipeBook` e decide a ordem dos que
/// dividem um balde.
pub struct SendOrder<'a> {
    pub displays: Displays,
    /// O `System.identityHashCode` da chave (`ResourceKey`) de uma receita.
    pub identity: &'a dyn Fn(&str) -> i32,
    /// O `hashCode` de quem premia uma receita fora de um comando (o `Listener` do avanço dela num `HashSet`, as receitas usadas de
    /// uma fornalha num mapa de identidade): no original sai de hashes de identidade.
    pub award: &'a dyn Fn(&str) -> i32,
    /// O que entrou veio de um `/recipe give` com várias receitas (`RecipeCommand.giveRecipes` com `getRecipes()`), que as dá na
    /// ordem do `RecipeManager`, a dos ids.
    pub ascending: bool,
}

impl SendOrder<'_> {
    /// `ServerRecipeBook.sendInitialRecipeBook`: as receitas conhecidas entram no `IdentityHashSet` na ordem em que foram lidas do
    /// jogador e saem na ordem da tabela dele; cada uma manda os displays dela em ordem.
    pub fn login(&self, known: &BTreeSet<i32>) -> Vec<i32> {
        let (rows, strays) = self.displays.rows_of(known.iter().copied());
        let ordered = identity_set_order(rows, |row| (self.identity)(self.displays.0[row].0));
        self.displays.expand(ordered, strays)
    }

    /// `ServerRecipeBook.addRecipes` do que entrou desde o último envio: em ordem de id quando veio do comando, senão na ordem de um
    /// `HashSet` dos hashes de quem premiou (que no original muda a cada execução).
    pub fn awarded(&self, added: &[i32]) -> Vec<i32> {
        if self.ascending {
            return added.to_vec();
        }
        let (rows, strays) = self.displays.rows_of(added.iter().copied());
        // A tabela de um `HashSet` novo: 16, dobrando quando o tamanho passa de 0,75 dela, cada balde na ordem de chegada. (Um balde
        // com nove nós viraria árvore no Java; com hashes sorteados isso não acontece na prática, e aqui ele só segue na ordem de
        // chegada, sem derrubar o servidor como o `hash_set_order` faria.)
        let mut capacity = 16_usize;
        while rows.len() > capacity * 3 / 4 {
            capacity *= 2;
        }
        let mut ordered: Vec<(usize, usize)> = rows
            .into_iter()
            .map(|row| {
                let hash = (self.award)(self.displays.0[row].0) as u32;
                ((hash ^ (hash >> 16)) as usize & (capacity - 1), row)
            })
            .collect();
        ordered.sort_by_key(|(bucket, _)| *bucket);
        self.displays
            .expand(ordered.into_iter().map(|(_, row)| row).collect(), strays)
    }
}

/// O `ClientRecipeBook` do jogador no cliente, sem as linhas de livro nenhum: os ids de todos os displays que ele conhece, na tabela
/// do `HashMap` do original (`known`), e os destacados.
#[derive(Clone, Debug, Default)]
pub struct ClientKnown {
    known: JavaIntMap<()>,
    highlight: BTreeSet<i32>,
}

impl ClientKnown {
    /// `ClientPacketListener.handleRecipeBookAdd` e `handleRecipeBookRemove`. Devolve se algo mudou.
    pub fn apply(&mut self, packet: &ToClient) -> bool {
        match packet {
            ToClient::Add { entries, replace } => {
                if *replace {
                    self.known.clear();
                    self.highlight.clear();
                }
                for added in entries {
                    self.known.put(added.entry.id, ());
                    if added.highlight {
                        self.highlight.insert(added.entry.id);
                    }
                }
                *replace || !entries.is_empty()
            }
            ToClient::Remove(ids) => {
                let mut changed = false;
                for id in ids {
                    self.highlight.remove(id);
                    changed |= self.known.remove(*id).is_some();
                }
                changed
            }
            ToClient::Settings(_) | ToClient::PlaceGhost(_) => false,
        }
    }

    /// `removeHighlight(id)`.
    pub fn seen(&mut self, id: i32) {
        self.highlight.remove(&id);
    }

    pub fn contains(&self, id: i32) -> bool {
        self.known.contains_key(id)
    }

    /// `known`, com a tabela como está.
    pub fn table(&self) -> &JavaIntMap<()> {
        &self.known
    }

    pub fn highlighted(&self) -> impl Iterator<Item = i32> + '_ {
        self.highlight.iter().copied()
    }
}

/// `ServerRecipeBook`: as receitas que o jogador desbloqueou (por id de display) e as que ainda estão destacadas (`highlight`, as que
/// ele ainda não viu). `add` e `remove` devolvem o que de fato mudou, que é o que o pacote leva.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Unlocked {
    known: BTreeSet<i32>,
    highlight: BTreeSet<i32>,
}

impl Unlocked {
    /// `ServerRecipeBook.add(id)`: verdadeiro se a receita é nova. Uma receita nova entra destacada (`addHighlight`) quando
    /// `highlight` é verdadeiro, que é o caso do desbloqueio em jogo (`addRecipes`) e não o da leitura do NBT.
    pub fn add(&mut self, id: i32, highlight: bool) -> bool {
        let added = self.known.insert(id);
        if added && highlight {
            self.highlight.insert(id);
        }
        added
    }

    pub fn remove(&mut self, id: i32) -> bool {
        self.highlight.remove(&id);
        self.known.remove(&id)
    }

    pub fn contains(&self, id: i32) -> bool {
        self.known.contains(&id)
    }

    pub fn is_highlighted(&self, id: i32) -> bool {
        self.highlight.contains(&id)
    }

    /// `removeHighlight(id)`, que o `SeenRecipe` aciona.
    pub fn seen(&mut self, id: i32) {
        self.highlight.remove(&id);
    }

    /// As receitas conhecidas, em ordem crescente de id (a ordem do NBT).
    pub fn known(&self) -> impl Iterator<Item = i32> + '_ {
        self.known.iter().copied()
    }

    pub fn highlighted(&self) -> impl Iterator<Item = i32> + '_ {
        self.highlight.iter().copied()
    }

    /// O livro do servidor visto pela tabela dos displays: de cada linha `(id, receita)`, o display é conhecido se a receita está em
    /// `ServerRecipeBook.known` e destacado se está em `highlight`.
    pub fn from_recipes<'a>(
        table: impl Iterator<Item = (i32, &'a str)>,
        known: impl Fn(&str) -> bool,
        highlight: impl Fn(&str) -> bool,
    ) -> Unlocked {
        let mut book = Unlocked::default();
        for (id, recipe) in table {
            if known(recipe) {
                book.add(id, highlight(recipe));
            }
        }
        book
    }

    /// O que falta mandar a um cliente que já recebeu `sent` para ele ficar com este livro. Sem `sent` é o
    /// `ServerRecipeBook.sendInitialRecipeBook` (tudo, `replace` e sem aviso, destacado o que está em `highlight`); com ele, o
    /// `ClientboundRecipeBookRemovePacket` do que saiu (`removeRecipes`) e o `ClientboundRecipeBookAddPacket` do que entrou
    /// (`addRecipes`: com o aviso de `showNotification` e destacado).
    pub fn sync(
        &self,
        sent: Option<&Unlocked>,
        entry: impl Fn(i32) -> Option<Entry>,
        order: &SendOrder<'_>,
    ) -> Vec<ToClient> {
        let added = |id: &i32, notification: bool| {
            entry(*id).map(|entry| AddedEntry {
                entry,
                notification,
                highlight: self.highlight.contains(id),
            })
        };
        let Some(sent) = sent else {
            return vec![ToClient::Add {
                entries: order
                    .login(&self.known)
                    .iter()
                    .filter_map(|id| added(id, false))
                    .collect(),
                replace: true,
            }];
        };
        let mut packets = Vec::new();
        let removed: Vec<i32> = sent.known.difference(&self.known).copied().collect();
        if !removed.is_empty() {
            packets.push(ToClient::Remove(removed));
        }
        let new: Vec<i32> = self.known.difference(&sent.known).copied().collect();
        let entries: Vec<AddedEntry> = order
            .awarded(&new)
            .iter()
            .filter_map(|id| added(id, true))
            .collect();
        if !entries.is_empty() {
            packets.push(ToClient::Add {
                entries,
                replace: false,
            });
        }
        packets
    }

    /// O cliente recebe um pacote do livro (`ClientRecipeBook.add`, `addHighlight`, `remove`, e o `clear` do `replace`). Devolve se
    /// algo mudou. O pacote de configurações não é daqui.
    pub fn apply(&mut self, packet: &ToClient) -> bool {
        match packet {
            ToClient::Add { entries, replace } => {
                if *replace {
                    self.known.clear();
                    self.highlight.clear();
                }
                for added in entries {
                    self.known.insert(added.entry.id);
                    if added.highlight {
                        self.highlight.insert(added.entry.id);
                    }
                }
                *replace || !entries.is_empty()
            }
            ToClient::Remove(ids) => {
                let mut changed = false;
                for id in ids {
                    changed |= self.remove(*id);
                }
                changed
            }
            ToClient::Settings(_) | ToClient::PlaceGhost(_) => false,
        }
    }

    /// `ServerRecipeBook.addRecipes` do que o jogador ganha: só as receitas novas entram no pacote, com aviso e destaque.
    pub fn award(&mut self, entries: &[Entry]) -> Option<ToClient> {
        let added: Vec<AddedEntry> = entries
            .iter()
            .filter(|entry| self.add(entry.id, true))
            .map(|entry| AddedEntry {
                entry: entry.clone(),
                notification: true,
                highlight: true,
            })
            .collect();
        (!added.is_empty()).then_some(ToClient::Add {
            entries: added,
            replace: false,
        })
    }

    /// `ServerRecipeBook.removeRecipes`: só as que ele conhecia entram no pacote.
    pub fn revoke(&mut self, ids: &[i32]) -> Option<ToClient> {
        let removed: Vec<i32> = ids.iter().copied().filter(|&id| self.remove(id)).collect();
        (!removed.is_empty()).then_some(ToClient::Remove(removed))
    }

    /// `ServerRecipeBook.sendInitialRecipeBook`: tudo o que ele conhece, com `replace`, e o destaque só das que ainda não viu.
    pub fn initial(&self, table: &[Entry]) -> ToClient {
        ToClient::Add {
            entries: table
                .iter()
                .filter(|entry| self.known.contains(&entry.id))
                .map(|entry| AddedEntry {
                    entry: entry.clone(),
                    notification: false,
                    highlight: self.highlight.contains(&entry.id),
                })
                .collect(),
            replace: true,
        }
    }
}

/// O que o servidor manda a um jogador para o livro dele ficar em dia (`ServerRecipeBook.sendInitialRecipeBook` na primeira vez,
/// depois `addRecipes` e `removeRecipes`): `before` é o que ele já recebeu (o livro e os tipos de livro das configurações), `now` o
/// livro de agora e `types` os tipos registrados. Nada quando os dois são iguais. As configurações (`settings`, já codificadas) vão com
/// o livro inteiro e de novo quando a lista de tipos muda (um tipo novo foi registrado).
pub fn server_sync(
    before: Option<(&Unlocked, &[&str])>,
    now: &Unlocked,
    types: &[&str],
    settings: impl FnOnce() -> Vec<u8>,
    entry: impl Fn(i32) -> Option<Entry>,
    order: &SendOrder<'_>,
) -> Vec<ToClient> {
    if before.is_some_and(|(sent, sent_types)| sent == now && sent_types == types) {
        return Vec::new();
    }
    let mut packets = Vec::new();
    if before.is_none_or(|(_, sent_types)| sent_types != types) {
        packets.push(ToClient::Settings(settings()));
    }
    packets.extend(now.sync(before.map(|(sent, _)| sent), entry, order));
    packets
}

/// O que volta ao livro de um jogador das listas lidas do arquivo dele (`recipes` e `toBeDisplayed` do `recipeBook` gravado), quando
/// quem abriu o mundo cortou do livro as receitas que não conhecia: as de `names` (as dos livros registrados) saem das duas listas e
/// são devolvidas, as conhecidas e, dentre elas, as destacadas. `seen` guarda quantos nomes havia e o tamanho da lista da última
/// conferência, para a lista só ser lida de novo quando um livro novo é registrado ou ela muda. `None` quando não há o que conferir.
pub fn restore_saved(
    recipes: &mut Vec<String>,
    to_be_displayed: &mut Vec<String>,
    names: &std::collections::HashSet<&str>,
    seen: &mut (usize, usize),
) -> Option<(Vec<String>, Vec<String>)> {
    let length = recipes.len();
    if *seen == (names.len(), length) {
        return None;
    }
    let take = |list: &mut Vec<String>| -> Vec<String> {
        let mut taken = Vec::new();
        list.retain(|name| {
            let ours = names.contains(name.as_str());
            if ours {
                taken.push(name.clone());
            }
            !ours
        });
        taken
    };
    let known = take(recipes);
    let highlight: Vec<String> = take(to_be_displayed)
        .into_iter()
        .filter(|name| known.contains(name))
        .collect();
    *seen = (names.len(), length - known.len());
    Some((known, highlight))
}

/// O que o cliente faz com um pacote: `ClientRecipeBook` mais o destaque, e o toast de cada receita nova (devolvido).
pub fn apply_to_client(
    book: &mut crate::recipe_book::ClientBook,
    packet: &ToClient,
    settings: &mut Settings,
    book_types: &[&str],
) -> Vec<Entry> {
    let mut notified = Vec::new();
    match packet {
        ToClient::Add { entries, replace } => {
            if *replace {
                book.clear();
            }
            for added in entries {
                book.add(added.entry.clone());
                if added.highlight {
                    book.add_highlight(added.entry.id);
                }
                if added.notification {
                    notified.push(added.entry.clone());
                }
            }
        }
        ToClient::Remove(ids) => {
            for id in ids {
                book.remove(*id);
            }
        }
        ToClient::Settings(bytes) => {
            if let Some(new) = Settings::decode(book_types, bytes) {
                settings.replace_from(&new);
            }
        }
        // O fantasma é da tela aberta, não do livro.
        ToClient::PlaceGhost(_) => {}
    }
    notified
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_server_sends_the_whole_book_once_and_then_only_what_changed() {
        let entry = |id: i32| {
            Some(Entry {
                id,
                category: "c".to_owned(),
                group: None,
            })
        };
        let order = SendOrder {
            displays: Displays(&[("m:a", 1, 1), ("m:b", 2, 1), ("m:c", 3, 1)]),
            identity: &|_| 1,
            award: &|_| 1,
            ascending: false,
        };
        let order = &order;
        let types = ["A", "B"];
        let mut now = Unlocked::default();
        now.add(1, true);
        now.add(2, false);
        // A primeira vez (`sendInitialRecipeBook`): as configurações e o livro inteiro com `replace`.
        let first = server_sync(None, &now, &types, || vec![7], entry, order);
        assert_eq!(first.len(), 2);
        assert_eq!(first[0], ToClient::Settings(vec![7]));
        assert!(
            matches!(&first[1], ToClient::Add { entries, replace: true } if entries.len() == 2)
        );
        // Nada mudou: nada vai, e as configurações nem são lidas.
        let sent = now.clone();
        assert!(
            server_sync(
                Some((&sent, &types)),
                &now,
                &types,
                || panic!("as configurações não mudaram"),
                entry,
                order
            )
            .is_empty()
        );
        // Uma receita entra e outra sai: só elas, sem as configurações.
        now.add(3, true);
        now.remove(1);
        let changed = server_sync(
            Some((&sent, &types)),
            &now,
            &types,
            || panic!("as configurações não mudaram"),
            entry,
            order,
        );
        assert!(changed.iter().any(|packet| matches!(packet, ToClient::Add { entries, replace: false } if entries.len() == 1 && entries[0].entry.id == 3)));
        assert!(
            changed
                .iter()
                .any(|packet| *packet == ToClient::Remove(vec![1]))
        );
        assert!(
            !changed
                .iter()
                .any(|packet| matches!(packet, ToClient::Settings(_)))
        );
        // O cliente que aplica o que recebeu fica com o livro do servidor.
        let mut client = Unlocked::default();
        for packet in first.iter().chain(&changed) {
            client.apply(packet);
        }
        assert_eq!(client, now);
        // Um tipo de livro novo registrado: as configurações vão de novo, mesmo com o livro igual.
        let sent = now.clone();
        let more = ["A", "B", "C"];
        assert_eq!(
            server_sync(Some((&sent, &types)), &now, &more, || vec![9], entry, order),
            vec![ToClient::Settings(vec![9])]
        );
    }

    #[test]
    fn a_big_packet_goes_in_pieces_that_fit_and_leave_the_client_the_same() {
        let entries: Vec<AddedEntry> = (0..5000)
            .map(|id| AddedEntry {
                entry: Entry {
                    id,
                    category: if id % 9 == 0 {
                        "mekanism:metallurgic_infusing_building".to_owned()
                    } else {
                        String::new()
                    },
                    group: None,
                },
                notification: false,
                highlight: id % 7 == 0,
            })
            .collect();
        let whole = ToClient::Add {
            entries: entries.clone(),
            replace: true,
        };
        assert!(whole.encode().len() > 30_000);
        let pieces = whole.clone().split(24_000);
        assert!(pieces.len() > 1);
        let mut joined = Vec::new();
        for (index, piece) in pieces.iter().enumerate() {
            assert!(piece.encode().len() <= 24_000);
            assert_eq!(ToClient::decode(&piece.encode()).as_ref(), Some(piece));
            let ToClient::Add { entries, replace } = piece else {
                panic!("pedaço de outro tipo");
            };
            assert_eq!(*replace, index == 0);
            joined.extend(entries.iter().cloned());
        }
        assert_eq!(joined, entries);
        let (mut one, mut many) = (ClientKnown::default(), ClientKnown::default());
        one.apply(&whole);
        for piece in &pieces {
            many.apply(piece);
        }
        assert_eq!(
            one.table().iter().map(|(id, ())| id).collect::<Vec<_>>(),
            many.table().iter().map(|(id, ())| id).collect::<Vec<_>>()
        );
        assert_eq!(one.table().capacity(), 8192);
        // Um pacote pequeno e o vazio com `replace` seguem inteiros.
        let empty = ToClient::Add {
            entries: Vec::new(),
            replace: true,
        };
        assert_eq!(empty.clone().split(24_000), vec![empty]);
        let removed = ToClient::Remove((0..10_000).collect()).split(24_000);
        assert!(removed.len() > 1 && removed.iter().all(|piece| piece.encode().len() <= 24_000));
    }

    #[test]
    fn the_saved_lists_give_back_the_recipes_of_the_registered_books_once() {
        let names: std::collections::HashSet<&str> = ["m:a", "m:b", "m:c"].into_iter().collect();
        let mut recipes = vec![
            "minecraft:x".to_owned(),
            "m:a".to_owned(),
            "m:b".to_owned(),
            "other:y".to_owned(),
        ];
        let mut display = vec!["m:b".to_owned(), "m:c".to_owned(), "minecraft:x".to_owned()];
        let mut seen = (usize::MAX, usize::MAX);
        let (known, highlight) =
            restore_saved(&mut recipes, &mut display, &names, &mut seen).unwrap();
        assert_eq!(known, ["m:a", "m:b"]);
        // Só a destacada que também é conhecida volta destacada (`m:c` não está em `recipes`).
        assert_eq!(highlight, ["m:b"]);
        // As listas lidas ficam sem as que voltaram: quem grava o jogador as reescreve do livro.
        assert_eq!(recipes, ["minecraft:x", "other:y"]);
        assert_eq!(display, ["minecraft:x"]);
        // Conferido: a mesma lista com os mesmos livros não é lida de novo.
        assert_eq!(seen, (3, 2));
        assert_eq!(
            restore_saved(&mut recipes, &mut display, &names, &mut seen),
            None
        );
        // Um livro novo registrado (mais nomes) faz conferir de novo, e o que é dele volta.
        let more: std::collections::HashSet<&str> =
            ["m:a", "m:b", "m:c", "other:y"].into_iter().collect();
        let (known, highlight) =
            restore_saved(&mut recipes, &mut display, &more, &mut seen).unwrap();
        assert_eq!(known, ["other:y"]);
        assert!(highlight.is_empty());
        assert_eq!(recipes, ["minecraft:x"]);
    }
    use crate::recipe_book::ClientBook;

    fn entry(id: i32) -> Entry {
        Entry {
            id,
            category: "mekanism:enriching_building".to_owned(),
            group: (id % 2 == 0).then_some(id / 2),
        }
    }

    #[test]
    fn the_nbt_fields_are_the_neoforge_ones() {
        assert_eq!(upper_camel("MEKANISM_ENRICHING"), "MekanismEnriching");
        assert_eq!(
            settings_fields("MEKANISM_METALLURGIC_INFUSING"),
            (
                "isMekanismMetallurgicInfusingGuiOpen".to_owned(),
                "isMekanismMetallurgicInfusingFilteringCraftable".to_owned()
            )
        );
    }

    #[test]
    fn settings_default_to_false_and_round_trip() {
        let types = ["MEKANISM_CRUSHING", "MEKANISM_ENRICHING"];
        let mut settings = Settings::default();
        assert!(!settings.is_open("MEKANISM_CRUSHING"));
        settings.set_open("MEKANISM_ENRICHING", true);
        settings.set_filtering("MEKANISM_ENRICHING", true);
        assert_eq!(settings.encode(&types), vec![0, 0, 1, 1]);
        assert_eq!(
            Settings::decode(&types, &[0, 0, 1, 1])
                .unwrap()
                .get("MEKANISM_ENRICHING"),
            settings.get("MEKANISM_ENRICHING")
        );
        assert!(Settings::decode(&types, &[0, 0, 1]).is_none());
        let fields = settings.write(&types);
        let mut read = Settings::default();
        read.read(&types, |name| {
            fields
                .iter()
                .find(|(field, _)| field == name)
                .map(|(_, value)| *value)
        });
        assert_eq!(
            read.get("MEKANISM_ENRICHING"),
            settings.get("MEKANISM_ENRICHING")
        );
        assert!(!read.is_open("MEKANISM_CRUSHING"));
    }

    #[test]
    fn packets_round_trip() {
        let add = ToClient::Add {
            entries: vec![
                AddedEntry {
                    entry: entry(481),
                    notification: true,
                    highlight: true,
                },
                AddedEntry {
                    entry: entry(482),
                    notification: false,
                    highlight: true,
                },
            ],
            replace: true,
        };
        assert_eq!(ToClient::decode(&add.encode()), Some(add));
        let remove = ToClient::Remove(vec![3, 4_116]);
        assert_eq!(ToClient::decode(&remove.encode()), Some(remove));
        let settings = ToClient::Settings(vec![1, 0, 0, 1]);
        assert_eq!(ToClient::decode(&settings.encode()), Some(settings));
        let change = ToServer::ChangeSettings(ChangeSettings {
            book_type: "MEKANISM_CRUSHING".to_owned(),
            open: true,
            filtering: false,
        });
        assert_eq!(ToServer::decode(&change.encode()), Some(change));
        assert_eq!(
            ToServer::decode(&ToServer::SeenRecipe(7).encode()),
            Some(ToServer::SeenRecipe(7))
        );
        assert_eq!(ToClient::decode(&[9]), None);
        assert_eq!(ToServer::decode(&[0, 0]), None);
    }

    #[test]
    fn award_sends_only_the_new_recipes_and_highlights_them() {
        let mut book = Unlocked::default();
        let first = book.award(&[entry(1), entry(2)]).unwrap();
        let ToClient::Add { entries, replace } = &first else {
            panic!()
        };
        assert!(!replace && entries.iter().all(|a| a.notification && a.highlight));
        // As duas já são conhecidas: nada a mandar.
        assert!(book.award(&[entry(1), entry(2)]).is_none());
        let again = book.award(&[entry(2), entry(5)]).unwrap();
        let ToClient::Add { entries, .. } = again else {
            panic!()
        };
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].entry.id, 5);
        book.seen(1);
        assert!(!book.is_highlighted(1) && book.is_highlighted(2));
        assert_eq!(book.revoke(&[2, 99]), Some(ToClient::Remove(vec![2])));
        assert!(book.revoke(&[2]).is_none());
        // O envio inicial leva só o que ele conhece, sem aviso, com o destaque do que ainda não viu.
        let ToClient::Add { entries, replace } = book.initial(&[entry(1), entry(2), entry(5)])
        else {
            panic!()
        };
        assert!(replace);
        assert_eq!(
            entries
                .iter()
                .map(|a| (a.entry.id, a.highlight, a.notification))
                .collect::<Vec<_>>(),
            vec![(1, false, false), (5, true, false)]
        );
    }

    #[test]
    fn the_client_applies_the_packets() {
        let types = ["MEKANISM_CRUSHING"];
        let mut book = ClientBook::default();
        let mut settings = Settings::default();
        let add = ToClient::Add {
            entries: vec![
                AddedEntry {
                    entry: entry(1),
                    notification: true,
                    highlight: true,
                },
                AddedEntry {
                    entry: entry(2),
                    notification: false,
                    highlight: false,
                },
            ],
            replace: false,
        };
        let toasts = apply_to_client(&mut book, &add, &mut settings, &types);
        assert_eq!(toasts.iter().map(|e| e.id).collect::<Vec<_>>(), vec![1]);
        assert!(book.will_highlight(1) && !book.will_highlight(2));
        apply_to_client(&mut book, &ToClient::Remove(vec![1]), &mut settings, &types);
        assert!(!book.will_highlight(1));
        apply_to_client(
            &mut book,
            &ToClient::Settings(vec![1, 1]),
            &mut settings,
            &types,
        );
        assert!(
            settings.is_open("MEKANISM_CRUSHING") && settings.is_filtering("MEKANISM_CRUSHING")
        );
        // `replace` zera o que havia.
        let replace = ToClient::Add {
            entries: vec![],
            replace: true,
        };
        apply_to_client(&mut book, &replace, &mut settings, &types);
        book.rebuild_collections(&[]);
        assert_eq!(book.collection_count(), 0);
    }

    #[test]
    fn the_server_book_reaches_the_client_by_the_initial_packet_and_then_by_what_changed() {
        let table = [(10, "m:a"), (11, "m:a"), (20, "m:b"), (30, "m:c")];
        let order = SendOrder {
            displays: Displays(&[("m:a", 10, 2), ("m:b", 20, 1), ("m:c", 30, 1)]),
            identity: &|_| 1,
            award: &|_| 1,
            ascending: false,
        };
        let order = &order;
        let entry = |id: i32| {
            Some(Entry {
                id,
                category: "c".to_owned(),
                group: None,
            })
        };
        let server = Unlocked::from_recipes(
            table.iter().copied(),
            |name| name != "m:c",
            |name| name == "m:b",
        );
        assert_eq!(server.known().collect::<Vec<_>>(), vec![10, 11, 20]);
        assert_eq!(server.highlighted().collect::<Vec<_>>(), vec![20]);
        // `sendInitialRecipeBook`: tudo, `replace`, sem aviso, destacado só o que o servidor guarda destacado.
        let mut client = Unlocked::default();
        client.add(99, true);
        let initial = server.sync(None, entry, order);
        assert_eq!(initial.len(), 1);
        match &initial[0] {
            ToClient::Add { entries, replace } => {
                assert!(*replace);
                assert!(entries.iter().all(|added| !added.notification));
                assert_eq!(
                    entries
                        .iter()
                        .filter(|added| added.highlight)
                        .map(|added| added.entry.id)
                        .collect::<Vec<_>>(),
                    vec![20]
                );
            }
            other => panic!("{other:?}"),
        }
        for packet in &initial {
            assert!(client.apply(&ToClient::decode(&packet.encode()).unwrap()));
        }
        assert_eq!(client, server);
        // `removeRecipes` e `addRecipes`: só o que mudou, o que entra com aviso.
        let next = Unlocked::from_recipes(
            table.iter().copied(),
            |name| name != "m:a",
            |name| name == "m:c",
        );
        let delta = next.sync(Some(&server), entry, order);
        assert_eq!(delta[0], ToClient::Remove(vec![10, 11]));
        match &delta[1] {
            ToClient::Add { entries, replace } => {
                assert!(!*replace);
                assert_eq!(entries.len(), 1);
                assert!(
                    entries[0].notification && entries[0].highlight && entries[0].entry.id == 30
                );
            }
            other => panic!("{other:?}"),
        }
        for packet in &delta {
            client.apply(packet);
        }
        assert_eq!(client.known().collect::<Vec<_>>(), vec![20, 30]);
        assert_eq!(client.highlighted().collect::<Vec<_>>(), vec![20, 30]);
        assert!(next.sync(Some(&next), entry, order).is_empty());
    }

    #[test]
    fn the_settings_packet_carries_the_type_names() {
        let mut settings = Settings::default();
        settings.set_open("MOD_B", true);
        settings.set_filtering("MOD_A", true);
        let bytes = settings.encode_named(&["MOD_B", "MOD_A", "MOD_C"]);
        let read = Settings::decode_named(&bytes).unwrap();
        assert_eq!(
            read,
            vec![
                (
                    "MOD_B".to_owned(),
                    TypeSettings {
                        open: true,
                        filtering: false
                    }
                ),
                (
                    "MOD_A".to_owned(),
                    TypeSettings {
                        open: false,
                        filtering: true
                    }
                ),
                ("MOD_C".to_owned(), TypeSettings::default()),
            ]
        );
        // Quem recebe junta ao que tem, sem depender da própria lista de tipos.
        let mut client = Settings::default();
        client.set_open("MOD_Z", true);
        for (name, value) in read {
            client.set(&name, value);
        }
        assert!(client.is_open("MOD_B") && client.is_filtering("MOD_A") && client.is_open("MOD_Z"));
        assert_eq!(Settings::decode_named(&bytes[..bytes.len() - 1]), None);
        assert_eq!(
            Settings::decode_named(&[bytes.as_slice(), &[0]].concat()),
            None
        );
    }
}
