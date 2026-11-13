//! O ar do bioma no Network Reader, contra o original (frente `ar-bioma`, decisão 184): `dev/referencia/rastros/ar_bioma.jsonl`, gravado pelo cliente do Mekanism
//! (`CenaLeitor`, `cenas/ar_bioma.txt`, `ar_bioma_rastro.py`). Cada caso é uma linha de três condutores termodinâmicos básicos em x = 300, z = 0 de um mundo plano (nível do mar -63) numa altura e
//! num bioma (`/fillbiome`); o rastro guarda, para cada posição que os condutores leem (a deles e a de cada vizinho), o bioma que o original achou ali, os bits da temperatura base, os da
//! temperatura com o ajuste de altura (`Biome.getTemperature(pos, seaLevel)`) e o `HeatAPI.getAmbientTemp(level, pos)`, mais o chat do leitor no condutor do meio 300 tiques depois.
//!
//! Dois testes: a conta do ar (a temperatura e o ar de cada posição, iguais ao bit) e a linha de condutores com esse ar por posição, 300 tiques, que acaba na temperatura que o chat do original
//! mostra (os mesmos dois decimais).
mod heat_owner_world;

use heat_owner_world::{Ambient, World};
use machinery_basis_core::biome_temperature::{Climate, sea_level};
use machinery_basis_core::sim::chunks::BlockPos;
use machinery_basis_core::transmission::reading::{
    NetworkReader, ReadingDetail, TransmitterReading,
};
use machinery_basis_core::transmission::thermodynamic_conductor::ConductorTier;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

/// Os ticks entre pôr os condutores e o clique (`ate=300` em `cenas/ar_bioma.txt`).
const TICKS_BEFORE_CLICK: usize = 300;
/// A linha de condutores do original: x = 300, z = 0.
const ORIGIN_X: i32 = 300;

fn cases() -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dev/referencia/rastros/ar_bioma.jsonl");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn position(read: &Value) -> [i32; 3] {
    let at = read["at"].as_array().unwrap();
    [
        at[0].as_i64().unwrap() as i32,
        at[1].as_i64().unwrap() as i32,
        at[2].as_i64().unwrap() as i32,
    ]
}

fn climate(read: &Value) -> Climate {
    Climate {
        temperature: f32::from_bits(read["base_bits"].as_i64().unwrap() as i32 as u32),
        frozen: read["modifier"] == "frozen",
    }
}

#[test]
fn the_ambient_of_every_position_the_conductors_read_is_the_original_bit_for_bit() {
    let cases = cases();
    assert!(cases.len() >= 10, "só {} casos", cases.len());
    let mut positions = 0;
    let mut above_snow_level = 0;
    let mut at_or_below_snow_level = 0;
    let mut below_sea_level = 0;
    let mut warm_biomes = 0;
    let mut cold_biomes = 0;
    for case in &cases {
        let sea = case["sea"].as_i64().unwrap() as i32;
        assert_eq!(
            sea,
            sea_level("minecraft:overworld", true),
            "{}",
            case["nome"]
        );
        for read in case["pos"].as_array().unwrap() {
            let pos = position(read);
            let climate = climate(read);
            let temperature = climate.temperature_at(pos, sea);
            assert_eq!(
                temperature.to_bits(),
                read["bits"].as_i64().unwrap() as i32 as u32,
                "{} em {pos:?}: temperatura {temperature:?}",
                case["nome"]
            );
            let ambient = climate.ambient_at(pos, sea);
            assert_eq!(
                ambient.to_bits(),
                read["ambient"].as_f64().unwrap().to_bits(),
                "{} em {pos:?}: ar {ambient:?}",
                case["nome"]
            );
            positions += 1;
            if pos[1] > sea + 17 {
                above_snow_level += 1;
            } else {
                at_or_below_snow_level += 1;
            }
            if pos[1] < sea {
                below_sea_level += 1;
            }
            if ambient > 300.0 {
                warm_biomes += 1;
            } else if ambient < 290.0 {
                cold_biomes += 1;
            }
        }
    }
    assert!(positions >= 180, "só {positions} posições");
    assert!(
        above_snow_level >= 50,
        "só {above_snow_level} acima do limite"
    );
    assert!(at_or_below_snow_level >= 50);
    assert!(below_sea_level >= 5, "só {below_sea_level} abaixo do mar");
    assert!(warm_biomes >= 10 && cold_biomes >= 10);
}

/// O valor em kelvin de um `300K`, `297.85K`, `91.34µK` ou `3.65mK` do chat, e a unidade dele (a casa que o chat arredonda).
fn parse_kelvin(text: &str) -> Option<(f64, f64)> {
    let text = text.trim().strip_suffix('K')?;
    let (digits, unit) = match text.chars().last()? {
        '\u{b5}' => (&text[..text.len() - '\u{b5}'.len_utf8()], 1e-6),
        'm' => (&text[..text.len() - 1], 1e-3),
        'k' => (&text[..text.len() - 1], 1e3),
        _ => (text, 1.0),
    };
    Some((digits.parse::<f64>().ok()? * unit, unit))
}

/// Os números do chat do leitor: o `Buffer: X above ambient` (a média da rede), o `Throughput: A transferred to acceptors, B lost to environment` e o `Temperature: X`.
struct Chat {
    mean: (f64, f64),
    transferred: (f64, f64),
    lost: (f64, f64),
    temperature: (f64, f64),
}

fn chat(case: &Value) -> Option<Chat> {
    let lines: Vec<&str> = case["chat"]
        .as_array()
        .unwrap()
        .iter()
        .map(|line| line.as_str().unwrap().trim())
        .collect();
    let value = |prefix: &str| lines.iter().find_map(|line| line.strip_prefix(prefix));
    let mean = value("*Buffer:")?.split(" above").next()?;
    let throughput = value("*Throughput:")?;
    let (transferred, rest) = throughput.split_once(" transferred to acceptors, ")?;
    let lost = rest.split(" lost").next()?;
    let temperature = value("*Temperature:")?;
    Some(Chat {
        mean: parse_kelvin(mean)?,
        transferred: parse_kelvin(transferred)?,
        lost: parse_kelvin(lost)?,
        temperature: parse_kelvin(temperature)?,
    })
}

/// O chat escreve o número cortado em duas casas (`300.0076` aparece como `300K`, `70.8996µK` como `70.89µK`): o valor da conta, cortado igual, tem de ser o do chat.
fn shows(value: f64, (shown, unit): (f64, f64)) -> bool {
    let cut = (value / unit * 100.0).floor() / 100.0 * unit;
    (cut - shown).abs() <= 1e-9 * unit.max(1e-6)
}

#[test]
fn the_conductor_line_gives_the_numbers_the_original_chat_shows_300_ticks_after_placement() {
    let mut compared = 0;
    for case in cases() {
        let Some(want) = chat(&case) else {
            continue;
        };
        let y = case["y"].as_i64().unwrap() as i32;
        let reads: HashMap<[i32; 3], f64> = case["pos"]
            .as_array()
            .unwrap()
            .iter()
            .map(|read| (position(read), read["ambient"].as_f64().unwrap()))
            .collect();
        let origin = BlockPos::new(ORIGIN_X, y, 0);
        let mut world = World::with_ambient(
            origin,
            Ambient::from_positions(origin, reads),
            (1..40).map(|n| n * 7919).collect(),
            (1..40).map(|n| n * 31).collect(),
        );
        for x in 0..3 {
            world.place_conductor(x, ConductorTier::Basic);
        }
        for _ in 0..TICKS_BEFORE_CLICK {
            world.tick();
        }
        let name = case["nome"].as_str().unwrap();
        let temperature = world
            .owner()
            .state_at(world.pos(1, 0))
            .unwrap()
            .heat
            .unwrap();
        let TransmitterReading::Network(network) = world.owner().read_network(world.pos(1, 0))
        else {
            panic!("{name}: sem rede");
        };
        let ReadingDetail::Heat {
            mean_temp,
            transferred,
            lost,
        } = network.detail
        else {
            panic!("{name}: leitura que não é de calor");
        };
        assert_eq!((network.transmitters, network.acceptors), (3, 4), "{name}");
        assert!(
            shows(temperature, want.temperature),
            "{name}: temperatura {temperature:?} contra {:?}",
            want.temperature
        );
        assert!(
            shows(mean_temp, want.mean),
            "{name}: média {mean_temp:?} contra {:?}",
            want.mean
        );
        assert!(
            shows(transferred, want.transferred),
            "{name}: passou {transferred:?} contra {:?}",
            want.transferred
        );
        assert!(
            shows(lost, want.lost),
            "{name}: perdeu {lost:?} contra {:?}",
            want.lost
        );
        compared += 1;
    }
    assert!(compared >= 10, "só {compared} casos com chat");
}
