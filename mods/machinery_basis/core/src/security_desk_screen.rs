//! A tela da Security Desk (`GuiSecurityDesk`) sem o desenho: onde cada elemento fica, o que o mouse e o teclado fazem a cada um, na ordem do Java, e o que a tela
//! manda ao servidor. O texto da caixa (`GuiTextField`, um `EditBox`) é de quem desenha; aqui fica só o que a tela decide com ele.
//!
//! As coordenadas são as do Java, relativas ao canto da tela (`getGuiLeft()`, `getGuiTop()`).
//!
//! Lido de `GuiSecurityDesk`, `GuiMekanism` (`mouseClicked`, `keyPressed`, `charTyped`, `mouseScrolled`, `mouseDragged`), `GuiElement`, `MekanismButton`,
//! `GuiTextField`, `GuiTextScrollList`, `GuiScrollList`, `GuiScrollableElement` e `AbstractWidget` (26.3).

use crate::recipe_data::SecurityMode;
use crate::scroll_list::ScrollList;
use crate::security_desk::{DeskAction, DeskView};

/// `super(container, inv, title, DEFAULT_IMAGE_WIDTH, DEFAULT_IMAGE_HEIGHT + 64)`.
pub const IMAGE_WIDTH: i32 = 176;
pub const IMAGE_HEIGHT: i32 = 166 + 64;
/// `titleLabelY -= 2`: o título fica em 4 e não em 6.
pub const TITLE_LABEL_Y: i32 = 4;
/// `MekanismContainer.getInventoryYOffset()` é 84 numa tela de 166: aqui o inventário desce os 64 da tela.
pub const INVENTORY_Y: i32 = 84 + 64;

/// Os três `GuiElementHolder`.
pub const HOLDERS: [[i32; 4]; 3] = [[141, 13, 26, 37], [141, 54, 26, 34], [141, 92, 26, 37]];
/// Os dois `GuiSlot(SlotType.INNER_HOLDER_SLOT, ...)`, sobre os slots do tile (146, 18) e (146, 97).
pub const INNER_SLOTS: [[i32; 2]; 2] = [[145, 17], [145, 96]];
/// `GuiSecurityLight(this, 144, 77, ...)`: 8 por 8, com a luz de 6 por 6 dentro.
pub const LIGHT: [i32; 4] = [144, 77, 8, 8];
/// `textureOnly(PUBLIC, this, 145, 32, 18, 18)` e `textureOnly(PRIVATE, this, 145, 111, 18, 18)`.
pub const PUBLIC_ICON: [i32; 4] = [145, 32, 18, 18];
pub const PRIVATE_ICON: [i32; 4] = [145, 111, 18, 18];
/// `GuiTextScrollList(this, 13, 13, 122, 42)`, linhas de 10.
pub const LIST: [i32; 4] = [13, 13, 122, 42];
pub const LIST_ELEMENT_HEIGHT: i32 = 10;
/// `TranslationButton(this, 13, 81, 122, 20, BUTTON_REMOVE, ...)`.
pub const REMOVE: [i32; 4] = [13, 81, 122, 20];
/// `TRUSTED_X` e `GuiTextField(this, TRUSTED_X, 68, 99, 11)`.
pub const TRUSTED_X: i32 = 35;
pub const FIELD: [i32; 4] = [TRUSTED_X, 68, 99, 11];
/// `ButtonType.NORMAL`: o botão de confirmar, `field.getRelativeRight() - field.getHeight()`, do tamanho da altura da caixa.
pub const CHECKMARK: [i32; 4] = [FIELD[0] + FIELD[2] - FIELD[3], FIELD[1], FIELD[3], FIELD[3]];
/// O `EditBox` de dentro (`GuiTextField.updateTextField`): `x + 2`, `y + 1 + (int) (height / 2F - 4)`, a largura menos a do botão de confirmar e 2, e a altura da caixa.
pub const EDIT_BOX: [i32; 4] = [
    FIELD[0] + 2,
    FIELD[1] + 1 + 1,
    FIELD[2] - (FIELD[3] + 2),
    FIELD[3],
];
/// Os três `MekanismImageButton` de modo, na ordem de `SecurityMode`.
pub const MODE_BUTTONS: [[i32; 4]; 3] = [[13, 113, 40, 16], [54, 113, 40, 16], [95, 113, 40, 16]];
/// `TooltipToggleButton(this, 146, 59, 16, ...)`.
pub const OVERRIDE: [i32; 4] = [146, 59, 16, 16];

/// Os textos de `drawForegroundText`: `[x, y, largura, maxLengthPad]` de cada `drawScrollingString`.
pub const TRUSTED_PLAYERS_TEXT: [i32; 4] = [13, 57, 122, 0];
pub const MODE_TEXT: [i32; 4] = [13, 103, 122, 0];
pub const ADD_TEXT: [i32; 4] = [1, 70, TRUSTED_X - 1, 3];
/// `renderInventoryTextAndOther`: "Inventory" em 53 pixels, e o dono à direita a partir de `inventoryLabelX + 50`, com `maxLengthPad` 6.
pub const INVENTORY_TEXT_WIDTH: i32 = 53;
pub const OWNER_TEXT_START: i32 = 8 + 50;
pub const OWNER_TEXT_PAD: i32 = 6;

/// Um elemento da tela que recebe clique, foco ou tooltip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeskWidget {
    /// O `GuiSecurityLight`: ativo e sem ação (o clique nele é dele, e tira o foco do resto).
    Light,
    List,
    Remove,
    Field,
    Mode(SecurityMode),
    Override,
}

/// Quais botões estão ligados (`active`), como `updateButtons` os deixou da última vez.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeskActive {
    pub remove: bool,
    pub modes: [bool; 3],
    pub override_button: bool,
    /// O botão de confirmar da caixa: ligado enquanto a caixa tem texto (o `responder` do `EditBox`).
    pub checkmark: bool,
}

/// O que um clique, uma tecla ou um caractere pede a quem tem o jogo.
#[derive(Clone, Debug, PartialEq)]
pub enum DeskOutput {
    /// `PacketGuiInteract`: o botão de menu.
    Menu(i32),
    /// `PacketAddTrusted`.
    AddTrusted(String),
    /// `trustedField.setText("")`.
    ClearField,
    /// `AbstractWidget.playDownSound` de um `MekanismButton` (`UI_BUTTON_CLICK`).
    ClickSound,
    /// `EditBox.onClick` com o botão esquerdo: o cursor vai para o `x` do mouse (relativo à tela).
    FieldCursor(f64),
    /// A caixa ganhou o foco (`EditBox.setFocused(true)`, que reinicia o pisca do cursor).
    FieldFocused,
}

/// Uma tecla, com o que a tela quer saber dela.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeskKey {
    /// `KeyEvent.isConfirmation`: Enter e o Enter do teclado numérico.
    Enter,
    /// A barra de espaço (`isSelection` é ela ou a confirmação).
    Space,
    /// `isEscape`.
    Escape,
    /// `isCycleFocus`: Tab.
    Tab,
    Other,
}

/// O que a tela fez com uma tecla.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyResult {
    /// Ninguém a quis: segue para a tela do jogo (o Escape fecha, a tecla do inventário fecha).
    Pass,
    /// Um elemento a consumiu.
    Consumed,
    /// A caixa de texto a consumiu e é dela (`textField.keyPressed`, ou a colagem): quem tem o `EditBox` a aplica.
    Field,
}

/// O estado da tela enquanto ela está aberta.
#[derive(Clone, Debug, PartialEq)]
pub struct DeskScreen {
    pub list: ScrollList,
    names: Vec<String>,
    selected: Option<usize>,
    active: DeskActive,
    focus: Option<DeskWidget>,
}

fn contains(rect: [i32; 4], at: [f64; 2]) -> bool {
    at[0] >= f64::from(rect[0])
        && at[1] >= f64::from(rect[1])
        && at[0] < f64::from(rect[0] + rect[2])
        && at[1] < f64::from(rect[1] + rect[3])
}

impl DeskScreen {
    /// `addGuiElements`: os botões nascem ligados (o de confirmar, desligado: `addCheckmarkButton`), e o `updateButtons()` do fim os acerta.
    pub fn new(view: &DeskView<'_>) -> Self {
        let mut screen = Self {
            list: ScrollList::new(LIST[0], LIST[1], LIST[2], LIST[3])
                .with_element_height(LIST_ELEMENT_HEIGHT),
            names: Vec::new(),
            selected: None,
            active: DeskActive {
                remove: true,
                modes: [true; 3],
                override_button: true,
                checkmark: false,
            },
            focus: None,
        };
        screen.update_buttons(view);
        screen
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn active(&self) -> DeskActive {
        self.active
    }

    pub fn focus(&self) -> Option<DeskWidget> {
        self.focus
    }

    /// A caixa aceita o teclado (`GuiTextField.canWrite`: o `EditBox` visível, focado e editável).
    pub fn field_focused(&self) -> bool {
        self.focus == Some(DeskWidget::Field)
    }

    /// `GuiTextScrollList.setText`: a seleção que não cabe mais some, e a lista que deixa de precisar da barra volta ao começo.
    fn set_text(&mut self, names: &[String]) {
        if self
            .selected
            .is_some_and(|selected| selected >= names.len())
        {
            self.selected = None;
        }
        self.names = names.to_vec();
        if !self.list.needs_scroll_bars(self.names.len()) {
            self.list.reset_scroll();
        }
    }

    /// `GuiSecurityDesk.updateButtons`: roda ao montar a tela, a cada tick (`containerTick`), antes de cada clique (`mouseClicked`) e depois de cada ação. A lista e o
    /// botão de tirar só mudam se o tile tem dono.
    pub fn update_buttons(&mut self, view: &DeskView<'_>) {
        if let Some(names) = view.names() {
            self.set_text(names);
            self.active.remove = self.selected.is_some();
        }
        let buttons = view.buttons();
        self.active.modes = [buttons.public, buttons.private, buttons.trusted];
        self.active.override_button = buttons.override_button;
    }

    /// O `responder` do `EditBox`: a cada mudança do texto, o botão de confirmar liga se há texto.
    pub fn text_changed(&mut self, text: &str) {
        self.active.checkmark = !text.is_empty();
    }

    /// As linhas visíveis: o índice na lista e o nome, da primeira à última que cabe (`renderForeground`).
    pub fn visible_rows(&self) -> impl Iterator<Item = (usize, usize, &str)> {
        let first = self.list.first_visible(self.names.len());
        (0..self.list.focused() as usize).filter_map(move |row| {
            self.names
                .get(first + row)
                .map(|name| (row, first + row, name.as_str()))
        })
    }

    /// `renderElements`: a linha (entre as visíveis) que leva o sprite de seleção.
    pub fn selected_row(&self) -> Option<usize> {
        let first = self.list.first_visible(self.names.len());
        let selected = self.selected?;
        (selected >= first && selected < first + self.list.focused() as usize)
            .then(|| selected - first)
    }

    /// `setTrusted(field)`: o Enter na caixa e o botão de confirmar. Só para o dono: manda o nome (se o cliente o manda), limpa a caixa e `updateButtons`.
    fn submit(&mut self, view: &DeskView<'_>, text: &str, out: &mut Vec<DeskOutput>) {
        if let Some(name) = view.submit_name(text) {
            if let Some(name) = name {
                out.push(DeskOutput::AddTrusted(name.to_owned()));
            }
            out.push(DeskOutput::ClearField);
            self.text_changed("");
            self.update_buttons(view);
        }
    }

    /// O `onLeftClick` de um botão (o direito é o mesmo: os construtores usados passam a mesma ação aos dois).
    fn press(&mut self, widget: DeskWidget, view: &DeskView<'_>, out: &mut Vec<DeskOutput>) {
        match widget {
            DeskWidget::Remove => {
                if let (Some(_), Some(selected)) = (view.frequency, self.selected) {
                    out.push(DeskOutput::Menu(
                        DeskAction::Remove(selected as i32).button(),
                    ));
                    self.selected = None;
                    self.update_buttons(view);
                }
            }
            DeskWidget::Mode(mode) => {
                out.push(DeskOutput::Menu(DeskAction::Mode(mode).button()));
                self.update_buttons(view);
            }
            DeskWidget::Override => {
                out.push(DeskOutput::Menu(DeskAction::ToggleOverride.button()));
                self.update_buttons(view);
            }
            DeskWidget::Light | DeskWidget::List | DeskWidget::Field => {}
        }
    }

    fn button_at(&self, at: [f64; 2]) -> Option<DeskWidget> {
        if self.active.override_button && contains(OVERRIDE, at) {
            return Some(DeskWidget::Override);
        }
        for mode in [
            SecurityMode::Trusted,
            SecurityMode::Private,
            SecurityMode::Public,
        ] {
            if self.active.modes[mode.ordinal()] && contains(MODE_BUTTONS[mode.ordinal()], at) {
                return Some(DeskWidget::Mode(mode));
            }
        }
        (self.active.remove && contains(REMOVE, at)).then_some(DeskWidget::Remove)
    }

    /// O elemento ativo sob o mouse (`GuiMekanism.findElementOver` com `isMouseOver`, que pede `active`): o que mostra o sprite de foco e o tooltip. O botão de
    /// confirmar é filho da caixa e conta como ela.
    pub fn hovered(&self, at: [f64; 2]) -> Option<DeskWidget> {
        if let Some(button) = self.button_at(at) {
            return Some(button);
        }
        if contains(FIELD, at) {
            return Some(DeskWidget::Field);
        }
        if contains(LIST, at) {
            return Some(DeskWidget::List);
        }
        contains(LIGHT, at).then_some(DeskWidget::Light)
    }

    /// O botão de confirmar está sob o mouse e ligado (o sprite de foco dele).
    pub fn checkmark_hovered(&self, at: [f64; 2]) -> bool {
        self.active.checkmark && contains(CHECKMARK, at)
    }

    /// `GuiSecurityDesk.mouseClicked`: `updateButtons()` e depois o clique vai aos elementos, do último posto ao primeiro (`GuiMekanism.mouseClicked`). `button` é 0
    /// para o esquerdo e 1 para o direito; `text` é o que a caixa tem. Devolve se um elemento ficou com o clique (senão ele segue para os slots) e o que fazer.
    pub fn click(
        &mut self,
        view: &DeskView<'_>,
        at: [f64; 2],
        button: i32,
        text: &str,
    ) -> (bool, Vec<DeskOutput>) {
        let mut out = Vec::new();
        self.update_buttons(view);
        let left = button == 0;
        let right = button == 1;
        if !left && !right {
            self.focus = None;
            return (false, out);
        }
        // Os botões: os dois botões do mouse valem (`isValidClickButton` com `onRightClick`), e o som toca antes da ação.
        if let Some(widget) = self.button_at(at) {
            out.push(DeskOutput::ClickSound);
            self.press(widget, view, &mut out);
            self.focus = Some(widget);
            return (true, out);
        }
        // A caixa: primeiro o `EditBox` (o esquerdo põe o cursor, o direito limpa o texto), depois o botão de confirmar, depois a área da caixa (só o esquerdo).
        if contains(EDIT_BOX, at) {
            if right {
                out.push(DeskOutput::ClearField);
                self.text_changed("");
            } else {
                out.push(DeskOutput::FieldCursor(at[0]));
            }
            self.focus_field(&mut out);
            return (true, out);
        }
        if self.active.checkmark && contains(CHECKMARK, at) {
            // `MekanismImageButton` com a mesma ação nos dois botões; depois `parent.setFocused(this)` deixa a caixa focada.
            out.push(DeskOutput::ClickSound);
            self.submit(view, text, &mut out);
            self.focus_field(&mut out);
            return (true, out);
        }
        if left && contains(FIELD, at) {
            self.focus_field(&mut out);
            return (true, out);
        }
        if left && contains(LIST, at) {
            // `GuiScrollableElement.onClick` (o polegar) e depois `GuiScrollList.onClick` (a linha), os dois sempre.
            let total = self.names.len();
            self.list.press_thumb(at, total);
            if let Some(row) = self.list.click_row(at, total) {
                self.selected = row;
            }
            self.focus = Some(DeskWidget::List);
            return (true, out);
        }
        if left && contains(LIGHT, at) {
            self.focus = Some(DeskWidget::Light);
            return (true, out);
        }
        // Nenhum elemento: `clearFocus()`, e o clique segue para os slots.
        self.focus = None;
        (false, out)
    }

    fn focus_field(&mut self, out: &mut Vec<DeskOutput>) {
        if self.focus != Some(DeskWidget::Field) {
            out.push(DeskOutput::FieldFocused);
        }
        self.focus = Some(DeskWidget::Field);
    }

    /// `GuiMekanism.mouseDragged`: só o elemento focado, com o botão esquerdo; aqui só a lista arrasta (o polegar).
    pub fn drag(&mut self, at: [f64; 2]) {
        if self.focus == Some(DeskWidget::List)
            && self.list.dragging()
            && self.list.needs_scroll_bars(self.names.len())
        {
            self.list.drag_to(at[1]);
        }
    }

    /// `mouseReleased` do elemento focado (`GuiScrollableElement.onRelease`).
    pub fn release(&mut self) {
        self.list.release();
    }

    /// `GuiScrollList.mouseScrolled`: a roda sobre a lista (com a barra). Devolve se a lista a tratou.
    pub fn scroll(&mut self, at: [f64; 2], delta: f64) -> bool {
        self.list.contains(at) && delta != 0.0 && self.list.adjust_scroll(delta, self.names.len())
    }

    /// `GuiMekanism.keyPressed`: os elementos antes da tela. A caixa focada fica com tudo menos o Escape e o Tab, e o Enter nela é o `setTrusted`; um botão focado e
    /// ligado é apertado pelo Enter e pelo espaço (`MekanismButton.keyPressed`, com o som). O Tab que ninguém quis leva o foco à caixa, o único elemento que aceita a
    /// navegação por Tab (`supportsTabNavigation`).
    pub fn key(
        &mut self,
        view: &DeskView<'_>,
        key: DeskKey,
        text: &str,
    ) -> (KeyResult, Vec<DeskOutput>) {
        let mut out = Vec::new();
        if self.field_focused() {
            match key {
                DeskKey::Escape => return (KeyResult::Pass, out),
                DeskKey::Tab => return (KeyResult::Consumed, out),
                DeskKey::Enter => {
                    self.submit(view, text, &mut out);
                    return (KeyResult::Consumed, out);
                }
                DeskKey::Space | DeskKey::Other => return (KeyResult::Field, out),
            }
        }
        if matches!(key, DeskKey::Enter | DeskKey::Space)
            && let Some(widget @ (DeskWidget::Remove | DeskWidget::Mode(_) | DeskWidget::Override)) =
                self.focus
        {
            let on = match widget {
                DeskWidget::Remove => self.active.remove,
                DeskWidget::Mode(mode) => self.active.modes[mode.ordinal()],
                _ => self.active.override_button,
            };
            if on {
                out.push(DeskOutput::ClickSound);
                self.press(widget, view, &mut out);
                return (KeyResult::Consumed, out);
            }
        }
        if key == DeskKey::Tab {
            self.focus_field(&mut out);
            return (KeyResult::Consumed, out);
        }
        (KeyResult::Pass, out)
    }

    /// `GuiTextField.charTyped`: com a caixa focada, só o caractere que o `InputValidator.USERNAME` aceita chega ao `EditBox`. Devolve se a caixa o quer.
    pub fn char_typed(&self, c: char) -> bool {
        self.field_focused() && crate::security_desk::username_char(c)
    }

    /// A colagem (`event.isPaste()`): um caractere fora do `USERNAME` recusa o texto inteiro.
    pub fn paste_accepted(text: &str) -> bool {
        text.chars().all(crate::security_desk::username_char)
    }
}
