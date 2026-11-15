//! `GuiRedstoneControlTab` (`GuiInsetElement`): a aba de controle de redstone, na lateral direita. As regras puras: onde fica, o sprite e o texto de cada modo, o clique dos
//! dois botões do mouse, o modo que o servidor escolhe e o estado de aviso (o desenho do suporte com o padrão de aviso). O desenho em si é do glue.

use crate::side_tab::{InsetTab, XP_TAB_HEIGHT};
use crate::tile::{RedstoneControl, RedstoneSync};

/// `GuiRedstoneControlTab`: `super(DISABLED, gui, tile, gui.getImageWidth(), 137, 26, 18, false)`; o botão de dentro fica em (179, 141).
pub const TAB_Y: i32 = 137;
/// `SpecialColors.TAB_REDSTONE_CONTROL`: a cor do suporte.
pub const TAB_TINT: u32 = 0xFF_C9_07_1F;

/// A aba, na lateral direita de uma tela de `image_width` de largura.
pub fn tab(image_width: i32) -> InsetTab {
    InsetTab::right(image_width, TAB_Y, XP_TAB_HEIGHT)
}

/// `getOverlay`: o sprite de cada modo (`DISABLED` é o `overlay` do construtor, `redstone_control/disabled`).
pub fn overlay(control: RedstoneControl) -> &'static str {
    match control {
        RedstoneControl::Disabled => "mekanism:redstone_control/disabled",
        RedstoneControl::High => "mekanism:redstone_control/high",
        RedstoneControl::Low => "mekanism:redstone_control/low",
        RedstoneControl::Pulse => "mekanism:redstone_control/pulse",
    }
}

/// `MekanismLang.REDSTONE_CONTROL_*`: a chave do texto de cada modo, que o `updateTooltip` mostra (`TooltipUtils.create(type.getTextComponent())`).
pub fn tooltip_key(control: RedstoneControl) -> &'static str {
    match control {
        RedstoneControl::Disabled => "redstone_control.mekanism.disabled",
        RedstoneControl::High => "redstone_control.mekanism.high",
        RedstoneControl::Low => "redstone_control.mekanism.low",
        RedstoneControl::Pulse => "redstone_control.mekanism.pulse",
    }
}

/// `GuiRedstoneControlTab.onClick`: o botão esquerdo manda `NEXT_REDSTONE_CONTROL` e o direito `PREVIOUS_REDSTONE_CONTROL`; só esses dois valem
/// (`isValidClickButton`), e não há conferência de dono no cliente (o servidor confere o acesso). `Some(avançar)` se o clique vira um pacote.
pub fn click(button: u8) -> Option<bool> {
    const LEFT: u8 = 0;
    const RIGHT: u8 = 1;
    (button == LEFT || button == RIGHT).then_some(button == LEFT)
}

/// `ITileRedstone.supportsMode`: toda máquina, o gerador e o cubo aceitam tudo menos o `PULSE` (só o Formulaic Assemblicator, o Laser Amplifier e o
/// Logistical Sorter o mudam, e nenhum deles está no port).
pub fn supports_mode(mode: RedstoneControl) -> bool {
    mode != RedstoneControl::Pulse
}

/// `PacketGuiInteract.NEXT_REDSTONE_CONTROL` e `PREVIOUS_REDSTONE_CONTROL`: `getNext` e `getPrevious` do modo de agora, só entre os que a máquina aceita.
pub fn cycle(current: RedstoneControl, forward: bool) -> RedstoneControl {
    current.cycle(forward, supports_mode)
}

/// O que o cliente tem do controle de redstone de um tile: os três dados do menu (`controlType`, `redstone` e `redstoneLastTick`), nas posições `control`, `powered` e
/// `was_powered` do espelho (antes do primeiro pacote: `DISABLED` e sem sinal).
pub fn read(
    mirror: &crate::container_sync::Mirror,
    control: usize,
    powered: usize,
    was_powered: usize,
) -> RedstoneSync {
    RedstoneSync {
        control_type: RedstoneControl::from_ordinal(mirror.enum_ordinal(control)),
        powered: mirror.boolean(powered),
        was_powered: mirror.boolean(was_powered),
    }
}

/// `PacketUtils.blockEntity` no servidor: o tile só é entregue a quem passa em `IBlockSecurityUtils.canAccess` (sem a Security Desk, a lista de confiança é vazia, e o operador não
/// passa por cima), e quem não passa é ignorado em silêncio. Não há conferência de dono: quem tem acesso a uma máquina pública troca o modo.
pub fn can_click(security: &crate::security::SecurityState, player: u128) -> bool {
    security.can_access_player(
        crate::security::SecurityConfig::default(),
        player,
        false,
        false,
        &|_, _| false,
    )
}

/// O modo que o servidor deixa depois do clique de `player` (`NEXT_REDSTONE_CONTROL` ou `PREVIOUS_REDSTONE_CONTROL`): o vizinho aceito, ou o mesmo se o jogador não tem acesso.
pub fn server_click(
    security: &crate::security::SecurityState,
    player: u128,
    current: RedstoneControl,
    forward: bool,
) -> RedstoneControl {
    if can_click(security, player) {
        cycle(current, forward)
    } else {
        current
    }
}

/// O aviso da aba (`GuiMekanismTile.addGenericTabs`): `REDSTONE_SIGNAL_ABSENT` (`HIGH` sem sinal), `REDSTONE_SIGNAL_PRESENT` (`LOW` com sinal) e
/// `REDSTONE_PULSE_REQUIRED` (`PULSE` sem pulso) viram um único `tile.getControlType() == X && !tile.isRedstoneActivated()`. Com um aviso, a aba
/// se desenha como `GuiInsetElement.draw` manda.
pub fn warning(sync: RedstoneSync) -> bool {
    sync.control_type != RedstoneControl::Disabled && !sync.is_activated()
}

/// `GuiWarningTab`: o suporte de 26 por 26 em y = 109, à esquerda (`-26`) ou, no Heat Generator (`GuiHeatGenerator.addWarningTab`), à direita (`imageWidth`), que aparece
/// quando o tile tem um aviso de redstone e as telas sem outros avisos (o gerador e o cubo) não têm mais nada a mostrar nele.
pub fn warning_tab_rect(image_width: i32, left: bool) -> crate::gui::Rect {
    [
        if left { -26.0 } else { image_width as f32 },
        109.0,
        26.0,
        26.0,
    ]
}

/// `MekanismLang.ISSUE_*` do aviso de redstone (`WarningType.REDSTONE_SIGNAL_ABSENT`, `REDSTONE_SIGNAL_PRESENT`, `REDSTONE_PULSE_REQUIRED`) que `getWarnings` traduz no tooltip
/// da aba de avisos, ou `None` sem aviso.
pub fn issue_lang_key(sync: RedstoneSync) -> Option<&'static str> {
    warning(sync).then_some(match sync.control_type {
        RedstoneControl::High => "gui.mekanism.issues.redstone_signal_absent",
        RedstoneControl::Low => "gui.mekanism.issues.redstone_signal_present",
        _ => "gui.mekanism.issues.redstone_pulse_required",
    })
}

/// `MekanismLang.ISSUES`, a primeira linha do tooltip da aba de avisos, em `EnumColor.YELLOW`.
pub const ISSUES_LANG: &str = "gui.mekanism.issues";
pub const ISSUES_COLOR: u32 = 0xFF_DD_4F;

/// O sprite do padrão de aviso (`GuiElement.WARNING_TEXTURE`, `warning.png`, 16 por 16, `gui.scaling` do tipo `tile`): o padrão ladrilha a partir do canto de cima e da
/// esquerda do retângulo em que é desenhado.
pub const WARNING_PATTERN_TILE: [f32; 2] = [16.0, 16.0];

/// Uma camada do suporte de uma aba inset com aviso (`GuiInsetElement.draw`), na ordem em que o original as desenha.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WarningLayer {
    /// `blitSprite(GUI_TEXTURED, getResource(), ...)`: o suporte sem cor, com o blend de sempre.
    HolderPlain,
    /// `blitSprite(GUI_TEXTURED_DST_COLOR, WARNING_TEXTURE, ...)`: o padrão sobre o retângulo todo, multiplicado pelo que já está no quadro (`DST_COLOR`, `ZERO`).
    PatternMultiplied,
}

/// As camadas do suporte com aviso, em ordem.
pub const WARNING_LAYERS: [WarningLayer; 2] = [WarningLayer::HolderPlain, WarningLayer::PatternMultiplied];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tab_sits_at_137_with_the_button_at_141() {
        assert_eq!(tab(176).holder(), [176.0, 137.0, 26.0, 26.0]);
        assert_eq!(tab(176).button(), [179.0, 141.0, 18.0, 18.0]);
    }

    #[test]
    fn both_mouse_buttons_click_and_the_others_do_not() {
        assert_eq!(click(0), Some(true));
        assert_eq!(click(1), Some(false));
        assert_eq!(click(2), None);
    }

    #[test]
    fn the_cycle_skips_the_pulse_that_the_machines_do_not_support() {
        use RedstoneControl::*;
        let forward: Vec<_> = [Disabled, High, Low, Pulse]
            .map(|m| cycle(m, true))
            .to_vec();
        assert_eq!(forward, [High, Low, Disabled, Disabled]);
        let backward: Vec<_> = [Disabled, High, Low, Pulse]
            .map(|m| cycle(m, false))
            .to_vec();
        assert_eq!(backward, [Low, Disabled, High, Low]);
    }

    #[test]
    fn only_the_modes_waiting_for_a_signal_warn() {
        let sync = |control_type, powered| RedstoneSync {
            control_type,
            powered,
            was_powered: false,
        };
        assert!(!warning(sync(RedstoneControl::Disabled, false)));
        assert!(!warning(sync(RedstoneControl::Disabled, true)));
        assert!(warning(sync(RedstoneControl::High, false)));
        assert!(!warning(sync(RedstoneControl::High, true)));
        assert!(!warning(sync(RedstoneControl::Low, false)));
        assert!(warning(sync(RedstoneControl::Low, true)));
        assert!(warning(sync(RedstoneControl::Pulse, false)));
    }

    #[test]
    fn the_warning_tab_and_its_issue_follow_the_redstone_warning() {
        assert_eq!(warning_tab_rect(176, true), [-26.0, 109.0, 26.0, 26.0]);
        assert_eq!(warning_tab_rect(176, false), [176.0, 109.0, 26.0, 26.0]);
        let sync = |control_type, powered| RedstoneSync {
            control_type,
            powered,
            was_powered: false,
        };
        assert_eq!(issue_lang_key(sync(RedstoneControl::Disabled, false)), None);
        assert_eq!(
            issue_lang_key(sync(RedstoneControl::High, false)),
            Some("gui.mekanism.issues.redstone_signal_absent")
        );
        assert_eq!(
            issue_lang_key(sync(RedstoneControl::Low, true)),
            Some("gui.mekanism.issues.redstone_signal_present")
        );
        assert_eq!(issue_lang_key(sync(RedstoneControl::Low, false)), None);
    }

    #[test]
    fn a_warning_holder_is_plain_and_then_multiplied_by_the_pattern() {
        assert_eq!(WARNING_LAYERS, [WarningLayer::HolderPlain, WarningLayer::PatternMultiplied]);
        assert_eq!(WARNING_PATTERN_TILE, [16.0, 16.0]);
    }
}
