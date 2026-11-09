//! `GeneralConfig.nutritionalPasteSaturation` e `nutritionalPasteMBPerFood` (`GeneralConfig.java:262-267`): a seção `nutritional_paste` do arquivo do
//! original (`rastros/mekanism_general.toml`, escrito por ele), a leitura dos dois valores e a conta de `PasteBucketConsumption.onConsume`
//! (`FluidType.BUCKET_VOLUME / mbPerFood` de fome e `saturation` de saturação).
use mekanism_rules::{GENERAL_CONFIG, PasteSettings, paste_settings};
use std::path::Path;

fn original_section() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/mekanism_general.toml");
    let text = std::fs::read_to_string(path).unwrap();
    let start = text
        .find("#Settings for configuring Nutritional Paste")
        .unwrap();
    let rest = &text[start..];
    rest[..rest.find("\n\n").unwrap()].to_owned()
}

#[test]
fn the_section_is_the_one_the_original_writes() {
    let ours = GENERAL_CONFIG.default_text();
    assert!(ours.contains(&original_section()), "{ours}");
}

#[test]
fn the_defaults_are_twenty_food_and_point_eight_saturation() {
    let loaded = GENERAL_CONFIG.load(&GENERAL_CONFIG.default_text());
    assert_eq!(paste_settings(&loaded), PasteSettings::DEFAULT);
    assert_eq!(
        PasteSettings::DEFAULT,
        PasteSettings {
            nutrition: 20,
            saturation: 0.8
        }
    );
}

#[test]
fn the_file_changes_the_nutrition_and_the_saturation() {
    let loaded =
        GENERAL_CONFIG.load("[nutritional_paste]\n\tsaturation = 2.5\n\tmbPerFood = 100\n");
    assert_eq!(
        paste_settings(&loaded),
        PasteSettings {
            nutrition: 10,
            saturation: 2.5
        }
    );
    // Fora do limite, o valor volta ao limite (`defineInRange` corrige): 1 mB por meio-prato é o máximo de fome (1000).
    let loaded = GENERAL_CONFIG.load("[nutritional_paste]\n\tmbPerFood = 0\n");
    assert_eq!(paste_settings(&loaded).nutrition, 1000);
}
