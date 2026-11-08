//! A cópia `.bak` que o NeoForge faz antes de corrigir um arquivo de configuração (`ConfigFileTypeHandler.backUpConfig`): `general.toml` guarda o de antes em
//! `general-1.toml.bak`, empurra as cópias velhas e solta a de número cinco. A rotação foi medida no original (`rastros/config_correcao/backups`).
use machinery_basis_core::client_config::{MAX_BACKUPS, back_up, backup_name};
use std::fs;
use std::path::PathBuf;

fn folder(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("config_backup")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_names_follow_the_original() {
    assert_eq!(backup_name("general.toml", 1), "general-1.toml.bak");
    assert_eq!(
        backup_name("machine-usage.toml", 5),
        "machine-usage-5.toml.bak"
    );
    assert_eq!(backup_name("noext", 2), "noext-2.bak");
}

#[test]
fn each_correction_pushes_the_older_copies_and_the_sixth_is_dropped() {
    let dir = folder("rotation");
    let file = dir.join("general.toml");
    // Sete correções: cada uma escreve um arquivo errado, copia-o e o substitui pelo corrigido.
    for run in 1..=7 {
        fs::write(&file, format!("blockDeactivationDelay = 500{run}\n")).unwrap();
        back_up(&file, MAX_BACKUPS).unwrap();
        fs::write(&file, "blockDeactivationDelay = 1200\n").unwrap();
    }
    let kept: Vec<String> = (1..=MAX_BACKUPS)
        .map(|n| fs::read_to_string(dir.join(backup_name("general.toml", n))).unwrap())
        .collect();
    // A mais nova em `-1`; as de 5001 e 5002 saíram.
    assert_eq!(
        kept,
        (3..=7)
            .rev()
            .map(|run| format!("blockDeactivationDelay = 500{run}\n"))
            .collect::<Vec<_>>()
    );
    assert!(!dir.join(backup_name("general.toml", 6)).exists());
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "blockDeactivationDelay = 1200\n"
    );
}

/// O original, corrigindo sete vezes seguidas o `general.toml` com `blockDeactivationDelay = 5001` a `5007` (`oficial_config_backups.sh`, sete execuções do
/// `runGameTestServer`), deixou cinco cópias: o mesmo que `back_up` deixa.
#[test]
fn the_copies_are_the_ones_the_original_left_after_seven_corrections() {
    let rastros = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../dev/referencia/rastros");
    let base = fs::read_to_string(rastros.join("mekanism_general.toml")).unwrap();
    let dir = folder("measured");
    let file = dir.join("general.toml");
    for run in 1..=7 {
        let wrong = base.replace(
            "blockDeactivationDelay = 60",
            &format!("blockDeactivationDelay = 500{run}"),
        );
        fs::write(&file, wrong).unwrap();
        back_up(&file, MAX_BACKUPS).unwrap();
        fs::write(&file, &base).unwrap();
    }
    for n in 1..=MAX_BACKUPS {
        let name = backup_name("general.toml", n);
        assert_eq!(
            fs::read_to_string(dir.join(&name)).unwrap(),
            fs::read_to_string(rastros.join("config_correcao/backups").join(&name)).unwrap(),
            "{name}"
        );
    }
}
