//! Os blocos do Mekanism Generators (`GeneratorsBlocks`, `GeneratorsBlockTypes`, `BlockShapes`), sem função: as propriedades, os estados, as formas de cada estado, a luz, a
//! reação ao pistão e a oclusão que o port declara, contra o que o original deu em jogo (`rastros/blocos_generators.jsonl`, `testes/BlocosTraceTest.java`). O
//! `native/block_rows_table.rs`, que a injeção `block_rows` aplica às linhas de jogo, é compilado aqui. O `heat_generator` já existia (gabarito do forno, Fase 5) e fica de fora.
#[path = "common/generators.rs"]
mod generators;

#[path = "../native/block_rows_table.rs"]
mod rows;

use generators::{blocos, content, declared, json, scope};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// O original em jogo, por id sem namespace.
fn original() -> BTreeMap<String, Value> {
    blocos()
        .into_iter()
        .map(|row| {
            (
                row["id"]
                    .as_str()
                    .unwrap()
                    .strip_prefix("mekanismgenerators:")
                    .unwrap()
                    .to_owned(),
                row,
            )
        })
        .collect()
}

fn declaration(id: &str) -> Value {
    json(
        &content()
            .join("data/mekanismgenerators/horizon/block")
            .join(format!("{id}.json")),
    )
}

/// Os dois blocos de `BlockTileGlass` (o som, o instrumento e o `noOcclusion` do vidro): o gabarito é o vidro do vanilla.
const GLASS: [&str; 2] = ["reactor_glass", "laser_focus_matrix"];

/// Uma caixa `[x1, y1, z1, x2, y2, z2]` de um array JSON.
fn boxes(value: &Value) -> Vec<[f64; 6]> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            let b = b.as_array().unwrap();
            std::array::from_fn(|i| b[i].as_f64().unwrap())
        })
        .collect()
}

/// Se duas listas de caixas cobrem o mesmo volume: a grade dos cortes das duas, célula a célula.
fn same_union(a: &[[f64; 6]], b: &[[f64; 6]]) -> bool {
    let mut cuts: [Vec<f64>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for shape in [a, b] {
        for bx in shape {
            for axis in 0..3 {
                cuts[axis].push(bx[axis]);
                cuts[axis].push(bx[axis + 3]);
            }
        }
    }
    for axis in &mut cuts {
        axis.sort_by(f64::total_cmp);
        axis.dedup();
    }
    let inside = |shape: &[[f64; 6]], p: [f64; 3]| {
        shape
            .iter()
            .any(|bx| (0..3).all(|axis| bx[axis] <= p[axis] && p[axis] < bx[axis + 3]))
    };
    for x in cuts[0].windows(2) {
        for y in cuts[1].windows(2) {
            for z in cuts[2].windows(2) {
                let p = [x[0], y[0], z[0]];
                if inside(a, p) != inside(b, p) {
                    return false;
                }
            }
        }
    }
    true
}

const FULL: [f64; 6] = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];

/// As caixas que o port declara para o `facing` (ou para o bloco inteiro, sem `facing`) numa forma `shapes` ou `collision_shapes`, ou o cubo cheio se o bloco não a declara.
fn declared_shape(declaration: &Value, key: &str, facing: Option<&str>) -> Vec<[f64; 6]> {
    let Some(shapes) = declaration.get(key) else {
        return vec![FULL];
    };
    match facing {
        Some(facing) => {
            assert_eq!(shapes["properties"], serde_json::json!(["facing"]));
            boxes(&shapes["values"][facing])
        }
        None => boxes(&shapes["default"]),
    }
}

/// Os 23 blocos da frente estão no registro do original, e o registro do original tem só eles mais o `heat_generator`.
#[test]
fn the_scope_is_the_registry_of_the_original_without_the_heat_generator() {
    let rastro = original();
    assert_eq!(rastro.len(), 24);
    assert_eq!(scope().len(), 23);
    assert!(rastro.contains_key("heat_generator"));
    for id in scope() {
        assert!(rastro[&id]["has_block_entity"] == true, "{id}");
        assert!(declared("block").contains(&id), "falta o bloco {id}");
    }
}

/// As formas de seleção e de colisão de cada estado são a união das caixas do original, e passam de 16 caixas só nos blocos da lista.
#[test]
fn every_state_has_the_shape_the_original_gives() {
    let rastro = original();
    let mut over = BTreeSet::new();
    let mut compared = 0;
    for id in scope() {
        let row = &rastro[&id];
        let declaration = declaration(&id);
        for state in row["states"].as_array().unwrap() {
            let facing = state["props"].get("facing").and_then(Value::as_str);
            for (kind, key) in [("outline", "shapes"), ("collision", "collision_shapes")] {
                let want = boxes(&row["shapes"][state["shape"].as_u64().unwrap() as usize][kind]);
                // A colisão cai na seleção quando o arquivo não tem `collision_shapes` (`b.collision_shapes.or(b.shapes)`).
                let got = if kind == "collision" && declaration.get(key).is_none() {
                    declared_shape(&declaration, "shapes", facing)
                } else {
                    declared_shape(&declaration, key, facing)
                };
                if got.len() > 16 {
                    over.insert(id.clone());
                }
                assert!(
                    same_union(&got, &want),
                    "{id} {:?}: a forma de {kind} difere do original",
                    state["props"]
                );
                compared += 1;
            }
        }
    }
    // O Horizon aceita 16 caixas por forma; a injeção `shape_limit` (do Mekanism) tira o limite dos blocos cuja forma coberta passa dele.
    assert_eq!(
        over.iter().map(String::as_str).collect::<Vec<_>>(),
        OVER_THE_LIMIT
    );
    assert!(compared >= 250, "{compared}");
}

/// Os blocos cuja forma, coberta por caixas que podem se sobrepor, passa de 16 caixas.
const OVER_THE_LIMIT: [&str; 2] = ["control_rod_assembly", "wind_generator"];

/// As propriedades de registro, as de estado e o item de cada bloco.
#[test]
fn every_block_is_declared_as_the_original_registers_it() {
    let rastro = original();
    for id in scope() {
        let row = &rastro[&id];
        let declaration = declaration(&id);
        let glass = GLASS.contains(&id.as_str());
        // O gabarito: um bloco sem comportamento próprio (a reação ao pistão do original vem da tabela de linhas), ou o vidro nos dois blocos de vidro.
        assert_eq!(
            declaration["template"],
            if glass {
                "minecraft:glass"
            } else {
                "minecraft:lodestone"
            },
            "{id}"
        );
        let properties = &declaration["properties"];
        let first = &row["states"][0];
        assert_eq!(properties["hardness"], row["hardness"], "{id}: dureza");
        assert_eq!(properties["requires_tool"], row["requires_tool"], "{id}");
        assert_eq!(properties["map_color"], first["map_color"], "{id}: cor");
        // `BlockTileGlass` e `BlockLaserFocusMatrix` trocam o som e o instrumento pelos do vidro (`HAT`, que o gabarito `minecraft:glass` já dá); os outros ficam com a pedra.
        assert_eq!(
            first["sound"],
            if glass {
                "minecraft:block.glass.break"
            } else {
                "minecraft:block.stone.break"
            },
            "{id}"
        );
        assert_eq!(
            properties["sounds"],
            if glass {
                "minecraft:glass"
            } else {
                "minecraft:stone"
            },
            "{id}"
        );
        assert_eq!(
            first["instrument"],
            if glass { "HAT" } else { "HARP" },
            "{id}"
        );
        assert_eq!(row["friction"], 0.6, "{id}");
        assert!(properties.get("friction").is_none(), "{id}");
        // A resistência que as explosões leem: nenhum bloco do Generators tem `AttributeCustomResistance`.
        assert_eq!(
            properties["explosion_resistance"], row["explosion_state"],
            "{id}"
        );
        assert_eq!(row["explosion_state"], row["resistance"], "{id}");
        assert!(
            rows::facts(&format!("mekanismgenerators:{id}"))
                .unwrap()
                .explosion
                .is_none()
        );
        // `occludes: false` só nos blocos que o original marca com `noOcclusion` (`canOcclude` falso: os dois de vidro); um bloco de forma menor que o cubo tem `canOcclude` verdadeiro, e a forma de
        // oclusão real dele entra em `occlusion.nbt` pela injeção `occlusion_shapes` (decisão 22 do plano).
        let occludes = row["states"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["can_occlude"] == true);
        assert_eq!(
            properties.get("occludes").and_then(Value::as_bool),
            if occludes { None } else { Some(false) },
            "{id}: occludes"
        );
        // Nenhum bloco do Generators tem luz fixa: a do estado ativo e a da lava vêm da tabela.
        assert!(properties.get("light").is_none(), "{id}");
        let fixed = row["states"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| {
                s["props"].get("active").is_none_or(|a| a == "false")
                    && s["props"].get("fluid_logged").is_none_or(|f| f == "empty")
            })
            .map(|s| s["light"].as_u64().unwrap())
            .max()
            .unwrap();
        assert_eq!(fixed, 0, "{id}: luz fixa");
        // As propriedades de estado: as mesmas, com os mesmos valores, e o estado padrão do original.
        let mut want_properties = BTreeMap::new();
        for property in row["properties"].as_array().unwrap() {
            let values: BTreeSet<String> = property["values"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect();
            want_properties.insert(property["name"].as_str().unwrap().to_owned(), values);
        }
        let mut got_properties = BTreeMap::new();
        if let Some(own) = declaration.get("state_properties") {
            for (name, property) in own.as_object().unwrap() {
                let values: BTreeSet<String> = if property["type"] == "boolean" {
                    ["false", "true"].map(str::to_owned).into()
                } else {
                    property["values"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_str().unwrap().to_owned())
                        .collect()
                };
                got_properties.insert(name.clone(), values);
            }
        }
        assert_eq!(
            got_properties, want_properties,
            "{id}: propriedades de estado"
        );
        let default = declaration
            .get("default_state")
            .cloned()
            .unwrap_or(Value::Object(Default::default()));
        assert_eq!(default, row["default"], "{id}: estado padrão");
        assert_eq!(
            row["states"].as_array().unwrap().len(),
            want_properties
                .values()
                .map(BTreeSet::len)
                .product::<usize>(),
            "{id}: quantidade de estados"
        );
        // O item: a raridade é a comum e a pilha é a de 64.
        assert_eq!(row["item"]["rarity"], "COMMON", "{id}");
        assert_eq!(row["item"]["max_stack"], 64, "{id}: pilha");
        assert!(declaration.get("components").is_none(), "{id}");
    }
}

/// A tabela das linhas de jogo (`block_rows_table.rs`) dá, para cada estado do original, a luz, a reação ao pistão e a oclusão que o original deu.
#[test]
fn the_rows_table_gives_every_state_what_the_original_gave() {
    let rastro = original();
    let mut states = 0;
    for id in scope() {
        let row = &rastro[&id];
        let full = format!("mekanismgenerators:{id}");
        let facts = rows::facts(&full).unwrap_or_else(|| panic!("{id} sem linha na tabela"));
        for state in row["states"].as_array().unwrap() {
            let combo: Vec<(String, String)> = state["props"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
                .collect();
            let wet = combo
                .iter()
                .any(|(k, v)| k == "fluid_logged" && v != "empty");
            assert_eq!(
                u64::from(rows::light(facts, &combo, 0)),
                state["light"].as_u64().unwrap(),
                "{id} {:?}",
                state["props"]
            );
            assert_eq!(facts.push, state["push"], "{id}");
            // A linha do port diz o `canOcclude` do original (verdadeiro, menos nos dois blocos de vidro): a face do vizinho é escondida pela forma de oclusão real, que a injeção
            // `occlusion_shapes` escreve (decisão 22).
            assert_eq!(facts.can_occlude, state["can_occlude"], "{id}");
            assert!(
                state["can_occlude"] == true || state["solid_render"] == false,
                "{id}"
            );
            assert_eq!(facts.solid_render, state["solid_render"], "{id}");
            let (dampening, skylight) = if wet {
                (facts.wet_dampening, facts.wet_skylight_down)
            } else {
                (facts.dampening, facts.skylight_down)
            };
            assert_eq!(
                u64::from(dampening),
                state["dampening"].as_u64().unwrap(),
                "{id} {:?}",
                state["props"]
            );
            assert_eq!(
                skylight, state["skylight_down"],
                "{id} {:?}",
                state["props"]
            );
            states += 1;
        }
    }
    assert_eq!(states, 148);
    // Nenhum bloco de fora entra na tabela.
    let ids: BTreeSet<String> = scope()
        .iter()
        .map(|id| format!("mekanismgenerators:{id}"))
        .collect();
    assert_eq!(
        rows::BLOCKS
            .iter()
            .map(|f| f.id.to_owned())
            .collect::<BTreeSet<_>>(),
        ids
    );
}

/// A luz do estado ativo de cada gerador, que o original soma ao 0 do bloco: o Heat Generator (fora desta tabela), o Bio-Generator e o Gas-Burning Generator acendem com
/// `ACTIVE_MELT_LIGHT` (13), o Wind Generator com a luz padrão da máquina (8), e os dois solares (`Attributes.ACTIVE`) e os multiblocos não acendem.
#[test]
fn the_active_light_is_the_one_of_each_block_type() {
    let lit: BTreeMap<&str, u8> = rows::BLOCKS
        .iter()
        .filter(|f| f.active_light > 0)
        .map(|f| {
            (
                f.id.strip_prefix("mekanismgenerators:").unwrap(),
                f.active_light,
            )
        })
        .collect();
    assert_eq!(
        lit,
        BTreeMap::from([
            ("bio_generator", 13),
            ("gas_burning_generator", 13),
            ("wind_generator", 8)
        ])
    );
}

/// O `canOcclude` e a forma de oclusão de cada estado são os do original (`rastros/blocos_oclusao.jsonl`, `testes/OclusaoTraceTest.java`): o `getOcclusionShape` do `Block` é a forma de seleção, que a
/// injeção `occlusion_shapes` escreve em `occlusion.nbt`; os dois blocos de vidro têm o gabarito `minecraft:glass`, sem oclusão.
#[test]
fn every_state_occludes_the_way_the_original_does() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/blocos_oclusao.jsonl");
    let rastro: BTreeMap<String, Value> = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| {
            let row: Value = serde_json::from_str(line).unwrap();
            (row["id"].as_str().unwrap().to_owned(), row)
        })
        .collect();
    let (mut compared, mut occluding) = (0, 0);
    for id in scope() {
        let row = &rastro[&format!("mekanismgenerators:{id}")];
        let declaration = declaration(&id);
        let facts = row["facts"].as_array().unwrap();
        for state in row["states"].as_array().unwrap() {
            let facing = state["props"].get("facing").and_then(Value::as_str);
            let want = &facts[state["facts"].as_u64().unwrap() as usize];
            let can = want["can_occlude"].as_bool().unwrap();
            assert_eq!(
                can,
                declaration["properties"]["occludes"] != serde_json::json!(false)
                    && declaration["template"] != serde_json::json!("minecraft:glass"),
                "{id} {:?}: canOcclude",
                state["props"]
            );
            compared += 1;
            if can {
                assert!(
                    same_union(
                        &declared_shape(&declaration, "shapes", facing),
                        &boxes(&want["occlusion"])
                    ),
                    "{id} {:?}: a forma de oclusão difere do original",
                    state["props"]
                );
                occluding += 1;
            }
        }
    }
    assert!(compared >= 100 && occluding >= 80, "{compared} {occluding}");
}

/// O Heat Generator (o gabarito `minecraft:furnace`, da Fase 5) tem `canOcclude` verdadeiro e a forma de oclusão da seleção no original: sem `occludes: false`, e a forma declarada é a do original.
#[test]
fn the_heat_generator_occludes_by_its_shape() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/blocos_oclusao.jsonl");
    let text = std::fs::read_to_string(&path).unwrap();
    let row: Value = text
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|row| row["id"] == "mekanismgenerators:heat_generator")
        .unwrap();
    let declaration = declaration("heat_generator");
    assert!(declaration["properties"].get("occludes").is_none());
    let facts = row["facts"].as_array().unwrap();
    for state in row["states"].as_array().unwrap() {
        let facing = state["props"].get("facing").and_then(Value::as_str);
        let want = &facts[state["facts"].as_u64().unwrap() as usize];
        assert_eq!(want["can_occlude"], true);
        assert!(
            same_union(
                &declared_shape(&declaration, "shapes", facing),
                &boxes(&want["occlusion"])
            ),
            "{:?}",
            state["props"]
        );
    }
}

/// A tabela `FURNACE`, do `heat_generator` que segue com o gabarito `minecraft:furnace` (a injeção `block_rows` aplica a reação ao pistão e a oclusão dela), diz em todo estado, seco e com fluido dentro,
/// o que o original deu.
#[test]
fn the_furnace_table_gives_the_heat_generator_what_the_original_gave() {
    let rastro = original();
    let row = &rastro["heat_generator"];
    let facts = rows::furnace_facts("mekanismgenerators:heat_generator").unwrap();
    assert_eq!(rows::FURNACE.len(), 1);
    assert!(rows::facts("mekanismgenerators:heat_generator").is_none());
    let mut wet_states = 0;
    for state in row["states"].as_array().unwrap() {
        let wet = state["props"]["fluid_logged"] != "empty";
        let (dampening, skylight) = if wet {
            (facts.wet_dampening, facts.wet_skylight_down)
        } else {
            (facts.dampening, facts.skylight_down)
        };
        assert_eq!(facts.push, state["push"]);
        assert_eq!(facts.can_occlude, state["can_occlude"]);
        assert_eq!(facts.solid_render, state["solid_render"]);
        assert_eq!(u64::from(dampening), state["dampening"].as_u64().unwrap());
        assert_eq!(skylight, state["skylight_down"]);
        wet_states += usize::from(wet);
    }
    assert_eq!(wet_states, 16);
    // O `BlockTileModel` é de forma menor que o cubo: o gabarito do forno não é.
    assert!(!facts.solid_render);
}
