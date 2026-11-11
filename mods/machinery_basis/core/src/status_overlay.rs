//! O texto do modo sobre a barra (`MekanismStatusOverlay`): o contador de 5 segundos, onde o texto fica e com que opacidade. O texto em si (o
//! `getScrollTextComponent` do item) e o desenho são de quem usa.

/// `MekanismStatusOverlay.BASE_TIMER`: `5 * TICKS_PER_SECOND`.
pub const BASE_TIMER: i32 = 100;

/// O estado do overlay: o contador e o último tique do `Hud` em que ele andou.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatusOverlay {
    timer: i32,
    last_tick: i32,
}

/// O que o quadro tem em volta (`Minecraft`, `Gui`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surroundings {
    pub gui_width: i32,
    pub gui_height: i32,
    /// `Hud.leftHeight` e `Hud.rightHeight`.
    pub left_height: i32,
    pub right_height: i32,
    /// `gameMode.canHurtPlayer()` (sobrevivência e aventura).
    pub can_hurt: bool,
    /// `Hud.overlayMessageTime > 0`: a barra de ação está na tela.
    pub overlay_message: bool,
    /// `Hud.getGuiTicks()`.
    pub gui_ticks: i32,
    /// Há jogador, não é espectador e o HUD não está escondido (F1).
    pub active: bool,
}

/// Onde e como o texto sai: o canto de cima à esquerda (`translate`, com a fração do centro), a opacidade (`timer / BASE_TIMER`) e a largura do texto.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Draw {
    pub x: f32,
    pub y: i32,
    pub alpha: f32,
    pub width: i32,
}

impl StatusOverlay {
    /// `setTimer`.
    pub fn set_timer(&mut self) {
        self.timer = BASE_TIMER;
    }

    pub fn timer(&self) -> i32 {
        self.timer
    }

    /// `render`: `text_width` é a largura do texto do item de modo na mão principal, ou `None` se a mão não tem item de modo ou o texto é nulo. O contador
    /// só anda uma vez por tique do `Hud`, e só enquanto o overlay passa pelas condições (`timer > 1` e `active`).
    pub fn render(&mut self, around: &Surroundings, text_width: Option<i32>) -> Option<Draw> {
        if !(self.timer > 1 && around.active) {
            return None;
        }
        let draw = text_width.map(|width| {
            let mut shift = 59.max(around.left_height.max(around.right_height));
            if !around.can_hurt {
                // O mesmo deslocamento de `Gui#renderSelectedItemName`.
                shift -= 14;
            } else if around.overlay_message {
                // Em sobrevivência, o texto cairia sobre a legenda da barra de ação.
                shift += 14;
            }
            shift += 13;
            Draw {
                x: (around.gui_width - width) as f32 / 2.0,
                y: around.gui_height - shift,
                alpha: self.timer as f32 / BASE_TIMER as f32,
                width,
            }
        });
        if self.last_tick != around.gui_ticks {
            self.last_tick = around.gui_ticks;
            self.timer -= 1;
        }
        draw
    }
}

/// `Color.rgbad(1, 1, 1, alpha)`: o alfa de 8 bits (`Math.round(a * 0xFF)`).
pub fn alpha_byte(alpha: f32) -> u8 {
    (f64::from(alpha) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn around(gui_ticks: i32) -> Surroundings {
        Surroundings {
            gui_width: 480,
            gui_height: 270,
            left_height: 49,
            right_height: 49,
            can_hurt: true,
            overlay_message: false,
            gui_ticks,
            active: true,
        }
    }

    #[test]
    fn nothing_is_drawn_until_the_timer_starts() {
        let mut overlay = StatusOverlay::default();
        assert_eq!(overlay.render(&around(1), Some(60)), None);
        assert_eq!(overlay.timer(), 0);
    }

    #[test]
    fn the_text_sits_thirteen_above_the_selected_item_name_line() {
        let mut overlay = StatusOverlay::default();
        overlay.set_timer();
        let draw = overlay.render(&around(1), Some(61)).unwrap();
        // Altura da tela 270, o deslocamento é o maior entre 59 e as barras (49), mais 13.
        assert_eq!(draw.y, 270 - (59 + 13));
        assert_eq!(draw.x, (480 - 61) as f32 / 2.0);
        assert_eq!(draw.alpha, 1.0);
        assert_eq!(draw.width, 61);
    }

    #[test]
    fn creative_moves_down_and_the_action_bar_moves_up() {
        let mut overlay = StatusOverlay::default();
        overlay.set_timer();
        let mut creative = around(1);
        creative.can_hurt = false;
        assert_eq!(overlay.render(&creative, Some(10)).unwrap().y, 270 - (59 - 14 + 13));
        let mut message = around(2);
        message.overlay_message = true;
        assert_eq!(overlay.render(&message, Some(10)).unwrap().y, 270 - (59 + 14 + 13));
        // Em criativo a barra de ação não conta (`else if`).
        let mut both = around(3);
        both.can_hurt = false;
        both.overlay_message = true;
        assert_eq!(overlay.render(&both, Some(10)).unwrap().y, 270 - (59 - 14 + 13));
    }

    #[test]
    fn tall_bars_push_the_text_up() {
        let mut overlay = StatusOverlay::default();
        overlay.set_timer();
        let mut tall = around(1);
        tall.left_height = 70;
        assert_eq!(overlay.render(&tall, Some(10)).unwrap().y, 270 - (70 + 13));
    }

    #[test]
    fn the_timer_runs_once_per_gui_tick_and_stops_at_one() {
        let mut overlay = StatusOverlay::default();
        overlay.set_timer();
        // Vários quadros no mesmo tique contam uma vez.
        for _ in 0..5 {
            overlay.render(&around(7), Some(10));
        }
        assert_eq!(overlay.timer(), BASE_TIMER - 1);
        let mut tick = 7;
        while overlay.timer() > 1 {
            tick += 1;
            assert!(overlay.render(&around(tick), Some(10)).is_some());
        }
        // Com o contador em 1 o texto não sai mais (`modeSwitchTimer > 1`).
        assert_eq!(overlay.render(&around(tick + 1), Some(10)), None);
        assert_eq!(overlay.timer(), 1);
    }

    #[test]
    fn the_fade_follows_the_timer() {
        let mut overlay = StatusOverlay::default();
        overlay.set_timer();
        let mut tick = 0;
        for _ in 0..50 {
            tick += 1;
            overlay.render(&around(tick), Some(10));
        }
        let draw = overlay.render(&around(tick + 1), Some(10)).unwrap();
        assert_eq!(draw.alpha, 0.5);
        assert_eq!(alpha_byte(draw.alpha), 128);
        assert_eq!(alpha_byte(0.02), 5);
    }

    #[test]
    fn a_hidden_hud_or_a_spectator_neither_draws_nor_counts() {
        let mut overlay = StatusOverlay::default();
        overlay.set_timer();
        let mut hidden = around(1);
        hidden.active = false;
        assert_eq!(overlay.render(&hidden, Some(10)), None);
        assert_eq!(overlay.timer(), BASE_TIMER);
    }

    #[test]
    fn a_hand_without_mode_item_still_counts_the_timer_down() {
        let mut overlay = StatusOverlay::default();
        overlay.set_timer();
        assert_eq!(overlay.render(&around(4), None), None);
        assert_eq!(overlay.timer(), BASE_TIMER - 1);
    }
}
