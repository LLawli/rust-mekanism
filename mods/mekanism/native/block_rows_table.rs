// GERADO por `dev/referencia/blocos_conteudo.py` a partir de `rastros/blocos.jsonl` (`testes/BlocosTraceTest.java`, o original em jogo). Não edite à mão.
//
// O que o original dá à linha de jogo de cada estado dos blocos de máquina e dos blocos avulsos com tile que o Horizon não deixa o mod dizer em `properties`:
// a luz do estado ativo (`AttributeStateActive`: `Attributes.ACTIVE_LIGHT` 8, `ACTIVE_MELT_LIGHT` 13, `ACTIVE_FULL_LIGHT` 15), a reação ao pistão (`BlockMekanism.
// getPistonPushReaction`: `IMMOVEABLE` com bloco de entidade) e a oclusão (`isSolidRender`, `canOcclude`, `getLightDampening`, `propagatesSkylightDown`: o cubo
// cheio do gabarito não vale para uma forma menor). A injeção `active_light` aplica a tabela em cada linha, antes das `properties` declaradas.

/// Uma linha da tabela: o bloco e o que a linha de jogo dele leva.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Facts {
    pub id: &'static str,
    /// A luz do estado com `active=true`, quando o original acende o bloco (0: o bloco não tem `active` com luz).
    pub active_light: u8,
    pub push: &'static str,
    pub can_occlude: bool,
    pub solid_render: bool,
    pub dampening: u8,
    pub skylight_down: bool,
    /// A oclusão do estado com água ou lava dentro (`fluid_logged`), que o fluido muda.
    pub wet_dampening: u8,
    pub wet_skylight_down: bool,
    /// A resistência a explosões que o arquivo do Horizon não aceita (negativa: `AttributeCustomResistance`).
    pub explosion: Option<f32>,
}

/// A luz da lava dentro do bloco (`IStateFluidLoggable.getFluidLightLevel`).
pub const LAVA_LIGHT: u8 = 15;

pub const BLOCKS: [Facts; 84] = [
    Facts { id: "mekanism:antiprotonic_nucleosynthesizer", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:chemical_crystallizer", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:chemical_dissolution_chamber", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:chemical_infuser", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:chemical_injection_chamber", active_light: 8, push: "IMMOVEABLE", can_occlude: true, solid_render: true, dampening: 15, skylight_down: false, wet_dampening: 15, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:chemical_oxidizer", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:chemical_washer", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:combiner", active_light: 8, push: "IMMOVEABLE", can_occlude: true, solid_render: true, dampening: 15, skylight_down: false, wet_dampening: 15, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:digital_miner", active_light: 0, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:dimensional_stabilizer", active_light: 8, push: "IMMOVEABLE", can_occlude: true, solid_render: true, dampening: 15, skylight_down: false, wet_dampening: 15, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:electric_pump", active_light: 0, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:electrolytic_separator", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:fluidic_plenisher", active_light: 0, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:formulaic_assemblicator", active_light: 8, push: "IMMOVEABLE", can_occlude: true, solid_render: true, dampening: 15, skylight_down: false, wet_dampening: 15, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:fuelwood_heater", active_light: 13, push: "IMMOVEABLE", can_occlude: true, solid_render: true, dampening: 15, skylight_down: false, wet_dampening: 15, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:isotopic_centrifuge", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:laser", active_light: 0, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:laser_amplifier", active_light: 0, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:laser_tractor_beam", active_light: 0, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:logistical_sorter", active_light: 0, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:modification_station", active_light: 0, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:nutritional_liquifier", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 1, skylight_down: false, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:oredictionificator", active_light: 0, push: "IMMOVEABLE", can_occlude: true, solid_render: true, dampening: 15, skylight_down: false, wet_dampening: 15, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:osmium_compressor", active_light: 8, push: "IMMOVEABLE", can_occlude: true, solid_render: true, dampening: 15, skylight_down: false, wet_dampening: 15, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:painting_machine", active_light: 8, push: "IMMOVEABLE", can_occlude: true, solid_render: true, dampening: 15, skylight_down: false, wet_dampening: 15, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:pigment_extractor", active_light: 8, push: "IMMOVEABLE", can_occlude: true, solid_render: true, dampening: 15, skylight_down: false, wet_dampening: 15, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:pigment_mixer", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 0, wet_skylight_down: true, explosion: None },
    Facts { id: "mekanism:precision_sawmill", active_light: 8, push: "IMMOVEABLE", can_occlude: true, solid_render: true, dampening: 15, skylight_down: false, wet_dampening: 15, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:pressurized_reaction_chamber", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:purification_chamber", active_light: 8, push: "IMMOVEABLE", can_occlude: true, solid_render: true, dampening: 15, skylight_down: false, wet_dampening: 15, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:qio_dashboard", active_light: 0, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:qio_drive_array", active_light: 0, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:qio_exporter", active_light: 0, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:qio_importer", active_light: 0, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:qio_redstone_adapter", active_light: 0, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:quantum_entangloporter", active_light: 0, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:resistive_heater", active_light: 13, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:rotary_condensentrator", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:security_desk", active_light: 0, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: Some(-1.0) },
    Facts { id: "mekanism:seismic_vibrator", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:solar_neutron_activator", active_light: 0, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:teleporter", active_light: 0, push: "IMMOVEABLE", can_occlude: true, solid_render: true, dampening: 15, skylight_down: false, wet_dampening: 15, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:chargepad", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:industrial_alarm", active_light: 15, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:radioactive_waste_barrel", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:personal_barrel", active_light: 0, push: "IMMOVEABLE", can_occlude: true, solid_render: true, dampening: 15, skylight_down: false, wet_dampening: 15, wet_skylight_down: false, explosion: Some(-1.0) },
    Facts { id: "mekanism:personal_chest", active_light: 0, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: Some(-1.0) },
    Facts { id: "mekanism:cardboard_box", active_light: 0, push: "IMMOVEABLE", can_occlude: true, solid_render: true, dampening: 15, skylight_down: false, wet_dampening: 15, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:basic_combining_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:basic_compressing_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:basic_crushing_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:basic_enriching_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:basic_infusing_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:basic_injecting_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:basic_purifying_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:basic_sawing_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:basic_smelting_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:advanced_combining_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:advanced_compressing_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:advanced_crushing_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:advanced_enriching_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:advanced_infusing_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:advanced_injecting_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:advanced_purifying_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:advanced_sawing_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:advanced_smelting_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:elite_combining_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:elite_compressing_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:elite_crushing_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:elite_enriching_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:elite_infusing_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:elite_injecting_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:elite_purifying_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:elite_sawing_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:elite_smelting_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:ultimate_combining_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:ultimate_compressing_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:ultimate_crushing_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:ultimate_enriching_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:ultimate_infusing_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:ultimate_injecting_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:ultimate_purifying_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:ultimate_sawing_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
    Facts { id: "mekanism:ultimate_smelting_factory", active_light: 8, push: "IMMOVEABLE", can_occlude: false, solid_render: false, dampening: 0, skylight_down: true, wet_dampening: 1, wet_skylight_down: false, explosion: None },
];

/// A linha da tabela de um bloco do Mekanism, pelo id.
pub fn facts(id: &str) -> Option<&'static Facts> {
    BLOCKS.iter().find(|facts| facts.id == id)
}

/// A luz do estado de `facts` com as propriedades `combo` (`active`, `fluid_logged`): a maior entre a do `active`, a da lava e a que o bloco já tinha.
pub fn light(facts: &Facts, combo: &[(String, String)], base: u8) -> u8 {
    let has = |name: &str, value: &str| combo.iter().any(|(k, v)| k == name && v == value);
    let mut light = base;
    if facts.active_light > 0 && has("active", "true") {
        light = light.max(facts.active_light);
    }
    if has("fluid_logged", "lava") {
        light = light.max(LAVA_LIGHT);
    }
    light
}
