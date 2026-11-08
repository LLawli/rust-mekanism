//! `LocalUnit`, o `MekanismConfig.local.tempUnit` do cliente em arquivo: a primeira leitura, a releitura quando o arquivo muda por fora,
//! a gravação do clique e a preservação das outras chaves.
use machinery_basis_core::temperature::{
    LOCAL_CONFIG_FILE, LocalUnit, RELOAD_EVERY, TemperatureUnit, unit_from_toml,
};
use std::fs::{self, File};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

/// Uma pasta própria do teste, dentro do `target` (nunca no `/tmp`).
fn folder(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("local_unit")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Reescreve o arquivo por fora e marca a data dele, que o sistema de arquivos pode arredondar.
fn edit(path: &PathBuf, text: &str, seconds: u64) {
    fs::write(path, text).unwrap();
    File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
        .unwrap();
}

#[test]
fn without_a_file_the_unit_is_kelvin() {
    let dir = folder("missing");
    let mut local = LocalUnit::new(dir.join(LOCAL_CONFIG_FILE));
    assert_eq!(local.get(Instant::now()), TemperatureUnit::Kelvin);
}

#[test]
fn the_first_read_takes_the_unit_from_the_file() {
    let dir = folder("first");
    let path = dir.join(LOCAL_CONFIG_FILE);
    edit(&path, "temperatureUnit = \"CELSIUS\"\n", 1_000);
    let mut local = LocalUnit::new(path);
    assert_eq!(local.get(Instant::now()), TemperatureUnit::Celsius);
}

#[test]
fn an_edit_from_outside_is_noticed_once_a_second_not_every_frame() {
    let dir = folder("outside");
    let path = dir.join(LOCAL_CONFIG_FILE);
    edit(&path, "temperatureUnit = \"CELSIUS\"\n", 1_000);
    let start = Instant::now();
    let mut local = LocalUnit::new(path.clone());
    assert_eq!(local.get(start), TemperatureUnit::Celsius);
    edit(&path, "temperatureUnit = \"RANKINE\"\n", 2_000);
    // Antes de passar um segundo da última olhada, a unidade é a de antes.
    assert_eq!(
        local.get(start + RELOAD_EVERY - Duration::from_millis(1)),
        TemperatureUnit::Celsius
    );
    assert_eq!(local.get(start + RELOAD_EVERY), TemperatureUnit::Rankine);
}

#[test]
fn a_file_that_did_not_change_is_not_read_again() {
    let dir = folder("unchanged");
    let path = dir.join(LOCAL_CONFIG_FILE);
    edit(&path, "temperatureUnit = \"CELSIUS\"\n", 1_000);
    let start = Instant::now();
    let mut local = LocalUnit::new(path.clone());
    assert_eq!(local.get(start), TemperatureUnit::Celsius);
    // O conteúdo muda, mas a data não: sem mudança de data, não relê.
    edit(&path, "temperatureUnit = \"RANKINE\"\n", 1_000);
    assert_eq!(
        local.get(start + RELOAD_EVERY * 5),
        TemperatureUnit::Celsius
    );
}

#[test]
fn a_removed_file_goes_back_to_the_default() {
    let dir = folder("removed");
    let path = dir.join(LOCAL_CONFIG_FILE);
    edit(&path, "temperatureUnit = \"FAHRENHEIT\"\n", 1_000);
    let start = Instant::now();
    let mut local = LocalUnit::new(path.clone());
    assert_eq!(local.get(start), TemperatureUnit::Fahrenheit);
    fs::remove_file(&path).unwrap();
    assert_eq!(local.get(start + RELOAD_EVERY), TemperatureUnit::Kelvin);
}

#[test]
fn setting_the_unit_writes_the_file_keeps_the_other_keys_and_applies_at_once() {
    let dir = folder("set");
    let path = dir.join("mekanism").join(LOCAL_CONFIG_FILE);
    let start = Instant::now();
    let mut local = LocalUnit::new(path.clone());
    // Sem pasta nem arquivo: o primeiro clique cria os dois.
    local.set(TemperatureUnit::Celsius, start);
    assert_eq!(local.get(start), TemperatureUnit::Celsius);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "temperatureUnit = \"CELSIUS\"\n"
    );
    edit(
        &path,
        "compactEnergyDisplay = false\ntemperatureUnit = \"CELSIUS\"\nholidays = true\n",
        3_000,
    );
    local.set(TemperatureUnit::Ambient, start);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "compactEnergyDisplay = false\ntemperatureUnit = \"AMBIENT\"\nholidays = true\n"
    );
    assert_eq!(
        unit_from_toml(&fs::read_to_string(&path).unwrap()),
        TemperatureUnit::Ambient
    );
    // A gravação não conta como edição de fora: a unidade segue a mesma na olhada seguinte.
    assert_eq!(
        local.get(start + RELOAD_EVERY * 2),
        TemperatureUnit::Ambient
    );
}

/// Depois do clique, a data do arquivo que o clique gravou já é a conhecida: um arquivo com a mesma data não é relido.
#[test]
fn the_file_the_click_wrote_is_not_read_back() {
    let dir = folder("set_not_reread");
    let path = dir.join(LOCAL_CONFIG_FILE);
    let start = Instant::now();
    let mut local = LocalUnit::new(path.clone());
    local.set(TemperatureUnit::Rankine, start);
    let written = fs::metadata(&path).unwrap().modified().unwrap();
    // O conteúdo muda por fora, com a mesma data que o clique deixou.
    fs::write(&path, "temperatureUnit = \"CELSIUS\"\n").unwrap();
    File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(written)
        .unwrap();
    assert_eq!(
        local.get(start + RELOAD_EVERY * 2),
        TemperatureUnit::Rankine
    );
}

#[test]
fn holidays_are_read_from_the_same_file_and_follow_an_edit() {
    let dir = folder("holidays");
    let path = dir.join(LOCAL_CONFIG_FILE);
    let start = Instant::now();
    // Sem arquivo: ligados (`define("holidays", true)`).
    let mut local = LocalUnit::new(path.clone());
    assert!(local.holidays(start));
    // O arquivo aparece desligado, e a releitura (no máximo uma por segundo) o vê.
    edit(
        &path,
        "temperatureUnit = \"CELSIUS\"\nholidays = false\n",
        1_000,
    );
    assert!(
        local.holidays(start + RELOAD_EVERY / 2),
        "antes de passar um segundo vale o de antes"
    );
    assert!(!local.holidays(start + RELOAD_EVERY));
    // A unidade e os feriados vêm da mesma leitura.
    assert_eq!(local.get(start + RELOAD_EVERY), TemperatureUnit::Celsius);
    edit(&path, "holidays = true\n", 2_000);
    assert!(local.holidays(start + RELOAD_EVERY * 2));
}
