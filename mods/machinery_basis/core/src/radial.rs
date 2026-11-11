//! O menu radial dos itens de modo (`GuiRadialSelector`): a geometria do anel, a fatia sob o mouse, as posições dos ícones e dos nomes, o que a rolagem e o
//! clique escolhem, e quando o menu abre e fecha (`ClientTickHandler`). O que desenha (o anel, o ícone e o texto) e o que manda o pacote é de quem usa; aqui
//! fica a conta, sem tipos do jogo.
//!
//! Sem raiz aninhada: nenhum modo portado tem `INestedRadialMode` (só os módulos do Meka-Tool têm), então `parents` está sempre vazio (`SELECT_RADIUS`, sem o
//! botão de voltar). Quando um item com raiz aninhada entrar, a pilha de pais entra aqui.

use crate::mth;

/// `GuiRadialSelector.DRAWS`, `INNER`, `OUTER`, `MIDDLE_DISTANCE`, `SELECT_RADIUS`.
pub const DRAWS: f32 = 720.0;
pub const INNER: f32 = 40.0;
pub const OUTER: f32 = 100.0;
pub const MIDDLE_DISTANCE: f32 = (INNER + OUTER) / 2.0;
pub const SELECT_RADIUS: f32 = 10.0;

/// O ícone de cada fatia é um quadrado de 24 pixels, a 12 à esquerda e a 20 acima do ponto da fatia.
pub const ICON_SIZE: i32 = 24;
pub const ICON_LEFT: i32 = 12;
pub const ICON_UP: i32 = 20;
/// O nome da fatia: escala 0,8, a `font.lineHeight` abaixo do ponto, `0xCCFFFFFF` com sombra.
pub const LABEL_SCALE: f32 = 0.8;
pub const LABEL_DOWN: i32 = 9;
pub const LABEL_ARGB: u32 = 0xCCFF_FFFF;

/// Um anel desenhado (`drawTorus`): a cor é RGBA em `[0, 1]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Torus {
    pub start_angle: f32,
    pub size_angle: f32,
    pub inner: f32,
    pub outer: f32,
    pub rgba: [f32; 4],
}

/// Onde uma fatia desenha, relativo ao centro da tela: o canto do ícone e o ponto do nome (`x`, `y` do `MIDDLE_DISTANCE`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slice {
    pub icon: [i32; 2],
    pub label: [f32; 2],
}

/// O que um quadro do menu desenha, na ordem do `render`: os anéis, depois os ícones, depois os nomes.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub toruses: Vec<Torus>,
    pub slices: Vec<Slice>,
    /// A fatia sob o mouse (`selection`), ou `None` perto do centro.
    pub selection: Option<usize>,
}

/// A cor da fatia atual: o `EnumColor` do modo (`color().getColor(i)`), ou nenhuma.
pub type SliceColor = Option<[f32; 3]>;

/// `StatUtils.wrapDegrees(float)`.
pub fn wrap_degrees(angle: f32) -> f32 {
    let mut angle = angle % 360.0;
    if angle < 0.0 {
        angle += 360.0;
    }
    angle
}

/// O desenho de um quadro. `current` é o índice do modo atual (ou o padrão do `RadialData`); `current_color` a cor da fatia desse modo; `modes` a
/// quantidade de fatias (`getModes().size()`, não zero: com zero o menu fecha).
pub fn frame(
    mouse: [i32; 2],
    center: [f32; 2],
    modes: usize,
    current: Option<usize>,
    current_color: SliceColor,
) -> Frame {
    let active = modes as f32;
    let angle_size = 360.0 / active;
    let mut toruses = vec![torus(0.0, 360.0, 0.3, 0.3, 0.3, 0.5)];
    if let Some(section) = current {
        let start = -90.0 + 360.0 * (-0.5 + section as f32) / active;
        toruses.push(match current_color {
            None => torus(start, angle_size, 0.4, 0.4, 0.4, 0.7),
            Some([r, g, b]) => torus(start, angle_size, r, g, b, 0.3),
        });
    }
    let x_diff = f64::from(mouse[0]) - f64::from(center[0]);
    let y_diff = f64::from(mouse[1]) - f64::from(center[1]);
    let distance = (x_diff * x_diff + y_diff * y_diff).sqrt();
    let mut selection = None;
    if distance > f64::from(SELECT_RADIUS) {
        let angle = (mth::RAD_TO_DEG as f64 * mth::atan2(y_diff, x_diff)) as f32;
        let mode_size = 180.0 / active;
        toruses.push(torus(angle - mode_size, angle_size, 0.8, 0.8, 0.8, 0.3));
        let selection_angle = wrap_degrees(angle + mode_size + 90.0);
        let drawn = (selection_angle * (active / 360.0)) as usize;
        selection = Some(drawn);
        toruses.push(torus(
            -90.0 + 360.0 * (-0.5 + drawn as f32) / active,
            angle_size,
            0.6,
            0.6,
            0.6,
            0.7,
        ));
    }
    let slices = (0..modes)
        .map(|position| {
            let degrees = 270.0 + 360.0 * (position as f32 / active);
            let angle = f64::from(mth::DEG_TO_RAD * degrees);
            let x = mth::cos(angle) * MIDDLE_DISTANCE;
            let y = mth::sin(angle) * MIDDLE_DISTANCE;
            Slice {
                icon: [
                    java_round(x - ICON_LEFT as f32),
                    java_round(y - ICON_UP as f32),
                ],
                label: [x, y],
            }
        })
        .collect();
    Frame {
        toruses,
        slices,
        selection,
    }
}

fn torus(start_angle: f32, size_angle: f32, r: f32, g: f32, b: f32, a: f32) -> Torus {
    Torus {
        start_angle,
        size_angle,
        inner: INNER,
        outer: OUTER,
        rgba: [r, g, b, a],
    }
}

/// `Math.round(float)`: `floor(x + 0.5)`.
fn java_round(value: f32) -> i32 {
    (value + 0.5).floor() as i32
}

/// Os vértices de um anel (`TorusRenderState.buildVertices`): um par (fora, dentro) por passo, de `i = 0` a `DRAWS * size / 360` inclusive. O `GL_TRIANGLE_STRIP`
/// liga cada par ao seguinte; quem desenha faz um quadrilátero por par de passos.
pub fn torus_pairs(center: [f32; 2], torus: &Torus) -> Vec<([f32; 2], [f32; 2])> {
    let draws = DRAWS * (torus.size_angle / 360.0);
    let mut out = Vec::new();
    let mut i = 0_u32;
    while i as f32 <= draws {
        let degrees = torus.start_angle + (i as f32 / DRAWS) * 360.0;
        let angle = f64::from(mth::DEG_TO_RAD * degrees);
        let (cos, sin) = (mth::cos(angle), mth::sin(angle));
        out.push((
            [center[0] + torus.outer * cos, center[1] + torus.outer * sin],
            [center[0] + torus.inner * cos, center[1] + torus.inner * sin],
        ));
        i += 1;
    }
    out
}

/// `ScrollIncrementer(discrete = true)`: a rolagem soma, e cada passo inteiro vira um deslocamento de -1 a 1. Passado mais de um segundo (20 tiques)
/// desde a última, a soma recomeça.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollIncrementer {
    last_scroll_time: i64,
    scroll_delta: f64,
}

impl Default for ScrollIncrementer {
    fn default() -> Self {
        Self::new()
    }
}

impl ScrollIncrementer {
    pub const fn new() -> Self {
        Self {
            last_scroll_time: -1,
            scroll_delta: 0.0,
        }
    }

    /// `scroll(double delta)` no tique `gui_ticks` (`discrete`).
    pub fn scroll(&mut self, delta: f64, gui_ticks: i64) -> i32 {
        if gui_ticks - self.last_scroll_time > 20 {
            self.scroll_delta = 0.0;
        }
        self.last_scroll_time = gui_ticks;
        self.scroll_delta += delta;
        let shift = self.scroll_delta as i32;
        self.scroll_delta %= 1.0;
        shift.clamp(-1, 1)
    }
}

/// `mouseScrolled(radialData, shift)`: o modo para onde a rolagem vai, o próximo de `current` (ciclo), ou `None` se não há modo atual ou o deslocamento é 0
/// (o deslocamento 0 consome o evento sem mudar nada, o `None` sem modo atual deixa o evento passar).
pub fn scrolled(modes: usize, current: Option<usize>, shift: i32) -> Scroll {
    if shift == 0 {
        return Scroll::Consumed;
    }
    match current {
        Some(index) if modes > 0 => Scroll::Select(
            (index as i64 + i64::from(shift)).rem_euclid(modes as i64) as usize,
        ),
        _ => Scroll::Passed,
    }
}

/// O que a rolagem fez.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scroll {
    /// O evento foi consumido sem escolher nada.
    Consumed,
    /// Escolheu o modo de índice dado e já o manda (`updateSelection`).
    Select(usize),
    /// Sem modo atual: o evento segue para a classe pai.
    Passed,
}

/// `updateSelection`: manda o modo escolhido, se há uma escolha e ela difere do modo atual.
pub fn selection_to_send(selection: Option<usize>, current: Option<usize>) -> Option<usize> {
    selection.filter(|chosen| Some(*chosen) != current)
}

/// O que o menu faz a cada tique do cliente (`ClientTickHandler.tickEnd`, a parte do radial).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tick {
    /// Nada a fazer: a tecla de contexto não vale (há outra tela aberta).
    Idle,
    /// Abrir (ou manter aberto) o menu do item no slot dado.
    Open(Slot),
    /// Fechar a tela que está aberta (o menu).
    Close,
}

/// A mão que tem o item de rádio: a principal primeiro (`updateSelectorRenderer(MAINHAND)`), depois a secundária.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    MainHand,
    OffHand,
}

/// `RadialConflictContext.isActive`: nenhuma tela aberta, ou a do próprio menu.
pub fn tick(
    screen_is_none_or_radial: bool,
    key_pressed: bool,
    main_hand_has_radial: bool,
    off_hand_has_radial: bool,
    screen_open: bool,
) -> Tick {
    if !screen_is_none_or_radial {
        return Tick::Idle;
    }
    if key_pressed {
        if main_hand_has_radial {
            return Tick::Open(Slot::MainHand);
        }
        if off_hand_has_radial {
            return Tick::Open(Slot::OffHand);
        }
    }
    if screen_open { Tick::Close } else { Tick::Idle }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CENTER: [f32; 2] = [480.0, 270.0];

    #[test]
    fn an_idle_cursor_at_the_center_selects_nothing_and_draws_the_base_and_current() {
        let frame = frame([480, 270], CENTER, 8, Some(2), Some([0.1, 0.2, 0.3]));
        assert_eq!(frame.selection, None);
        assert_eq!(frame.toruses.len(), 2);
        assert_eq!(frame.toruses[0].size_angle, 360.0);
        assert_eq!(frame.toruses[0].rgba, [0.3, 0.3, 0.3, 0.5]);
        // Fatia 2 de 8: de -90 + 360 * 1,5 / 8 = -22,5 graus, 45 de largura.
        assert_eq!(frame.toruses[1].start_angle, -22.5);
        assert_eq!(frame.toruses[1].size_angle, 45.0);
        assert_eq!(frame.toruses[1].rgba, [0.1, 0.2, 0.3, 0.3]);
    }

    #[test]
    fn a_current_mode_without_color_is_gray() {
        let frame = frame([480, 270], CENTER, 4, Some(0), None);
        assert_eq!(frame.toruses[1].rgba, [0.4, 0.4, 0.4, 0.7]);
    }

    #[test]
    fn the_cursor_picks_the_slice_it_points_at() {
        // Fatia 0 fica no alto (270 graus: o ícone de cima), e as outras seguem no sentido horário.
        for (mouse, expected) in [
            ([480, 200], 0),
            ([560, 270], 2),
            ([480, 340], 4),
            ([400, 270], 6),
            ([540, 215], 1),
            ([420, 215], 7),
        ] {
            let frame = frame(mouse, CENTER, 8, None, None);
            assert_eq!(frame.selection, Some(expected), "{mouse:?}");
            // Base, realce do mouse e fatia escolhida.
            assert_eq!(frame.toruses.len(), 3);
            assert_eq!(frame.toruses[1].rgba, [0.8, 0.8, 0.8, 0.3]);
            assert_eq!(frame.toruses[2].rgba, [0.6, 0.6, 0.6, 0.7]);
        }
    }

    #[test]
    fn inside_the_select_radius_nothing_is_picked() {
        assert_eq!(frame([489, 270], CENTER, 8, None, None).selection, None);
        assert_eq!(frame([480, 280], CENTER, 8, None, None).selection, None);
        assert_eq!(frame([480, 281], CENTER, 8, None, None).selection, Some(4));
    }

    #[test]
    fn the_icons_sit_on_the_middle_ring() {
        let frame = frame([480, 270], CENTER, 8, None, None);
        assert_eq!(frame.slices.len(), 8);
        // 270 graus: x = 0, y = -70.
        assert_eq!(frame.slices[0].icon, [-12, -90]);
        assert!(frame.slices[0].label[0].abs() < 1.0e-3);
        assert!((frame.slices[0].label[1] + 70.0).abs() < 1.0e-3);
        // 0 graus (a fatia 2): x = 70, y = 0.
        assert_eq!(frame.slices[2].icon, [58, -20]);
    }

    #[test]
    fn the_ring_has_a_vertex_pair_per_step() {
        let ring = torus(0.0, 360.0, 0.3, 0.3, 0.3, 0.5);
        // `i` de 0 a 720 inclusive.
        assert_eq!(torus_pairs(CENTER, &ring).len(), 721);
        let slice = torus(-22.5, 45.0, 0.3, 0.3, 0.3, 0.5);
        assert_eq!(torus_pairs(CENTER, &slice).len(), 91);
        let (outer, inner) = torus_pairs(CENTER, &ring)[0];
        assert_eq!(outer, [580.0, 270.0]);
        assert_eq!(inner, [520.0, 270.0]);
    }

    #[test]
    fn scrolling_steps_one_slice_and_wraps() {
        assert_eq!(scrolled(8, Some(7), 1), Scroll::Select(0));
        assert_eq!(scrolled(8, Some(0), -1), Scroll::Select(7));
        assert_eq!(scrolled(8, Some(3), 0), Scroll::Consumed);
        assert_eq!(scrolled(8, None, 1), Scroll::Passed);
    }

    #[test]
    fn the_incrementer_is_discrete_and_forgets_after_a_second() {
        let mut scroll = ScrollIncrementer::default();
        assert_eq!(scroll.scroll(0.4, 10), 0);
        assert_eq!(scroll.scroll(0.4, 11), 0);
        assert_eq!(scroll.scroll(0.4, 12), 1);
        assert_eq!(scroll.scroll(5.0, 13), 1);
        assert_eq!(scroll.scroll(-3.0, 14), -1);
        // Passados 21 tiques a soma recomeça.
        assert_eq!(scroll.scroll(0.5, 14), 0);
        assert_eq!(scroll.scroll(0.0, 36), 0);
        assert_eq!(scroll.scroll(0.6, 37), 0);
    }

    #[test]
    fn only_a_different_choice_is_sent() {
        assert_eq!(selection_to_send(Some(3), Some(3)), None);
        assert_eq!(selection_to_send(Some(4), Some(3)), Some(4));
        assert_eq!(selection_to_send(Some(4), None), Some(4));
        assert_eq!(selection_to_send(None, Some(3)), None);
    }

    #[test]
    fn the_tick_opens_with_the_key_and_closes_without_it() {
        assert_eq!(tick(true, true, true, false, false), Tick::Open(Slot::MainHand));
        assert_eq!(tick(true, true, false, true, false), Tick::Open(Slot::OffHand));
        assert_eq!(tick(true, true, true, true, true), Tick::Open(Slot::MainHand));
        // Sem a tecla, ou sem item de rádio, a tela do menu fecha.
        assert_eq!(tick(true, false, true, false, true), Tick::Close);
        assert_eq!(tick(true, true, false, false, true), Tick::Close);
        // Sem tela aberta não há o que fechar.
        assert_eq!(tick(true, false, true, false, false), Tick::Idle);
        // Outra tela aberta: o contexto não vale.
        assert_eq!(tick(false, true, true, false, true), Tick::Idle);
    }
}
