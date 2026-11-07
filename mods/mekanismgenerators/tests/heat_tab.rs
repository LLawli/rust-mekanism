//! A aba de calor (`GuiHeatTab`) do Heat Generator sem o jogo: o retângulo, o clique que troca a unidade, o sprite de cada unidade, o
//! idioma e as linhas do tooltip, contra `dev/referencia/rastros/temperature_units.jsonl` (o `TemperatureUnit`, o
//! `getTemperatureDisplay` e as linhas que o original monta, gravados no jogo).
use machinery_basis_core::temperature::{
    CONFIG_KEY, LOCAL_CONFIG_FILE, TemperatureUnit, temperature_display,
};
use machinery_basis_core::units::format_lang;
use mekanismgenerators_rules::{
    DISSIPATED_RATE_LANG, HEAT_CAPACITY, HEAT_TAB, TEMPERATURE_LANG, TRANSFERRED_RATE_LANG,
    UNIT_LANG, heat_tab_click, heat_tab_sprite, heat_tab_tooltip, unit_lang,
};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn trace(kind: &str) -> Vec<Value> {
    let path = root().join("dev/referencia/rastros/temperature_units.jsonl");
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|line| line["kind"] == kind)
        .collect()
}

fn unit(name: &Value) -> TemperatureUnit {
    TemperatureUnit::from_name(name.as_str().unwrap()).unwrap()
}

fn lang() -> Value {
    let path = root().join("mods/mekanism/content/assets/mekanism/lang/en_us.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn english(key: &str) -> String {
    lang()[key]
        .as_str()
        .unwrap_or_else(|| panic!("{key}"))
        .to_owned()
}

#[test]
fn the_tab_is_the_texture_element_at_minus_26_109() {
    assert_eq!(HEAT_TAB.rect, [-26.0, 109.0, 26.0, 26.0]);
    assert!(HEAT_TAB.is_mouse_over([-26.0, 109.0], false));
    assert!(HEAT_TAB.is_mouse_over([-1.0, 134.0], false));
    assert!(!HEAT_TAB.is_mouse_over([0.0, 120.0], false));
    assert!(!HEAT_TAB.is_mouse_over([-27.0, 120.0], false));
    assert!(!HEAT_TAB.is_mouse_over([-10.0, 108.0], false));
    assert!(!HEAT_TAB.is_mouse_over([-10.0, 135.0], false));
    // Uma janela por cima do ponto esconde a aba.
    assert!(!HEAT_TAB.is_mouse_over([-10.0, 120.0], true));
}

#[test]
fn the_left_button_advances_the_unit_and_the_right_one_goes_back() {
    let on_tab = [-13.0, 122.0];
    let mut seen = Vec::new();
    let mut current = TemperatureUnit::Kelvin;
    for _ in 0..6 {
        current = heat_tab_click(current, on_tab, 0, false).unwrap();
        seen.push(current.name());
    }
    assert_eq!(
        seen,
        [
            "CELSIUS",
            "RANKINE",
            "FAHRENHEIT",
            "AMBIENT",
            "KELVIN",
            "CELSIUS"
        ]
    );
    assert_eq!(
        heat_tab_click(TemperatureUnit::Kelvin, on_tab, 1, false),
        Some(TemperatureUnit::Ambient)
    );
    assert_eq!(
        heat_tab_click(TemperatureUnit::Rankine, on_tab, 1, false),
        Some(TemperatureUnit::Celsius)
    );
}

#[test]
fn a_click_that_is_not_the_tabs_goes_on() {
    let kelvin = TemperatureUnit::Kelvin;
    // O botão do meio não é um clique da aba.
    assert_eq!(heat_tab_click(kelvin, [-13.0, 122.0], 2, false), None);
    // Fora do retângulo.
    assert_eq!(heat_tab_click(kelvin, [10.0, 122.0], 0, false), None);
    assert_eq!(heat_tab_click(kelvin, [-13.0, 140.0], 1, false), None);
    // Uma janela por cima do ponto fica com o clique.
    assert_eq!(heat_tab_click(kelvin, [-13.0, 122.0], 0, true), None);
}

#[test]
fn each_unit_has_its_own_sprite_and_it_exists() {
    let sprites = root().join("mods/mekanism/content/assets/mekanism/textures/gui/sprites");
    let names: Vec<String> = TemperatureUnit::ALL
        .iter()
        .map(|u| heat_tab_sprite(*u))
        .collect();
    assert_eq!(
        names,
        [
            "mekanism:gui/sprites/tab/heat_info_k",
            "mekanism:gui/sprites/tab/heat_info_c",
            "mekanism:gui/sprites/tab/heat_info_r",
            "mekanism:gui/sprites/tab/heat_info_f",
            "mekanism:gui/sprites/tab/heat_info_stp",
        ]
    );
    for name in names {
        let relative = name.strip_prefix("mekanism:gui/sprites/").unwrap();
        let bytes = std::fs::read(sprites.join(format!("{relative}.png"))).unwrap();
        assert_eq!(&bytes[1..4], b"PNG", "{name}");
        // O IHDR guarda a largura e a altura: 26 por 26, o tamanho do elemento.
        let size = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
        assert_eq!([size(16), size(20)], [26, 26], "{name}");
    }
}

#[test]
fn the_config_is_the_local_one_with_kelvin_as_default() {
    let config = &trace("config")[0];
    assert_eq!(config["file"], "local");
    assert_eq!(config["type"], "LOCAL");
    assert_eq!(config["path"], CONFIG_KEY);
    assert_eq!(unit(&config["default"]), TemperatureUnit::default());
    assert_eq!(
        LOCAL_CONFIG_FILE,
        format!("{}.toml", config["file"].as_str().unwrap())
    );
}

#[test]
fn the_unit_table_is_the_one_the_game_recorded() {
    let units = trace("unit");
    assert_eq!(units.len(), 5);
    for line in &units {
        let unit_value = unit(&line["name"]);
        assert_eq!(
            unit_value.ordinal() as u64,
            line["ordinal"].as_u64().unwrap()
        );
        assert_eq!(unit_value.symbol(), line["symbol"].as_str().unwrap());
        assert_eq!(unit_value.tab_name(), line["tab_name"].as_str().unwrap());
        assert_eq!(
            unit_value.zero_offset(),
            line["zero_offset"].as_f64().unwrap()
        );
        assert_eq!(
            unit_value.interval_size(),
            line["interval_size"].as_f64().unwrap()
        );
        assert_eq!(unit_value.next(), unit(&line["next"]));
        assert_eq!(unit_value.previous(), unit(&line["previous"]));
        assert_eq!(unit_lang(unit_value), line["short_key"].as_str().unwrap());
    }
}

#[test]
fn every_temperature_display_is_the_games() {
    let displays = trace("display");
    assert_eq!(displays.len(), 5 * 23 * 2);
    for line in &displays {
        let (temp, shift) = (
            line["temp"].as_f64().unwrap(),
            line["shift"].as_bool().unwrap(),
        );
        assert_eq!(
            temperature_display(temp, unit(&line["unit"]), shift, 2),
            line["text"].as_str().unwrap(),
            "{} {temp:e} shift {shift}",
            line["unit"]
        );
    }
}

/// O componente que o jogo gravou, escrito com o idioma do port: a tradução e os argumentos, e os argumentos traduzíveis pelo nome curto.
fn rendered(component: &Value, english: &dyn Fn(&str) -> String) -> String {
    let args: Vec<String> = component["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|arg| match arg.as_str() {
            Some(text) => text.to_owned(),
            None => english(arg["translate"].as_str().unwrap()),
        })
        .collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    format_lang(&english(component["translate"].as_str().unwrap()), &args)
}

#[test]
fn the_tooltip_lines_are_what_the_original_builds_for_every_unit() {
    let tooltips = trace("tooltip");
    assert_eq!(tooltips.len(), 5 * 5);
    let lang = lang();
    let english = |key: &str| {
        lang[key]
            .as_str()
            .unwrap_or_else(|| panic!("{key}"))
            .to_owned()
    };
    for case in &tooltips {
        let local = unit(&case["unit"]);
        let lines = case["lines"].as_array().unwrap();
        let keys: Vec<&str> = lines
            .iter()
            .map(|line| line["translate"].as_str().unwrap())
            .collect();
        assert_eq!(
            keys,
            [
                TEMPERATURE_LANG,
                TRANSFERRED_RATE_LANG,
                DISSIPATED_RATE_LANG,
                UNIT_LANG
            ]
        );
        assert_eq!(lines[3]["args"][0]["translate"], unit_lang(local));
        let expected: Vec<String> = lines.iter().map(|line| rendered(line, &english)).collect();
        let actual = heat_tab_tooltip(
            case["heat"].as_f64().unwrap(),
            case["transfer_loss"].as_f64().unwrap(),
            case["environment_loss"].as_f64().unwrap(),
            local,
            &english,
        );
        assert_eq!(actual, expected, "{case}");
    }
}

#[test]
fn the_tooltip_temperature_is_the_heat_over_the_capacity() {
    // 3283,7429445 de calor sobre 10 de capacidade são 328,37429445 K, e a perda vem como diferença (sem o deslocamento do zero).
    let lines = heat_tab_tooltip(
        3_283.742_944_5,
        0.0,
        2.2e-5,
        TemperatureUnit::Celsius,
        &english,
    );
    assert_eq!(HEAT_CAPACITY, 10.0);
    assert_eq!(
        lines,
        [
            "Temp: 55.22\u{b0}C",
            "Transferred: 0\u{b0}C/t",
            "Dissipated: 22\u{b5}\u{b0}C/t",
            "Unit: C"
        ]
    );
}

#[test]
fn the_lang_lines_are_the_originals() {
    let recorded = &trace("lang")[0]["lang"];
    let recorded = recorded.as_object().unwrap();
    assert_eq!(recorded.len(), 4 + 5);
    let ours = lang();
    for (key, value) in recorded {
        assert_eq!(&ours[key.as_str()], value, "{key}");
    }
}
