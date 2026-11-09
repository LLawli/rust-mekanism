// O `transform` de modelo (injeções `model_transform_decode` e `model_transform_bake`, módulo `model-transform`) no `horizon-assets` do pin, com o texto das injeções posto à mão
// (`dev/referencia/transform_horizon_assets.py` monta a cópia e roda este teste): o `advanced_solar_generator` e o `wind_generator` do original, a herança, o `composite` com
// filho que herda e filho com o próprio `transform`, a rotação do blockstate e o `transform` que o NeoForge recusa. Não roda no gate (precisa do código do Horizon).
use horizon_assets::{
    bake::{BlockModelTransform, ModelQuad, bake_block_model},
    model::CuboidModel,
    resolve::{AssetReader, ModelLibrary},
};
use std::{collections::BTreeMap, sync::Arc};

fn content() -> String {
    std::env::var("CONTENT").expect("CONTENT: mods/mekanismgenerators/content/assets")
}

struct Files;
impl AssetReader for Files {
    fn read_text(&self, location: &str) -> Option<String> {
        let (ns, path) = location.split_once(':')?;
        std::fs::read_to_string(format!("{}/{ns}/{path}", content())).ok()
    }
}

fn library(models: &[(&str, String)]) -> ModelLibrary {
    let mut map = BTreeMap::new();
    for (name, json) in models {
        map.insert((*name).to_owned(), CuboidModel::from_json(json).unwrap());
    }
    ModelLibrary::new(map).unwrap().with_assets(Arc::new(Files))
}

fn real(name: &str) -> (String, String) {
    (format!("mekanismgenerators:block/{name}"), std::fs::read_to_string(format!("{}/mekanismgenerators/models/block/{name}.json", content())).unwrap())
}

fn bake(lib: &ModelLibrary, name: &str, turns: [u8; 3]) -> Vec<ModelQuad> {
    bake_block_model(&lib.resolve(name).unwrap(), BlockModelTransform::new(turns, false)).unwrap()
}

fn bounds(quads: &[ModelQuad]) -> [f32; 6] {
    let mut b = [f32::MAX, f32::MAX, f32::MAX, f32::MIN, f32::MIN, f32::MIN];
    for q in quads {
        for p in &q.positions {
            for a in 0..3 {
                b[a] = b[a].min(p[a]);
                b[a + 3] = b[a + 3].max(p[a]);
            }
        }
    }
    b
}

#[test]
fn advanced_solar_rises_one_block() {
    let (name, json) = real("advanced_solar_generator");
    let without = json.replace("\"transform\"", "\"transform_x\"");
    let with = library(&[(&name, json.clone())]);
    let plain = library(&[(&name, without)]);
    let a = bake(&with, &name, [0; 3]);
    let b = bake(&plain, &name, [0; 3]);
    assert_eq!(a.len(), b.len());
    for (qa, qb) in a.iter().zip(&b) {
        for (pa, pb) in qa.positions.iter().zip(&qb.positions) {
            assert_eq!(pa[0], pb[0]);
            assert_eq!(pa[2], pb[2]);
            assert!((pa[1] - (pb[1] + 1.0)).abs() < 1e-6, "{pa:?} {pb:?}");
        }
        assert_eq!(qa.facing, qb.facing);
        assert_eq!(qa.cull, qb.cull);
        assert_eq!(qa.uv, qb.uv);
    }
    println!("bounds with {:?} without {:?}", bounds(&a), bounds(&b));
    // Rotated blockstate (y: 90 is [0,3,0] quarter turns in the Horizon): the translation turns with the model, so +1 in y stays +1.
    let a = bake(&with, &name, [0, 3, 0]);
    let b = bake(&plain, &name, [0, 3, 0]);
    for (qa, qb) in a.iter().zip(&b) {
        for (pa, pb) in qa.positions.iter().zip(&qb.positions) {
            assert!((pa[1] - (pb[1] + 1.0)).abs() < 1e-5);
            assert!((pa[0] - pb[0]).abs() < 1e-6 && (pa[2] - pb[2]).abs() < 1e-6);
        }
    }
}

#[test]
fn wind_generator_child_moves_half_a_block_and_the_other_child_stays() {
    let (name, json) = real("wind_generator");
    let without = json.replace("\"transform\"", "\"transform_x\"");
    let with = library(&[(&name, json.clone())]);
    let plain = library(&[(&name, without)]);
    for turns in [[0, 0, 0], [0, 1, 0], [0, 2, 0], [0, 3, 0]] {
        let a = bake(&with, &name, turns);
        let b = bake(&plain, &name, turns);
        assert_eq!(a.len(), b.len());
        let (mut moved, mut kept) = (0, 0);
        for (qa, qb) in a.iter().zip(&b) {
            let d: Vec<[f32; 3]> = qa.positions.iter().zip(&qb.positions).map(|(x, y)| [x[0] - y[0], x[1] - y[1], x[2] - y[2]]).collect();
            if d.iter().all(|v| v.iter().all(|c| c.abs() < 1e-6)) {
                kept += 1;
            } else {
                moved += 1;
                // The same shift on the four vertices, in the block frame rotated by the blockstate.
                for v in &d[1..] {
                    for a in 0..3 {
                        assert!((v[a] - d[0][a]).abs() < 1e-5);
                    }
                }
                assert!(d[0][1].abs() < 1e-6, "{turns:?} {:?}", d[0]);
                assert!((d[0][0].abs() - 0.5).abs() < 1e-5 && (d[0][2].abs() - 0.5).abs() < 1e-5, "{turns:?} {:?}", d[0]);
            }
        }
        println!("{turns:?}: moved {moved} kept {kept}");
        assert!(moved > 0 && kept > 0);
    }
}

#[test]
fn inheritance_and_nesting() {
    let cube = r##"{"textures":{"a":"block/stone"},"elements":[{"from":[0,0,0],"to":[16,16,16],"faces":{"up":{"texture":"#a","cullface":"up"},"down":{"texture":"#a"}}}]}"##;
    let lib = library(&[
        ("t:base", cube.to_owned()),
        ("t:plain_child", r#"{"parent":"t:base"}"#.to_owned()),
        ("t:moved_parent", r#"{"parent":"t:base","transform":{"translation":[0,2,0]}}"#.to_owned()),
        ("t:inherits", r#"{"parent":"t:moved_parent"}"#.to_owned()),
        ("t:overrides", r#"{"parent":"t:moved_parent","transform":{"translation":[1,0,0]}}"#.to_owned()),
        ("t:identity", r#"{"parent":"t:moved_parent","transform":"identity"}"#.to_owned()),
        (
            "t:composite",
            r#"{"loader":"neoforge:composite","transform":{"translation":[0,10,0]},"children":{"inherits":{"parent":"t:base"},"own":{"parent":"t:base","transform":{"translation":[0,0,3]}},"ref":{"parent":"t:moved_parent"}}}"#.to_owned(),
        ),
    ]);
    let top = |name: &str| bounds(&bake(&lib, name, [0; 3]));
    assert_eq!(top("t:plain_child"), [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
    assert_eq!(top("t:inherits"), [0.0, 2.0, 0.0, 1.0, 3.0, 1.0]);
    assert_eq!(top("t:overrides"), [1.0, 0.0, 0.0, 2.0, 1.0, 1.0]);
    assert_eq!(top("t:identity"), [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
    // composite: root +10 y; child "inherits" only gets it; "own" gets +3 z on top; "ref" (a model with its own +2 y) gets +2 inside the composite's +10.
    let quads = bake(&lib, "t:composite", [0; 3]);
    assert_eq!(quads.len(), 6);
    let per_child: Vec<[f32; 6]> = quads.chunks(2).map(bounds).collect();
    assert_eq!(per_child[0], [0.0, 10.0, 0.0, 1.0, 11.0, 1.0]);
    assert_eq!(per_child[1], [0.0, 10.0, 3.0, 1.0, 11.0, 4.0]);
    assert_eq!(per_child[2], [0.0, 12.0, 0.0, 1.0, 13.0, 1.0]);
    // The quads keep their cull side and facing for a translation.
    assert!(quads.iter().any(|q| q.cull.is_some()));
}

#[test]
fn rotation_turns_faces_and_culls() {
    let lib = library(&[(
        "t:turned",
        r##"{"textures":{"a":"block/stone"},"transform":{"rotation":[0,0,90],"origin":"center"},"elements":[{"from":[0,0,0],"to":[16,16,16],"faces":{"up":{"texture":"#a","cullface":"up"}}}]}"##.to_owned(),
    )]);
    let quads = bake(&lib, "t:turned", [0; 3]);
    assert_eq!(quads.len(), 1);
    println!("{:?} {:?}", quads[0].facing, quads[0].cull);
    assert_ne!(quads[0].facing, horizon_assets::model::Direction::Up);
    assert_eq!(quads[0].cull, Some(quads[0].facing));
}

#[test]
fn a_bad_transform_is_ignored() {
    // O valor de uma variável não devolve o erro: o `transform` que o NeoForge recusa fica sem efeito (e um aviso no log).
    let lib = library(&[("t:bad", r##"{"textures":{"a":"block/stone"},"transform":{"translation":[0,1]},"elements":[{"from":[0,0,0],"to":[16,16,16],"faces":{"up":{"texture":"#a"}}}]}"##.to_owned())]);
    assert_eq!(bounds(&bake(&lib, "t:bad", [0; 3])), [0.0, 1.0, 0.0, 1.0, 1.0, 1.0]);
    assert!(CuboidModel::from_json(r#"{"transform":"identity"}"#).is_ok());
}

#[test]
fn the_translation_turns_with_the_blockstate() {
    let cube = |extra: &str| format!(r##"{{"textures":{{"a":"block/stone"}},{extra}"elements":[{{"from":[0,0,0],"to":[16,16,16],"faces":{{"up":{{"texture":"#a"}}}}}}]}}"##);
    let lib = library(&[("t:moved", cube(r#""transform":{"translation":[1,0,0]},"#)), ("t:plain", cube(""))]);
    for turns in [[0, 0, 0], [0, 1, 0], [0, 2, 0], [0, 3, 0], [1, 0, 0], [0, 0, 1]] {
        let rows = BlockModelTransform::new(turns, false).matrix_rows();
        let expected = [rows[0][0], rows[1][0], rows[2][0]];
        let moved = bake(&lib, "t:moved", turns);
        let plain = bake(&lib, "t:plain", turns);
        for (a, b) in moved[0].positions.iter().zip(&plain[0].positions) {
            for axis in 0..3 {
                assert!((a[axis] - b[axis] - expected[axis]).abs() < 1e-5, "{turns:?}: {a:?} {b:?} {expected:?}");
            }
        }
    }
}
