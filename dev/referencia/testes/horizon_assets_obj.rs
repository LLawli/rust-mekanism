// Os 22 transmissores desenhados pelo OBJ do original, no `horizon-assets` do pin, com o texto das injeções `obj_material_parse` e `obj_material_quads` e do módulo `obj-material` posto à mão
// (`dev/referencia/obj_horizon_assets.py` monta a cópia e roda este teste): cada uma das 12 partes de cada transmissor (o lado ligado e o desligado) tem os quads do objeto do OBJ
// (`transmitter_small.obj.mek` e `transmitter_large.obj.mek`), a cor `Kd 0.8` do `.mtl` em todos e o índice de tinta -1; o item tem as seis partes `NONE`; e a parte `downNORMAL` do cabo tem as posições
// e os uv do OBJ, com o v virado. Não roda no gate (precisa do código do Horizon). Sem as injeções a cor sai vazia e o teste falha.
use horizon_assets::{
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
        for side in SIDES {
            for (status, suffix) in [("NORMAL", "normal"), ("NONE", "none")] {
                let name = format!("mekanism:block/transmitter_part/{id}/{side}_{suffix}");
                let model = lib.resolve(&name).unwrap();
                let quads =
                    bake_block_model(&model, BlockModelTransform::new([0; 3], false)).unwrap();
                assert_eq!(quads.len(), counts[&format!("{side}{status}")], "{name}");
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
                }
                total_quads += quads.len();
                checked += 1;
            }
        }
        // O item: as seis partes NONE.
        let item = lib.resolve(&format!("mekanism:item/{id}")).unwrap();
        let quads = bake_block_model(&item, BlockModelTransform::new([0; 3], false)).unwrap();
        let want: usize = SIDES.iter().map(|s| counts[&format!("{s}NONE")]).sum();
        assert_eq!(quads.len(), want, "{id} item");
        assert!(quads.iter().all(|q| q.color == Some(0xFFCC_CCCCu32 as i32)));
    }
    assert_eq!(checked, 22 * 12);
    println!("{checked} partes, {total_quads} quads");
    let _ = STATUSES;
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
