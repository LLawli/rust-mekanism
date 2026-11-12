//! Os blocos de máquina, os blocos avulsos com tile, os de armazenamento, indução, multibloco e os transmissores do original (`MekanismBlocks`, `MekanismBlockTypes`, `BlockShapes`), sem função: as propriedades, os estados, as formas de
//! cada estado, a luz, a reação ao pistão e a oclusão que o port declara, contra o que o original deu em jogo (`rastros/blocos.jsonl`, `testes/BlocosTraceTest.java`).
//! `native/block_rows_table.rs`, que a injeção `active_light` aplica às linhas de jogo, é compilado aqui.
mod common;

#[path = "../native/block_rows_table.rs"]
mod rows;

use common::craft::{
    TIERS, blocos, cables, content, declared, json, structure_scope, transmitters,
};
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
/// A espessura do bloco que conecta de cada família de transmissor tiered (6 pixels o centro pequeno, 8 o grande) e a tag que a junta.
const TRANSMITTER_KINDS: [(&str, u32, &str); 4] = [
    ("pressurized_tube", 6, "chemical"),
    ("thermodynamic_conductor", 6, "heat"),
    ("mechanical_pipe", 8, "fluid"),
    ("logistical_transporter", 8, "items"),
];
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

/// Os 84 blocos da frente `blocos-maquinas`, na ordem em que o conteúdo os lista.
fn machine_scope() -> Vec<String> {
    let mut out: Vec<String> = MACHINES.iter().map(|id| (*id).to_owned()).collect();
    for tier in TIERS {
        for kind in FACTORIES {
            out.push(format!("{tier}_{kind}_factory"));
        }
    }
    out
}

fn scope() -> Vec<String> {
    let mut out = machine_scope();
    out.extend(structure_scope());
    out
}

/// O que a tabela de linhas leva: o escopo e os cabos.
fn rows_scope() -> Vec<String> {
    let mut out = scope();
    out.extend(cables());
    out
}

fn is_transmitter(id: &str) -> bool {
    cables().iter().any(|c| c == id) || transmitters().iter().any(|t| t == id)
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
    for id in rows_scope() {
        let row = &rastro[&id];
        let declaration = declaration(&id);
        if is_transmitter(&id) {
            // Um transmissor sem block entity tem só o centro (`BlockTransmitter.getShape` sem conexões): o cubo de 6 pixels (cabo, tubo, condutor) ou de 8 (cano, transportador),
            // que o `thickness` do bloco que conecta dá ao Horizon. Nenhuma forma própria.
            assert!(
                declaration.get("shapes").is_none()
                    && declaration.get("collision_shapes").is_none(),
                "{id}"
            );
            let thickness = declaration["thickness"].as_f64().unwrap();
            let half = thickness / 32.0;
            let center = [
                0.5 - half,
                0.5 - half,
                0.5 - half,
                0.5 + half,
                0.5 + half,
                0.5 + half,
            ];
            for state in row["states"].as_array().unwrap() {
                for kind in ["outline", "collision"] {
                    let want =
                        boxes(&row["shapes"][state["shape"].as_u64().unwrap() as usize][kind]);
                    assert_eq!(want, vec![center], "{id} {kind}");
                    compared += 1;
                }
            }
            continue;
        }
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
    assert!(compared >= 2_000, "{compared}");
}

/// Os blocos cuja forma, coberta por caixas que podem se sobrepor, passa de 16 caixas (os cinco cubos de energia entraram com a frente `blocos-estrutura`).
const OVER_THE_LIMIT: [&str; 17] = [
    "advanced_energy_cube",
    "basic_energy_cube",
    "chemical_infuser",
    "chemical_washer",
    "creative_energy_cube",
    "digital_miner",
    "electrolytic_separator",
    "elite_energy_cube",
    "isotopic_centrifuge",
    "logistical_sorter",
    "modification_station",
    "qio_exporter",
    "quantum_entangloporter",
    "rotary_condensentrator",
    "seismic_vibrator",
    "solar_neutron_activator",
    "ultimate_energy_cube",
];

/// A espessura do bloco que conecta de cada transmissor, a tag que junta a família e a cor do mapa que o original dá.
fn transmitter_facts(id: &str) -> (u32, String) {
    if id.ends_with("_universal_cable") {
        return (6, "mekanism:energy_connects".to_owned());
    }
    for (kind, thickness, tag) in TRANSMITTER_KINDS {
        if id.ends_with(kind) {
            return (thickness, format!("mekanism:transmitters/{tag}"));
        }
    }
    (8, "mekanism:transmitters/items".to_owned())
}

/// Os blocos que o original põe como `BlockMekanism` simples de bloco de entidade levam o gabarito `lodestone`; o vidro estrutural tem o som do vidro.
fn expected_sound(id: &str) -> &'static str {
    if id == "structural_glass" {
        "minecraft:glass"
    } else {
        "minecraft:stone"
    }
}

/// As propriedades de registro, as de estado e o item de cada bloco.
#[test]
fn every_block_is_declared_as_the_original_registers_it() {
    let rastro = original();
    let declared_blocks = declared("block");
    for id in rows_scope() {
        assert!(declared_blocks.contains(&id), "falta o bloco {id}");
        let row = &rastro[&id];
        let declaration = declaration(&id);
        let properties = &declaration["properties"];
        let first = &row["states"][0];
        assert_eq!(properties["hardness"], row["hardness"], "{id}: dureza");
        assert_eq!(properties["requires_tool"], row["requires_tool"], "{id}");
        assert_eq!(properties["map_color"], first["map_color"], "{id}: cor");
        // O som: `DEFAULT` do `SoundType` é a pedra (`BlockBehaviour.Properties`); só o vidro estrutural troca (`SoundType.GLASS`).
        let want_sound = expected_sound(&id);
        assert_eq!(
            first["sound"],
            format!(
                "minecraft:block.{}.break",
                want_sound.strip_prefix("minecraft:").unwrap()
            ),
            "{id}"
        );
        assert_eq!(properties["sounds"], want_sound, "{id}");
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
        // O item: a raridade (o SPS e a bobina supercarregada são épicos) e a pilha do original (16 no Cardboard Box); o `bounding_block` não tem item.
        let rarity = declaration["components"]["minecraft:rarity"].as_str();
        assert_eq!(
            row["item"]["rarity"],
            match rarity {
                Some("epic") => "EPIC",
                None => "COMMON",
                other => panic!("{id}: {other:?}"),
            },
            "{id}"
        );
        let stack = declaration["components"]["minecraft:max_stack_size"]
            .as_u64()
            .unwrap_or(64);
        assert_eq!(
            stack,
            row["item"]["max_stack"].as_u64().unwrap(),
            "{id}: pilha"
        );
        assert_eq!(
            declaration.get("item").and_then(Value::as_bool) == Some(false),
            row["item"]["id"] == "minecraft:air",
            "{id}: só o bloco de apoio não tem item"
        );
        // O desenho: só o `bounding_block` é invisível (`RenderShape.INVISIBLE`).
        assert_eq!(
            properties
                .get("render_shape")
                .and_then(Value::as_str)
                .unwrap_or("MODEL"),
            first["render_shape"],
            "{id}"
        );
        if is_transmitter(&id) {
            // O bloco que conecta do Horizon (o chorus plant sem as regras dele): a espessura, a tag da família, os seis lados por estado (a conexão do original vem da block entity, Fases 3
            // e 4), o `fluid_logged` do original com os mesmos valores e o mesmo padrão (decisão 27) e nenhuma oclusão declarada (o centro nunca é um cubo).
            let (thickness, tag) = transmitter_facts(&id);
            assert_eq!(declaration["template"], "minecraft:chorus_plant", "{id}");
            assert_eq!(declaration["thickness"], thickness, "{id}");
            assert_eq!(declaration["connects_to"], format!("#{tag}"), "{id}");
            assert_eq!(
                declaration["state_properties"],
                serde_json::json!({"fluid_logged": {"type": "enum", "values": ["empty", "water", "lava"]}}),
                "{id}"
            );
            assert_eq!(declaration["default_state"], row["default"], "{id}");
            assert_eq!(
                row["default"],
                serde_json::json!({"fluid_logged": "empty"}),
                "{id}"
            );
            assert!(properties.get("occludes").is_none(), "{id}");
            assert_eq!(properties["requires_tool"], false, "{id}");
            let names: Vec<&str> = row["properties"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| p["name"].as_str().unwrap())
                .collect();
            assert_eq!(names, ["fluid_logged"], "{id}");
            assert!(
                first["solid_render"] == false,
                "{id}: o centro não é um cubo"
            );
            continue;
        }
        // O gabarito: um bloco sem comportamento próprio, de reação normal ao pistão (a reação do original vem da tabela de linhas). O vidro estrutural (`BlockTileGlass`: sem oclusão,
        // sombra 1, sem a face entre dois vidros, forma visual vazia, som e instrumento do vidro) é o `minecraft:glass`, que dá tudo isso (decisão 26).
        let glass = id == "structural_glass";
        // O cubo de energia abre o menu pela injeção `use_hand` (decisões 47 e 68), não pelo gabarito: com o `beacon` o bloco de nota em cima tocava o `hat` do beacon, e o
        // original (instrumento padrão) toca o `harp`.
        assert_eq!(
            declaration["template"],
            if glass {
                "minecraft:glass"
            } else {
                "minecraft:lodestone"
            },
            "{id}"
        );
        // `occludes: false` só no bloco que o original marca com `noOcclusion` (`canOcclude` falso: o Nutritional Liquifier e o `bounding_block`); um bloco de forma menor que o cubo tem `canOcclude`
        // verdadeiro, e a forma de oclusão real dele entra em `occlusion.nbt` pela injeção `occlusion_shapes` (decisão 22). O vidro estrutural tem o gabarito `minecraft:glass`, sem oclusão.
        let occludes = row["states"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["can_occlude"] == true);
        assert_eq!(
            properties.get("occludes").and_then(Value::as_bool),
            if occludes || glass { None } else { Some(false) },
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
    }
}

/// A tabela das linhas de jogo (`block_rows_table.rs`) dá, para cada estado do original, a luz, a reação ao pistão e a oclusão que o original deu.
#[test]
fn the_rows_table_gives_every_state_what_the_original_gave() {
    let rastro = original();
    let mut states = 0;
    for id in rows_scope() {
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
            // A linha do port diz o `canOcclude` do original (verdadeiro, menos no Nutritional Liquifier, no vidro estrutural e no `bounding_block`): a face do vizinho é escondida pela forma de
            // oclusão real, que a injeção `occlusion_shapes` escreve (decisão 22).
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
    assert!(states >= 2_100, "{states}");
    // Nenhum bloco de fora entra na tabela.
    let ids: BTreeSet<String> = rows_scope()
        .iter()
        .map(|id| format!("mekanism:{id}"))
        .collect();
    assert_eq!(
        rows::BLOCKS
            .iter()
            .map(|f| f.id.to_owned())
            .collect::<BTreeSet<_>>(),
        ids
    );
}

/// `rastros/blocos_oclusao.jsonl` (`testes/OclusaoTraceTest.java`, o original em jogo): por bloco do Mekanism e do Generators, os estados e a tabela `facts` que eles apontam (a forma de oclusão e a de cada face,
/// `canOcclude`, `isSolid`, `canBeReplaced(fluid)` e `isPathfindable`).
fn oclusao() -> BTreeMap<String, Value> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/blocos_oclusao.jsonl");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| {
            let row: Value = serde_json::from_str(line).unwrap();
            (row["id"].as_str().unwrap().to_owned(), row)
        })
        .collect()
}

/// O que o port diz de `canOcclude` num bloco declarado: `occludes: false` (o `noOcclusion`), o gabarito `minecraft:glass` (já sem oclusão), e os canos e cabos, que a injeção `occlusion_shapes` põe
/// verdadeiro (o gabarito `chorus_plant` é `noOcclusion`).
fn port_can_occlude(id: &str, declaration: &Value) -> bool {
    if is_transmitter(id) {
        return true;
    }
    declaration["properties"]["occludes"] != serde_json::json!(false)
        && declaration["template"] != serde_json::json!("minecraft:glass")
}

/// A forma de oclusão que o port escreve em `occlusion.nbt` (a injeção `occlusion_shapes`): o centro de um cano ou cabo, a forma `shapes` declarada, ou o cubo cheio do gabarito.
fn port_occlusion(id: &str, declaration: &Value, facing: Option<&str>) -> Vec<[f64; 6]> {
    if is_transmitter(id) {
        let half = declaration["thickness"].as_f64().unwrap() / 32.0;
        return vec![[
            0.5 - half,
            0.5 - half,
            0.5 - half,
            0.5 + half,
            0.5 + half,
            0.5 + half,
        ]];
    }
    declared_shape(declaration, "shapes", facing)
}

/// O `canOcclude` e a forma de oclusão de cada estado são os do original: o `getOcclusionShape` do `Block` é a forma de seleção, e o `BlockTransmitter` a troca pelo centro. A linha da tabela de jogo
/// (`block_rows_table.rs`) diz o mesmo `canOcclude`.
#[test]
fn every_state_occludes_the_way_the_original_does() {
    let rastro = oclusao();
    let mut compared = 0;
    let mut occluding = 0;
    for id in rows_scope() {
        let row = &rastro[&format!("mekanism:{id}")];
        let declaration = declaration(&id);
        let facts = row["facts"].as_array().unwrap();
        for state in row["states"].as_array().unwrap() {
            let facing = state["props"].get("facing").and_then(Value::as_str);
            let want = &facts[state["facts"].as_u64().unwrap() as usize];
            let can = want["can_occlude"].as_bool().unwrap();
            assert_eq!(
                can,
                port_can_occlude(&id, &declaration),
                "{id} {:?}: canOcclude",
                state["props"]
            );
            assert_eq!(
                can,
                rows::facts(&format!("mekanism:{id}")).unwrap().can_occlude,
                "{id}: a tabela de linhas"
            );
            compared += 1;
            if can {
                assert!(
                    same_union(
                        &port_occlusion(&id, &declaration, facing),
                        &boxes(&want["occlusion"])
                    ),
                    "{id} {:?}: a forma de oclusão difere do original",
                    state["props"]
                );
                occluding += 1;
            }
        }
    }
    assert!(
        compared >= 1_000 && occluding >= 700,
        "{compared} {occluding}"
    );
}
