//! Os sons das máquinas portadas são os do original: a entrada de cada um no `sounds.json` (arquivo, legenda, `attenuation_distance` ausente, que dá os 16
//! blocos do `Sound.getAttenuationDistance`) e os bytes do `.ogg` (tamanho e FNV-1a de 64 bits contra o arquivo do original). `rastros/mekanism_sounds.json`
//! e `rastros/mekanism_sound_files.json` vêm do `datagen/generated` e de `src/main/resources` do commit de referência.
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
fn the_ported_sound_events_are_the_originals() {
    let port: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join("content/assets/mekanism/sounds.json")).unwrap(),
    )
    .unwrap();
    let original = reference("mekanism_sounds.json");
    assert_eq!(original.as_object().unwrap().len(), 8);
    for (name, entry) in original.as_object().unwrap() {
        assert_eq!(&port[name], entry, "{name}");
        assert!(
            entry.get("attenuation_distance").is_none(),
            "{name}: o padrão é 16"
        );
    }
    // O port não traz outro evento além dos que o original tem.
    assert_eq!(port.as_object().unwrap().len(), 8);
}

#[test]
fn the_ported_sound_files_are_the_originals() {
    let files = reference("mekanism_sound_files.json");
    assert_eq!(files.as_object().unwrap().len(), 8);
    for (name, expected) in files.as_object().unwrap() {
        let bytes =
            std::fs::read(root().join("content/assets/mekanism/sounds").join(name)).unwrap();
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
