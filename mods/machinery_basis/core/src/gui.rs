//! A geometria dos elementos de tela de `mekanism.client.gui.element` que os avisos usam: a aba, a barra, a seta de progresso e o
//! sprite em mosaico. Só contas, sem jogo: quem desenha (o glue) recebe os retângulos prontos. Um retângulo é `[x, y, largura, altura]`
//! em pixels de GUI, e as coordenadas são as do elemento no Java, relativas ao canto da tela.

pub type Rect = [f32; 4];

/// `GuiWarningTab`, à esquerda (`left = true`): `x = -26`, `y = 109`, 26 por 26.
pub const WARNING_TAB_LEFT: Rect = [-26.0, 109.0, 26.0, 26.0];

/// Se o ponto `at` está dentro de `rect` (`GuiElement.isMouseOver`: o canto de cima e da esquerda entram, o de baixo e o da direita não).
pub fn contains(rect: Rect, at: [f32; 2]) -> bool {
    let [x, y, w, h] = rect;
    (x..x + w).contains(&at[0]) && (y..y + h).contains(&at[1])
}

/// Um pedaço do desenho de um sprite em mosaico: o trecho do sprite e onde ele cai.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub src: Rect,
    pub dst: Rect,
}

/// `gui.scaling` do tipo `tile` (o `.mcmeta` de `warning` e `warning_background`): o sprite de `tile` por `tile` se repete a partir do
/// canto de cima e da esquerda de `dst`, e o que passa da borda é cortado, não encolhido (`GuiGraphics.blitTiledSprite`). `clip` é o
/// `enableScissor`: só o que cai dentro dele é desenhado.
pub fn tile_cells(dst: Rect, tile: [f32; 2], clip: Option<Rect>) -> Vec<Cell> {
    let [x, y, w, h] = dst;
    let [tile_w, tile_h] = tile;
    let (clip_left, clip_top, clip_right, clip_bottom) = match clip {
        Some([cx, cy, cw, ch]) => (cx, cy, cx + cw, cy + ch),
        None => (f32::MIN, f32::MIN, f32::MAX, f32::MAX),
    };
    let mut cells = Vec::new();
    let mut offset_y = 0.0;
    while offset_y < h {
        let cell_h = tile_h.min(h - offset_y);
        let mut offset_x = 0.0;
        while offset_x < w {
            let cell_w = tile_w.min(w - offset_x);
            let (left, top) = (x + offset_x, y + offset_y);
            let (right, bottom) = (left + cell_w, top + cell_h);
            let (left, top) = (left.max(clip_left), top.max(clip_top));
            let (right, bottom) = (right.min(clip_right), bottom.min(clip_bottom));
            if right > left && bottom > top {
                cells.push(Cell {
                    src: [
                        left - (x + offset_x),
                        top - (y + offset_y),
                        right - left,
                        bottom - top,
                    ],
                    dst: [left, top, right - left, bottom - top],
                });
            }
            offset_x += tile_w;
        }
        offset_y += tile_h;
    }
    cells
}

/// A partir de que nível de uma barra o `warning` é desenhado também por cima do conteúdo (`GuiBar.drawBackground`).
pub const BAR_WARNING_OVERLAY_LEVEL: f64 = 0.98;

/// O que `GuiBar.drawBackground` desenha de aviso: o fundo, e o pedaço por cima do conteúdo quando a barra está quase cheia.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarWarning {
    /// `warning_background`, dentro da moldura (um pixel para dentro de cada lado).
    pub background: Rect,
    /// `warning` no mesmo retângulo, cortado pela metade de trás (a direita numa barra vertical, a de baixo numa horizontal).
    pub overlay: Option<Overlay>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Overlay {
    pub dst: Rect,
    pub clip: Rect,
}

/// `rect` é o elemento inteiro, com a moldura (largura e altura de `GuiBar`, que já somam dois). `level` é `handler.getLevel()`.
pub fn bar_warning(rect: Rect, level: f64, horizontal: bool) -> BarWarning {
    let [x, y, w, h] = rect;
    let inner = [x + 1.0, y + 1.0, w - 2.0, h - 2.0];
    // O `warning` de cima só vem com conteúdo (`handlerLevel > 0`), e 0,98 já garante isso.
    let overlay = (level >= BAR_WARNING_OVERLAY_LEVEL).then(|| {
        let [ix, iy, iw, ih] = inner;
        // Inteiros, como o `targetWidth /= 2` do Java.
        let clip = if horizontal {
            let half = (ih as i32 / 2) as f32;
            [ix, iy + half, iw, half]
        } else {
            let half = (iw as i32 / 2) as f32;
            [ix + half, iy, half, ih]
        };
        Overlay { dst: inner, clip }
    });
    BarWarning {
        background: inner,
        overlay,
    }
}

/// `GuiProgress.calculateProgressSize`: pelo menos um pixel se há progresso, como a chama do forno.
fn progress_size(progress: f32, size: f32) -> f32 {
    (progress * (size - 1.0)).ceil() + 1.0
}

/// A parte desenhada de um `GuiProgress` horizontal que enche da esquerda para a direita, como `ProgressType.RIGHT` (`inner_offset`
/// 0) e `BAR` (1). Devolve onde ela começa dentro do sprite e a largura, ou nada se não há progresso. Com aviso o progresso é 1,
/// e o sprite é o do aviso.
pub fn progress_fill(
    width: f32,
    inner_offset: f32,
    progress: f32,
    warning: bool,
) -> Option<(f32, f32)> {
    // `getProgress`: no máximo uma unidade, mesmo com upgrades de velocidade.
    let progress = if warning { 1.0 } else { progress.min(1.0) };
    if progress == 0.0 {
        return None;
    }
    Some((
        inner_offset,
        progress_size(progress, width - 2.0 * inner_offset),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tab_sits_on_the_left_of_the_screen() {
        assert_eq!(WARNING_TAB_LEFT, [-26.0, 109.0, 26.0, 26.0]);
        assert!(contains(WARNING_TAB_LEFT, [-26.0, 109.0]));
        assert!(contains(WARNING_TAB_LEFT, [-0.5, 134.5]));
        assert!(!contains(WARNING_TAB_LEFT, [0.0, 120.0]));
        assert!(!contains(WARNING_TAB_LEFT, [-10.0, 135.0]));
        assert!(!contains(WARNING_TAB_LEFT, [-27.0, 120.0]));
    }

    #[test]
    fn a_tile_repeats_from_the_top_left_and_is_cut_at_the_edge() {
        // Uma barra de 4 por 52 (a barra de energia sem a moldura) com tile de 16: um trecho de 4 de largura por quatro de altura, o
        // último cortado em 4 linhas.
        let cells = tile_cells([165.0, 17.0, 4.0, 52.0], [16.0, 16.0], None);
        assert_eq!(cells.len(), 4);
        assert_eq!(cells[0].src, [0.0, 0.0, 4.0, 16.0]);
        assert_eq!(cells[0].dst, [165.0, 17.0, 4.0, 16.0]);
        assert_eq!(cells[3].src, [0.0, 0.0, 4.0, 4.0]);
        assert_eq!(cells[3].dst, [165.0, 65.0, 4.0, 4.0]);
        let area: f32 = cells.iter().map(|c| c.dst[2] * c.dst[3]).sum();
        assert_eq!(area, 4.0 * 52.0);
    }

    #[test]
    fn a_clip_cuts_cells_and_keeps_the_pattern_aligned() {
        // A metade da direita de 4 de largura: a coluna 2 e a 3 do sprite, não as duas primeiras.
        let cells = tile_cells(
            [165.0, 17.0, 4.0, 52.0],
            [16.0, 16.0],
            Some([167.0, 17.0, 2.0, 52.0]),
        );
        assert_eq!(cells[0].src, [2.0, 0.0, 2.0, 16.0]);
        assert_eq!(cells[0].dst, [167.0, 17.0, 2.0, 16.0]);
        let area: f32 = cells.iter().map(|c| c.dst[2] * c.dst[3]).sum();
        assert_eq!(area, 2.0 * 52.0);
        // Um clip fora não desenha nada.
        assert!(
            tile_cells(
                [0.0, 0.0, 8.0, 8.0],
                [16.0, 16.0],
                Some([20.0, 0.0, 4.0, 4.0])
            )
            .is_empty()
        );
    }

    #[test]
    fn a_wide_area_repeats_the_tile_in_both_directions() {
        let cells = tile_cells([0.0, 0.0, 40.0, 20.0], [16.0, 16.0], None);
        assert_eq!(cells.len(), 6);
        assert_eq!(cells[2].src, [0.0, 0.0, 8.0, 16.0]);
        assert_eq!(cells[5].src, [0.0, 0.0, 8.0, 4.0]);
    }

    #[test]
    fn the_bar_warning_is_inside_the_frame() {
        let bar = bar_warning([164.0, 16.0, 6.0, 54.0], 0.0, false);
        assert_eq!(bar.background, [165.0, 17.0, 4.0, 52.0]);
        assert_eq!(bar.overlay, None);
    }

    #[test]
    fn the_overlay_needs_contents_and_ninety_eight_percent() {
        let rect = [164.0, 16.0, 6.0, 54.0];
        assert_eq!(bar_warning(rect, 0.97, false).overlay, None);
        assert_eq!(bar_warning(rect, 0.0, false).overlay, None);
        let overlay = bar_warning(rect, 0.98, false).overlay.unwrap();
        assert_eq!(overlay.dst, [165.0, 17.0, 4.0, 52.0]);
        // Vertical: a metade da direita.
        assert_eq!(overlay.clip, [167.0, 17.0, 2.0, 52.0]);
        // Horizontal: a metade de baixo.
        let horizontal = bar_warning([10.0, 10.0, 52.0, 6.0], 1.0, true)
            .overlay
            .unwrap();
        assert_eq!(horizontal.dst, [11.0, 11.0, 50.0, 4.0]);
        assert_eq!(horizontal.clip, [11.0, 13.0, 50.0, 2.0]);
    }

    #[test]
    fn the_progress_has_at_least_one_pixel_and_the_bar_skips_its_frame() {
        // BAR: 25 de largura, um pixel de moldura de cada lado, 23 de miolo.
        assert_eq!(progress_fill(25.0, 1.0, 0.0, false), None);
        assert_eq!(progress_fill(25.0, 1.0, 0.001, false), Some((1.0, 2.0)));
        assert_eq!(progress_fill(25.0, 1.0, 0.5, false), Some((1.0, 12.0)));
        assert_eq!(progress_fill(25.0, 1.0, 1.0, false), Some((1.0, 23.0)));
        // RIGHT: 32 de largura sem moldura.
        assert_eq!(progress_fill(32.0, 0.0, 0.5, false), Some((0.0, 17.0)));
        assert_eq!(progress_fill(32.0, 0.0, 1.0, false), Some((0.0, 32.0)));
        // Mais que uma unidade não passa do sprite.
        assert_eq!(progress_fill(32.0, 0.0, 3.0, false), Some((0.0, 32.0)));
    }

    #[test]
    fn a_warning_draws_the_full_progress_even_with_none() {
        assert_eq!(progress_fill(25.0, 1.0, 0.0, true), Some((1.0, 23.0)));
        assert_eq!(progress_fill(32.0, 0.0, 0.2, true), Some((0.0, 32.0)));
    }
}
