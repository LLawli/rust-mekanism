//! `GuiInsetElement`: uma aba na lateral da tela, com um botão de 18 por 18 dentro de um suporte de 26 de largura
//! (`GuiSideHolder`). Só a geometria e as regras de quem a recebe; o desenho (fundo do botão, suporte tingido, ícone) é do glue.
//! Os retângulos são `[x, y, largura, altura]` em pixels de GUI, relativos ao canto da tela, como no Java.

use crate::gui::{Rect, contains};

/// `GuiSideHolder`: a largura do suporte.
pub const HOLDER_WIDTH: i32 = 26;
/// O lado do botão de dentro (`innerSize`) das abas de 26 por 18.
pub const BUTTON_SIZE: i32 = 18;

/// Uma aba inset: onde o suporte fica, e de que lado da tela.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InsetTab {
    pub x: i32,
    pub y: i32,
    pub height: i32,
    pub left: bool,
}

impl InsetTab {
    /// Uma aba na lateral direita, como `new GuiXpContainmentTab(gui, source, false, y)`: o suporte começa em `imageWidth`.
    pub fn right(image_width: i32, y: i32, height: i32) -> Self {
        Self {
            x: image_width,
            y,
            height,
            left: false,
        }
    }

    /// O suporte (`GuiSideHolder`): 26 de largura.
    pub fn holder(&self) -> Rect {
        [
            self.x as f32,
            self.y as f32,
            HOLDER_WIDTH as f32,
            self.height as f32,
        ]
    }

    /// `border`: a folga entre o suporte e o botão (`(width - innerWidth) / 2`).
    fn border() -> i32 {
        (HOLDER_WIDTH - BUTTON_SIZE) / 2
    }

    /// O botão de dentro (`getButtonX`, `getButtonY`, `innerWidth` e `innerHeight`): a folga para dentro, e mais um pixel para o lado
    /// da tela (`left ? 1 : -1`).
    pub fn button(&self) -> Rect {
        let side = if self.left { 1 } else { -1 };
        [
            (self.x + Self::border() + side) as f32,
            (self.y + Self::border()) as f32,
            BUTTON_SIZE as f32,
            BUTTON_SIZE as f32,
        ]
    }

    /// `GuiInsetElement.isMouseOver`: só o botão de dentro responde, e só a aba visível e ativa.
    pub fn is_mouse_over(&self, visible: bool, active: bool, at: [f32; 2]) -> bool {
        active && visible && contains(self.button(), at)
    }
}

/// `GuiElement.drawButton`: o fundo do botão. Sem a aba ativa vale `button_inactive`, com o mouse em cima `button_focus`, senão `button`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonLook {
    Normal,
    Focus,
    Inactive,
}

impl ButtonLook {
    pub fn of(active: bool, hovered: bool) -> Self {
        if !active {
            Self::Inactive
        } else if hovered {
            Self::Focus
        } else {
            Self::Normal
        }
    }

    /// O nome do sprite do fundo, sem o namespace (`ButtonBackground.DEFAULT`: `button`, `button_focus`, `button_inactive`).
    pub fn sprite(self) -> &'static str {
        match self {
            Self::Normal => "button",
            Self::Focus => "button_focus",
            Self::Inactive => "button_inactive",
        }
    }
}

/// Uma aba que só informa (`GuiTexturedElement` de 26 por 26 na lateral, como `GuiEnergyTab` e `GuiHeatTab`): um sprite, sem suporte tingido nem
/// botão, e o tooltip quando o mouse está sobre o elemento todo (`GuiElement.isMouseOver`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InfoTab {
    pub rect: Rect,
}

impl InfoTab {
    /// O mouse sobre a aba: o retângulo inteiro, sem janela por cima (`isMouseOverCheckWindows`).
    pub fn is_mouse_over(&self, at: [f32; 2], blocked: bool) -> bool {
        !blocked && contains(self.rect, at)
    }
}

/// `GuiXpContainmentTab` (`SupportsXpContainment`): a aba que coleta a experiência guardada. Os valores são os do Java; o `y` e a cor
/// quem escolhe é o mod, porque dependem do que mais a tela tem (o livro de receitas desloca a aba).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct XpTab {
    pub tab: InsetTab,
}

/// O botão de `GuiXpContainmentTab`: 26 por 18, a altura do suporte.
pub const XP_TAB_HEIGHT: i32 = 26;
/// `y -= 28` quando o livro de receitas não está na tela (`isRecipeBookDisabled() || !supportsRecipeBook`): a aba sobe para onde ele ficaria.
pub const XP_TAB_RECIPE_BOOK_SHIFT: i32 = 28;

impl XpTab {
    /// `new GuiXpContainmentTab(gui, source, false, y, supportsRecipeBook)`: `recipe_book` é `!isRecipeBookDisabled() && supportsRecipeBook`.
    pub fn new(image_width: i32, y: i32, recipe_book: bool) -> Self {
        let y = if recipe_book {
            y
        } else {
            y - XP_TAB_RECIPE_BOOK_SHIFT
        };
        Self {
            tab: InsetTab::right(image_width, y, XP_TAB_HEIGHT),
        }
    }

    /// `tick`: `visible = dataSource.hasXpContainment()`.
    pub fn visible(has_containment: bool) -> bool {
        has_containment
    }

    /// O clique que `onClick` manda ao servidor (`PacketGuiInteract(COLLECT_XP)`): só o botão esquerdo (`isValidClickButton`), com a aba
    /// visível, dentro do botão. `blocked` é o ponto estar atrás de uma janela (`isMouseOverCheckWindows`).
    pub fn click(&self, has_containment: bool, at: [f32; 2], right: bool, blocked: bool) -> bool {
        !right
            && !blocked
            && self
                .tab
                .is_mouse_over(Self::visible(has_containment), true, at)
    }

    /// O foco do botão e o tooltip: o mouse sobre o botão, a aba visível e nenhuma janela por cima do ponto.
    pub fn hovered(&self, has_containment: bool, at: [f32; 2], blocked: bool) -> bool {
        !blocked
            && self
                .tab
                .is_mouse_over(Self::visible(has_containment), true, at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_holder_starts_at_the_image_width() {
        let tab = InsetTab::right(176, 90, 26);
        assert_eq!(tab.holder(), [176.0, 90.0, 26.0, 26.0]);
    }

    #[test]
    fn the_button_is_inset_by_the_border_and_pulled_a_pixel_toward_the_screen() {
        // border = (26 - 18) / 2 = 4: 176 + 4 - 1 = 179 on the right, x + 4 + 1 on the left.
        assert_eq!(
            InsetTab::right(176, 90, 26).button(),
            [179.0, 94.0, 18.0, 18.0]
        );
        let left = InsetTab {
            x: -26,
            y: 6,
            height: 26,
            left: true,
        };
        assert_eq!(left.button(), [-21.0, 10.0, 18.0, 18.0]);
        // The upgrade tab, `GuiUpgradeWindowTab` at (imageWidth, 6): the same button the window code hit-tests.
        assert_eq!(
            InsetTab::right(176, 6, 26).button(),
            [179.0, 10.0, 18.0, 18.0]
        );
    }

    #[test]
    fn only_the_button_answers_the_mouse() {
        let tab = InsetTab::right(176, 90, 26);
        assert!(tab.is_mouse_over(true, true, [179.0, 94.0]));
        assert!(tab.is_mouse_over(true, true, [196.5, 111.5]));
        // The holder's border, the pixel past the button and the pixel before it.
        assert!(!tab.is_mouse_over(true, true, [177.0, 92.0]));
        assert!(!tab.is_mouse_over(true, true, [197.0, 100.0]));
        assert!(!tab.is_mouse_over(true, true, [100.0, 100.0]));
        assert!(!tab.is_mouse_over(true, true, [188.0, 112.0]));
        assert!(!tab.is_mouse_over(true, true, [178.9, 100.0]));
        // Invisible or inactive.
        assert!(!tab.is_mouse_over(false, true, [188.0, 100.0]));
        assert!(!tab.is_mouse_over(true, false, [188.0, 100.0]));
    }

    #[test]
    fn the_button_background_follows_active_and_hover() {
        assert_eq!(ButtonLook::of(true, false), ButtonLook::Normal);
        assert_eq!(ButtonLook::of(true, true), ButtonLook::Focus);
        assert_eq!(ButtonLook::of(false, true), ButtonLook::Inactive);
        assert_eq!(ButtonLook::of(false, false), ButtonLook::Inactive);
        assert_eq!(ButtonLook::Focus.sprite(), "button_focus");
        assert_eq!(ButtonLook::Inactive.sprite(), "button_inactive");
        assert_eq!(ButtonLook::Normal.sprite(), "button");
    }

    #[test]
    fn an_info_tab_answers_over_its_whole_rect_unless_a_window_covers_it() {
        let tab = InfoTab {
            rect: [-26.0, 137.0, 26.0, 26.0],
        };
        assert!(tab.is_mouse_over([-26.0, 137.0], false));
        assert!(tab.is_mouse_over([-0.5, 162.5], false));
        assert!(!tab.is_mouse_over([0.0, 150.0], false));
        assert!(!tab.is_mouse_over([-10.0, 163.0], false));
        assert!(!tab.is_mouse_over([-27.0, 150.0], false));
        assert!(!tab.is_mouse_over([-10.0, 150.0], true));
    }

    #[test]
    fn the_xp_tab_sits_at_90_and_moves_up_without_a_recipe_book() {
        assert_eq!(
            XpTab::new(176, 90, true).tab.holder(),
            [176.0, 90.0, 26.0, 26.0]
        );
        assert_eq!(
            XpTab::new(176, 90, false).tab.holder(),
            [176.0, 62.0, 26.0, 26.0]
        );
        assert_eq!(
            XpTab::new(176, 90, true).tab.button(),
            [179.0, 94.0, 18.0, 18.0]
        );
    }

    #[test]
    fn the_xp_tab_answers_only_with_containment_and_the_left_button() {
        let tab = XpTab::new(176, 90, true);
        let inside = [188.0, 100.0];
        assert!(tab.click(true, inside, false, false));
        assert!(!tab.click(false, inside, false, false));
        assert!(!tab.click(true, inside, true, false));
        assert!(!tab.click(true, [100.0, 100.0], false, false));
        assert!(!tab.click(true, inside, false, true));
        assert!(tab.hovered(true, inside, false));
        assert!(!tab.hovered(false, inside, false));
        assert!(!tab.hovered(true, inside, true));
        assert!(!XpTab::visible(false));
        assert!(XpTab::visible(true));
    }
}
