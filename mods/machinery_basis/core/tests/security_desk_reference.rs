//! A Security Desk contra o original: `dev/referencia/rastros/security_desk.jsonl` (`SecurityDeskTraceTest.java`). O dono põe a mesa e uma máquina; cada passo mexe
//! na frequência dele (um nome entra e sai da lista, o modo troca, a sobreposição liga) e grava a frequência (o NBT, a tabela inteira, os bytes do pacote do menu, o
//! `hashCode` e o `getSyncHash`), o `PacketSyncSecurity`, o `PacketBatchSecurityUpdate`, se a tabela e o dado do menu ficaram sujos, e quem acessa a máquina em cada
//! modo, no servidor e no cliente. Este teste refaz os passos na lib e compara tudo.
//!
//! O resolvedor de nomes do rastro é o do `GameTestServer` (`MockUserNameToIdResolver`, que diferencia maiúsculas): os pares nome e UUID entram aqui como o rastro os
//! deu. O do servidor integrado (`CachedUserNameToIdResolver`) é `security_desk::resolve_name`, conferido à parte.
use machinery_basis_core::containers::AutomationType;
use machinery_basis_core::frequency::{FrequencyLookup, lookup_file, lookup_id};
use machinery_basis_core::security::{SecurityConfig, SecurityData, SecurityMode, SecurityState, owner_only_access};
use machinery_basis_core::security_desk::{self as desk, DeskAction, DeskEffect, DeskSlot, ItemSecurity};
use machinery_basis_core::security_frequency::{self as freq, ClientSecurity, SecurityFrequency, SecurityLookup, SyncSecurity};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

const OWNER_NAME: &str = "test-mock-player";

fn rows() -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../dev/referencia/rastros/security_desk.jsonl");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    text.lines().map(|line| serde_json::from_str(line).expect("linha JSON válida")).collect()
}

fn row<'a>(rows: &'a [Value], step: &str) -> &'a Value {
    rows.iter().find(|row| row["step"] == step).unwrap_or_else(|| panic!("passo {step}"))
}

fn uuid(text: &Value) -> u128 {
    u128::from_str_radix(&text.as_str().expect("uuid").replace('-', ""), 16).expect("uuid em hexadecimal")
}

fn uuid_and_name(text: &Value) -> (u128, String) {
    let (id, name) = text.as_str().expect("perfil").split_once(' ').expect("uuid e nome");
    (u128::from_str_radix(&id.replace('-', ""), 16).expect("uuid"), name.to_owned())
}

fn mode(text: &Value) -> SecurityMode {
    SecurityMode::from_name(&text.as_str().expect("modo").to_ascii_lowercase()).expect("modo conhecido")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

struct Players {
    owner: u128,
    guest: u128,
    stranger: u128,
}

/// O que cada passo do rastro faz, na ordem do `SecurityDeskTraceTest.run`.
enum Act {
    Place,
    Add(&'static str),
    Mode(SecurityMode),
    Toggle,
    Remove(i32),
    Button(i32),
}

fn script() -> Vec<(&'static str, Vec<Act>, Vec<DeskEffect>)> {
    use DeskEffect::{MarkForSave, SyncSecurity, ValidateAccess};
    use SecurityMode::{Private, Public, Trusted};
    vec![
        ("placed", vec![Act::Place], vec![]),
        ("add_guest", vec![Act::Add("Convidado")], vec![]),
        ("add_guest_lowercase", vec![Act::Add("convidado")], vec![]),
        ("add_unknown", vec![Act::Add("Zz_Nunca_Visto1")], vec![]),
        ("add_owner", vec![Act::Add("Dono")], vec![]),
        ("mode_trusted", vec![Act::Mode(Trusted)], vec![MarkForSave, SyncSecurity, ValidateAccess]),
        ("mode_trusted_again", vec![Act::Mode(Trusted)], vec![]),
        ("mode_private", vec![Act::Mode(Private)], vec![MarkForSave, SyncSecurity, ValidateAccess]),
        ("override_on_private", vec![Act::Toggle], vec![MarkForSave, SyncSecurity, ValidateAccess]),
        ("override_on_trusted", vec![Act::Mode(Trusted)], vec![MarkForSave, SyncSecurity]),
        ("override_on_public", vec![Act::Mode(Public)], vec![MarkForSave, SyncSecurity]),
        ("remove_out_of_range", vec![Act::Mode(Trusted), Act::Remove(7)], vec![MarkForSave]),
        ("remove_negative", vec![Act::Remove(-1)], vec![MarkForSave]),
        ("remove_unknown", vec![Act::Remove(1)], vec![MarkForSave, DeskEffect::CloseIfNoAccess(0)]),
        ("remove_guest", vec![Act::Remove(0)], vec![MarkForSave, DeskEffect::CloseIfNoAccess(0)]),
        ("add_guest_back", vec![Act::Add("Convidado")], vec![]),
        ("override_off", vec![Act::Toggle], vec![MarkForSave, SyncSecurity, ValidateAccess]),
        ("stranger_packet_mode_private", vec![Act::Button(DeskAction::Mode(Private).button())], vec![MarkForSave, SyncSecurity, ValidateAccess]),
        ("stranger_packet_override", vec![Act::Button(DeskAction::ToggleOverride.button())], vec![MarkForSave, SyncSecurity, ValidateAccess]),
        ("stranger_packet_remove", vec![Act::Button(DeskAction::Remove(0).button())], vec![MarkForSave, DeskEffect::CloseIfNoAccess(0)]),
        (
            "owner_packets_back",
            vec![Act::Button(DeskAction::ToggleOverride.button()), Act::Button(DeskAction::Mode(Trusted).button())],
            vec![MarkForSave, SyncSecurity],
        ),
        ("saved", vec![], vec![]),
    ]
}

/// Refaz o rastro passo a passo e entrega a tabela depois de cada um.
fn replay(mut check: impl FnMut(&str, &Value, &mut SecurityLookup, &Players, &[DeskEffect], &[DeskEffect])) {
    let rows = rows();
    let placed = row(&rows, "placed");
    let players = Players { owner: uuid(&placed["owner"]), guest: uuid(&placed["guest"]), stranger: uuid(&placed["stranger"]) };
    let mut resolver: HashMap<&str, (u128, String)> = HashMap::new();
    resolver.insert("Convidado", (players.guest, "Convidado".to_owned()));
    resolver.insert("convidado", uuid_and_name(&row(&rows, "names")["cache_lower"]));
    resolver.insert("Zz_Nunca_Visto1", uuid_and_name(&row(&rows, "add_unknown")["resolved"]));
    resolver.insert("Dono", (players.owner, "Dono".to_owned()));
    let names = |_: u128| OWNER_NAME.to_owned();
    let mut lookup: SecurityLookup = FrequencyLookup::new(None, SecurityMode::Public);
    for (step, acts, expected) in script() {
        let mut effects = Vec::new();
        for act in acts {
            if let Act::Place = act {
                freq::frequency_of_owner(&mut lookup, players.owner, &names);
                continue;
            }
            let frequency = lookup.get_mut(&Some(players.owner)).expect("a frequência do dono");
            effects = match act {
                Act::Place => unreachable!(),
                Act::Add(name) => {
                    let resolved = resolver.get(name).map(|(id, name)| (*id, name.as_str()));
                    desk::add_trusted(frequency, resolved);
                    Vec::new()
                }
                Act::Mode(mode) => desk::set_mode(frequency, mode),
                Act::Toggle => desk::toggle_override(frequency),
                Act::Remove(index) => desk::remove_trusted(frequency, index),
                Act::Button(button) => DeskAction::of_button(button).expect("botão da mesa").apply(frequency),
            };
        }
        check(step, row(&rows, step), &mut lookup, &players, &effects, &expected);
    }
}

#[test]
fn the_frequency_follows_every_step_of_the_desk() {
    let mut last_menu_hash = 0;
    replay(|step, row, lookup, players, _, _| {
        let frequency = lookup.get(&Some(players.owner)).expect("a frequência do dono").clone();
        let traced = &row["frequency"];
        assert_eq!(machinery_basis_core::frequency::Frequency::encode(&frequency), traced["nbt"], "{step}: o NBT do CODEC");
        assert_eq!(lookup.encode(), row["lookup"], "{step}: a tabela");
        assert_eq!(hex(&frequency.stream_encode()), traced["stream"].as_str().unwrap(), "{step}: os bytes do STREAM_CODEC");
        assert_eq!(i64::from(frequency.hash_code()), traced["hash"].as_i64().unwrap(), "{step}: hashCode");
        assert_eq!(i64::from(frequency.sync_hash()), traced["sync_hash"].as_i64().unwrap(), "{step}: getSyncHash");
        assert_eq!(frequency.security(), mode(&traced["mode"]), "{step}: o modo");
        assert_eq!(frequency.is_overridden(), traced["override"].as_bool().unwrap(), "{step}: a sobreposição");
        assert_eq!(frequency.owner_name(), traced["owner_name"].as_str().unwrap(), "{step}: o nome do dono");
        assert_eq!(frequency.is_valid(), traced["valid"].as_bool().unwrap(), "{step}: válida");
        let names: Vec<&str> = traced["names"].as_array().unwrap().iter().map(|name| name.as_str().unwrap()).collect();
        assert_eq!(frequency.trusted_names(), names, "{step}: a lista de nomes");
        for (who, id) in [("owner", players.owner), ("guest", players.guest), ("stranger", players.stranger)] {
            assert_eq!(frequency.is_trusted(id), traced["trusted"][who].as_bool().unwrap(), "{step}: isTrusted({who})");
        }
        // A frequência que o cliente lê do pacote: os nomes, o modo e a sobreposição, sem os UUIDs e com o hash da lista em zero.
        let client = SecurityFrequency::stream_decode(&frequency.stream_encode()).expect("o pacote decodifica");
        let client_names: Vec<&str> = traced["client_names"].as_array().unwrap().iter().map(|name| name.as_str().unwrap()).collect();
        assert_eq!(client.trusted_names(), client_names, "{step}: os nomes no cliente");
        assert_eq!(client.security(), mode(&traced["client_mode"]), "{step}: o modo no cliente");
        assert_eq!(client.is_overridden(), traced["client_override"].as_bool().unwrap(), "{step}: a sobreposição no cliente");
        assert_eq!(client.owner_name(), traced["client_owner_name"].as_str().unwrap(), "{step}: o nome do dono no cliente");
        assert_eq!(i64::from(client.hash_code()), traced["client_hash"].as_i64().unwrap(), "{step}: hashCode no cliente");
        assert_eq!(i64::from(client.sync_hash()), traced["client_sync_hash"].as_i64().unwrap(), "{step}: getSyncHash no cliente");
        assert!(!client.is_trusted(players.guest), "{step}: o cliente não recebe os UUIDs de confiança");
        // A tabela só fica suja ao criar a frequência; o dado do menu só fica sujo quando o `hashCode` muda (uma vez, ao abrir).
        assert_eq!(lookup.is_dirty(), row["lookup_dirty"].as_bool().unwrap(), "{step}: a tabela suja");
        lookup.set_dirty(false);
        let (menu_hash, menu_bytes) = SecurityFrequency::menu_sync(Some(&frequency));
        let dirty = if menu_hash == last_menu_hash { "CLEAN" } else { "DIRTY" };
        last_menu_hash = menu_hash;
        assert_eq!(dirty, row["menu_dirty"].as_str().unwrap(), "{step}: o dado do menu sujo");
        assert_eq!(hex(&menu_bytes), row["menu_bytes"].as_str().unwrap(), "{step}: os bytes do dado do menu");
        assert_eq!(SecurityFrequency::menu_read(&menu_bytes).as_ref(), Some(&client), "{step}: o dado do menu lido");
    });
    assert_eq!(SecurityFrequency::menu_sync(None), (0, vec![0]));
    assert_eq!(SecurityFrequency::menu_read(&[0]), None);
}

#[test]
fn the_packets_carry_the_owner_the_name_and_the_security_data() {
    replay(|step, row, lookup, players, _, _| {
        let frequency = lookup.get(&Some(players.owner)).expect("a frequência do dono");
        let packet = SyncSecurity::of_frequency(frequency).expect("a frequência tem dono");
        assert_eq!(hex(&packet.encode()), row["sync_packet"].as_str().unwrap(), "{step}: PacketSyncSecurity(frequency)");
        assert_eq!(SyncSecurity::decode(&packet.encode()), Some(packet.clone()), "{step}: ida e volta");
        let placed = SyncSecurity::of_player(players.owner, &|_| OWNER_NAME.to_owned());
        assert_eq!(hex(&placed.encode()), row["sync_packet_placed"].as_str().unwrap(), "{step}: PacketSyncSecurity(uuid)");
        assert_eq!(SyncSecurity::decode(&placed.encode()), Some(placed), "{step}: ida e volta sem dado");
        let batch = freq::batch(lookup);
        let traced = row["batch"].as_object().unwrap();
        assert_eq!(batch.len(), traced.len(), "{step}: o lote tem uma entrada por dono");
        for entry in &batch {
            let expected = traced.iter().find(|(id, _)| uuid(&Value::from(id.as_str())) == entry.player).expect("o dono no lote").1;
            assert_eq!(entry.name, expected["name"].as_str().unwrap(), "{step}: o nome no lote");
            let data = entry.data.expect("o lote leva o dado");
            assert_eq!(data.mode, mode(&expected["mode"]), "{step}: o modo no lote");
            assert_eq!(data.overridden, expected["override"].as_bool().unwrap(), "{step}: a sobreposição no lote");
        }
    });
}

#[test]
fn who_reaches_the_machine_follows_the_list_and_the_override() {
    let config = SecurityConfig::default();
    replay(|step, row, lookup, players, _, _| {
        let frequency = lookup.get(&Some(players.owner)).expect("a frequência do dono").clone();
        let mut synced = ClientSecurity::default();
        synced.sync(SyncSecurity::of_frequency(&frequency).unwrap());
        let unsynced = ClientSecurity::default();
        for machine_mode in SecurityMode::VALUES {
            let state = SecurityState { owner: Some(players.owner), mode: machine_mode };
            let traced = &row["access"][machine_mode.serialized_name().to_ascii_uppercase()];
            assert!(traced.get("client_error").is_none(), "{step}: o rastro tem o lado do cliente");
            let sides: [(&str, bool, SecurityData); 3] = [
                ("server", false, freq::server_data(Some(lookup), state.owner)),
                ("client_synced", true, synced.data(state.owner)),
                ("client_unsynced", true, unsynced.data(state.owner)),
            ];
            for (side, client, data) in sides {
                let trusted = |owner: u128, player: u128| freq::server_trusted(Some(lookup), owner, player);
                let expected = &traced[side];
                for (who, player) in [("owner", Some(players.owner)), ("guest", Some(players.guest)), ("stranger", Some(players.stranger)), ("null", None)] {
                    assert_eq!(
                        state.can_access_with(config, player, client, data, &trusted),
                        expected[who].as_bool().unwrap(),
                        "{step}: máquina {machine_mode:?}, {side}, {who}"
                    );
                }
                let effective = state.final_data(config, data);
                assert_eq!(effective.mode, mode(&expected["mode"]), "{step}: máquina {machine_mode:?}, {side}: o modo efetivo");
                assert_eq!(effective.overridden, expected["override"].as_bool().unwrap(), "{step}: máquina {machine_mode:?}, {side}: sobreposto");
            }
        }
    });
}

#[test]
fn each_desk_action_marks_syncs_and_validates_as_the_tile_does() {
    replay(|step, _, _, _, effects, expected| {
        let shape = |effects: &[DeskEffect]| -> Vec<DeskEffect> {
            effects.iter().map(|effect| if let DeskEffect::CloseIfNoAccess(_) = effect { DeskEffect::CloseIfNoAccess(0) } else { *effect }).collect()
        };
        assert_eq!(shape(effects), shape(expected), "{step}: o que a ação provoca");
    });
}

#[test]
fn the_desk_only_opens_and_breaks_for_its_owner() {
    let rows = rows();
    let placed = row(&rows, "placed");
    let config = SecurityConfig::default();
    let owner = Some(uuid(&placed["owner"]));
    for who in ["owner", "guest", "stranger"] {
        let allowed = owner_only_access(config, owner, Some(uuid(&placed[who])));
        let access = &placed["desk_access"];
        assert_eq!(allowed, access[who].as_bool().unwrap(), "{who}: canAccess da mesa");
        assert_eq!(allowed, access[format!("{who}_open")].as_str().unwrap().starts_with("Success"), "{who}: abrir a mesa");
        assert_eq!(allowed, access[format!("{who}_break")].as_f64().unwrap() > 0.0, "{who}: quebrar a mesa");
    }
    assert!(owner_only_access(config, None, Some(1)), "sem dono, todos");
    assert!(owner_only_access(SecurityConfig { allow_protection: false, ..config }, owner, Some(1)), "com a proteção desligada, todos");
    assert_eq!(placed["lookup_id"], lookup_id("mekanism", freq::TYPE_NAME, None, SecurityMode::Public));
    assert_eq!(lookup_file(placed["lookup_id"].as_str().unwrap()), "data/mekanism/frequency/security/public.dat");
    assert_eq!(lookup_id("mekanism", "teleporter", owner, SecurityMode::Private), format!("mekanism:frequency/teleporter/{}/private", placed["owner"].as_str().unwrap()));
    // O tile da mesa guarda o dono e a identidade da frequência, como toda máquina com segurança.
    let nbt = &placed["desk_nbt"];
    assert_eq!(nbt["component_frequency"]["security"], SecurityFrequency::identity_value(owner.unwrap()));
    assert_eq!(SecurityFrequency::identity_owner(&nbt["component_frequency"]["security"]), Some((owner.unwrap(), SecurityMode::Public)));
    assert_eq!(row(&rows, "saved")["desk_nbt"]["component_frequency"], nbt["component_frequency"], "o tile não guarda a lista nem o modo da frequência");
}

#[test]
fn the_table_reads_back_what_it_wrote() {
    let mut last = None;
    replay(|_, row, lookup, players, _, _| last = Some((row["lookup"].clone(), lookup.clone(), players.owner, players.guest)));
    let (saved, lookup, owner, guest) = last.expect("o último passo");
    let names = |id: u128| if id == owner { "Dono".to_owned() } else if id == guest { "Convidado".to_owned() } else { format!("<{}>", machinery_basis_core::frequency::uuid_text(id)) };
    let read: SecurityLookup = FrequencyLookup::decode(&saved, SecurityMode::Public, &names).expect("a tabela decodifica");
    assert!(!read.is_dirty(), "a tabela lida do disco não está suja");
    assert_eq!(read.encode(), saved);
    let frequency = read.get(&Some(owner)).expect("a frequência do dono");
    assert_eq!(frequency.trusted(), lookup.get(&Some(owner)).unwrap().trusted());
    assert_eq!(frequency.trusted_names(), ["Dono", "Convidado"], "os nomes vêm do cache de nomes do servidor ao ler");
    assert_eq!(frequency.owner_name(), "Dono");
    assert!(frequency.is_valid());
    // Um tile lido do disco valida a frequência dele: se a tabela já a tem, nada muda; se não tem, entra uma nova e a tabela fica suja.
    let mut read = read;
    freq::validate_owner(&mut read, owner, &names);
    assert!(!read.is_dirty());
    assert_eq!(read.encode(), saved);
    freq::validate_owner(&mut read, guest, &names);
    assert!(read.is_dirty());
    let fresh = read.get(&Some(guest)).expect("a frequência nova");
    assert!(fresh.is_valid());
    assert_eq!((fresh.security(), fresh.is_overridden(), fresh.trusted().len()), (SecurityMode::Public, false, 0));
    assert_eq!(read.len(), 2);
}

#[test]
fn names_are_checked_as_the_text_field_and_the_packet_do() {
    let rows = rows();
    let names = row(&rows, "names");
    for (name, entry) in names.as_object().unwrap() {
        let Some(entry) = entry.as_object() else { continue };
        if !entry.contains_key("validator") {
            continue;
        }
        // `PacketAddTrusted.handle`: `!name.isEmpty() && InputValidator.test(name, USERNAME)`.
        assert_eq!(desk::server_accepts_name(name), !name.is_empty() && entry["validator"].as_bool().unwrap(), "{name:?}: o servidor aceita");
        // `GuiSecurityDesk.addTrusted(text.trim())`: `StringUtil.isValidPlayerName`.
        let trimmed = entry["trim"].as_str().unwrap();
        let trimmed_valid = names[trimmed]["valid_player_name"].as_bool().unwrap();
        assert_eq!(desk::client_sends_name(name), trimmed_valid.then_some(trimmed), "{name:?}: o cliente manda");
    }
    // O perfil offline de um nome: o UUID do Java (`UUID.nameUUIDFromBytes`), com o nome como veio e em minúsculas.
    let unknown = row(&rows, "add_unknown");
    assert_eq!(desk::offline_uuid("Zz_Nunca_Visto1"), uuid(&unknown["offline_exact"]));
    assert_eq!(desk::offline_uuid("zz_nunca_visto1"), uuid(&unknown["offline_lower"]));
    assert_eq!(desk::offline_uuid("convidado"), uuid_and_name(&names["cache_lower"]).0);
    // O servidor integrado: quem ele já viu sai pelo nome sem diferenciar maiúsculas; quem nunca viu vira o perfil offline do nome em minúsculas.
    let guest = uuid(&names["guest_uuid"]);
    let known = |lower: &str| (lower == "convidado").then(|| (guest, "Convidado".to_owned()));
    assert_eq!(desk::resolve_name("CONVIDADO", &known), (guest, "Convidado".to_owned()));
    assert_eq!(desk::resolve_name("Zz_Nunca_Visto1", &known), (uuid(&unknown["offline_lower"]), "zz_nunca_visto1".to_owned()));
}

fn item_of(stack: &Value) -> ItemSecurity {
    let ownable = stack["id"] == "mekanism:crusher";
    let components = &stack["components"];
    ItemSecurity {
        ownable,
        securable: ownable,
        owner: components.get("mekanism:owner").and_then(machinery_basis_core::security::owner_of),
        mode: components.get("mekanism:security").map(mode),
    }
}

#[test]
fn the_two_slots_lock_and_unlock_as_the_original() {
    let rows = rows();
    let slots = row(&rows, "slots");
    let placed = row(&rows, "placed");
    let owner = uuid(&placed["owner"]);
    let frequency_mode = mode(&slots["frequency_mode"]);
    assert_eq!(slots["count"], 2);
    let mut seen = 0;
    for (name, entry) in slots.as_object().unwrap() {
        let Some(stack) = entry.get("stack") else { continue };
        seen += 1;
        let item = item_of(stack);
        for (slot_name, slot) in [("unlock", DeskSlot::Unlock), ("lock", DeskSlot::Lock)] {
            for (automation_name, automation) in [("EXTERNAL", AutomationType::External), ("INTERNAL", AutomationType::Internal), ("MANUAL", AutomationType::Manual)] {
                let inserted = entry[format!("{slot_name}_insert_{automation_name}")].as_i64().unwrap() == 1;
                assert_eq!(desk::can_insert(slot, &item, automation, Some(owner)), inserted, "{name}: inserir em {slot_name} por {automation_name}");
                if let Some(extracted) = entry.get(format!("{slot_name}_extract_{automation_name}")) {
                    assert_eq!(desk::can_extract(slot, &item, automation), extracted.as_i64().unwrap() == 1, "{name}: tirar de {slot_name} por {automation_name}");
                }
            }
            let Some(after) = entry.get(format!("{slot_name}_after")) else { continue };
            let change = match slot {
                DeskSlot::Unlock => desk::unlock(&item, owner),
                DeskSlot::Lock => desk::lock(&item, owner, frequency_mode),
            };
            let components = &after["components"];
            let owner_after = components.get("mekanism:owner").and_then(machinery_basis_core::security::owner_of);
            let removed = components.get("!mekanism:security").is_some();
            let mode_after = components.get("mekanism:security").map(mode);
            match change {
                None => {
                    assert_eq!(after, stack, "{name}: {slot_name} não mexe no item");
                }
                Some(change) => {
                    assert_eq!(change.owner, owner_after, "{name}: o dono depois de {slot_name}");
                    match change.mode {
                        Some(None) => assert!(removed && mode_after.is_none(), "{name}: {slot_name} tira o componente de segurança"),
                        Some(Some(set)) => assert_eq!(Some(set), mode_after, "{name}: o modo depois de {slot_name}"),
                        None => assert_eq!((item.mode, false), (mode_after, removed), "{name}: {slot_name} não mexe no modo"),
                    }
                }
            }
        }
    }
    assert_eq!(seen, 5, "cinco itens no rastro");
}

/// `HashList` (lido do Java, `HashList.add` e `SecurityFrequency.removeTrusted`; o rastro não tem dois jogadores com o mesmo nome guardado): a lista de nomes não
/// repete um nome, então dois jogadores de confiança com o mesmo nome ocupam uma linha, e tirar a segunda posição tira o jogador e nenhum nome.
#[test]
fn two_trusted_players_with_one_stored_name_share_a_row() {
    let mut frequency = SecurityFrequency::new(Some(1), SecurityMode::Public, &|_| "Dono".to_owned());
    assert!(frequency.add_trusted(2, "Igual"));
    assert!(frequency.add_trusted(3, "Igual"));
    assert!(!frequency.add_trusted(3, "Outro"), "quem já está na lista não entra de novo");
    assert_eq!(frequency.trusted(), [2, 3]);
    assert_eq!(frequency.trusted_names(), ["Igual"]);
    assert_eq!(frequency.remove_trusted(1), Some(3));
    assert_eq!(frequency.trusted_names(), ["Igual"], "o índice 1 não existe na lista de nomes");
    assert_eq!(frequency.remove_trusted(0), Some(2));
    assert!(frequency.trusted_names().is_empty());
}
