//! Os itens e blocos de craft e de recurso do original (`MekanismItems`, `MekanismBlocks`, `BlockResourceInfo`, `OreType`), sem função: cada um tem todo o conteúdo que
//! o original dá a ele. Os arquivos que o port copia da datagen (`src/datagen/generated/mekanism`) ou dos recursos (`src/main/resources`) do original são conferidos
//! contra o hash que `dev/referencia/craft_rastros.py` tirou do checkout dele (`rastros/craft_datagen.json`); os ids da frente vêm da lista abaixo, e um que falte falha.
mod common;

use common::craft::{
    EQUIPMENT_ITEMS, assets, check_model, content, declared, digest, json, rastro,
};
use serde_json::Value;
use std::collections::BTreeSet;

/// Os itens da frente: o que `MekanismItems` registra como `Item` simples (sem função além de ingrediente), em ordem alfabética. Os 74 que o port não tinha e os que
/// já tinha (circuitos, ligas, pós, enriquecidos, osmium e aço).
const ITEMS: [&str; 107] = [
    "advanced_control_circuit",
    "alloy_atomic",
    "alloy_infused",
    "alloy_reinforced",
    "basic_control_circuit",
    "bio_fuel",
    "clump_copper",
    "clump_gold",
    "clump_iron",
    "clump_lead",
    "clump_osmium",
    "clump_tin",
    "clump_uranium",
    "crystal_copper",
    "crystal_gold",
    "crystal_iron",
    "crystal_lead",
    "crystal_osmium",
    "crystal_tin",
    "crystal_uranium",
    "dirty_dust_copper",
    "dirty_dust_gold",
    "dirty_dust_iron",
    "dirty_dust_lead",
    "dirty_dust_osmium",
    "dirty_dust_tin",
    "dirty_dust_uranium",
    "dirty_netherite_scrap",
    "dust_bronze",
    "dust_charcoal",
    "dust_coal",
    "dust_copper",
    "dust_diamond",
    "dust_emerald",
    "dust_fluorite",
    "dust_gold",
    "dust_iron",
    "dust_lapis_lazuli",
    "dust_lead",
    "dust_lithium",
    "dust_netherite",
    "dust_obsidian",
    "dust_osmium",
    "dust_quartz",
    "dust_refined_obsidian",
    "dust_steel",
    "dust_sulfur",
    "dust_tin",
    "dust_uranium",
    "dye_base",
    "electrolytic_core",
    "elite_control_circuit",
    "enriched_carbon",
    "enriched_diamond",
    "enriched_gold",
    "enriched_iron",
    "enriched_redstone",
    "enriched_refined_obsidian",
    "enriched_tin",
    "fluorite_gem",
    "hdpe_pellet",
    "hdpe_rod",
    "hdpe_sheet",
    "hdpe_stick",
    "ingot_bronze",
    "ingot_lead",
    "ingot_osmium",
    "ingot_refined_glowstone",
    "ingot_refined_obsidian",
    "ingot_steel",
    "ingot_tin",
    "ingot_uranium",
    "module",
    "module_base",
    "nugget_bronze",
    "nugget_lead",
    "nugget_osmium",
    "nugget_refined_glowstone",
    "nugget_refined_obsidian",
    "nugget_steel",
    "nugget_tin",
    "nugget_uranium",
    "pellet_antimatter",
    "pellet_plutonium",
    "pellet_polonium",
    "qio_drive_base",
    "qio_drive_hyper_dense",
    "qio_drive_supermassive",
    "qio_drive_time_dilating",
    "raw_lead",
    "raw_osmium",
    "raw_tin",
    "raw_uranium",
    "reprocessed_fissile_fragment",
    "salt",
    "sawdust",
    "shard_copper",
    "shard_gold",
    "shard_iron",
    "shard_lead",
    "shard_osmium",
    "shard_tin",
    "shard_uranium",
    "substrate",
    "teleportation_core",
    "ultimate_control_circuit",
    "yellow_cake_uranium",
];

/// Os blocos da frente: os blocos de recurso (`BlockResourceInfo`), os minérios (`OreType`), o sal, o bloco de bio fuel, o quadro do teleporter e o Steel Casing.
const BLOCKS: [&str; 28] = [
    "block_bio_fuel",
    "block_bronze",
    "block_charcoal",
    "block_fluorite",
    "block_lead",
    "block_osmium",
    "block_raw_lead",
    "block_raw_osmium",
    "block_raw_tin",
    "block_raw_uranium",
    "block_refined_glowstone",
    "block_refined_obsidian",
    "block_salt",
    "block_steel",
    "block_tin",
    "block_uranium",
    "deepslate_fluorite_ore",
    "deepslate_lead_ore",
    "deepslate_osmium_ore",
    "deepslate_tin_ore",
    "deepslate_uranium_ore",
    "fluorite_ore",
    "lead_ore",
    "osmium_ore",
    "steel_casing",
    "teleporter_frame",
    "tin_ore",
    "uranium_ore",
];

/// Os 74 itens e 25 blocos que o port não tinha (a lista `itens-faltando` da frente).
const NEW_COUNT: usize = 99;

#[test]
fn the_front_declares_every_item_and_block() {
    let items = declared("item");
    let blocks = declared("block");
    for id in ITEMS {
        assert!(
            items.contains(id),
            "item {id} sem declaração em horizon/item"
        );
    }
    for id in BLOCKS {
        assert!(
            blocks.contains(id),
            "bloco {id} sem declaração em horizon/block"
        );
    }
    assert_eq!(ITEMS.len(), ITEMS.iter().collect::<BTreeSet<_>>().len());
    // Tudo o que o port declara de craft e de recurso está na lista (o que não está tem outra frente: máquinas, cabos, `upgrade`).
    let listed: BTreeSet<&str> = ITEMS.iter().copied().collect();
    // Os de equipamento têm a lista e os testes da frente `itens-equipamento` (`equipment_content.rs`).
    let extra: Vec<_> = items
        .iter()
        .filter(|id| !listed.contains(id.as_str()) && !EQUIPMENT_ITEMS.contains(&id.as_str()))
        .collect();
    assert_eq!(
        extra,
        ["upgrade"],
        "itens declarados fora da lista da frente"
    );
    assert!(NEW_COUNT <= ITEMS.len() + BLOCKS.len());
}

fn model_of_item(id: &str) -> String {
    let item = json(&assets().join("items").join(format!("{id}.json")));
    item["model"]["model"]
        .as_str()
        .unwrap_or_else(|| panic!("{id}: item sem modelo"))
        .to_owned()
}

fn check_blockstate_models(value: &Value, seen: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if key == "model"
                    && let Some(reference) = child.as_str()
                {
                    check_model(reference, seen);
                }
                check_blockstate_models(child, seen);
            }
        }
        Value::Array(list) => list
            .iter()
            .for_each(|child| check_blockstate_models(child, seen)),
        _ => {}
    }
}

#[test]
fn every_item_has_its_model_and_texture() {
    let mut seen = BTreeSet::new();
    for id in ITEMS {
        check_model(&model_of_item(id), &mut seen);
    }
}

#[test]
fn every_block_has_blockstate_item_model_loot_table_and_name() {
    let lang = json(&assets().join("lang/en_us.json"));
    let mut seen = BTreeSet::new();
    for id in BLOCKS {
        let blockstate = assets().join("blockstates").join(format!("{id}.json"));
        assert!(blockstate.is_file(), "falta o estado de bloco de {id}");
        check_blockstate_models(&json(&blockstate), &mut seen);
        check_model(&model_of_item(id), &mut seen);
        let loot = content()
            .join("data/mekanism/loot_table/blocks")
            .join(format!("{id}.json"));
        assert!(loot.is_file(), "falta a loot table de {id}");
        assert_eq!(
            json(&loot)["random_sequence"],
            format!("mekanism:blocks/{id}"),
            "{id}"
        );
        assert!(
            lang[format!("block.mekanism.{id}")].is_string(),
            "falta o nome de {id}"
        );
    }
    for id in ITEMS {
        assert!(
            lang[format!("item.mekanism.{id}")].is_string(),
            "falta o nome de {id}"
        );
    }
}

/// O que cada loot table do original faz: o minério solta o bruto com fortuna e a si mesmo com toque suave; o bloco solta a si mesmo.
#[test]
fn ore_loot_tables_drop_the_raw_item_with_fortune() {
    for (ore, raw) in [
        ("tin", "raw_tin"),
        ("uranium", "raw_uranium"),
        ("lead", "raw_lead"),
        ("osmium", "raw_osmium"),
        ("fluorite", "fluorite_gem"),
    ] {
        for block in [format!("{ore}_ore"), format!("deepslate_{ore}_ore")] {
            let table = json(
                &content()
                    .join("data/mekanism/loot_table/blocks")
                    .join(format!("{block}.json")),
            );
            let children = &table["pools"][0]["entries"][0]["children"];
            assert_eq!(
                children[0]["name"],
                format!("mekanism:{block}"),
                "{block}: toque suave"
            );
            assert_eq!(
                children[0]["condition"], "minecraft:tool/can_silk_touch",
                "{block}"
            );
            assert_eq!(children[1]["name"], format!("mekanism:{raw}"), "{block}");
            // A fluorita solta de 2 a 4 gemas antes da fortuna (`MekanismBlockLootTables`, `uniform(2, 4)`); os metais, uma.
            let mut modifiers = children[1]["modifier"].as_array().unwrap().iter();
            if ore == "fluorite" {
                let count = modifiers.next().unwrap();
                assert_eq!(count["type"], "minecraft:set_count", "{block}");
                assert_eq!(
                    count["count"],
                    serde_json::json!({"type": "minecraft:uniform", "min": 2, "max": 4}),
                    "{block}"
                );
            }
            let fortune = modifiers.next().unwrap();
            assert_eq!(fortune["type"], "minecraft:apply_bonus", "{block}");
            assert_eq!(fortune["enchantment"], "minecraft:fortune", "{block}");
            assert_eq!(fortune["formula"], "minecraft:ore_drops", "{block}");
            assert_eq!(
                modifiers.next().unwrap()["type"],
                "minecraft:explosion_decay",
                "{block}"
            );
            assert!(modifiers.next().is_none(), "{block}");
        }
    }
}

#[test]
fn every_copied_file_is_the_one_the_original_writes() {
    let manifest = rastro("craft_datagen.json");
    let manifest = manifest.as_object().unwrap();
    assert!(
        manifest.len() > 1_000,
        "só {} arquivos no rastro",
        manifest.len()
    );
    let mut wrong = Vec::new();
    for (relative, expected) in manifest {
        let path = content().join(relative);
        if !path.is_file() {
            wrong.push(format!("falta {relative}"));
        } else if digest(&path) != expected.as_str().unwrap() {
            wrong.push(format!("{relative} difere do original"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    // Os arquivos de cada id da frente estão no rastro (o que falta não passa por vazio).
    for id in ITEMS {
        let relative = format!("assets/mekanism/items/{id}.json");
        assert!(
            manifest.contains_key(&relative),
            "{relative} fora do rastro"
        );
    }
    for id in BLOCKS {
        for relative in [
            format!("assets/mekanism/items/{id}.json"),
            format!("assets/mekanism/blockstates/{id}.json"),
            format!("data/mekanism/loot_table/blocks/{id}.json"),
        ] {
            assert!(
                manifest.contains_key(&relative),
                "{relative} fora do rastro"
            );
        }
    }
}

/// As receitas escritas à mão do ósmio (`osmium_from_*`) saíram: o original tem as dele em `processing/osmium/ingot`.
#[test]
fn the_hand_written_osmium_recipes_are_gone() {
    let folder = content().join("data/mekanism/recipe");
    for entry in std::fs::read_dir(&folder).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        assert!(!name.starts_with("osmium_from_"), "{name}");
    }
    for name in [
        "from_ore_smelting",
        "from_ore_blasting",
        "from_deepslate_ore_smelting",
        "from_deepslate_ore_blasting",
        "from_raw_smelting",
        "from_raw_blasting",
    ] {
        assert!(
            folder
                .join("processing/osmium/ingot")
                .join(format!("{name}.json"))
                .is_file(),
            "{name}"
        );
    }
}

/// Os materiais de acabamento (`MekanismTrimMaterials`): o arquivo, a paleta e o nome, para cada item que dá um (`Item.Properties.trimMaterial`).
#[test]
fn every_trim_material_has_its_file_palette_and_name() {
    let lang = json(&assets().join("lang/en_us.json"));
    let materials = [
        "antimatter",
        "bronze",
        "fluorite",
        "lead",
        "osmium",
        "plutonium",
        "polonium",
        "refined_glowstone",
        "refined_obsidian",
        "steel",
        "tin",
        "uranium",
    ];
    for material in materials {
        let file = json(
            &content()
                .join("data/mekanism/trim_material")
                .join(format!("{material}.json")),
        );
        let palette = file["palette_id"]
            .as_str()
            .unwrap()
            .strip_prefix("mekanism:")
            .unwrap();
        assert!(
            assets()
                .join("textures/palettes")
                .join(format!("{palette}.png"))
                .is_file(),
            "{material}: paleta"
        );
        assert!(
            lang[format!("trim_material.mekanism.{material}")].is_string(),
            "{material}: nome"
        );
    }
    let tag = json(&content().join("data/minecraft/tags/item/trim_materials.json"));
    let tagged: BTreeSet<&str> = tag["values"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    for (item, material) in [
        ("ingot_osmium", "osmium"),
        ("ingot_tin", "tin"),
        ("ingot_lead", "lead"),
        ("ingot_uranium", "uranium"),
        ("ingot_bronze", "bronze"),
        ("ingot_steel", "steel"),
        ("ingot_refined_obsidian", "refined_obsidian"),
        ("ingot_refined_glowstone", "refined_glowstone"),
        ("fluorite_gem", "fluorite"),
        ("pellet_antimatter", "antimatter"),
        ("pellet_plutonium", "plutonium"),
        ("pellet_polonium", "polonium"),
    ] {
        let declaration = json(
            &content()
                .join("data/mekanism/horizon/item")
                .join(format!("{item}.json")),
        );
        assert_eq!(
            declaration["components"]["minecraft:provides_trim_material"],
            format!("mekanism:{material}"),
            "{item}"
        );
        assert!(
            tagged.contains(format!("mekanism:{item}").as_str()),
            "{item} fora de #minecraft:trim_materials"
        );
    }
}

/// O texto de cada chave de `lang/en_us.json` é o da datagen do original (`rastros/craft_lang.json`), e cada item, bloco, material de acabamento e descrição da frente tem a sua.
#[test]
fn the_names_are_the_originals() {
    let original = rastro("craft_lang.json");
    let lang = json(&assets().join("lang/en_us.json"));
    let ours = lang.as_object().unwrap();
    assert!(ours.len() >= 240, "{}", ours.len());
    for (key, value) in ours {
        assert_eq!(original.get(key), Some(value), "{key}");
    }
    for key in original.as_object().unwrap().keys() {
        assert!(ours.contains_key(key), "falta {key}");
    }
    for id in ITEMS {
        assert!(
            ours.contains_key(&format!("item.mekanism.{id}")),
            "item.mekanism.{id}"
        );
    }
    for id in BLOCKS {
        assert!(
            ours.contains_key(&format!("block.mekanism.{id}")),
            "block.mekanism.{id}"
        );
    }
    for id in [
        "osmium_ore",
        "tin_ore",
        "uranium_ore",
        "lead_ore",
        "fluorite_ore",
        "teleporter_frame",
    ] {
        assert!(
            ours.contains_key(&format!("description.mekanism.{id}")),
            "description.mekanism.{id}"
        );
    }
    for (key, expected) in [
        ("item.mekanism.hdpe_pellet", "HDPE Pellet"),
        (
            "item.mekanism.ingot_refined_glowstone",
            "Refined Glowstone Ingot",
        ),
        ("block.mekanism.block_osmium", "Block of Osmium"),
        ("block.mekanism.block_salt", "Salt Block"),
        (
            "block.mekanism.deepslate_fluorite_ore",
            "Deepslate Fluorite Ore",
        ),
        ("trim_material.mekanism.steel", "Steel Material"),
        ("constants.mekanism.mod_name", "Mekanism"),
    ] {
        assert_eq!(lang[key], expected, "{key}");
    }
}
