// Regras das máquinas, sem tipos do jogo. Este arquivo é a lib de teste do mod e também o módulo
// crate::game::mekanism_rules. Segue TileEntityMetallurgicInfuser e TileEntityElectricMachine.
use machinery_basis_core::recipe::{ConversionFields, RecipeFields};
use machinery_basis_core::sides::{Face, FaceSlots, Sides};

pub const INFUSER: &str = "mekanism:metallurgic_infuser";
pub const INFUSING: &str = "mekanism:metallurgic_infusing";
pub const CHEMICAL_CONVERSION: &str = "mekanism:chemical_conversion";
pub const INFUSING_FIELDS: RecipeFields = RecipeFields {
    item: "item_input",
    resource: Some("chemical_input"),
    output: "output",
};
pub const ELECTRIC_FIELDS: RecipeFields = RecipeFields {
    item: "input",
    resource: None,
    output: "output",
};
pub const CONVERSION_FIELDS: ConversionFields = ConversionFields {
    item: "input",
    output: "output",
};

pub const ENRICHMENT_CHAMBER: &str = "mekanism:enrichment_chamber";
pub const CRUSHER: &str = "mekanism:crusher";
pub const ENERGIZED_SMELTER: &str = "mekanism:energized_smelter";
pub const ENRICHING: &str = "mekanism:enriching";
pub const CRUSHING: &str = "mekanism:crushing";
pub const SMELTING: &str = "smelting";

/// `BASE_TICKS_REQUIRED`, `MAX_INFUSE` e os padrões de `UsageConfig`.
pub const TICKS_REQUIRED: u32 = 200;
pub const TANK_CAPACITY: u64 = 1_000;
pub const INFUSER_USAGE: i32 = 5;
pub const ENRICHMENT_USAGE: i32 = 5;
pub const CRUSHER_USAGE: i32 = 8;
pub const SMELTER_USAGE: i32 = 5;

/// `AttributeEnergy.STORAGE_MULTIPLIER`: o buffer guarda quatro ticks de uso.
pub const fn energy_capacity(usage: i32) -> i32 {
    4 * usage
}

// Na ordem de getInitialInventory.
pub const INFUSION_SLOT: usize = 0;
pub const INPUT_SLOT: usize = 1;
pub const OUTPUT_SLOT: usize = 2;
pub const ENERGY_SLOT: usize = 3;
pub const SLOTS: usize = 4;

// Na ordem de TileEntityElectricMachine.getInitialInventory.
pub const ELECTRIC_INPUT_SLOT: usize = 0;
pub const ELECTRIC_OUTPUT_SLOT: usize = 1;
pub const ELECTRIC_ENERGY_SLOT: usize = 2;
pub const ELECTRIC_SLOTS: usize = 3;

const INPUT_SLOTS: &[usize] = &[INPUT_SLOT];
const INFUSION_SLOTS: &[usize] = &[INFUSION_SLOT];
const ELECTRIC_INPUT_SLOTS: &[usize] = &[ELECTRIC_INPUT_SLOT];

/// `LightConfigInfo.EXTRA_MACHINE`: a direita é saída, a base é o slot extra, as costas são o slot
/// de energia e o resto é entrada. O slot de energia ainda não aceita nada.
pub fn item_slots(front: Face, face: Face) -> FaceSlots {
    if face == front.right_of() || face == front.opposite() {
        FaceSlots::default()
    } else if face == Face::Down {
        FaceSlots {
            inputs: INFUSION_SLOTS,
            outputs: &[],
        }
    } else {
        FaceSlots {
            inputs: INPUT_SLOTS,
            outputs: &[],
        }
    }
}

/// `LightConfigInfo.MACHINE`: a direita é saída, as costas são o slot de energia, e o resto entrada.
pub fn electric_item_slots(front: Face, face: Face) -> FaceSlots {
    if face == front.right_of() || face == front.opposite() {
        FaceSlots::default()
    } else {
        FaceSlots {
            inputs: ELECTRIC_INPUT_SLOTS,
            outputs: &[],
        }
    }
}

/// Para um hopper: recebe por tudo que tem slot de entrada, e não entrega por nenhuma face. A saída
/// fica à direita, e um hopper só puxa do bloco acima dele.
pub fn sides(front: Face, slots: fn(Face, Face) -> FaceSlots) -> Sides {
    let mut sides = Sides::default();
    for face in Face::ALL {
        sides.input[face as usize] = !slots(front, face).inputs.is_empty();
    }
    sides
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_enter_by_the_front_the_left_and_the_top() {
        let front = Face::North;
        for face in [Face::North, front.left_of(), Face::Up] {
            assert_eq!(item_slots(front, face).inputs, [INPUT_SLOT], "{face:?}");
        }
    }

    #[test]
    fn the_base_feeds_the_infusion_slot() {
        assert_eq!(item_slots(Face::East, Face::Down).inputs, [INFUSION_SLOT]);
    }

    #[test]
    fn the_right_and_the_back_take_nothing() {
        for front in [Face::North, Face::South, Face::East, Face::West] {
            let sides = sides(front, item_slots);
            assert!(!sides.accepts_input(front.right_of()));
            assert!(!sides.accepts_input(front.opposite()));
            assert!(sides.accepts_input(front));
        }
    }

    #[test]
    fn no_face_gives_to_a_hopper() {
        assert_eq!(sides(Face::North, item_slots).output, [false; 6]);
        assert_eq!(sides(Face::North, electric_item_slots).output, [false; 6]);
    }

    #[test]
    fn an_electric_machine_also_takes_by_the_base() {
        let front = Face::North;
        for face in [Face::North, front.left_of(), Face::Up, Face::Down] {
            let slots = electric_item_slots(front, face);
            assert_eq!(slots.inputs, [ELECTRIC_INPUT_SLOT], "{face:?}");
        }
        assert!(
            electric_item_slots(front, front.right_of())
                .inputs
                .is_empty()
        );
        assert!(
            electric_item_slots(front, front.opposite())
                .inputs
                .is_empty()
        );
    }

    #[test]
    fn the_buffer_holds_four_ticks() {
        assert_eq!(energy_capacity(INFUSER_USAGE), 20);
        assert_eq!(energy_capacity(CRUSHER_USAGE), 32);
    }
}
