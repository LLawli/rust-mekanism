//! O item dos tanques: as linhas do tooltip contra o rastro do original (`tank_fluid_item_use.jsonl`, as 11 linhas `tooltip`) e a troca do modo balde
//! (`ItemBlockFluidTank.changeMode`, `:186-195`, e `getScrollTextComponent`, `:198-200`).
use mekanism_rules::{
    DisplayChange, TankTooltip, bucket_mode_change, bucket_mode_scroll_text, description_key,
    name_color, tank_tooltip_lines,
};
use serde_json::{Value, json};
use std::path::Path;

const TIERS: [&str; 5] = ["basic", "advanced", "elite", "ultimate", "creative"];
/// `FluidTankTier` e `ChemicalTankTier`: a capacidade padrão.
const FLUID_CAPACITY: [u64; 5] = [32_000, 64_000, 128_000, 256_000, i64::MAX as u64];
const CHEMICAL_CAPACITY: [u64; 5] = [64_000, 256_000, 1_024_000, 8_192_000, i64::MAX as u64];

fn tooltips() -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/tank_fluid_item_use.jsonl");
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|line| line.get("tooltip").is_some())
        .collect()
}

#[test]
fn the_tooltip_lines_match_the_original() {
    let lines = tooltips();
    assert_eq!(lines.len(), 11, "os onze tooltips do rastro");
    let mut seen = 0;
    for line in lines {
        let spec = line["tooltip"].as_str().unwrap();
        let part: Vec<&str> = spec.split(':').collect();
        let chemical = part[0].starts_with("ctank");
        let tier = TIERS.iter().position(|tier| *tier == part[1]).unwrap();
        let full = part[0].ends_with("full");
        let amount: u64 = if full {
            i64::MAX as u64
        } else {
            part[3].parse().unwrap()
        };
        let id = format!(
            "{}:{}",
            if chemical { "mekanism" } else { "minecraft" },
            part[2]
        );
        let tip = TankTooltip {
            chemical,
            tier,
            stored: (amount > 0).then_some((id.as_str(), amount)),
            capacity: if chemical {
                CHEMICAL_CAPACITY[tier]
            } else {
                FLUID_CAPACITY[tier]
            },
            inventory: false,
            bucket_mode: match line["mode"].as_str().unwrap() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            },
        };
        let ours: Vec<Value> = tank_tooltip_lines(&tip)
            .iter()
            .map(|chat| chat.to_json())
            .collect();
        let theirs = line["lines"].as_array().unwrap();
        // A primeira linha é o nome, a segunda a segurança e a última a descrição: o resto é dos appenders dos tanques.
        assert_eq!(&ours[..], &theirs[2..theirs.len() - 1], "{spec}");
        seen += ours.len();
    }
    assert_eq!(
        seen,
        6 * 4 + 5 * 3,
        "quatro linhas por Fluid Tank e três por Chemical Tank"
    );
}

#[test]
fn an_inventory_shows_yes() {
    let tip = TankTooltip {
        chemical: false,
        tier: 0,
        stored: None,
        capacity: 32_000,
        inventory: true,
        bucket_mode: Some(false),
    };
    assert_eq!(
        tank_tooltip_lines(&tip)[2].to_json(),
        json!({"translate": "tooltip.mekanism.inventory", "with": [{"translate": "boolean.mekanism.yes", "color": "#CFCFCF"}], "color": "#30FFF9"})
    );
}

#[test]
fn an_odd_shift_toggles_the_bucket_mode() {
    for (shift, changes) in [
        (1, true),
        (-1, true),
        (3, true),
        (-5, true),
        (0, false),
        (2, false),
        (-4, false),
    ] {
        for mode in [false, true] {
            let change = bucket_mode_change(mode, shift, DisplayChange::None);
            assert_eq!(change.changed, changes, "shift {shift}");
            assert_eq!(
                change.mode,
                if changes { !mode } else { mode },
                "shift {shift}"
            );
            assert_eq!((change.overlay, change.show), (None, false));
        }
    }
}

#[test]
fn the_mode_change_tells_the_player_as_the_display_asks() {
    let main = bucket_mode_change(false, 1, DisplayChange::MainHand);
    assert!(main.show && main.overlay.is_none());
    let other = bucket_mode_change(false, 1, DisplayChange::Other);
    assert!(!other.show);
    assert_eq!(
        other.overlay.unwrap().to_json(),
        json!({"translate": "tooltip.mekanism.bucket_mode", "with": [{"translate": "boolean.mekanism.on", "color": "#75FF89"}]})
    );
    let off = bucket_mode_change(true, -1, DisplayChange::Other);
    assert_eq!(
        off.overlay.unwrap().to_json(),
        json!({"translate": "tooltip.mekanism.bucket_mode", "with": [{"translate": "boolean.mekanism.off", "color": "#FF383C"}]})
    );
    // Sem mudança não há aviso.
    let same = bucket_mode_change(true, 2, DisplayChange::Other);
    assert!(same.overlay.is_none() && !same.show);
}

#[test]
fn the_scroll_text_is_gray_with_the_coloured_state() {
    assert_eq!(
        bucket_mode_scroll_text(true).to_json(),
        json!({"translate": "tooltip.mekanism.bucket_mode", "with": [{"translate": "boolean.mekanism.on", "color": "#75FF89"}], "color": "#CFCFCF"})
    );
    assert_eq!(
        bucket_mode_scroll_text(false).to_json(),
        json!({"translate": "tooltip.mekanism.bucket_mode", "with": [{"translate": "boolean.mekanism.off", "color": "#FF383C"}], "color": "#CFCFCF"})
    );
}

/// O nome na cor do tier (a primeira linha) e a descrição que os cinco tiers dividem (a última), como o rastro os grava.
#[test]
fn the_name_colour_and_the_description_match_the_original() {
    for line in tooltips() {
        let part: Vec<&str> = line["tooltip"].as_str().unwrap().split(':').collect();
        let kind = if part[0].starts_with("ctank") {
            "chemical_tank"
        } else {
            "fluid_tank"
        };
        let item = format!("mekanism:{}_{kind}", part[1]);
        let lines = line["lines"].as_array().unwrap();
        let name = &lines[0]["extra"][0];
        assert_eq!(
            name["translate"],
            format!("block.mekanism.{}_{kind}", part[1])
        );
        let color = name_color(&item).unwrap_or_else(|| panic!("{item}: sem cor no nome"));
        assert_eq!(name["color"], format!("#{color:06X}"), "{item}");
        assert_eq!(
            lines[lines.len() - 1]["translate"],
            description_key(&item),
            "{item}"
        );
    }
}

/// O leite do NeoForge, que o Mekanism liga (`NeoForgeMod.enableMilkFluid`): o balde de leite do vanilla é o recipiente dele (`BucketResourceHandler.java:30-31`), o
/// nome é o do tipo de fluido, e as texturas são as do NeoForge, com os tempos de quadro de lá (`milk_still.png.mcmeta`, 3, e `milk_flowing.png.mcmeta`, 2).
#[test]
fn the_milk_of_neoforge_is_a_fluid_with_the_milk_bucket_and_its_own_textures() {
    use mekanism_rules::{
        MILK, MILK_BUCKET, contents_fluid, flowing_sprite, fluid_lang_key, vanilla_bucket_fluid,
        vanilla_bucket_sound, vanilla_fluid_bucket,
    };
    assert_eq!(MILK, "minecraft:milk");
    assert_eq!(vanilla_bucket_fluid(MILK_BUCKET), Some(MILK));
    assert_eq!(vanilla_fluid_bucket(MILK), Some("minecraft:milk_bucket"));
    assert_eq!(
        vanilla_bucket_fluid("minecraft:water_bucket"),
        Some("minecraft:water")
    );
    assert_eq!(
        vanilla_bucket_fluid("minecraft:lava_bucket"),
        Some("minecraft:lava")
    );
    assert_eq!(
        vanilla_fluid_bucket("minecraft:water"),
        Some("minecraft:water_bucket")
    );
    assert_eq!(
        vanilla_fluid_bucket("minecraft:lava"),
        Some("minecraft:lava_bucket")
    );
    // O balde vazio, o de neve em pó e os dos fluidos de mod não são daqui.
    for other in [
        "minecraft:bucket",
        "minecraft:powder_snow_bucket",
        "mekanism:brine_bucket",
    ] {
        assert_eq!(vanilla_bucket_fluid(other), None, "{other}");
    }
    assert_eq!(vanilla_fluid_bucket("mekanism:brine"), None);
    // Sem tinta e sem luz, e sem depender do arquivo de fluido do mod (o leite não tem bloco).
    let look = contents_fluid(MILK, None).unwrap();
    assert_eq!(look.sprite, "mekanism:block/milk_still");
    assert_eq!(
        (look.rgb, look.light, look.lighter_than_air),
        (0xFF_FF_FF, 0, false)
    );
    assert_eq!(flowing_sprite(&look.sprite), "mekanism:block/milk_flowing");
    assert_eq!(
        flowing_sprite("minecraft:block/water_still"),
        "minecraft:block/water_flow"
    );
    assert_eq!(
        flowing_sprite("mekanism:mek_liquid/brine"),
        "mekanism:mek_liquid/brine_flow"
    );
    // Um fluido desconhecido segue sem desenho.
    assert_eq!(contents_fluid("minecraft:honey", None), None);
    // Os sons do balde: os do leite têm as amostras dos da água.
    assert_eq!(
        vanilla_bucket_sound(MILK, true),
        Some("minecraft:item.bucket.fill")
    );
    assert_eq!(
        vanilla_bucket_sound(MILK, false),
        Some("minecraft:item.bucket.empty")
    );
    assert_eq!(
        vanilla_bucket_sound("minecraft:lava", true),
        Some("minecraft:item.bucket.fill_lava")
    );
    assert_eq!(
        vanilla_bucket_sound("minecraft:lava", false),
        Some("minecraft:item.bucket.empty_lava")
    );
    assert_eq!(
        vanilla_bucket_sound("minecraft:water", true),
        Some("minecraft:item.bucket.fill")
    );
    assert_eq!(vanilla_bucket_sound("mekanism:brine", true), None);
    // O nome (`FluidType.getDescriptionId`) e a tradução do NeoForge.
    assert_eq!(fluid_lang_key(MILK), "fluid_type.minecraft.milk");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let lang: Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("content/assets/mekanism/lang/en_us.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(lang["fluid_type.minecraft.milk"], "Milk");
    // As texturas e os tempos de quadro: 32 quadros de 16x16 e de 32x32.
    let textures = root.join("content/assets/mekanism/textures/block");
    for (name, frametime, bytes) in [("milk_still", 3, 1406), ("milk_flowing", 2, 3960)] {
        let png = std::fs::read(textures.join(format!("{name}.png"))).unwrap();
        assert_eq!(&png[1..4], b"PNG", "{name}");
        assert_eq!(png.len(), bytes, "{name}: o arquivo do NeoForge 26.3.0.100");
        let meta: Value = serde_json::from_str(
            &std::fs::read_to_string(textures.join(format!("{name}.png.mcmeta"))).unwrap(),
        )
        .unwrap();
        assert_eq!(meta["animation"]["frametime"], frametime, "{name}");
    }
}

/// `ItemBlockFluidTank.getMilkSound` (`:112-121`).
#[test]
fn each_milked_animal_has_its_sound() {
    use mekanism_rules::{Milked, milk_sound};
    assert_eq!(
        milk_sound(Milked::Cow, false),
        Some("minecraft:entity.cow.milk")
    );
    assert_eq!(
        milk_sound(Milked::Cow, true),
        Some("minecraft:entity.cow.milk")
    );
    assert_eq!(
        milk_sound(Milked::Mooshroom, false),
        Some("minecraft:entity.mooshroom.milk")
    );
    assert_eq!(
        milk_sound(Milked::Goat, false),
        Some("minecraft:entity.goat.milk")
    );
    assert_eq!(
        milk_sound(Milked::Goat, true),
        Some("minecraft:entity.goat.screaming.milk")
    );
}
