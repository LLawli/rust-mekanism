//! Os 22 fluidos do original (18 do `MekanismFluids` e 4 do `GeneratorsFluids`) como conteúdo: o bloco de líquido, o balde, os modelos, as
//! texturas, o nome e as tags, conferidos contra a datagen do original (`dev/referencia/datagen/fluidos`, copiada de
//! `src/datagen/generated`) e contra o rastro do Java (`dev/referencia/rastros/fluids.jsonl`, `FluidsTraceTest`). O conteúdo vem de
//! `scripts/gerar_fluidos.py`.
//!
//! O Horizon não tem fluido de mod no mundo (`FluidKind` é uma enumeração de água e lava), então o fluido parado e o que corre são um bloco
//! só, com a propriedade `level` do `LiquidBlock`; o fluxo é da cola (`machinery_basis`), e o modelo faz o papel do mesher de fluidos.
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// (mod, nome) na ordem de registro do original: `MekanismFluids` e depois `GeneratorsFluids`.
const FLUIDS: [(&str, &str); 22] = [
    ("mekanism", "hydrogen"),
    ("mekanism", "oxygen"),
    ("mekanism", "chlorine"),
    ("mekanism", "sulfur_dioxide"),
    ("mekanism", "sulfur_trioxide"),
    ("mekanism", "sulfuric_acid"),
    ("mekanism", "hydrogen_chloride"),
    ("mekanism", "hydrofluoric_acid"),
    ("mekanism", "uranium_oxide"),
    ("mekanism", "uranium_hexafluoride"),
    ("mekanism", "ethene"),
    ("mekanism", "sodium"),
    ("mekanism", "superheated_sodium"),
    ("mekanism", "brine"),
    ("mekanism", "lithium"),
    ("mekanism", "steam"),
    ("mekanism", "heavy_water"),
    ("mekanism", "nutritional_paste"),
    ("mekanismgenerators", "bioethanol"),
    ("mekanismgenerators", "deuterium"),
    ("mekanismgenerators", "fusion_fuel"),
    ("mekanismgenerators", "tritium"),
];

fn mods() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn content(owner: &str) -> PathBuf {
    mods().join(owner).join("content")
}

fn datagen(owner: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/datagen/fluidos")
        .join(owner)
}

fn text(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn json(path: &Path) -> Value {
    serde_json::from_str(&text(path)).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// O arquivo do conteúdo é o da datagen, byte a byte.
fn assert_copy(ours: &Path, original: &Path) {
    assert_eq!(
        text(ours),
        text(original),
        "{} difere do arquivo da datagen",
        ours.display()
    );
}

/// O arquivo de uma textura de bloco: `ns:caminho` vira `assets/ns/textures/caminho.png` no conteúdo, e o `minecraft:` é do jogo.
fn texture_file(id: &str) -> Option<PathBuf> {
    let (namespace, path) = id.split_once(':').unwrap();
    (namespace != "minecraft").then(|| {
        content(namespace)
            .join("assets")
            .join(namespace)
            .join("textures")
            .join(format!("{path}.png"))
    })
}

fn model_file(id: &str) -> PathBuf {
    let (namespace, path) = id.split_once(':').unwrap();
    content(namespace)
        .join("assets")
        .join(namespace)
        .join("models")
        .join(format!("{path}.json"))
}

#[test]
fn there_are_22_fluids_and_each_one_has_a_block_and_a_bucket_declared() {
    assert_eq!(FLUIDS.len(), 22);
    for (owner, name) in FLUIDS {
        let block = json(&content(owner).join(format!("data/{owner}/horizon/block/{name}.json")));
        // O bloco de líquido não tem item (`LiquidBlock` sem `BlockItem`): o item é o balde.
        assert_eq!(block["item"], false, "{owner}:{name}");
        assert_eq!(block["template"], "minecraft:structure_void");
        let levels: Vec<&str> = block["state_properties"]["level"]["values"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(
            levels,
            (0..16).map(|n| n.to_string()).collect::<Vec<_>>(),
            "{owner}:{name}: `LiquidBlock.LEVEL` vai de 0 a 15"
        );
        assert_eq!(block["state_properties"]["up"]["type"], "boolean");
        // `BlockBehaviour.Properties.of().noCollision().strength(100.0F).noLootTable().replaceable()` (`FluidDeferredRegister`).
        assert_eq!(block["properties"]["hardness"], 100.0);
        assert_eq!(block["properties"]["explosion_resistance"], 100.0);
        assert_eq!(block["collision_shapes"]["default"], serde_json::json!([]));
        assert_eq!(block["shapes"]["default"], serde_json::json!([]));
        let bucket =
            json(&content(owner).join(format!("data/{owner}/horizon/item/{name}_bucket.json")));
        // `itemProperties.stacksTo(1).craftRemainder(Items.BUCKET)`.
        assert_eq!(bucket["max_stack_size"], 1, "{owner}:{name}_bucket");
        assert_eq!(bucket["crafting_remainder"], "minecraft:bucket");
    }
}

#[test]
fn the_liquid_blocks_are_replaceable_as_the_original_tags_them() {
    for owner in ["mekanism", "mekanismgenerators"] {
        assert_copy(
            &content(owner).join("data/minecraft/tags/block/replaceable.json"),
            &datagen(owner).join("data/minecraft/tags/block/replaceable.json"),
        );
    }
}

#[test]
fn the_names_are_the_ones_of_the_original() {
    let original = json(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../dev/referencia/datagen/fluidos/lang.json"),
    );
    let original = original.as_object().unwrap();
    assert_eq!(original.len(), 44);
    for owner in ["mekanism", "mekanismgenerators"] {
        let ours = json(&content(owner).join(format!("assets/{owner}/lang/en_us.json")));
        for (key, value) in original
            .iter()
            .filter(|(k, _)| k.contains(&format!(".{owner}.")))
        {
            assert_eq!(ours[key], *value, "{key}");
        }
    }
}

#[test]
fn the_bucket_tags_are_the_datagen_files() {
    for (owner, name) in FLUIDS {
        assert_copy(
            &content(owner).join(format!("data/c/tags/item/buckets/{name}.json")),
            &datagen(owner).join(format!("data/c/tags/item/buckets/{name}.json")),
        );
        assert_eq!(
            json(&content(owner).join(format!("data/c/tags/item/buckets/{name}.json")))["values"],
            serde_json::json!([format!("{owner}:{name}_bucket")])
        );
    }
}

/// `c:creepers` e `c:endermen` só citam o vanilla e o Horizon lê tag de `entity_type` (`load_tags`, que varre `data/<namespace>/tags/entity_type`
/// de `minecraft`, `c` e dos mods): entram como o original as escreveu. `c:radiation_immune` cita `mekanism:robit`, que o port não tem, e uma
/// tag do original não se edita para listar menos: espera o Robit.
#[test]
fn the_entity_type_tags_that_name_only_vanilla_are_copied() {
    for name in ["creepers", "endermen"] {
        let ours = content("mekanism").join(format!("data/c/tags/entity_type/{name}.json"));
        assert_copy(
            &ours,
            &datagen("mekanism").join(format!("data/c/tags/entity_type/{name}.json")),
        );
        for entry in json(&ours)["values"].as_array().unwrap() {
            assert!(entry.as_str().unwrap().starts_with("minecraft:"), "{name}");
        }
    }
    let radiation =
        json(&datagen("mekanism").join("data/c/tags/entity_type/radiation_immune.json"));
    assert_eq!(radiation["values"], serde_json::json!(["mekanism:robit"]));
    assert!(
        !content("mekanism")
            .join("data/c/tags/entity_type/radiation_immune.json")
            .exists(),
        "a tag cita o Robit, que o port ainda não tem"
    );
}

/// O Horizon só lê tag de fluido para o F3 (`debug_lines.rs`, `load_tags(root, "fluid")`) e para a água e a lava do vanilla
/// (`motion::fluids::is_tagged`, `FluidTagId` 0 e 1, fixos): nenhum código do jogo consulta uma tag de fluido de mod, e o fluido de mod nem
/// existe no motor (`FluidKind`). As 23 tags `c:` de fluido da datagen não entram: só citariam ids que ninguém registra como fluido
/// (`mekanism:hydrogen` é um bloco aqui, e `mekanism:flowing_hydrogen` não existe), e uma tag que ninguém lê não prova nada. Pedido ao Horizon: H47.
#[test]
fn the_fluid_tags_wait_for_the_horizon_to_have_mod_fluids() {
    let mut original = BTreeSet::new();
    for owner in ["mekanism", "mekanismgenerators"] {
        let dir = datagen(owner).join("data/c/tags/fluid");
        for entry in std::fs::read_dir(dir).unwrap() {
            original.insert(format!(
                "{owner}/{}",
                entry.unwrap().file_name().to_str().unwrap()
            ));
        }
        assert!(
            !content(owner).join("data/c/tags/fluid").exists(),
            "{owner}: o Horizon não lê tag de fluido de mod; ver H47 em docs/injecoes-horizon.md"
        );
    }
    // 19 do Mekanism (os 18 fluidos mais `c:gaseous`) e 4 do Generators.
    assert_eq!(original.len(), 23);
}

#[test]
fn every_block_state_points_at_a_model_and_every_model_at_a_texture() {
    for (owner, name) in FLUIDS {
        let states = json(&content(owner).join(format!("assets/{owner}/blockstates/{name}.json")));
        let variants = states["variants"].as_object().unwrap();
        assert_eq!(variants.len(), 32, "{owner}:{name}: 16 níveis vezes `up`");
        let mut models = BTreeSet::new();
        for (key, variant) in variants {
            let (level, up) = key.split_once(',').unwrap();
            let level: u32 = level.strip_prefix("level=").unwrap().parse().unwrap();
            let up = up.strip_prefix("up=").unwrap() == "true";
            // `FlowingFluid.getHeight`: 1 com o mesmo fluido em cima, senão `getOwnHeight` (a quantidade, 8 menos o nível, sobre 9).
            let ninths = if up {
                9
            } else if (1..=7).contains(&level) {
                8 - level
            } else {
                8
            };
            let model = variant["model"].as_str().unwrap();
            assert_eq!(model, format!("{owner}:block/{name}_h{ninths}"), "{key}");
            models.insert(model.to_owned());
        }
        assert_eq!(models.len(), 9, "{owner}:{name}");
        for model in models {
            let model = json(&model_file(&model));
            let parent = model["parent"].as_str().unwrap();
            assert!(model_file(parent).is_file(), "{parent}");
            for key in ["still", "flow"] {
                let texture = model["textures"][key].as_str().unwrap();
                if let Some(file) = texture_file(texture) {
                    assert!(file.is_file(), "{texture}");
                    assert!(
                        PathBuf::from(format!("{}.mcmeta", file.display())).is_file(),
                        "{texture}: sem a animação"
                    );
                }
            }
        }
    }
}

#[test]
fn the_particle_of_the_original_block_is_the_still_texture() {
    for (owner, name) in FLUIDS {
        let original =
            json(&datagen(owner).join(format!("assets/{owner}/models/block/{name}.json")));
        let particle = original["textures"]["particle"].as_str().unwrap();
        let ours = json(&model_file(&format!("{owner}:block/{name}_h8")));
        let still = ours["textures"]["still"].as_str().unwrap();
        // Só os fluidos tingidos têm a textura própria (a cor entra no pixel, porque o bloco não tem tinta no Horizon).
        if particle == "mekanism:mek_liquid/liquid" && still != particle {
            assert_eq!(still, format!("{owner}:mek_liquid/{name}"));
        } else {
            assert_eq!(still, particle, "{owner}:{name}");
        }
    }
}

#[test]
fn the_bucket_model_is_the_bucket_with_the_fluid_layer_of_the_original() {
    for (owner, name) in FLUIDS {
        let original =
            json(&datagen(owner).join(format!("assets/{owner}/items/{name}_bucket.json")));
        let original = &original["model"];
        assert_eq!(original["type"], "neoforge:fluid_container");
        assert_eq!(original["fluid"], format!("{owner}:{name}"));
        let ours = json(&content(owner).join(format!("assets/{owner}/items/{name}_bucket.json")));
        assert_eq!(
            ours["model"]["model"],
            format!("{owner}:item/{name}_bucket")
        );
        let model = json(&model_file(&format!("{owner}:item/{name}_bucket")));
        assert_eq!(model["parent"], "minecraft:item/generated");
        // A base do `DynamicFluidContainerModel` é o balde do vanilla.
        assert_eq!(model["textures"]["layer0"], original["textures"]["base"]);
        assert_eq!(model["textures"]["layer1"], "mekanism:item/bucket_fluid");
    }
}

#[test]
fn every_fluid_texture_is_a_file_of_the_content() {
    let mut seen = BTreeSet::new();
    for (owner, name) in FLUIDS {
        for suffix in ["", "_flow"] {
            let model = json(&model_file(&format!("{owner}:block/{name}_h8")));
            let key = if suffix.is_empty() { "still" } else { "flow" };
            let texture = model["textures"][key].as_str().unwrap().to_owned();
            if let Some(file) = texture_file(&texture) {
                assert!(file.is_file(), "{texture}");
                seen.insert(texture);
            }
        }
    }
    // 19 fluidos com a textura tingida própria, parada e que corre (todos menos o hidrogênio, o vapor e a água pesada), e as quatro do original
    // (`liquid`, `liquid_flow`, `steam`, `steam_flow`); a água pesada usa a água do vanilla.
    assert_eq!(seen.len(), 19 * 2 + 4, "{seen:?}");
}

// ---- O rastro do Java (`FluidsTraceTest`, `dev/referencia/rastros/fluids.jsonl`) ----

fn trace() -> Vec<Value> {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../dev/referencia/rastros/fluids.jsonl");
    text(&path)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn id(row: &Value) -> (String, String) {
    (
        row["mod"].as_str().unwrap().to_owned(),
        row["name"].as_str().unwrap().to_owned(),
    )
}

#[test]
fn the_trace_has_the_fluids_in_the_order_the_original_registers_them() {
    let ours: Vec<(String, String)> = FLUIDS
        .iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect();
    assert_eq!(trace().iter().map(id).collect::<Vec<_>>(), ours);
}

#[test]
fn the_fluid_file_carries_the_numbers_of_the_original_fluid_and_fluid_type() {
    for row in trace() {
        let (owner, name) = id(&row);
        let file =
            json(&content(&owner).join(format!("data/{owner}/machinery_basis/liquid/{name}.json")));
        let who = format!("{owner}:{name}");
        let still = &row["fluid"]["still"];
        assert_eq!(file["block"], row["block"]["id"], "{who}");
        assert_eq!(file["bucket"], row["bucket"]["id"], "{who}");
        assert_eq!(file["tick_delay"], still["tickDelay"], "{who}");
        assert_eq!(
            file["slope_find_distance"], still["slopeFindDistance"],
            "{who}"
        );
        assert_eq!(file["drop_off"], still["dropOff"], "{who}");
        assert_eq!(
            file["can_convert_to_source"], still["canConvertToSource"],
            "{who}"
        );
        assert_eq!(file["light"], row["type"]["lightLevel"], "{who}");
        let color = row["type"]["color"].as_i64().unwrap() & 0xFF_FF_FF;
        assert_eq!(file["color"].as_i64().unwrap(), color, "{who}");
        assert_eq!(
            file["fill_sound"], row["type"]["sound_bucket_fill"],
            "{who}"
        );
        assert_eq!(
            file["empty_sound"], row["type"]["sound_bucket_empty"],
            "{who}"
        );
        // `ItemNutritionalPasteBucket.use`: com fome o balde é bebido, e só sem fome ele esvazia.
        let drinkable = row["bucket"]["class"] == "mekanism.common.item.ItemNutritionalPasteBucket";
        assert_eq!(
            file["drink_when_hungry"].as_bool().unwrap_or(false),
            drinkable,
            "{who}"
        );
        assert_eq!(drinkable, name == "nutritional_paste");
    }
}

#[test]
fn the_liquid_block_has_the_properties_of_the_original_block() {
    for row in trace() {
        let (owner, name) = id(&row);
        let who = format!("{owner}:{name}");
        let block = json(&content(&owner).join(format!("data/{owner}/horizon/block/{name}.json")));
        let original = &row["block"];
        assert_eq!(
            block["properties"]["hardness"], original["destroySpeed"],
            "{who}"
        );
        assert_eq!(
            block["properties"]["explosion_resistance"], original["explosionResistance"],
            "{who}"
        );
        let map_color = original["mapColor_id"].as_i64().unwrap();
        if map_color == 0 {
            assert!(block["properties"].get("map_color").is_none(), "{who}");
        } else {
            assert_eq!(
                block["properties"]["map_color"].as_i64().unwrap(),
                map_color,
                "{who}"
            );
        }
        // O `LiquidBlock` não emite luz, nem o de um fluido com `FluidType.lightLevel` (`lightEmission_world`, os 16 níveis).
        for level in original["levels"].as_array().unwrap() {
            assert_eq!(level["lightEmission_world"], 0, "{who}");
        }
        assert!(block["properties"].get("light").is_none(), "{who}");
        // Os 16 estados do `LiquidBlock.LEVEL`, o 0 como padrão.
        assert_eq!(original["states"], 16);
        assert_eq!(original["defaultState"]["level"], 0);
        assert_eq!(original["levels"].as_array().unwrap().len(), 16);
        // Não tem item de bloco.
        assert_eq!(original["blockItem"], "minecraft:air");
        assert_eq!(block["item"], false);
        // O nome é o do bloco (`FluidDeferredRegister`: `descriptionId(rl.toLanguageKey("block"))`).
        assert_eq!(
            row["type"]["descriptionId"],
            format!("block.{owner}.{name}"),
            "{who}"
        );
        // O balde não tem raridade nem durabilidade: o que o item tem a mais que o padrão é o tamanho da pilha.
        assert_eq!(row["bucket"]["maxStackSize"], 1, "{who}");
        assert_eq!(row["bucket"]["rarity"], "COMMON", "{who}");
        assert_eq!(row["bucket"]["craftRemainder"], "minecraft:bucket", "{who}");
    }
}

/// `ItemNutritionalPasteBucket`: os componentes do balde de pasta nutritiva no rastro são o `Consumables.defaultDrink()` do original, o
/// `usingConvertsTo(Items.BUCKET)` e o marcador `mekanism:paste_consumption`; os outros 21 baldes não têm nenhum deles.
#[test]
fn only_the_paste_bucket_is_drunk() {
    for row in trace() {
        let (owner, name) = id(&row);
        let item =
            json(&content(&owner).join(format!("data/{owner}/horizon/item/{name}_bucket.json")));
        let components = row["bucket"]["defaultStack"]["components"]
            .as_object()
            .unwrap();
        let ours = item
            .get("components")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        if name == "nutritional_paste" {
            assert_eq!(
                ours["minecraft:consumable"]["animation"],
                components["minecraft:consumable"]["animation"]
            );
            assert_eq!(
                ours["minecraft:consumable"]["sound"],
                components["minecraft:consumable"]["sound"]
            );
            assert_eq!(
                ours["minecraft:consumable"]["has_consume_particles"],
                components["minecraft:consumable"]["has_consume_particles"]
            );
            assert_eq!(
                ours["minecraft:use_remainder"],
                components["minecraft:use_remainder"]
            );
            assert!(components.contains_key("mekanism:paste_consumption"));
            // A fome e a saturação vêm da configuração (`paste_config.rs`) pelo efeito `mekanism:paste`; o `minecraft:food` só faz beber com fome.
            assert_eq!(ours["minecraft:food"]["nutrition"], 0);
            assert_eq!(
                ours["minecraft:consumable"]["on_consume_effects"][0]["type"],
                "mekanism:paste"
            );
        } else {
            for key in [
                "minecraft:consumable",
                "minecraft:use_remainder",
                "mekanism:paste_consumption",
                "minecraft:food",
            ] {
                assert!(!components.contains_key(key), "{owner}:{name}: {key}");
                assert!(!ours.contains_key(key), "{owner}:{name}: {key}");
            }
        }
    }
}

#[test]
fn the_model_textures_are_the_ones_of_the_fluid_type() {
    for row in trace() {
        let (owner, name) = id(&row);
        let who = format!("{owner}:{name}");
        let model = json(&model_file(&format!("{owner}:block/{name}_h8")));
        let color = row["type"]["color"].as_i64().unwrap() & 0xFF_FF_FF_FF;
        let still = row["type"]["stillTexture"].as_str().unwrap();
        let flowing = row["type"]["flowingTexture"].as_str().unwrap();
        // Sem tinta no bloco, a cor entra no pixel: o fluido colorido tem a textura própria, feita da do original.
        let tinted = color != 0xFF_FF_FF_FF && still == "mekanism:mek_liquid/liquid";
        if tinted {
            assert_eq!(
                model["textures"]["still"],
                format!("{owner}:mek_liquid/{name}"),
                "{who}"
            );
            assert_eq!(
                model["textures"]["flow"],
                format!("{owner}:mek_liquid/{name}_flow"),
                "{who}"
            );
        } else {
            assert_eq!(model["textures"]["still"], still, "{who}");
            assert_eq!(model["textures"]["flow"], flowing, "{who}");
        }
        // A tinta do balde é a do tipo.
        let item = json(&content(&owner).join(format!("assets/{owner}/items/{name}_bucket.json")));
        assert_eq!(
            item["model"]["tints"][1]["value"].as_i64().unwrap(),
            color & 0xFF_FF_FF,
            "{who}"
        );
        // `lightLevel` do tipo: 15 em três fluidos, e só o modelo do balde os distingue (`applyFluidLuminosity`).
        assert!(
            matches!(row["type"]["lightLevel"].as_i64().unwrap(), 0 | 15),
            "{who}"
        );
    }
}

#[test]
fn the_names_in_the_trace_are_the_lang_keys_we_have() {
    for row in trace() {
        let (owner, name) = id(&row);
        let lang = json(&content(&owner).join(format!("assets/{owner}/lang/en_us.json")));
        assert!(
            lang.get(format!("block.{owner}.{name}")).is_some(),
            "{owner}:{name}"
        );
        let key = row["bucket"]["defaultStack"]["components"]["minecraft:item_name"]["translate"]
            .as_str()
            .unwrap();
        assert!(lang.get(key).is_some(), "{key}");
        assert_eq!(key, format!("item.{owner}.{name}_bucket"));
    }
}
