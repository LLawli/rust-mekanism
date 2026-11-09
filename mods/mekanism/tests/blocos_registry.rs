//! Os blocos de máquina e os blocos avulsos com tile do original (`MekanismBlocks`, `MekanismBlockTypes`, `BlockShapes`), sem função: as propriedades, os estados, as formas de
//! cada estado, a luz, a reação ao pistão e a oclusão que o port declara, contra o que o original deu em jogo (`rastros/blocos.jsonl`, `testes/BlocosTraceTest.java`).
//! `native/block_rows_table.rs`, que a injeção `active_light` aplica às linhas de jogo, é compilado aqui.
mod common;

#[path = "../native/block_rows_table.rs"]
mod rows;

use common::craft::{blocos, content, declared, json};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const MACHINES: [&str; 48] = [
    "antiprotonic_nucleosynthesizer",
    "chemical_crystallizer",
    "chemical_dissolution_chamber",
    "chemical_infuser",
    "chemical_injection_chamber",
    "chemical_oxidizer",
    "chemical_washer",
    "combiner",
    "digital_miner",
    "dimensional_stabilizer",
    "electric_pump",
    "electrolytic_separator",
    "fluidic_plenisher",
    "formulaic_assemblicator",
    "fuelwood_heater",
    "isotopic_centrifuge",
    "laser",
    "laser_amplifier",
    "laser_tractor_beam",
    "logistical_sorter",
    "modification_station",
    "nutritional_liquifier",
    "oredictionificator",
    "osmium_compressor",
    "painting_machine",
    "pigment_extractor",
    "pigment_mixer",
    "precision_sawmill",
    "pressurized_reaction_chamber",
    "purification_chamber",
    "qio_dashboard",
    "qio_drive_array",
    "qio_exporter",
    "qio_importer",
    "qio_redstone_adapter",
    "quantum_entangloporter",
    "resistive_heater",
    "rotary_condensentrator",
    "security_desk",
    "seismic_vibrator",
    "solar_neutron_activator",
    "teleporter",
    "chargepad",
    "industrial_alarm",
    "radioactive_waste_barrel",
    "personal_barrel",
    "personal_chest",
    "cardboard_box",
];
const TIERS: [&str; 4] = ["basic", "advanced", "elite", "ultimate"];
const FACTORIES: [&str; 9] = [
    "combining",
    "compressing",
    "crushing",
    "enriching",
    "infusing",
    "injecting",
    "purifying",
    "sawing",
    "smelting",
];

/// Os 84 blocos da frente, na ordem em que o conteúdo os lista.
fn scope() -> Vec<String> {
    let mut out: Vec<String> = MACHINES.iter().map(|id| (*id).to_owned()).collect();
    for tier in TIERS {
        for kind in FACTORIES {
            out.push(format!("{tier}_{kind}_factory"));
        }
    }
    out
}

/// O original em jogo, por id sem namespace.
fn original() -> BTreeMap<String, Value> {
    blocos()
        .into_iter()
        .map(|row| {
            (
                row["id"]
                    .as_str()
                    .unwrap()
                    .strip_prefix("mekanism:")
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
            .join("data/mekanism/horizon/block")
            .join(format!("{id}.json")),
    )
}

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
    // O Horizon aceita 16 caixas por forma; a injeção `shape_limit` tira o limite dos blocos cuja forma coberta passa dele.
    assert_eq!(
        over.iter().map(String::as_str).collect::<Vec<_>>(),
        OVER_THE_LIMIT
    );
    assert!(compared >= 1_600, "{compared}");
}

/// Os blocos cuja forma, coberta por caixas que podem se sobrepor, passa de 16 caixas.
const OVER_THE_LIMIT: [&str; 12] = [
    "chemical_infuser",
    "chemical_washer",
    "digital_miner",
    "electrolytic_separator",
    "isotopic_centrifuge",
    "logistical_sorter",
    "modification_station",
    "qio_exporter",
    "quantum_entangloporter",
    "rotary_condensentrator",
    "seismic_vibrator",
    "solar_neutron_activator",
];

/// As propriedades de registro, as de estado e o item de cada bloco.
#[test]
fn every_block_is_declared_as_the_original_registers_it() {
    let rastro = original();
    let declared_blocks = declared("block");
    for id in scope() {
        assert!(declared_blocks.contains(&id), "falta o bloco {id}");
        let row = &rastro[&id];
        let declaration = declaration(&id);
        // O gabarito: um bloco sem comportamento próprio, de reação normal ao pistão (a reação do original vem da tabela de linhas).
        assert_eq!(declaration["template"], "minecraft:lodestone", "{id}");
        let properties = &declaration["properties"];
        let first = &row["states"][0];
        assert_eq!(properties["hardness"], row["hardness"], "{id}: dureza");
        assert_eq!(properties["requires_tool"], row["requires_tool"], "{id}");
        assert_eq!(properties["map_color"], first["map_color"], "{id}: cor");
        // `DEFAULT` do `SoundType` é a pedra (`BlockBehaviour.Properties`): nenhum bloco troca o som.
        assert_eq!(first["sound"], "minecraft:block.stone.break", "{id}");
        assert_eq!(properties["sounds"], "minecraft:stone", "{id}");
        assert_eq!(row["friction"], 0.6, "{id}");
        assert!(properties.get("friction").is_none(), "{id}");
        // A resistência: a que as explosões leem (`getExplosionResistance(state, ...)`). `AttributeCustomResistance(-1)` não cabe no arquivo e vai pela tabela de linhas.
        let blast = row["explosion_state"].as_f64().unwrap();
        if blast >= 0.0 {
            assert_eq!(
                properties["explosion_resistance"], row["explosion_state"],
                "{id}"
            );
            assert_eq!(row["explosion_state"], row["resistance"], "{id}");
            assert!(
                rows::facts(&format!("mekanism:{id}"))
                    .unwrap()
                    .explosion
                    .is_none()
            );
        } else {
            assert!(properties.get("explosion_resistance").is_none(), "{id}");
            assert_eq!(
                rows::facts(&format!("mekanism:{id}")).unwrap().explosion,
                Some(blast as f32),
                "{id}"
            );
        }
        // `occludes: false` nos blocos de forma menor que o cubo (`isSolidRender` falso no original): o Horizon esconde a face do vizinho pela forma de oclusão do gabarito (o cubo cheio)
        // e pelo `can_occlude` de `render-definitions.json`; só `occludes: false` o faz parar (decisão 22).
        let solid = row["states"].as_array().unwrap().iter().all(|s| s["solid_render"] == true);
        assert_eq!(
            properties.get("occludes").and_then(Value::as_bool),
            if solid { None } else { Some(false) },
            "{id}: occludes"
        );
        // A luz fixa (só o Teleporter): `AttributeLight(3)`. A do estado ativo e a da lava vêm da tabela.
        let fixed: Vec<u64> = row["states"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| {
                s["props"].get("active").is_none_or(|a| a == "false")
                    && s["props"].get("fluid_logged").is_none_or(|f| f == "empty")
            })
            .map(|s| s["light"].as_u64().unwrap())
            .collect();
        let fixed = fixed.into_iter().max().unwrap();
        assert_eq!(
            properties.get("light").and_then(Value::as_u64).unwrap_or(0),
            fixed,
            "{id}: luz fixa"
        );
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
        // O item: a raridade é a comum e a pilha é a do original (16 no Cardboard Box).
        assert_eq!(row["item"]["rarity"], "COMMON", "{id}");
        let stack = declaration["components"]["minecraft:max_stack_size"]
            .as_u64()
            .unwrap_or(64);
        assert_eq!(
            stack,
            row["item"]["max_stack"].as_u64().unwrap(),
            "{id}: pilha"
        );
    }
}

/// A tabela das linhas de jogo (`block_rows_table.rs`) dá, para cada estado do original, a luz, a reação ao pistão e a oclusão que o original deu.
#[test]
fn the_rows_table_gives_every_state_what_the_original_gave() {
    let rastro = original();
    let mut states = 0;
    for id in scope() {
        let row = &rastro[&id];
        let declaration = declaration(&id);
        let full = format!("mekanism:{id}");
        let facts = rows::facts(&full).unwrap_or_else(|| panic!("{id} sem linha na tabela"));
        let base = declaration["properties"]
            .get("light")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u8;
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
            // A luz de `properties.light` vale em todo estado e a injeção roda antes dela; o resultado é o maior dos dois.
            let light = rows::light(facts, &combo, 0).max(base);
            assert_eq!(
                u64::from(light),
                state["light"].as_u64().unwrap(),
                "{id} {:?}",
                state["props"]
            );
            assert_eq!(facts.push, state["push"], "{id}");
            // O original dá `canOcclude` verdadeiro (menos no Nutritional Liquifier) e o Horizon esconde a face do vizinho por ele, sem olhar a forma: a linha do port o dá só ao cubo
            // cheio (`solid_render`), senão a grama sob a máquina some (decisão 22). O resto do original está na linha.
            assert_eq!(facts.can_occlude, state["solid_render"], "{id}");
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
    assert!(states >= 1_700, "{states}");
    // Nenhum bloco de fora entra na tabela.
    let ids: BTreeSet<String> = scope().iter().map(|id| format!("mekanism:{id}")).collect();
    assert_eq!(
        rows::BLOCKS
            .iter()
            .map(|f| f.id.to_owned())
            .collect::<BTreeSet<_>>(),
        ids
    );
}
