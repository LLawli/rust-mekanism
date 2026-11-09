//! O desenho vestido do MekaSuit, do jetpack, das botas e do tanque de mergulho (`client/render/armor` do original): a tabela `mekanism_rules::worn_gear::WORN_GEAR` confere com o
//! Java (`rastros/armor_gear.json`, lido do texto do código de cliente por `dev/referencia/armadura_rastros.py`) e com o OBJ do MekaSuit (os grupos de objetos de cada peça, a posição
//! do corpo, o brilho, as peças `excl_` e `shared_`), e os modelos avulsos do conteúdo são exatamente os que a tabela cita.
mod common;

use common::craft::{assets, check_model, content, digest, json, rastro};
use machinery_basis_core::worn_gear::{
    Attachment, Foil, Gear, Op, Slot, compose, foil_of, hidden_parts, hides_cape, transform_point,
    visible_attachments,
};
use mekanism_rules::worn_gear::{ATLAS_SPRITES, WORN_GEAR, models};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

fn gear() -> Value {
    rastro("armor_gear.json")
}

fn number(value: &Value) -> f32 {
    value.as_f64().unwrap() as f32
}

fn vector(value: &Value) -> [f32; 3] {
    let list = value.as_array().unwrap();
    assert_eq!(list.len(), 3);
    [number(&list[0]), number(&list[1]), number(&list[2])]
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn slot_named(name: &str) -> Slot {
    match name {
        "HEAD" => Slot::Head,
        "CHEST" => Slot::Chest,
        "LEGS" => Slot::Legs,
        "FEET" => Slot::Feet,
        other => panic!("slot {other}"),
    }
}

/// Os nomes dos objetos do OBJ, na ordem do arquivo.
fn object_names() -> Vec<String> {
    let text = std::fs::read_to_string(assets().join("models/entity/mekasuit.obj")).unwrap();
    text.lines()
        .filter_map(|line| line.strip_prefix("o "))
        .map(|name| name.trim().to_owned())
        .collect()
}

fn item(id: &str) -> &'static Gear {
    WORN_GEAR
        .iter()
        .find(|gear| gear.item == format!("mekanism:{id}"))
        .unwrap_or_else(|| panic!("{id} fora da tabela"))
}

#[test]
fn the_copied_files_are_the_ones_the_original_writes() {
    let manifest = rastro("armor_assets.json");
    let manifest = manifest.as_object().unwrap();
    assert_eq!(
        manifest.len(),
        7,
        "o rastro tem o OBJ, o .mtl, o .json e as quatro texturas"
    );
    for (relative, expected) in manifest {
        let path = content().join(relative);
        assert!(path.is_file(), "falta {relative}");
        assert_eq!(
            digest(&path),
            expected.as_str().unwrap(),
            "{relative} difere do original"
        );
    }
}

#[test]
fn the_obj_is_the_one_the_original_bakes_with_its_parts_named_as_the_code_reads_them() {
    let rastro = gear();
    let parts = rastro["obj_parts"].as_array().unwrap();
    let names = object_names();
    assert_eq!(names.len(), 126);
    assert_eq!(
        parts
            .iter()
            .map(|part| part["name"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        names
    );
    // Todo objeto é um cubo de seis quadrados ou de menos (faces escondidas): nenhum triângulo, nenhum objeto vazio.
    for part in parts {
        assert_eq!(part["triangles"], 0, "{part}");
        assert!((3..=6).contains(&part["quads"].as_u64().unwrap()), "{part}");
    }
    let mut seen = BTreeSet::new();
    check_model("mekanism:entity/mekasuit", &mut seen);
}

/// `MekaSuitArmor.createQuads` sem módulos: os objetos de cada peça, agrupados por posição do corpo, classe (opaco, brilho, vidro) e etiqueta (`excl_`, `shared_`).
type Groups = BTreeMap<(String, String, String, String), Vec<String>>;

fn java_groups() -> Groups {
    let rastro = gear();
    let mekasuit = &rastro["mekasuit"];
    let positions: Vec<(&str, &str)> = mekasuit["positions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["name"].as_str().unwrap(), p["contains"].as_str().unwrap()))
        .collect();
    let checks = mekasuit["equipment_check"].as_object().unwrap();
    let tags = &mekasuit["tags"];
    let mut groups = Groups::new();
    for name in object_names() {
        // `ModelPos.get`: o primeiro da ordem do enum cujo `contains` casa o nome em minúsculas (as asas não existem no OBJ do traje).
        let lowered = name.to_lowercase();
        let position = positions
            .iter()
            .find(|(_, contains)| lowered.contains(contains))
            .unwrap_or_else(|| panic!("{name}: sem posição"))
            .0;
        for (slot, word) in checks {
            // `checkEquipment`: a peça casa se o nome contém a palavra dela.
            if !name.contains(word.as_str().unwrap()) {
                continue;
            }
            let class = if name.contains(tags["glass"].as_str().unwrap()) {
                "translucent"
            } else if name.contains(tags["emissive"].as_str().unwrap()) {
                "emissive"
            } else {
                "opaque"
            };
            let label = if name.starts_with(tags["exclusive"].as_str().unwrap()) {
                "excl"
            } else if name.starts_with(tags["shared"].as_str().unwrap()) {
                "shared"
            } else {
                ""
            };
            groups
                .entry((
                    slot.to_lowercase(),
                    position.to_lowercase(),
                    class.to_owned(),
                    label.to_owned(),
                ))
                .or_default()
                .push(name.clone());
        }
    }
    groups
}

fn model_file_name(key: &(String, String, String, String)) -> String {
    let piece = match key.0.as_str() {
        "head" => "head",
        "chest" => "chest",
        "legs" => "legs",
        "feet" => "feet",
        other => panic!("{other}"),
    };
    [piece, key.1.as_str(), key.2.as_str(), key.3.as_str()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

#[test]
fn every_group_of_the_suit_is_a_baked_model_that_shows_exactly_its_objects() {
    let groups = java_groups();
    let names = object_names();
    // 22 grupos (peça, posição, classe, etiqueta): é o que as 126 partes dão com as regras do Java, e o que o original pode pedir a `getParts`.
    assert_eq!(groups.len(), 22);
    let directory = assets().join("models/entity/mekasuit");
    let files: BTreeSet<String> = std::fs::read_dir(&directory)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    let expected: BTreeSet<String> = groups
        .keys()
        .map(|key| format!("{}.json", model_file_name(key)))
        .collect();
    assert_eq!(files, expected);
    for (key, shown) in &groups {
        let model = json(&directory.join(format!("{}.json", model_file_name(key))));
        assert_eq!(model["parent"], "mekanism:entity/mekasuit");
        let visibility = model["visibility"].as_object().unwrap();
        // O original cita todos os objetos (`ModelUtil.partVisibility`), e o Horizon mostra o que não for citado.
        assert_eq!(visibility.len(), names.len(), "{key:?}");
        for name in &names {
            assert_eq!(visibility[name], shown.contains(name), "{key:?} {name}");
        }
    }
}

#[test]
fn the_table_has_one_attachment_per_group_with_the_neighbour_rule_of_the_original() {
    let rastro = gear();
    let mekasuit = &rastro["mekasuit"];
    let instances = mekasuit["instances"].as_object().unwrap();
    assert_eq!(instances.len(), 4);
    let groups = java_groups();
    let ids = [
        ("HELMET", "mekasuit_helmet"),
        ("BODYARMOR", "mekasuit_bodyarmor"),
        ("PANTS", "mekasuit_pants"),
        ("BOOTS", "mekasuit_boots"),
    ];
    let mut seen = BTreeSet::new();
    for (instance, id) in ids {
        let slot = slot_named(instances[instance]["slot"].as_str().unwrap());
        let adjacent = slot_named(instances[instance]["adjacent"].as_str().unwrap());
        let gear = item(id);
        assert_eq!(gear.slot, slot);
        assert_eq!(gear.family, "mekasuit");
        let mut expected = Vec::new();
        for (key, _) in groups
            .iter()
            .filter(|(key, _)| key.0 == format!("{slot:?}").to_lowercase())
        {
            let hidden = match key.3.as_str() {
                // `if (name.startsWith(EXCLUSIVE_TAG)) { if (wornParts.contains(adjacentType)) continue; }`
                "excl" => Some(adjacent),
                // `else if (name.startsWith(SHARED_TAG) && wornParts.contains(adjacentType) && adjacentType.ordinal() > type.ordinal()) continue;`
                "shared" if adjacent.ordinal() > slot.ordinal() => Some(adjacent),
                _ => None,
            };
            assert_ne!(
                key.2, "translucent",
                "o traje não tem vidro: nenhuma peça translúcida na tabela"
            );
            expected.push((
                format!("mekanism:entity/mekasuit/{}", model_file_name(key)),
                key.1.clone(),
                key.2 == "emissive",
                hidden,
            ));
        }
        let mut actual: Vec<(String, String, bool, Option<Slot>)> = gear
            .attachments
            .iter()
            .map(|a| {
                (
                    a.model.to_owned(),
                    a.part.to_owned(),
                    a.unlit,
                    a.hidden_if_worn,
                )
            })
            .collect();
        expected.sort();
        actual.sort();
        assert_eq!(actual, expected, "{id}");
        for attachment in gear.attachments {
            assert!(
                seen.insert(attachment.model),
                "{} em duas peças",
                attachment.model
            );
            assert!(
                !attachment.needs_visible_part,
                "o traje desenha mesmo com as partes escondidas (`isVisible` é sempre true)"
            );
        }
    }
    assert_eq!(seen.len(), 22);
}

#[test]
fn the_suit_poses_are_the_ones_of_model_pos() {
    let rastro = gear();
    let mekasuit = &rastro["mekasuit"];
    let base = vector(&mekasuit["base_translation"]);
    assert_eq!(base, [-1.0, 0.5, 0.0]);
    assert_eq!(
        rastro["obj_state"],
        serde_json::json!({"rotation": "ZP", "radians": "PI"})
    );
    assert_eq!(mekasuit["baby_scales_all_but_head"], true);
    let baby = mekasuit["baby_translation"].as_object().unwrap();
    // A rotação de PI em Z em volta do centro do bloco: (x, y, z) vira (1 - x, 1 - y, z).
    let state = [Op::Translate([1.0, 1.0, 0.0]), Op::Scale([-1.0, -1.0, 1.0])];
    for position in mekasuit["positions"].as_array().unwrap() {
        let name = position["name"].as_str().unwrap();
        // As asas só existem com o módulo do élitro (Fase 6).
        let part = match name {
            "HEAD" => "head",
            "BODY" => "body",
            "LEFT_ARM" => "left_arm",
            "RIGHT_ARM" => "right_arm",
            "LEFT_LEG" => "left_leg",
            "RIGHT_LEG" => "right_leg",
            _ => continue,
        };
        let transform = add(base, vector(&position["add"]));
        let mut ops = vec![Op::Translate(transform)];
        ops.extend(state);
        let mut baby_ops = Vec::new();
        if name != "HEAD" {
            baby_ops.push(Op::AgeScale);
        }
        if let Some(shift) = baby.get(name) {
            baby_ops.push(Op::Translate(vector(shift)));
        }
        baby_ops.push(Op::Translate(transform));
        baby_ops.extend(state);
        let mut found = 0;
        for gear in WORN_GEAR.iter().filter(|gear| gear.family == "mekasuit") {
            for attachment in gear.attachments.iter().filter(|a| a.part == part) {
                found += 1;
                assert_eq!(attachment.ops, ops, "{} {}", gear.item, attachment.model);
                assert_eq!(
                    attachment.baby_ops, baby_ops,
                    "{} {} bebê",
                    gear.item, attachment.model
                );
            }
        }
        assert!(found > 0, "{name} sem peças");
    }
}

#[test]
fn the_obj_state_turns_a_point_about_the_block_centre() {
    // O ponto do corpo (0,25; 1,46875; 0,1246875) do OBJ vira (0,75; -0,46875; 0,1246875) antes da translação de base: com ela, (-0,25; 0,03125; 0,1246875): o canto de cima
    // do corpo do jogador, 0,5 pixel abaixo do ombro (`1.501` do vanilla contra os `1.5` do Blockbench).
    let head = &item("mekasuit_bodyarmor").attachments[0];
    assert_eq!(head.part, "body");
    let matrix = compose(head.ops, 1.0);
    let moved = transform_point(&matrix, [0.25, 1.46875, 0.1246875]);
    assert!(
        (moved[0] + 0.25).abs() < 1e-6
            && (moved[1] - 0.03125).abs() < 1e-6
            && (moved[2] - 0.1246875).abs() < 1e-6,
        "{moved:?}"
    );
}

#[test]
fn the_chest_and_boot_poses_are_the_ones_of_the_code() {
    let rastro = gear();
    let chest = &rastro["chest"];
    for (id, key) in [
        ("jetpack", "jetpack"),
        ("jetpack_armored", "jetpack"),
        ("scuba_tank", "scuba_tank"),
    ] {
        let numbers = &chest[key];
        let gear = item(id);
        assert_eq!(gear.slot, Slot::Chest);
        assert!(gear.hides.is_empty());
        let [attachment] = gear.attachments else {
            panic!("{id}")
        };
        assert_eq!(attachment.part, "body");
        assert!(
            attachment.needs_visible_part,
            "`ChestArmor.isVisible` é `body.visible`"
        );
        assert!(!attachment.unlit && attachment.hidden_if_worn.is_none());
        let mut ops = vec![
            Op::Translate(vector(&numbers["adult"])),
            Op::Translate(vector(&numbers["translate"])),
            Op::Scale(vector(&numbers["scale"])),
            // O modelo de item sai com o centro do bloco na origem (medido contra o print do original, `jetpack_costas`: sem isto o jetpack sobe meio bloco).
            Op::Translate([-0.5, -0.5, -0.5]),
        ];
        assert_eq!(attachment.ops, ops, "{id}");
        ops.remove(0);
        let mut baby = vec![Op::AgeScale];
        if !numbers["baby"].is_null() {
            baby.push(Op::Translate(vector(&numbers["baby"])));
        }
        baby.extend(ops);
        assert_eq!(attachment.baby_ops, baby, "{id} bebê");
    }
    assert_eq!(
        item("jetpack").attachments[0].model,
        "mekanism:item/jetpack"
    );
    assert_eq!(
        item("jetpack_armored").attachments[0].model,
        "mekanism:item/jetpack_armored"
    );
    assert_eq!(
        item("scuba_tank").attachments[0].model,
        "mekanism:item/scuba_tank"
    );

    let runners = &rastro["free_runners"];
    let models = &rastro["models"];
    for (id, suffix) in [("free_runners", ""), ("free_runners_armored", "_armored")] {
        let gear = item(id);
        assert_eq!(gear.slot, Slot::Feet);
        assert!(gear.hides.is_empty() && !gear.hides_cape);
        assert_eq!(gear.attachments.len(), 2);
        for (attachment, side) in gear.attachments.iter().zip(["left", "right"]) {
            assert_eq!(attachment.part, format!("{side}_leg"));
            assert_eq!(
                attachment.model,
                format!(
                    "mekanism:{}",
                    models[format!("{side}_free_runner{suffix}")]
                        .as_str()
                        .unwrap()
                )
            );
            assert!(
                attachment.needs_visible_part,
                "`FreeRunnerArmor.render` só desenha a bota da perna visível"
            );
            let xo = number(&runners["xo"][side]);
            let at = [
                xo,
                number(&runners["translate_y"]),
                number(&runners["translate_z"]),
            ];
            let scale = vector(&runners["scale"]);
            assert_eq!(
                attachment.ops,
                [Op::Translate(at), Op::Scale(scale)],
                "{id} {side}"
            );
            assert_eq!(runners["baby"]["scale_age"], true);
            assert_eq!(
                attachment.baby_ops,
                [
                    Op::AgeScale,
                    Op::Translate(vector(&runners["baby"]["translate"])),
                    Op::Translate(at),
                    Op::Scale(scale)
                ],
                "{id} {side} bebê"
            );
        }
    }
}

/// O brilho de encantamento (`stack.hasFoil()`) de cada item vestido é o tipo de desenho do original (`rastros/armor_gear.json`, `foil`, lido do texto do código de cliente e
/// do Minecraft): o MekaSuit leva o `armorCutoutNoCullGlint` (a textura da armadura, escala 0,16) em toda peça, e o jetpack, o tanque e as botas levam o brilho do item
/// (`itemCutoutGlint`, a textura do item, escala 8).
#[test]
fn the_enchantment_glint_of_each_item_is_the_render_type_of_the_original() {
    let foil = &gear()["foil"];
    assert_eq!(foil["mekasuit"]["render_type"], "armorCutoutNoCullGlint");
    assert_eq!(foil["mekasuit"]["sampler"], "ENCHANTED_GLINT_ARMOR");
    assert_eq!(number(&foil["mekasuit"]["scale"]), 0.16);
    assert_eq!(
        foil["mekasuit"]["pieces_with_glint"], 3,
        "a opaca, a emissiva e a translúcida"
    );
    for family in ["free_runners", "jetpack", "scuba_tank"] {
        assert_eq!(foil[family]["render_type"], "itemCutoutGlint", "{family}");
        assert_eq!(foil[family]["sampler"], "ENCHANTED_GLINT_ITEM", "{family}");
        assert_eq!(number(&foil[family]["scale"]), 8.0, "{family}");
    }
    let style = |name: &str| match foil[name]["style"].as_str().unwrap() {
        "armor" => Foil::Armor,
        "item" => Foil::Item,
        other => panic!("{other}"),
    };
    for gear in &WORN_GEAR {
        let id = gear.item.strip_prefix("mekanism:").unwrap();
        let family = match id {
            "jetpack" | "jetpack_armored" => "jetpack",
            "scuba_tank" => "scuba_tank",
            "free_runners" | "free_runners_armored" => "free_runners",
            suit => {
                assert!(suit.starts_with("mekasuit_"), "{suit}");
                "mekasuit"
            }
        };
        assert_eq!(gear.foil, style(family), "{id}");
        // O brilho é do slot do item: encantado, brilha; sem encantar, não.
        let mut enchanted = [false; 4];
        assert_eq!(foil_of(gear, &enchanted), None, "{id}");
        enchanted[gear.slot.index()] = true;
        assert_eq!(foil_of(gear, &enchanted), Some(style(family)), "{id}");
    }
}

/// Os sprites que os modelos vestidos usam (as texturas do OBJ do MekaSuit, do jetpack, do tanque e das botas, seguindo `parent` e `children`), um a um.
fn model_textures() -> BTreeSet<String> {
    fn take(node: &Value, textures: &mut BTreeSet<String>) {
        for value in node["textures"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(_, v)| v)
        {
            if let Some(text) = value.as_str().filter(|t| !t.starts_with('#')) {
                textures.insert(text.to_owned());
            }
        }
    }
    fn walk(reference: &str, seen: &mut BTreeSet<String>, textures: &mut BTreeSet<String>) {
        if !seen.insert(reference.to_owned()) {
            return;
        }
        let path = assets().join("models").join(format!(
            "{}.json",
            reference.trim_start_matches("mekanism:")
        ));
        let model: Value = serde_json::from_str(
            &std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())),
        )
        .unwrap();
        take(&model, textures);
        for child in model["children"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(_, v)| v)
        {
            take(child, textures);
            if let Some(parent) = child["parent"].as_str() {
                walk(parent, seen, textures);
            }
        }
        if let Some(parent) = model["parent"].as_str() {
            walk(parent, seen, textures);
        }
    }
    let mut textures = BTreeSet::new();
    let mut seen = BTreeSet::new();
    for model in models() {
        walk(model, &mut seen, &mut textures);
    }
    textures
}

/// O brilho de encantamento amostra a textura do brilho com a UV0 do quad no atlas de itens do original, então a tabela guarda onde o original põe cada sprite
/// (`rastros/armor_atlas.json`, do `CenaEquip`): cada entrada é a do rastro, e todo sprite que um modelo vestido usa está na tabela.
#[test]
fn the_atlas_sprites_are_where_the_original_puts_them() {
    let atlas = rastro("armor_atlas.json");
    for sprite in &ATLAS_SPRITES {
        let rect: Vec<f32> = atlas[format!("mekanism:{}", sprite.name)]
            .as_array()
            .unwrap_or_else(|| panic!("{} fora do rastro", sprite.name))
            .iter()
            .map(number)
            .collect();
        assert_eq!(rect, sprite.rect, "{}", sprite.name);
    }
    let named: BTreeSet<&str> = ATLAS_SPRITES.iter().map(|sprite| sprite.name).collect();
    assert_eq!(named.len(), ATLAS_SPRITES.len(), "um sprite duas vezes");
    for texture in model_textures() {
        let bare = texture.trim_start_matches("mekanism:");
        assert!(
            named.contains(bare),
            "{texture} usa o atlas do original e a tabela não o tem"
        );
    }
    // O atlas de itens do original tem 1024 pixels de lado: o retângulo do jetpack (64 por 128) cai em múltiplos de 1/1024.
    let jetpack = ATLAS_SPRITES
        .iter()
        .find(|sprite| sprite.name == "item/jetpack")
        .unwrap()
        .rect;
    assert_eq!(((jetpack[2] - jetpack[0]) * 1024.0).round(), 64.0);
    assert_eq!(((jetpack[3] - jetpack[1]) * 1024.0).round(), 128.0);
}

#[test]
fn what_each_piece_hides_is_what_the_render_state_modifier_hides() {
    let rastro = gear();
    let registration = &rastro["registration"];
    let hidden = |value: &Value| -> Vec<String> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|part| part.as_str().unwrap().to_owned())
            .collect()
    };
    for (id, key) in [
        ("mekasuit_helmet", "mekasuit_helmet"),
        ("mekasuit_bodyarmor", "mekasuit_bodyarmor"),
        ("mekasuit_pants", "mekasuit_pants"),
        ("jetpack", "jetpack_or_scuba_tank"),
        ("jetpack_armored", "jetpack_or_scuba_tank"),
        ("scuba_tank", "jetpack_or_scuba_tank"),
    ] {
        let gear = item(id);
        assert_eq!(
            gear.hides
                .iter()
                .map(|part| (*part).to_owned())
                .collect::<Vec<_>>(),
            hidden(&registration[key]["hides"]),
            "{id}"
        );
        assert_eq!(gear.hides_cape, registration[key]["hides_cape"], "{id}");
    }
    // As botas do MekaSuit e as Free Runners não escondem nada.
    assert!(item("mekasuit_boots").hides.is_empty() && !item("mekasuit_boots").hides_cape);
}

#[test]
fn the_standalone_models_are_every_model_the_table_cites_and_each_one_exists() {
    let rastro = gear();
    let named = models();
    let unique: BTreeSet<&str> = named.iter().copied().collect();
    assert_eq!(unique.len(), named.len(), "um modelo citado duas vezes");
    // 22 do traje, 3 do peito (jetpack, jetpack blindado, tanque) e 4 de botas.
    assert_eq!(named.len(), 22 + 3 + 4);
    let from_rastro: BTreeSet<String> = [
        "left_free_runner",
        "left_free_runner_armored",
        "right_free_runner",
        "right_free_runner_armored",
    ]
    .iter()
    .map(|key| format!("mekanism:{}", rastro["models"][*key].as_str().unwrap()))
    .collect();
    for model in &from_rastro {
        assert!(unique.contains(model.as_str()), "{model}");
    }
    let mut seen = BTreeSet::new();
    for model in named {
        check_model(model, &mut seen);
    }
    // O OBJ, o .mtl e as texturas vieram junto (`check_model` do filho abre o `parent`).
    assert!(seen.contains("entity/mekasuit"));
}

fn worn<'a>(
    head: Option<&'a str>,
    chest: Option<&'a str>,
    legs: Option<&'a str>,
    feet: Option<&'a str>,
) -> [Option<&'a str>; 4] {
    [head, chest, legs, feet]
}

fn models_of(worn: &[Option<&str>; 4]) -> BTreeSet<&'static str> {
    visible_attachments(&WORN_GEAR, worn)
        .into_iter()
        .map(|a: &Attachment| a.model)
        .collect()
}

#[test]
fn the_suit_shows_the_parts_the_neighbours_leave() {
    let suit = [
        "mekanism:mekasuit_helmet",
        "mekanism:mekasuit_bodyarmor",
        "mekanism:mekasuit_pants",
        "mekanism:mekasuit_boots",
    ];
    let all = worn(Some(suit[0]), Some(suit[1]), Some(suit[2]), Some(suit[3]));
    let shown = models_of(&all);
    // O traje inteiro: o peito sem as compartilhadas (a cabeça está vestida), as pernas com elas, os pés sem as exclusivas nem as compartilhadas (as pernas estão vestidas).
    assert!(!shown.contains("mekanism:entity/mekasuit/chest_body_opaque_shared"));
    assert!(shown.contains("mekanism:entity/mekasuit/legs_body_opaque_shared"));
    assert!(shown.contains("mekanism:entity/mekasuit/legs_left_leg_opaque_shared"));
    assert!(!shown.contains("mekanism:entity/mekasuit/feet_left_leg_opaque_excl"));
    assert!(!shown.contains("mekanism:entity/mekasuit/feet_left_leg_opaque_shared"));
    assert!(shown.contains("mekanism:entity/mekasuit/feet_left_leg_opaque"));
    assert_eq!(shown.len(), 22 - 1 - 4);
    // Só o peito: com as compartilhadas, porque a cabeça não está vestida.
    let only_chest = models_of(&worn(None, Some(suit[1]), None, None));
    assert!(only_chest.contains("mekanism:entity/mekasuit/chest_body_opaque_shared"));
    assert_eq!(only_chest.len(), 7);
    // Só as botas: com as exclusivas (a canela) e as compartilhadas.
    let only_boots = models_of(&worn(None, None, None, Some(suit[3])));
    assert!(only_boots.contains("mekanism:entity/mekasuit/feet_right_leg_opaque_excl"));
    assert_eq!(only_boots.len(), 6);
    // Só o capacete: as duas peças da cabeça (o opaco e os LEDs).
    let helmet_only = models_of(&worn(Some(suit[0]), None, None, None));
    assert_eq!(helmet_only.len(), 2);
}

#[test]
fn the_jetpack_the_tank_and_the_boots_hide_the_cape_and_the_suit_hides_the_body() {
    assert!(hides_cape(
        &WORN_GEAR,
        &worn(None, Some("mekanism:jetpack"), None, None)
    ));
    assert!(hides_cape(
        &WORN_GEAR,
        &worn(None, Some("mekanism:jetpack_armored"), None, None)
    ));
    assert!(hides_cape(
        &WORN_GEAR,
        &worn(None, Some("mekanism:scuba_tank"), None, None)
    ));
    assert!(hides_cape(
        &WORN_GEAR,
        &worn(None, Some("mekanism:mekasuit_bodyarmor"), None, None)
    ));
    assert!(!hides_cape(
        &WORN_GEAR,
        &worn(
            Some("mekanism:mekasuit_helmet"),
            None,
            None,
            Some("mekanism:free_runners")
        )
    ));
    assert!(!hides_cape(
        &WORN_GEAR,
        &worn(None, Some("minecraft:diamond_chestplate"), None, None)
    ));
    assert_eq!(
        hidden_parts(
            &WORN_GEAR,
            &worn(
                Some("mekanism:mekasuit_helmet"),
                Some("mekanism:mekasuit_bodyarmor"),
                None,
                None
            )
        ),
        ["head", "body", "left_arm", "right_arm"]
    );
    // Um item que a tabela não cita, no slot que for, não conta (e o item da tabela em outro slot também não).
    assert!(
        hidden_parts(
            &WORN_GEAR,
            &worn(Some("mekanism:mekasuit_bodyarmor"), None, None, None)
        )
        .is_empty()
    );
}

#[test]
fn the_jetpack_draws_with_a_visible_body_and_the_runners_with_a_visible_leg() {
    // O jetpack e o tanque pedem o corpo visível, e o traje vestido por cima não o deixa: ele esconde o corpo (`body.visible`).
    assert_eq!(
        models_of(&worn(None, Some("mekanism:jetpack"), None, None)),
        BTreeSet::from(["mekanism:item/jetpack"])
    );
    assert_eq!(
        models_of(&worn(None, Some("mekanism:scuba_tank"), None, None)),
        BTreeSet::from(["mekanism:item/scuba_tank"])
    );
    assert_eq!(
        models_of(&worn(
            None,
            None,
            None,
            Some("mekanism:free_runners_armored")
        )),
        BTreeSet::from([
            "mekanism:item/left_free_runner_armored",
            "mekanism:item/right_free_runner_armored"
        ])
    );
    // As calças do MekaSuit escondem as pernas (`overrideModelPartVisibility`): as Free Runners não se desenham (`FreeRunnerArmor.isVisible`), como no original.
    assert!(
        models_of(&worn(
            None,
            None,
            Some("mekanism:mekasuit_pants"),
            Some("mekanism:free_runners")
        ))
        .iter()
        .all(|m| m.starts_with("mekanism:entity/"))
    );
}

/// Os quads de cada objeto do OBJ (`rastros/armor_gear.json`, `obj_parts`), por nome.
fn quads_of_objects() -> BTreeMap<String, u64> {
    let rastro = gear();
    rastro["obj_parts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|part| {
            (
                part["name"].as_str().unwrap().to_owned(),
                part["quads"].as_u64().unwrap(),
            )
        })
        .collect()
}

/// Os quads dos objetos que um modelo avulso mostra (a visibilidade do arquivo).
fn quads_of_model(model: &str) -> u64 {
    let quads = quads_of_objects();
    let file = assets()
        .join("models")
        .join(format!("{}.json", model.strip_prefix("mekanism:").unwrap()));
    json(&file)["visibility"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(_, shown)| shown.as_bool().unwrap())
        .map(|(name, _)| quads[name])
        .sum()
}

/// `MekaSuitArmor.createQuads` rodado no cliente do original (`CenaEquip`, linha `grupos`, `rastros/armor_groups.jsonl`): para cada peça do traje e cada combinação de peças vestidas que
/// a tem, os quads opacos, brilhantes e translúcidos de cada posição do corpo. A tabela do port, com a visibilidade de cada modelo e os quads de cada objeto, dá os mesmos números.
#[test]
fn the_groups_of_every_piece_and_combination_are_the_quads_the_original_bakes() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/armor_groups.jsonl");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let lines: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    // 4 peças x 8 combinações que a têm, e as posições que cada uma desenha.
    assert!(lines.len() >= 72, "só {} linhas", lines.len());
    let suit = [
        "mekanism:mekasuit_helmet",
        "mekanism:mekasuit_bodyarmor",
        "mekanism:mekasuit_pants",
        "mekanism:mekasuit_boots",
    ];
    let slots = ["HEAD", "CHEST", "LEGS", "FEET"];
    // (peça, combinação, posição) -> (opacos, brilho)
    let mut java: BTreeMap<(String, Vec<String>, String), (u64, u64)> = BTreeMap::new();
    for line in &lines {
        assert_eq!(line["translucidos"], 0, "{line}");
        let worn: Vec<String> = line["vestidas"]
            .as_array()
            .unwrap()
            .iter()
            .map(|slot| slot.as_str().unwrap().to_owned())
            .collect();
        let key = (
            line["peca"].as_str().unwrap().to_owned(),
            worn,
            line["posicao"].as_str().unwrap().to_owned(),
        );
        assert!(
            java.insert(
                key,
                (
                    line["opacos"].as_u64().unwrap(),
                    line["brilho"].as_u64().unwrap()
                )
            )
            .is_none(),
            "{line}"
        );
    }
    let mut checked = 0;
    for (index, piece) in slots.iter().enumerate() {
        for mask in 0..16u32 {
            if mask & (1 << index) == 0 {
                continue;
            }
            let combination: Vec<String> = slots
                .iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(_, slot)| (*slot).to_owned())
                .collect();
            let mut worn: [Option<&str>; 4] = [None; 4];
            for (i, id) in suit.iter().enumerate() {
                if mask & (1 << i) != 0 {
                    worn[i] = Some(id);
                }
            }
            let mut ours: BTreeMap<String, (u64, u64)> = BTreeMap::new();
            for attachment in visible_attachments(&WORN_GEAR, &worn) {
                // Só as peças desta peça do traje (o modelo se chama `<peça>_<posição>_<classe>`).
                if !attachment.model.starts_with(&format!(
                    "mekanism:entity/mekasuit/{}_",
                    ["head", "chest", "legs", "feet"][index]
                )) {
                    continue;
                }
                let entry = ours.entry(attachment.part.to_uppercase()).or_default();
                let quads = quads_of_model(attachment.model);
                if attachment.unlit {
                    entry.1 += quads;
                } else {
                    entry.0 += quads;
                }
            }
            let theirs: BTreeMap<String, (u64, u64)> = java
                .iter()
                .filter(|((p, w, _), _)| p == piece && *w == combination)
                .map(|((_, _, position), counts)| (position.clone(), *counts))
                .collect();
            assert_eq!(ours, theirs, "{piece} com {combination:?}");
            checked += 1;
        }
    }
    assert_eq!(checked, 32);
    assert_eq!(java.len(), lines.len());
}
