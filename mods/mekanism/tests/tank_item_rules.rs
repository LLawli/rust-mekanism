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
