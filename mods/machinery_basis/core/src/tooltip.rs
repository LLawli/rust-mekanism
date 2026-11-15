//! A quebra de linhas de um tooltip: `Tooltip.create` guarda o texto já cortado por `Font.split(message, 170)`, que é
//! `StringSplitter.splitLines`. O Horizon desenha as linhas que o mod entrega como estão, então quem as entrega as corta aqui.

/// `Tooltip.MAX_WIDTH`.
pub const MAX_WIDTH: f32 = 170.0;

/// `StringSplitter.splitLines` sobre texto simples. `width` mede um trecho de texto em pixels de GUI (a soma dos caracteres).
///
/// Segue o `LineBreakFinder`: o primeiro caractere de uma linha sempre cabe, um `\n` fecha a linha, e quando um caractere passa de
/// `max_width` a linha fecha no último espaço que viu (que não entra em linha nenhuma), ou antes do caractere se não viu nenhum.
pub fn split_lines(text: &str, max_width: f32, width: &dyn Fn(&str) -> f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let split = line_break(rest, max_width, width);
        lines.push(rest[..split].to_owned());
        let after = rest[split..].chars().next();
        // O espaço ou o `\n` onde a linha fechou é consumido; um caractere que estourou a largura abre a linha seguinte.
        let skipped = if matches!(after, Some(' ' | '\n')) {
            1
        } else {
            0
        };
        rest = &rest[split + skipped..];
    }
    lines
}

/// `StringSplitter.findLineBreak`: o byte onde a linha acaba, ou o tamanho do texto se ele cabe inteiro.
fn line_break(text: &str, max_width: f32, width: &dyn Fn(&str) -> f32) -> usize {
    let mut last_space: Option<usize> = None;
    let mut next_char = 0;
    let mut had_width = false;
    for (index, c) in text.char_indices() {
        if c == '\n' {
            return index;
        }
        if c == ' ' {
            last_space = Some(index);
        }
        let end = index + c.len_utf8();
        // `width` do trecho até aqui, como a soma que o Java vai acumulando.
        let total = width(&text[..end]);
        let char_width = total - width(&text[..index]);
        if !had_width || total <= max_width {
            had_width |= char_width != 0.0;
            next_char = end;
        } else {
            return last_space.unwrap_or(index);
        }
    }
    next_char
}

/// `MenuTooltipPositioner.positionTooltip`: onde cai o canto de cima e da esquerda do texto de um tooltip de widget
/// (`WidgetTooltipHolder.createTooltipPositioner`), que fica ao lado do cursor mas por cima ou por baixo do widget, e não sobre ele.
/// Tudo em pixels de GUI inteiros: `screen` e `size` são largura e altura, `mouse` o cursor e `widget` o `ScreenRectangle`
/// (`[esquerda, topo, largura, altura]`).
pub fn menu_position(
    screen: [i32; 2],
    mouse: [i32; 2],
    size: [i32; 2],
    widget: [i32; 4],
) -> [i32; 2] {
    const MARGIN: i32 = 5;
    const MOUSE_OFFSET_X: i32 = 12;
    const OVERLAP: i32 = 3;
    let [screen_width, screen_height] = screen;
    let [mouse_x, mouse_y] = mouse;
    let [tooltip_width, tooltip_height] = size;
    let [_, widget_top, _, widget_height] = widget;
    let widget_bottom = widget_top + widget_height;
    let mut x = mouse_x + MOUSE_OFFSET_X;
    if x + tooltip_width > screen_width - MARGIN {
        x = (mouse_x - MOUSE_OFFSET_X - tooltip_width).max(9);
    }
    let mut y = mouse_y + OVERLAP;
    let padded_height = tooltip_height + OVERLAP + OVERLAP;
    let lowest_possible_y = widget_bottom + OVERLAP + menu_offset(0, 0, widget_height);
    let max_y = screen_height - MARGIN;
    if lowest_possible_y + padded_height <= max_y {
        y += menu_offset(y, widget_top, widget_height);
    } else {
        y -= padded_height + menu_offset(y, widget_bottom, widget_height);
    }
    [x, y]
}

/// `MenuTooltipPositioner.getOffset`: `Mth.lerp` em `f32` e `Math.round`, que arredonda a metade para cima.
fn menu_offset(mouse_y: i32, widget_y: i32, widget_height: i32) -> i32 {
    let distance = (mouse_y - widget_y).abs().min(widget_height);
    let delta = distance as f32 / widget_height as f32;
    let start = (widget_height - 3) as f32;
    let end = 5.0_f32;
    (start + delta * (end - start) + 0.5).floor() as i32
}

/// As linhas de dica que o componente `DETAILS` e o `DESCRIPTION` de um item põem no fim do tooltip (`MekanismDataComponents.registerComponentAppenderAfterAll(DETAILS, ...)` e
/// `registerComponentAppenderAfter(DESCRIPTION, ...)`): sem a tecla de detalhes a dica `Hold Left Shift for details.`; depois dela, com Shift mais N a descrição e sem eles a dica
/// `Hold SHIFT + N for a description.`. Um item sem `DETAILS` não ganha a dica de detalhes e mostra a descrição ou a dica dela do mesmo jeito (o `DESCRIPTION` é outro componente).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldLine {
    /// `tooltip.mekanism.hold_for_details`.
    DetailsHint,
    /// `tooltip.mekanism.hold_for_description`.
    DescriptionHint,
    /// O texto da descrição do item.
    Description,
}

/// Quais linhas de dica um item mostra: `has_details` (o item tem o componente `DETAILS`), `has_description` (tem o `DESCRIPTION`), `details_down` (a tecla de detalhes, Left Shift) e
/// `description_down` (Shift mais N).
pub fn hold_lines(has_details: bool, has_description: bool, details_down: bool, description_down: bool) -> Vec<HoldLine> {
    let mut lines = Vec::new();
    if has_details && !details_down {
        lines.push(HoldLine::DetailsHint);
    }
    if has_description {
        lines.push(if description_down { HoldLine::Description } else { HoldLine::DescriptionHint });
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Seis pixels por caractere, o que basta para conferir onde as linhas fecham.
    fn six(text: &str) -> f32 {
        text.chars().count() as f32 * 6.0
    }

    #[test]
    fn a_text_that_fits_is_one_line() {
        assert_eq!(split_lines("abc def", 170.0, &six), ["abc def"]);
    }

    #[test]
    fn a_long_line_breaks_at_the_last_space_and_drops_it() {
        // Cabem 5 caracteres (30 de 30): o sexto estoura, e o último espaço visto é o do índice 3.
        assert_eq!(split_lines("abc defg", 30.0, &six), ["abc", "defg"]);
        assert_eq!(split_lines("ab cd efg", 30.0, &six), ["ab cd", "efg"]);
    }

    #[test]
    fn a_word_longer_than_the_line_breaks_before_the_char_that_overflows() {
        assert_eq!(split_lines("abcdefgh", 30.0, &six), ["abcde", "fgh"]);
    }

    #[test]
    fn a_newline_closes_the_line() {
        assert_eq!(split_lines("ab\ncd", 170.0, &six), ["ab", "cd"]);
        // Sem linha vazia no fim.
        assert_eq!(split_lines("ab\n", 170.0, &six), ["ab"]);
    }

    #[test]
    fn the_first_char_always_fits() {
        assert_eq!(split_lines("abc", 3.0, &six), ["a", "b", "c"]);
    }

    #[test]
    fn a_leading_space_is_part_of_the_first_line() {
        // As mensagens de aviso começam com " - ": o espaço do começo fica, e a quebra cai num espaço de dentro.
        assert_eq!(
            split_lines(" - No matching recipe", 90.0, &six),
            [" - No matching", "recipe"]
        );
    }

    #[test]
    fn nothing_makes_no_line() {
        assert!(split_lines("", 170.0, &six).is_empty());
    }
    #[test]
    fn a_tab_tooltip_goes_below_the_tab_beside_the_cursor() {
        // A tela de 640 por 360 de GUI, a aba em (206, 206) e o cursor no meio dela: duas linhas de 20 de altura.
        let at = menu_position([640, 360], [219, 219], [170, 20], [206, 206, 26, 26]);
        assert_eq!(at, [231, 234]);
    }

    #[test]
    fn a_tooltip_with_no_room_below_goes_above_and_one_with_no_room_right_goes_left() {
        let above = menu_position([640, 360], [100, 340], [100, 20], [90, 330, 26, 26]);
        assert_eq!(above, [112, 303]);
        let left = menu_position([640, 360], [600, 100], [100, 20], [590, 90, 26, 26]);
        assert_eq!(left[0], 488);
    }

    #[test]
    fn the_offset_rounds_half_up_like_math_round() {
        // distância 0: widgetHeight - 3; no fim do widget: 5.
        assert_eq!(menu_offset(10, 10, 26), 23);
        assert_eq!(menu_offset(40, 10, 26), 5);
        // lerp(0.5, 23, 5) = 14.
        assert_eq!(menu_offset(23, 10, 26), 14);
    }

    #[test]
    fn the_hold_lines_follow_the_details_and_description_keys() {
        use HoldLine::*;
        assert_eq!(hold_lines(true, true, false, false), [DetailsHint, DescriptionHint]);
        assert_eq!(hold_lines(true, true, true, false), [DescriptionHint]);
        assert_eq!(hold_lines(true, true, true, true), [Description]);
        assert_eq!(hold_lines(true, true, false, true), [DetailsHint, Description]);
        assert_eq!(hold_lines(false, true, false, false), [DescriptionHint]);
        assert_eq!(hold_lines(true, false, false, false), [DetailsHint]);
        assert!(hold_lines(false, false, false, false).is_empty());
    }
}
