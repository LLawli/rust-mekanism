//! `GuiScrollableElement` e `GuiScrollList` sem desenho: a barra de rolagem de uma lista de linhas de 12 pixels. As coordenadas são
//! as do Java, relativas ao canto da janela (ou da tela) onde a lista mora.

/// `GuiInstallableScrollList`: a altura de uma linha.
pub const ELEMENT_HEIGHT: i32 = 12;

/// Uma lista de `width` por `height` em `(x, y)`, como `GuiUpgradeScrollList` (108 por 50) a cria.
#[derive(Clone, Debug, PartialEq)]
pub struct ScrollList {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    /// `scroll`: a fração percorrida, de 0 a 1.
    scroll: f32,
    /// `dragOffset`, e se a barra está sendo arrastada.
    drag_offset: Option<i32>,
    /// `elementHeight`: 12 nas listas de upgrades (`GuiInstallableScrollList`), 10 na de texto (`GuiTextScrollList`).
    element_height: i32,
}

/// `GuiScrollList`: `barXShift = width - 6`, `barYShift = 2`, a barra tem 4 por 4 e `maxBarHeight = height - 4`.
const BAR_Y_SHIFT: i32 = 2;
const BAR_SIZE: i32 = 4;
const BAR_X_SHIFT_FROM_RIGHT: i32 = 6;
const BAR_MARGIN: i32 = 4;

impl ScrollList {
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
            scroll: 0.0,
            drag_offset: None,
            element_height: ELEMENT_HEIGHT,
        }
    }

    /// A mesma lista com outra altura de linha (`GuiScrollList(..., elementHeight, ...)`).
    pub fn with_element_height(mut self, element_height: i32) -> Self {
        self.element_height = element_height;
        self
    }

    /// `elementHeight`.
    pub fn element_height(&self) -> i32 {
        self.element_height
    }

    /// `scroll = 0`: o que `GuiTextScrollList.setText` faz quando a lista deixa de precisar da barra.
    pub fn reset_scroll(&mut self) {
        self.scroll = 0.0;
    }

    /// `barXShift`.
    pub fn bar_x_shift(&self) -> i32 {
        self.width - BAR_X_SHIFT_FROM_RIGHT
    }

    fn bar_x(&self) -> i32 {
        self.x + self.bar_x_shift()
    }

    fn bar_y(&self) -> i32 {
        self.y + BAR_Y_SHIFT
    }

    /// `getMax()` da barra: o quanto o polegar anda, `maxBarHeight - barHeight`.
    fn travel(&self) -> i32 {
        self.height - BAR_MARGIN - BAR_SIZE
    }

    /// `getFocusedElements`: quantas linhas cabem.
    pub fn focused(&self) -> i32 {
        (self.height - 2) / self.element_height
    }

    /// `needsScrollBars`.
    pub fn needs_scroll_bars(&self, total: usize) -> bool {
        i32::try_from(total).unwrap_or(i32::MAX) > self.focused()
    }

    /// `getScroll()`: o deslocamento do polegar em pixels, preso à faixa. Não olha se a barra é necessária.
    pub fn thumb_offset(&self) -> i32 {
        ((self.scroll * self.travel() as f32) as i32).clamp(0, self.travel())
    }

    /// Onde o polegar fica, em `[x, y, largura, altura]` (a posição do `list_bar`).
    pub fn thumb_rect(&self) -> [i32; 4] {
        [
            self.bar_x(),
            self.bar_y() + self.thumb_offset(),
            BAR_SIZE,
            BAR_SIZE,
        ]
    }

    /// `getCurrentSelection`: o índice da primeira linha visível.
    pub fn first_visible(&self, total: usize) -> usize {
        if !self.needs_scroll_bars(total) {
            return 0;
        }
        let elements = total as f32 - self.focused() as f32;
        ((elements + 0.5) * self.scroll) as usize
    }

    /// `mouseClicked` na barra: o clique só conta sobre o polegar, com os limites inclusivos. Com barra começa o arrasto; sem ela a
    /// rolagem volta a zero. Devolve se o clique foi no polegar.
    pub fn press_thumb(&mut self, mouse: [f64; 2], total: usize) -> bool {
        let [x, y, w, h] = self.thumb_rect();
        let on_thumb = mouse[0] >= f64::from(x)
            && mouse[0] <= f64::from(x + w)
            && mouse[1] >= f64::from(y)
            && mouse[1] <= f64::from(y + h);
        if !on_thumb {
            return false;
        }
        if self.needs_scroll_bars(total) {
            let from_bar = mouse[1] - f64::from(self.bar_y() + self.thumb_offset());
            self.drag_offset = Some(from_bar as i32);
        } else {
            self.scroll = 0.0;
        }
        true
    }

    /// `onDrag` da barra: a fração é a posição do mouse menos o começo da faixa e o ponto onde se pegou o polegar.
    pub fn drag_to(&mut self, mouse_y: f64) {
        if let Some(offset) = self.drag_offset {
            let fraction =
                (mouse_y - f64::from(self.bar_y()) - f64::from(offset)) / f64::from(self.travel());
            self.scroll = (fraction as f32).clamp(0.0, 1.0);
        }
    }

    /// `onRelease`.
    pub fn release(&mut self) {
        self.drag_offset = None;
    }

    pub fn dragging(&self) -> bool {
        self.drag_offset.is_some()
    }

    /// `adjustScroll(delta)`: uma rodada do mouse anda `1 / elements`, e só havendo barra. Devolve se tratou.
    pub fn adjust_scroll(&mut self, delta: f64, total: usize) -> bool {
        if !self.needs_scroll_bars(total) {
            return false;
        }
        let elements = (total as f32 - self.focused() as f32).ceil();
        let sign = if delta > 0.0 {
            1.0
        } else if delta < 0.0 {
            -1.0
        } else {
            0.0
        };
        self.scroll = (self.scroll - sign / elements).clamp(0.0, 1.0);
        true
    }

    /// `isMouseOver` da lista inteira, com a barra.
    pub fn contains(&self, mouse: [f64; 2]) -> bool {
        mouse[0] >= f64::from(self.x)
            && mouse[0] < f64::from(self.x + self.width)
            && mouse[1] >= f64::from(self.y)
            && mouse[1] < f64::from(self.y + self.height)
    }

    /// A linha sob o mouse para o hover e o tooltip (`updateTooltip`, `renderElements`): `x` entre `getX() + 1` e
    /// `getX() + barXShift - 1`, e `y` na faixa da linha `i`. Devolve o índice entre as linhas visíveis (0 a `focused - 1`).
    pub fn hovered_row(&self, mouse: [f64; 2]) -> Option<i32> {
        if mouse[0] < f64::from(self.x + 1)
            || mouse[0] >= f64::from(self.x + self.bar_x_shift() - 1)
        {
            return None;
        }
        (0..self.focused()).find(|i| {
            let top = f64::from(self.y + 1 + self.element_height * i);
            mouse[1] >= top && mouse[1] < top + f64::from(self.element_height)
        })
    }

    /// O clique nas linhas (`GuiScrollList.onClick`): dentro da área devolve `Some(Some(índice))` numa linha com item,
    /// `Some(None)` numa área vazia (limpa a seleção) e `None` fora da área. O limite inferior da linha é inclusivo, como no Java.
    pub fn click_row(&self, mouse: [f64; 2], total: usize) -> Option<Option<usize>> {
        let inside = mouse[0] >= f64::from(self.x + 1)
            && mouse[0] < f64::from(self.x + self.bar_x_shift() - 1)
            && mouse[1] >= f64::from(self.y + 1)
            && mouse[1] < f64::from(self.y + self.height - 1);
        if !inside {
            return None;
        }
        let first = self.first_visible(total);
        for i in 0..self.focused() as usize {
            if first + i >= total {
                break;
            }
            let top = f64::from(self.y + 1 + self.element_height * i as i32);
            if mouse[1] >= top && mouse[1] <= top + f64::from(self.element_height) {
                return Some(Some(first + i));
            }
        }
        Some(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lista da janela de upgrades, em (6, 18).
    fn list() -> ScrollList {
        ScrollList::new(6, 18, 108, 50)
    }

    #[test]
    fn the_list_shows_four_rows_and_the_thumb_travels_42() {
        let list = list();
        assert_eq!(list.focused(), 4);
        assert_eq!(list.bar_x_shift(), 102);
        assert_eq!(list.thumb_rect(), [108, 20, 4, 4]);
        assert!(!list.needs_scroll_bars(4));
        assert!(list.needs_scroll_bars(5));
        assert_eq!(list.first_visible(4), 0);
    }

    #[test]
    fn the_wheel_moves_one_element_at_a_time_and_only_with_a_bar() {
        let mut list = list();
        assert!(!list.adjust_scroll(-1.0, 4));
        // 6 linhas: 2 elementos a mais, cada passo anda meio caminho.
        assert!(list.adjust_scroll(-1.0, 6));
        assert_eq!(list.first_visible(6), 1);
        assert_eq!(list.thumb_offset(), 21);
        assert!(list.adjust_scroll(-1.0, 6));
        assert_eq!(list.first_visible(6), 2);
        assert_eq!(list.thumb_offset(), 42);
        // Para cima volta.
        assert!(list.adjust_scroll(1.0, 6));
        assert_eq!(list.first_visible(6), 1);
        list.adjust_scroll(1.0, 6);
        list.adjust_scroll(1.0, 6);
        assert_eq!(list.first_visible(6), 0);
    }

    #[test]
    fn dragging_the_thumb_follows_the_mouse_and_keeps_the_grab_point() {
        let mut list = list();
        // Pegar o polegar 2 pixels abaixo do topo dele.
        assert!(list.press_thumb([110.0, 22.0], 8));
        assert!(list.dragging());
        list.drag_to(22.0 + 21.0);
        assert_eq!(list.thumb_offset(), 21);
        list.drag_to(500.0);
        assert_eq!(list.thumb_offset(), 42);
        list.drag_to(-500.0);
        assert_eq!(list.thumb_offset(), 0);
        list.release();
        assert!(!list.dragging());
        list.drag_to(40.0);
        assert_eq!(list.thumb_offset(), 0, "solto, o mouse não arrasta");
    }

    #[test]
    fn a_click_off_the_thumb_or_without_a_bar_does_not_drag() {
        let mut list = list();
        // O trilho fora do polegar não faz nada.
        assert!(!list.press_thumb([110.0, 40.0], 8));
        // Sem barra necessária, o clique no polegar só zera a rolagem.
        list.adjust_scroll(-1.0, 8);
        assert!(list.thumb_offset() > 0);
        assert!(list.press_thumb([110.0, f64::from(list.thumb_rect()[1])], 3));
        assert_eq!(list.thumb_offset(), 0);
        assert!(!list.dragging());
        // Os limites do polegar são inclusivos.
        assert!(list.press_thumb([112.0, 24.0], 3));
        assert!(!list.press_thumb([112.5, 24.0], 3));
    }

    #[test]
    fn rows_are_hit_inside_the_text_area_and_the_lower_edge_is_inclusive() {
        let list = list();
        assert_eq!(list.hovered_row([7.0, 19.0]), Some(0));
        assert_eq!(list.hovered_row([6.0, 19.0]), None);
        assert_eq!(list.hovered_row([106.9, 19.0]), Some(0));
        assert_eq!(list.hovered_row([107.0, 19.0]), None);
        assert_eq!(list.hovered_row([50.0, 19.0 + 12.0]), Some(1));
        assert_eq!(list.hovered_row([50.0, 19.0 + 47.9]), Some(3));
        // O clique usa `<=` embaixo: na fronteira entre duas linhas vence a de cima.
        assert_eq!(list.click_row([50.0, 31.0], 3), Some(Some(0)));
        assert_eq!(list.click_row([50.0, 31.5], 3), Some(Some(1)));
        // Área vazia limpa; fora da área, nada.
        assert_eq!(list.click_row([50.0, 19.0 + 36.5], 3), Some(None));
        assert_eq!(list.click_row([50.0, 17.0], 3), None);
        assert_eq!(list.click_row([5.0, 30.0], 3), None);
    }

    #[test]
    fn the_first_visible_row_offsets_the_clicked_index() {
        let mut list = list();
        list.adjust_scroll(-1.0, 6);
        assert_eq!(list.click_row([50.0, 20.0], 6), Some(Some(1)));
    }
}
