//! O som do Heat Generator é o do original: a entrada de `tile.generator.heat` no `sounds.json` (arquivo, legenda, `attenuation_distance` ausente, que dá os 16 blocos do
//! `Sound.getAttenuationDistance`), o `.ogg` byte a byte (tamanho e FNV-1a de 64 bits contra o arquivo do original) e a legenda no idioma. `rastros/generators_sounds.json` e
//! `rastros/generators_sound_files.json` vêm do `datagen/generated` e de `src/generators/resources` do commit de referência.
use serde_json::Value;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn reference(name: &str) -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(root().join("../../dev/referencia/rastros").join(name)).unwrap(),
    )
    .unwrap()
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[test]
fn the_ported_sound_event_is_the_originals() {
    let port: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join("content/assets/mekanismgenerators/sounds.json"))
            .unwrap(),
    )
    .unwrap();
    let original = reference("generators_sounds.json");
    assert_eq!(original.as_object().unwrap().len(), 1);
    for (name, entry) in original.as_object().unwrap() {
        assert_eq!(&port[name], entry, "{name}");
        assert!(
            entry.get("attenuation_distance").is_none(),
            "{name}: o padrão é 16"
        );
    }
    // O port só traz o evento do gerador portado.
    assert_eq!(port.as_object().unwrap().len(), 1);
    assert_eq!(
        mekanismgenerators_rules::HEAT_GENERATOR_SOUND,
        "mekanismgenerators:tile.generator.heat"
    );
}

#[test]
fn the_ported_sound_file_is_the_originals() {
    let files = reference("generators_sound_files.json");
    assert_eq!(files.as_object().unwrap().len(), 1);
    for (name, expected) in files.as_object().unwrap() {
        let bytes = std::fs::read(
            root()
                .join("content/assets/mekanismgenerators/sounds")
                .join(name),
        )
        .unwrap();
        assert_eq!(
            bytes.len() as u64,
            expected[0].as_u64().unwrap(),
            "{name}: tamanho"
        );
        assert_eq!(
            format!("{:016x}", fnv1a(&bytes)),
            expected[1].as_str().unwrap(),
            "{name}: bytes"
        );
    }
}

#[test]
fn the_subtitle_of_the_event_is_in_the_language_file() {
    let lang: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join("content/assets/mekanismgenerators/lang/en_us.json"))
            .unwrap(),
    )
    .unwrap();
    let sounds = reference("generators_sounds.json");
    let subtitle = sounds["tile.generator.heat"]["subtitle"].as_str().unwrap();
    assert_eq!(lang[subtitle], "Heat generator hums");
}
