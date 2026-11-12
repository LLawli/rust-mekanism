//! O registro dos itens e blocos de craft e de recurso: as propriedades que o original dá (`MekanismItems`, `MekanismBlocks.registerResourceBlock` e `registerOre`,
//! `BlockResourceInfo`, `MiscResource`, `PrimaryResource`, `OreType`) contra o que as declarações `horizon/item` e `horizon/block` dão com o gabarito do vanilla
//! (`rastros/horizon_template_facts.json`: dureza, resistência, cor, instrumento do bloco de nota e reação a pistão que o gabarito traz).
mod common;

use common::craft::{EQUIPMENT_ITEMS, content, declared, json, rastro};
use mekanism_rules::{name_color, ore_experience};
use serde_json::{Value, json};

/// O `J` de `mod_content.rs` do Horizon (JSON com as chaves na ordem do arquivo), só com o que a injeção `note_instrument` toca.
#[derive(Clone, Debug, PartialEq)]
enum J {
    Null,
    Num(i64),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

/// O `Rename` de `mod_content.rs`, só com o id de destino.
struct Rename {
    to: String,
}

/// O helper da injeção, como mora em `mod_content.rs`.
mod note_instrument {
    use super::{J, Rename};
    include!("../native/note_instrument.rs");
    pub fn run(row: &mut J, to: &str) {
        let renamed = Rename { to: to.to_owned() };
        mekanism_note_instrument((), row, &renamed);
    }
}

/// Um bloco do original: dureza, resistência, exige ferramenta, `MapColor` (id), luz, instrumento do bloco de nota, reação a pistão e som de quebra.
struct Block {
    id: &'static str,
    hardness: f64,
    resistance: f64,
    tool: bool,
    map_color: u64,
    light: u64,
    instrument: &'static str,
    push: &'static str,
    sound: &'static str,
}

#[allow(clippy::too_many_arguments)]
const fn block(
    id: &'static str,
    hardness: f64,
    resistance: f64,
    tool: bool,
    map_color: u64,
    light: u64,
    instrument: &'static str,
    push: &'static str,
    sound: &'static str,
) -> Block {
    Block {
        id,
        hardness,
        resistance,
        tool,
        map_color,
        light,
        instrument,
        push,
        sound,
    }
}

const STONE: &str = "minecraft:block.stone.break";
const DEEPSLATE: &str = "minecraft:block.deepslate.break";

/// `BlockResourceInfo` (`BlockResourceInfo.java:19-32`, `modifyProperties`: `requiresCorrectToolForDrops`, `strength(hardness, resistance)`, `mapColor`, `instrument`,
/// `lightLevel`, `pushReaction`), `registerOre` (`MekanismBlocks.java:1181-1194`), `SALT_BLOCK` e `BIO_FUEL_BLOCK` (`:980-982`), `TELEPORTER_FRAME` e `STEEL_CASING`
/// (`:244-245`, com a luz 3 do `BlockTypeBuilder.withLight(3)` do quadro). `MapColor` pelo id do construtor (`MapColor.java`).
const BLOCKS: [Block; 28] = [
    block(
        "block_osmium",
        7.5,
        12.0,
        true,
        23,
        0,
        "HARP",
        "PUSH_PULL",
        STONE,
    ),
    block(
        "block_raw_osmium",
        7.5,
        12.0,
        true,
        23,
        0,
        "BASEDRUM",
        "PUSH_PULL",
        STONE,
    ),
    block(
        "block_tin",
        5.0,
        6.0,
        true,
        36,
        0,
        "HARP",
        "PUSH_PULL",
        STONE,
    ),
    block(
        "block_raw_tin",
        5.0,
        6.0,
        true,
        36,
        0,
        "BASEDRUM",
        "PUSH_PULL",
        STONE,
    ),
    block(
        "block_lead",
        5.0,
        9.0,
        true,
        22,
        0,
        "HARP",
        "PUSH_PULL",
        STONE,
    ),
    block(
        "block_raw_lead",
        5.0,
        9.0,
        true,
        22,
        0,
        "BASEDRUM",
        "PUSH_PULL",
        STONE,
    ),
    block(
        "block_uranium",
        5.0,
        9.0,
        true,
        1,
        0,
        "HARP",
        "PUSH_PULL",
        STONE,
    ),
    block(
        "block_raw_uranium",
        5.0,
        9.0,
        true,
        1,
        0,
        "BASEDRUM",
        "PUSH_PULL",
        STONE,
    ),
    block(
        "block_charcoal",
        5.0,
        6.0,
        true,
        29,
        0,
        "BASEDRUM",
        "PUSH_PULL",
        STONE,
    ),
    block(
        "block_fluorite",
        5.0,
        9.0,
        true,
        8,
        0,
        "HARP",
        "PUSH_PULL",
        STONE,
    ),
    block(
        "block_bronze",
        5.0,
        9.0,
        true,
        15,
        0,
        "HARP",
        "PUSH_PULL",
        STONE,
    ),
    block(
        "block_steel",
        5.0,
        9.0,
        true,
        59,
        0,
        "HARP",
        "PUSH_PULL",
        STONE,
    ),
    block(
        "block_refined_obsidian",
        50.0,
        2400.0,
        true,
        24,
        8,
        "BASEDRUM",
        "IMMOVEABLE",
        STONE,
    ),
    block(
        "block_refined_glowstone",
        5.0,
        6.0,
        true,
        18,
        15,
        "BASEDRUM",
        "PUSH_PULL",
        STONE,
    ),
    block(
        "block_salt",
        0.5,
        0.5,
        false,
        0,
        0,
        "SNARE",
        "PUSH_PULL",
        "minecraft:block.sand.break",
    ),
    block(
        "block_bio_fuel",
        0.5,
        0.5,
        false,
        26,
        0,
        "BANJO",
        "PUSH_PULL",
        "minecraft:block.grass.break",
    ),
    block(
        "teleporter_frame",
        5.0,
        6.0,
        true,
        59,
        3,
        "HARP",
        "PUSH_PULL",
        STONE,
    ),
    block(
        "steel_casing",
        3.5,
        9.0,
        true,
        59,
        0,
        "HARP",
        "PUSH_PULL",
        STONE,
    ),
    block(
        "tin_ore",
        3.0,
        3.0,
        true,
        11,
        0,
        "BASEDRUM",
        "PUSH_PULL",
        STONE,
    ),
    block(
        "deepslate_tin_ore",
        4.5,
        3.0,
        true,
        59,
        0,
        "BASEDRUM",
        "PUSH_PULL",
        DEEPSLATE,
    ),
    block(
        "osmium_ore",
        3.0,
        3.0,
        true,
        11,
        0,
        "BASEDRUM",
        "PUSH_PULL",
        STONE,
    ),
    block(
        "deepslate_osmium_ore",
        4.5,
        3.0,
        true,
        59,
        0,
        "BASEDRUM",
        "PUSH_PULL",
        DEEPSLATE,
    ),
    block(
        "uranium_ore",
        3.0,
        3.0,
        true,
        11,
        0,
        "BASEDRUM",
        "PUSH_PULL",
        STONE,
    ),
    block(
        "deepslate_uranium_ore",
        4.5,
        3.0,
        true,
        59,
        0,
        "BASEDRUM",
        "PUSH_PULL",
        DEEPSLATE,
    ),
    block(
        "fluorite_ore",
        3.0,
        3.0,
        true,
        11,
        0,
        "BASEDRUM",
        "PUSH_PULL",
        STONE,
    ),
    block(
        "deepslate_fluorite_ore",
        4.5,
        3.0,
        true,
        59,
        0,
        "BASEDRUM",
        "PUSH_PULL",
        DEEPSLATE,
    ),
    block(
        "lead_ore",
        3.0,
        3.0,
        true,
        11,
        0,
        "BASEDRUM",
        "PUSH_PULL",
        STONE,
    ),
    block(
        "deepslate_lead_ore",
        4.5,
        3.0,
        true,
        59,
        0,
        "BASEDRUM",
        "PUSH_PULL",
        DEEPSLATE,
    ),
];

/// O que o gabarito não dá: o instrumento do bloco de nota (`BlockProperties` não tem `instrument` e nenhum gabarito de bloco liso do vanilla tem o `SNARE` ou o `BANJO` sem outro
/// efeito: areia cai, o feno tem o estado `axis`). O gabarito `lapis_block` dá `HARP`, e a injeção `note_instrument` (`native/note_instrument.rs`, H44) troca na linha do estado
/// o `HARP` pelo instrumento do original; o teste `the_note_instrument_injection_gives_the_original_instrument` confere. O teste abaixo exige que as diferenças do gabarito sejam
/// exatamente estes dois blocos.
const INSTRUMENT_DIFFERENCES: [(&str, &str, &str); 2] = [
    ("block_salt", "SNARE", "HARP"),
    ("block_bio_fuel", "BANJO", "HARP"),
];

/// O mapa de nomes de bloco do vanilla (`properties.sounds`) para o som de quebra dele.
fn sound_of_block(name: &str) -> &str {
    match name {
        "minecraft:sand" => "minecraft:block.sand.break",
        "minecraft:grass_block" => "minecraft:block.grass.break",
        other => panic!("sons de {other} não conferidos"),
    }
}

#[test]
fn every_block_has_the_properties_of_the_original() {
    let facts = rastro("horizon_template_facts.json");
    let mut instrument_differences = Vec::new();
    for spec in &BLOCKS {
        let declaration = json(
            &content()
                .join("data/mekanism/horizon/block")
                .join(format!("{}.json", spec.id)),
        );
        let template = declaration["template"].as_str().unwrap();
        let base = &facts[template];
        assert!(
            base.is_object(),
            "{}: gabarito {template} fora do rastro",
            spec.id
        );
        let properties = &declaration["properties"];
        let number = |key: &str| {
            properties[key]
                .as_f64()
                .unwrap_or_else(|| base[key].as_f64().unwrap())
        };
        assert_eq!(number("hardness"), spec.hardness, "{}: dureza", spec.id);
        assert_eq!(
            number("explosion_resistance"),
            spec.resistance,
            "{}: resistência",
            spec.id
        );
        assert_eq!(
            properties["requires_tool"]
                .as_bool()
                .unwrap_or_else(|| base["requires_tool"].as_bool().unwrap()),
            spec.tool,
            "{}: ferramenta",
            spec.id
        );
        assert_eq!(
            properties["map_color"].as_u64(),
            Some(spec.map_color),
            "{}: cor do mapa",
            spec.id
        );
        assert_eq!(
            properties["light"]
                .as_u64()
                .unwrap_or_else(|| base["emission"].as_u64().unwrap()),
            spec.light,
            "{}: luz",
            spec.id
        );
        assert_eq!(
            base["push"], spec.push,
            "{}: reação a pistão do gabarito",
            spec.id
        );
        let sound = properties["sounds"].as_str().map_or_else(
            || base["sound"].as_str().unwrap(),
            |name| sound_of_block(name),
        );
        assert_eq!(sound, spec.sound, "{}: som", spec.id);
        let instrument = base["instrument"].as_str().unwrap();
        if instrument != spec.instrument {
            instrument_differences.push((spec.id, spec.instrument, instrument));
        }
        // O bloco de recurso e o minério têm um estado só, e o item do bloco sai do gabarito.
        assert!(declaration.get("state_properties").is_none(), "{}", spec.id);
    }
    assert_eq!(instrument_differences, INSTRUMENT_DIFFERENCES);
    // Todo bloco da frente tem declaração própria (nenhum fora da tabela).
    let table: Vec<&str> = BLOCKS.iter().map(|b| b.id).collect();
    for id in declared("block") {
        if id.ends_with("_ore")
            || id.starts_with("block_")
            || id == "steel_casing"
            || id == "teleporter_frame"
        {
            assert!(
                table.contains(&id.as_str()),
                "bloco {id} fora da tabela do original"
            );
        }
    }
}

/// O componente de combustível do original: `cookingFuel(provider)`, com o provider escrito por extenso (o Horizon só lê os de `data/minecraft/context_int_provider`).
fn fuel(left: u64) -> Value {
    json!({
        "burn_time": {
            "type": "minecraft:div", "left": left,
            "right": {"type": "minecraft:conditional", "condition": "minecraft:block/fast_cooking",
                      "on_false": "minecraft:cooking/normal_burn_time_reduction_factor", "on_true": "minecraft:cooking/fast_burn_time_reduction_factor"}
        },
        "speed_multiplier": "minecraft:cooking/speed_default"
    })
}

#[test]
fn the_block_items_carry_what_the_original_gives_them() {
    let read = |id: &str| {
        json(
            &content()
                .join("data/mekanism/horizon/block")
                .join(format!("{id}.json")),
        )
    };
    // `BlockResourceInfo.modifyProperties(Item.Properties)`: refined obsidian is fire resistant, charcoal burns 16.000 ticks (`COOKING_TIME_CHARCOAL_BLOCK`).
    let obsidian = read("block_refined_obsidian");
    assert_eq!(
        obsidian["components"]["minecraft:damage_resistant"],
        json!({"types": "#minecraft:is_fire"})
    );
    assert_eq!(
        read("block_charcoal")["components"]["minecraft:cooking_fuel"],
        fuel(16_000)
    );
    // `BIO_FUEL_BLOCK` (`MekanismBlocks.java:981-982`): `COOKING_TIME_BIO_FUEL_BLOCK`, dez vezes o do item.
    assert_eq!(
        read("block_bio_fuel")["components"]["minecraft:cooking_fuel"],
        fuel(1_000)
    );
    for id in [
        "block_osmium",
        "block_tin",
        "block_salt",
        "tin_ore",
        "teleporter_frame",
    ] {
        assert!(read(id).get("components").is_none(), "{id}");
    }
}

/// Um item do original: raridade, pilha máxima, resistência ao fogo, material de acabamento, combustível e cor do nome.
struct Item {
    rarity: Option<&'static str>,
    stack: Option<u64>,
    fire: bool,
    trim: Option<&'static str>,
    fuel: Option<u64>,
    color: Option<&'static str>,
}

fn expected_item(id: &str) -> Item {
    // `MekanismItems.java`: raridade (`register(name, Rarity)`), `registerAlloy` e as cores de `EnumColor` e de `BaseTier` (`ITEMS.register(name, color)`).
    let rarity = match id {
        "alloy_infused"
        | "hdpe_pellet"
        | "hdpe_rod"
        | "hdpe_sheet"
        | "hdpe_stick"
        | "electrolytic_core"
        | "yellow_cake_uranium" => Some("uncommon"),
        "alloy_reinforced" | "teleportation_core" | "reprocessed_fissile_fragment" => Some("rare"),
        "alloy_atomic" => Some("epic"),
        _ => None,
    };
    // `registerQIODrive`: `stacksTo(1)`.
    let stack = id.starts_with("qio_drive_").then_some(1);
    // `MiscResource.REFINED_OBSIDIAN` e `dirty_netherite_scrap` (`Properties::fireResistant`).
    let fire = id.contains("refined_obsidian") || id == "dirty_netherite_scrap";
    // `PrimaryResource.modifyProperties` e `MiscResource.modifyProperties`: só o lingote (e a gema de fluorita, `modifyProperties(properties, null)`) e os pellets.
    let trim = match id {
        "ingot_osmium" => Some("osmium"),
        "ingot_tin" => Some("tin"),
        "ingot_lead" => Some("lead"),
        "ingot_uranium" => Some("uranium"),
        "ingot_bronze" => Some("bronze"),
        "ingot_steel" => Some("steel"),
        "ingot_refined_obsidian" => Some("refined_obsidian"),
        "ingot_refined_glowstone" => Some("refined_glowstone"),
        "fluorite_gem" => Some("fluorite"),
        "pellet_antimatter" => Some("antimatter"),
        "pellet_plutonium" => Some("plutonium"),
        "pellet_polonium" => Some("polonium"),
        _ => None,
    };
    let fuel = (id == "bio_fuel").then_some(100);
    let color = match id {
        "basic_control_circuit" | "qio_drive_base" => Some("#5FFFB8"),
        "advanced_control_circuit" | "qio_drive_hyper_dense" => Some("#FF806A"),
        "elite_control_circuit" | "qio_drive_time_dilating" => Some("#4BF8FF"),
        "ultimate_control_circuit" | "qio_drive_supermassive" => Some("#F787FF"),
        "pellet_antimatter" => Some("#A460D9"),
        "pellet_plutonium" => Some("#CFCFCF"),
        "pellet_polonium" => Some("#559EFF"),
        _ => None,
    };
    Item {
        rarity,
        stack,
        fire,
        trim,
        fuel,
        color,
    }
}

#[test]
fn every_item_has_the_properties_of_the_original() {
    let mut checked = 0;
    for id in declared("item") {
        // Os baldes são da frente `fluidos` (`fluids.rs`).
        if id == "upgrade" || EQUIPMENT_ITEMS.contains(&id.as_str()) || id.ends_with("_bucket") {
            continue;
        }
        let declaration = json(
            &content()
                .join("data/mekanism/horizon/item")
                .join(format!("{id}.json")),
        );
        let expected = expected_item(&id);
        // O gabarito é `minecraft:iron_nugget`: sem material de acabamento, combustível nem resistência (o `iron_ingot` trazia `provides_trim_material`).
        assert_eq!(declaration["template"], "minecraft:iron_nugget", "{id}");
        assert_eq!(
            declaration["rarity"].as_str(),
            expected.rarity,
            "{id}: raridade"
        );
        assert_eq!(
            declaration["max_stack_size"].as_u64(),
            expected.stack,
            "{id}: pilha"
        );
        let components = &declaration["components"];
        assert_eq!(
            components["minecraft:damage_resistant"].is_object(),
            expected.fire,
            "{id}: fogo"
        );
        if expected.fire {
            assert_eq!(
                components["minecraft:damage_resistant"],
                json!({"types": "#minecraft:is_fire"}),
                "{id}"
            );
        }
        assert_eq!(
            components["minecraft:provides_trim_material"]
                .as_str()
                .map(str::to_owned),
            expected.trim.map(|material| format!("mekanism:{material}")),
            "{id}: material de acabamento"
        );
        assert_eq!(
            components["minecraft:cooking_fuel"],
            expected.fuel.map_or(Value::Null, fuel),
            "{id}: combustível"
        );
        let name = &components["minecraft:item_name"];
        // A cor do tooltip (`name_color`, a regra do hook) é a do componente do item, e nenhum outro item a tem.
        let from_rule = name_color(&format!("mekanism:{id}")).map(|rgb| format!("#{rgb:06X}"));
        assert_eq!(from_rule.as_deref(), expected.color, "{id}: cor do tooltip");
        match expected.color {
            Some(color) => assert_eq!(
                *name,
                json!({"translate": format!("item.mekanism.{id}"), "color": color}),
                "{id}: nome"
            ),
            None => assert!(name.is_null(), "{id}: nome com cor"),
        }
        let known = [
            "minecraft:damage_resistant",
            "minecraft:provides_trim_material",
            "minecraft:cooking_fuel",
            "minecraft:item_name",
        ];
        for key in components.as_object().into_iter().flat_map(|c| c.keys()) {
            assert!(
                known.contains(&key.as_str()),
                "{id}: componente {key} fora da tabela"
            );
        }
        checked += 1;
    }
    assert!(checked >= 106, "{checked}");
}

/// `OreType.FLUORITE` dá de 1 a 4 de experiência (`BlockOre.getExpDrop`); os outros minérios, nada. `Mth.nextInt` sorteia `nextInt(max - min + 1) + min`.
#[test]
fn only_fluorite_ore_gives_experience_and_it_draws_one_to_four() {
    for block in ["mekanism:fluorite_ore", "mekanism:deepslate_fluorite_ore"] {
        let mut asked = Vec::new();
        for roll in 0..4 {
            let xp = ore_experience(block, |bound| {
                asked.push(bound);
                roll
            });
            assert_eq!(xp, Some(1 + roll as u32), "{block}");
        }
        assert_eq!(asked, [4, 4, 4, 4], "o sorteio é nextInt(4)");
    }
    for block in [
        "mekanism:tin_ore",
        "mekanism:osmium_ore",
        "mekanism:uranium_ore",
        "mekanism:lead_ore",
        "mekanism:deepslate_lead_ore",
        "minecraft:coal_ore",
    ] {
        assert_eq!(
            ore_experience(block, |_| panic!("não sorteia")),
            None,
            "{block}"
        );
    }
}

/// Uma linha de estado de `facts.json` (`StateFacts`: lados firmes, fluido, sinais e o instrumento na posição 8) com o instrumento dado.
fn state_row(instrument: &str) -> J {
    J::Arr(vec![
        J::Num(63),
        J::Num(0),
        J::Num(0),
        J::Num(0),
        J::Null,
        J::Num(0),
        J::Num(0),
        J::Num(0),
        J::Str(instrument.to_owned()),
        J::Null,
    ])
}

#[test]
fn the_note_instrument_injection_gives_the_original_instrument() {
    // Os dois blocos: o `HARP` do gabarito vira o instrumento do original (`MekanismBlocks.java:980-982`).
    for (id, original, template) in INSTRUMENT_DIFFERENCES {
        let mut row = state_row(template);
        note_instrument::run(&mut row, &format!("mekanism:{id}"));
        assert_eq!(row, state_row(original), "{id}");
    }
    // Só eles: outro bloco do Mekanism, o gabarito e um bloco do vanilla ficam como estão.
    for to in [
        "mekanism:block_osmium",
        "mekanism:salt",
        "minecraft:lapis_block",
        "mekanism:block_salt_extra",
    ] {
        let mut row = state_row("HARP");
        note_instrument::run(&mut row, to);
        assert_eq!(row, state_row("HARP"), "{to}");
    }
    // Só o `HARP` do gabarito: um instrumento que a linha já tinha não é tocado.
    let mut row = state_row("BASEDRUM");
    note_instrument::run(&mut row, "mekanism:block_salt");
    assert_eq!(row, state_row("BASEDRUM"));
    // Só a linha de estado: um nó que não é uma lista, uma lista curta, ou uma lista cujo primeiro valor não é um número (uma lista de nomes) passam sem mudança.
    let object = J::Obj(vec![("sound".to_owned(), J::Str("HARP".to_owned()))]);
    let short = J::Arr(vec![J::Num(1), J::Str("HARP".to_owned())]);
    let names = J::Arr(
        ["a", "b", "c", "d", "e", "f", "g", "h", "HARP"]
            .into_iter()
            .map(|name| J::Str(name.to_owned()))
            .collect(),
    );
    // Oito valores, um a menos do que a posição do instrumento pede: não há instrumento a trocar nem posição 8 a ler.
    let eight = J::Arr((0..8).map(J::Num).collect());
    for node in [
        J::Null,
        J::Num(8),
        J::Str("HARP".to_owned()),
        object,
        short,
        names,
        eight,
    ] {
        let mut copy = node.clone();
        note_instrument::run(&mut copy, "mekanism:block_salt");
        assert_eq!(copy, node);
    }
}
