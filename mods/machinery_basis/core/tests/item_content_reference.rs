//! O que o item de bloco leva quando a máquina é quebrada e o que o tile guarda quando o item volta, contra o original: `dev/referencia/rastros/item_content_<máquina>.jsonl`
//! (`testes/ItemContentTraceTest.java`, `runGameTestServer`) grava, para as quatro máquinas e o Heat Generator, os componentes do item que a tabela de loot solta
//! (`Block.getDrops` com o tile), o tooltip dele e o NBT do tile antes de quebrar e depois de pôr o item de volta, em quatro cenários (`empty`, `upgrades`, `full`, `round`).
use machinery_basis_core::item_content as content;
use serde_json::Value;
use std::path::PathBuf;

const MACHINES: [(&str, usize); 5] = [
    ("enrichment_chamber", 3),
    ("crusher", 3),
    ("energized_smelter", 3),
    ("metallurgic_infuser", 4),
    ("heat_generator", 2),
];

fn rows(machine: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../../dev/referencia/rastros/item_content_{machine}.jsonl"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    text.lines().map(|line| serde_json::from_str(line).expect("linha JSON válida")).collect()
}

fn components(row: &Value) -> &Value {
    &row["drops"][0]["stack"]["components"]
}

/// O calor que a Heat Generator vazia guardava no rastro: a temperatura ambiente do lugar do teste.
fn ambient_heat(machine: &str) -> Option<f64> {
    (machine == "heat_generator").then(|| rows(machine)[0]["tile"]["nbt"]["heat_capacitor"]["state"]["heat"].as_f64().unwrap())
}

/// Os componentes que o tile dá ao item, no formato do JSON do rastro (só os de conteúdo, sem o dono nem o nome nem a energia).
fn collected(machine: &str, size: usize, nbt: &Value) -> serde_json::Map<String, Value> {
    let mut out = serde_json::Map::new();
    if let Some(value) = content::upgrades_collect(nbt.get("component_upgrade")) {
        out.insert("mekanism:upgrades".into(), value);
    }
    if let Some(value) = content::containers_collect(&content::containers_from_saved(nbt.get("items"), size)) {
        out.insert("mekanism:items".into(), value);
    }
    if machine == "metallurgic_infuser"
        && let Some(value) = content::containers_collect(&content::containers_from_saved(nbt.get("chemical_tanks"), 1))
    {
        out.insert("mekanism:chemicals".into(), value);
    }
    if machine == "heat_generator" {
        if let Some(value) = content::containers_collect(&content::containers_from_saved(nbt.get("fluid_tanks"), 1)) {
            out.insert("mekanism:fluids".into(), value);
        }
        let state = &nbt["heat_capacitor"]["state"];
        let ambient = ambient_heat(machine).unwrap();
        let heat = state["heat"].as_f64().unwrap();
        if let Some(value) = content::heat_collect(heat, state["heat_capacity"].as_f64().unwrap(), heat == ambient) {
            out.insert("mekanism:heat".into(), value);
        }
    }
    if let Some(value) = content::redstone_collect(nbt["control_type"].as_str().unwrap_or(content::REDSTONE_DISABLED)) {
        out.insert("mekanism:redstone_control".into(), value);
    }
    out
}

const CONTENT_KEYS: [&str; 6] =
    ["mekanism:upgrades", "mekanism:items", "mekanism:chemicals", "mekanism:fluids", "mekanism:heat", "mekanism:redstone_control"];

fn only_content(components: &Value) -> serde_json::Map<String, Value> {
    components.as_object().unwrap().iter().filter(|(key, _)| CONTENT_KEYS.contains(&key.as_str())).map(|(key, value)| (key.clone(), value.clone())).collect()
}

#[test]
fn the_broken_machine_gives_the_item_the_components_of_the_original() {
    for (machine, size) in MACHINES {
        for row in rows(machine) {
            let scenario = row["scenario"].as_str().unwrap();
            assert_eq!(collected(machine, size, &row["tile"]["nbt"]), only_content(components(&row)), "{machine} {scenario}");
        }
    }
}

#[test]
fn the_item_placed_back_gives_the_tile_what_the_original_keeps() {
    for (machine, size) in MACHINES {
        for row in rows(machine) {
            let scenario = row["scenario"].as_str().unwrap();
            let item = components(&row);
            let nbt = &row["tile"]["nbt"];
            if scenario == "round" {
                // O tile de depois de pôr o item guarda o mesmo que o tile de antes de ele ser quebrado (`full`).
                let full = rows(machine).into_iter().find(|row| row["scenario"] == "full").unwrap();
                assert_eq!(collected(machine, size, nbt), collected(machine, size, &full["tile"]["nbt"]), "{machine} round contra full");
            }
            let upgrades = item.get("mekanism:upgrades").and_then(content::upgrades_apply);
            assert_eq!(upgrades.as_ref(), nbt.get("component_upgrade"), "{machine} {scenario} upgrades");
            let items = item.get("mekanism:items").and_then(|value| content::containers_apply(value, size)).and_then(|list| content::containers_to_saved(&list));
            if scenario == "round" || scenario == "full" || scenario == "empty" || scenario == "upgrades" {
                // O rastro grava os tiles por pôr o item em jogo (`empty`, `upgrades` e `full` partem do item liso e enchem o tile), então só o `round` vem do item.
                if scenario == "round" {
                    assert_eq!(items.as_ref(), nbt.get("items"), "{machine} round itens");
                }
            }
        }
    }
}

#[test]
fn a_list_of_another_size_is_not_applied() {
    let item = serde_json::json!({"containers": [{}, {}]});
    assert_eq!(content::containers_apply(&item, 3), None);
    assert!(content::containers_apply(&item, 2).is_some());
}

#[test]
fn the_tooltip_says_if_the_item_holds_a_stack() {
    for (machine, _) in MACHINES {
        for row in rows(machine) {
            let tooltip: Vec<&str> = row["drops"][0]["tooltip"].as_array().unwrap().iter().map(|line| line["text"].as_str().unwrap()).collect();
            let yes = content::has_inventory(components(&row).get("mekanism:items"));
            assert!(tooltip.contains(&if yes { "Inventory: yes" } else { "Inventory: no" }), "{machine} {}", row["scenario"]);
        }
    }
}
