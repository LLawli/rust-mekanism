// O que a aba criativa do Mekanism pede além do arquivo `creative_tab/mekanism.json` (só ids de item): as pilhas com componente e os itens que o original acrescenta às
// abas do vanilla. `MekanismCreativeTabs.addToExistingTabs` (`BuildCreativeModeTabContentsEvent`) põe os itens do mod no fim de cada aba do vanilla, na ordem da lista
// dele, e `ItemUpgrade` entra nas abas só pelas pilhas de cada tipo (`ICustomCreativeTabContents`, `addDefault` falso), na ordem do registro `UPGRADES`.
// O Horizon só aceita ids nos itens de uma aba de mod (`stack(id)` em `Plan::creative_tabs`) e não conhece as abas do vanilla por mod.

/// A aba do mod: `<id do mod>:<nome do arquivo>`.
pub const MEKANISM_TAB: &str = "mekanism:mekanism";

/// A pilha que o Horizon escreve para um id (`{count:1,id:"..."}`).
pub fn plain_stack(id: &str) -> String {
    format!("{{count:1,id:\"{id}\"}}")
}

/// Os itens que a aba mostra vazios e cheios (`ICustomCreativeTabContents.addItems` -> `ContainerType.ENERGY.getFilledVariant`, na aba do mod e em cada aba do vanilla a que
/// `addToExistingTabs` os acrescenta), com a capacidade padrão (`gear.toml` e `tiers.toml`): a aba é montada antes de o servidor ler os arquivos, e o glue (`sync_creative_energy`) troca
/// pela capacidade de agora a cada tick (decisão 58). O cubo criativo fica de fora: o original o põe em duas pilhas com a configuração de lados do cubo (`ItemBlockEnergyCube.addItems`),
/// que espera o tile do cubo.
pub const FILLED_VARIANTS: [(&str, i64); 16] = [
    ("mekanism:robit", 100_000),
    ("mekanism:energy_tablet", 1_000_000),
    ("mekanism:portable_teleporter", 1_000_000),
    ("mekanism:mekasuit_helmet", 16_000_000),
    ("mekanism:mekasuit_bodyarmor", 16_000_000),
    ("mekanism:mekasuit_pants", 16_000_000),
    ("mekanism:mekasuit_boots", 16_000_000),
    ("mekanism:atomic_disassembler", 1_000_000),
    ("mekanism:electric_bow", 120_000),
    ("mekanism:meka_tool", 16_000_000),
    ("mekanism:free_runners", 64_000),
    ("mekanism:free_runners_armored", 64_000),
    ("mekanism:basic_energy_cube", 4_000_000),
    ("mekanism:advanced_energy_cube", 16_000_000),
    ("mekanism:elite_energy_cube", 64_000_000),
    ("mekanism:ultimate_energy_cube", 256_000_000),
];

/// A pilha de um item cheio: o componente `mekanism:energy` é um `long` (`ExtraCodecs.NON_NEGATIVE_LONG`), escrito com o sufixo `L`.
pub fn filled_stack(id: &str, energy: i64) -> String {
    format!("{{count:1,id:\"{id}\",components:{{\"mekanism:energy\":{energy}L}}}}")
}

/// A energia da pilha cheia de um item, se a aba o mostra vazio e cheio.
pub fn filled_energy(id: &str) -> Option<i64> {
    FILLED_VARIANTS
        .iter()
        .find(|(item, _)| *item == id)
        .map(|(_, energy)| *energy)
}

/// Os tipos de upgrade na ordem do registro (`data/mekanism/mekanism/upgrade`, ordem do nome do arquivo).
pub const UPGRADE_TYPES: [&str; 8] = [
    "anchor",
    "chemical",
    "energy",
    "filter",
    "muffling",
    "speed",
    "stone_generator",
    "xp_containment",
];

/// A pilha do item de upgrade de um tipo (`IUpgradeHelper.asStack`): o componente `mekanism:upgrade_type`.
pub fn upgrade_stack(kind: &str) -> String {
    format!(
        "{{count:1,id:\"mekanism:upgrade\",components:{{\"mekanism:upgrade_type\":\"mekanism:{kind}\"}}}}"
    )
}

/// Os itens do mod que o original acrescenta a cada aba do vanilla, com os ids que o port tem, na ordem da lista do original (`addToExistingTabs`).
/// `mekanism:upgrade` marca onde entram as pilhas de upgrade.
pub const VANILLA_TABS: [(&str, &[&str]); 7] = [
    (
        "minecraft:building_blocks",
        &[
            "mekanism:block_salt",
            "mekanism:block_bronze",
            "mekanism:block_steel",
            "mekanism:block_charcoal",
            "mekanism:block_refined_obsidian",
            "mekanism:block_refined_glowstone",
            "mekanism:block_osmium",
            "mekanism:block_tin",
            "mekanism:block_lead",
            "mekanism:block_uranium",
        ],
    ),
    (
        "minecraft:natural_blocks",
        &[
            "mekanism:block_salt",
            "mekanism:tin_ore",
            "mekanism:deepslate_tin_ore",
            "mekanism:osmium_ore",
            "mekanism:deepslate_osmium_ore",
            "mekanism:uranium_ore",
            "mekanism:deepslate_uranium_ore",
            "mekanism:fluorite_ore",
            "mekanism:deepslate_fluorite_ore",
            "mekanism:lead_ore",
            "mekanism:deepslate_lead_ore",
        ],
    ),
    (
        "minecraft:functional_blocks",
        &[
            "mekanism:basic_smelting_factory",
            "mekanism:basic_enriching_factory",
            "mekanism:basic_crushing_factory",
            "mekanism:basic_compressing_factory",
            "mekanism:basic_combining_factory",
            "mekanism:basic_purifying_factory",
            "mekanism:basic_injecting_factory",
            "mekanism:basic_infusing_factory",
            "mekanism:basic_sawing_factory",
            "mekanism:advanced_smelting_factory",
            "mekanism:advanced_enriching_factory",
            "mekanism:advanced_crushing_factory",
            "mekanism:advanced_compressing_factory",
            "mekanism:advanced_combining_factory",
            "mekanism:advanced_purifying_factory",
            "mekanism:advanced_injecting_factory",
            "mekanism:advanced_infusing_factory",
            "mekanism:advanced_sawing_factory",
            "mekanism:elite_smelting_factory",
            "mekanism:elite_enriching_factory",
            "mekanism:elite_crushing_factory",
            "mekanism:elite_compressing_factory",
            "mekanism:elite_combining_factory",
            "mekanism:elite_purifying_factory",
            "mekanism:elite_injecting_factory",
            "mekanism:elite_infusing_factory",
            "mekanism:elite_sawing_factory",
            "mekanism:ultimate_smelting_factory",
            "mekanism:ultimate_enriching_factory",
            "mekanism:ultimate_crushing_factory",
            "mekanism:ultimate_compressing_factory",
            "mekanism:ultimate_combining_factory",
            "mekanism:ultimate_purifying_factory",
            "mekanism:ultimate_injecting_factory",
            "mekanism:ultimate_infusing_factory",
            "mekanism:ultimate_sawing_factory",
            "mekanism:basic_bin",
            "mekanism:advanced_bin",
            "mekanism:elite_bin",
            "mekanism:ultimate_bin",
            "mekanism:creative_bin",
            "mekanism:enrichment_chamber",
            "mekanism:osmium_compressor",
            "mekanism:combiner",
            "mekanism:crusher",
            "mekanism:digital_miner",
            "mekanism:metallurgic_infuser",
            "mekanism:purification_chamber",
            "mekanism:energized_smelter",
            "mekanism:teleporter",
            "mekanism:electric_pump",
            "mekanism:logistical_sorter",
            "mekanism:rotary_condensentrator",
            "mekanism:chemical_oxidizer",
            "mekanism:chemical_infuser",
            "mekanism:chemical_injection_chamber",
            "mekanism:electrolytic_separator",
            "mekanism:precision_sawmill",
            "mekanism:chemical_dissolution_chamber",
            "mekanism:chemical_washer",
            "mekanism:chemical_crystallizer",
            "mekanism:seismic_vibrator",
            "mekanism:pressurized_reaction_chamber",
            "mekanism:isotopic_centrifuge",
            "mekanism:nutritional_liquifier",
            "mekanism:basic_fluid_tank",
            "mekanism:advanced_fluid_tank",
            "mekanism:elite_fluid_tank",
            "mekanism:ultimate_fluid_tank",
            "mekanism:creative_fluid_tank",
            "mekanism:fluidic_plenisher",
            "mekanism:solar_neutron_activator",
            "mekanism:resistive_heater",
            "mekanism:formulaic_assemblicator",
            "mekanism:antiprotonic_nucleosynthesizer",
            "mekanism:pigment_extractor",
            "mekanism:pigment_mixer",
            "mekanism:painting_machine",
            "mekanism:dimensional_stabilizer",
            "mekanism:basic_energy_cube",
            "mekanism:advanced_energy_cube",
            "mekanism:elite_energy_cube",
            "mekanism:ultimate_energy_cube",
            "mekanism:creative_energy_cube",
            "mekanism:basic_universal_cable",
            "mekanism:advanced_universal_cable",
            "mekanism:elite_universal_cable",
            "mekanism:ultimate_universal_cable",
            "mekanism:basic_mechanical_pipe",
            "mekanism:advanced_mechanical_pipe",
            "mekanism:elite_mechanical_pipe",
            "mekanism:ultimate_mechanical_pipe",
            "mekanism:basic_pressurized_tube",
            "mekanism:advanced_pressurized_tube",
            "mekanism:elite_pressurized_tube",
            "mekanism:ultimate_pressurized_tube",
            "mekanism:basic_logistical_transporter",
            "mekanism:advanced_logistical_transporter",
            "mekanism:elite_logistical_transporter",
            "mekanism:ultimate_logistical_transporter",
            "mekanism:restrictive_transporter",
            "mekanism:diversion_transporter",
            "mekanism:basic_thermodynamic_conductor",
            "mekanism:advanced_thermodynamic_conductor",
            "mekanism:elite_thermodynamic_conductor",
            "mekanism:ultimate_thermodynamic_conductor",
            "mekanism:basic_chemical_tank",
            "mekanism:advanced_chemical_tank",
            "mekanism:elite_chemical_tank",
            "mekanism:ultimate_chemical_tank",
            "mekanism:creative_chemical_tank",
            "mekanism:security_desk",
            "mekanism:radioactive_waste_barrel",
            "mekanism:personal_chest",
            "mekanism:personal_barrel",
            "mekanism:chargepad",
            "mekanism:laser",
            "mekanism:laser_amplifier",
            "mekanism:laser_tractor_beam",
            "mekanism:quantum_entangloporter",
            "mekanism:oredictionificator",
            "mekanism:fuelwood_heater",
            "mekanism:modification_station",
            "mekanism:qio_drive_array",
            "mekanism:qio_dashboard",
            "mekanism:qio_exporter",
            "mekanism:qio_importer",
            "mekanism:qio_redstone_adapter",
        ],
    ),
    (
        "minecraft:redstone_blocks",
        &[
            "mekanism:industrial_alarm",
            "mekanism:basic_smelting_factory",
            "mekanism:basic_enriching_factory",
            "mekanism:basic_crushing_factory",
            "mekanism:basic_compressing_factory",
            "mekanism:basic_combining_factory",
            "mekanism:basic_purifying_factory",
            "mekanism:basic_injecting_factory",
            "mekanism:basic_infusing_factory",
            "mekanism:basic_sawing_factory",
            "mekanism:advanced_smelting_factory",
            "mekanism:advanced_enriching_factory",
            "mekanism:advanced_crushing_factory",
            "mekanism:advanced_compressing_factory",
            "mekanism:advanced_combining_factory",
            "mekanism:advanced_purifying_factory",
            "mekanism:advanced_injecting_factory",
            "mekanism:advanced_infusing_factory",
            "mekanism:advanced_sawing_factory",
            "mekanism:elite_smelting_factory",
            "mekanism:elite_enriching_factory",
            "mekanism:elite_crushing_factory",
            "mekanism:elite_compressing_factory",
            "mekanism:elite_combining_factory",
            "mekanism:elite_purifying_factory",
            "mekanism:elite_injecting_factory",
            "mekanism:elite_infusing_factory",
            "mekanism:elite_sawing_factory",
            "mekanism:ultimate_smelting_factory",
            "mekanism:ultimate_enriching_factory",
            "mekanism:ultimate_crushing_factory",
            "mekanism:ultimate_compressing_factory",
            "mekanism:ultimate_combining_factory",
            "mekanism:ultimate_purifying_factory",
            "mekanism:ultimate_injecting_factory",
            "mekanism:ultimate_infusing_factory",
            "mekanism:ultimate_sawing_factory",
            "mekanism:basic_bin",
            "mekanism:advanced_bin",
            "mekanism:elite_bin",
            "mekanism:ultimate_bin",
            "mekanism:creative_bin",
            "mekanism:dynamic_valve",
            "mekanism:thermal_evaporation_valve",
            "mekanism:induction_casing",
            "mekanism:induction_port",
            "mekanism:boiler_valve",
            "mekanism:radioactive_waste_barrel",
            "mekanism:enrichment_chamber",
            "mekanism:osmium_compressor",
            "mekanism:combiner",
            "mekanism:crusher",
            "mekanism:digital_miner",
            "mekanism:metallurgic_infuser",
            "mekanism:purification_chamber",
            "mekanism:energized_smelter",
            "mekanism:teleporter",
            "mekanism:electric_pump",
            "mekanism:logistical_sorter",
            "mekanism:rotary_condensentrator",
            "mekanism:chemical_oxidizer",
            "mekanism:chemical_infuser",
            "mekanism:chemical_injection_chamber",
            "mekanism:electrolytic_separator",
            "mekanism:precision_sawmill",
            "mekanism:chemical_dissolution_chamber",
            "mekanism:chemical_washer",
            "mekanism:chemical_crystallizer",
            "mekanism:pressurized_reaction_chamber",
            "mekanism:isotopic_centrifuge",
            "mekanism:nutritional_liquifier",
            "mekanism:basic_fluid_tank",
            "mekanism:advanced_fluid_tank",
            "mekanism:elite_fluid_tank",
            "mekanism:ultimate_fluid_tank",
            "mekanism:creative_fluid_tank",
            "mekanism:fluidic_plenisher",
            "mekanism:laser_amplifier",
            "mekanism:laser_tractor_beam",
            "mekanism:solar_neutron_activator",
            "mekanism:formulaic_assemblicator",
            "mekanism:antiprotonic_nucleosynthesizer",
            "mekanism:pigment_extractor",
            "mekanism:pigment_mixer",
            "mekanism:painting_machine",
            "mekanism:sps_port",
            "mekanism:dimensional_stabilizer",
            "mekanism:basic_energy_cube",
            "mekanism:advanced_energy_cube",
            "mekanism:elite_energy_cube",
            "mekanism:ultimate_energy_cube",
            "mekanism:creative_energy_cube",
            "mekanism:basic_universal_cable",
            "mekanism:advanced_universal_cable",
            "mekanism:elite_universal_cable",
            "mekanism:ultimate_universal_cable",
            "mekanism:basic_mechanical_pipe",
            "mekanism:advanced_mechanical_pipe",
            "mekanism:elite_mechanical_pipe",
            "mekanism:ultimate_mechanical_pipe",
            "mekanism:basic_pressurized_tube",
            "mekanism:advanced_pressurized_tube",
            "mekanism:elite_pressurized_tube",
            "mekanism:ultimate_pressurized_tube",
            "mekanism:basic_thermodynamic_conductor",
            "mekanism:advanced_thermodynamic_conductor",
            "mekanism:elite_thermodynamic_conductor",
            "mekanism:ultimate_thermodynamic_conductor",
            "mekanism:basic_chemical_tank",
            "mekanism:advanced_chemical_tank",
            "mekanism:elite_chemical_tank",
            "mekanism:ultimate_chemical_tank",
            "mekanism:creative_chemical_tank",
            "mekanism:diversion_transporter",
        ],
    ),
    (
        "minecraft:tools_and_utilities",
        &[
            "mekanism:configurator",
            "mekanism:network_reader",
            "mekanism:dosimeter",
            "mekanism:geiger_counter",
            "mekanism:dictionary",
            "mekanism:configuration_card",
            "mekanism:gauge_dropper",
            "mekanism:crafting_formula",
            "mekanism:portable_qio_dashboard",
            "mekanism:atomic_disassembler",
            "mekanism:meka_tool",
            "mekanism:scuba_mask",
            "mekanism:scuba_tank",
            "mekanism:free_runners",
            "mekanism:free_runners_armored",
            "mekanism:jetpack",
            "mekanism:jetpack_armored",
            "mekanism:hdpe_elytra",
            "mekanism:hazmat_mask",
            "mekanism:hazmat_gown",
            "mekanism:hazmat_pants",
            "mekanism:hazmat_boots",
            "mekanism:cardboard_box",
            "mekanism:upgrade",
            "mekanism:basic_tier_installer",
            "mekanism:advanced_tier_installer",
            "mekanism:elite_tier_installer",
            "mekanism:ultimate_tier_installer",
            // Os tanques de fluido (`MekanismCreativeTabs.java:132-135`); os tanques cheios (`addFilledTanks`) esperam os fluidos nos tanques.
            "mekanism:basic_fluid_tank",
            "mekanism:advanced_fluid_tank",
            "mekanism:elite_fluid_tank",
            "mekanism:ultimate_fluid_tank",
            "mekanism:creative_fluid_tank",
            // Depois dos tanques: `addToDisplay(MekanismFluids.FLUIDS, event)` (`MekanismCreativeTabs.java:136`) põe os baldes dos 18
            // fluidos, na ordem de registro. Os do Generators são da aba dele.
            "mekanism:hydrogen_bucket",
            "mekanism:oxygen_bucket",
            "mekanism:chlorine_bucket",
            "mekanism:sulfur_dioxide_bucket",
            "mekanism:sulfur_trioxide_bucket",
            "mekanism:sulfuric_acid_bucket",
            "mekanism:hydrogen_chloride_bucket",
            "mekanism:hydrofluoric_acid_bucket",
            "mekanism:uranium_oxide_bucket",
            "mekanism:uranium_hexafluoride_bucket",
            "mekanism:ethene_bucket",
            "mekanism:sodium_bucket",
            "mekanism:superheated_sodium_bucket",
            "mekanism:brine_bucket",
            "mekanism:lithium_bucket",
            "mekanism:steam_bucket",
            "mekanism:heavy_water_bucket",
            "mekanism:nutritional_paste_bucket",
        ],
    ),
    (
        "minecraft:combat",
        &[
            "mekanism:atomic_disassembler",
            "mekanism:flamethrower",
            "mekanism:electric_bow",
            "mekanism:meka_tool",
            "mekanism:mekasuit_helmet",
            "mekanism:mekasuit_bodyarmor",
            "mekanism:mekasuit_pants",
            "mekanism:mekasuit_boots",
            "mekanism:free_runners_armored",
            "mekanism:jetpack_armored",
        ],
    ),
    (
        "minecraft:ingredients",
        &[
            "mekanism:module_base",
            "mekanism:alloy_infused",
            "mekanism:alloy_reinforced",
            "mekanism:alloy_atomic",
            "mekanism:basic_control_circuit",
            "mekanism:advanced_control_circuit",
            "mekanism:elite_control_circuit",
            "mekanism:ultimate_control_circuit",
            "mekanism:enriched_carbon",
            "mekanism:enriched_redstone",
            "mekanism:enriched_diamond",
            "mekanism:enriched_refined_obsidian",
            "mekanism:enriched_gold",
            "mekanism:enriched_tin",
            "mekanism:bio_fuel",
            "mekanism:block_bio_fuel",
            "mekanism:substrate",
            "mekanism:hdpe_pellet",
            "mekanism:hdpe_rod",
            "mekanism:hdpe_sheet",
            "mekanism:pellet_antimatter",
            "mekanism:pellet_plutonium",
            "mekanism:pellet_polonium",
            "mekanism:reprocessed_fissile_fragment",
            "mekanism:electrolytic_core",
            "mekanism:teleportation_core",
            "mekanism:enriched_iron",
            "mekanism:sawdust",
            "mekanism:salt",
            "mekanism:dye_base",
            "mekanism:fluorite_gem",
            "mekanism:dust_fluorite",
            "mekanism:yellow_cake_uranium",
            "mekanism:dirty_netherite_scrap",
            "mekanism:dust_netherite",
            "mekanism:dust_charcoal",
            "mekanism:dust_coal",
            "mekanism:dust_sulfur",
            "mekanism:dust_bronze",
            "mekanism:dust_lapis_lazuli",
            "mekanism:dust_quartz",
            "mekanism:dust_emerald",
            "mekanism:dust_diamond",
            "mekanism:dust_steel",
            "mekanism:dust_obsidian",
            "mekanism:dust_refined_obsidian",
            "mekanism:nugget_bronze",
            "mekanism:nugget_steel",
            "mekanism:nugget_refined_obsidian",
            "mekanism:nugget_refined_glowstone",
            "mekanism:ingot_bronze",
            "mekanism:ingot_steel",
            "mekanism:ingot_refined_obsidian",
            "mekanism:ingot_refined_glowstone",
            "mekanism:shard_iron",
            "mekanism:shard_gold",
            "mekanism:shard_osmium",
            "mekanism:shard_copper",
            "mekanism:shard_tin",
            "mekanism:shard_lead",
            "mekanism:shard_uranium",
            "mekanism:crystal_iron",
            "mekanism:crystal_gold",
            "mekanism:crystal_osmium",
            "mekanism:crystal_copper",
            "mekanism:crystal_tin",
            "mekanism:crystal_lead",
            "mekanism:crystal_uranium",
            "mekanism:dust_iron",
            "mekanism:dust_gold",
            "mekanism:dust_osmium",
            "mekanism:dust_copper",
            "mekanism:dust_tin",
            "mekanism:dust_lead",
            "mekanism:dust_uranium",
            "mekanism:dirty_dust_iron",
            "mekanism:dirty_dust_gold",
            "mekanism:dirty_dust_osmium",
            "mekanism:dirty_dust_copper",
            "mekanism:dirty_dust_tin",
            "mekanism:dirty_dust_lead",
            "mekanism:dirty_dust_uranium",
            "mekanism:clump_iron",
            "mekanism:clump_gold",
            "mekanism:clump_osmium",
            "mekanism:clump_copper",
            "mekanism:clump_tin",
            "mekanism:clump_lead",
            "mekanism:clump_uranium",
            "mekanism:ingot_osmium",
            "mekanism:ingot_tin",
            "mekanism:ingot_lead",
            "mekanism:ingot_uranium",
            "mekanism:raw_osmium",
            "mekanism:raw_tin",
            "mekanism:raw_lead",
            "mekanism:raw_uranium",
            "mekanism:nugget_osmium",
            "mekanism:nugget_tin",
            "mekanism:nugget_lead",
            "mekanism:nugget_uranium",
        ],
    ),
];

/// As pilhas (no formato do Horizon) que a aba `tab` ganha no fim, na ordem do original: os itens da lista e, no lugar de `mekanism:upgrade`, uma pilha por tipo.
pub fn additions(tab: &str) -> Vec<String> {
    let mut stacks = Vec::new();
    for (id, items) in VANILLA_TABS {
        if id != tab {
            continue;
        }
        for item in items {
            if *item == "mekanism:upgrade" {
                stacks.extend(UPGRADE_TYPES.iter().map(|kind| upgrade_stack(kind)));
            } else {
                stacks.push(plain_stack(item));
                if let Some(energy) = filled_energy(item) {
                    stacks.push(filled_stack(item, energy));
                }
            }
        }
    }
    stacks
}

/// A aba do mod: o item `mekanism:upgrade` vira as pilhas dos tipos, no mesmo lugar (`ICustomCreativeTabContents.addItems`; o item sem tipo não entra), e cada item que guarda
/// energia ganha o cheio logo depois do vazio ([`FILLED_VARIANTS`]). Devolve se mudou.
pub fn edit_mod_tab(items: &mut Vec<String>) -> bool {
    let upgrade = plain_stack("mekanism:upgrade");
    let mut edited = Vec::with_capacity(items.len() + FILLED_VARIANTS.len() + UPGRADE_TYPES.len());
    for item in items.iter() {
        if *item == upgrade {
            edited.extend(UPGRADE_TYPES.iter().map(|kind| upgrade_stack(kind)));
            continue;
        }
        edited.push(item.clone());
        if let Some(energy) = FILLED_VARIANTS
            .iter()
            .find(|(id, _)| plain_stack(id) == *item)
            .map(|(_, energy)| *energy)
        {
            let id = item
                .trim_start_matches("{count:1,id:\"")
                .trim_end_matches("\"}");
            edited.push(filled_stack(id, energy));
        }
    }
    let changed = edited != *items;
    *items = edited;
    changed
}

/// Uma aba do jogo (`id`) com os itens que o Horizon escreveu (`items`): o fim dela ganha o que o original acrescenta (a aba do mod troca o upgrade pelas pilhas).
/// Devolve se mudou.
pub fn edit_tab(id: &str, items: &mut Vec<String>) -> bool {
    if id == MEKANISM_TAB {
        return edit_mod_tab(items);
    }
    let added = additions(id);
    if added.is_empty() {
        return false;
    }
    items.extend(added);
    true
}
