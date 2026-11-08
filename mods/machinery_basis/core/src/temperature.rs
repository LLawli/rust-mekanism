//! A unidade de temperatura das telas: `UnitDisplayUtils.TemperatureUnit`, o `MekanismConfig.local.tempUnit` que o clique na aba de calor
//! troca, e o texto que `MekanismUtils.getTemperatureDisplay` monta. Nada aqui conhece id de mod: os textos de idioma e os sprites quem
//! escolhe é o mod, pelo [`TemperatureUnit::tab_name`] e pela posição na lista.

use crate::units::{display_base, format_lang};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

/// Casas decimais que `MekanismUtils.getTemperatureDisplay` passa (`UnitDisplayUtils.getTemperatureDisplay(.., 2)`).
pub const DISPLAY_DECIMALS: i32 = 2;

/// A chave do valor no arquivo da configuração local (`LocalConfig.tempUnit`: `defineEnum("temperatureUnit", KELVIN)`).
pub const CONFIG_KEY: &str = "temperatureUnit";
/// A chave de `LocalConfig.holidays`.
pub const HOLIDAYS_KEY: &str = "holidays";

/// `UnitDisplayUtils.TemperatureUnit`, na ordem do enum: `ordinal`, `getNext` e `getPrevious` seguem essa ordem.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TemperatureUnit {
    #[default]
    Kelvin,
    Celsius,
    Rankine,
    Fahrenheit,
    Ambient,
}

impl TemperatureUnit {
    /// `TemperatureUnit.VALUES`.
    pub const ALL: [Self; 5] = [
        Self::Kelvin,
        Self::Celsius,
        Self::Rankine,
        Self::Fahrenheit,
        Self::Ambient,
    ];

    pub fn ordinal(self) -> usize {
        self as usize
    }

    /// `BY_ID` (`ByIdMap.continuous(.., OutOfBoundsStrategy.ZERO)`): um índice fora da lista vale o primeiro.
    pub fn by_id(id: i32) -> Self {
        usize::try_from(id)
            .ok()
            .and_then(|id| Self::ALL.get(id).copied())
            .unwrap_or(Self::Kelvin)
    }

    /// `IIncrementalEnum.byIndexWrapping`: o índice dá a volta (`Mth.positiveModulo`).
    pub fn by_index_wrapping(index: i32) -> Self {
        let count = Self::ALL.len() as i32;
        Self::ALL[index.rem_euclid(count) as usize]
    }

    /// `IIncrementalEnum.getNext()`.
    pub fn next(self) -> Self {
        Self::by_index_wrapping(self.ordinal() as i32 + 1)
    }

    /// `IIncrementalEnum.getPrevious()`.
    pub fn previous(self) -> Self {
        Self::by_index_wrapping(self.ordinal() as i32 - 1)
    }

    /// O símbolo (`getSymbol`), que fecha o número: `K`, `°C`, `R`, `°F`, `+STP`.
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Kelvin => "K",
            Self::Celsius => "\u{b0}C",
            Self::Rankine => "R",
            Self::Fahrenheit => "\u{b0}F",
            Self::Ambient => "+STP",
        }
    }

    /// `getTabName`: o final do nome do sprite da aba de calor.
    pub fn tab_name(self) -> &'static str {
        match self {
            Self::Kelvin => "k",
            Self::Celsius => "c",
            Self::Rankine => "r",
            Self::Fahrenheit => "f",
            Self::Ambient => "stp",
        }
    }

    /// `zeroOffset`: quanto o zero da escala está acima do zero do kelvin, na escala dela.
    pub fn zero_offset(self) -> f64 {
        match self {
            Self::Kelvin | Self::Rankine => 0.0,
            Self::Celsius => 273.15,
            Self::Fahrenheit => 459.67,
            Self::Ambient => 300.0,
        }
    }

    /// `intervalSize`: quantos graus da escala cabem num kelvin.
    pub fn interval_size(self) -> f64 {
        match self {
            Self::Kelvin | Self::Celsius | Self::Ambient => 1.0,
            Self::Rankine | Self::Fahrenheit => 1.8,
        }
    }

    /// `convertFromK`: `shift` desloca o zero (uma temperatura), e sem ele só se muda o tamanho do grau (uma diferença).
    pub fn convert_from_k(self, temp: f64, shift: bool) -> f64 {
        temp * self.interval_size() - if shift { self.zero_offset() } else { 0.0 }
    }

    /// `convertToK`.
    pub fn convert_to_k(self, temp: f64, shift: bool) -> f64 {
        (temp + if shift { self.zero_offset() } else { 0.0 }) / self.interval_size()
    }

    /// O nome do enum, como o arquivo da configuração o guarda (`KELVIN`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Kelvin => "KELVIN",
            Self::Celsius => "CELSIUS",
            Self::Rankine => "RANKINE",
            Self::Fahrenheit => "FAHRENHEIT",
            Self::Ambient => "AMBIENT",
        }
    }

    /// `EnumGetMethod.NAME_IGNORECASE`, o padrão de `defineEnum`.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|unit| unit.name().eq_ignore_ascii_case(name))
    }
}

/// `InputConstants.MOUSE_BUTTON_LEFT` e `MOUSE_BUTTON_RIGHT`, os números de botão do GLFW.
pub const MOUSE_BUTTON_LEFT: i32 = 0;
pub const MOUSE_BUTTON_RIGHT: i32 = 1;

/// `GuiHeatTab.onClick` (com `isValidClickButton`): o botão esquerdo avança a unidade (`IIncrementalEnum::getNext`) e o direito volta
/// (`getPrevious`). Qualquer outro botão não é um clique da aba e devolve `None`, e o clique segue para o resto da tela.
pub fn rotate(unit: TemperatureUnit, button: i32) -> Option<TemperatureUnit> {
    match button {
        MOUSE_BUTTON_LEFT => Some(unit.next()),
        MOUSE_BUTTON_RIGHT => Some(unit.previous()),
        _ => None,
    }
}

/// `UnitDisplayUtils.getTemperatureDisplay`: `temp` em kelvin escrita em `unit`, com a medida que cabe (`mK`, `kK`).
pub fn temperature_display(temp: f64, unit: TemperatureUnit, shift: bool, decimals: i32) -> String {
    display_base(
        unit.convert_from_k(temp, shift),
        Some(unit.symbol()),
        decimals,
        false,
    )
}

/// `MekanismUtils.getTemperatureDisplay(temp, unit, shift)`: `temp` está em `from`, e sai na unidade da configuração local (`local`),
/// com duas casas. `shift` diz se é uma temperatura (desloca o zero) ou uma diferença (só o tamanho do grau).
pub fn local_display(
    temp: f64,
    from: TemperatureUnit,
    local: TemperatureUnit,
    shift: bool,
) -> String {
    temperature_display(
        from.convert_to_k(temp, true),
        local,
        shift,
        DISPLAY_DECIMALS,
    )
}

/// As linhas do tooltip de `GuiHeatTab` (`updateTooltip`): cada `(modelo, valor)` do `infoHandler`, com o valor já escrito, e por último
/// `MekanismLang.UNIT` com o texto curto da unidade da configuração local. Os modelos são textos de idioma com `%1$s`.
pub fn heat_info_lines(
    rows: &[(&str, String)],
    unit_label: &str,
    unit_template: &str,
) -> Vec<String> {
    let mut lines: Vec<String> = rows
        .iter()
        .map(|(template, value)| format_lang(template, &[value]))
        .collect();
    lines.push(format_lang(unit_template, &[unit_label]));
    lines
}

/// `LocalConfig.tempUnit` lido do texto do arquivo: a primeira linha `temperatureUnit = "<NOME>"`. Chave ausente
/// ou nome que não existe valem o padrão, `KELVIN` (a configuração troca o valor inválido pelo padrão).
pub fn unit_from_toml(text: &str) -> TemperatureUnit {
    value_line(text)
        .map(|(_, value)| value)
        .and_then(|value| TemperatureUnit::from_name(value.trim().trim_matches('"')))
        .unwrap_or_default()
}

/// `LocalConfig.holidays` lido do texto do arquivo (`define("holidays", true)`): `true` ou `false`, também escrito como texto em qualquer caixa (o validador de `define(String,
/// boolean)`). Chave ausente ou valor que não é um booleano valem o padrão, `true`.
pub fn holidays_from_toml(text: &str) -> bool {
    text.lines()
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key.trim() == HOLIDAYS_KEY).then(|| {
                value
                    .split('#')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .trim_matches('"')
                    .to_ascii_lowercase()
            })
        })
        .and_then(|value| match value.as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        })
        .unwrap_or(true)
}

/// O texto do arquivo com `tempUnit` trocado por `unit`, sem tocar nas outras chaves nem nos comentários. Sem a chave, ela entra no fim.
pub fn unit_into_toml(text: &str, unit: TemperatureUnit) -> String {
    let line = format!("{CONFIG_KEY} = \"{}\"", unit.name());
    match value_line(text) {
        Some((index, _)) => {
            let mut lines: Vec<&str> = text.lines().collect();
            lines[index] = &line;
            let mut out = lines.join("\n");
            out.push('\n');
            out
        }
        None => {
            let mut out = text.to_owned();
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&line);
            out.push('\n');
            out
        }
    }
}

/// O arquivo da configuração local (`LocalConfig.getFileName()`: `local`), na pasta do mod.
pub const LOCAL_CONFIG_FILE: &str = "local.toml";

/// De quanto em quanto tempo se olha a data do arquivo (o jogo olha a cada segundo, e relê o que mudou).
pub const RELOAD_EVERY: Duration = Duration::from_secs(1);

/// `MekanismConfig.local.tempUnit` como arquivo no cliente: a unidade lida do `local.toml`, relida quando alguém edita o arquivo (a
/// configuração do original recarrega a cada mudança no disco), e gravada quando o clique da aba a troca (`tempUnit.set` e `save`).
#[derive(Debug)]
pub struct LocalUnit {
    path: PathBuf,
    unit: TemperatureUnit,
    holidays: bool,
    modified: Option<SystemTime>,
    checked: Option<Instant>,
}

impl LocalUnit {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            unit: TemperatureUnit::default(),
            holidays: true,
            modified: None,
            checked: None,
        }
    }

    fn modified(&self) -> Option<SystemTime> {
        std::fs::metadata(&self.path).ok()?.modified().ok()
    }

    fn read(&mut self) {
        self.modified = self.modified();
        let text = std::fs::read_to_string(&self.path).unwrap_or_default();
        self.unit = unit_from_toml(&text);
        self.holidays = holidays_from_toml(&text);
    }

    /// A unidade de agora. Na primeira vez lê o arquivo, e depois só quando a data dele muda, olhando no máximo a cada
    /// [`RELOAD_EVERY`]: o desenho pergunta a cada quadro.
    pub fn get(&mut self, now: Instant) -> TemperatureUnit {
        self.refresh(now);
        self.unit
    }

    /// `MekanismConfig.local.holidays` de agora, relido do arquivo como a unidade.
    pub fn holidays(&mut self, now: Instant) -> bool {
        self.refresh(now);
        self.holidays
    }

    fn refresh(&mut self, now: Instant) {
        match self.checked {
            None => {
                self.checked = Some(now);
                self.read();
            }
            Some(last) if now.duration_since(last) >= RELOAD_EVERY => {
                self.checked = Some(now);
                if self.modified() != self.modified {
                    self.read();
                }
            }
            Some(_) => {}
        }
    }

    /// `tempUnit.set(unit)` e `save()`: a unidade vale já no próximo quadro, e o arquivo guarda as outras chaves como estavam.
    pub fn set(&mut self, unit: TemperatureUnit, now: Instant) {
        let text = std::fs::read_to_string(&self.path).unwrap_or_default();
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&self.path, unit_into_toml(&text, unit));
        self.unit = unit;
        self.modified = self.modified();
        self.checked = Some(now);
    }
}

/// O índice da linha e o valor (depois do `=`) da chave da unidade.
fn value_line(text: &str) -> Option<(usize, &str)> {
    // Uma linha comentada (`#temperatureUnit = ...`) tem `#` na chave e nunca é igual a ela.
    text.lines().enumerate().find_map(|(index, line)| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == CONFIG_KEY).then_some((index, value))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Os valores da tabela vêm de `UnitDisplayUtils.TemperatureUnit` (`dev/referencia/rastros/temperature_units.jsonl` os confere
    /// contra o Java rodando).
    #[test]
    fn holidays_are_on_unless_the_file_says_false() {
        assert!(holidays_from_toml(""));
        assert!(holidays_from_toml("holidays = true\n"));
        assert!(!holidays_from_toml(
            "compactEnergyDisplay = true\nholidays = false\n"
        ));
        assert!(!holidays_from_toml("holidays = \"FALSE\" # nada\n"));
        // Um valor que não é booleano volta ao padrão, que é ligado.
        assert!(holidays_from_toml("holidays = 3\n"));
        assert!(holidays_from_toml("#holidays = false\n"));
    }

    #[test]
    fn the_units_keep_the_original_order_symbols_and_scales() {
        let table: Vec<_> = TemperatureUnit::ALL
            .iter()
            .map(|unit| {
                (
                    unit.name(),
                    unit.symbol(),
                    unit.tab_name(),
                    unit.zero_offset(),
                    unit.interval_size(),
                )
            })
            .collect();
        assert_eq!(
            table,
            [
                ("KELVIN", "K", "k", 0.0, 1.0),
                ("CELSIUS", "\u{b0}C", "c", 273.15, 1.0),
                ("RANKINE", "R", "r", 0.0, 1.8),
                ("FAHRENHEIT", "\u{b0}F", "f", 459.67, 1.8),
                ("AMBIENT", "+STP", "stp", 300.0, 1.0),
            ]
        );
        for (index, unit) in TemperatureUnit::ALL.iter().enumerate() {
            assert_eq!(unit.ordinal(), index);
        }
    }

    #[test]
    fn a_left_click_goes_forward_and_a_right_click_back_with_wrap_around() {
        use TemperatureUnit::*;
        let forward: Vec<_> = TemperatureUnit::ALL.iter().map(|u| u.next()).collect();
        assert_eq!(forward, [Celsius, Rankine, Fahrenheit, Ambient, Kelvin]);
        let back: Vec<_> = TemperatureUnit::ALL.iter().map(|u| u.previous()).collect();
        assert_eq!(back, [Ambient, Kelvin, Celsius, Rankine, Fahrenheit]);
    }

    #[test]
    fn only_the_left_and_right_buttons_rotate_the_unit() {
        use TemperatureUnit::*;
        assert_eq!(rotate(Kelvin, MOUSE_BUTTON_LEFT), Some(Celsius));
        assert_eq!(rotate(Kelvin, MOUSE_BUTTON_RIGHT), Some(Ambient));
        assert_eq!(rotate(Ambient, 0), Some(Kelvin));
        assert_eq!(rotate(Celsius, 2), None);
        assert_eq!(rotate(Celsius, 3), None);
        assert_eq!(rotate(Celsius, -1), None);
    }

    #[test]
    fn an_id_outside_the_list_is_the_first_unit() {
        assert_eq!(TemperatureUnit::by_id(3), TemperatureUnit::Fahrenheit);
        assert_eq!(TemperatureUnit::by_id(5), TemperatureUnit::Kelvin);
        assert_eq!(TemperatureUnit::by_id(-1), TemperatureUnit::Kelvin);
        assert_eq!(
            TemperatureUnit::by_index_wrapping(-6),
            TemperatureUnit::Ambient
        );
        assert_eq!(
            TemperatureUnit::by_index_wrapping(12),
            TemperatureUnit::Rankine
        );
    }

    #[test]
    fn a_temperature_shifts_the_zero_and_a_difference_does_not() {
        use TemperatureUnit::*;
        assert_eq!(Celsius.convert_from_k(300.0, true), 300.0 - 273.15);
        assert_eq!(Celsius.convert_from_k(2.0, false), 2.0);
        assert_eq!(Fahrenheit.convert_from_k(300.0, true), 300.0 * 1.8 - 459.67);
        assert_eq!(Fahrenheit.convert_from_k(2.0, false), 2.0 * 1.8);
        assert_eq!(Rankine.convert_from_k(300.0, true), 540.0);
        assert_eq!(Ambient.convert_from_k(310.0, true), 10.0);
        assert_eq!(Fahrenheit.convert_to_k(0.0, true), 459.67 / 1.8);
        assert_eq!(Celsius.convert_to_k(0.0, true), 273.15);
        assert_eq!(Kelvin.convert_to_k(5.0, true), 5.0);
    }

    #[test]
    fn the_names_are_read_ignoring_case() {
        assert_eq!(
            TemperatureUnit::from_name("celsius"),
            Some(TemperatureUnit::Celsius)
        );
        assert_eq!(
            TemperatureUnit::from_name("AMBIENT"),
            Some(TemperatureUnit::Ambient)
        );
        assert_eq!(TemperatureUnit::from_name("kelvin "), None);
        assert_eq!(TemperatureUnit::from_name(""), None);
    }

    #[test]
    fn the_display_picks_the_measure_and_appends_the_symbol() {
        use TemperatureUnit::*;
        assert_eq!(temperature_display(300.0, Kelvin, true, 2), "300K");
        assert_eq!(temperature_display(300.0, Celsius, true, 2), "26.85\u{b0}C");
        assert_eq!(temperature_display(1_500.0, Kelvin, true, 2), "1.5kK");
        assert_eq!(temperature_display(0.0, Kelvin, true, 2), "0K");
        assert_eq!(temperature_display(0.0123, Kelvin, false, 2), "12.3mK");
        assert_eq!(
            temperature_display(2.213e-5, Kelvin, false, 2),
            "22.13\u{b5}K"
        );
        assert_eq!(temperature_display(300.0, Ambient, true, 2), "0+STP");
        assert_eq!(temperature_display(-5.0, Celsius, false, 2), "-5\u{b0}C");
    }

    #[test]
    fn the_local_display_converts_from_the_given_unit() {
        use TemperatureUnit::*;
        // 300 * 1,8 - 459,67 dá 80,32999..., e o truncamento a duas casas tira o último centésimo.
        assert_eq!(
            local_display(300.0, Kelvin, Fahrenheit, true),
            "80.32\u{b0}F"
        );
        assert_eq!(local_display(300.0, Kelvin, Kelvin, true), "300K");
        // De uma escala com zero deslocado: 26,85 C são 300 K.
        assert_eq!(local_display(26.85, Celsius, Kelvin, true), "300K");
    }

    #[test]
    fn the_info_lines_end_with_the_unit() {
        let rows = [
            ("Temp: %1$s", "300K".to_owned()),
            ("Transferred: %1$s/t", "0K".to_owned()),
        ];
        assert_eq!(
            heat_info_lines(&rows, "C", "Unit: %1$s"),
            ["Temp: 300K", "Transferred: 0K/t", "Unit: C"]
        );
        assert_eq!(heat_info_lines(&[], "K", "Unit: %1$s"), ["Unit: K"]);
    }

    #[test]
    fn the_unit_is_read_from_the_config_text() {
        assert_eq!(unit_from_toml(""), TemperatureUnit::Kelvin);
        assert_eq!(
            unit_from_toml("temperatureUnit = \"FAHRENHEIT\"\n"),
            TemperatureUnit::Fahrenheit
        );
        assert_eq!(
            unit_from_toml("#temperatureUnit = \"CELSIUS\"\ncompactEnergyDisplay = true\n"),
            TemperatureUnit::Kelvin
        );
        assert_eq!(
            unit_from_toml("compactEnergyDisplay = true\ntemperatureUnit=\"rankine\"\n"),
            TemperatureUnit::Rankine
        );
        assert_eq!(
            unit_from_toml("temperatureUnit = \"KELVN\"\n"),
            TemperatureUnit::Kelvin
        );
    }

    #[test]
    fn saving_the_unit_keeps_the_rest_of_the_file() {
        let text = "#Comment\ncompactEnergyDisplay = false\ntemperatureUnit = \"KELVIN\"\nholidays = true\n";
        let saved = unit_into_toml(text, TemperatureUnit::Celsius);
        assert_eq!(
            saved,
            "#Comment\ncompactEnergyDisplay = false\ntemperatureUnit = \"CELSIUS\"\nholidays = true\n"
        );
        assert_eq!(unit_from_toml(&saved), TemperatureUnit::Celsius);
        assert_eq!(
            unit_into_toml("", TemperatureUnit::Ambient),
            "temperatureUnit = \"AMBIENT\"\n"
        );
        assert_eq!(
            unit_into_toml("holidays = true", TemperatureUnit::Rankine),
            "holidays = true\ntemperatureUnit = \"RANKINE\"\n"
        );
        // Um comentário com o nome da chave não é o valor.
        assert_eq!(
            unit_into_toml("#temperatureUnit = \"KELVIN\"\n", TemperatureUnit::Celsius),
            "#temperatureUnit = \"KELVIN\"\ntemperatureUnit = \"CELSIUS\"\n"
        );
    }
}
