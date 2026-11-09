//! Que moldura um slot de máquina tem na tela: `GuiMekanism.addSlots` escolhe pelo papel que a configuração de lados dá ao slot
//! (`findDataType`) e, sem ele, pelo tipo do slot no contêiner. Só a escolha: os ids dos sprites são de quem desenha.

pub use crate::side_config::DataType;

/// `ContainerSlotType`: o que o slot é para o contêiner, quando a configuração de lados não o diz.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContainerSlotType {
    Ignored,
    Normal,
    Power,
    Input,
    Extra,
    Output,
    Validity,
}

/// `SlotType`: a moldura que se desenha.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotType {
    Normal,
    Dark,
    Power,
    Extra,
    Input,
    Input2,
    Output,
    Output2,
    InnerHolderSlot,
}

impl SlotType {
    /// O nome do sprite (`SlotType.texture`: `slot/<nome>`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Dark => "dark",
            Self::Power => "power",
            Self::Extra => "extra",
            Self::Input => "input",
            Self::Input2 => "input_2",
            Self::Output => "output",
            Self::Output2 => "output_2",
            Self::InnerHolderSlot => "inner_holder_slot",
        }
    }

    /// `SlotType.get(DataType)`: o papel da configuração dá a moldura, e o que não tem moldura própria cai na normal.
    pub fn of(data_type: DataType) -> Self {
        match data_type {
            DataType::Input | DataType::Input1 => Self::Input,
            DataType::Input2 => Self::Input2,
            DataType::Output | DataType::Output1 => Self::Output,
            DataType::Output2 => Self::Output2,
            DataType::Energy => Self::Power,
            DataType::Extra => Self::Extra,
            DataType::None | DataType::InputOutput => Self::Normal,
        }
    }
}

/// `SlotOverlay`: o desenho que vai por cima da moldura (o raio do slot de energia).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotOverlay {
    Minus,
    Plus,
    Power,
    Input,
    Output,
    Check,
    X,
    Formula,
    Upgrade,
    Select,
    Module,
}

impl SlotOverlay {
    /// O nome do sprite (`SlotOverlay.texture`: `slot/overlay/<nome>`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Minus => "minus",
            Self::Plus => "plus",
            Self::Power => "power",
            Self::Input => "input",
            Self::Output => "output",
            Self::Check => "check",
            Self::X => "x",
            Self::Formula => "formula",
            Self::Upgrade => "upgrade",
            Self::Select => "select",
            Self::Module => "module",
        }
    }
}

/// Um slot de item numa tela: onde o item fica, o que ele é para o contêiner e o desenho por cima da moldura.
#[derive(Clone, Copy, Debug)]
pub struct SlotSpec {
    pub item: [f32; 2],
    pub container: ContainerSlotType,
    pub overlay: Option<SlotOverlay>,
}

/// A configuração de itens de uma máquina (`ConfigInfo`): cada papel com os slots dele, na ordem em que `addSlotInfo` os registrou.
#[derive(Clone, Copy, Debug)]
pub struct ItemConfig {
    pub entries: &'static [(DataType, &'static [usize])],
}

impl ItemConfig {
    /// `getSupportedDataTypes().size()`: os papéis registrados mais o `NONE`, que todo `ConfigInfo` tem.
    pub fn supported(&self) -> usize {
        1 + self.entries.len()
    }

    /// `getDataTypeForContainer`: os papéis que têm o slot, na ordem do registro.
    pub fn types_for(&self, slot: usize) -> Vec<DataType> {
        self.entries
            .iter()
            .filter(|(_, slots)| slots.contains(&slot))
            .map(|(data_type, _)| *data_type)
            .collect()
    }

    /// `ISideConfiguration.getActiveDataType`: o primeiro papel do slot, mas só se ele não vale para todos (um slot que tem todos os
    /// papéis não diz nada). `None` quando o slot não está na configuração.
    pub fn active_data_type(&self, slot: usize) -> Option<DataType> {
        let types = self.types_for(slot);
        let count = types.len();
        (count > 0 && count < self.supported()).then(|| types[0])
    }
}

/// A moldura de um slot de uma `InventoryContainerSlot` (`GuiMekanism.addSlots`), ou nada se o slot não desenha moldura
/// (`ContainerSlotType.IGNORED`). `config` é a configuração de itens da máquina, se ela é configurável.
pub fn slot_type(
    config: Option<&ItemConfig>,
    slot: usize,
    container_type: ContainerSlotType,
) -> Option<SlotType> {
    if let Some(data_type) = config.and_then(|config| config.active_data_type(slot)) {
        return Some(SlotType::of(data_type));
    }
    match container_type {
        ContainerSlotType::Input | ContainerSlotType::Extra => Some(SlotType::Normal),
        ContainerSlotType::Output => Some(SlotType::Normal),
        ContainerSlotType::Power => Some(SlotType::Power),
        ContainerSlotType::Normal | ContainerSlotType::Validity => Some(SlotType::Normal),
        ContainerSlotType::Ignored => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `TileComponentConfig.setupItemIOConfig`: entrada, saída, os dois juntos e a energia.
    const IO: ItemConfig = ItemConfig {
        entries: &[
            (DataType::Input, &[0]),
            (DataType::Output, &[1]),
            (DataType::InputOutput, &[0, 1]),
            (DataType::Energy, &[2]),
        ],
    };

    #[test]
    fn the_role_of_a_slot_picks_its_frame() {
        assert_eq!(
            slot_type(Some(&IO), 0, ContainerSlotType::Input),
            Some(SlotType::Input)
        );
        assert_eq!(
            slot_type(Some(&IO), 1, ContainerSlotType::Output),
            Some(SlotType::Output)
        );
        assert_eq!(
            slot_type(Some(&IO), 2, ContainerSlotType::Power),
            Some(SlotType::Power)
        );
    }

    #[test]
    fn the_first_registered_role_wins() {
        // A entrada está em `Input` e em `InputOutput`: vale a primeira.
        assert_eq!(IO.types_for(0), [DataType::Input, DataType::InputOutput]);
        assert_eq!(IO.active_data_type(0), Some(DataType::Input));
        assert_eq!(IO.supported(), 5);
    }

    #[test]
    fn a_slot_in_every_role_still_has_one() {
        // `supported` conta o `NONE`, que nenhum slot tem, então `count < supported` vale sempre que o slot está em algum papel.
        const ALL: ItemConfig = ItemConfig {
            entries: &[(DataType::Input, &[0]), (DataType::Output, &[0])],
        };
        assert_eq!(ALL.supported(), 3);
        assert_eq!(ALL.active_data_type(0), Some(DataType::Input));
        assert_eq!(ALL.active_data_type(1), None);
    }

    #[test]
    fn without_a_config_the_container_type_picks_the_frame() {
        assert_eq!(
            slot_type(None, 0, ContainerSlotType::Extra),
            Some(SlotType::Normal)
        );
        assert_eq!(
            slot_type(None, 0, ContainerSlotType::Input),
            Some(SlotType::Normal)
        );
        assert_eq!(
            slot_type(None, 0, ContainerSlotType::Output),
            Some(SlotType::Normal)
        );
        assert_eq!(
            slot_type(None, 0, ContainerSlotType::Power),
            Some(SlotType::Power)
        );
        assert_eq!(
            slot_type(None, 0, ContainerSlotType::Validity),
            Some(SlotType::Normal)
        );
        assert_eq!(slot_type(None, 0, ContainerSlotType::Ignored), None);
        // Um slot fora da configuração também cai no tipo do contêiner.
        assert_eq!(
            slot_type(Some(&IO), 9, ContainerSlotType::Power),
            Some(SlotType::Power)
        );
    }

    #[test]
    fn an_overlay_is_named_after_its_sprite() {
        assert_eq!(SlotOverlay::Power.name(), "power");
        assert_eq!(SlotOverlay::Minus.name(), "minus");
    }

    #[test]
    fn each_role_has_its_own_frame_and_the_rest_the_plain_one() {
        let cases = [
            (DataType::Input, "input"),
            (DataType::Input1, "input"),
            (DataType::Input2, "input_2"),
            (DataType::Output, "output"),
            (DataType::Output1, "output"),
            (DataType::Output2, "output_2"),
            (DataType::Energy, "power"),
            (DataType::Extra, "extra"),
            (DataType::None, "normal"),
            (DataType::InputOutput, "normal"),
        ];
        for (data_type, name) in cases {
            assert_eq!(SlotType::of(data_type).name(), name, "{data_type:?}");
        }
    }
}
