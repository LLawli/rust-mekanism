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
            "mekanism:enrichment_chamber",
            "mekanism:crusher",
            "mekanism:metallurgic_infuser",
            "mekanism:energized_smelter",
            "mekanism:basic_universal_cable",
            "mekanism:advanced_universal_cable",
            "mekanism:elite_universal_cable",
            "mekanism:ultimate_universal_cable",
        ],
    ),
    (
        "minecraft:redstone_blocks",
        &[
            "mekanism:enrichment_chamber",
            "mekanism:crusher",
            "mekanism:metallurgic_infuser",
            "mekanism:energized_smelter",
            "mekanism:basic_universal_cable",
            "mekanism:advanced_universal_cable",
            "mekanism:elite_universal_cable",
            "mekanism:ultimate_universal_cable",
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
            "mekanism:upgrade",
            "mekanism:basic_tier_installer",
            "mekanism:advanced_tier_installer",
            "mekanism:elite_tier_installer",
            "mekanism:ultimate_tier_installer",
            // Depois dos tanques (que o port ainda não tem): `addToDisplay(MekanismFluids.FLUIDS, event)` (`MekanismCreativeTabs.java:136`) põe os baldes dos 18
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
            }
        }
    }
    stacks
}

/// A aba do mod: o item `mekanism:upgrade` vira as pilhas dos tipos, no mesmo lugar (`ICustomCreativeTabContents.addItems`; o item sem tipo não entra).
/// Devolve se mudou.
pub fn edit_mod_tab(items: &mut Vec<String>) -> bool {
    let plain = plain_stack("mekanism:upgrade");
    let Some(at) = items.iter().position(|item| *item == plain) else {
        return false;
    };
    items.splice(
        at..=at,
        UPGRADE_TYPES.iter().map(|kind| upgrade_stack(kind)),
    );
    true
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
