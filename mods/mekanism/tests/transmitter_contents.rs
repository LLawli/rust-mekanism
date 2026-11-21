//! O conteúdo dos transmissores no mod (frente `cabo-conteudo`, Fase 4): os 24 modelos avulsos do conteúdo (um por grupo do OBJ do original), o sprite do calor, o que cada fluido
//! mostra e o que cada químico mostra.
use mekanism_rules as rules;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

fn content() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("content/assets/mekanism")
}

#[test]
fn there_is_one_standalone_model_per_obj_group_with_only_that_group_visible() {
    let obj =
        fs::read_to_string(content().join("models/block/transmitter_contents.obj.mek")).unwrap();
    let mut groups: Vec<&str> = obj
        .lines()
        .filter_map(|line| line.strip_prefix("g "))
        .collect();
    groups.sort_unstable();
    assert_eq!(
        groups,
        machinery_basis_core::transmitter_contents::all_groups()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .tap_sorted()
    );
    for group in &groups {
        let path = content().join(format!(
            "models/block/transmitter_contents/{}.json",
            group.to_lowercase()
        ));
        let model: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(model["loader"], "neoforge:obj");
        assert_eq!(
            model["model"],
            "mekanism:models/block/transmitter_contents.obj.mek"
        );
        assert_eq!(model["shade_quads"], false);
        let visible: Vec<&str> = model["visibility"]
            .as_object()
            .unwrap()
            .iter()
            .filter(|(_, shown)| shown.as_bool() == Some(true))
            .map(|(name, _)| name.as_str())
            .collect();
        assert_eq!(visible, [*group], "{group}");
        assert_eq!(model["visibility"].as_object().unwrap().len(), 24);
    }
}

trait TapSorted {
    fn tap_sorted(self) -> Self;
}

impl TapSorted for Vec<&str> {
    fn tap_sorted(mut self) -> Self {
        self.sort_unstable();
        self
    }
}

#[test]
fn the_heat_and_energy_sprites_are_animated_textures_of_the_original() {
    for name in ["energy", "heat"] {
        let png = content().join(format!("textures/mek_liquid/{name}.png"));
        assert!(png.exists(), "{name}");
        let meta: Value = serde_json::from_str(
            &fs::read_to_string(content().join(format!("textures/mek_liquid/{name}.png.mcmeta")))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(meta["animation"]["frametime"], 2, "{name}");
    }
    assert_eq!(rules::ENERGY_SPRITE, "mekanism:mek_liquid/energy");
    assert_eq!(rules::HEAT_SPRITE, "mekanism:mek_liquid/heat");
}

#[test]
fn the_fluids_with_their_own_texture_are_the_ones_that_have_the_file() {
    let mut files: Vec<String> = fs::read_dir(content().join("textures/mek_liquid"))
        .unwrap()
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter_map(|name| name.strip_suffix(".png").map(str::to_owned))
        .filter(|name| {
            !name.ends_with("_flow") && !["energy", "heat", "liquid"].contains(&name.as_str())
        })
        .collect();
    files.sort_unstable();
    let mut own: Vec<&str> = rules::FLUIDS_WITH_OWN_TEXTURE.to_vec();
    own.sort_unstable();
    assert_eq!(files, own);
}

#[test]
fn a_fluid_shows_the_sprite_the_tint_and_the_light_of_the_original() {
    let water = rules::contents_fluid("minecraft:water", None).unwrap();
    assert_eq!(
        (
            water.sprite.as_str(),
            water.rgb,
            water.light,
            water.lighter_than_air
        ),
        ("minecraft:block/water_still", 0x3F_76_E4, 0, false)
    );
    let lava = rules::contents_fluid("minecraft:lava", None).unwrap();
    assert_eq!(
        (lava.sprite.as_str(), lava.light),
        ("minecraft:block/lava_still", 15)
    );
    // Só o vapor é mais leve que o ar; o hidrogênio não tem textura própria e usa `liquid.png` com a cor do arquivo.
    let steam = rules::contents_fluid("mekanism:steam", Some((0xFFFFFF, 0))).unwrap();
    assert!(steam.lighter_than_air);
    assert_eq!(steam.sprite, "mekanism:mek_liquid/steam");
    let hydrogen = rules::contents_fluid("mekanism:hydrogen", Some((0xFFFFFF, 0))).unwrap();
    assert!(!hydrogen.lighter_than_air);
    assert_eq!(hydrogen.sprite, rules::LIQUID_SPRITE);
    let heavy_water = rules::contents_fluid("mekanism:heavy_water", Some((0x0D_14_55, 0))).unwrap();
    assert_eq!(
        (heavy_water.sprite.as_str(), heavy_water.rgb),
        ("minecraft:block/water_still", 0x0D_14_55)
    );
    // `getFluidTexture(fluid, FLOWING)`, o lado da válvula do Fluid Tank.
    assert_eq!(
        rules::flowing_sprite(&water.sprite),
        "minecraft:block/water_flow"
    );
    assert_eq!(
        rules::flowing_sprite(&lava.sprite),
        "minecraft:block/lava_flow"
    );
    assert_eq!(
        rules::flowing_sprite(&heavy_water.sprite),
        "minecraft:block/water_flow"
    );
    assert_eq!(
        rules::flowing_sprite(&steam.sprite),
        "mekanism:mek_liquid/steam_flow"
    );
    assert_eq!(
        rules::flowing_sprite(&hydrogen.sprite),
        "mekanism:mek_liquid/liquid_flow"
    );
    let sodium =
        rules::contents_fluid("mekanism:superheated_sodium", Some((0xFF_FF_FF, 15))).unwrap();
    assert_eq!(
        (sodium.sprite.as_str(), sodium.light, sodium.rgb),
        ("mekanism:mek_liquid/superheated_sodium", 15, 0xFF_FF_FF)
    );
    assert!(rules::contents_fluid("mekanism:unknown", None).is_none());
}

#[test]
fn a_chemical_shows_its_icon_with_the_tint_without_the_alpha() {
    assert_eq!(
        rules::contents_chemical(Some("mekanism:mek_chemical/slurry/clean"), 0xFF_F2_CD_67),
        ("mekanism:mek_chemical/slurry/clean".to_owned(), 0xF2_CD_67)
    );
    assert_eq!(
        rules::contents_chemical(None, 0xFF_00_FF_00),
        (rules::LIQUID_SPRITE.to_owned(), 0x00_FF_00)
    );
}

#[test]
fn the_generators_fluids_use_their_own_textures_with_the_light_of_the_file() {
    let ethanol =
        rules::contents_fluid("mekanismgenerators:bioethanol", Some((0x12_34_56, 15))).unwrap();
    assert_eq!(
        (
            ethanol.sprite.as_str(),
            ethanol.rgb,
            ethanol.light,
            ethanol.lighter_than_air
        ),
        (
            "mekanismgenerators:mek_liquid/bioethanol",
            0xFF_FFFF,
            15,
            false
        )
    );
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../mekanismgenerators/content/assets/mekanismgenerators/textures/mek_liquid");
    for name in rules::GENERATORS_FLUIDS {
        assert!(dir.join(format!("{name}.png")).exists(), "{name}");
    }
}

#[test]
fn a_chemical_keeps_the_light_level_of_its_file() {
    let dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("content/data/mekanism/mekanism/chemical");
    let mut with_light = 0;
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let json: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let id = format!("mekanism:{}", path.file_stem().unwrap().to_str().unwrap());
        let Some(resource) = rules::chemical_resource(&id, &json) else {
            continue;
        };
        let expected = json.get("light_level").and_then(Value::as_u64).unwrap_or(0) as u8;
        assert_eq!(resource.light, expected, "{id}");
        with_light += usize::from(expected > 0);
    }
    assert!(with_light >= 5, "{with_light}");
}

#[test]
fn the_contents_model_sits_inside_the_block_once_moved_by_the_pose_translation() {
    // O OBJ do conteúdo está em torno da origem (-0,5 a 0,5): sem o `translate(0.5, 0.5, 0.5)` do `RenderTransmitterBase.submit` o conteúdo sai meio bloco para baixo, para o norte e para oeste.
    let obj =
        fs::read_to_string(content().join("models/block/transmitter_contents.obj.mek")).unwrap();
    let origin = machinery_basis_core::transmitter_contents::CONTENTS_MODEL_ORIGIN;
    let mut vertices = 0;
    let (mut low, mut high) = ([f32::MAX; 3], [f32::MIN; 3]);
    for line in obj.lines().filter_map(|line| line.strip_prefix("v ")) {
        let coordinates: Vec<f32> = line
            .split_whitespace()
            .map(|word| word.parse().unwrap())
            .collect();
        for axis in 0..3 {
            low[axis] = low[axis].min(coordinates[axis] + origin[axis]);
            high[axis] = high[axis].max(coordinates[axis] + origin[axis]);
        }
        vertices += 1;
    }
    assert!(vertices > 100, "{vertices} vertices");
    for axis in 0..3 {
        assert!(
            low[axis] >= 0.0 && high[axis] <= 1.0,
            "axis {axis}: {} to {}",
            low[axis],
            high[axis]
        );
        // O conteúdo chega às duas faces do bloco: o modelo cobre o bloco todo, e não só um canto dele.
        assert!(
            high[axis] - low[axis] > 0.9,
            "axis {axis}: {} to {}",
            low[axis],
            high[axis]
        );
    }
}
