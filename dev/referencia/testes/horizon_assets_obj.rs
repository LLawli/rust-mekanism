// Os 22 transmissores desenhados pelo OBJ do original, no `horizon-assets` do pin, com o texto das injeções `obj_material_parse` e `obj_material_quads` e do módulo `obj-material`, e as do
// `transform` de modelo, postos à mão (`dev/referencia/obj_horizon_assets.py` monta a cópia e roda este teste): cada parte de cada transmissor (um modelo por lado e por estado visual do
// `TransmitterModelData`: `NORMAL`, `PUSH`, `PULL`, `NONE` e as duas do trecho reto onde há) tem os quads do objeto do OBJ (`transmitter_small.obj.mek` e `transmitter_large.obj.mek`), a cor
// `Kd 0.8` do `.mtl` em todos e o índice de tinta -1; o centro do trecho reto usa o sprite do braço (`#side`, o `NoneSegmentRemap`) e o girado tem as posições da `Transformation` de
// `makeIconStatusTransform` (`rastros/transmitter_model_parts.json`, o original); o item tem as seis partes `NONE`; e a parte `downNORMAL` do cabo tem as posições e os uv do OBJ, com o v
// virado. Não roda no gate (precisa do código do Horizon). Sem as injeções a cor sai vazia e o teste falha.
use horizon_assets::{
    blockstate::{
        BlockStateDefinition,
        bind::{Properties, PropertySchema, PropertyValues, SelectedModels},
    },
    bake::{BlockModelTransform, bake_block_model},
    model::CuboidModel,
    resolve::{AssetReader, ModelLibrary},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
};

fn content() -> PathBuf {
    PathBuf::from(std::env::var("CONTENT").expect("CONTENT: mods/mekanism/content"))
}
struct Root(PathBuf);
impl AssetReader for Root {
    fn read_text(&self, location: &str) -> Option<String> {
        let (ns, path) = location.split_once(':')?;
        std::fs::read_to_string(self.0.join(format!("assets/{ns}/{path}"))).ok()
    }
}
fn models() -> BTreeMap<String, CuboidModel> {
    let base = content().join("assets/mekanism/models");
    let mut out = BTreeMap::new();
    out.insert(
        "minecraft:block/block".to_owned(),
        CuboidModel::from_json("{}").unwrap(),
    );
    let mut stack = vec![base.clone()];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            if p.extension().is_some_and(|x| x == "json") {
                let rel = p.strip_prefix(&base).unwrap().with_extension("");
                let id = format!("mekanism:{}", rel.to_string_lossy());
                if let Ok(m) = CuboidModel::from_json(&std::fs::read_to_string(&p).unwrap()) {
                    out.insert(id, m);
                }
            }
        }
    }
    out
}
const SIDES: [&str; 6] = ["down", "up", "north", "south", "west", "east"];
const STATUSES: [&str; 4] = ["NONE", "NORMAL", "PUSH", "PULL"];
fn faces_per_object(path: &Path) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    let mut cur = String::new();
    for l in std::fs::read_to_string(path).unwrap().lines() {
        if let Some(n) = l.strip_prefix("o ") {
            cur = n.to_owned();
        }
        if l.starts_with("f ") {
            *out.entry(cur.clone()).or_insert(0) += 1;
        }
    }
    out
}
fn ids() -> Vec<String> {
    let mut v = vec![];
    for f in [
        "universal_cable",
        "pressurized_tube",
        "thermodynamic_conductor",
        "mechanical_pipe",
        "logistical_transporter",
    ] {
        for t in ["basic", "advanced", "elite", "ultimate"] {
            v.push(format!("{t}_{f}"));
        }
    }
    v.push("restrictive_transporter".into());
    v.push("diversion_transporter".into());
    v
}

/// `VisualConnectionStatus.VALUES`: o fim do nome do modelo e a parte do OBJ que cada um mostra.
const VISUAL: [(&str, &str); 6] = [
    ("normal", "NORMAL"),
    ("push", "PUSH"),
    ("pull", "PULL"),
    ("none", "NONE"),
    ("none_contiguous", "NONE"),
    ("none_contiguous_rotated", "NONE"),
];

fn rastro(name: &str) -> serde_json::Value {
    let path = PathBuf::from(std::env::var("RASTROS").expect("RASTROS: dev/referencia/rastros")).join(name);
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn every_part_bakes_with_the_obj_geometry_the_kd_and_a_real_sprite() {
    let lib = ModelLibrary::new(models())
        .unwrap()
        .with_assets(Arc::new(Root(content())));
    let small =
        faces_per_object(&content().join("assets/mekanism/models/block/transmitter_small.obj.mek"));
    let large =
        faces_per_object(&content().join("assets/mekanism/models/block/transmitter_large.obj.mek"));
    let mut checked = 0;
    let mut total_quads = 0;
    for id in ids() {
        let counts = if id.contains("cable") || id.contains("tube") || id.contains("conductor") {
            &small
        } else {
            &large
        };
        // Só o desviador não tem as partes do trecho reto (`hideContiguousJoin` falso).
        let straight = id != "diversion_transporter";
        for side in SIDES {
            let mut centre: Vec<String> = Vec::new();
            for (suffix, part) in VISUAL {
                let contiguous = suffix.starts_with("none_contiguous");
                let name = format!("mekanism:block/transmitter_part/{id}/{side}_{suffix}");
                if contiguous && !straight {
                    continue;
                }
                let model = lib.resolve(&name).unwrap();
                let quads =
                    bake_block_model(&model, BlockModelTransform::new([0; 3], false)).unwrap();
                assert_eq!(quads.len(), counts[&format!("{side}{part}")], "{name}");
                for q in &quads {
                    assert_eq!(q.color, Some(0xFFCC_CCCCu32 as i32), "{name}");
                    assert_eq!(q.tint_index, -1, "{name}");
                    assert!(
                        q.material
                            .sprite
                            .starts_with("mekanism:block/models/multipart/"),
                        "{name} {}",
                        q.material.sprite
                    );
                    // O centro (`NONE`) usa a textura de centro; no trecho reto o `NoneSegmentRemap` põe a do braço (`#side`, o arquivo `_vertical`).
                    let vertical = q.material.sprite.ends_with("_vertical");
                    if part == "NONE" {
                        assert_eq!(vertical, contiguous, "{name} {}", q.material.sprite);
                    }
                }
                if part == "NONE" {
                    centre.push(suffix.to_owned());
                }
                total_quads += quads.len();
                checked += 1;
            }
            assert_eq!(centre.len(), if straight { 3 } else { 1 }, "{id} {side}");
        }
        // O item: as seis partes NONE.
        let item = lib.resolve(&format!("mekanism:item/{id}")).unwrap();
        let quads = bake_block_model(&item, BlockModelTransform::new([0; 3], false)).unwrap();
        let want: usize = SIDES.iter().map(|s| counts[&format!("{s}NONE")]).sum();
        assert_eq!(quads.len(), want, "{id} item");
        assert!(quads.iter().all(|q| q.color == Some(0xFFCC_CCCCu32 as i32)));
    }
    assert_eq!(checked, 21 * 36 + 24);
    println!("{checked} partes, {total_quads} quads");
    let _ = STATUSES;
}

/// O blockstate de cada transmissor é um `multipart` que o Horizon aceita (as condições `AND` que nenhum estado cumpre) e, ligado aos estados do bloco (os seis lados e o `fluid_logged`), escolhe
/// por padrão as seis partes `NONE`: os índices `lado * partes por lado + 3`.
#[test]
fn the_blockstate_of_every_transmitter_selects_the_none_part_of_each_side_by_default() {
    let sides = ["down", "up", "north", "south", "west", "east"];
    let mut schema: PropertySchema = BTreeMap::new();
    for side in sides {
        schema.insert(
            side.to_owned(),
            PropertyValues::Named(["false", "true"].iter().map(|v| (*v).to_owned()).collect()),
        );
    }
    schema.insert(
        "fluid_logged".to_owned(),
        PropertyValues::Named(["empty", "water", "lava"].iter().map(|v| (*v).to_owned()).collect()),
    );
    let mut states: Vec<Properties> = Vec::new();
    for connected in [false, true] {
        let mut state = Properties::new();
        for side in sides {
            state.insert(side.to_owned(), connected.to_string());
        }
        state.insert("fluid_logged".to_owned(), "empty".to_owned());
        states.push(state);
    }
    for id in ids() {
        let text = std::fs::read_to_string(content().join(format!("assets/mekanism/blockstates/{id}.json"))).unwrap();
        let bound = BlockStateDefinition::from_json(&text).unwrap().bind(&schema, &states).unwrap();
        let per_side = if id == "diversion_transporter" { 4 } else { 6 };
        let want: Vec<usize> = (0..6).map(|side| side * per_side + 3).collect();
        assert_eq!(bound.len(), 2, "{id}");
        for selected in bound {
            assert_eq!(selected, Some(SelectedModels::Multipart(want.clone())), "{id}");
        }
    }
}

/// A parte girada do trecho reto: os vértices do centro (`NONE`) levados pela `Transformation` de `makeIconStatusTransform` (a matriz que o original gravou), em volta do centro do bloco.
#[test]
fn the_rotated_straight_part_has_the_vertices_of_the_none_part_turned_like_the_original() {
    let lib = ModelLibrary::new(models())
        .unwrap()
        .with_assets(Arc::new(Root(content())));
    let transforms = rastro("transmitter_model_parts.json");
    let mut moved = 0;
    for entry in transforms["icon_transforms"].as_array().unwrap() {
        let side = entry["side"].as_str().unwrap();
        let rows: Vec<Vec<f32>> = entry["matrix"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row.as_array().unwrap().iter().map(|v| v.as_f64().unwrap() as f32).collect())
            .collect();
        for id in ["basic_universal_cable", "ultimate_mechanical_pipe"] {
            let bake = |suffix: &str| {
                let model = lib.resolve(&format!("mekanism:block/transmitter_part/{id}/{side}_{suffix}")).unwrap();
                bake_block_model(&model, BlockModelTransform::new([0; 3], false)).unwrap()
            };
            let flat = bake("none_contiguous");
            let turned = bake("none_contiguous_rotated");
            assert_eq!(flat.len(), turned.len(), "{id} {side}");
            assert!(!flat.is_empty());
            for (a, b) in flat.iter().zip(&turned) {
                for (p, q) in a.positions.iter().zip(&b.positions) {
                    let v = [p[0] - 0.5, p[1] - 0.5, p[2] - 0.5];
                    let want: Vec<f32> = rows
                        .iter()
                        .map(|row| row[0] * v[0] + row[1] * v[1] + row[2] * v[2] + row[3] + 0.5)
                        .collect();
                    for axis in 0..3 {
                        assert!((q[axis] - want[axis]).abs() < 1e-5, "{id} {side} {p:?} -> {q:?} contra {want:?}");
                    }
                    if p != q {
                        moved += 1;
                    }
                }
                // Os uv seguem o vértice: a textura gira com a face.
                assert_eq!(a.uv, b.uv, "{id} {side}");
            }
        }
    }
    // Girar não é ficar no lugar: o teste pode falhar.
    assert!(moved > 0);
}

/// A parte `downNORMAL` do cabo: posições e uv do OBJ, na mesma ordem das faces, com o v virado (`flip_v`).
#[test]
fn a_part_has_the_vertices_of_its_obj_object() {
    let lib = ModelLibrary::new(models())
        .unwrap()
        .with_assets(Arc::new(Root(content())));
    let model = lib
        .resolve("mekanism:block/transmitter_part/basic_universal_cable/down_normal")
        .unwrap();
    let quads = bake_block_model(&model, BlockModelTransform::new([0; 3], false)).unwrap();
    // As posições do OBJ do objeto.
    let text = std::fs::read_to_string(
        content().join("assets/mekanism/models/block/transmitter_small.obj.mek"),
    )
    .unwrap();
    let mut v: Vec<[f32; 3]> = vec![];
    let mut vt: Vec<[f32; 2]> = vec![];
    let mut cur = "";
    let mut faces: Vec<Vec<([f32; 3], [f32; 2])>> = vec![];
    for l in text.lines() {
        let w: Vec<&str> = l.split_whitespace().collect();
        match w.first().copied() {
            Some("o") => cur = w[1],
            Some("v") => v.push([
                w[1].parse().unwrap(),
                w[2].parse().unwrap(),
                w[3].parse().unwrap(),
            ]),
            Some("vt") => vt.push([w[1].parse().unwrap(), w[2].parse().unwrap()]),
            Some("f") if cur == "downNORMAL" => faces.push(
                w[1..]
                    .iter()
                    .map(|c| {
                        let p: Vec<usize> = c.split('/').map(|x| x.parse().unwrap()).collect();
                        (v[p[0] - 1], vt[p[1] - 1])
                    })
                    .collect(),
            ),
            _ => {}
        }
    }
    assert_eq!(faces.len(), quads.len());
    for (f, q) in faces.iter().zip(&quads) {
        for i in 0..4 {
            let (p, uv) = f[i.min(f.len() - 1)];
            assert_eq!(q.positions[i], p);
            assert_eq!(q.uv[i], [uv[0], 1.0 - uv[1]]);
        }
    }
}
