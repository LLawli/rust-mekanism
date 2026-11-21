//! A aba criativa do Mekanism (`MekanismCreativeTabs`): a aba do mod, com os itens e blocos na ordem de registro (`CreativeTabDeferredRegister.addToDisplay` sobre
//! `MekanismItems.ITEMS` e `MekanismBlocks.BLOCKS`), e o que o original acrescenta às abas do vanilla (`addToExistingTabs`). O Horizon põe a aba do mod pelo arquivo
//! `creative_tab/mekanism.json` (só ids); as pilhas de upgrade com tipo e as abas do vanilla vêm da injeção `creative_tabs`, cuja tabela e conta são
//! `native/creative_tabs_table.rs`, compilada aqui.
mod common;

#[path = "../native/creative_tabs_table.rs"]
mod table;

use common::craft::{blocos, content, declared, json, rastro};
use serde_json::Value;
use std::collections::BTreeSet;

fn port_ids() -> BTreeSet<String> {
    declared("item")
        .into_iter()
        .chain(declared("block"))
        .collect()
}

/// A aba do mod: os itens na ordem de registro de `MekanismItems`, depois os blocos na de `MekanismBlocks`, só os que o port tem. O `module` não entra sozinho (o
/// original o troca por uma pilha por módulo, e o port não tem módulos), e o `upgrade` entra pelo id, que a injeção troca pelas pilhas de tipo.
#[test]
fn the_mod_tab_lists_the_registration_order_of_the_original() {
    let order = rastro("craft_creative_order.json");
    let have = port_ids();
    let mut expected: Vec<String> = Vec::new();
    for key in ["items", "blocks"] {
        for id in order[key].as_array().unwrap() {
            let id = id.as_str().unwrap();
            if have.contains(id) && id != "module" {
                expected.push(format!("mekanism:{id}"));
            }
        }
    }
    // `CreativeTabDeferredRegister.addToDisplay(MekanismFluids.FLUIDS, ..)` vem depois dos blocos: os baldes dos 18 fluidos do Mekanism, na ordem de registro
    // (`rastros/fluids.jsonl`, da frente `fluidos`).
    let fluids = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../dev/referencia/rastros/fluids.jsonl"),
    )
    .unwrap();
    for line in fluids.lines().filter(|line| !line.trim().is_empty()) {
        let row: serde_json::Value = serde_json::from_str(line).unwrap();
        if row["mod"] == "mekanism" {
            expected.push(row["bucket"]["id"].as_str().unwrap().to_owned());
        }
    }
    let tab = json(&content().join("data/mekanism/horizon/creative_tab/mekanism.json"));
    let ours: Vec<String> = tab["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(ours, expected);
    assert!(ours.len() >= 140, "{}", ours.len());
    // `registerMain(MekanismLang.MEKANISM, METALLURGIC_INFUSER.getItemHolder(), ..)`: o título é "Mekanism" e o ícone é o Metallurgic Infuser.
    assert_eq!(tab["title"], "constants.mekanism.mod_name");
    assert_eq!(tab["icon"], "mekanism:metallurgic_infuser");
    let lang = json(&content().join("data/../assets/mekanism/lang/en_us.json"));
    assert_eq!(lang["constants.mekanism.mod_name"], "Mekanism");
    // Cada id da aba existe no port e cada um aparece uma vez.
    assert_eq!(ours.iter().collect::<BTreeSet<_>>().len(), ours.len());
    for id in &ours {
        assert!(have.contains(id.strip_prefix("mekanism:").unwrap()), "{id}");
    }
}

/// O que `addToExistingTabs` põe nas abas do vanilla, com o que o port tem, na ordem do original (`MekanismCreativeTabs.java:80-150`). Os nomes do Java por aba.
const FLUID_BUCKETS: [&str; 18] = [
    "hydrogen_bucket",
    "oxygen_bucket",
    "chlorine_bucket",
    "sulfur_dioxide_bucket",
    "sulfur_trioxide_bucket",
    "sulfuric_acid_bucket",
    "hydrogen_chloride_bucket",
    "hydrofluoric_acid_bucket",
    "uranium_oxide_bucket",
    "uranium_hexafluoride_bucket",
    "ethene_bucket",
    "sodium_bucket",
    "superheated_sodium_bucket",
    "brine_bucket",
    "lithium_bucket",
    "steam_bucket",
    "heavy_water_bucket",
    "nutritional_paste_bucket",
];

fn java_tabs() -> Vec<(&'static str, Vec<&'static str>)> {
    // `BUILDING_BLOCKS`: o sal, o bronze, o aço, o carvão vegetal, o obsidiano e o glowstone refinados, e o bloco de cada `PrimaryResource` com `resourceBlockInfo`.
    let building = vec![
        "block_salt",
        "block_bronze",
        "block_steel",
        "block_charcoal",
        "block_refined_obsidian",
        "block_refined_glowstone",
        "block_osmium",
        "block_tin",
        "block_lead",
        "block_uranium",
    ];
    // `NATURAL_BLOCKS`: o sal e os minérios na ordem de `OreType` (`stone()`, `deepslate()`).
    let natural = vec![
        "block_salt",
        "tin_ore",
        "deepslate_tin_ore",
        "osmium_ore",
        "deepslate_osmium_ore",
        "uranium_ore",
        "deepslate_uranium_ore",
        "fluorite_ore",
        "deepslate_fluorite_ore",
        "lead_ore",
        "deepslate_lead_ore",
    ];
    // `FUNCTIONAL_BLOCKS` e `REDSTONE_BLOCKS` (`MekanismCreativeTabs.java:99-115`), com o que o original em jogo diz de cada bloco (`rastros/blocos.jsonl`): `functional_tab` é
    // "transmissor ou `Machine`" e `redstone_tab` é "tem `AttributeComparator`", ambos na ordem do registro; depois a lista fixa de cada aba.
    let blocks = blocos();
    let have = port_ids();
    let ported = |row: &&Value| {
        have.contains(
            row["id"]
                .as_str()
                .unwrap()
                .strip_prefix("mekanism:")
                .unwrap(),
        )
    };
    let id = |row: &Value| {
        row["id"]
            .as_str()
            .unwrap()
            .strip_prefix("mekanism:")
            .unwrap()
            .to_owned()
    };
    let mut machines: Vec<String> = blocks
        .iter()
        .filter(ported)
        .filter(|r| r["functional_tab"] == true)
        .map(&id)
        .collect();
    machines.extend(
        [
            "security_desk",
            "radioactive_waste_barrel",
            "personal_chest",
            "personal_barrel",
            "chargepad",
            "laser",
            "laser_amplifier",
            "laser_tractor_beam",
            "quantum_entangloporter",
            "oredictionificator",
            "fuelwood_heater",
            "modification_station",
            "qio_drive_array",
            "qio_dashboard",
            "qio_exporter",
            "qio_importer",
            "qio_redstone_adapter",
        ]
        .map(str::to_owned),
    );
    // O alarme primeiro; depois cada bloco com comparador e cada transmissor com `AttributeTier` que não é o do transportador (`MekanismCreativeTabs.java:107-112`: cabos,
    // tubos, canos e condutores), na ordem do registro; e no fim o desviador.
    let tiered = |r: &Value| {
        let attributes = r["attributes"].as_array().unwrap();
        let transporter = r["item"]["components"].as_array().unwrap().iter().any(|c| {
            c.as_str()
                .unwrap()
                .starts_with("mekanism:tier/transporter=")
        });
        ["BlockSmallTransmitter", "BlockLargeTransmitter"].contains(&r["class"].as_str().unwrap())
            && attributes.iter().any(|a| a == "AttributeTier")
            && !transporter
    };
    let mut redstone = vec!["industrial_alarm".to_owned()];
    redstone.extend(
        blocks
            .iter()
            .filter(ported)
            .filter(|r| (r["redstone_tab"] == true || tiered(r)) && id(r) != "industrial_alarm")
            .map(id),
    );
    redstone.push("diversion_transporter".to_owned());
    let machines: Vec<&'static str> = machines
        .into_iter()
        .map(|id| &*Box::leak(id.into_boxed_str()))
        .collect();
    let redstone: Vec<&'static str> = redstone
        .into_iter()
        .map(|id| &*Box::leak(id.into_boxed_str()))
        .collect();
    // `INGREDIENTS`: a lista de `MekanismCreativeTabs.java:132-145` e depois `PROCESSED_RESOURCES.values()` (por tipo, depois por recurso, na ordem de registro).
    let mut ingredients = vec![
        "module_base",
        "alloy_infused",
        "alloy_reinforced",
        "alloy_atomic",
        "basic_control_circuit",
        "advanced_control_circuit",
        "elite_control_circuit",
        "ultimate_control_circuit",
        "enriched_carbon",
        "enriched_redstone",
        "enriched_diamond",
        "enriched_refined_obsidian",
        "enriched_gold",
        "enriched_tin",
        "bio_fuel",
        "block_bio_fuel",
        "substrate",
        "hdpe_pellet",
        "hdpe_rod",
        "hdpe_sheet",
        "pellet_antimatter",
        "pellet_plutonium",
        "pellet_polonium",
        "reprocessed_fissile_fragment",
        "electrolytic_core",
        "teleportation_core",
        "enriched_iron",
        "sawdust",
        "salt",
        "dye_base",
        "fluorite_gem",
        "dust_fluorite",
        "yellow_cake_uranium",
        "dirty_netherite_scrap",
        "dust_netherite",
        "dust_charcoal",
        "dust_coal",
        "dust_sulfur",
        "dust_bronze",
        "dust_lapis_lazuli",
        "dust_quartz",
        "dust_emerald",
        "dust_diamond",
        "dust_steel",
        "dust_obsidian",
        "dust_refined_obsidian",
        "nugget_bronze",
        "nugget_steel",
        "nugget_refined_obsidian",
        "nugget_refined_glowstone",
        "ingot_bronze",
        "ingot_steel",
        "ingot_refined_obsidian",
        "ingot_refined_glowstone",
    ];
    let primary = ["iron", "gold", "osmium", "copper", "tin", "lead", "uranium"];
    let mut processed: Vec<String> = Vec::new();
    for kind in [
        "shard",
        "crystal",
        "dust",
        "dirty_dust",
        "clump",
        "ingot",
        "raw",
        "nugget",
    ] {
        for resource in primary {
            let vanilla = ["iron", "gold", "copper"].contains(&resource);
            if !(vanilla && ["ingot", "raw", "nugget"].contains(&kind)) {
                processed.push(format!("{kind}_{resource}"));
            }
        }
    }
    let processed: Vec<&'static str> = processed
        .into_iter()
        .map(|id| &*Box::leak(id.into_boxed_str()))
        .collect();
    ingredients.extend(processed);
    // `TOOLS_AND_UTILITIES` (`MekanismCreativeTabs.java:120-132`): as ferramentas, as armaduras de uso geral, o upgrade (que a injeção troca por uma pilha por tipo) e os
    // instaladores, e os cinco tanques de fluido (`:132-135`).
    let mut tools = vec![
        "configurator",
        "network_reader",
        "dosimeter",
        "geiger_counter",
        "dictionary",
        "configuration_card",
        "gauge_dropper",
        "crafting_formula",
        "portable_qio_dashboard",
        "atomic_disassembler",
        "meka_tool",
        "scuba_mask",
        "scuba_tank",
        "free_runners",
        "free_runners_armored",
        "jetpack",
        "jetpack_armored",
        "hdpe_elytra",
        "hazmat_mask",
        "hazmat_gown",
        "hazmat_pants",
        "hazmat_boots",
        "cardboard_box",
        "upgrade",
        "basic_tier_installer",
        "advanced_tier_installer",
        "elite_tier_installer",
        "ultimate_tier_installer",
        "basic_fluid_tank",
        "advanced_fluid_tank",
        "elite_fluid_tank",
        "ultimate_fluid_tank",
        "creative_fluid_tank",
    ];
    // `addToDisplay(MekanismFluids.FLUIDS, event)` (`MekanismCreativeTabs.java:136`): os baldes do Mekanism, depois dos instaladores e dos tanques.
    tools.extend(FLUID_BUCKETS);
    // `COMBAT` (`MekanismCreativeTabs.java:138-141`).
    let combat = vec![
        "atomic_disassembler",
        "flamethrower",
        "electric_bow",
        "meka_tool",
        "mekasuit_helmet",
        "mekasuit_bodyarmor",
        "mekasuit_pants",
        "mekasuit_boots",
        "free_runners_armored",
        "jetpack_armored",
    ];
    vec![
        ("minecraft:building_blocks", building),
        ("minecraft:natural_blocks", natural),
        ("minecraft:functional_blocks", machines),
        ("minecraft:redstone_blocks", redstone),
        ("minecraft:tools_and_utilities", tools),
        ("minecraft:combat", combat),
        ("minecraft:ingredients", ingredients),
    ]
}

#[test]
fn the_vanilla_tabs_get_what_the_original_adds_in_its_order() {
    let have = port_ids();
    let java = java_tabs();
    assert_eq!(java.len(), table::VANILLA_TABS.len());
    for ((tab, expected), (ours_tab, ours)) in java.iter().zip(table::VANILLA_TABS.iter()) {
        assert_eq!(tab, ours_tab);
        let expected: Vec<String> = expected
            .iter()
            .filter(|id| have.contains(**id))
            .map(|id| format!("mekanism:{id}"))
            .collect();
        let ours: Vec<String> = ours.iter().map(|id| (*id).to_owned()).collect();
        assert_eq!(ours, expected, "{tab}");
    }
}

#[test]
fn the_stacks_are_written_as_the_horizon_reads_them() {
    assert_eq!(
        table::plain_stack("mekanism:salt"),
        "{count:1,id:\"mekanism:salt\"}"
    );
    assert_eq!(
        table::upgrade_stack("speed"),
        "{count:1,id:\"mekanism:upgrade\",components:{\"mekanism:upgrade_type\":\"mekanism:speed\"}}"
    );
    // Os tipos são os arquivos de `data/mekanism/mekanism/upgrade`, na ordem do nome.
    let mut kinds: Vec<String> =
        std::fs::read_dir(content().join("data/mekanism/mekanism/upgrade"))
            .unwrap()
            .map(|entry| {
                entry
                    .unwrap()
                    .file_name()
                    .into_string()
                    .unwrap()
                    .trim_end_matches(".json")
                    .to_owned()
            })
            .collect();
    kinds.sort();
    assert_eq!(kinds, table::UPGRADE_TYPES);
}

#[test]
fn the_mod_tab_swaps_the_upgrade_item_for_one_stack_per_type() {
    let mut items = vec![
        table::plain_stack("mekanism:module_base"),
        table::plain_stack("mekanism:upgrade"),
        table::plain_stack("mekanism:salt"),
    ];
    assert!(table::edit_tab(table::MEKANISM_TAB, &mut items));
    assert_eq!(
        items.len(),
        2 + table::UPGRADE_TYPES.len()
            + table::FILLED_TANK_FLUIDS.len()
            + table::FILLED_TANK_CHEMICALS.len()
    );
    assert_eq!(items[0], table::plain_stack("mekanism:module_base"));
    assert_eq!(items[1], table::upgrade_stack("anchor"));
    assert_eq!(items[8], table::upgrade_stack("xp_containment"));
    assert_eq!(items[9], table::plain_stack("mekanism:salt"));
    // Sem o item, nada muda; o item sem tipo não sobra.
    assert!(!table::edit_tab(table::MEKANISM_TAB, &mut items));
}

#[test]
fn a_vanilla_tab_gets_the_items_at_the_end_and_other_tabs_are_left_alone() {
    let mut items = vec![table::plain_stack("minecraft:coal")];
    assert!(table::edit_tab("minecraft:ingredients", &mut items));
    assert_eq!(items[0], table::plain_stack("minecraft:coal"));
    assert_eq!(items[1], table::plain_stack("mekanism:module_base"));
    assert!(items.len() > 100);
    let mut search = vec![table::plain_stack("minecraft:coal")];
    assert!(!table::edit_tab("minecraft:search", &mut search));
    assert!(!table::edit_tab("minecraft:spawn_eggs", &mut search));
    assert_eq!(search.len(), 1);
    // As ferramentas: 26 itens e, no lugar do upgrade, uma pilha por tipo.
    let mut tools = Vec::new();
    assert!(table::edit_tab("minecraft:tools_and_utilities", &mut tools));
    // O Cardboard Box vem antes do item de upgrade (`MekanismCreativeTabs.java:128`). Os quatro itens que guardam energia (o Atomic Disassembler, o Meka-Tool e os dois Free Runners)
    // levam o cheio depois do vazio (`addToDisplay(event, ..)` chama o `addItems` do `ICustomCreativeTabContents`).
    assert_eq!(
        tools.len(),
        27 + 5
            + table::UPGRADE_TYPES.len()
            + FLUID_BUCKETS.len()
            + 4
            + table::FILLED_TANK_FLUIDS.len()
    );
    assert_eq!(tools[0], table::plain_stack("mekanism:configurator"));
    assert_eq!(tools[22 + 4], table::plain_stack("mekanism:cardboard_box"));
    assert_eq!(tools[23 + 4], table::upgrade_stack("anchor"));
    assert_eq!(
        tools[31 + 4],
        table::plain_stack("mekanism:basic_tier_installer")
    );
    // O último instalador, os cinco tanques de fluido e os 18 baldes dos fluidos do Mekanism depois deles.
    assert_eq!(
        tools[table::UPGRADE_TYPES.len() + 27 - 1 + 4],
        table::plain_stack("mekanism:ultimate_tier_installer")
    );
    assert_eq!(
        tools[table::UPGRADE_TYPES.len() + 27 + 4],
        table::plain_stack("mekanism:basic_fluid_tank")
    );
    assert_eq!(
        tools[table::UPGRADE_TYPES.len() + 27 + 8],
        table::plain_stack("mekanism:creative_fluid_tank")
    );
    assert_eq!(
        tools[table::UPGRADE_TYPES.len() + 27 + 9],
        table::plain_stack("mekanism:hydrogen_bucket")
    );
    // Depois do último balde vêm os tanques de fluido cheios (`addFilledTanks(.., false)`).
    assert_eq!(
        tools[tools.len() - 1 - table::FILLED_TANK_FLUIDS.len()],
        table::plain_stack("mekanism:nutritional_paste_bucket")
    );
    assert_eq!(
        tools.last().unwrap(),
        &table::filled_fluid_tank("minecraft:milk")
    );
    let mut combat = vec![table::plain_stack("minecraft:bow")];
    assert!(table::edit_tab("minecraft:combat", &mut combat));
    // Os oito itens de energia da aba (o Atomic Disassembler, o arco, o Meka-Tool, as quatro peças do MekaSuit e o Free Runners blindado) levam o cheio depois do vazio.
    assert_eq!(combat.len(), 11 + 8);
    assert_eq!(
        combat[10 + 8],
        table::plain_stack("mekanism:jetpack_armored")
    );
}

/// `MekanismCreativeTabs.addFilledTanks`: um Fluid Tank criativo por fluido do registro (a água, a lava e os do Mekanism na ordem de `rastros/fluids.jsonl`) e um Chemical
/// Tank criativo por químico, na ordem do registro de dados (a dos arquivos da datagen, `TreeMap` de `Identifier`), sem o `mekanism:empty`.
#[test]
fn the_filled_tanks_follow_the_registries_of_the_original() {
    let rastro = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/fluids.jsonl");
    let mut fluids = vec!["minecraft:water".to_owned(), "minecraft:lava".to_owned()];
    for line in std::fs::read_to_string(rastro)
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
    {
        let row: serde_json::Value = serde_json::from_str(line).unwrap();
        if row["mod"] == "mekanism" {
            fluids.push(format!("mekanism:{}", row["name"].as_str().unwrap()));
        }
    }
    // O leite do NeoForge vem depois dos fluidos do Mekanism (`rastros/creative_tab_tanks.jsonl`, do cliente do original).
    fluids.push("minecraft:milk".to_owned());
    assert_eq!(table::FILLED_TANK_FLUIDS.to_vec(), fluids);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("content/data/mekanism/mekanism/chemical");
    let mut chemicals: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter_map(|name| {
            name.strip_suffix(".json")
                .map(|name| format!("mekanism:{name}"))
        })
        .filter(|id| id != "mekanism:empty")
        .collect();
    chemicals.sort_unstable();
    assert!(chemicals.len() >= 64, "{}", chemicals.len());
    assert_eq!(table::FILLED_TANK_CHEMICALS.to_vec(), chemicals);
}

#[test]
fn the_filled_tanks_close_the_mod_tab_and_follow_the_buckets_in_the_tools_tab() {
    assert_eq!(
        table::filled_fluid_tank("minecraft:water"),
        "{count:1,id:\"mekanism:creative_fluid_tank\",components:{\"mekanism:fluids\":{containers:[{amount:9223372036854775807L,type:{id:\"minecraft:water\"}}]}}}"
    );
    assert_eq!(
        table::filled_chemical_tank("mekanism:hydrogen"),
        "{count:1,id:\"mekanism:creative_chemical_tank\",components:{\"mekanism:chemicals\":{containers:[{amount:9223372036854775807L,type:\"mekanism:hydrogen\"}]}}}"
    );
    let fluids = table::FILLED_TANK_FLUIDS.len();
    let chemicals = table::FILLED_TANK_CHEMICALS.len();
    // A aba do mod: os de fluido e depois os de químico, no fim (`addFilledTanks(.., true)`), uma vez só.
    let mut items = vec![table::plain_stack("mekanism:salt")];
    assert!(table::edit_tab(table::MEKANISM_TAB, &mut items));
    assert_eq!(items.len(), 1 + fluids + chemicals);
    assert_eq!(items[1], table::filled_fluid_tank("minecraft:water"));
    assert_eq!(items[2], table::filled_fluid_tank("minecraft:lava"));
    assert_eq!(
        items[fluids - 1],
        table::filled_fluid_tank("mekanism:nutritional_paste")
    );
    assert_eq!(items[fluids], table::filled_fluid_tank("minecraft:milk"));
    assert_eq!(
        items[1 + fluids],
        table::filled_chemical_tank("mekanism:antimatter")
    );
    assert_eq!(
        items[fluids + chemicals],
        table::filled_chemical_tank("mekanism:yellow")
    );
    assert!(!table::edit_tab(table::MEKANISM_TAB, &mut items));
    // As ferramentas: só os de fluido (`addFilledTanks(.., false)`), depois do último balde.
    let mut tools = Vec::new();
    assert!(table::edit_tab(table::FILLED_TANKS_TAB, &mut tools));
    let first = tools
        .iter()
        .position(|stack| *stack == table::filled_fluid_tank("minecraft:water"))
        .unwrap();
    assert_eq!(
        tools[first - 1],
        table::plain_stack("mekanism:nutritional_paste_bucket")
    );
    assert_eq!(first + fluids, tools.len());
    assert!(
        !tools
            .iter()
            .any(|stack| stack.contains("creative_chemical_tank"))
    );
    // As outras abas do vanilla não ganham tanque cheio.
    let mut blocks = Vec::new();
    table::edit_tab("minecraft:functional_blocks", &mut blocks);
    assert!(
        !blocks
            .iter()
            .any(|stack| stack.contains("mekanism:fluids") || stack.contains("mekanism:chemicals"))
    );
}

/// As duas abas como o cliente do original as monta (`rastros/creative_tab_tanks.jsonl`, gravado pela `CenaTelas` depois de
/// `CreativeModeTabs.tryRebuildTabContents`, um item por linha na ordem da aba): os tanques criativos cheios do Mekanism e do vanilla saem na ordem das duas tabelas
/// do port, cheios até `Long.MAX_VALUE`; na aba do mod eles fecham a aba (os de fluido e depois os de químico), e a Tools & Utilities só tem os de fluido. O cliente
/// do original roda com o Generators, que acrescenta os fluidos e os químicos dele no meio da ordem: ficam de fora desta conta (os fluidos estão na tabela do
/// Generators; os químicos dele o port ainda não tem na aba).
#[test]
fn the_filled_tanks_come_in_the_order_the_original_client_shows() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/creative_tab_tanks.jsonl");
    let rows: Vec<Value> = std::fs::read_to_string(path)
        .expect("o rastro das abas criativas")
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let ours = |id: &str| id.starts_with("minecraft:") || id.starts_with("mekanism:");
    for (tab, with_chemicals) in [
        ("mekanism:mekanism", true),
        ("minecraft:tools_and_utilities", false),
    ] {
        let of_tab: Vec<&Value> = rows.iter().filter(|row| row["tab"] == tab).collect();
        assert!(of_tab.len() > 20, "{tab}: {}", of_tab.len());
        let filled = |key: &str, item: &str| -> Vec<String> {
            of_tab
                .iter()
                // Outros itens também trazem fluido ou químico (o cantil cheio, o tanque de mergulho): só os tanques criativos contam.
                .filter(|row| row.get(key).is_some() && row["item"] == item)
                .map(|row| {
                    assert_eq!(
                        row["amount"].as_i64(),
                        Some(table::FILLED_TANK_AMOUNT),
                        "{tab}"
                    );
                    row[key].as_str().unwrap().to_owned()
                })
                .filter(|id| ours(id))
                .collect()
        };
        assert_eq!(
            filled("fluid", "mekanism:creative_fluid_tank"),
            table::FILLED_TANK_FLUIDS.to_vec(),
            "{tab}"
        );
        let chemicals = filled("chemical", "mekanism:creative_chemical_tank");
        if with_chemicals {
            assert_eq!(chemicals, table::FILLED_TANK_CHEMICALS.to_vec(), "{tab}");
        } else {
            assert!(chemicals.is_empty(), "{tab}");
        }
        // Os tanques cheios vêm juntos, os de fluido antes dos de químico. Na aba do mod eles a fecham; na do vanilla, os outros mods do cliente do original
        // (o Generators, o Additions e o Tools) ainda põem os itens deles depois.
        let is_filled = |row: &&Value| {
            (row["item"] == "mekanism:creative_fluid_tank" && row.get("fluid").is_some())
                || (row["item"] == "mekanism:creative_chemical_tank"
                    && row.get("chemical").is_some())
        };
        let first = of_tab.iter().position(&is_filled).unwrap();
        let count = of_tab.iter().filter(|row| is_filled(row)).count();
        let block = &of_tab[first..first + count];
        assert!(block.iter().all(is_filled), "{tab}");
        let kinds: Vec<bool> = block
            .iter()
            .map(|row| row.get("chemical").is_some())
            .collect();
        assert!(kinds.windows(2).all(|pair| pair[0] <= pair[1]), "{tab}");
        if with_chemicals {
            assert_eq!(first + count, of_tab.len(), "{tab}");
        }
    }
}
