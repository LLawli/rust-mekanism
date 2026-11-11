//! As janelas de configuração de lados e de transporte (`GuiSideConfiguration`, `GuiTransporterConfig`) sem o desenho: os pacotes
//! que elas mandam ao servidor (`PacketSideData`, `PacketEjectConfiguration`, `PacketBatchConfiguration`, `PacketEjectColor`,
//! `PacketInputColor` e o `GuiInteraction.STRICT_INPUT`) com o que cada `handle` faz, a conta do botão de limpar
//! (`getTargetType`), as cores de `EnumColor` e `TransporterUtils.increment` e `decrement`, e a geometria dos filhos das duas janelas.
//!
//! A lib não conhece máquina nenhuma: as contas são sobre [`TileComponentConfig`] e [`TileComponentEjector`], e quem usa entrega os dois.
use crate::ejector::TileComponentEjector;
use crate::side_config::{
    ConfigInfo, DataType, ENUM_COLOR_NAMES, RelativeSide, TileComponentConfig, TransmissionType,
};
use crate::sides::Face;

/// O que o cliente vê da configuração de lados e do ejetor de um bloco: o que `tile.getConfig()` e `tile.getEjector()` dão à tela.
pub struct ConfigView {
    pub config: TileComponentConfig,
    pub ejector: TileComponentEjector,
    pub facing: Face,
}

/// `MekClickType`: o clique esquerdo, o direito e o esquerdo com shift (`MekClickType.left(holdingShift)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickType {
    Left,
    Right,
    ShiftLeft,
}

impl ClickType {
    pub const VALUES: [ClickType; 3] = [ClickType::Left, ClickType::Right, ClickType::ShiftLeft];

    /// `MekClickType.left(holdingShift)`.
    pub fn left(holding_shift: bool) -> ClickType {
        if holding_shift {
            ClickType::ShiftLeft
        } else {
            ClickType::Left
        }
    }

    pub fn ordinal(self) -> u8 {
        self as u8
    }

    /// `MekClickType.BY_ID`: o índice dá a volta (`OutOfBoundsStrategy.WRAP`).
    pub fn by_id(id: i64) -> ClickType {
        Self::VALUES[id.rem_euclid(3) as usize]
    }
}

/// `TransmissionType.BY_ID`: o índice dá a volta.
fn transmission_by_id(id: i64) -> TransmissionType {
    TransmissionType::VALUES[id.rem_euclid(5) as usize]
}

/// `EnumColor.getRgbCode`, na ordem do `ordinal` (a mesma de [`ENUM_COLOR_NAMES`]).
pub const ENUM_COLOR_RGB: [u32; 18] = [
    0x40_40_40, 0x36_6B_D0, 0x59_C1_5F, 0x00_F3_D0, 0xC9_07_1F, 0xA4_60_D9, 0xFF_A1_60, 0xCF_CF_CF,
    0x7A_7A_7A, 0x55_9E_FF, 0x75_FF_89, 0x30_FF_F9, 0xFF_38_3C, 0xD5_5E_CB, 0xFF_DD_4F, 0xFF_FF_FF,
    0xA1_76_49, 0xFF_BC_C4,
];

/// `EnumColor.getPackedColor`: o ARGB opaco da cor.
pub fn color_argb(color: u8) -> u32 {
    0xFF00_0000 | ENUM_COLOR_RGB[usize::from(color).min(ENUM_COLOR_RGB.len() - 1)]
}

/// A chave de tradução do nome de uma `EnumColor` (`color.mekanism.<prefixo>`).
pub fn color_key(color: u8) -> String {
    format!(
        "color.mekanism.{}",
        ENUM_COLOR_NAMES[usize::from(color).min(ENUM_COLOR_NAMES.len() - 1)]
    )
}

/// `TransporterUtils.increment`: sem cor vai para a primeira, a última volta a sem cor.
pub fn increment_color(color: Option<u8>) -> Option<u8> {
    match color {
        None => Some(0),
        Some(c) if usize::from(c) == ENUM_COLOR_NAMES.len() - 1 => None,
        Some(c) => Some(c + 1),
    }
}

/// `TransporterUtils.decrement`: sem cor vai para a última, a primeira volta a sem cor.
pub fn decrement_color(color: Option<u8>) -> Option<u8> {
    match color {
        None => Some((ENUM_COLOR_NAMES.len() - 1) as u8),
        Some(0) => None,
        Some(c) => Some(c - 1),
    }
}

/// `BasicColorButton.getButtonBlitColor`: a cor do botão, com a saturação 0,1 menor e o valor 0,1 maior (`Color.hsv`, que
/// arredonda os canais). Devolve o ARGB.
pub fn button_tint(rgb: u32) -> u32 {
    let (r, g, b) = (
        f64::from((rgb >> 16) & 0xFF) / 255.0,
        f64::from((rgb >> 8) & 0xFF) / 255.0,
        f64::from(rgb & 0xFF) / 255.0,
    );
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let mut hue = if delta == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / delta) % 6.0
    } else if max == g {
        ((b - r) / delta) + 2.0
    } else {
        ((r - g) / delta) + 4.0
    };
    hue *= 60.0;
    if hue < 0.0 {
        hue += 360.0;
    }
    let mut saturation = if max == 0.0 { 0.0 } else { delta / max };
    let mut value = max;
    saturation = (saturation - 0.1).max(0.0);
    value = (value + 0.1).min(1.0);
    let hue_index = (hue % 360.0) / 60.0;
    let i = hue_index as i32;
    let diff = hue_index - f64::from(i);
    let p = value * (1.0 - saturation);
    let q = value * (1.0 - (saturation * diff));
    let t = value * (1.0 - (saturation * (1.0 - diff)));
    let (r, g, b) = match i {
        0 => (value, t, p),
        1 => (q, value, p),
        2 => (p, value, t),
        3 => (p, q, value),
        4 => (t, p, value),
        _ => (value, p, q),
    };
    let channel = |c: f64| (c * 255.0).round() as u32;
    0xFF00_0000 | (channel(r) << 16) | (channel(g) << 8) | channel(b)
}

/// Os pacotes de configuração que as janelas mandam, um por `handle` do Java.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigPacket {
    /// `PacketSideData`.
    SideData {
        click: ClickType,
        side: RelativeSide,
        transmission: TransmissionType,
    },
    /// `PacketEjectConfiguration`.
    Eject { transmission: TransmissionType },
    /// `PacketBatchConfiguration`: sem tipo, todos os tipos da máquina.
    Batch {
        transmission: Option<TransmissionType>,
        target: DataType,
    },
    /// `PacketEjectColor`.
    EjectColor { click: ClickType },
    /// `PacketInputColor`.
    InputColor { click: ClickType, side: RelativeSide },
    /// `GuiInteraction.STRICT_INPUT`.
    StrictInput,
}

impl ConfigPacket {
    const SIDE_DATA: u8 = 0;
    const EJECT: u8 = 1;
    const BATCH: u8 = 2;
    const EJECT_COLOR: u8 = 3;
    const INPUT_COLOR: u8 = 4;
    const STRICT_INPUT: u8 = 5;

    /// O corpo da mensagem: o tipo do pacote e os campos, um byte cada (o `ordinal`, que o `idMapper` do Java manda como varint).
    pub fn encode(&self) -> Vec<u8> {
        match *self {
            ConfigPacket::SideData {
                click,
                side,
                transmission,
            } => vec![
                Self::SIDE_DATA,
                click.ordinal(),
                side.ordinal() as u8,
                transmission.ordinal() as u8,
            ],
            ConfigPacket::Eject { transmission } => vec![Self::EJECT, transmission.ordinal() as u8],
            ConfigPacket::Batch {
                transmission,
                target,
            } => vec![
                Self::BATCH,
                u8::from(transmission.is_some()),
                transmission.map_or(0, |kind| kind.ordinal() as u8),
                target.ordinal() as u8,
            ],
            ConfigPacket::EjectColor { click } => vec![Self::EJECT_COLOR, click.ordinal()],
            ConfigPacket::InputColor { click, side } => {
                vec![Self::INPUT_COLOR, click.ordinal(), side.ordinal() as u8]
            }
            ConfigPacket::StrictInput => vec![Self::STRICT_INPUT],
        }
    }

    /// O inverso de [`ConfigPacket::encode`], com a volta dos `BY_ID` do Java (`WRAP`, e `ZERO` no `DataType`). `None` é um corpo
    /// malformado, que o servidor ignora.
    pub fn decode(bytes: &[u8]) -> Option<ConfigPacket> {
        let (&kind, rest) = bytes.split_first()?;
        let at = |index: usize| rest.get(index).map(|&byte| i64::from(byte));
        Some(match kind {
            Self::SIDE_DATA => ConfigPacket::SideData {
                click: ClickType::by_id(at(0)?),
                side: RelativeSide::by_id(at(1)?),
                transmission: transmission_by_id(at(2)?),
            },
            Self::EJECT => ConfigPacket::Eject {
                transmission: transmission_by_id(at(0)?),
            },
            Self::BATCH => ConfigPacket::Batch {
                transmission: (at(0)? != 0).then(|| transmission_by_id(at(1).unwrap_or(0))),
                target: DataType::by_id(at(2)?),
            },
            Self::EJECT_COLOR => ConfigPacket::EjectColor {
                click: ClickType::by_id(at(0)?),
            },
            Self::INPUT_COLOR => ConfigPacket::InputColor {
                click: ClickType::by_id(at(0)?),
                side: RelativeSide::by_id(at(1)?),
            },
            Self::STRICT_INPUT => ConfigPacket::StrictInput,
            _ => return None,
        })
    }

    /// O `handle` do pacote sobre a configuração e o ejetor do tile virado para `facing`. Devolve se algo mudou. As mudanças de lado
    /// ficam em [`TileComponentConfig::take_changes`] (`sideChanged`), para quem chama invalidar as capabilities e salvar.
    pub fn apply(
        &self,
        config: &mut TileComponentConfig,
        ejector: &mut TileComponentEjector,
        facing: Face,
    ) -> bool {
        match *self {
            ConfigPacket::SideData {
                click,
                side,
                transmission,
            } => {
                let Some(info) = config.config_mut(transmission) else {
                    return false;
                };
                let before = info.data_type(side);
                let after = match click {
                    ClickType::Left => info.increment_data_type(side),
                    ClickType::Right => info.decrement_data_type(side),
                    ClickType::ShiftLeft => {
                        // "We only need to update it if we are changing it to none".
                        if before != DataType::None {
                            info.set_data_type(DataType::None, side);
                        }
                        DataType::None
                    }
                };
                if before != after {
                    config.side_changed(transmission, side, facing);
                    return true;
                }
                false
            }
            ConfigPacket::Eject { transmission } => {
                let Some(info) = config.config_mut(transmission) else {
                    return false;
                };
                info.set_ejecting(!info.is_ejecting());
                true
            }
            ConfigPacket::Batch {
                transmission,
                target,
            } => {
                let mut changed = false;
                let kinds: Vec<TransmissionType> = match transmission {
                    Some(kind) => vec![kind],
                    None => config.transmissions().to_vec(),
                };
                for kind in kinds {
                    changed |= update_all_sides(config, kind, target, facing);
                }
                changed
            }
            ConfigPacket::EjectColor { click } => {
                let color = ejector.output_color();
                ejector.set_output_color(match click {
                    ClickType::Left => increment_color(color),
                    ClickType::Right => decrement_color(color),
                    ClickType::ShiftLeft => None,
                });
                true
            }
            ConfigPacket::InputColor { click, side } => {
                let color = ejector.input_color(side);
                ejector.set_input_color(
                    config,
                    side,
                    match click {
                        ClickType::Left => increment_color(color),
                        ClickType::Right => decrement_color(color),
                        ClickType::ShiftLeft => None,
                    },
                );
                true
            }
            ConfigPacket::StrictInput => {
                ejector.set_strict_input(!ejector.has_strict_input());
                true
            }
        }
    }
}

/// O que o servidor manda ao cliente que tem uma janela de configuração aberta: por tipo de transmissão (na ordem de `getTransmissions()`) o
/// `ejecting` e o `DataType` dos seis lados, e do ejetor o `strictInput`, a cor de saída e as seis cores de entrada (255 é nenhuma cor).
/// No original o `ejecting`, o `strictInput` e as cores vão pelos dados do menu (`getSpecificSyncableData`) e os `DataType` pelo pacote de
/// atualização do tile; aqui tudo vai junto, por um canal só.
pub fn encode_snapshot(config: &TileComponentConfig, ejector: &TileComponentEjector) -> Vec<u8> {
    let mut out = Vec::new();
    for &kind in config.transmissions() {
        let Some(info) = config.config(kind) else {
            continue;
        };
        out.push(u8::from(info.is_ejecting()));
        out.extend(RelativeSide::VALUES.iter().map(|&side| info.data_type(side).ordinal() as u8));
    }
    out.push(u8::from(ejector.has_strict_input()));
    out.push(ejector.output_color().unwrap_or(255));
    out.extend(RelativeSide::VALUES.iter().map(|&side| ejector.input_color(side).unwrap_or(255)));
    out
}

/// O inverso de [`encode_snapshot`], sobre a configuração e o ejetor que o cliente montou da máquina. Falso se o tamanho não bate.
pub fn apply_snapshot(bytes: &[u8], config: &mut TileComponentConfig, ejector: &mut TileComponentEjector) -> bool {
    let kinds = config.transmissions().to_vec();
    if bytes.len() != kinds.len() * 7 + 8 {
        return false;
    }
    let mut at = 0;
    for kind in kinds {
        let Some(info) = config.config_mut(kind) else {
            continue;
        };
        info.set_ejecting(bytes[at] != 0);
        for (offset, side) in RelativeSide::VALUES.into_iter().enumerate() {
            info.set_data_type(DataType::by_id(i64::from(bytes[at + 1 + offset])), side);
        }
        at += 7;
    }
    ejector.set_strict_input(bytes[at] != 0);
    ejector.set_output_color((bytes[at + 1] != 255).then_some(bytes[at + 1]));
    for (offset, side) in RelativeSide::VALUES.into_iter().enumerate() {
        let color = bytes[at + 2 + offset];
        ejector.set_input_color(config, side, (color != 255).then_some(color));
    }
    true
}

/// `PacketBatchConfiguration.updateAllSides`.
fn update_all_sides(
    config: &mut TileComponentConfig,
    kind: TransmissionType,
    target: DataType,
    facing: Face,
) -> bool {
    let Some(info) = config.config_mut(kind) else {
        return false;
    };
    if !info.supports(target) {
        return false;
    }
    let mut changed = Vec::new();
    for side in RelativeSide::VALUES {
        if info.set_data_type(target, side) {
            changed.push(side);
        }
    }
    let any = !changed.is_empty();
    for side in changed {
        config.side_changed(kind, side, facing);
    }
    any
}

/// `GuiSideConfiguration.getTargetType`: o `DataType` para que o botão de limpar leva todos os lados do tipo. Com shift, `NONE`.
/// Sem shift, vale o `DataType` que os lados ligados têm em comum, avançado (`next`) ou recuado (`previous`) entre os que o tipo
/// suporta; lados ligados diferentes, ou nenhum ligado, dão `NONE`.
pub fn target_type(info: Option<&ConfigInfo>, shift: bool, forward: bool) -> DataType {
    if shift {
        return DataType::None;
    }
    let Some(info) = info else {
        return DataType::None;
    };
    let mut common: Option<DataType> = None;
    for side in RelativeSide::VALUES {
        if info.is_side_enabled(side) {
            let current = info.data_type(side);
            match common {
                None => common = Some(current),
                Some(known) if known != current => return DataType::None,
                Some(_) => {}
            }
        }
    }
    match common {
        Some(kind) if forward => kind.next(|candidate| info.supports(candidate)),
        Some(kind) => kind.previous(|candidate| info.supports(candidate)),
        None => DataType::None,
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// Geometria dos filhos (relativa à janela).

/// `GuiSideConfiguration`: o tamanho da janela.
pub const SIDE_SIZE: [i32; 2] = [156, 135];
/// `GuiTransporterConfig`: o tamanho da janela.
pub const TRANSPORTER_SIZE: [i32; 2] = [156, 119];
/// Os botões de lado (`SideDataButton`, 22 por 22) da janela de lados, por `RelativeSide`.
pub fn side_button(side: RelativeSide) -> [i32; 2] {
    match side {
        RelativeSide::Bottom => [67, 92],
        RelativeSide::Top => [67, 46],
        RelativeSide::Front => [67, 69],
        RelativeSide::Back => [44, 92],
        RelativeSide::Left => [44, 69],
        RelativeSide::Right => [90, 69],
    }
}
/// Os botões de lado da janela de transporte.
pub fn transporter_button(side: RelativeSide) -> [i32; 2] {
    match side {
        RelativeSide::Bottom => [41, 64 + 16],
        RelativeSide::Top => [41, 34],
        RelativeSide::Front => [41, 57],
        RelativeSide::Back => [18, 64 + 16],
        RelativeSide::Left => [18, 57],
        RelativeSide::Right => [64, 57],
    }
}
/// O lado do `SideDataButton`.
pub const SIDE_BUTTON_SIZE: i32 = 22;
/// A tela interna do ejetor (`GuiInnerScreen` em 38, 25, 80 por 12) da janela de lados.
pub const SIDE_INNER_SCREEN: [i32; 4] = [38, 25, 80, 12];
/// A tela interna do `Strict Input` da janela de transporte.
pub const TRANSPORTER_INNER_SCREEN: [i32; 4] = [38, 15, 80, 12];
/// O botão de ejetar (`MekanismImageButton` de 14) e o de limpar os lados.
pub const EJECT_BUTTON: [i32; 4] = [136, 6, 14, 14];
pub const CLEAR_BUTTON: [i32; 4] = [136, 95, 14, 14];
/// O botão do `Strict Input` (`button/exclamation`).
pub const STRICT_BUTTON: [i32; 4] = [136, 6, 14, 14];
/// O `GuiSlot` normal da cor de saída e o `ColorButton` dentro dele.
pub const OUTPUT_SLOT: [i32; 4] = [111, 48, 18, 18];
pub const OUTPUT_COLOR_BUTTON: [i32; 4] = [112, 49, 16, 16];

/// `GuiSideConfiguration`: o suporte da aba do tipo `index` (`GuiConfigTypeTab`, 26 por 26): do lado esquerdo as quatro primeiras,
/// do direito as demais.
pub fn type_tab(index: usize, window_width: i32) -> [i32; 4] {
    let left = index < 4;
    [
        if left { -26 } else { window_width },
        2 + 28 * (index % 4) as i32,
        26,
        26,
    ]
}

/// O tamanho do botão de dentro de uma aba (`innerSize`), e do suporte (`GuiSideHolder`, que é quadrado).
pub const TAB_INNER: i32 = 18;
pub const TAB_SIZE: i32 = 26;

/// `GuiInsetElement` de uma aba: o botão de dentro (18 por 18) fica a `(26 - 18) / 2 = 4` pixels do canto do suporte, e `left`
/// desloca 1 pixel para dentro (`getButtonX`: `+ border + (left ? 1 : -1)`).
pub fn inset_button(holder: [i32; 2], left: bool) -> [i32; 4] {
    let border = (TAB_SIZE - TAB_INNER) / 2;
    [
        holder[0] + border + if left { 1 } else { -1 },
        holder[1] + border,
        TAB_INNER,
        TAB_INNER,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::side_config::TileComponentConfig;

    fn config() -> TileComponentConfig {
        let mut config = TileComponentConfig::new(&[TransmissionType::Item, TransmissionType::Energy]);
        config.setup_item_io_config(&[0], &[1], 2, false);
        config.setup_input_config(TransmissionType::Energy, 0);
        config
    }

    #[test]
    fn colors_cycle_through_none() {
        assert_eq!(increment_color(None), Some(0));
        assert_eq!(increment_color(Some(16)), Some(17));
        assert_eq!(increment_color(Some(17)), None);
        assert_eq!(decrement_color(None), Some(17));
        assert_eq!(decrement_color(Some(1)), Some(0));
        assert_eq!(decrement_color(Some(0)), None);
    }

    #[test]
    fn click_types_wrap_like_the_id_mapper() {
        assert_eq!(ClickType::by_id(0), ClickType::Left);
        assert_eq!(ClickType::by_id(2), ClickType::ShiftLeft);
        assert_eq!(ClickType::by_id(3), ClickType::Left);
        assert_eq!(ClickType::by_id(-1), ClickType::ShiftLeft);
    }

    #[test]
    fn packets_round_trip() {
        let packets = [
            ConfigPacket::SideData {
                click: ClickType::ShiftLeft,
                side: RelativeSide::Bottom,
                transmission: TransmissionType::Energy,
            },
            ConfigPacket::Eject {
                transmission: TransmissionType::Item,
            },
            ConfigPacket::Batch {
                transmission: None,
                target: DataType::Input,
            },
            ConfigPacket::Batch {
                transmission: Some(TransmissionType::Chemical),
                target: DataType::None,
            },
            ConfigPacket::EjectColor {
                click: ClickType::Right,
            },
            ConfigPacket::InputColor {
                click: ClickType::Left,
                side: RelativeSide::Back,
            },
            ConfigPacket::StrictInput,
        ];
        for packet in packets {
            assert_eq!(ConfigPacket::decode(&packet.encode()), Some(packet));
        }
        assert_eq!(ConfigPacket::decode(&[]), None);
        assert_eq!(ConfigPacket::decode(&[99]), None);
        assert_eq!(ConfigPacket::decode(&[ConfigPacket::SIDE_DATA, 0]), None);
    }

    #[test]
    fn side_data_clicks_follow_the_packet() {
        let mut config = config();
        let mut ejector = TileComponentEjector::new();
        let click = |click| ConfigPacket::SideData {
            click,
            side: RelativeSide::Front,
            transmission: TransmissionType::Item,
        };
        // Sem DataType o clique esquerdo vai ao primeiro suportado.
        assert!(click(ClickType::Left).apply(&mut config, &mut ejector, Face::North));
        let after_left = config.data_type(TransmissionType::Item, RelativeSide::Front);
        assert_ne!(after_left, Some(DataType::None));
        assert_eq!(config.take_changes().len(), 1);
        // O direito volta.
        assert!(click(ClickType::Right).apply(&mut config, &mut ejector, Face::North));
        assert_eq!(
            config.data_type(TransmissionType::Item, RelativeSide::Front),
            Some(DataType::None)
        );
        // Shift em um lado que já é NONE não muda nem avisa.
        assert!(!click(ClickType::ShiftLeft).apply(&mut config, &mut ejector, Face::North));
        config.take_changes();
        assert!(click(ClickType::Left).apply(&mut config, &mut ejector, Face::North));
        config.take_changes();
        assert!(click(ClickType::ShiftLeft).apply(&mut config, &mut ejector, Face::North));
        assert_eq!(
            config.data_type(TransmissionType::Item, RelativeSide::Front),
            Some(DataType::None)
        );
        assert_eq!(config.take_changes().len(), 1);
    }

    #[test]
    fn eject_toggles_and_batch_sets_every_side() {
        let mut config = config();
        let mut ejector = TileComponentEjector::new();
        let eject = ConfigPacket::Eject {
            transmission: TransmissionType::Item,
        };
        eject.apply(&mut config, &mut ejector, Face::North);
        assert!(config.config(TransmissionType::Item).unwrap().is_ejecting());
        eject.apply(&mut config, &mut ejector, Face::North);
        assert!(!config.config(TransmissionType::Item).unwrap().is_ejecting());

        let batch = ConfigPacket::Batch {
            transmission: Some(TransmissionType::Item),
            target: DataType::Input,
        };
        config.take_changes();
        assert!(batch.apply(&mut config, &mut ejector, Face::North));
        for side in RelativeSide::VALUES {
            assert_eq!(
                config.data_type(TransmissionType::Item, side),
                Some(DataType::Input)
            );
        }
        assert_eq!(config.take_changes().len(), 6);
        // Já está tudo igual: nada muda.
        assert!(!batch.apply(&mut config, &mut ejector, Face::North));
        // Um DataType que o tipo não suporta não faz nada.
        let unsupported = ConfigPacket::Batch {
            transmission: Some(TransmissionType::Energy),
            target: DataType::Output,
        };
        assert!(!unsupported.apply(&mut config, &mut ejector, Face::North));
    }

    #[test]
    fn target_type_needs_a_common_type() {
        let mut config = config();
        let info = config.config(TransmissionType::Item).unwrap();
        // Tudo em NONE: o próximo suportado.
        let next = target_type(Some(info), false, true);
        assert_ne!(next, DataType::None);
        assert_eq!(target_type(Some(info), true, true), DataType::None);
        assert_eq!(target_type(None, false, true), DataType::None);
        let info = config.config_mut(TransmissionType::Item).unwrap();
        info.set_data_type(DataType::Input, RelativeSide::Front);
        let info = config.config(TransmissionType::Item).unwrap();
        // Lados diferentes: NONE.
        assert_eq!(target_type(Some(info), false, true), DataType::None);
        assert_eq!(target_type(Some(info), false, false), DataType::None);
    }

    #[test]
    fn input_color_ignores_a_disabled_side() {
        let mut config = config();
        let mut ejector = TileComponentEjector::new();
        let packet = ConfigPacket::InputColor {
            click: ClickType::Left,
            side: RelativeSide::Top,
        };
        packet.apply(&mut config, &mut ejector, Face::North);
        assert_eq!(ejector.input_color(RelativeSide::Top), Some(0));
        packet.apply(&mut config, &mut ejector, Face::North);
        assert_eq!(ejector.input_color(RelativeSide::Top), Some(1));
        let shift = ConfigPacket::InputColor {
            click: ClickType::ShiftLeft,
            side: RelativeSide::Top,
        };
        shift.apply(&mut config, &mut ejector, Face::North);
        assert_eq!(ejector.input_color(RelativeSide::Top), None);
        config
            .config_mut(TransmissionType::Item)
            .unwrap()
            .add_disabled_sides(&[RelativeSide::Back]);
        ConfigPacket::InputColor {
            click: ClickType::Left,
            side: RelativeSide::Back,
        }
        .apply(&mut config, &mut ejector, Face::North);
        assert_eq!(ejector.input_color(RelativeSide::Back), None);
    }

    #[test]
    fn a_snapshot_carries_the_sides_and_the_ejector() {
        let mut from = config();
        let mut ejector = TileComponentEjector::new();
        let packets = [
            ConfigPacket::SideData { click: ClickType::Left, side: RelativeSide::Front, transmission: TransmissionType::Item },
            ConfigPacket::Eject { transmission: TransmissionType::Item },
            ConfigPacket::StrictInput,
            ConfigPacket::EjectColor { click: ClickType::Left },
            ConfigPacket::InputColor { click: ClickType::Right, side: RelativeSide::Left },
        ];
        for packet in packets {
            packet.apply(&mut from, &mut ejector, Face::North);
        }
        let bytes = encode_snapshot(&from, &ejector);
        let mut to = config();
        let mut other = TileComponentEjector::new();
        assert!(apply_snapshot(&bytes, &mut to, &mut other));
        assert_eq!(encode_snapshot(&to, &other), bytes);
        assert_eq!(other.input_color(RelativeSide::Left), Some(17));
        assert!(other.has_strict_input());
        assert!(!apply_snapshot(&bytes[1..], &mut to, &mut other));
    }

    #[test]
    fn the_button_tint_lightens_and_desaturates() {
        // Cinza sem matiz: só o valor sobe 0,1 (64 + 25,5 = 89,5 arredonda para 90 ou 89 conforme o ponto flutuante).
        let grey = button_tint(0x40_40_40);
        assert!((0xFF59_5959..=0xFF5A_5A5A).contains(&grey) && (grey & 0xFF) == ((grey >> 8) & 0xFF));
        // Branco fica branco.
        assert_eq!(button_tint(0xFF_FF_FF), 0xFFFF_FFFF);
    }

    #[test]
    fn the_inset_button_sits_four_pixels_inside() {
        assert_eq!(inset_button([-26, 6], true), [-21, 10, 18, 18]);
        assert_eq!(inset_button([-26, 34], true), [-21, 38, 18, 18]);
    }
}
