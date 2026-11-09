//! Os itens de equipamento do original (`MekanismItems`: ferramentas, armaduras, leitores, instaladores de tier, o Robit), sem função: cada um existe com o que o original
//! dá a ele como conteúdo (registro, modelo, textura, nome, tags, receitas de mesa e posição nas abas), e o uso (clique, energia, tela, voo, módulos) fica de fora.
//! As propriedades saem do texto do Java (`dev/referencia/craft_rastros.py`, `rastros/equipment_items.json`), as tags e as receitas da datagen
//! (`craft_tags.json`, `craft_recipes.json`) e os arquivos copiados são conferidos por hash (`craft_datagen.json`, em `craft_content.rs`).
mod common;

use common::craft::{
    EQUIPMENT_ITEMS, assets, check_model, content, declared, digest, json, rastro,
};
use mekanism_rules::name_color;
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

fn declaration(id: &str) -> Value {
    json(
        &content()
            .join("data/mekanism/horizon/item")
            .join(format!("{id}.json")),
    )
}

#[test]
fn the_front_declares_exactly_its_items() {
    let java = rastro("equipment_items.json");
    let from_java: BTreeSet<&str> = java["items"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let listed: BTreeSet<&str> = EQUIPMENT_ITEMS.iter().copied().collect();
    assert_eq!(listed.len(), EQUIPMENT_ITEMS.len());
    assert_eq!(from_java, listed);
    let declared = declared("item");
    for id in EQUIPMENT_ITEMS {
        assert!(declared.contains(id), "{id} sem declaração em horizon/item");
    }
}

/// `ArmorMaterial.createAttributes` (`ArmorMaterial.java`): a armadura, a resistência (sempre) e o recuo (só se maior que zero), todos `add_value`, com o id
/// `minecraft:armor.<tipo>` e o grupo de slot do tipo.
fn attributes(kind: &str, slot: &str, defense: u64, toughness: f64, knockback: f64) -> Value {
    let id = format!("minecraft:armor.{kind}");
    let entry = |attribute: &str, amount: Value| json!({"type": attribute, "id": id, "amount": amount, "operation": "add_value", "slot": slot});
    let mut list = vec![
        entry("minecraft:armor", json!(defense)),
        entry("minecraft:armor_toughness", json!(toughness)),
    ];
    if knockback > 0.0 {
        list.push(entry("minecraft:knockback_resistance", json!(knockback)));
    }
    Value::Array(list)
}

/// Os atributos de armadura de cada item que os dá (`MekanismArmorMaterials`), com os números da configuração de inicialização e do `ArmorMaterials.NETHERITE` que o
/// gerador leu do Java. O recuo é um `float` (0,1F) que o `AttributeModifier` guarda como `double`.
fn expected_attributes(id: &str, slot: &str, armor: &Value) -> Value {
    let kind = armor_type(slot);
    match kind_of(id) {
        "mekaSuit" => {
            let netherite = &armor["netherite"];
            assert_eq!(armor["mekasuit_uses_netherite_defaults"], true);
            attributes(
                kind,
                slot,
                netherite[kind].as_u64().unwrap(),
                netherite["toughness"].as_f64().unwrap(),
                f64::from(netherite["knockback"].as_f64().unwrap() as f32),
            )
        }
        other => {
            let config = &armor[other];
            attributes(
                kind,
                slot,
                config["armor"].as_u64().unwrap(),
                config["toughness"].as_f64().unwrap(),
                f64::from(config["knockback"].as_f64().unwrap() as f32),
            )
        }
    }
}

fn kind_of(id: &str) -> &'static str {
    if id.starts_with("mekasuit_") {
        "mekaSuit"
    } else if id == "free_runners_armored" {
        "armored_free_runners"
    } else {
        "armored_jetpack"
    }
}

/// O tipo de armadura (`ArmorType`) de cada slot do `Equippable`.
fn armor_type(slot: &str) -> &'static str {
    match slot {
        "head" => "helmet",
        "chest" => "chestplate",
        "legs" => "leggings",
        _ => "boots",
    }
}

/// O que `Item.Properties` guarda no original para o item, em componentes do Horizon: tudo o que é só dado. O que é uso (modo, energia, tanque, ferramenta, arma, módulos,
/// descrição) não entra.
fn expected_components(id: &str, java: &Value, armor: &Value) -> Map<String, Value> {
    let mut components = Map::new();
    if java["fire_resistant"] == true {
        components.insert(
            "minecraft:damage_resistant".into(),
            json!({"types": "#minecraft:is_fire"}),
        );
    }
    if let Some(value) = java["enchantable"].as_u64() {
        components.insert("minecraft:enchantable".into(), json!({"value": value}));
    }
    if let Some(seconds) = java["use_cooldown"].as_f64() {
        components.insert("minecraft:use_cooldown".into(), json!({"seconds": seconds}));
    }
    if let Some(durability) = java["durability"].as_u64() {
        components.insert("minecraft:max_damage".into(), json!(durability));
    }
    if let Some(slot) = java["slot"].as_str() {
        let mut equippable = json!({"slot": slot});
        if let Some(sound) = java["equip_sound"].as_str() {
            // `SoundEvents.ARMOR_EQUIP_ELYTRA` é `item.armor.equip_elytra`.
            equippable["equip_sound"] = json!(format!(
                "minecraft:{}",
                sound.replace("armor_equip_", "item.armor.equip_")
            ));
        }
        if let Some(asset) = java["asset"].as_str() {
            equippable["asset_id"] = json!(format!("mekanism:{asset}"));
        }
        if java["damage_on_hurt"] == false {
            equippable["damage_on_hurt"] = json!(false);
        }
        components.insert("minecraft:equippable".into(), equippable);
        if java["attributes"].is_string() {
            components.insert(
                "minecraft:attribute_modifiers".into(),
                expected_attributes(id, slot, armor),
            );
        }
    }
    if let Some(visibility) = java["mob_visibility"].as_f64() {
        components.insert(
            "minecraft:mob_visibility".into(),
            json!({"targeting_entity_types": "#mekanism:mekasuit_reduced_visibility", "visibility": visibility}),
        );
    }
    if let Some(repair) = java["repairable"].as_str() {
        components.insert(
            "minecraft:repairable".into(),
            json!({"items": format!("mekanism:{repair}")}),
        );
    }
    if let Some(rgb) = name_color(&format!("mekanism:{id}")) {
        components.insert(
            "minecraft:item_name".into(),
            json!({"translate": format!("item.mekanism.{id}"), "color": format!("#{rgb:06X}")}),
        );
    }
    components
}

#[test]
fn every_item_has_the_registration_of_the_original() {
    let java = rastro("equipment_items.json");
    let armor = &java["armor"];
    for id in EQUIPMENT_ITEMS {
        let item = &java["items"][id];
        let declared = declaration(id);
        // `hdpe_elytra` herda do élitro do vanilla o planador, o som, o slot e a durabilidade, que o original dá no registro; o resto parte de um item liso.
        let glider = item["glider"] == true;
        assert_eq!(
            declared["template"],
            if glider {
                "minecraft:elytra"
            } else {
                "minecraft:iron_nugget"
            },
            "{id}: gabarito"
        );
        assert_eq!(
            declared["rarity"].as_str(),
            item["rarity"].as_str(),
            "{id}: raridade"
        );
        assert_eq!(
            declared["max_stack_size"].as_u64(),
            item["stack"].as_u64(),
            "{id}: pilha"
        );
        let expected = expected_components(id, item, armor);
        let have = declared["components"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        assert_eq!(have, expected, "{id}: componentes");
        assert!(
            declared.as_object().unwrap().keys().all(|k| [
                "template",
                "rarity",
                "max_stack_size",
                "components"
            ]
            .contains(&k.as_str())),
            "{id}: chave fora da tabela"
        );
    }
}

/// O que cada classe de armadura e de ferramenta dá, conferido item a item contra o texto do Java: as raridades, as pilhas e os slots que o plano promete.
#[test]
fn the_java_reading_finds_what_the_original_gives() {
    let java = rastro("equipment_items.json");
    let item = |id: &str| &java["items"][id];
    // Nenhum item parou de ser lido: um campo vazio onde o original dá valor engana o resto do teste.
    assert_eq!(item("mekasuit_helmet")["rarity"], "epic");
    assert_eq!(item("mekasuit_helmet")["fire_resistant"], true);
    assert_eq!(item("mekasuit_helmet")["slot"], "head");
    assert_eq!(item("mekasuit_helmet")["mob_visibility"], 0.9);
    assert_eq!(item("meka_tool")["fire_resistant"], true);
    assert_eq!(item("hazmat_gown")["asset"], "hazmat");
    assert_eq!(item("hazmat_gown")["slot"], "chest");
    assert_eq!(item("jetpack")["stack"], 1);
    assert_eq!(item("jetpack")["rarity"], "rare");
    assert_eq!(item("scuba_mask")["enchantable"], 9);
    assert_eq!(item("scuba_mask")["asset"], Value::Null);
    assert_eq!(item("electric_bow")["enchantable"], 1);
    assert_eq!(item("geiger_counter")["use_cooldown"], 0.5);
    assert_eq!(item("hdpe_elytra")["durability"], 648);
    assert_eq!(item("hdpe_elytra")["damage_on_hurt"], false);
    assert_eq!(
        item("free_runners_armored")["asset"],
        "armored_free_runners"
    );
    assert_eq!(item("crafting_formula")["stack"], Value::Null);
    assert_eq!(java["armor"]["netherite"]["chestplate"], 8);
    assert_eq!(java["armor"]["armored_jetpack"]["armor"], 8);
}

#[test]
fn armor_attributes_are_the_ones_the_original_computes() {
    let java = rastro("equipment_items.json");
    let helmet = declaration("mekasuit_helmet");
    let list = helmet["components"]["minecraft:attribute_modifiers"]
        .as_array()
        .unwrap();
    assert_eq!(list.len(), 3);
    assert_eq!(list[0]["amount"], 3);
    assert_eq!(list[1]["amount"], 3.0);
    assert_eq!(list[2]["type"], "minecraft:knockback_resistance");
    // 0,1F como `double`: o valor que o `AttributeModifier` guarda, não 0,1.
    assert_eq!(list[2]["amount"], 0.100_000_001_490_116_12_f64);
    assert_ne!(list[2]["amount"], 0.1);
    assert_eq!(list[2]["slot"], "head");
    // O recuo zero das botas e do jetpack blindados não vira modificador.
    for id in ["free_runners_armored", "jetpack_armored"] {
        let list = declaration(id)["components"]["minecraft:attribute_modifiers"].clone();
        assert_eq!(list.as_array().unwrap().len(), 2, "{id}");
    }
    let boots = declaration("free_runners_armored");
    assert_eq!(
        boots["components"]["minecraft:attribute_modifiers"][0]["id"],
        "minecraft:armor.boots"
    );
    assert_eq!(
        boots["components"]["minecraft:attribute_modifiers"][0]["amount"],
        3
    );
    assert_eq!(java["armor"]["armored_free_runners"]["armor"], 3);
}

fn model_of_item(id: &str) -> Vec<String> {
    fn collect(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    if (key == "model" || key == "base")
                        && let Some(reference) = child.as_str()
                    {
                        out.push(reference.to_owned());
                    }
                    collect(child, out);
                }
            }
            Value::Array(list) => list.iter().for_each(|child| collect(child, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    collect(
        &json(&assets().join("items").join(format!("{id}.json"))),
        &mut out,
    );
    assert!(!out.is_empty(), "{id}: item sem modelo");
    out
}

#[test]
fn every_item_has_its_models_textures_and_name() {
    let lang = json(&assets().join("lang/en_us.json"));
    let original = rastro("craft_lang.json");
    let mut seen = BTreeSet::new();
    for id in EQUIPMENT_ITEMS {
        for model in model_of_item(id) {
            check_model(&model, &mut seen);
        }
        let key = format!("item.mekanism.{id}");
        assert!(lang[&key].is_string(), "falta o nome de {id}");
        assert_eq!(lang[&key], original[&key], "{key}");
    }
    // Os modelos que o item escolhe por estado (modos, carga, arco puxado) estão todos aqui.
    for model in [
        "item/geiger_counter_5",
        "item/electric_bow_pulling_2",
        "item/configurator_wrench",
        "item/crafting_formula_invalid",
        "item/configuration_card_encoded",
        "item/hdpe_elytra_broken",
        "item/meka_tool_left",
        "item/free_runners_armored",
        "item/jetpack_armored",
    ] {
        assert!(seen.contains(model), "{model}");
    }
    assert!(seen.len() >= 55, "{}", seen.len());
    // Os dois textos de descrição que o original põe nos itens (`DESCRIPTION_DICTIONARY`, `DESCRIPTION_SEISMIC_READER`).
    for key in [
        "description.mekanism.dictionary",
        "description.mekanism.seismic_reader",
    ] {
        assert_eq!(lang[key], original[key], "{key}");
    }
}

/// A armadura vestida: o ativo de equipamento do Mekanism (`MekanismEquipmentAssets`) que tem arquivo (o traje de hazmat e o élitro) traz as camadas e as texturas dele; os
/// outros (`free_runners`, `jetpack`, `mekasuit`, `scuba_gear`) existem só como marcadores, sem arquivo, porque o original os desenha por código.
#[test]
fn worn_equipment_assets_have_their_layers_and_textures() {
    for (asset, layers) in [
        (
            "hazmat",
            &["humanoid", "humanoid_baby", "humanoid_leggings"][..],
        ),
        ("hdpe_elytra", &["wings"][..]),
    ] {
        let file = json(&assets().join("equipment").join(format!("{asset}.json")));
        let declared: Vec<&String> = file["layers"].as_object().unwrap().keys().collect();
        assert_eq!(declared.len(), layers.len(), "{asset}");
        for layer in layers {
            let texture = file["layers"][layer][0]["texture"].as_str().unwrap();
            assert_eq!(texture, format!("mekanism:{asset}"));
            assert!(
                assets()
                    .join("textures/entity/equipment")
                    .join(layer)
                    .join(format!("{asset}.png"))
                    .is_file(),
                "{asset}/{layer}"
            );
        }
    }
    for marker in ["free_runners", "jetpack", "mekasuit", "scuba_gear"] {
        assert!(
            !assets()
                .join("equipment")
                .join(format!("{marker}.json"))
                .exists(),
            "{marker} é só marcador"
        );
    }
    let manifest = rastro("craft_datagen.json");
    for relative in [
        "assets/mekanism/equipment/hazmat.json",
        "assets/mekanism/equipment/hdpe_elytra.json",
        "assets/mekanism/textures/entity/equipment/wings/hdpe_elytra.png",
        "assets/mekanism/textures/entity/equipment/humanoid/hazmat.png",
        "assets/mekanism/models/entity/mekatool_right.obj",
        "data/mekanism/tags/entity_type/mekasuit_reduced_visibility.json",
    ] {
        assert!(
            manifest.get(relative).is_some(),
            "{relative} fora do rastro"
        );
        assert_eq!(
            digest(&content().join(relative)),
            manifest[relative].as_str().unwrap(),
            "{relative}"
        );
    }
}

/// Os arquivos de cada item (definição, modelos que ele cita) são cópias do original: estão no rastro de hashes.
#[test]
fn every_item_file_is_in_the_hash_trace() {
    let manifest = rastro("craft_datagen.json");
    let mut seen = BTreeSet::new();
    for id in EQUIPMENT_ITEMS {
        let relative = format!("assets/mekanism/items/{id}.json");
        assert!(
            manifest.get(&relative).is_some(),
            "{relative} fora do rastro"
        );
        for model in model_of_item(id) {
            check_model(&model, &mut seen);
        }
    }
    for path in &seen {
        let relative = format!("assets/mekanism/models/{path}.json");
        assert!(
            manifest.get(&relative).is_some(),
            "{relative} fora do rastro"
        );
    }
}

/// As tags da datagen que citam os itens: as vanilla de armadura (`head_armor`...), as de ferramenta (`c:tools/*`) e as de contêiner de módulo. As do vanilla de dano e de
/// entidade (o `flamethrower`, o Robit) citam o que não é item e esperam o tipo de dano e a entidade.
#[test]
fn the_tags_list_the_equipment_the_original_lists() {
    let tags = rastro("craft_tags.json");
    let in_tag = |name: &str| -> Vec<String> {
        tags[name]
            .as_array()
            .unwrap_or_else(|| panic!("a tag {name} não está no rastro"))
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect()
    };
    for (tag, expected) in [
        (
            "minecraft/tags/item/head_armor.json",
            &["hazmat_mask", "mekasuit_helmet"][..],
        ),
        (
            "minecraft/tags/item/chest_armor.json",
            &["hazmat_gown", "mekasuit_bodyarmor"][..],
        ),
        (
            "minecraft/tags/item/leg_armor.json",
            &["hazmat_pants", "mekasuit_pants"][..],
        ),
        (
            "minecraft/tags/item/foot_armor.json",
            &["hazmat_boots", "mekasuit_boots"][..],
        ),
        ("c/tags/item/tools/wrench.json", &["configurator"][..]),
        (
            "c/tags/item/tools/mining_tool.json",
            &["atomic_disassembler", "meka_tool"][..],
        ),
        ("c/tags/item/tools/bow.json", &["electric_bow"][..]),
        (
            "minecraft/tags/item/enchantable/durability.json",
            &["hdpe_elytra"][..],
        ),
    ] {
        let listed: Vec<String> = in_tag(tag)
            .into_iter()
            .filter(|id| id.starts_with("mekanism:"))
            .collect();
        let expected: Vec<String> = expected.iter().map(|id| format!("mekanism:{id}")).collect();
        assert_eq!(listed, expected, "{tag}");
        let ours = json(&content().join("data").join(tag));
        let ours: Vec<String> = ours["values"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        assert_eq!(ours, in_tag(tag), "{tag}: o arquivo do port");
    }
    // Cada tag de item que cita um item de equipamento e só o que o port tem está no port, e nenhuma cita o que ele não tem.
    let mut present = 0;
    for (tag, values) in tags.as_object().unwrap() {
        if !tag.contains("/tags/item/") {
            continue;
        }
        let cites_equipment = values.as_array().unwrap().iter().any(|v| {
            v.as_str()
                .and_then(|id| id.strip_prefix("mekanism:"))
                .is_some_and(|id| EQUIPMENT_ITEMS.contains(&id))
        });
        if cites_equipment {
            assert!(content().join("data").join(tag).is_file(), "falta {tag}");
            present += 1;
        }
    }
    assert!(present >= 20, "{present}");
}

/// As tags `c:` do NeoForge que as receitas do traje de hazmat e do conta-gotas citam e que o Horizon não traz: o arquivo é o do NeoForge.
#[test]
fn the_neoforge_tags_the_recipes_need_are_the_neoforge_ones() {
    let neoforge = rastro("craft_neoforge_tags.json");
    let neoforge = neoforge.as_object().unwrap();
    // As quatro do traje de hazmat e do conta-gotas, as cinco das receitas de mesa dos blocos de máquina e as três das barras (disperso de pressão e transportadores especiais).
    assert_eq!(neoforge.len(), 12);
    for (relative, values) in neoforge {
        let ours = json(&content().join("data").join(relative));
        assert_eq!(&ours["values"], values, "{relative}");
    }
    for id in [
        "hazmat_mask",
        "hazmat_gown",
        "hazmat_pants",
        "hazmat_boots",
        "gauge_dropper",
        "pressure_disperser",
        "transmitter/restrictive_transporter",
        "transmitter/diversion_transporter",
    ] {
        assert!(
            content()
                .join("data/mekanism/recipe")
                .join(format!("{id}.json"))
                .is_file(),
            "{id}"
        );
    }
}

/// As receitas que dão um item de equipamento. As de mesa estão no port quando tudo o que citam existe (`craft_data.rs` confere a regra para todas), e as de tipo `mekanism:mek_data`
/// (a receita que copia os dados do item, como a energia e os módulos) esperam o tipo.
#[test]
fn the_recipes_of_the_equipment_are_in_or_wait_for_a_known_cause() {
    let java = rastro("equipment_items.json");
    let recipes = java["recipes"].as_object().unwrap();
    assert_eq!(recipes.len(), 37);
    let mut absent_table = BTreeSet::new();
    let mut mek_data = BTreeSet::new();
    for (relative, kind) in recipes {
        let present = content().join("data").join(relative).is_file();
        match kind.as_str().unwrap() {
            "mekanism:mek_data" => {
                assert!(!present, "{relative}");
                mek_data.insert(relative.as_str());
            }
            "minecraft:crafting_shaped" | "minecraft:crafting_shapeless" => {
                if !present {
                    absent_table.insert(relative.as_str());
                }
            }
            other => panic!("{relative}: tipo {other}"),
        }
    }
    assert_eq!(
        mek_data,
        BTreeSet::from([
            "mekanism/recipe/atomic_disassembler.json",
            "mekanism/recipe/electric_bow.json",
            "mekanism/recipe/flamethrower.json",
            "mekanism/recipe/free_runners.json",
            "mekanism/recipe/free_runners_armored.json",
            "mekanism/recipe/jetpack.json",
            "mekanism/recipe/jetpack_armored.json",
            "mekanism/recipe/network_reader.json",
            "mekanism/recipe/portable_teleporter.json",
            "mekanism/recipe/robit.json",
            "mekanism/recipe/scuba_tank.json",
            "mekanism/recipe/seismic_reader.json",
        ])
    );
    // As cinco que esperavam o `basic_induction_cell` (o Meka-Tool e as quatro peças do MekaSuit) entraram com a frente `blocos-estrutura`: toda receita de mesa está no port.
    assert_eq!(absent_table, BTreeSet::new());
}
