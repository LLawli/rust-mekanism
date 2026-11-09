//! A aba criativa do Mekanism (`MekanismCreativeTabs`): a aba do mod, com os itens e blocos na ordem de registro (`CreativeTabDeferredRegister.addToDisplay` sobre
//! `MekanismItems.ITEMS` e `MekanismBlocks.BLOCKS`), e o que o original acrescenta às abas do vanilla (`addToExistingTabs`). O Horizon põe a aba do mod pelo arquivo
//! `creative_tab/mekanism.json` (só ids); as pilhas de upgrade com tipo e as abas do vanilla vêm da injeção `creative_tabs`, cuja tabela e conta são
//! `native/creative_tabs_table.rs`, compilada aqui.
mod common;

#[path = "../native/creative_tabs_table.rs"]
mod table;

use common::craft::{content, declared, json, rastro};
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
    // `FUNCTIONAL_BLOCKS` e `REDSTONE_BLOCKS`: as máquinas (`Machine`, com `AttributeComparator`) e os cabos que não são de transporte, em ordem de registro.
    let machines = vec![
        "enrichment_chamber",
        "crusher",
        "metallurgic_infuser",
        "energized_smelter",
        "basic_universal_cable",
        "advanced_universal_cable",
        "elite_universal_cable",
        "ultimate_universal_cable",
    ];
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
    // instaladores. A caixa de papelão, os tanques de fluido e os fluidos não têm id no port ainda.
    let tools = vec![
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
        "upgrade",
        "basic_tier_installer",
        "advanced_tier_installer",
        "elite_tier_installer",
        "ultimate_tier_installer",
    ];
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
        ("minecraft:functional_blocks", machines.clone()),
        ("minecraft:redstone_blocks", machines),
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
    assert_eq!(items.len(), 2 + table::UPGRADE_TYPES.len());
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
    assert_eq!(tools.len(), 26 + table::UPGRADE_TYPES.len());
    assert_eq!(tools[0], table::plain_stack("mekanism:configurator"));
    assert_eq!(tools[22], table::upgrade_stack("anchor"));
    assert_eq!(
        tools[30],
        table::plain_stack("mekanism:basic_tier_installer")
    );
    let mut combat = vec![table::plain_stack("minecraft:bow")];
    assert!(table::edit_tab("minecraft:combat", &mut combat));
    assert_eq!(combat.len(), 11);
    assert_eq!(combat[10], table::plain_stack("mekanism:jetpack_armored"));
}
