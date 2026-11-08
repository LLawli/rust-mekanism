//! Os arquivos de configuração de servidor do Generators (`Mekanism/generators.toml` e `Mekanism/generator-storage.toml`, `Type.SYNCED`): o texto que o port
//! escreve contra o que o original escreveu sozinho (`rastros/mekanism_generators.toml` e `mekanism_generator_storage.toml`), a leitura e a correção de valores errados
//! contra o que o NeoForge fez com os mesmos arquivos (`rastros/config_correcao`), e o efeito de cada chave do Heat Generator.
use machinery_basis_core::client_config::{Loaded, Spec};
use machinery_basis_core::heat::AMBIENT_TEMP;
use mekanismgenerators_rules::{
    GENERATOR_STORAGE_CONFIG, GENERATORS_CONFIG, HeatConfig, State, Surroundings, finish_tick, step,
};
use std::path::{Path, PathBuf};

fn rastros() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../dev/referencia/rastros")
}

fn reference(name: &str) -> String {
    std::fs::read_to_string(rastros().join(format!("mekanism_{name}.toml"))).unwrap()
}

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

const FILES: [(&str, &str, &Spec); 2] = [
    ("generators", "generators.toml", &GENERATORS_CONFIG),
    (
        "generator_storage",
        "generator-storage.toml",
        &GENERATOR_STORAGE_CONFIG,
    ),
];

fn config_of(generators: &Loaded, storage: &Loaded) -> HeatConfig {
    HeatConfig::of(generators, storage)
}

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
                    "{name}"
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

#[test]
fn the_files_the_original_wrote_read_clean_with_the_defaults() {
    for (name, _, spec) in FILES {
        assert!(!spec.load(&reference(name)).corrected, "{name}");
        assert!(!spec.load(&spec.default_text()).corrected, "{name}");
    }
    let config = config_of(
        &GENERATORS_CONFIG.load(&reference("generators")),
        &GENERATOR_STORAGE_CONFIG.load(&reference("generator_storage")),
    );
    assert_eq!(config, HeatConfig::DEFAULT);
    // Os padrões das constantes dos rastros são os do arquivo.
    assert_eq!(
        config.max_production(),
        mekanismgenerators_rules::MAX_PRODUCTION
    );
    assert_eq!(
        config.energy_capacity_int(),
        mekanismgenerators_rules::ENERGY_CAPACITY
    );
    assert_eq!(config.tank(), mekanismgenerators_rules::TANK_CAPACITY);
}

/// `entrada` tem valores abaixo do mínimo, acima do máximo, de outro tipo, um `heatGenerationFluidRate` que passa do tanque padrão (o validador vê o padrão
/// na primeira leitura) e (em `generator-storage.toml`) uma chave que falta.
#[test]
fn a_wrong_file_is_corrected_the_way_neoforge_corrected_it() {
    for (_, file, spec) in FILES {
        let entrada = crafted("entrada", file);
        let saida = crafted("saida", file);
        let loaded = spec.load(&entrada);
        assert!(loaded.corrected, "{file}");
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
        assert!(
            !spec.load(&saida).corrected,
            "{file}: o do original lê limpo"
        );
    }
}

#[test]
fn each_corrected_key_has_the_value_the_original_gave_it() {
    let config = config_of(
        &GENERATORS_CONFIG.load(&crafted("entrada", "generators.toml")),
        &GENERATOR_STORAGE_CONFIG.load(&crafted("entrada", "generator-storage.toml")),
    );
    assert_eq!(
        config,
        HeatConfig {
            // `-5` sobe ao mínimo, `99999999999` desce a `Integer.MAX_VALUE / 7`, `"abc"` volta ao padrão, e o tanque de 5000 vale.
            heat_generation: 0,
            heat_generation_lava: 306_783_378,
            heat_generation_nether: 10,
            tank_capacity: 5_000,
            // 3000 cabe no tanque de 5000 do arquivo, mas o validador vê o tanque padrão (1000) na primeira leitura: o original deu 100.
            fluid_rate: 100,
            energy_capacity: 1,
        }
    );
}

/// Na leitura seguinte (o arquivo muda com o jogo aberto) o validador vê o tanque em cache, o de antes: 3000 vale se o tanque lido antes era de 5000.
#[test]
fn a_reload_validates_the_fluid_rate_against_the_tank_that_was_in_use() {
    let first = GENERATORS_CONFIG.load("[heat_generator]\n\ttankCapacity = 5000\n");
    let text = "[heat_generator]\n\ttankCapacity = 5000\n\theatGenerationFluidRate = 3000\n";
    let again = GENERATORS_CONFIG.load_after(text, Some(&first));
    assert_eq!(
        HeatConfig::of(&again, &GENERATOR_STORAGE_CONFIG.defaults()).fluid_rate,
        3000
    );
    // Com o tanque de antes menor, o mesmo valor volta ao padrão.
    let small = GENERATORS_CONFIG.defaults();
    let again = GENERATORS_CONFIG.load_after(text, Some(&small));
    assert_eq!(
        HeatConfig::of(&again, &GENERATOR_STORAGE_CONFIG.defaults()).fluid_rate,
        100
    );
    // Zero e negativo nunca valem.
    for rate in ["0", "-1"] {
        let loaded = GENERATORS_CONFIG.load(&format!(
            "[heat_generator]\n\theatGenerationFluidRate = {rate}\n"
        ));
        assert_eq!(
            HeatConfig::of(&loaded, &GENERATOR_STORAGE_CONFIG.defaults()).fluid_rate,
            100,
            "{rate}"
        );
    }
}

// ---- O efeito de cada chave ----

const PLAINS: Surroundings = Surroundings {
    lava_faces: 0,
    in_nether: false,
    ambient: AMBIENT_TEMP,
    face_ambient: [AMBIENT_TEMP; 6],
};

fn state_with(config: HeatConfig, lava: u64) -> State {
    let mut state = State::new(AMBIENT_TEMP);
    state.config = config;
    state.lava = lava;
    state
}

#[test]
fn max_production_adds_the_three_keys_with_a_clamp_at_the_int_limit() {
    let config = HeatConfig::DEFAULT;
    assert_eq!(config.max_production(), 7 * 7 + 10 + 100);
    let big = HeatConfig {
        heat_generation_lava: 306_783_378,
        heat_generation_nether: i32::MAX,
        ..config
    };
    assert_eq!(big.max_production(), i32::MAX, "`MathUtils.addClamped`");
    let none = HeatConfig {
        heat_generation: 0,
        heat_generation_lava: 0,
        heat_generation_nether: 0,
        ..config
    };
    assert_eq!(none.max_production(), 0);
}

#[test]
fn the_fluid_rate_is_what_each_active_tick_burns_and_the_least_the_tank_needs() {
    let config = HeatConfig {
        fluid_rate: 250,
        tank_capacity: 2_000,
        ..HeatConfig::DEFAULT
    };
    let mut state = state_with(config, 249);
    assert!(!step(&mut state, &PLAINS).active, "249 mB não pagam 250");
    let mut state = state_with(config, 600);
    let produced = step(&mut state, &PLAINS);
    assert!(produced.active);
    assert_eq!((produced.lava_used, state.lava), (250, 350));
}

#[test]
fn the_heat_keys_change_the_temperature_the_way_the_formula_says() {
    // Um tick sobre o ar, com lava: o calor da queima (`heatGeneration`) e o do `getBoost` (lava ao lado e Nether) entram antes da conta, na mesma capacidade, então
    // a temperatura só depende da soma deles.
    let after_one_tick = |config: HeatConfig, around: &Surroundings, lava: u64| {
        let mut state = state_with(config, lava);
        step(&mut state, around);
        state.heat.temperature()
    };
    let burning = |generation: i32| {
        after_one_tick(
            HeatConfig {
                heat_generation: generation,
                ..HeatConfig::DEFAULT
            },
            &PLAINS,
            1_000,
        )
    };
    // O padrão (100) é o valor que o Java mediu no primeiro tick (`assert_matches(1, ...)` do runtime).
    assert!((burning(100) - 309.4944).abs() < 1e-4, "{}", burning(100));
    assert!((burning(0) - AMBIENT_TEMP).abs() < 1e-9);
    assert!(burning(400) > burning(100));
    // Lava em três lados do Nether, sem queima: 3 * 7 + 10 = 31 de calor, o mesmo que queimar com `heatGeneration = 31`.
    let around = Surroundings {
        lava_faces: 3,
        in_nether: true,
        ..PLAINS
    };
    let base = HeatConfig {
        heat_generation: 0,
        ..HeatConfig::DEFAULT
    };
    let by_boost = |config: HeatConfig| after_one_tick(config, &around, 0);
    assert_eq!(by_boost(base), burning(31));
    assert_eq!(
        by_boost(HeatConfig {
            heat_generation_lava: 20,
            heat_generation_nether: 0,
            ..base
        }),
        burning(60)
    );
    // `heatGenerationLava` zero ignora a lava do lado, e fora do Nether `heatGenerationNether` não entra.
    assert_eq!(
        by_boost(HeatConfig {
            heat_generation_lava: 0,
            ..base
        }),
        burning(10)
    );
    let overworld = Surroundings {
        in_nether: false,
        ..around
    };
    assert_eq!(after_one_tick(base, &overworld, 0), burning(21));
}

#[test]
fn the_energy_buffer_and_the_tank_follow_their_keys() {
    let config = HeatConfig {
        energy_capacity: 10,
        tank_capacity: 4_000,
        ..HeatConfig::DEFAULT
    };
    let mut state = state_with(config, 1_000);
    state.heat.heat = 10.0 * 500.0;
    state.energy = 4;
    let produced = step(&mut state, &PLAINS);
    assert!(state.energy <= 10, "o buffer de 10 corta o que o calor dá");
    assert_eq!(state.energy, 10);
    assert!(produced.energy <= 6);
    // O comparador lê o tanque pela capacidade do arquivo: 1.000 mB de 4.000 são o quarto do tanque.
    let mut state = state_with(config, 1_000);
    let mut seen = None;
    finish_tick(&mut state, 0, &mut seen);
    let quarter = state.tile.current_redstone_level();
    let full = {
        let mut other = state_with(HeatConfig::DEFAULT, 1_000);
        let mut seen = None;
        finish_tick(&mut other, 0, &mut seen);
        other.tile.current_redstone_level()
    };
    assert_eq!(full, 15);
    assert!(quarter < full, "{quarter}");
    assert_eq!(quarter, 4, "1.000 de 4.000: `floor(15 * 1000 / 4000) + 1`");
}

#[test]
fn a_capacity_past_the_int_the_horizon_holds_stays_at_the_limit() {
    let config = HeatConfig {
        energy_capacity: i64::MAX,
        ..HeatConfig::DEFAULT
    };
    assert_eq!(config.energy_capacity_int(), i32::MAX);
}
