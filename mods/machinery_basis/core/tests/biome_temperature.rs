//! A temperatura do bioma num bloco (`Biome.getTemperature(pos, seaLevel)`) e o ar que o Mekanism tira dela
//! (`HeatAPI.getAmbientTemp`), contra o rastro do Java em `dev/referencia/rastros/heat_ambient_table.jsonl`: para 12 biomas, 6
//! colunas x/z, 10 alturas e 4 níveis do mar, os bits do `float` da temperatura e o `double` do ar, iguais ao bit.
//!
//! Cobre o modificador `frozen` (os oceanos congelados, com três ruídos de semente própria) e o ajuste por altura acima de
//! `seaLevel + 17` (o `TEMPERATURE_NOISE`, de semente 1234, lido por posição).
use machinery_basis_core::biome_temperature::{
    Climate, ClimateNoises, Seeds, VANILLA_SEEDS, sea_level,
};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

fn rows() -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dev/referencia/rastros/heat_ambient_table.jsonl");
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn climates(rows: &[Value]) -> HashMap<String, Climate> {
    let table = rows
        .iter()
        .find_map(|row| row.get("biomes"))
        .expect("a linha dos biomas");
    table
        .as_object()
        .unwrap()
        .iter()
        .map(|(id, biome)| {
            let bits = biome["temperature_bits"].as_i64().unwrap() as i32 as u32;
            (
                id.clone(),
                Climate {
                    temperature: f32::from_bits(bits),
                    frozen: biome["modifier"] == "frozen",
                },
            )
        })
        .collect()
}

/// Uma linha `[bioma, x, y, z, nível do mar, bits da temperatura, ar]`.
struct Row {
    biome: String,
    pos: [i32; 3],
    sea: i32,
    bits: u32,
    ambient: f64,
}

fn parse(row: &Value) -> Row {
    let cells = row.as_array().unwrap();
    Row {
        biome: cells[0].as_str().unwrap().to_owned(),
        pos: [
            cells[1].as_i64().unwrap() as i32,
            cells[2].as_i64().unwrap() as i32,
            cells[3].as_i64().unwrap() as i32,
        ],
        sea: cells[4].as_i64().unwrap() as i32,
        bits: cells[5].as_i64().unwrap() as i32 as u32,
        ambient: cells[6].as_f64().unwrap(),
    }
}

/// Confere uma linha com os ruídos dados e devolve se o ajuste por altura mexeu na temperatura.
fn check(row: &Row, climate: &Climate, noises: &ClimateNoises) -> bool {
    let temperature = climate.height_adjusted(noises, row.pos, row.sea);
    assert_eq!(
        temperature.to_bits(),
        row.bits,
        "{} em {:?} (mar {}): temperatura {temperature}",
        row.biome,
        row.pos,
        row.sea
    );
    assert_eq!(
        machinery_basis_core::heat::ambient(f64::from(temperature)),
        row.ambient,
        "{} em {:?} (mar {}): ar",
        row.biome,
        row.pos,
        row.sea
    );
    temperature != climate.temperature
}

#[test]
fn every_row_matches_the_original_bit_for_bit() {
    let rows = rows();
    let climates = climates(&rows);
    let noises = ClimateNoises::new(&VANILLA_SEEDS);
    let mut checked = 0;
    let mut adjusted = 0;
    let mut frozen_rows = 0;
    for row in rows.iter().filter_map(|row| row.get("row")) {
        let row = parse(row);
        let climate = &climates[&row.biome];
        frozen_rows += usize::from(climate.frozen);
        adjusted += usize::from(check(&row, climate, &noises));
        checked += 1;
    }
    // 12 biomas x 6 colunas x 10 alturas x 4 níveis do mar. Os pisos guardam que a tabela não ficou vazia: a comparação sozinha passaria
    // com zero linhas.
    assert_eq!(checked, 12 * 6 * 10 * 4);
    assert!(
        adjusted > 1_000,
        "só {adjusted} linhas com ajuste de altura"
    );
    assert!(
        frozen_rows >= 2 * 6 * 10 * 4,
        "{frozen_rows} linhas de bioma congelado"
    );
}

/// A grade do modificador `frozen`: as duas respostas aparecem, e o limiar do gelo (0,3) e o da variação pequena (0,8) são cruzados.
#[test]
fn the_frozen_scan_matches_and_crosses_both_thresholds() {
    let rows = rows();
    let climates = climates(&rows);
    let noises = ClimateNoises::new(&VANILLA_SEEDS);
    let (mut iced, mut kept) = (0, 0);
    for row in rows.iter().filter_map(|row| row.get("scan")) {
        let row = parse(row);
        let climate = &climates[&row.biome];
        check(&row, climate, &noises);
        if f32::from_bits(row.bits) == climate.temperature {
            kept += 1;
        } else {
            iced += 1;
        }
    }
    assert_eq!(iced + kept, 2 * 30 * 30);
    assert!(iced > 300 && kept > 300, "{iced} com gelo e {kept} sem");
}

#[test]
fn the_height_adjustment_starts_above_sea_level_plus_17() {
    let rows = rows();
    let climates = climates(&rows);
    let noises = ClimateNoises::new(&VANILLA_SEEDS);
    for row in rows.iter().filter_map(|row| row.get("row")) {
        let row = parse(row);
        let climate = &climates[&row.biome];
        if climate.frozen {
            continue;
        }
        let temperature = f32::from_bits(row.bits);
        let at_or_below = row.pos[1] <= row.sea + 17;
        assert_eq!(
            temperature == climate.temperature,
            at_or_below,
            "{} em {:?} (mar {})",
            row.biome,
            row.pos,
            row.sea
        );
        assert_eq!(check(&row, climate, &noises), !at_or_below);
    }
}

#[test]
fn real_positions_of_the_test_world_match() {
    let rows = rows();
    let climates = climates(&rows);
    let noises = ClimateNoises::new(&VANILLA_SEEDS);
    let worlds: Vec<Row> = rows
        .iter()
        .filter_map(|row| row.get("world"))
        .map(parse)
        .collect();
    assert_eq!(worlds.len(), 5);
    for row in &worlds {
        check(row, &climates[&row.biome], &noises);
    }
}

#[test]
fn the_test_world_is_flat_and_its_sea_level_is_minus_63() {
    let rows = rows();
    let overworld = rows[0]["levels"]["overworld"].as_i64().unwrap() as i32;
    assert_eq!(overworld, sea_level("minecraft:overworld", true));
    // Os níveis do nether e do end do mundo de teste também são -63: o `GameTestServer` põe um gerador plano em todos. Os do jogo
    // vêm de `noise_settings/{nether,end}.json` (32 e 0), que o teste não alcança.
    assert_eq!(rows[0]["levels"]["minecraft:the_nether"], -63);
    assert_eq!(rows[0]["levels"]["minecraft:the_end"], -63);
}

/// O cache do bioma (`Biome.temperatureCache`) é por posição e vale para qualquer nível do mar: a mesma posição com outro nível devolve
/// o valor do primeiro. O port não tem esse cache (é compartilhado com todo chamador do vanilla); o teste guarda a diferença conhecida.
#[test]
fn the_biome_cache_ignores_the_sea_level_and_the_port_does_not_have_it() {
    let rows = rows();
    let climates = climates(&rows);
    let aliased: Vec<Row> = rows
        .iter()
        .filter_map(|row| row.get("aliased"))
        .map(parse)
        .collect();
    assert_eq!(aliased.len(), 4);
    let climate = &climates["minecraft:plains"];
    let noises = ClimateNoises::new(&VANILLA_SEEDS);
    assert!(aliased.iter().all(|row| row.bits == aliased[0].bits));
    // O primeiro, com o nível do mar 63, é o cálculo; os outros são o cache.
    assert!(check(&aliased[0], climate, &noises));
    assert_ne!(
        climate
            .height_adjusted(&noises, aliased[1].pos, aliased[1].sea)
            .to_bits(),
        aliased[1].bits,
        "se o port passar a ter o cache, esta diferença deixa de existir"
    );
}

/// Mutações: cada uma tem de quebrar a comparação com o rastro.
fn mutated_fails(
    climate_of: impl Fn(&Climate) -> Climate,
    noises: &ClimateNoises,
    sea_shift: i32,
) -> bool {
    let rows = rows();
    let climates = climates(&rows);
    rows.iter()
        .filter_map(|row| row.get("row").or_else(|| row.get("scan")))
        .map(parse)
        .any(|row| {
            let climate = climate_of(&climates[&row.biome]);
            let temperature = climate.height_adjusted(noises, row.pos, row.sea + sea_shift);
            temperature.to_bits() != row.bits
        })
}

#[test]
fn a_wrong_snow_level_fails_the_table() {
    let noises = ClimateNoises::new(&VANILLA_SEEDS);
    assert!(!mutated_fails(|c| *c, &noises, 0), "a base tem de passar");
    assert!(mutated_fails(|c| *c, &noises, 1));
    assert!(mutated_fails(|c| *c, &noises, -1));
}

#[test]
fn another_seed_fails_the_table() {
    for seeds in [
        Seeds {
            height: 1235,
            ..VANILLA_SEEDS
        },
        Seeds {
            frozen: 3457,
            ..VANILLA_SEEDS
        },
        Seeds {
            info: 2346,
            ..VANILLA_SEEDS
        },
    ] {
        assert!(
            mutated_fails(|c| *c, &ClimateNoises::new(&seeds), 0),
            "uma semente trocada passou"
        );
    }
}

#[test]
fn ignoring_the_frozen_modifier_fails_the_table() {
    let noises = ClimateNoises::new(&VANILLA_SEEDS);
    assert!(mutated_fails(
        |c| Climate {
            frozen: false,
            ..*c
        },
        &noises,
        0
    ));
}

#[test]
fn ignoring_the_height_fails_the_table() {
    let rows = rows();
    let climates = climates(&rows);
    let noises = ClimateNoises::new(&VANILLA_SEEDS);
    let any = rows
        .iter()
        .filter_map(|row| row.get("row"))
        .map(parse)
        .any(|row| {
            let climate = &climates[&row.biome];
            // Sem o ajuste, só o modificador do bioma: a temperatura sem a altura.
            let flat =
                climate.height_adjusted(&noises, [row.pos[0], i32::MIN / 2, row.pos[2]], row.sea);
            flat.to_bits() != row.bits
        });
    assert!(any);
}
