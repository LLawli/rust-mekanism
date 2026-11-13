//! O leitor de rede (`ItemNetworkReader`) contra o original em jogo (`testes/NetworkReaderTraceTest.java`, `rastros/network_reader.jsonl`): o clique de verdade
//! (`ServerPlayerGameMode.useItemOn`) num transmissor de cada família, num Heat Generator e numa máquina vizinha, com os números crus de cada rede (o que o port alimenta ao texto) e as
//! mensagens de chat que o jogador recebeu, comparadas como JSON de componente.
use machinery_basis_core::side_config::TransmissionType;
use machinery_basis_core::sides::Face;
use machinery_basis_core::transmission::reading::{
    NetworkReading, ReadingDetail, TransmitterReading,
};
use mekanism_rules::network_reader;
use mekanism_rules::network_reader::{
    Comp, LocalConfig, connected_messages, heat_machine_messages, transmitter_messages,
};
use mekanism_rules::{chemical_lang_key, fluid_lang_key};
use serde_json::Value;
use std::path::Path;

fn rows() -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/network_reader.jsonl");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    text.lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|row: &Value| row.get("meta").is_none())
        .collect()
}

fn number(raw: &Value, key: &str) -> i64 {
    raw[key]
        .as_str()
        .unwrap_or_else(|| panic!("{key}"))
        .parse()
        .unwrap()
}

fn double(raw: &Value, key: &str) -> f64 {
    raw[key]
        .as_str()
        .unwrap_or_else(|| panic!("{key}"))
        .parse()
        .unwrap()
}

fn family_kind(family: &str) -> TransmissionType {
    match family {
        "energy" => TransmissionType::Energy,
        "fluid" => TransmissionType::Fluid,
        "chemical" => TransmissionType::Chemical,
        "heat" => TransmissionType::Heat,
        "inventory" => TransmissionType::Item,
        other => panic!("família {other}"),
    }
}

/// O componente do nome do recurso guardado (`FluidResource.getHoverName`, o nome do químico): o que a cola faz com o id.
fn resource_name(kind: TransmissionType, id: &str) -> Comp {
    let key = if kind == TransmissionType::Fluid {
        fluid_lang_key(id)
    } else {
        chemical_lang_key(id)
    };
    Comp::translate_key(&key)
}

/// A rede lida dos números crus que o original gravou.
fn reading_of(raw: &Value) -> (Option<TransmissionType>, TransmitterReading<Comp>) {
    if raw["transmitter"] != true {
        return (None, TransmitterReading::NoTransmitter);
    }
    if raw["has_network"] != true {
        return (None, TransmitterReading::NoNetwork);
    }
    let family = raw["family"].as_str().unwrap();
    let kind = family_kind(family);
    let detail = match family {
        "energy" => ReadingDetail::Energy {
            needed: number(raw, "needed"),
            stored: number(raw, "stored"),
            flow: number(raw, "prev_transfer"),
            capacity: number(raw, "capacity"),
        },
        "fluid" | "chemical" => {
            let id = raw["resource"].as_str().unwrap();
            ReadingDetail::Resource {
                needed: number(raw, "needed"),
                stored: (!id.is_empty()).then(|| (resource_name(kind, id), number(raw, "stored"))),
                flow: number(raw, "prev_transfer"),
                capacity: number(raw, "capacity"),
            }
        }
        "heat" => ReadingDetail::Heat {
            mean_temp: double(raw, "mean_temp"),
            transferred: double(raw, "heat_transferred"),
            lost: double(raw, "heat_lost"),
        },
        _ => ReadingDetail::Inventory,
    };
    (
        Some(kind),
        TransmitterReading::Network(NetworkReading {
            transmitters: raw["transmitters"].as_u64().unwrap() as usize,
            acceptors: raw["acceptors"].as_u64().unwrap() as usize,
            detail,
        }),
    )
}

fn kind_of_case(case: &str) -> TransmissionType {
    match case.split('/').next().unwrap().split('_').next().unwrap() {
        // Sem rede a família não conta: a mensagem é a mesma.
        "nonetwork" => TransmissionType::Energy,
        "cable" => TransmissionType::Energy,
        "pipe" => TransmissionType::Fluid,
        "tube" => TransmissionType::Chemical,
        "conductor" => TransmissionType::Heat,
        "transporter" => TransmissionType::Item,
        other => panic!("caso {other}"),
    }
}

fn json(messages: &[Comp]) -> Value {
    Value::Array(messages.iter().map(Comp::to_json).collect())
}

/// Compara linha a linha, para o erro dizer qual mensagem difere.
fn assert_chat(ours: &[Comp], original: &Value, what: &str) {
    let original = original.as_array().unwrap();
    for (index, line) in original.iter().enumerate() {
        let mine = ours.get(index).map(Comp::to_json);
        assert_eq!(mine.as_ref(), Some(line), "{what}, linha {index}");
    }
    assert_eq!(ours.len(), original.len(), "{what}, quantidade de linhas");
}

#[test]
fn the_click_on_a_transmitter_writes_what_the_original_wrote() {
    let mut checked = 0;
    let mut families = std::collections::BTreeSet::new();
    for row in rows() {
        let raw = &row["raw"];
        if raw["transmitter"] != true {
            continue;
        }
        let case = row["case"].as_str().unwrap();
        let (kind, reading) = reading_of(raw);
        let kind = kind.unwrap_or_else(|| kind_of_case(case));
        let heat = raw["heat_temperature"]
            .as_str()
            .map(|text| text.parse::<f64>().unwrap());
        let messages = transmitter_messages(kind, &reading, heat, LocalConfig::default());
        assert_chat(
            &messages,
            &row["chat"],
            &format!("{case} {} {}", row["face"], row["sneak"]),
        );
        families.insert(kind);
        checked += 1;
    }
    assert!(checked >= 240, "só {checked} cliques conferidos");
    assert_eq!(families.len(), 5, "as cinco famílias");
}

#[test]
fn the_click_on_a_machine_with_heat_writes_the_temperature() {
    let mut checked = 0;
    for row in rows() {
        let raw = &row["raw"];
        let case = row["case"].as_str().unwrap();
        if raw["transmitter"] == true || !case.ends_with("/generator") {
            continue;
        }
        let temp: f64 = raw["heat_temperature"].as_str().unwrap().parse().unwrap();
        // Sem agachar, o bloco abre a tela e o item não chega a ser usado (o gancho só roda depois que o bloco deixa o clique passar).
        let expected = if row["sneak"] == true {
            json(&heat_machine_messages(temp, LocalConfig::default()))
        } else {
            Value::Array(Vec::new())
        };
        assert_eq!(expected, row["chat"], "{case} {}", row["face"]);
        checked += usize::from(row["sneak"] == true);
    }
    assert!(checked >= 1, "só {checked} cliques conferidos");
}

#[test]
fn a_machine_touched_by_three_networks_lists_them_in_the_order_of_the_sides() {
    // `mixed/chamber`, agachado, na face de cima: o cabo ao norte (energia, lado norte), o transportador a oeste (itens, lado oeste) e o cabo a leste (energia, lado leste).
    let row = rows()
        .into_iter()
        .find(|row| row["case"] == "mixed/chamber" && row["sneak"] == true)
        .expect("a linha");
    let mut messages = Vec::new();
    messages.extend(connected_messages(TransmissionType::Energy, &[Face::North]));
    messages.extend(connected_messages(TransmissionType::Item, &[Face::West]));
    messages.extend(connected_messages(TransmissionType::Energy, &[Face::East]));
    assert_eq!(json(&messages), row["chat"]);
}

#[test]
fn several_sides_are_joined_with_a_comma_nested_to_the_left() {
    let messages = connected_messages(
        TransmissionType::Fluid,
        &[Face::Down, Face::North, Face::East],
    );
    let sides = messages[1].to_json();
    assert_eq!(sides["translate"], "network_reader.mekanism.connected");
    let list = &sides["with"][0];
    assert_eq!(list["translate"], "generic.mekanism.square_bracket");
    let outer = &list["with"][0];
    assert_eq!(outer["translate"], "generic.mekanism.with_comma");
    assert_eq!(outer["with"][1]["translate"], "direction.mekanism.east");
    let inner = &outer["with"][0];
    assert_eq!(inner["translate"], "generic.mekanism.with_comma");
    assert_eq!(inner["with"][0]["translate"], "direction.mekanism.down");
    assert_eq!(inner["with"][1]["translate"], "direction.mekanism.north");
}

#[test]
fn the_values_the_original_formats_in_odd_ranges_are_covered() {
    // Os valores postos por reflexão: energia cheia, vazão máxima (infinito), calor em notação científica e a eficiência arredondada.
    let rows = rows();
    let has = |needle: &str| {
        rows.iter()
            .any(|row| row["chat"].to_string().contains(needle))
    };
    assert!(has("gui.mekanism.infinite"), "o infinito");
    assert!(has("\"10M\"") || has("10M"), "a medida mega");
    assert!(has("generic.mekanism.percent"), "a eficiência");
    assert!(has("network.mekanism.fluid_needed"), "o balde");
    assert!(has("network.mekanism.no_network"), "sem rede");
}

// ---------------------------------------------------------------------------------------------------------------------------------------------------------------
// O modo de depuração (`MekanismAPI.debug`, o `/mek debug`): `rastros/network_reader_debug.jsonl`.

fn debug_rows() -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../dev/referencia/rastros/network_reader_debug.jsonl");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    text.lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|row: &Value| row.get("meta").is_none())
        .collect()
}

fn family_of_class(class: &str) -> TransmissionType {
    match class {
        "EnergyNetwork" => TransmissionType::Energy,
        "FluidNetwork" => TransmissionType::Fluid,
        "ChemicalNetwork" => TransmissionType::Chemical,
        "HeatNetwork" => TransmissionType::Heat,
        "InventoryNetwork" => TransmissionType::Item,
        other => panic!("{other}"),
    }
}

fn registry_of(note: &Value) -> Vec<(TransmissionType, usize, usize)> {
    note["registry"]
        .as_array()
        .unwrap()
        .iter()
        .map(|network| {
            (
                family_of_class(network["class"].as_str().unwrap()),
                network["transmitters"].as_u64().unwrap() as usize,
                network["acceptors"].as_u64().unwrap() as usize,
            )
        })
        .collect()
}

/// A mensagem de cada rede (`getTextComponent`) é a do original, para as cinco famílias e as contagens do rastro.
#[test]
fn the_description_of_every_network_is_the_one_of_the_original() {
    let mut count = 0;
    for row in debug_rows() {
        let Some(registry) = row["registry"].as_array() else { continue };
        for network in registry {
            let (kind, transmitters, acceptors) = (
                family_of_class(network["class"].as_str().unwrap()),
                network["transmitters"].as_u64().unwrap() as usize,
                network["acceptors"].as_u64().unwrap() as usize,
            );
            // O original põe a cor só no clique do leitor (`TextComponentUtil.build(DARK_GRAY, ..)`): aqui o texto da rede, sem estilo.
            assert!(network["text"].get("color").is_none());
            assert_eq!(network_reader::network_description(kind, transmitters, acceptors).to_json(), network["text"], "{}", row["case"]);
            count += 1;
        }
    }
    assert!(count > 100, "{count}");
}

/// O clique agachado num bloco sem block entity, com a depuração ligada: a borda, uma linha por rede na ordem do conjunto, a borda final. Cada clique usa o registro da nota anterior.
#[test]
fn the_sneaking_reader_writes_the_registry_with_the_debug_on_and_nothing_otherwise() {
    let mut registry: Vec<(TransmissionType, usize, usize)> = Vec::new();
    let mut debug = false;
    let mut clicks_with_chat = 0;
    for row in debug_rows() {
        if row.get("registry").is_some() {
            registry = registry_of(&row);
            debug = row["debug"].as_bool().unwrap();
            continue;
        }
        let case = row["case"].as_str().unwrap();
        let Some(sneak) = row["sneak"].as_bool() else { continue };
        let no_block_entity = !case.starts_with("cable") && !case.starts_with("chest");
        let chat = row["chat"].as_array().cloned().unwrap_or_default();
        // O que a cena deixa ligado ao clique: a depuração vira `true` na linha `stone/sneak/debug_on` e segue assim até `after_changes`.
        let on = case.contains("debug_on") || case.ends_with("after_changes");
        if no_block_entity && sneak && on {
            let wanted: Vec<Value> = network_reader::debug_messages(&registry).iter().map(Comp::to_json).collect();
            assert_eq!(chat, wanted, "{case}");
            assert!(chat.len() >= 3, "{case}");
            clicks_with_chat += 1;
        } else if no_block_entity {
            assert!(chat.is_empty(), "{case}: sem depuração, sem agachar ou com block entity não há linha");
        }
        let _ = debug;
    }
    assert!(clicks_with_chat >= 4, "{clicks_with_chat}");
}

/// O `/mek debug`: a mensagem de cada estado.
#[test]
fn the_toggle_message_is_the_one_of_the_original() {
    let mut seen = [false; 2];
    for row in debug_rows() {
        if row["command"] != "mek debug" || row["debug_usable"] != true {
            continue;
        }
        let on = row["debug_after"].as_bool().unwrap();
        let chat = row["chat"].as_array().unwrap();
        assert_eq!(chat.len(), 1);
        assert_eq!(chat[0], network_reader::debug_toggle_message(on).to_json(), "{}", row["case"]);
        seen[usize::from(on)] = true;
    }
    assert_eq!(seen, [true, true]);
}

/// O nível de cada nó e o erro do Brigadier de cada linha, contra o `CommandNode.canUse` e o chat do original em cada nível de permissão.
#[test]
fn the_mek_tree_answers_every_line_and_level_like_the_original() {
    use machinery_basis_core::commands::{Parsed, parse};
    let tree = mekanism_rules::command::tree(());
    let mut lines = 0;
    for row in debug_rows() {
        let Some(command) = row["command"].as_str() else { continue };
        let level = match row["level"].as_str().unwrap() {
            "ALL" => 0,
            "MODERATORS" => 1,
            "GAMEMASTERS" => 2,
            "ADMINS" => 3,
            "OWNERS" => 4,
            other => panic!("{other}"),
        };
        let case = row["case"].as_str().unwrap();
        // `CommandNode.canUse` de `mek` e de `mek debug`.
        assert_eq!(row["mek_usable"], level >= tree.level, "{case}");
        assert_eq!(row["debug_usable"], level >= tree.children[0].level, "{case}");
        let chat = row["chat"].as_array().unwrap();
        match parse(&tree, command, level).unwrap() {
            Parsed::Run(()) => {
                assert_eq!(chat.len(), 1, "{case}");
                assert!(chat[0]["translate"] == "command.mekanism.debug", "{case}");
            }
            Parsed::Error(error) => {
                assert_eq!(chat.len(), 2, "{case}");
                // `sendFailure(Component.literal(message))` e a linha de contexto: o trecho lido, o que sobra em vermelho sublinhado e o `command.context.here`.
                assert_eq!(chat[0]["extra"][0]["translate"], error.key, "{case}");
                let context = &chat[1]["extra"][0]["extra"];
                assert_eq!(context[0], error.input[..error.cursor], "{case}");
                if error.cursor < error.input.len() {
                    assert_eq!(context[1]["text"], error.input[error.cursor..], "{case}");
                    assert_eq!(context[1]["underlined"], true, "{case}");
                    assert_eq!(context[2]["translate"], "command.context.here", "{case}");
                } else {
                    assert_eq!(context[1]["translate"], "command.context.here", "{case}");
                }
                assert_eq!(chat[1]["extra"][0]["click_event"]["command"], format!("/{command}"), "{case}");
            }
        }
        lines += 1;
    }
    assert_eq!(lines, 8);
}
