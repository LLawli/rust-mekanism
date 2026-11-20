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
    pub fn sync(&self, sent: Option<&Unlocked>, entry: impl Fn(i32) -> Option<Entry>) -> Vec<ToClient> {
        let added = |id: &i32, notification: bool| {
            entry(*id).map(|entry| AddedEntry {
                entry,
                notification,
                highlight: self.highlight.contains(id),
            })
        };
        let Some(sent) = sent else {
            return vec![ToClient::Add {
                entries: self.known.iter().filter_map(|id| added(id, false)).collect(),
                replace: true,
            }];
        };
        let mut packets = Vec::new();
        let removed: Vec<i32> = sent.known.difference(&self.known).copied().collect();
        if !removed.is_empty() {
            packets.push(ToClient::Remove(removed));
        }
        let entries: Vec<AddedEntry> = self
            .known
            .difference(&sent.known)
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
            ToClient::Settings(_) => false,
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
    }
    notified
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let entry = |id: i32| {
            Some(Entry {
                id,
                category: "c".to_owned(),
                group: None,
            })
        };
        let server = Unlocked::from_recipes(table.iter().copied(), |name| name != "m:c", |name| name == "m:b");
        assert_eq!(server.known().collect::<Vec<_>>(), vec![10, 11, 20]);
        assert_eq!(server.highlighted().collect::<Vec<_>>(), vec![20]);
        // `sendInitialRecipeBook`: tudo, `replace`, sem aviso, destacado só o que o servidor guarda destacado.
        let mut client = Unlocked::default();
        client.add(99, true);
        let initial = server.sync(None, entry);
        assert_eq!(initial.len(), 1);
        match &initial[0] {
            ToClient::Add { entries, replace } => {
                assert!(*replace);
                assert!(entries.iter().all(|added| !added.notification));
                assert_eq!(entries.iter().filter(|added| added.highlight).map(|added| added.entry.id).collect::<Vec<_>>(), vec![20]);
            }
            other => panic!("{other:?}"),
        }
        for packet in &initial {
            assert!(client.apply(&ToClient::decode(&packet.encode()).unwrap()));
        }
        assert_eq!(client, server);
        // `removeRecipes` e `addRecipes`: só o que mudou, o que entra com aviso.
        let next = Unlocked::from_recipes(table.iter().copied(), |name| name != "m:a", |name| name == "m:c");
        let delta = next.sync(Some(&server), entry);
        assert_eq!(delta[0], ToClient::Remove(vec![10, 11]));
        match &delta[1] {
            ToClient::Add { entries, replace } => {
                assert!(!*replace);
                assert_eq!(entries.len(), 1);
                assert!(entries[0].notification && entries[0].highlight && entries[0].entry.id == 30);
            }
            other => panic!("{other:?}"),
        }
        for packet in &delta {
            client.apply(packet);
        }
        assert_eq!(client.known().collect::<Vec<_>>(), vec![20, 30]);
        assert_eq!(client.highlighted().collect::<Vec<_>>(), vec![20, 30]);
        assert!(next.sync(Some(&next), entry).is_empty());
    }
}
