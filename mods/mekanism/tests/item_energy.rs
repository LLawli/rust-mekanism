//! A energia guardada no item (`ContainerType.ENERGY`, o componente `mekanism:energy`) contra o original: a capacidade e a barra de cada item, a linha do tooltip, a máquina posta
//! e quebrada, as pilhas das abas criativas e a energia que a receita `mekanism:mek_data` soma. Os rastros são de `testes/ItemEnergyTraceTest.java` (`item_energy_*.jsonl`) e
//! `testes/MekDataTraceTest.java` (`mek_data_assemble.jsonl`).
mod common;

#[path = "common/crafting_stub.rs"]
mod items;

#[allow(dead_code)]
mod mekanism_mek_data {
    pub mod horizon_world {
        pub mod nbt {
            pub use crate::items::Tag;
        }
    }
    include!("../native/mek_data_energy.rs");
}

// Daqui só as pilhas cheias de energia: a regra das opções `prefilled` é do teste `creative_tabs`.
#[allow(dead_code)]
#[path = "../native/creative_tabs_table.rs"]
mod table;

use machinery_basis_core::item_energy::{
    copy_to_container, energy_display, is_bar_visible, item_bar_width,
};
use mekanism_rules as rules;
use serde_json::Value;
use std::path::Path;

fn lines(name: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|row: &Value| row.get("meta").is_none())
        .collect()
}

fn lang() -> Value {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("content/assets/mekanism/lang/en_us.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// A capacidade que o port dá ao item com a configuração padrão (`ServerConfig::DEFAULT`), ou `None` se ele não guarda energia.
fn capacity(item: &str) -> Option<i64> {
    let config = rules::ServerConfig::DEFAULT;
    if let Some(gear) = rules::gear_energy(item, &config.gear) {
        return Some(gear.capacity);
    }
    if let Some(tier) = rules::cube_tier(item) {
        return Some(config.cubes[tier]);
    }
    if let Some(tier) = rules::cell_tier(item) {
        return Some(config.cells[tier]);
    }
    let usage = match item {
        "mekanism:enrichment_chamber" => config.usage.enrichment_chamber,
        "mekanism:crusher" => config.usage.crusher,
        "mekanism:energized_smelter" => config.usage.energized_smelter,
        "mekanism:metallurgic_infuser" => config.usage.metallurgic_infuser,
        _ => return (item == "mekanismgenerators:heat_generator").then_some(240),
    };
    Some(i64::from(rules::energy_capacity(usage)))
}

#[test]
fn every_item_that_stores_energy_has_the_capacity_of_the_original() {
    let mut checked = 0;
    for row in lines("item_energy_items.jsonl") {
        if row["step"] != "head" {
            continue;
        }
        let item = row["item"].as_str().unwrap();
        match row["capacity"].as_i64() {
            Some(expected) => {
                assert_eq!(capacity(item), Some(expected), "{item}");
                checked += 1;
            }
            None => assert_eq!(capacity(item), None, "{item}"),
        }
    }
    // 12 de equipamento, 4 máquinas, o Heat Generator, 5 cubos e 1 célula (o rastro).
    assert_eq!(checked, 23);
    // O Configurator não guarda energia no 26.3.
    assert_eq!(capacity("mekanism:configurator"), None);
}

#[test]
fn the_gear_config_defaults_are_the_values_the_original_loads() {
    let config = lines("item_energy_items.jsonl")
        .into_iter()
        .find(|row| row["step"] == "gear_config")
        .unwrap();
    let gear = rules::Gear::DEFAULT;
    for (item, key) in [
        (gear.tablet, "tablet"),
        (gear.disassembler, "disassembler"),
        (gear.bow, "electricBow"),
        (gear.teleporter, "portableTeleporter"),
        (gear.free_runners, "freeRunner"),
    ] {
        let max = config[format!("{key}MaxEnergy")].as_i64().unwrap();
        let rate = config[format!("{key}ChargeRate")].as_i64().unwrap();
        assert_eq!((item.capacity, i64::from(item.rate)), (max, rate), "{key}");
    }
    assert_eq!(
        gear.meka_tool.capacity,
        config["mekaToolBaseEnergyCapacity"]
    );
    assert_eq!(
        i64::from(gear.meka_tool.rate),
        config["mekaToolBaseChargeRate"]
    );
    assert_eq!(
        gear.meka_suit.capacity,
        config["mekaSuitBaseEnergyCapacity"]
    );
    assert_eq!(
        i64::from(gear.meka_suit.rate),
        config["mekaSuitBaseChargeRate"]
    );
}

#[test]
fn the_bar_is_the_one_of_the_original_for_every_level() {
    let mut with_bar = 0;
    for row in lines("item_energy_items.jsonl") {
        if row.get("stack").is_none() {
            continue;
        }
        let stack = &row["stack"];
        let item = stack["item"].as_str().unwrap();
        let energy = stack["energy"].as_i64().unwrap();
        let count = i32::try_from(stack["count"].as_i64().unwrap()).unwrap();
        let visible = rules::shows_energy_bar(item)
            && is_bar_visible(count, energy)
            && capacity(item).is_some();
        assert_eq!(visible, stack["bar_visible"], "{item} {energy} x{count}");
        if visible {
            let width = item_bar_width(energy, capacity(item).unwrap());
            assert_eq!(i64::from(width), stack["bar_width"], "{item} {energy}");
            with_bar += 1;
        }
    }
    assert!(with_bar >= 60, "{with_bar}");
}

/// O texto de um componente do rastro: o `text`, os `extra` e a tradução com os argumentos já montados.
fn render(component: &Value, lang: &Value) -> String {
    match component {
        Value::String(text) => text.clone(),
        Value::Object(object) => {
            let mut out = String::new();
            if let Some(text) = object.get("text").and_then(Value::as_str) {
                out.push_str(text);
            }
            if let Some(key) = object.get("translate").and_then(Value::as_str) {
                let template = lang[key].as_str().unwrap_or_else(|| panic!("{key}"));
                let args: Vec<String> = object
                    .get("with")
                    .and_then(Value::as_array)
                    .map(|args| args.iter().map(|arg| render(arg, lang)).collect())
                    .unwrap_or_default();
                let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                out.push_str(&machinery_basis_core::units::format_lang(template, &refs));
            }
            if let Some(extra) = object.get("extra").and_then(Value::as_array) {
                for part in extra {
                    out.push_str(&render(part, lang));
                }
            }
            out
        }
        other => panic!("{other}"),
    }
}

#[test]
fn the_stored_energy_line_is_the_one_of_the_original() {
    let lang = lang();
    let text = |key: &str| lang[key].as_str().unwrap().to_owned();
    let mut checked = 0;
    for row in lines("item_energy_items.jsonl") {
        if row.get("stack").is_none() {
            continue;
        }
        let stack = &row["stack"];
        let item = stack["item"].as_str().unwrap();
        let Some(tooltip) = stack.get("tooltip").and_then(Value::as_array) else {
            continue;
        };
        let line = tooltip
            .iter()
            .find(|line| line["translate"] == rules::STORED_ENERGY_LANG);
        let Some(cap) = capacity(item) else {
            assert!(line.is_none(), "{item}");
            continue;
        };
        let line = line.unwrap_or_else(|| panic!("{item}"));
        let energy = stack["energy"].as_i64().unwrap();
        let display = energy_display(
            energy,
            cap,
            &text(rules::ENERGY_UNIT_LANG),
            &text(rules::FRACTION_LANG),
            &text(rules::INFINITE_LANG),
        );
        // O argumento da linha, com a cor cinza; o rótulo, em verde.
        let argument = &line["with"][0];
        assert_eq!(render(argument, &lang), display, "{item} {energy}");
        let color = |value: &Value| {
            u32::from_str_radix(value["color"].as_str().unwrap().trim_start_matches('#'), 16)
                .unwrap()
        };
        assert_eq!(color(line), rules::STORED_ENERGY_COLOR, "{item}");
        assert_eq!(color(argument), rules::STORED_ENERGY_ARG_COLOR, "{item}");
        assert_eq!(
            render(line, &lang),
            format!("Stored energy: {display}"),
            "{item}"
        );
        checked += 1;
    }
    assert!(checked >= 100, "{checked}");
}

#[test]
fn the_component_is_a_long_that_the_plain_item_does_not_carry() {
    for row in lines("item_energy_items.jsonl") {
        if row.get("stack").is_none() {
            continue;
        }
        let stack = &row["stack"];
        let item = stack["item"].as_str().unwrap();
        let energy = stack["energy"].as_i64().unwrap();
        let supports = capacity(item).is_some();
        assert_eq!(stack["has_component"], supports, "{item}");
        // O padrão do item é `0L`: zero não sobra como retalho, o resto sim.
        let patch = stack["patch"].as_array().unwrap();
        assert_eq!(
            patch.iter().any(|key| key == rules::ENERGY_COMPONENT),
            supports && energy > 0,
            "{item} {energy}"
        );
        if supports && energy > 0 {
            assert_eq!(
                stack["encoded"]["components"][rules::ENERGY_COMPONENT],
                energy,
                "{item}"
            );
        }
    }
}

#[test]
fn a_placed_machine_keeps_what_fits_and_a_broken_one_drops_all_it_has() {
    let mut placed = 0;
    let mut broken = 0;
    for row in lines("item_energy_machine.jsonl") {
        if let Some(carried) = row["energy_in_item"].as_i64() {
            let tile = &row["placed"];
            let expected = if carried < 0 {
                0
            } else {
                copy_to_container(carried, tile["capacity"].as_i64().unwrap())
            };
            assert_eq!(tile["energy"], expected, "{row}");
            placed += 1;
        } else {
            // O item solto leva a energia do tile, sem limite, e o zero não deixa retalho (o item empilha com o simples).
            let stored = row["before_break"].as_i64().unwrap();
            let drop = &row["drops"][0];
            assert_eq!(drop["energy"], stored, "{row}");
            let patch = drop["patch"].as_array().unwrap();
            assert_eq!(
                patch.iter().any(|key| key == rules::ENERGY_COMPONENT),
                stored > 0,
                "{row}"
            );
            assert_eq!(row["stacks_with_plain"], stored == 0, "{row}");
            broken += 1;
        }
    }
    assert_eq!((placed, broken), (30, 20));
}

#[test]
fn the_creative_tab_shows_the_empty_and_the_filled_item_side_by_side() {
    let rows = lines("item_energy_creative.jsonl");
    let mut filled = 0;
    for row in &rows {
        if row["tab"] != "mekanism" || row.get("item").is_none() {
            continue;
        }
        let item = row["item"].as_str().unwrap();
        let energy = row["energy"].as_i64().unwrap();
        if item == "mekanism:creative_energy_cube" {
            continue;
        }
        if energy > 0 {
            assert_eq!(table::filled_energy(item), Some(energy), "{item}");
            filled += 1;
        }
    }
    assert_eq!(filled, table::FILLED_VARIANTS.len());
    // Cada cheio vem logo depois do vazio, na lista que o Horizon escreveu.
    let mut tab: Vec<String> = [
        "mekanism:robit",
        "mekanism:salt",
        "mekanism:energy_tablet",
        "mekanism:basic_energy_cube",
    ]
    .iter()
    .map(|id| table::plain_stack(id))
    .collect();
    assert!(table::edit_tab(table::MEKANISM_TAB, &mut tab));
    // No fim da aba do mod vêm os tanques cheios (`addFilledTanks`, frente `tanques`), depois de tudo.
    let tanks = table::filled_tanks(true);
    assert_eq!(tab[tab.len() - tanks.len()..], tanks[..]);
    tab.truncate(tab.len() - tanks.len());
    assert_eq!(
        tab,
        [
            table::plain_stack("mekanism:robit"),
            table::filled_stack("mekanism:robit", 100_000),
            table::plain_stack("mekanism:salt"),
            table::plain_stack("mekanism:energy_tablet"),
            table::filled_stack("mekanism:energy_tablet", 1_000_000),
            table::plain_stack("mekanism:basic_energy_cube"),
            table::filled_stack("mekanism:basic_energy_cube", 4_000_000),
        ]
    );
    // O Heat Generator e as máquinas só aparecem vazios.
    for row in rows {
        if let Some(item) = row["item"].as_str()
            && row["energy"].as_i64().unwrap() > 0
            && item != "mekanism:creative_energy_cube"
        {
            assert!(table::filled_energy(item).is_some(), "{item}");
        }
    }
}

#[test]
fn the_creative_cube_enters_the_tab_as_the_empty_and_the_full_stack_with_the_side_config() {
    // O original o põe em duas pilhas (`ItemBlockEnergyCube.addItems`, `addDefault` falso): a energia toda de entrada e o item vazio, e a toda de saída e o item cheio.
    let rows: Vec<Value> = lines("item_energy_creative.jsonl")
        .into_iter()
        .filter(|row| row["tab"] == "mekanism" && row["item"] == "mekanism:creative_energy_cube")
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["energy"], 0);
    assert_eq!(
        rows[0]["patch"],
        serde_json::json!(["mekanism:side_config"])
    );
    assert_eq!(rows[1]["energy"], i64::MAX);
    assert_eq!(
        rows[1]["patch"],
        serde_json::json!(["mekanism:energy", "mekanism:side_config"])
    );
    let [empty, full] = table::creative_cube_stacks();
    assert!(empty.contains("\"mekanism:side_config\"") && !empty.contains("\"mekanism:energy\""));
    assert!(full.contains("\"mekanism:energy\":9223372036854775807L") && full.contains("eject:1b"));
    // Entra no lugar do item simples, na aba do mod e nas do vanilla.
    let mut tab = vec![
        table::plain_stack("mekanism:basic_energy_cube"),
        table::plain_stack("mekanism:creative_energy_cube"),
        table::plain_stack("mekanism:salt"),
    ];
    assert!(table::edit_tab(table::MEKANISM_TAB, &mut tab));
    assert_eq!(
        tab[2..5],
        [
            empty.clone(),
            full.clone(),
            table::plain_stack("mekanism:salt")
        ]
    );
    let added = table::additions("minecraft:functional_blocks");
    let at = added.iter().position(|stack| *stack == empty).unwrap();
    assert_eq!(added[at + 1], full);
    assert!(!added.contains(&table::plain_stack("mekanism:creative_energy_cube")));
}

fn stack(item: &str, energy: i64) -> items::ItemStack {
    let mut stack = items::ItemStack::new(item, 1);
    if energy > 0 {
        stack
            .data_mut()
            .set_other(rules::ENERGY_COMPONENT, Some(items::Tag::Long(energy)));
    }
    stack
}

fn energy_of(stack: &items::ItemStack) -> i64 {
    match stack
        .data()
        .and_then(|data| data.other(rules::ENERGY_COMPONENT))
    {
        Some(items::Tag::Long(value)) => *value,
        _ => 0,
    }
}

#[test]
fn the_mek_data_recipe_adds_the_energy_of_the_inputs_up_to_the_capacity_of_the_result() {
    use mekanism_mek_data as module;
    module::forget();
    for recipe in [
        "mekanism:energy_cube/advanced",
        "mekanism:factory/basic/enriching",
    ] {
        module::register(recipe);
    }
    for item in rules::GEAR_ITEMS
        .iter()
        .chain(rules::CUBE_TIERS.iter())
        .chain(
            [
                "mekanism:enrichment_chamber",
                "mekanism:basic_induction_cell",
            ]
            .iter(),
        )
    {
        module::set_capacity(item, capacity(item).unwrap());
    }
    let cases = lines("mek_data_assemble.jsonl");
    let mut checked = 0;
    for case in cases {
        let recipe = case["recipe"].as_str().unwrap();
        let grid: Vec<Option<items::ItemStack>> = case["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|input| {
                (!input["empty"].as_bool().unwrap()).then(|| {
                    stack(
                        input["item"].as_str().unwrap(),
                        input["energy"].as_i64().unwrap(),
                    )
                })
            })
            .collect();
        let result = &case["result"];
        let template = items::ItemStack::new(result["item"].as_str().unwrap(), 1);
        let made = module::assemble(recipe, &grid, Some(template)).unwrap();
        // A fábrica não guarda energia no port (os blocos de fábrica ainda não existem): o rastro dá a do original, e a conta é a mesma sempre que o resultado a guarda.
        if module_knows(result["item"].as_str().unwrap()) {
            assert_eq!(
                energy_of(&made),
                result["energy"].as_i64().unwrap(),
                "{}",
                case["case"]
            );
            checked += 1;
        }
    }
    assert!(checked >= 5, "{checked}");
}

fn module_knows(item: &str) -> bool {
    capacity(item).is_some()
}

/// Os casos que o rastro não cobre: o excesso se perde (a soma não passa da capacidade do resultado), a soma satura em `Long.MAX_VALUE`, e uma receita que não é `mek_data` não copia nada.
#[test]
fn the_sum_is_clamped_to_the_result_saturates_and_only_mek_data_copies() {
    use mekanism_mek_data as module;
    module::forget();
    module::register("mekanism:energy_cube/advanced");
    module::set_capacity("mekanism:energy_tablet", 1_000_000);
    module::set_capacity("mekanism:crusher", 32);
    module::set_capacity("mekanism:creative_energy_cube", i64::MAX);
    let grid = |inputs: &[(&str, i64)]| -> Vec<Option<items::ItemStack>> {
        inputs
            .iter()
            .map(|(item, energy)| Some(stack(item, *energy)))
            .collect()
    };
    let tablets = grid(&[
        ("mekanism:energy_tablet", 1_000),
        ("mekanism:energy_tablet", 2_500),
    ]);
    let made = |name: &str, grid: &[Option<items::ItemStack>], result: &str, stored: i64| {
        module::assemble(name, grid, Some(stack(result, stored))).unwrap()
    };
    // O que cabe: 32 de 3 500.
    assert_eq!(
        energy_of(&made(
            "mekanism:energy_cube/advanced",
            &tablets,
            "mekanism:crusher",
            0
        )),
        32
    );
    // Já com 30 guardados, cabem 2.
    assert_eq!(
        energy_of(&made(
            "mekanism:energy_cube/advanced",
            &tablets,
            "mekanism:crusher",
            30
        )),
        32
    );
    // A soma satura.
    let huge = grid(&[
        ("mekanism:creative_energy_cube", i64::MAX),
        ("mekanism:energy_tablet", 1_000),
    ]);
    module::set_capacity("mekanism:creative_energy_cube", i64::MAX);
    assert_eq!(
        energy_of(&made(
            "mekanism:energy_cube/advanced",
            &huge,
            "mekanism:creative_energy_cube",
            0
        )),
        i64::MAX
    );
    // Uma receita que não é `mek_data` não copia, e um resultado que não guarda energia fica como está.
    assert_eq!(
        energy_of(&made("minecraft:stick", &tablets, "mekanism:crusher", 0)),
        0
    );
    assert_eq!(
        energy_of(&made(
            "mekanism:energy_cube/advanced",
            &tablets,
            "minecraft:stone",
            0
        )),
        0
    );
}

/// Os arquivos `gear.toml` e `tiers.toml` com o texto padrão dão os valores padrão que o resto do port usa: a especificação e as constantes não se afastam.
#[test]
fn the_default_config_files_give_the_defaults_the_port_uses() {
    let gear = rules::GEAR_CONFIG.load_after(&rules::GEAR_CONFIG.default_text(), None);
    assert_eq!(rules::Gear::of(&gear), rules::Gear::DEFAULT);
    let tiers = rules::TIERS_CONFIG.load_after(&rules::TIERS_CONFIG.default_text(), None);
    assert_eq!(
        rules::cube_capacities(&tiers),
        rules::DEFAULT_CUBE_CAPACITIES
    );
    assert_eq!(rules::cube_rates(&tiers), rules::DEFAULT_CUBE_RATES);
    assert_eq!(
        rules::cell_capacities(&tiers),
        rules::DEFAULT_CELL_CAPACITIES
    );
    let text = rules::GEAR_CONFIG.default_text();
    assert!(
        text.contains("[atomic_disassembler]") && text.contains("\tmaxEnergy = 1000000"),
        "{text}"
    );
}

/// Os dois HEAD de `Recipes::find_with` rodam na ordem alfabética do id: o que guarda a grade (`_grid`) tem de vir antes do que a lê (`_use`), senão o segundo lê a grade da chamada anterior.
#[test]
fn the_head_that_stashes_the_grid_sorts_before_the_head_that_reads_it() {
    let toml = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("native/horizon-mixin.toml"),
    )
    .unwrap();
    assert!(
        toml.contains("id = \"mek_data_energy_grid\"\n")
            && toml.contains("id = \"mek_data_energy_use\"\n")
    );
    assert!("mek_data_energy_grid" < "mek_data_energy_use");
}

/// A aba criativa leva o cheio com a capacidade de agora: `filled_variant_energy` com a configuração padrão dá o valor da tabela (`FILLED_VARIANTS`, o do rastro), e com `gear.toml` e `tiers.toml`
/// mudados dá o valor novo (`getFilledVariant` é a capacidade).
#[test]
fn the_filled_stack_of_the_tab_follows_the_config() {
    let config = rules::ServerConfig::DEFAULT;
    for (item, energy) in table::FILLED_VARIANTS {
        assert_eq!(
            rules::filled_variant_energy(item, &config),
            Some(energy),
            "{item}"
        );
    }
    let mut changed = config;
    changed.gear.tablet.capacity = 2_000_000;
    changed.gear.meka_suit.capacity = 5;
    changed.cubes[1] = 123;
    assert_eq!(
        rules::filled_variant_energy("mekanism:energy_tablet", &changed),
        Some(2_000_000)
    );
    assert_eq!(
        rules::filled_variant_energy("mekanism:mekasuit_boots", &changed),
        Some(5)
    );
    assert_eq!(
        rules::filled_variant_energy("mekanism:advanced_energy_cube", &changed),
        Some(123)
    );
    assert_eq!(
        rules::filled_variant_energy("mekanism:creative_energy_cube", &changed),
        None
    );
    assert_eq!(
        rules::filled_variant_energy("mekanism:enrichment_chamber", &changed),
        None
    );
    assert_eq!(
        rules::filled_variant_energy("mekanism:configurator", &changed),
        None
    );
}
