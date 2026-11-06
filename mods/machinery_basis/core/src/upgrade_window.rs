//! A janela de upgrades (`GuiUpgradeWindow`, `GuiSupportedUpgrades`, `PacketRemoveUpgrade`) sem desenho: as contas de layout, o
//! que a tela interna escreve e o formato do que o servidor manda. A lib não conhece nenhum upgrade: a ordem dos tipos (a do
//! registro) chega de quem chama.

use crate::gui::NineSlice;
use crate::upgrade::UpgradeComponent;

/// O `gui.scaling` dos sprites de nove partes que a janela desenha, como os `.png.mcmeta` do Mekanism dizem (um teste confere cada
/// um contra o arquivo do mod): `holder_right`, `shadow`, `button`, `button_digital`, `element_holder`, `inner_screen`,
/// `scroll/list_border` e `list/base`, `hovered` e `selected`.
pub const HOLDER_SLICE: NineSlice = slice(26.0, 9.0, 4.0);
pub const SHADOW_SLICE: NineSlice = slice(9.0, 9.0, 3.0);
pub const BUTTON_SLICE: NineSlice = slice(200.0, 20.0, 2.0);
pub const DIGITAL_SLICE: NineSlice = slice(3.0, 3.0, 1.0);
pub const ELEMENT_HOLDER_SLICE: NineSlice = slice(16.0, 16.0, 2.0);
pub const INNER_SCREEN_SLICE: NineSlice = slice(256.0, 256.0, 32.0);
pub const LIST_BORDER_SLICE: NineSlice = slice(6.0, 6.0, 2.0);
pub const LIST_ROW_SLICE: NineSlice = slice(12.0, 12.0, 1.0);

const fn slice(width: f32, height: f32, border: f32) -> NineSlice {
    NineSlice {
        size: [width, height],
        border: [border; 4],
        stretch_inner: false,
    }
}

/// O que o mod diz sobre a janela de upgrades de um bloco. São só dados: quem liga ao jogo é a `machinery_basis`.
pub struct UpgradeWindowSpec {
    pub block: &'static str,
    /// Os índices dos dois slots virtuais (entrada e saída) no menu do bloco.
    pub virtual_slots: [usize; 2],
    /// Quantos dados o contêiner sincroniza ao nascer (`addContainerTrackers`).
    pub tracked_base: usize,
    /// A chave de `startTracking` da janela e quantos dados ela acrescenta.
    pub tracking_key: u8,
    pub tracked_items: usize,
    /// Onde, nos data slots do menu, ficam o `upgradeTicks` e as contagens, e quantos são.
    pub sync_offset: usize,
    pub sync_len: usize,
    /// O namespace dos sprites (`<ns>:gui/sprites/...`) e dos textos de upgrade (`upgrade.<ns>.<tipo>`).
    pub ns: &'static str,
    /// O tipo da janela e a tabela de todos, na ordem do enum.
    pub window_type: crate::window::WindowType,
    pub window_types: &'static [crate::window::WindowType],
    /// O painel de fundo (`base`) e o `gui.scaling` dele.
    pub base_slice: NineSlice,
    /// `maxUpgradeMultiplier`.
    pub max_multiplier: i32,
    /// Os tipos de upgrade e o que a tag do bloco aceita.
    pub data: fn() -> std::rc::Rc<crate::upgrade::UpgradeData>,
    /// Do nome da cor (`red`) para o ARGB (`EnumColor`).
    pub color: fn(&str) -> u32,
    /// O item de upgrade e o componente que guarda o tipo nele.
    pub item: &'static str,
    pub item_component: &'static str,
}

/// `GuiUpgradeWindow`: a largura da janela e a altura sem as linhas de upgrades suportados.
pub const WINDOW_WIDTH: i32 = 198;
const WINDOW_BASE_HEIGHT: i32 = 76;
/// `Math.max(18, 12 * rows)`: a faixa de baixo nunca é menor que isso.
const MIN_SUPPORTED_HEIGHT: i32 = 18;

/// `GuiSupportedUpgrades`.
const ELEMENT_WIDTH: i32 = 167;
const PADDED_ELEMENT_WIDTH: i32 = ELEMENT_WIDTH - 2;
pub const ELEMENT_SIZE: i32 = 12;
const ROW_ROOM: i32 = PADDED_ELEMENT_WIDTH / ELEMENT_SIZE;

/// A janela abre em `(getGuiWidth() - 198) / 2` na horizontal e 15 na vertical (`GuiUpgradeWindowTab.createWindow`).
pub fn default_position(gui_width: i32) -> [i32; 2] {
    [(gui_width - WINDOW_WIDTH) / 2, 15]
}

/// A altura: `76 + max(18, 12 * rows)`.
pub fn window_height(rows: i32) -> i32 {
    WINDOW_BASE_HEIGHT + MIN_SUPPORTED_HEIGHT.max(ELEMENT_SIZE * rows)
}

/// O layout da faixa "Supported:" (`GuiSupportedUpgrades`): a primeira linha começa depois do rótulo, as outras na borda.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SupportedLayout {
    /// `firstRowStart`: a largura do rótulo mais um, no máximo a largura útil.
    pub first_row_start: i32,
    /// `firstRowRoom`: quantos ícones cabem na primeira linha.
    pub first_row_room: i32,
}

impl SupportedLayout {
    /// `label_width` é `font.width(UPGRADES_SUPPORTED)`.
    pub fn new(label_width: i32) -> Self {
        let first_row_start = (label_width + 1).min(PADDED_ELEMENT_WIDTH);
        Self {
            first_row_start,
            first_row_room: (PADDED_ELEMENT_WIDTH - first_row_start) / ELEMENT_SIZE,
        }
    }

    /// `calculateNeededRows`.
    pub fn needed_rows(&self, count: usize) -> i32 {
        let count = i32::try_from(count).unwrap_or(i32::MAX);
        if count == 0 {
            return 1;
        }
        if count <= self.first_row_room {
            return 1;
        }
        2 + (count - self.first_row_room) / ROW_ROOM
    }

    /// `getRow`.
    fn row(&self, index: i32) -> i32 {
        if index < self.first_row_room {
            0
        } else {
            1 + (index - self.first_row_room) / ROW_ROOM
        }
    }

    /// `getUpgradePos`: o canto do ícone, relativo ao elemento mais um pixel.
    pub fn position(&self, index: usize) -> [i32; 2] {
        let index = i32::try_from(index).unwrap_or(i32::MAX);
        let row = self.row(index);
        if row == 0 {
            // `index % firstRowRoom` quebra com zero de espaço: o Java teria uma divisão por zero, aqui fica no começo.
            let column = if self.first_row_room == 0 {
                0
            } else {
                index % self.first_row_room
            };
            return [self.first_row_start + column * ELEMENT_SIZE, 0];
        }
        let shifted = index - self.first_row_room;
        [(shifted % ROW_ROOM) * ELEMENT_SIZE, row * ELEMENT_SIZE]
    }

    /// `updateTooltip`: o índice do ícone sob o mouse, `rel` sendo o mouse menos o canto do elemento menos um. A divisão é a de
    /// inteiros do Java (para zero), então a sobra à esquerda do primeiro ícone ainda aponta para ele. `None` fora da área ou além
    /// do último tipo.
    pub fn index_at(&self, rel: [i32; 2], count: usize) -> Option<usize> {
        if count == 0 {
            return None;
        }
        let last_row = self.row(i32::try_from(count - 1).ok()?);
        if rel[0] < 0
            || rel[0] >= PADDED_ELEMENT_WIDTH
            || rel[1] < 0
            || rel[1] >= ELEMENT_SIZE * (last_row + 1)
        {
            return None;
        }
        let target_row = rel[1] / ELEMENT_SIZE;
        let index = if target_row == 0 {
            (rel[0] - self.first_row_start) / ELEMENT_SIZE
        } else {
            self.first_row_room + ROW_ROOM * (target_row - 1) + rel[0] / ELEMENT_SIZE
        };
        usize::try_from(index).ok().filter(|index| *index < count)
    }
}

/// `String.valueOf(float)`, o que o `translatable` faz com um número: no mínimo um dígito depois do ponto, e notação científica
/// abaixo de 1e-3 e a partir de 1e7.
pub fn java_float(value: f32) -> String {
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        }
        .to_owned();
    }
    let magnitude = value.abs();
    if (1e-3..1e7).contains(&magnitude) {
        let text = format!("{value}");
        if text.contains('.') {
            text
        } else {
            format!("{text}.0")
        }
    } else {
        let text = format!("{value:e}");
        let (mantissa, exponent) = text.split_once('e').unwrap_or((&text, "0"));
        let mantissa = if mantissa.contains('.') {
            mantissa.to_owned()
        } else {
            format!("{mantissa}.0")
        };
        format!("{mantissa}E{exponent}")
    }
}

/// `IUpgradeTile.getUpgradeWindowInfo`: uma linha "Effect: Nx" quando o tipo é suportado e aceita mais de um. `max_multiplier`
/// é `maxUpgradeMultiplier` (10 por padrão). As máquinas portadas não são exponenciais, e o ramo `Math.pow(2, installed)` do Java
/// não existe aqui.
pub fn effect_text(
    supported: bool,
    installed: i32,
    max: i32,
    max_multiplier: i32,
) -> Option<String> {
    if !supported || max <= 1 {
        return None;
    }
    let effect = f64::from(max_multiplier).powf(f64::from(installed as f32 / max as f32));
    // `Math.round(effect * 100) / 100F`
    let rounded = (effect * 100.0 + 0.5).floor() as i64;
    Some(java_float(rounded as f32 / 100.0))
}

/// O que o servidor sincroniza para quem tem a janela aberta: `upgradeTicks` (`SyncableInt`) e o mapa de upgrades
/// (`SyncableStreamCodec.upgradeMap`). Aqui o mapa vai como uma contagem por tipo, na ordem do registro.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpgradeSync {
    pub ticks: i32,
    pub counts: Vec<i32>,
}

impl UpgradeSync {
    /// `getSpecificSyncableData`: o `upgradeTicks` e o mapa de upgrades do componente, com uma contagem por tipo na ordem do registro.
    pub fn of(component: &UpgradeComponent) -> Self {
        Self {
            ticks: component.upgrade_ticks(),
            counts: component
                .data()
                .type_ids()
                .iter()
                .map(|id| component.get_upgrades(id))
                .collect(),
        }
    }

    /// Os data slots: o primeiro é `upgradeTicks`, depois uma contagem por tipo, completando com zeros até `len`. Tipos além de `len - 1`
    /// ficam sem contagem (o menu declara um número fixo de data slots).
    pub fn write(&self, len: usize) -> Vec<i32> {
        let mut slots = vec![0; len];
        if let Some(first) = slots.first_mut() {
            *first = self.ticks;
        }
        for (slot, count) in slots.iter_mut().skip(1).zip(&self.counts) {
            *slot = *count;
        }
        slots
    }

    /// O inverso de [`write`](Self::write): o cliente recebe cada slot como `i16`.
    pub fn read(slots: &[i32]) -> Self {
        let wire = |value: i32| i32::from(value as i16);
        Self {
            ticks: slots.first().copied().map_or(0, wire),
            counts: slots.iter().skip(1).copied().map(wire).collect(),
        }
    }

    /// `upgrades.getOrDefault(type, 0)` para o tipo de índice `index`.
    pub fn count(&self, index: usize) -> i32 {
        self.counts.get(index).copied().unwrap_or(0)
    }

    /// `getInstalledTypes()`: os índices com ao menos um instalado, na ordem do registro.
    pub fn installed(&self) -> Vec<usize> {
        self.counts
            .iter()
            .enumerate()
            .filter(|(_, count)| **count > 0)
            .map(|(index, _)| index)
            .collect()
    }
}

/// O índice de um tipo na ordem do registro: é o que `PacketRemoveUpgrade` manda no lugar do `Holder`.
pub fn type_index(order: &[String], id: &str) -> Option<u16> {
    order
        .iter()
        .position(|candidate| candidate == id)
        .and_then(|index| u16::try_from(index).ok())
}

/// O tipo de um índice recebido; um índice fora do registro não é nada.
pub fn type_at(order: &[String], index: u16) -> Option<&str> {
    order.get(usize::from(index)).map(String::as_str)
}

/// `PacketRemoveUpgrade`: o índice do tipo (dois bytes) e `removeAll` (um byte).
pub fn encode_remove(index: u16, remove_all: bool) -> Vec<u8> {
    let [high, low] = index.to_be_bytes();
    vec![high, low, u8::from(remove_all)]
}

/// O inverso de [`encode_remove`]; `None` para uma mensagem malformada.
pub fn decode_remove(bytes: &[u8]) -> Option<(u16, bool)> {
    match bytes {
        [high, low, all] if *all <= 1 => Some((u16::from_be_bytes([*high, *low]), *all == 1)),
        _ => None,
    }
}

/// `IFancyFontRenderer.getOverflowedBy`: quanto um texto largo demais já andou para a esquerda, `max_position` sendo a largura que
/// passa da área. O ciclo é um cosseno dentro de um seno, e o texto começa no começo da linha (o sinal trocado dos idiomas da
/// esquerda para a direita). As constantes (`PERIOD_PER_SCROLLED_PIXEL` 0,5 e `MIN_SCROLL_PERIOD` 3) são as do vanilla.
pub fn scroll_overflow(max_position: f64, millis_visible: i64) -> f64 {
    let seconds = millis_visible as f64 / 1_000.0;
    let period = (max_position * 0.5).max(3.0);
    let modifier = -(2.0 * std::f64::consts::PI * seconds / period).cos();
    let scrolled = (std::f64::consts::FRAC_PI_2 * modifier).sin() / 2.0 + 0.5;
    scrolled * max_position
}

/// `Font.split(text, maxWidth)`: quebra nos espaços; uma palavra mais larga que a linha quebra no caractere. `width` mede um trecho.
pub fn wrap_lines(text: &str, max_width: i32, width: &dyn Fn(&str) -> f32) -> Vec<String> {
    let max = max_width as f32;
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split(' ') {
        let candidate = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if width(&candidate) <= max {
            line = candidate;
            continue;
        }
        if !line.is_empty() {
            lines.push(std::mem::take(&mut line));
        }
        // A palavra sozinha pode não caber: corta onde passa.
        let mut piece = String::new();
        for character in word.chars() {
            let mut next = piece.clone();
            next.push(character);
            if width(&next) > max && !piece.is_empty() {
                lines.push(std::mem::take(&mut piece));
            }
            piece.push(character);
        }
        line = piece;
    }
    lines.push(line);
    lines
}

/// `Component.translatable` com os argumentos: troca `%1$s`, `%2$s` (ou `%s`, em ordem) pelo texto de cada um.
pub fn translate(template: &str, args: &[String]) -> String {
    let mut out = String::new();
    let mut next = 0;
    let mut rest = template;
    while let Some(at) = rest.find('%') {
        out.push_str(&rest[..at]);
        rest = &rest[at + 1..];
        if let Some(tail) = rest.strip_prefix("s") {
            out.push_str(args.get(next).map_or("", String::as_str));
            next += 1;
            rest = tail;
        } else if let Some((digits, tail)) = rest.split_once("$s")
            && let Ok(index) = digits.parse::<usize>()
        {
            out.push_str(args.get(index.saturating_sub(1)).map_or("", String::as_str));
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix('%') {
            out.push('%');
            rest = tail;
        } else {
            out.push('%');
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "Supported:" na fonte padrão: 5 + 5 + 5 + 5 + 5 + 5 + 3 + 5 + 5 + 1 pixels mais o espaço entre letras.
    const LABEL: i32 = 54;

    #[test]
    fn eight_types_fit_the_first_row_and_the_window_is_94_high() {
        let layout = SupportedLayout::new(LABEL);
        assert_eq!(layout.first_row_start, 55);
        assert_eq!(layout.first_row_room, 9);
        assert_eq!(layout.needed_rows(8), 1);
        assert_eq!(layout.needed_rows(0), 1);
        assert_eq!(layout.needed_rows(9), 1);
        assert_eq!(layout.needed_rows(10), 2);
        assert_eq!(window_height(1), 94);
        assert_eq!(window_height(2), 100);
        assert_eq!(window_height(0), 94);
        assert_eq!(default_position(176), [-11, 15]);
    }

    #[test]
    fn the_rows_wrap_at_thirteen_after_the_first() {
        let layout = SupportedLayout::new(LABEL);
        assert_eq!(layout.needed_rows(22), 2 + 13 / 13);
        assert_eq!(layout.needed_rows(21), 2);
        assert_eq!(layout.position(0), [55, 0]);
        assert_eq!(layout.position(8), [55 + 8 * 12, 0]);
        assert_eq!(layout.position(9), [0, 12]);
        assert_eq!(layout.position(21), [12 * 12, 12]);
        assert_eq!(layout.position(22), [0, 24]);
    }

    #[test]
    fn a_long_label_pushes_the_icons_to_the_next_row() {
        let layout = SupportedLayout::new(300);
        assert_eq!(layout.first_row_start, 165);
        assert_eq!(layout.first_row_room, 0);
        assert_eq!(layout.needed_rows(1), 2);
        assert_eq!(layout.position(0), [0, 12]);
    }

    #[test]
    fn the_tooltip_index_uses_java_integer_division() {
        let layout = SupportedLayout::new(LABEL);
        assert_eq!(layout.index_at([55, 0], 8), Some(0));
        assert_eq!(layout.index_at([66, 11], 8), Some(0));
        assert_eq!(layout.index_at([67, 0], 8), Some(1));
        // Até 11 pixels à esquerda do primeiro ícone a divisão para zero ainda acerta o índice 0.
        assert_eq!(layout.index_at([44, 0], 8), Some(0));
        assert_eq!(layout.index_at([43, 0], 8), None);
        assert_eq!(layout.index_at([55 + 7 * 12, 0], 8), Some(7));
        // Além do último tipo, ou fora da área.
        assert_eq!(layout.index_at([55 + 8 * 12, 0], 8), None);
        assert_eq!(layout.index_at([165, 0], 8), None);
        assert_eq!(layout.index_at([60, 12], 8), None);
        assert_eq!(layout.index_at([60, -1], 8), None);
        assert_eq!(layout.index_at([60, 0], 0), None);
        // Segunda linha.
        assert_eq!(layout.index_at([13, 12], 12), Some(9 + 1));
    }

    #[test]
    fn floats_print_like_java() {
        assert_eq!(java_float(1.0), "1.0");
        assert_eq!(java_float(10.0), "10.0");
        assert_eq!(java_float(2.37), "2.37");
        assert_eq!(java_float(0.5), "0.5");
        assert_eq!(java_float(0.0), "0.0");
        assert_eq!(java_float(12_345_678.0), "1.2345678E7");
        assert_eq!(java_float(1e10), "1.0E10");
        assert_eq!(java_float(0.0001), "1.0E-4");
    }

    #[test]
    fn the_effect_is_the_multiplier_to_the_installed_fraction() {
        assert_eq!(effect_text(true, 0, 8, 10).as_deref(), Some("1.0"));
        assert_eq!(effect_text(true, 8, 8, 10).as_deref(), Some("10.0"));
        assert_eq!(effect_text(true, 4, 8, 10).as_deref(), Some("3.16"));
        assert_eq!(effect_text(true, 3, 8, 10).as_deref(), Some("2.37"));
        assert_eq!(effect_text(true, 1, 8, 10).as_deref(), Some("1.33"));
        // Um tipo que cabe um só, ou que a máquina não aceita, não tem linha.
        assert_eq!(effect_text(true, 1, 1, 10), None);
        assert_eq!(effect_text(false, 1, 8, 10), None);
    }

    #[test]
    fn the_sync_round_trips_through_sixteen_bits() {
        let sync = UpgradeSync {
            ticks: 20,
            counts: vec![0, 3, 0, 8, 1, 0, 0, 64],
        };
        let slots = sync.write(9);
        assert_eq!(slots, [20, 0, 3, 0, 8, 1, 0, 0, 64]);
        assert_eq!(UpgradeSync::read(&slots), sync);
        assert_eq!(sync.installed(), [1, 3, 4, 7]);
        assert_eq!(sync.count(3), 8);
        assert_eq!(sync.count(40), 0);
        // Mais tipos que slots: os que sobram não chegam.
        assert_eq!(sync.write(3), [20, 0, 3]);
        assert_eq!(UpgradeSync::read(&[]), UpgradeSync::default());
    }

    #[test]
    fn the_remove_packet_maps_a_type_to_its_index_and_back() {
        let order: Vec<String> = ["a", "b", "c"].iter().map(|s| (*s).to_owned()).collect();
        assert_eq!(type_index(&order, "c"), Some(2));
        assert_eq!(type_index(&order, "z"), None);
        assert_eq!(type_at(&order, 1), Some("b"));
        assert_eq!(type_at(&order, 3), None);
        assert_eq!(decode_remove(&encode_remove(2, true)), Some((2, true)));
        assert_eq!(
            decode_remove(&encode_remove(300, false)),
            Some((300, false))
        );
        assert_eq!(decode_remove(&[0, 1]), None);
        assert_eq!(decode_remove(&[0, 1, 2]), None);
    }

    #[test]
    fn the_scroll_starts_at_the_beginning_and_reaches_the_end() {
        assert!(scroll_overflow(20.0, 0).abs() < 1e-9);
        // Meio ciclo depois (período de 10 s para 20 pixels): o fim.
        assert!((scroll_overflow(20.0, 5_000) - 20.0).abs() < 1e-9);
        // O período mínimo é de 3 s.
        assert!((scroll_overflow(2.0, 1_500) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn words_wrap_at_spaces_and_long_words_at_characters() {
        let width = |text: &str| text.chars().count() as f32 * 5.0;
        assert_eq!(wrap_lines("No selection.", 70, &width), ["No selection."]);
        assert_eq!(
            wrap_lines("Stone Generator Upgrade", 60, &width),
            ["Stone", "Generator", "Upgrade"]
        );
        assert_eq!(wrap_lines("aaaaaaaa", 20, &width), ["aaaa", "aaaa"]);
        assert_eq!(wrap_lines("", 20, &width), [""]);
    }

    #[test]
    fn translation_fills_numbered_and_plain_placeholders() {
        let args = ["3".to_owned(), "8".to_owned()];
        assert_eq!(translate("Amount: %1$s/%2$s", &args), "Amount: 3/8");
        assert_eq!(translate("%2$s then %1$s", &args), "8 then 3");
        assert_eq!(translate("%s and %s", &args), "3 and 8");
        assert_eq!(translate("100%% sure", &args), "100% sure");
        assert_eq!(
            translate("%1$s Upgrade", &["Speed".to_owned()]),
            "Speed Upgrade"
        );
    }
}
