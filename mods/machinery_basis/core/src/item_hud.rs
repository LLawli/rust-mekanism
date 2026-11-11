//! O texto de HUD dos itens equipados (`MekanismHUD.render`, a parte do texto): quais linhas, onde cada uma fica e o fundo. A lista de linhas de cada item
//! (`IItemHUDProvider.addHUDStrings`) é do mod; aqui só a posição.

/// `MekanismHUD.EQUIPMENT_ORDER`: a ordem em que os slots contribuem com linhas.
pub const EQUIPMENT_ORDER: [&str; 6] = ["offhand", "mainhand", "head", "chest", "legs", "feet"];

/// A cor do texto: `0xFFC8C8C8`.
pub const TEXT_ARGB: u32 = 0xFFC8_C8C8;

/// Uma linha posicionada: o grupo (o slot) e o índice dentro dele, e o canto de cima à esquerda no sistema com a escala do HUD aplicada.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Line {
    pub group: usize,
    pub index: usize,
    pub x: i32,
    pub y: i32,
}

/// O fundo atrás do texto (`GuiUtils.drawBackdrop(graphics, minecraft, x, y, maxTextWidth, maxTextHeight, 1)`), já com a margem de 2 pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Backdrop {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

/// O desenho do texto: a escala, as linhas, o fundo e o `maxTextHeight` (que o HUD do MekaSuit usa depois).
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub scale: f32,
    pub lines: Vec<Line>,
    pub backdrop: Option<Backdrop>,
    pub max_text_height: i32,
}

/// `groups[i][j]` é a largura da linha `j` do grupo `i` (grupos vazios já foram tirados). `background` é se o fundo de texto das opções não é zero
/// (`options.getBackgroundColor(0.0F) != 0`). Sem linhas, o `maxTextHeight` é a altura da tela e nada se desenha.
pub fn layout(
    groups: &[Vec<i32>],
    gui_width: i32,
    gui_height: i32,
    hud_scale: f32,
    reverse: bool,
    background: bool,
) -> Layout {
    let count: usize = groups.iter().map(Vec::len).sum();
    if count == 0 {
        return Layout {
            scale: hud_scale,
            lines: Vec::new(),
            backdrop: None,
            max_text_height: gui_height,
        };
    }
    let x_scale = (gui_width as f32 / hud_scale) as i32;
    let y_scale = (gui_height as f32 / hud_scale) as i32;
    let start = (groups.len() * 2 + count * 9) as i32;
    let mut y = y_scale - start;
    let max_text_height = (y as f32 * hud_scale) as i32;
    let backdrop = background.then(|| {
        let max_width = groups.iter().flatten().copied().max().unwrap_or(0).max(0);
        let x = if reverse { x_scale - max_width - 2 } else { 2 };
        // `drawBackdrop(.., x, y, width, height, 1)`: a altura é o `maxTextHeight`, não a do texto.
        Backdrop {
            x0: x - 2,
            y0: y - 2,
            x1: x + max_width + 2,
            y1: y + max_text_height + 2,
        }
    });
    let mut lines = Vec::with_capacity(count);
    for (group, widths) in groups.iter().enumerate() {
        for (index, width) in widths.iter().enumerate() {
            let x = if reverse { x_scale - width - 2 } else { 2 };
            lines.push(Line { group, index, x, y });
            y += 9;
        }
        y += 2;
    }
    Layout {
        scale: hud_scale,
        lines,
        backdrop,
        max_text_height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_lines_draw_nothing_and_leave_the_screen_height() {
        let layout = layout(&[], 480, 270, 0.6, false, true);
        assert!(layout.lines.is_empty());
        assert_eq!(layout.backdrop, None);
        assert_eq!(layout.max_text_height, 270);
    }

    #[test]
    fn one_line_sits_at_the_bottom_left_with_two_pixels_of_margin() {
        // Escala 0,6 (`float`): 270 / 0,6F = 449,99997, e o `(int)` fica em 449 (não 450).
        let layout = layout(&[vec![90]], 480, 270, 0.6, false, true);
        assert_eq!(
            layout.lines,
            vec![Line {
                group: 0,
                index: 0,
                x: 2,
                y: 449 - (2 + 9)
            }]
        );
        assert_eq!(layout.max_text_height, 262);
    }

    #[test]
    fn the_backdrop_height_is_the_scaled_text_start_not_the_text_height() {
        let layout = layout(&[vec![90]], 480, 270, 0.6, false, true);
        let y = 449 - 11;
        let height = 262;
        assert_eq!(
            layout.backdrop,
            Some(Backdrop {
                x0: 0,
                y0: y - 2,
                x1: 2 + 90 + 2,
                y1: y + height + 2
            })
        );
    }

    #[test]
    fn groups_are_separated_by_two_pixels() {
        let layout = layout(&[vec![50, 70], vec![30]], 480, 270, 1.0, false, false);
        assert_eq!(layout.backdrop, None);
        let start = 2 * 2 + 3 * 9;
        let y0 = 270 - start;
        let rows: Vec<(usize, i32)> = layout.lines.iter().map(|l| (l.group, l.y)).collect();
        assert_eq!(rows, vec![(0, y0), (0, y0 + 9), (1, y0 + 20)]);
    }

    #[test]
    fn reversed_text_aligns_to_the_right_edge() {
        let layout = layout(&[vec![50, 70]], 480, 270, 1.0, true, true);
        assert_eq!(layout.lines[0].x, 480 - 50 - 2);
        assert_eq!(layout.lines[1].x, 480 - 70 - 2);
        // O fundo usa a linha mais larga.
        let backdrop = layout.backdrop.unwrap();
        assert_eq!(backdrop.x0, 480 - 70 - 2 - 2);
        assert_eq!(backdrop.x1, 480 - 70 - 2 + 70 + 2);
    }
}
