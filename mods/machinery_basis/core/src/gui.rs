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

/// `MathUtils.divideToLevel`: quanto de `to_divide` `numerator` ocupa, no máximo 1. Sem capacidade, a barra está cheia.
pub fn divide_to_level(numerator: f64, to_divide: f64) -> f64 {
    if to_divide == 0.0 || numerator >= to_divide {
        1.0
    } else {
        numerator / to_divide
    }
}

/// `GuiBar.calculateSize`: pelo menos um pixel se há conteúdo, como a chama do forno (`Mth.ceil(progress * (size - 1)) + 1`).
pub fn bar_size(level: f64, size: i32) -> i32 {
    (level * f64::from(size - 1)).ceil() as i32 + 1
}

/// O preenchimento de uma barra vertical (`GuiVerticalPowerBar.renderBarContents`, `GuiTankBar.renderBarContentsDirect`): de baixo
/// para cima, dentro da moldura. `rect` é o elemento inteiro, com a moldura (`GuiBar` soma dois à largura e à altura). Sem nível
/// não há nada (`handlerLevel > 0`).
pub fn bar_contents(rect: Rect, level: f64) -> Option<Rect> {
    if level <= 0.0 {
        return None;
    }
    let [x, y, w, h] = rect;
    let size = bar_size(level, h as i32 - 2) as f32;
    Some([x + 1.0, y + h - 1.0 - size, w - 2.0, size])
}

/// `GuiUtils.TilingDirection`: de que canto o mosaico de um preenchimento começa.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TilingDirection {
    DownRight,
    DownLeft,
    UpRight,
    UpLeft,
}

impl TilingDirection {
    fn shift(desired: i32, sprite: i32) -> i32 {
        match desired % sprite {
            0 => 0,
            remainder => sprite - remainder,
        }
    }

    /// `getXShift`: só as direções que começam pela direita deslocam.
    fn x_shift(self, desired_width: i32, sprite_width: i32) -> i32 {
        match self {
            Self::DownRight | Self::UpRight => 0,
            Self::DownLeft | Self::UpLeft => Self::shift(desired_width, sprite_width),
        }
    }

    fn y_shift(self, desired_height: i32, sprite_height: i32) -> i32 {
        match self {
            Self::DownRight | Self::DownLeft => 0,
            Self::UpRight | Self::UpLeft => Self::shift(desired_height, sprite_height),
        }
    }
}

/// `GuiUtils.drawTiledSprite`: o sprite (`tile` de tamanho) repetido sobre `dst`, com o mosaico ancorado no canto que `direction`
/// diz e o excesso cortado por `enableScissor`. Para subir de baixo (`UpRight`) o mosaico começa acima de `dst` por `shift`, de
/// modo que a última linha do sprite fica sempre rente à base.
pub fn tiled_fill(dst: Rect, tile: [f32; 2], direction: TilingDirection) -> Vec<Cell> {
    let [x, y, w, h] = dst;
    if w == 0.0 || h == 0.0 {
        return Vec::new();
    }
    let x_shift = direction.x_shift(w as i32, tile[0] as i32) as f32;
    let y_shift = direction.y_shift(h as i32, tile[1] as i32) as f32;
    tile_cells(
        [x - x_shift, y - y_shift, w + x_shift, h + y_shift],
        tile,
        Some(dst),
    )
}

/// `GuiTankGauge.getScaledLevel`: a altura do conteúdo de um medidor, pelo menos um pixel. `height` é a do elemento, com a moldura.
/// Um tanque vazio ou sem capacidade não tem nada, e um cheio até o infinito (`Long.MAX_VALUE`) enche tudo.
pub fn gauge_level(stored: u64, capacity: u64, height: i32) -> i32 {
    if stored == 0 || capacity == 0 {
        return 0;
    }
    if stored == i64::MAX as u64 {
        return height - 2;
    }
    // `Math.round`: meio para cima.
    let scaled = (stored as f64 / capacity as f64 * f64::from(height - 2) + 0.5).floor();
    (scaled as i32).max(1)
}

/// Um sprite de nove partes (`GuiSpriteScaling.NineSlice`): o tamanho dele, as bordas (esquerda, cima, direita, baixo) e se as
/// bordas e o miolo esticam em vez de repetir (`stretch_inner`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NineSlice {
    pub size: [f32; 2],
    pub border: [f32; 4],
    pub stretch_inner: bool,
}

impl NineSlice {
    /// O `gui.scaling` de tipo `nine_slice` de um `.png.mcmeta`, com a mesma borda dos quatro lados. `None` se o arquivo diz outra
    /// coisa (sem metadado, o jogo estica).
    pub fn from_mcmeta(text: &str) -> Option<Self> {
        let json: serde_json::Value = serde_json::from_str(text).ok()?;
        let scaling = &json["gui"]["scaling"];
        if scaling["type"] != "nine_slice" {
            return None;
        }
        let number = |key: &str| scaling[key].as_f64().map(|value| value as f32);
        let border = &scaling["border"];
        let border = match border.as_f64() {
            Some(all) => [all as f32; 4],
            None => ["left", "top", "right", "bottom"]
                .map(|side| border[side].as_f64().unwrap_or(0.0) as f32),
        };
        let size = [number("width")?, number("height")?];
        // `NineSlice.validate`: bordas que se encontram não deixam miolo, e o sprite fica sem metadado.
        if border[0] + border[2] >= size[0] || border[1] + border[3] >= size[1] {
            return None;
        }
        Some(Self {
            size,
            border,
            stretch_inner: scaling["stretch_inner"].as_bool().unwrap_or(false),
        })
    }
}

/// Mosaico de uma região `src` do sprite sobre `dst` (`blitTiledSprite`), cada pedaço com o corte que cabe.
fn tile_region(src: Rect, dst: Rect) -> Vec<Cell> {
    let [sx, sy, sw, sh] = src;
    let [dx, dy, dw, dh] = dst;
    let mut cells = Vec::new();
    if sw <= 0.0 || sh <= 0.0 || dw <= 0.0 || dh <= 0.0 {
        return cells;
    }
    let mut y = 0.0;
    while y < dh {
        let h = sh.min(dh - y);
        let mut x = 0.0;
        while x < dw {
            let w = sw.min(dw - x);
            cells.push(Cell {
                src: [sx, sy, w, h],
                dst: [dx + x, dy + y, w, h],
            });
            x += sw;
        }
        y += sh;
    }
    cells
}

/// `GuiGraphicsExtractor.blitNineSlicedSprite` sobre `dst`: os cantos no tamanho do sprite, as bordas e o miolo repetidos (ou
/// esticados com `stretch_inner`). Cada `Cell` diz o trecho do sprite, em pixels dele, e onde cai.
pub fn nine_slice_cells(slice: NineSlice, dst: Rect) -> Vec<Cell> {
    let [x, y, w, h] = dst;
    let [sw, sh] = slice.size;
    let [left, top, right, bottom] = slice.border;
    let (half_w, half_h) = ((w / 2.0).floor(), (h / 2.0).floor());
    let (left, right) = (left.min(half_w), right.min(half_w));
    let (top, bottom) = (top.min(half_h), bottom.min(half_h));
    let mut cells = Vec::new();
    let part = |cells: &mut Vec<Cell>, tx: f32, ty: f32, dst: Rect| {
        if dst[2] != 0.0 && dst[3] != 0.0 {
            cells.push(Cell {
                src: [tx, ty, dst[2], dst[3]],
                dst,
            });
        }
    };
    let inner = |cells: &mut Vec<Cell>, dst: Rect, src: Rect| {
        if dst[2] <= 0.0 || dst[3] <= 0.0 {
            return;
        }
        if slice.stretch_inner {
            cells.push(Cell { src, dst });
        } else {
            cells.extend(tile_region(src, dst));
        }
    };
    if w == sw && h == sh {
        part(&mut cells, 0.0, 0.0, [x, y, w, h]);
    } else if h == sh {
        part(&mut cells, 0.0, 0.0, [x, y, left, h]);
        inner(
            &mut cells,
            [x + left, y, w - right - left, h],
            [left, 0.0, sw - right - left, sh],
        );
        part(&mut cells, sw - right, 0.0, [x + w - right, y, right, h]);
    } else if w == sw {
        part(&mut cells, 0.0, 0.0, [x, y, w, top]);
        inner(
            &mut cells,
            [x, y + top, w, h - bottom - top],
            [0.0, top, sw, sh - bottom - top],
        );
        part(&mut cells, 0.0, sh - bottom, [x, y + h - bottom, w, bottom]);
    } else {
        let (center_w, center_h) = (w - right - left, h - bottom - top);
        let (src_w, src_h) = (sw - right - left, sh - bottom - top);
        part(&mut cells, 0.0, 0.0, [x, y, left, top]);
        inner(
            &mut cells,
            [x + left, y, center_w, top],
            [left, 0.0, src_w, top],
        );
        part(&mut cells, sw - right, 0.0, [x + w - right, y, right, top]);
        part(
            &mut cells,
            0.0,
            sh - bottom,
            [x, y + h - bottom, left, bottom],
        );
        inner(
            &mut cells,
            [x + left, y + h - bottom, center_w, bottom],
            [left, sh - bottom, src_w, bottom],
        );
        part(
            &mut cells,
            sw - right,
            sh - bottom,
            [x + w - right, y + h - bottom, right, bottom],
        );
        inner(
            &mut cells,
            [x, y + top, left, center_h],
            [0.0, top, left, src_h],
        );
        inner(
            &mut cells,
            [x + left, y + top, center_w, center_h],
            [left, top, src_w, src_h],
        );
        inner(
            &mut cells,
            [x + w - right, y + top, right, center_h],
            [sw - right, top, right, src_h],
        );
    }
    cells
}

/// O tamanho de um slot e da moldura dele (`SlotType.SLOT_SIZE`).
pub const SLOT_SIZE: f32 = 18.0;

/// `GuiMekanism.addSlots`: a moldura de um slot começa um pixel antes do item (`slot.x - 1`).
pub fn slot_frame(item: [f32; 2]) -> Rect {
    [item[0] - 1.0, item[1] - 1.0, SLOT_SIZE, SLOT_SIZE]
}

/// Onde o título cai (`GuiMekanism.renderTitleText`): `drawScrollingString` centrado entre `maxLengthPad` (4) de cada lado da
/// largura da tela. Texto mais largo que isso rola no Java, partindo do começo dele.
pub fn title_x(image_width: f32, text_width: f32) -> f32 {
    const PAD: f32 = 4.0;
    let (min_x, max_x) = (PAD, image_width - PAD);
    let area = max_x - min_x;
    if text_width > area {
        return min_x;
    }
    min_x + (area - text_width) / 2.0
}

/// `AbstractContainerScreen.titleLabelY` e `inventoryLabelX`.
pub const TITLE_LABEL_Y: f32 = 6.0;
pub const INVENTORY_LABEL_X: f32 = 8.0;

/// `AbstractContainerScreen.inventoryLabelY`: `imageHeight - 94`.
pub fn inventory_label_y(image_height: f32) -> f32 {
    image_height - 94.0
}

/// `SpecialColors.TEXT_TITLE`: `CommonColors.DARK_GRAY`, a cor do título e do rótulo do inventário.
pub const TITLE_TEXT_COLOR: u32 = 0x40_40_40;

/// `GuiProgress.getProgress` com os valores que o servidor sincroniza (`getRecipeProgress / (float) ticksRequired`). Antes da
/// primeira sincronização o cliente ainda tem o `baseTicksRequired` com que o tile nasceu, e o progresso é zero.
pub fn scaled_progress(progress: i32, ticks_required: i32, base_ticks_required: i32) -> f32 {
    let required = if ticks_required == 0 {
        base_ticks_required
    } else {
        ticks_required
    };
    progress as f32 / required as f32
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

    #[test]
    fn the_level_is_the_fraction_capped_at_one() {
        assert_eq!(divide_to_level(0.0, 100.0), 0.0);
        assert_eq!(divide_to_level(50.0, 100.0), 0.5);
        assert_eq!(divide_to_level(100.0, 100.0), 1.0);
        assert_eq!(divide_to_level(200.0, 100.0), 1.0);
        // Sem capacidade, cheio.
        assert_eq!(divide_to_level(5.0, 0.0), 1.0);
    }

    #[test]
    fn a_bar_has_at_least_one_pixel_of_contents() {
        assert_eq!(bar_size(0.0001, 52), 2);
        assert_eq!(bar_size(0.5, 52), 27);
        assert_eq!(bar_size(1.0, 52), 52);
        assert_eq!(bar_size(1.0, 1), 1);
    }

    #[test]
    fn the_contents_grow_from_the_bottom_inside_the_frame() {
        // A barra de energia da Enrichment Chamber: 4 por 52 dentro de uma moldura de 6 por 54.
        let rect = [164.0, 16.0, 6.0, 54.0];
        assert_eq!(bar_contents(rect, 0.0), None);
        assert_eq!(bar_contents(rect, 0.5), Some([165.0, 42.0, 4.0, 27.0]));
        // Cheia, ocupa o miolo inteiro: de y + 1 até y + altura - 1.
        assert_eq!(bar_contents(rect, 1.0), Some([165.0, 17.0, 4.0, 52.0]));
        let [_, y, _, h] = bar_contents(rect, 0.2).unwrap();
        assert_eq!(y + h, 69.0);
    }

    #[test]
    fn a_gauge_rounds_half_up_and_keeps_one_pixel() {
        // O medidor largo: 64 por 48 dentro de uma moldura de 66 por 50.
        assert_eq!(gauge_level(0, 1000, 50), 0);
        assert_eq!(gauge_level(10, 0, 50), 0);
        assert_eq!(gauge_level(500, 1000, 50), 24);
        assert_eq!(gauge_level(1, 1000, 50), 1);
        // 0,53125 * 48 = 25,5: meio para cima.
        assert_eq!(gauge_level(531_250, 1_000_000, 50), 26);
        assert_eq!(gauge_level(i64::MAX as u64, 1000, 50), 48);
    }

    #[test]
    fn a_gauge_does_not_clamp_an_overfilled_tank() {
        // `GuiTankGauge.getScaledLevel` não limita: 5.000 mB num tanque de 1.000 dão cinco vezes a altura (decisão 243).
        assert_eq!(gauge_level(1000, 1000, 50), 48);
        assert_eq!(gauge_level(5000, 1000, 50), 240);
    }

    #[test]
    fn a_fill_going_up_keeps_the_last_row_of_the_sprite_on_the_base() {
        // 30 de altura com mosaico de 16: o mosaico começa 2 acima, e a primeira linha perde as duas de cima.
        let cells = tiled_fill(
            [56.0, 20.0, 64.0, 30.0],
            [16.0, 16.0],
            TilingDirection::UpRight,
        );
        assert_eq!(cells[0].src, [0.0, 2.0, 16.0, 14.0]);
        assert_eq!(cells[0].dst, [56.0, 20.0, 16.0, 14.0]);
        assert_eq!(cells[4].src, [0.0, 0.0, 16.0, 16.0]);
        assert_eq!(cells[4].dst, [56.0, 34.0, 16.0, 16.0]);
        let area: f32 = cells.iter().map(|c| c.dst[2] * c.dst[3]).sum();
        assert_eq!(area, 64.0 * 30.0);
        // Múltiplo exato: sem deslocamento.
        let exact = tiled_fill(
            [0.0, 0.0, 16.0, 32.0],
            [16.0, 16.0],
            TilingDirection::UpRight,
        );
        assert_eq!(exact[0].src, [0.0, 0.0, 16.0, 16.0]);
        assert_eq!(exact.len(), 2);
    }

    #[test]
    fn a_fill_going_down_starts_at_the_top() {
        // O químico da Metallurgic Infuser: desce da esquerda e de cima, e o resto é cortado.
        let cells = tiled_fill(
            [8.0, 40.0, 4.0, 20.0],
            [16.0, 16.0],
            TilingDirection::DownRight,
        );
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0].src, [0.0, 0.0, 4.0, 16.0]);
        assert_eq!(cells[1].src, [0.0, 0.0, 4.0, 4.0]);
        assert_eq!(cells[1].dst, [8.0, 56.0, 4.0, 4.0]);
        assert!(
            tiled_fill(
                [0.0, 0.0, 0.0, 5.0],
                [16.0, 16.0],
                TilingDirection::DownRight
            )
            .is_empty()
        );
    }

    const BAR_BASE: &str =
        r#"{"gui":{"scaling":{"type":"nine_slice","width":5,"height":5,"border":1}}}"#;
    const PANEL_BASE: &str =
        r#"{"gui":{"scaling":{"type":"nine_slice","width":16,"height":16,"border":4}}}"#;

    #[test]
    fn the_mcmeta_gives_the_nine_slice() {
        let slice = NineSlice::from_mcmeta(BAR_BASE).unwrap();
        assert_eq!(slice.size, [5.0, 5.0]);
        assert_eq!(slice.border, [1.0; 4]);
        assert!(!slice.stretch_inner);
        // Outro tipo de escala, e bordas que se encontram, não são nove partes.
        let tile = r#"{"gui":{"scaling":{"type":"tile","width":2,"height":2}}}"#;
        assert_eq!(NineSlice::from_mcmeta(tile), None);
        let meeting =
            r#"{"gui":{"scaling":{"type":"nine_slice","width":4,"height":4,"border":2}}}"#;
        assert_eq!(NineSlice::from_mcmeta(meeting), None);
        let sides = r#"{"gui":{"scaling":{"type":"nine_slice","width":9,"height":9,"border":{"left":1,"top":2,"right":3,"bottom":4},"stretch_inner":true}}}"#;
        let slice = NineSlice::from_mcmeta(sides).unwrap();
        assert_eq!(slice.border, [1.0, 2.0, 3.0, 4.0]);
        assert!(slice.stretch_inner);
    }

    fn covered(cells: &[Cell]) -> f32 {
        cells.iter().map(|c| c.dst[2] * c.dst[3]).sum()
    }

    #[test]
    fn the_bar_frame_tiles_the_inside_of_the_sprite_and_covers_the_element() {
        let slice = NineSlice::from_mcmeta(BAR_BASE).unwrap();
        let cells = nine_slice_cells(slice, [164.0, 16.0, 6.0, 54.0]);
        assert_eq!(covered(&cells), 6.0 * 54.0);
        // O miolo de 3 por 3 do sprite repete: um pedaço inteiro e um cortado em uma coluna, não esticado.
        let center: Vec<_> = cells
            .iter()
            .filter(|c| {
                c.dst[0] >= 165.0 && c.dst[0] < 169.0 && c.dst[1] >= 17.0 && c.dst[1] < 69.0
            })
            .collect();
        assert_eq!(center[0].src, [1.0, 1.0, 3.0, 3.0]);
        assert_eq!(center[0].dst, [165.0, 17.0, 3.0, 3.0]);
        assert_eq!(center[1].src, [1.0, 1.0, 1.0, 3.0]);
        assert_eq!(center[1].dst, [168.0, 17.0, 1.0, 3.0]);
        // Os quatro cantos, um pixel cada.
        assert_eq!(cells[0].src, [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(cells[0].dst, [164.0, 16.0, 1.0, 1.0]);
        let last = cells
            .iter()
            .find(|c| c.src == [4.0, 4.0, 1.0, 1.0])
            .unwrap();
        assert_eq!(last.dst, [169.0, 69.0, 1.0, 1.0]);
    }

    #[test]
    fn the_panel_covers_the_whole_screen() {
        let slice = NineSlice::from_mcmeta(PANEL_BASE).unwrap();
        let cells = nine_slice_cells(slice, [0.0, 0.0, 176.0, 166.0]);
        assert_eq!(covered(&cells), 176.0 * 166.0);
        // Do tamanho do sprite, um pedaço só; mais largo, as bordas ficam e o miolo de 8 repete duas vezes.
        assert_eq!(nine_slice_cells(slice, [0.0, 0.0, 16.0, 16.0]).len(), 1);
        assert_eq!(nine_slice_cells(slice, [0.0, 0.0, 24.0, 16.0]).len(), 4);
        let stretched = NineSlice {
            stretch_inner: true,
            ..slice
        };
        assert_eq!(
            nine_slice_cells(stretched, [0.0, 0.0, 176.0, 166.0]).len(),
            9
        );
    }

    #[test]
    fn a_slot_frame_starts_a_pixel_before_the_item() {
        assert_eq!(slot_frame([64.0, 17.0]), [63.0, 16.0, 18.0, 18.0]);
    }

    #[test]
    fn the_labels_sit_where_the_container_screen_puts_them() {
        // Centrado entre os 4 pixels de cada lado: o mesmo que centrar na tela inteira.
        assert_eq!(title_x(176.0, 100.0), 38.0);
        assert_eq!(title_x(176.0, 99.0), 38.5);
        // Mais largo que a área: começa do começo dela.
        assert_eq!(title_x(176.0, 200.0), 4.0);
        assert_eq!(inventory_label_y(166.0), 72.0);
        assert_eq!(TITLE_LABEL_Y, 6.0);
        assert_eq!(INVENTORY_LABEL_X, 8.0);
        assert_eq!(TITLE_TEXT_COLOR, 0x40_40_40);
    }

    #[test]
    fn the_progress_uses_the_synced_ticks_required() {
        assert_eq!(scaled_progress(100, 200, 200), 0.5);
        // Com upgrades de velocidade a conta usa os ticks de agora, não os de base.
        assert_eq!(scaled_progress(50, 100, 200), 0.5);
        assert_eq!(scaled_progress(100, 100, 200), 1.0);
        // Antes da primeira sincronização vale o `baseTicksRequired`, como o tile do cliente.
        assert_eq!(scaled_progress(0, 0, 200), 0.0);
        assert_eq!(scaled_progress(100, 0, 200), 0.5);
    }
}
