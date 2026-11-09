// O que o Mekanism Generators acrescenta às abas do vanilla (`GeneratorsCreativeTabs.addToExistingTabs`, o evento `BuildCreativeModeTabContentsEvent`): os itens ao fim de cada
// aba, na ordem da lista dele. GERADO por `dev/referencia/generators_conteudo.py` (a ordem vem do original em jogo, `rastros/blocos_generators.jsonl`). O Horizon só deixa o mod
// declarar a aba própria (`creative_tab/*.json`) e não conhece as abas do vanilla por mod: a injeção `creative_tabs` põe a lista no fim de cada aba do vanilla.

/// Os itens do Generators que o original acrescenta a cada aba do vanilla, na ordem da lista do original.
pub const VANILLA_TABS: [(&str, &[&str]); 4] = [
    (
        "minecraft:functional_blocks",
        &[
            "mekanismgenerators:heat_generator",
            "mekanismgenerators:solar_generator",
            "mekanismgenerators:advanced_solar_generator",
            "mekanismgenerators:wind_generator",
            "mekanismgenerators:bio_generator",
            "mekanismgenerators:gas_burning_generator",
        ],
    ),
    (
        "minecraft:redstone_blocks",
        &[
            "mekanismgenerators:heat_generator",
            "mekanismgenerators:solar_generator",
            "mekanismgenerators:gas_burning_generator",
            "mekanismgenerators:bio_generator",
            "mekanismgenerators:advanced_solar_generator",
            "mekanismgenerators:wind_generator",
            "mekanismgenerators:turbine_valve",
        ],
    ),
    (
        "minecraft:tools_and_utilities",
        &[
            "mekanismgenerators:bioethanol_bucket",
            "mekanismgenerators:deuterium_bucket",
            "mekanismgenerators:fusion_fuel_bucket",
            "mekanismgenerators:tritium_bucket",
        ],
    ),
    (
        "minecraft:ingredients",
        &[
            "mekanismgenerators:hohlraum",
            "mekanismgenerators:solar_panel",
        ],
    ),
];

/// Acrescenta ao fim das `stacks` de uma aba do vanilla (`id`) o que o Generators põe nela, como a pilha que o Horizon escreve para um id. Devolve se mudou.
pub fn edit_tab(id: &str, stacks: &mut Vec<String>) -> bool {
    let Some((_, ids)) = VANILLA_TABS.iter().find(|(tab, _)| *tab == id) else {
        return false;
    };
    stacks.extend(ids.iter().map(|id| format!("{{count:1,id:\"{id}\"}}")));
    true
}
