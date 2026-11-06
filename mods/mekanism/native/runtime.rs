// Regras das máquinas, sem tipos do jogo. Este arquivo é a lib de teste do mod e também o módulo
// crate::game::mekanism_rules. Segue TileEntityMetallurgicInfuser e TileEntityElectricMachine.
use machinery_basis_core::cached_recipe::ItemToItemRecipe;
use machinery_basis_core::chemical::ChemicalConversion;
use machinery_basis_core::containers::InventorySlot;
use machinery_basis_core::operation_tracker::RecipeError;
use machinery_basis_core::recipe::{ConversionFields, RecipeFields};
use machinery_basis_core::sides::{Face, FaceSlots, Sides};
use machinery_basis_core::tile::{
    ChemicalMachineConfig, ElectricMachineConfig, MachineConfig, ProgressMachine, UpgradeConfig,
};
use machinery_basis_core::two_input_recipe::ItemChemicalToItemRecipe;
use machinery_basis_core::upgrade::{UpgradeData, UpgradeTags, UpgradeTypes};
use std::rc::Rc;

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

/// `CableTier`: o id de cada cabo e quanto ele guarda.
pub const UNIVERSAL_CABLES: [(&str, i32); 4] = [
    ("mekanism:basic_universal_cable", 8_000),
    ("mekanism:advanced_universal_cable", 128_000),
    ("mekanism:elite_universal_cable", 1_024_000),
    ("mekanism:ultimate_universal_cable", 8_192_000),
];

/// `BASE_TICKS_REQUIRED`, `MAX_INFUSE` e os padrões de `UsageConfig`.
pub const TICKS_REQUIRED: u32 = 200;
pub const TANK_CAPACITY: u64 = 1_000;
pub const INFUSER_USAGE: i32 = 5;
pub const ENRICHMENT_USAGE: i32 = 5;
pub const CRUSHER_USAGE: i32 = 8;
pub const SMELTER_USAGE: i32 = 5;

/// `blockDeactivationDelay`, o padrão de `GeneralConfig`: quantos ticks o bloco espera para apagar.
pub const BLOCK_DEACTIVATION_DELAY: i32 = 60;
/// `UpgradeUtils`: o multiplicador dos upgrades (`upgradesMultiplier`). O máximo de cada tipo vem dos dados.
pub const UPGRADE_MULTIPLIER: f64 = 10.0;

/// `MekanismItems.UPGRADE` e o componente `MekanismDataComponents.UPGRADE_TYPE`, que guarda o tipo no item.
pub const UPGRADE_ITEM: &str = "mekanism:upgrade";
pub const UPGRADE_TYPE_COMPONENT: &str = "mekanism:upgrade_type";
/// `UpgradeIds.SPEED` e `UpgradeIds.ENERGY`.
pub const SPEED_UPGRADE: &str = "mekanism:speed";
pub const ENERGY_UPGRADE: &str = "mekanism:energy";
/// `MekanismTags.Upgrades.DEFAULT_MACHINE_UPGRADES`: o que `AttributeUpgradeSupport.DEFAULT_MACHINE_UPGRADES` aceita, e que as
/// quatro máquinas portadas usam.
pub const DEFAULT_MACHINE_UPGRADES: &str = "mekanism:upgrade_support/default_machine";
/// O prefixo da propriedade de modelo que diz o tipo do upgrade na mão: `mekanism:upgrade_type/<nome>`. É o lugar do
/// `ComponentItemModel` do original, que escolhe a textura pelo componente.
pub const UPGRADE_MODEL_PROPERTY: &str = "mekanism:upgrade_type/";

/// Os dados de upgrade que as quatro máquinas portadas usam: os tipos, o que a tag `default_machine` aceita, e o item que os
/// carrega, com o limite de pilha dele.
pub fn upgrade_data(
    types: &UpgradeTypes,
    tags: &UpgradeTags,
    item_max_stack_size: u32,
) -> Rc<UpgradeData> {
    Rc::new(UpgradeData::new(
        UPGRADE_ITEM,
        item_max_stack_size,
        types,
        tags,
        DEFAULT_MACHINE_UPGRADES,
    ))
}

/// `TileEntityEnrichmentChamber.TRACKED_ERROR_TYPES`, na ordem da tela.
pub const ELECTRIC_TRACKED_ERRORS: [RecipeError; 4] = [
    RecipeError::NOT_ENOUGH_ENERGY,
    RecipeError::NOT_ENOUGH_INPUT,
    RecipeError::NOT_ENOUGH_OUTPUT_SPACE,
    RecipeError::INPUT_DOESNT_PRODUCE_OUTPUT,
];

/// `TileEntityMetallurgicInfuser.TRACKED_ERROR_TYPES`, na ordem da tela.
pub const INFUSER_TRACKED_ERRORS: [RecipeError; 5] = [
    RecipeError::NOT_ENOUGH_ENERGY,
    RecipeError::NOT_ENOUGH_INPUT,
    RecipeError::NOT_ENOUGH_SECONDARY_INPUT,
    RecipeError::NOT_ENOUGH_OUTPUT_SPACE,
    RecipeError::INPUT_DOESNT_PRODUCE_OUTPUT,
];

/// O que as máquinas de progresso do Mekanism partilham: os valores de `UsageConfig`, `GeneralConfig` e `UpgradeUtils`.
fn machine_config(
    energy_per_tick: i32,
    tracked_errors: &[RecipeError],
    check_offset: u64,
    upgrades: Rc<UpgradeData>,
) -> MachineConfig {
    MachineConfig {
        energy_per_tick,
        base_ticks_required: TICKS_REQUIRED as i32,
        deactivation_delay: BLOCK_DEACTIVATION_DELAY,
        upgrades: UpgradeConfig {
            max_upgrade_multiplier: UPGRADE_MULTIPLIER,
            speed: Rc::from(SPEED_UPGRADE),
            energy: Rc::from(ENERGY_UPGRADE),
            data: upgrades,
        },
        tracked_errors: tracked_errors.to_vec(),
        check_offset,
    }
}

/// Uma máquina elétrica como a lib a recebe: valores do mod, receitas de quem as leu do jogo. O teste dos rastros usa esta
/// mesma configuração.
pub fn electric_config(
    energy_per_tick: i32,
    recipes: Vec<Rc<ItemToItemRecipe>>,
    check_offset: u64,
    upgrades: Rc<UpgradeData>,
) -> ElectricMachineConfig {
    ElectricMachineConfig {
        machine: machine_config(
            energy_per_tick,
            &ELECTRIC_TRACKED_ERRORS,
            check_offset,
            upgrades,
        ),
        recipes,
    }
}

/// O Metallurgic Infuser como a lib o recebe, também usado pelo teste dos rastros.
pub fn infuser_config(
    energy_per_tick: i32,
    recipes: Vec<Rc<ItemChemicalToItemRecipe>>,
    conversions: Vec<ChemicalConversion>,
    check_offset: u64,
    upgrades: Rc<UpgradeData>,
) -> ChemicalMachineConfig {
    ChemicalMachineConfig {
        machine: machine_config(
            energy_per_tick,
            &INFUSER_TRACKED_ERRORS,
            check_offset,
            upgrades,
        ),
        recipes,
        conversions,
        tank_capacity: TANK_CAPACITY,
    }
}

/// O `MEKTRACE` de um tick: os campos dos rastros de `dev/referencia/rastros`, na mesma ordem. O Metallurgic Infuser acrescenta
/// `tank` e `infusion` depois dos erros. Com `upgrades`, vêm no fim `speed`, `energy_upgrades`, `upgrade_progress`, `upgrade_in` e
/// `upgrade_out`, que só o rastro dos slots de upgrade grava.
pub fn trace_line(
    now: i64,
    machine: &ProgressMachine,
    chest: Option<&[(usize, String, u32)]>,
    upgrades: bool,
) -> String {
    let path = |id: &str| {
        id.split_once(':')
            .map_or(id.to_owned(), |(_, path)| path.to_owned())
    };
    let slot = |slot: &InventorySlot| {
        let id = slot
            .resource()
            .map_or(String::new(), |resource| path(&resource.id));
        format!("[\"{id}\",{}]", slot.amount())
    };
    let names = [
        (RecipeError::NOT_ENOUGH_ENERGY, "energy"),
        (RecipeError::NOT_ENOUGH_INPUT, "input"),
        (RecipeError::NOT_ENOUGH_OUTPUT_SPACE, "output_space"),
        (RecipeError::INPUT_DOESNT_PRODUCE_OUTPUT, "wrong_output"),
        (RecipeError::NOT_ENOUGH_SECONDARY_INPUT, "secondary"),
    ];
    let errors: Vec<String> = names
        .iter()
        .filter(|(error, _)| machine.has_error(*error))
        .map(|(_, name)| format!("\"{name}\""))
        .collect();
    let tile = machine.tile();
    let chemical = machine.chemical_side().map_or(String::new(), |side| {
        format!(
            ",\"tank\":{},\"infusion\":{}",
            side.tank.amount(),
            slot(&side.slot)
        )
    });
    // Com o ejetor ligado o rastro grava o baú: `[slot, id, quantidade]` dos slots com algo.
    let chest = chest.map_or(String::new(), |stacks| {
        let stacks: Vec<String> = stacks
            .iter()
            .map(|(index, id, amount)| format!("[{index},\"{id}\",{amount}]"))
            .collect();
        format!(",\"chest\":[{}]", stacks.join(","))
    });
    // `upgradeSlot` do `MachineTraceTest`: o tipo sem o namespace e a quantidade, ou vazio.
    let upgrade_slot = |slot: &InventorySlot| {
        let kind = slot
            .resource()
            .and_then(|resource| resource.variant)
            .map_or(String::new(), |variant| path(&variant));
        format!("[\"{kind}\",{}]", slot.amount())
    };
    let upgrades = if upgrades {
        let component = machine.upgrades();
        format!(
            ",\"speed\":{},\"energy_upgrades\":{},\"upgrade_progress\":{:?},\"upgrade_in\":{},\"upgrade_out\":{}",
            component.get_upgrades(SPEED_UPGRADE),
            component.get_upgrades(ENERGY_UPGRADE),
            component.scaled_upgrade_progress(),
            upgrade_slot(component.upgrade_slot()),
            upgrade_slot(component.upgrade_output_slot())
        )
    } else {
        String::new()
    };
    format!(
        "{{\"t\":{now},\"marks\":[],\"progress\":{},\"required\":{},\"energy\":{},\"capacity\":{},\"per_tick\":{},\"active\":{},\"block_active\":{},\"powered\":{},\"comparator\":{},\"input\":{},\"output\":{},\"errors\":[{}]{}{}{}}}",
        machine.recipe_progress(),
        machine.ticks_required(),
        machine.energy().amount(),
        machine.energy().capacity(),
        machine.energy().energy_per_tick(),
        tile.active(),
        tile.block_active(),
        tile.is_powered(),
        tile.current_redstone_level(),
        slot(machine.input()),
        slot(machine.output()),
        errors.join(","),
        chemical,
        chest,
        upgrades
    )
}

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

/// Quanto o cabo guarda, se o bloco for um cabo.
pub fn cable_capacity(block: &str) -> Option<i32> {
    let cable = UNIVERSAL_CABLES.iter().find(|(id, _)| *id == block);
    cable.map(|&(_, capacity)| capacity)
}

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
    fn each_cable_tier_has_its_capacity() {
        assert_eq!(
            cable_capacity("mekanism:basic_universal_cable"),
            Some(8_000)
        );
        assert_eq!(
            cable_capacity("mekanism:ultimate_universal_cable"),
            Some(8_192_000)
        );
        assert_eq!(cable_capacity(INFUSER), None);
    }

    #[test]
    fn the_buffer_holds_four_ticks() {
        assert_eq!(energy_capacity(INFUSER_USAGE), 20);
        assert_eq!(energy_capacity(CRUSHER_USAGE), 32);
    }
}
