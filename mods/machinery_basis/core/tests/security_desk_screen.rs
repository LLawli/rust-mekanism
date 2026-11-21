//! A tela da Security Desk sem o jogo (`security_desk_screen`): a seleção e a rolagem da lista, o foco, e o que cada botão, a caixa e o teclado mandam. Os valores
//! esperados são os de `GuiSecurityDesk`, `GuiTextScrollList`, `GuiScrollList`, `GuiScrollableElement`, `MekanismButton` e `GuiTextField` (26.3), lidos do Java: a
//! tela é do cliente e o `GameTestServer` não a roda; a comparação em jogo com o original é por pixel.
use machinery_basis_core::security::SecurityMode;
use machinery_basis_core::security_desk::{BUTTON_MODE, BUTTON_OVERRIDE, BUTTON_REMOVE, DeskView};
use machinery_basis_core::security_desk_screen::{
    self as screen, DeskActive, DeskKey, DeskOutput, DeskScreen, DeskWidget, KeyResult,
};
use machinery_basis_core::security_frequency::SecurityFrequency;

const OWNER: u128 = 0x11;
const OTHER: u128 = 0x22;

fn frequency(mode: SecurityMode, names: &[&str]) -> SecurityFrequency {
    let mut frequency = SecurityFrequency::new(Some(OWNER), mode, &|_| "Dono".to_owned());
    for (index, name) in names.iter().enumerate() {
        frequency.add_trusted(0x100 + index as u128, name);
    }
    frequency
}

fn view(frequency: &SecurityFrequency, viewer: u128) -> DeskView<'_> {
    DeskView {
        owner: Some(OWNER),
        frequency: Some(frequency),
        viewer: Some(viewer),
    }
}

/// O meio de um retângulo.
fn middle(rect: [i32; 4]) -> [f64; 2] {
    [
        f64::from(rect[0]) + f64::from(rect[2]) / 2.0,
        f64::from(rect[1]) + f64::from(rect[3]) / 2.0,
    ]
}

/// O ponto da linha `row` da lista (linhas de 10 a partir de `y + 1`).
fn row_point(row: i32) -> [f64; 2] {
    [
        f64::from(screen::LIST[0] + 20),
        f64::from(screen::LIST[1] + 1 + screen::LIST_ELEMENT_HEIGHT * row + 4),
    ]
}

#[test]
fn the_layout_is_the_one_of_the_java_screen() {
    assert_eq!((screen::IMAGE_WIDTH, screen::IMAGE_HEIGHT), (176, 230));
    assert_eq!(screen::INVENTORY_Y, 148);
    assert_eq!(screen::TITLE_LABEL_Y, 4);
    // `field.getRelativeRight() - field.getHeight()`: 35 + 99 - 11.
    assert_eq!(screen::CHECKMARK, [123, 68, 11, 11]);
    // O `EditBox`: x + 2, y + 1 + (int) (5,5 - 4), 99 - (11 + 2).
    assert_eq!(screen::EDIT_BOX, [37, 70, 86, 11]);
    let frequency = frequency(SecurityMode::Public, &[]);
    let desk = DeskScreen::new(&view(&frequency, OWNER));
    // (42 - 2) / 10 linhas, a barra em `width - 6`.
    assert_eq!(desk.list.focused(), 4);
    assert_eq!(desk.list.bar_x_shift(), 116);
    assert_eq!(desk.list.thumb_rect(), [13 + 116, 15, 4, 4]);
}

#[test]
fn the_buttons_start_as_update_buttons_leaves_them() {
    let public = frequency(SecurityMode::Public, &["a"]);
    let desk = DeskScreen::new(&view(&public, OWNER));
    assert_eq!(
        desk.active(),
        DeskActive {
            remove: false,
            modes: [false, true, true],
            override_button: true,
            checkmark: false
        }
    );
    assert_eq!(desk.names(), ["a".to_owned()]);
    let trusted = frequency(SecurityMode::Trusted, &[]);
    assert_eq!(
        DeskScreen::new(&view(&trusted, OWNER)).active().modes,
        [true, true, false]
    );
    // Quem não é o dono: nenhum botão de modo nem o de sobreposição; a lista aparece do mesmo jeito.
    let stranger = DeskScreen::new(&view(&public, OTHER));
    assert_eq!(
        stranger.active(),
        DeskActive {
            remove: false,
            modes: [false; 3],
            override_button: false,
            checkmark: false
        }
    );
    assert_eq!(stranger.names().len(), 1);
    // Sem dono no tile, `updateButtons` não toca na lista nem no botão de tirar, que fica como nasceu (ligado).
    let ownerless = DeskScreen::new(&DeskView {
        owner: None,
        frequency: None,
        viewer: Some(OWNER),
    });
    assert_eq!(
        ownerless.active(),
        DeskActive {
            remove: true,
            modes: [false; 3],
            override_button: false,
            checkmark: false
        }
    );
}

#[test]
fn each_button_sends_its_packet_with_the_click_sound() {
    let frequency = frequency(SecurityMode::Public, &["a", "b"]);
    let view = view(&frequency, OWNER);
    let mut desk = DeskScreen::new(&view);
    for (mode, button) in [(SecurityMode::Private, 0), (SecurityMode::Trusted, 1)] {
        let (taken, out) = desk.click(
            &view,
            middle(screen::MODE_BUTTONS[mode.ordinal()]),
            button,
            "",
        );
        assert!(taken);
        assert_eq!(
            out,
            [
                DeskOutput::ClickSound,
                DeskOutput::Menu(BUTTON_MODE + mode.ordinal() as i32)
            ],
            "{mode:?}"
        );
        assert_eq!(desk.focus(), Some(DeskWidget::Mode(mode)));
    }
    // O botão do modo em que a frequência está fica desligado: o clique não é dele, e o foco some.
    let (taken, out) = desk.click(&view, middle(screen::MODE_BUTTONS[0]), 0, "");
    assert!(!taken);
    assert_eq!(out, []);
    assert_eq!(desk.focus(), None);
    let (taken, out) = desk.click(&view, middle(screen::OVERRIDE), 0, "");
    assert!(taken);
    assert_eq!(
        out,
        [DeskOutput::ClickSound, DeskOutput::Menu(BUTTON_OVERRIDE)]
    );
    // A tela não muda com o clique: o menu só traz a frequência nova ao reabrir, e o botão de `PRIVATE` segue ligado.
    assert_eq!(desk.active().modes, [false, true, true]);
    // O botão do meio do mouse não é de ninguém.
    assert_eq!(
        desk.click(&view, middle(screen::OVERRIDE), 2, ""),
        (false, vec![])
    );
}

#[test]
fn the_list_selects_a_row_and_the_remove_button_sends_its_index() {
    let frequency = frequency(SecurityMode::Public, &["a", "b", "c"]);
    let view = view(&frequency, OWNER);
    let mut desk = DeskScreen::new(&view);
    // Sem seleção o botão de tirar está desligado.
    assert_eq!(
        desk.click(&view, middle(screen::REMOVE), 0, ""),
        (false, vec![])
    );
    let (taken, out) = desk.click(&view, row_point(1), 0, "");
    assert!(taken);
    assert_eq!(out, []);
    assert_eq!(desk.selected(), Some(1));
    assert_eq!(desk.selected_row(), Some(1));
    assert_eq!(desk.focus(), Some(DeskWidget::List));
    // O botão só liga no `updateButtons` seguinte (o do tick, ou o do começo do próximo clique).
    assert!(!desk.active().remove);
    let (taken, out) = desk.click(&view, middle(screen::REMOVE), 1, "");
    assert!(taken);
    assert_eq!(
        out,
        [DeskOutput::ClickSound, DeskOutput::Menu(BUTTON_REMOVE + 1)]
    );
    assert_eq!(desk.selected(), None);
    assert!(!desk.active().remove);
    // A linha vazia dentro da área limpa a seleção; o clique direito na lista não é dela.
    desk.click(&view, row_point(2), 0, "");
    assert_eq!(desk.selected(), Some(2));
    assert_eq!(desk.click(&view, row_point(0), 1, ""), (false, vec![]));
    assert_eq!(desk.selected(), Some(2));
    desk.click(&view, row_point(3), 0, "");
    assert_eq!(desk.selected(), None);
    // O limite de baixo da linha é inclusivo (`<= shiftedY + elementHeight`): o primeiro pixel da linha 1 ainda é a linha 0.
    desk.click(&view, [40.0, f64::from(screen::LIST[1] + 1 + 10)], 0, "");
    assert_eq!(desk.selected(), Some(0));
}

#[test]
fn a_selection_past_the_new_list_is_dropped() {
    let three = frequency(SecurityMode::Public, &["a", "b", "c"]);
    let mut desk = DeskScreen::new(&view(&three, OWNER));
    desk.click(&view(&three, OWNER), row_point(2), 0, "");
    desk.update_buttons(&view(&three, OWNER));
    assert!(desk.active().remove);
    let two = frequency(SecurityMode::Public, &["a", "b"]);
    desk.update_buttons(&view(&two, OWNER));
    assert_eq!(desk.selected(), None);
    assert!(!desk.active().remove);
}

#[test]
fn the_list_scrolls_with_the_wheel_and_the_thumb() {
    let names: Vec<String> = (0..9).map(|n| format!("nome{n}")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let frequency = frequency(SecurityMode::Public, &refs);
    let view = view(&frequency, OWNER);
    let mut desk = DeskScreen::new(&view);
    let rows = |desk: &DeskScreen| {
        desk.visible_rows()
            .map(|(_, index, _)| index)
            .collect::<Vec<_>>()
    };
    assert_eq!(rows(&desk), [0, 1, 2, 3]);
    // Fora da lista a roda não é dela.
    assert!(!desk.scroll(middle(screen::REMOVE), -1.0));
    // 9 nomes, 4 visíveis: cada passo da roda anda 1/5, e a primeira linha é `(int) (5,5 * scroll)`.
    assert!(desk.scroll(middle(screen::LIST), -1.0));
    assert_eq!(rows(&desk), [1, 2, 3, 4]);
    for _ in 0..3 {
        desk.scroll(middle(screen::LIST), -1.0);
    }
    assert_eq!(rows(&desk), [4, 5, 6, 7]);
    desk.scroll(middle(screen::LIST), -1.0);
    assert_eq!(rows(&desk), [5, 6, 7, 8]);
    assert_eq!(desk.list.thumb_offset(), 42 - 4 - 4);
    desk.scroll(middle(screen::LIST), 1.0);
    assert_eq!(rows(&desk), [4, 5, 6, 7]);
    // A seleção fora das linhas visíveis não tem sprite.
    desk.click(&view, row_point(0), 0, "");
    assert_eq!(desk.selected(), Some(4));
    desk.scroll(middle(screen::LIST), -1.0);
    assert_eq!(desk.selected_row(), None);
    // O polegar: pega-se nele e arrasta-se até o topo.
    let [x, y, _, _] = desk.list.thumb_rect();
    desk.click(&view, [f64::from(x) + 1.0, f64::from(y) + 1.0], 0, "");
    assert!(desk.list.dragging());
    desk.drag([0.0, 0.0]);
    assert_eq!(rows(&desk), [0, 1, 2, 3]);
    desk.release();
    assert!(!desk.list.dragging());
    desk.drag([0.0, 500.0]);
    assert_eq!(rows(&desk), [0, 1, 2, 3]);
    // A lista que deixa de precisar da barra volta ao começo.
    desk.scroll(middle(screen::LIST), -1.0);
    let short = self::frequency(SecurityMode::Public, &["a"]);
    desk.update_buttons(&self::view(&short, OWNER));
    assert_eq!(desk.list.thumb_offset(), 0);
}

#[test]
fn the_field_takes_the_focus_the_cursor_and_the_right_click() {
    let frequency = frequency(SecurityMode::Public, &[]);
    let view = view(&frequency, OWNER);
    let mut desk = DeskScreen::new(&view);
    assert!(!desk.field_focused());
    assert!(!desk.char_typed('a'));
    let at = middle(screen::EDIT_BOX);
    assert_eq!(
        desk.click(&view, at, 0, ""),
        (
            true,
            vec![DeskOutput::FieldCursor(at[0]), DeskOutput::FieldFocused]
        )
    );
    assert!(desk.field_focused());
    // Já focada: o cursor anda, o pisca não recomeça.
    assert_eq!(
        desk.click(&view, at, 0, "ab"),
        (true, vec![DeskOutput::FieldCursor(at[0])])
    );
    // O clique direito no `EditBox` limpa o texto.
    desk.text_changed("ab");
    assert!(desk.active().checkmark);
    assert_eq!(
        desk.click(&view, at, 1, "ab"),
        (true, vec![DeskOutput::ClearField])
    );
    assert!(!desk.active().checkmark);
    // As duas linhas de cima da caixa não são do `EditBox` (ele começa em y + 2): focam sem mexer no cursor, e só com o esquerdo.
    desk.click(&view, middle(screen::LIGHT), 0, "");
    assert_eq!(desk.focus(), Some(DeskWidget::Light));
    assert_eq!(
        desk.click(&view, [50.0, 68.5], 0, ""),
        (true, vec![DeskOutput::FieldFocused])
    );
    assert_eq!(desk.click(&view, [50.0, 68.5], 1, ""), (false, vec![]));
    assert!(!desk.field_focused());
    // As duas linhas abaixo da caixa (79 e 80) ainda são do `EditBox`, que tem 11 de altura a partir de 70.
    assert_eq!(
        desk.click(&view, [50.0, 80.5], 0, ""),
        (
            true,
            vec![DeskOutput::FieldCursor(50.0), DeskOutput::FieldFocused]
        )
    );
    // Só os caracteres de `USERNAME` chegam à caixa.
    assert!(
        desk.char_typed('a')
            && desk.char_typed('Z')
            && desk.char_typed('7')
            && desk.char_typed('_')
    );
    assert!(!desk.char_typed(' ') && !desk.char_typed('-') && !desk.char_typed('é'));
    assert!(DeskScreen::paste_accepted("Abc_1") && !DeskScreen::paste_accepted("a b"));
    // Um clique fora de tudo tira o foco e segue para os slots.
    assert_eq!(desk.click(&view, [5.0, 5.0], 0, ""), (false, vec![]));
    assert!(!desk.field_focused());
}

#[test]
fn enter_and_the_checkmark_send_the_name_and_clear_the_field() {
    let frequency = frequency(SecurityMode::Public, &[]);
    let owner = view(&frequency, OWNER);
    let mut desk = DeskScreen::new(&owner);
    // Sem texto o botão de confirmar está desligado: o clique nele é da caixa.
    assert_eq!(
        desk.click(&owner, middle(screen::CHECKMARK), 0, ""),
        (true, vec![DeskOutput::FieldFocused])
    );
    desk.text_changed(" Steve");
    assert!(desk.checkmark_hovered(middle(screen::CHECKMARK)));
    // O nome com espaço não é mandado (`isValidPlayerName` depois do `trim`, que o tira: " Steve" vira "Steve").
    let (taken, out) = desk.click(&owner, middle(screen::CHECKMARK), 0, " Steve");
    assert!(taken);
    assert_eq!(
        out,
        [
            DeskOutput::ClickSound,
            DeskOutput::AddTrusted("Steve".to_owned()),
            DeskOutput::ClearField
        ]
    );
    assert!(desk.field_focused());
    assert!(!desk.active().checkmark);
    // O Enter na caixa faz o mesmo, sem som; o nome longo demais limpa a caixa e não manda nada.
    assert_eq!(
        desk.key(&owner, DeskKey::Enter, "Alex"),
        (
            KeyResult::Consumed,
            vec![
                DeskOutput::AddTrusted("Alex".to_owned()),
                DeskOutput::ClearField
            ]
        )
    );
    assert_eq!(
        desk.key(&owner, DeskKey::Enter, "abcdefghijklmnopq"),
        (KeyResult::Consumed, vec![DeskOutput::ClearField])
    );
    // Quem não é o dono: o Enter e o botão não fazem nada, e a caixa fica com o texto.
    let stranger = view(&frequency, OTHER);
    assert_eq!(
        desk.key(&stranger, DeskKey::Enter, "Alex"),
        (KeyResult::Consumed, vec![])
    );
    desk.text_changed("Alex");
    assert_eq!(
        desk.click(&stranger, middle(screen::CHECKMARK), 0, "Alex"),
        (true, vec![DeskOutput::ClickSound])
    );
    assert!(desk.active().checkmark);
}

#[test]
fn the_keyboard_goes_to_the_focused_element() {
    let frequency = frequency(SecurityMode::Public, &["a"]);
    let view = view(&frequency, OWNER);
    let mut desk = DeskScreen::new(&view);
    // Sem foco as teclas são do jogo, e o Tab leva o foco à caixa.
    assert_eq!(
        desk.key(&view, DeskKey::Other, ""),
        (KeyResult::Pass, vec![])
    );
    assert_eq!(
        desk.key(&view, DeskKey::Enter, ""),
        (KeyResult::Pass, vec![])
    );
    assert_eq!(
        desk.key(&view, DeskKey::Tab, ""),
        (KeyResult::Consumed, vec![DeskOutput::FieldFocused])
    );
    // Com a caixa focada, toda tecla é dela (a do inventário não fecha a tela), menos o Escape.
    assert_eq!(
        desk.key(&view, DeskKey::Other, ""),
        (KeyResult::Field, vec![])
    );
    assert_eq!(
        desk.key(&view, DeskKey::Space, ""),
        (KeyResult::Field, vec![])
    );
    assert_eq!(
        desk.key(&view, DeskKey::Escape, ""),
        (KeyResult::Pass, vec![])
    );
    assert_eq!(
        desk.key(&view, DeskKey::Tab, ""),
        (KeyResult::Consumed, vec![])
    );
    // Um botão clicado fica focado: o Enter e o espaço o apertam de novo, com o som.
    desk.click(&view, middle(screen::MODE_BUTTONS[2]), 0, "");
    for key in [DeskKey::Enter, DeskKey::Space] {
        assert_eq!(
            desk.key(&view, key, ""),
            (
                KeyResult::Consumed,
                vec![DeskOutput::ClickSound, DeskOutput::Menu(BUTTON_MODE + 2)]
            )
        );
    }
    assert_eq!(
        desk.key(&view, DeskKey::Other, ""),
        (KeyResult::Pass, vec![])
    );
    // O botão de tirar focado e já desligado (a seleção saiu com o clique) não é apertado.
    desk.click(&view, row_point(0), 0, "");
    desk.click(&view, middle(screen::REMOVE), 0, "");
    assert_eq!(desk.focus(), Some(DeskWidget::Remove));
    assert_eq!(
        desk.key(&view, DeskKey::Enter, ""),
        (KeyResult::Pass, vec![])
    );
}

#[test]
fn the_hover_only_sees_active_elements() {
    let frequency = frequency(SecurityMode::Private, &[]);
    let view = view(&frequency, OWNER);
    let desk = DeskScreen::new(&view);
    assert_eq!(
        desk.hovered(middle(screen::MODE_BUTTONS[0])),
        Some(DeskWidget::Mode(SecurityMode::Public))
    );
    // O botão desligado não tem foco nem tooltip.
    assert_eq!(desk.hovered(middle(screen::MODE_BUTTONS[1])), None);
    assert_eq!(desk.hovered(middle(screen::REMOVE)), None);
    assert_eq!(
        desk.hovered(middle(screen::OVERRIDE)),
        Some(DeskWidget::Override)
    );
    assert_eq!(desk.hovered(middle(screen::LIST)), Some(DeskWidget::List));
    assert_eq!(desk.hovered(middle(screen::FIELD)), Some(DeskWidget::Field));
    assert_eq!(desk.hovered(middle(screen::LIGHT)), Some(DeskWidget::Light));
    assert_eq!(desk.hovered([5.0, 5.0]), None);
    // Os limites: o primeiro pixel é do botão, o da direita e o de baixo não.
    let [x, y, w, h] = screen::OVERRIDE.map(f64::from);
    assert_eq!(desk.hovered([x, y]), Some(DeskWidget::Override));
    assert_eq!(desk.hovered([x + w, y]), None);
    assert_eq!(desk.hovered([x, y + h]), None);
    assert!(!desk.checkmark_hovered(middle(screen::CHECKMARK)));
}
