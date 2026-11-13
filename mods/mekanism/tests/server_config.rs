//! Os arquivos de configuração de servidor do Mekanism (`Mekanism/general.toml`, `machine-usage.toml` e `tiers.toml`, `Type.SYNCED`): o texto que o port
//! escreve contra o que o original escreveu sozinho (`rastros/mekanism_*.toml`), a leitura e a correção de valores errados contra o que o NeoForge fez
//! com os mesmos arquivos (`rastros/config_correcao/entrada` e `saida`, escritos por `oficial_config.sh`), e o efeito de cada chave portada.
mod common;

use machinery_basis_core::cached_recipe::{ItemIngredient, ItemOutput, ItemToItemRecipe};
use machinery_basis_core::client_config::{Loaded, Setting, Spec};
use machinery_basis_core::containers::AutomationType;
use machinery_basis_core::item_handler::NoNeighbours;
use machinery_basis_core::settings::MachineSettings;
use machinery_basis_core::tile::ProgressMachine;
use machinery_basis_core::transaction::Transaction;
use mekanism_rules::{
    CRUSHER_USAGE, ENRICHMENT_USAGE, GENERAL_CONFIG, INFUSER_USAGE, SMELTER_USAGE, TIERS_CONFIG,
    UNIVERSAL_CABLES, USAGE_CONFIG, Usage, cable_capacities, cable_tier, electric_config,
    machine_settings,
};
use std::path::{Path, PathBuf};
use std::rc::Rc;

fn rastros() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../dev/referencia/rastros")
}

/// O arquivo que o cliente oficial escreveu sozinho.
fn reference(name: &str) -> String {
    std::fs::read_to_string(rastros().join(format!("mekanism_{name}.toml"))).unwrap()
}

/// Um arquivo errado de propósito (`entrada`) e como o NeoForge o deixou depois de ler (`saida`).
fn crafted(kind: &str, file: &str) -> String {
    std::fs::read_to_string(rastros().join("config_correcao").join(kind).join(file)).unwrap()
}

/// O comentário e a linha de uma chave em `section` (vazia para as de fora de seção).
fn block(text: &str, section: &str, key: &str) -> String {
    let mut current = String::new();
    let lines: Vec<&str> = text.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if let Some(name) = trimmed
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            current = name.to_owned();
        } else if current == section && trimmed.starts_with(&format!("{key} = ")) {
            let mut first = index;
            while first > 0 && lines[first - 1].trim_start().starts_with('#') {
                first -= 1;
            }
            return lines[first..=index].join("\n");
        }
    }
    panic!("{section}.{key} não está no arquivo");
}

/// O comentário e o cabeçalho de uma seção.
fn header(text: &str, section: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let at = lines
        .iter()
        .position(|line| line.trim() == format!("[{section}]"))
        .unwrap_or_else(|| panic!("[{section}] não está no arquivo"));
    let mut first = at;
    while first > 0 && lines[first - 1].trim_start().starts_with('#') {
        first -= 1;
    }
    lines[first..=at].join("\n")
}

/// Cada chave da especificação e o valor lido, em texto.
fn values(spec: &Spec, loaded: &Loaded) -> Vec<String> {
    let mut out = Vec::new();
    for section in spec.sections {
        for entry in section.entries {
            let value = match entry.setting {
                Setting::Bool { .. } => format!("{:?}", loaded.bool(spec, section.name, entry.key)),
                Setting::Float { .. } => {
                    format!("{:?}", loaded.float(spec, section.name, entry.key))
                }
                Setting::Int { .. } | Setting::IntWhere { .. } => {
                    format!("{:?}", loaded.int(spec, section.name, entry.key))
                }
                Setting::Long { .. } => format!("{:?}", loaded.long(spec, section.name, entry.key)),
            };
            out.push(format!("{}.{} = {value}", section.name, entry.key));
        }
    }
    out
}

const FILES: [(&str, &str, &Spec); 3] = [
    ("general", "general.toml", &GENERAL_CONFIG),
    ("machine_usage", "machine-usage.toml", &USAGE_CONFIG),
    ("tiers", "tiers.toml", &TIERS_CONFIG),
];

#[test]
fn the_default_text_of_each_key_is_the_one_the_original_wrote() {
    for (name, _, spec) in FILES {
        let original = reference(name);
        let text = spec.default_text();
        for section in spec.sections {
            if !section.name.is_empty() {
                assert_eq!(
                    header(&text, section.name),
                    header(&original, section.name),
                    "{name} [{}]",
                    section.name
                );
            }
            for entry in section.entries {
                assert_eq!(
                    block(&text, section.name, entry.key),
                    block(&original, section.name, entry.key),
                    "{name} {}.{}",
                    section.name,
                    entry.key
                );
            }
        }
    }
}

/// A parte de `[transmitters]` do texto, do comentário da seção até o fim do arquivo.
fn transmitters_part(text: &str) -> String {
    let start = text
        .find("#Settings for configuring Transmitters\n")
        .expect("a seção [transmitters]");
    text[start..].to_owned()
}

/// `[transmitters.energy]`, `.fluid`, `.chemical`, `.items` e `.heat` byte a byte: o texto, a ordem das chaves e das seções, as linhas em branco e as
/// tabulações do `tiers.toml` que o original escreveu sozinho (o teste de cada chave não vê uma linha em branco a menos nem uma seção fora de ordem).
#[test]
fn the_transmitter_sections_of_tiers_are_the_original_byte_for_byte() {
    let original = reference("tiers");
    let port = TIERS_CONFIG.default_text();
    assert_eq!(transmitters_part(&port), transmitters_part(&original));
    for section in ["energy", "fluid", "chemical", "items", "heat"] {
        let name = format!("transmitters.{section}");
        assert_eq!(header(&port, &name), header(&original, &name), "{name}");
    }
}

#[test]
fn the_files_the_original_wrote_read_clean_with_the_defaults() {
    for (name, _, spec) in FILES {
        let loaded = spec.load(&reference(name));
        assert!(!loaded.corrected, "{name}");
        assert_eq!(
            values(spec, &loaded),
            values(spec, &spec.defaults()),
            "{name}"
        );
        assert!(!spec.load(&spec.default_text()).corrected, "{name}");
    }
    assert_eq!(
        machine_settings(&GENERAL_CONFIG.load(&reference("general"))),
        MachineSettings::DEFAULT
    );
    assert_eq!(
        Usage::of(&USAGE_CONFIG.load(&reference("machine_usage"))),
        Usage::DEFAULT
    );
}

/// Os arquivos de `entrada` têm valores fora da faixa, de outro tipo, chaves de fora, uma seção de fora e (em `tiers.toml`) nenhum comentário. O NeoForge
/// deixou os de `saida`: cada chave portada tem de ter o mesmo valor e o mesmo bloco de texto.
#[test]
fn a_wrong_file_is_corrected_the_way_neoforge_corrected_it() {
    for (_, file, spec) in FILES {
        let entrada = crafted("entrada", file);
        let saida = crafted("saida", file);
        let loaded = spec.load(&entrada);
        assert!(loaded.corrected, "{file}");
        assert_eq!(
            values(spec, &loaded),
            values(spec, &spec.load(&saida)),
            "{file}"
        );
        let written = spec.rewrite(&entrada, &loaded);
        for section in spec.sections {
            for entry in section.entries {
                assert_eq!(
                    block(&written, section.name, entry.key),
                    block(&saida, section.name, entry.key),
                    "{file} {}.{}",
                    section.name,
                    entry.key
                );
            }
        }
        assert!(
            !spec.load(&written).corrected,
            "{file}: o arquivo escrito lê limpo"
        );
        // A especificação do port é parcial: o que ela não conhece o NeoForge apaga e o port guarda.
        if file == "general.toml" {
            assert!(written.contains("bogus = 1") && written.contains("[bogus_section]"));
        }
    }
}

/// As chaves de fora de seção de um arquivo, na ordem em que aparecem (a seção sem nome vai até o primeiro `[`).
fn root_keys(text: &str) -> Vec<String> {
    text.lines()
        .take_while(|line| !line.trim_start().starts_with('['))
        .filter(|line| !line.trim_start().starts_with('#') && line.contains(" = "))
        .map(|line| line.split(" = ").next().unwrap().trim().to_owned())
        .collect()
}

/// Um `general.toml` de antes da chave `transmitterAlloyUpgrade` (`rastros/config_correcao/entrada_antiga`, o arquivo de uma versão velha): o NeoForge acrescenta a chave que falta no fim
/// das chaves de fora de seção (depois da última, `maxSolarNeutronActivatorRate`, e antes da primeira seção), não no lugar em que a especificação a declara (`saida_antiga`, escrito por
/// `oficial_config.sh`). O port faz o mesmo, na ordem de todas as chaves do arquivo, as dele e as que só o original conhece.
#[test]
fn a_key_missing_from_an_old_file_goes_to_the_end_of_the_root_keys_as_neoforge_put_it() {
    let entrada = crafted("entrada_antiga", "general.toml");
    let saida = crafted("saida_antiga", "general.toml");
    assert!(!root_keys(&entrada).contains(&"transmitterAlloyUpgrade".to_owned()));
    let loaded = GENERAL_CONFIG.load(&entrada);
    assert_eq!(
        machine_settings(&loaded),
        MachineSettings {
            block_deactivation_delay: 60,
            max_upgrade_multiplier: 10,
            transmitter_alloy_upgrade: true
        },
        "a chave que falta vale o padrão"
    );
    let written = GENERAL_CONFIG.rewrite(&entrada, &loaded);
    assert_eq!(root_keys(&written), root_keys(&saida));
    assert_eq!(root_keys(&saida).last().unwrap(), "transmitterAlloyUpgrade");
    assert_eq!(
        block(&written, "", "transmitterAlloyUpgrade"),
        block(&saida, "", "transmitterAlloyUpgrade")
    );
}

#[test]
fn each_corrected_key_has_the_value_the_original_gave_it() {
    let general = GENERAL_CONFIG.load(&crafted("entrada", "general.toml"));
    assert_eq!(
        machine_settings(&general),
        MachineSettings {
            block_deactivation_delay: 1_200,
            max_upgrade_multiplier: 1,
            transmitter_alloy_upgrade: true
        }
    );
    let usage = Usage::of(&USAGE_CONFIG.load(&crafted("entrada", "machine-usage.toml")));
    assert_eq!(
        usage,
        Usage {
            enrichment_chamber: 0,
            crusher: i32::MAX,
            metallurgic_infuser: INFUSER_USAGE,
            energized_smelter: SMELTER_USAGE,
        }
    );
    // `basicCapacity = 0` sobe ao mínimo, o texto vai ao padrão e o `long` de 5 bilhões cabe.
    let tiers = TIERS_CONFIG.load(&crafted("entrada", "tiers.toml"));
    assert_eq!(
        cable_capacities(&tiers),
        [1, 128_000, 1_024_000, 5_000_000_000]
    );
}

/// `transmitterAlloyUpgrade = false` no arquivo desliga o upgrade pela liga (`ItemAlloy.useOn` lê a chave a cada clique); o texto da chave é o do original.
#[test]
fn the_alloy_upgrade_option_follows_the_file() {
    let off = reference("general").replace(
        "transmitterAlloyUpgrade = true",
        "transmitterAlloyUpgrade = false",
    );
    assert!(!machine_settings(&GENERAL_CONFIG.load(&off)).transmitter_alloy_upgrade);
    assert!(
        machine_settings(&GENERAL_CONFIG.load(&reference("general"))).transmitter_alloy_upgrade
    );
    // Uma chave que não é booleana volta ao padrão, como o NeoForge corrige.
    let wrong = reference("general").replace(
        "transmitterAlloyUpgrade = true",
        "transmitterAlloyUpgrade = 7",
    );
    let loaded = GENERAL_CONFIG.load(&wrong);
    assert!(loaded.corrected);
    assert!(machine_settings(&loaded).transmitter_alloy_upgrade);
}

#[test]
fn the_cables_follow_the_order_of_the_tiers() {
    assert_eq!(cable_tier("mekanism:basic_universal_cable"), Some(0));
    assert_eq!(cable_tier("mekanism:ultimate_universal_cable"), Some(3));
    assert_eq!(cable_tier("mekanism:crusher"), None);
    let defaults: Vec<i64> = UNIVERSAL_CABLES
        .iter()
        .map(|(_, capacity)| i64::from(*capacity))
        .collect();
    assert_eq!(
        cable_capacities(&TIERS_CONFIG.defaults()).to_vec(),
        defaults
    );
    let loaded = TIERS_CONFIG.load("[transmitters.energy]\n\tadvancedCapacity = 250000\n");
    assert_eq!(
        cable_capacities(&loaded),
        [8_000, 250_000, 1_024_000, 8_192_000]
    );
}

#[test]
fn the_pipes_follow_the_order_of_the_tiers_and_the_file_changes_capacity_and_rate() {
    use machinery_basis_core::transmission::mechanical_pipe::PipeTier;
    use mekanism_rules::{MECHANICAL_PIPES, pipe_capacities, pipe_rates, pipe_tier};
    assert_eq!(pipe_tier("mekanism:basic_mechanical_pipe"), Some(0));
    assert_eq!(pipe_tier("mekanism:ultimate_mechanical_pipe"), Some(3));
    assert_eq!(pipe_tier("mekanism:basic_universal_cable"), None);
    assert_eq!(MECHANICAL_PIPES.len(), PipeTier::ALL.len());
    // Os padrões do arquivo são os de `PipeTier.getBaseCapacity` e `getBaseTransferRate`, os que a lib já tinha.
    let defaults = TIERS_CONFIG.defaults();
    assert_eq!(
        pipe_capacities(&defaults).to_vec(),
        PipeTier::ALL.map(PipeTier::capacity).to_vec()
    );
    assert_eq!(
        pipe_rates(&defaults).to_vec(),
        PipeTier::ALL.map(PipeTier::transfer_rate).to_vec()
    );
    let loaded = TIERS_CONFIG
        .load("[transmitters.fluid]\n\tadvancedCapacity = 50000\n\telitePullAmount = 12000\n\tbasicPullAmount = 0\n");
    assert_eq!(pipe_capacities(&loaded), [2_000, 50_000, 32_000, 128_000]);
    assert_eq!(pipe_rates(&loaded), [1, 1_000, 12_000, 32_000]);
}

#[test]
fn the_conductors_follow_the_order_of_the_tiers_and_the_file_changes_the_three_values() {
    use machinery_basis_core::transmission::thermodynamic_conductor::ConductorTier;
    use mekanism_rules::{THERMODYNAMIC_CONDUCTORS, conductor_tier, conductor_values};
    assert_eq!(
        conductor_tier("mekanism:basic_thermodynamic_conductor"),
        Some(0)
    );
    assert_eq!(
        conductor_tier("mekanism:ultimate_thermodynamic_conductor"),
        Some(3)
    );
    assert_eq!(conductor_tier("mekanism:basic_mechanical_pipe"), None);
    assert_eq!(THERMODYNAMIC_CONDUCTORS.len(), ConductorTier::ALL.len());
    // Os padrões do arquivo são os de `ConductorTier` (`baseConduction`, `baseHeatCapacity`, `baseConductionInsulation`), os que a lib já tinha.
    let defaults = conductor_values(&TIERS_CONFIG.defaults());
    for (tier, values) in ConductorTier::ALL.into_iter().zip(defaults) {
        assert_eq!(values, tier.base_values(), "{tier:?}");
    }
    // Cada chave muda só o próprio número do próprio nível; abaixo do mínimo sobe ao mínimo (`1` para a condução e a capacidade, `0` para a isolação).
    let loaded = TIERS_CONFIG.load(
        "[transmitters.heat]\n\tadvancedInverseConduction = 12.5\n\teliteHeatCapacity = 4.0\n\tultimateInsulation = 7.0\n\tbasicInverseConduction = 0.2\n\tbasicInsulation = -3.0\n",
    );
    let values = conductor_values(&loaded);
    assert_eq!(
        values[0].inverse_conduction, 1.0,
        "0,2 está abaixo do mínimo: sobe a 1"
    );
    assert_eq!(
        values[0].insulation, 0.0,
        "-3 está abaixo do mínimo: sobe a 0"
    );
    assert_eq!(values[1].inverse_conduction, 12.5);
    assert_eq!(values[1].insulation, 400.0);
    assert_eq!(values[2].heat_capacity, 4.0);
    assert_eq!(values[2].inverse_conduction, 5.0);
    assert_eq!(values[3].insulation, 7.0);
    assert_eq!(values[3].heat_capacity, 1.0);
}

#[test]
fn the_tubes_follow_the_order_of_the_tiers_and_the_file_changes_capacity_and_rate() {
    use machinery_basis_core::transmission::pressurized_tube::TubeTier;
    use mekanism_rules::{PRESSURIZED_TUBES, tube_capacities, tube_rates, tube_tier};
    assert_eq!(tube_tier("mekanism:basic_pressurized_tube"), Some(0));
    assert_eq!(tube_tier("mekanism:ultimate_pressurized_tube"), Some(3));
    assert_eq!(tube_tier("mekanism:basic_mechanical_pipe"), None);
    assert_eq!(PRESSURIZED_TUBES.len(), TubeTier::ALL.len());
    // Os padrões do arquivo são os de `TubeTier.getBaseCapacity` e `getBaseTransferRate`, os que a lib já tinha.
    let defaults = TIERS_CONFIG.defaults();
    assert_eq!(
        tube_capacities(&defaults).to_vec(),
        TubeTier::ALL.map(TubeTier::capacity).to_vec()
    );
    assert_eq!(
        tube_rates(&defaults).to_vec(),
        TubeTier::ALL.map(TubeTier::transfer_rate).to_vec()
    );
    // Antes de o arquivo ser lido, `ServerConfig` tem os padrões de `TubeTier`.
    assert_eq!(
        mekanism_rules::ServerConfig::DEFAULT.tubes.to_vec(),
        TubeTier::ALL.map(TubeTier::capacity).to_vec()
    );
    assert_eq!(
        mekanism_rules::ServerConfig::DEFAULT.tube_rates.to_vec(),
        TubeTier::ALL.map(TubeTier::transfer_rate).to_vec()
    );
    let loaded = TIERS_CONFIG.load(
        "[transmitters.chemical]\n\tadvancedCapacity = 50000\n\telitePullAmount = 12000\n\tbasicPullAmount = 0\n",
    );
    assert_eq!(
        tube_capacities(&loaded),
        [4_000, 50_000, 256_000, 1_024_000]
    );
    assert_eq!(tube_rates(&loaded), [1, 2_000, 12_000, 256_000]);
}

// ---- O efeito de cada chave ----

fn machine_with(usage: i32) -> ProgressMachine {
    let recipe = Rc::new(ItemToItemRecipe {
        id: "mekanism:test/clay_ball".to_owned(),
        input: ItemIngredient::item("clay"),
        input_count: 1,
        output: ItemOutput {
            resource: common::item("clay_ball"),
            count: 4,
        },
    });
    let mut config = electric_config(
        usage,
        ItemToItemRecipe::cache_of(vec![recipe]),
        0,
        common::upgrade_data(),
    );
    config.energy_conversion = common::energy_conversions();
    ProgressMachine::electric(config)
}

/// A energia que a máquina aceita de uma vez, que é a capacidade dela.
fn fill(machine: &ProgressMachine) -> i32 {
    let root = Transaction::open_root();
    let accepted = machine
        .energy()
        .insert(i32::MAX, &root, AutomationType::External);
    root.commit();
    accepted
}

#[test]
fn the_usage_sets_the_energy_per_tick_and_the_buffer_of_four_ticks() {
    for (key, default) in [
        ("enrichmentChamber", ENRICHMENT_USAGE),
        ("crusher", CRUSHER_USAGE),
        ("metallurgicInfuser", INFUSER_USAGE),
        ("energizedSmelter", SMELTER_USAGE),
    ] {
        let usage = Usage::of(&USAGE_CONFIG.load(&format!("{key} = 20\n")));
        let value = match key {
            "enrichmentChamber" => usage.enrichment_chamber,
            "crusher" => usage.crusher,
            "metallurgicInfuser" => usage.metallurgic_infuser,
            _ => usage.energized_smelter,
        };
        assert_eq!(value, 20, "{key}");
        // As outras três ficam no padrão.
        assert_eq!(
            Usage::of(&USAGE_CONFIG.load("")),
            Usage::DEFAULT,
            "{key} {default}"
        );
        let machine = machine_with(value);
        assert_eq!(machine.energy().energy_per_tick(), 20);
        assert_eq!(
            machine.energy().capacity(),
            80,
            "`AttributeEnergy.STORAGE_MULTIPLIER` vezes o uso"
        );
        assert_eq!(fill(&machine), 80);
    }
    // Zero: a máquina não gasta nada e o buffer é zero (`Math.max(config, usage)`).
    let free = machine_with(0);
    assert_eq!(free.energy().capacity(), 0);
}

#[test]
fn a_run_spends_the_configured_usage_each_tick() {
    let mut machine = machine_with(20);
    machine
        .input()
        .set_contents(Some(common::item("clay")), 3, None);
    fill(&machine);
    let before = machine.energy().amount();
    machine.tick_server(1, &NoNeighbours);
    assert_eq!(
        before - machine.energy().amount(),
        20,
        "o gasto de um tick de operação"
    );
}

#[test]
fn the_energy_buffer_is_capped_at_what_the_horizon_holds() {
    assert_eq!(mekanism_rules::energy_capacity(5), 20);
    assert_eq!(mekanism_rules::energy_capacity(i32::MAX / 4), 2_147_483_644);
    assert_eq!(mekanism_rules::energy_capacity(i32::MAX / 4 + 1), i32::MAX);
    assert_eq!(mekanism_rules::energy_capacity(i32::MAX), i32::MAX);
    // A máquina viva guarda a conta inteira.
    assert_eq!(
        machine_with(i32::MAX).energy().capacity(),
        4 * i64::from(i32::MAX)
    );
}

#[test]
fn the_transporters_follow_the_order_of_the_tiers_and_the_file_changes_speed_and_pull_amount() {
    use machinery_basis_core::transporter::node::TransporterTier;
    use mekanism_rules::{
        DEFAULT_TRANSPORTER_PULL_AMOUNTS, DEFAULT_TRANSPORTER_SPEEDS, LOGISTICAL_TRANSPORTERS,
        transporter_pull_amounts, transporter_speeds,
    };
    assert_eq!(LOGISTICAL_TRANSPORTERS.len(), TransporterTier::VALUES.len());
    // Os padrões do arquivo são os de `TransporterTier` (`getBasePull`, `getBaseSpeed`), os que a lib tinha.
    let defaults = TIERS_CONFIG.defaults();
    assert_eq!(
        transporter_pull_amounts(&defaults),
        DEFAULT_TRANSPORTER_PULL_AMOUNTS
    );
    assert_eq!(transporter_speeds(&defaults), DEFAULT_TRANSPORTER_SPEEDS);
    assert_eq!(DEFAULT_TRANSPORTER_PULL_AMOUNTS, [1, 16, 32, 64]);
    assert_eq!(DEFAULT_TRANSPORTER_SPEEDS, [5, 10, 20, 50]);
    let loaded = TIERS_CONFIG.load(
        "[transmitters.items]\n\t\tadvancedPullAmount = 3\n\t\telitespeed = 7\n\t\tultimateSpeed = 99\n\t\tbasicSpeed = 0\n",
    );
    assert_eq!(transporter_pull_amounts(&loaded), [1, 3, 32, 64]);
    assert_eq!(transporter_speeds(&loaded), [1, 10, 20, 99]);
}
