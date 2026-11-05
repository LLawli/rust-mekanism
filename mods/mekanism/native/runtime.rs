// Regras do Metallurgic Infuser, sem tipos do jogo. Este arquivo é a lib de teste do mod e também o
// módulo crate::game::mekanism_rules. Segue TileEntityMetallurgicInfuser.
use machinery_basis_core::recipe::{ConversionFields, ItemResourceFields};
use machinery_basis_core::sides::{Face, FaceSlots, Sides};

pub const INFUSER: &str = "mekanism:metallurgic_infuser";
pub const MENU: &str = "mekanism:metallurgic_infuser";
pub const INFUSING: &str = "mekanism:metallurgic_infusing";
pub const CHEMICAL_CONVERSION: &str = "mekanism:chemical_conversion";
pub const INFUSING_FIELDS: ItemResourceFields = ItemResourceFields {
    item: "item_input",
    resource: "chemical_input",
    output: "output",
};
pub const CONVERSION_FIELDS: ConversionFields = ConversionFields {
    item: "input",
    output: "output",
};

/// `BASE_TICKS_REQUIRED`, `MAX_INFUSE` e o padrão de `UsageConfig`. O buffer guarda
/// `AttributeEnergy.STORAGE_MULTIPLIER` ticks de uso.
pub const TICKS_REQUIRED: u32 = 200;
pub const TANK_CAPACITY: u64 = 1_000;
pub const ENERGY_PER_TICK: i32 = 5;
pub const ENERGY_CAPACITY: i32 = 4 * ENERGY_PER_TICK;

// Na ordem de getInitialInventory.
pub const INFUSION_SLOT: usize = 0;
pub const INPUT_SLOT: usize = 1;
pub const OUTPUT_SLOT: usize = 2;
pub const ENERGY_SLOT: usize = 3;
pub const SLOTS: usize = 4;

const INPUT_SLOTS: &[usize] = &[INPUT_SLOT];
const INFUSION_SLOTS: &[usize] = &[INFUSION_SLOT];

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

/// Para um hopper: recebe por tudo que tem slot de entrada, e não entrega por nenhuma face. A saída
/// fica à direita, e um hopper só puxa do bloco acima dele.
pub fn sides(front: Face) -> Sides {
    let mut sides = Sides::default();
    for face in Face::ALL {
        sides.input[face as usize] = !item_slots(front, face).inputs.is_empty();
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
            let sides = sides(front);
            assert!(!sides.accepts_input(front.right_of()));
            assert!(!sides.accepts_input(front.opposite()));
            assert!(sides.accepts_input(front));
        }
    }

    #[test]
    fn no_face_gives_to_a_hopper() {
        assert_eq!(sides(Face::North).output, [false; 6]);
    }

    #[test]
    fn the_buffer_holds_four_ticks() {
        assert_eq!(ENERGY_CAPACITY, 20);
    }
}
