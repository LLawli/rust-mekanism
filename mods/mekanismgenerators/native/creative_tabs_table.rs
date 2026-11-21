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
    let filled = add_filled_tanks(stacks);
    let Some((_, ids)) = VANILLA_TABS.iter().find(|(tab, _)| *tab == id) else {
        return filled;
    };
    stacks.extend(ids.iter().map(|id| format!("{{count:1,id:\"{id}\"}}")));
    true
}

/// Os fluidos do Generators, na ordem em que `GeneratorsFluids` os registra (`rastros/fluids.jsonl`).
pub const FLUIDS: [&str; 4] = ["mekanismgenerators:bioethanol", "mekanismgenerators:deuterium", "mekanismgenerators:fusion_fuel", "mekanismgenerators:tritium"];

/// O começo da pilha de um Fluid Tank criativo cheio, como o Mekanism a escreve na aba (`MekanismCreativeTabs.addFilledTanks`).
const FILLED_FLUID_TANK: &str = "{count:1,id:\"mekanism:creative_fluid_tank\",components:{\"mekanism:fluids\":";

/// `addFilledTanks` do Mekanism percorre o registro inteiro de fluidos: com o Generators instalado, os quatro fluidos dele ganham um tanque cheio logo depois dos do
/// Mekanism (a ordem do registro é a dos mods), em toda aba que tem os tanques cheios. Devolve se mudou.
pub fn add_filled_tanks(stacks: &mut Vec<String>) -> bool {
    let Some(last) = stacks.iter().rposition(|stack| stack.starts_with(FILLED_FLUID_TANK)) else {
        return false;
    };
    let ours: Vec<String> = FLUIDS
        .iter()
        .map(|fluid| format!("{FILLED_FLUID_TANK}{{containers:[{{amount:{}L,type:{{id:\"{fluid}\"}}}}]}}}}}}", i64::MAX))
        .collect();
    if stacks.iter().any(|stack| *stack == ours[0]) {
        return false;
    }
    stacks.splice(last + 1..last + 1, ours);
    true
}
