//! O `bio_fuel` queima: o original o registra com `cookingFuel(COOKING_TIME_BIO_FUEL)` (`MekanismItems.java:182`), e o Horizon lê o componente
//! `minecraft:cooking_fuel` do JSON do item (`horizon/item/bio_fuel.json`) com o provider escrito por extenso, porque ele só carrega os
//! providers de `data/minecraft/context_int_provider`. O provider do original é o arquivo que a datagen dele escreve
//! (`context_int_provider/cooking/time_bio_fuel.json`, copiado em `rastros/context_int_provider_time_bio_fuel.json`): 5 s (100 ticks) divididos por 1,
//! ou por 2 num bloco que cozinha mais depressa (`DatapackRegistryUtils.cooking`, `MekanismDatapackRegistries.java:231-233`).
//!
//! Os rastros `furnace_bio_fuel` e `blast_furnace_bio_fuel` (`RestosTraceTest`) gravam um forno e um alto-forno do vanilla com 64 `raw_iron` e 4
//! `bio_fuel`, tick a tick: o tempo que o forno lê (`getBurnDuration`), o relógio aceso, o combustível que sai do slot e o estado `lit`. O teste
//! repete a parte do combustível do `AbstractFurnaceBlockEntity.serverTick` com o tempo de queima que sai do JSON do item.
mod common;

use common::burn_time::{burn_time, burn_time_provider, item_json};
use serde_json::Value;
use std::path::Path;

fn trace(name: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(format!("{name}.jsonl"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn the_item_carries_the_provider_the_original_generated() {
    let original: Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../dev/referencia/rastros/context_int_provider_time_bio_fuel.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let item = item_json("bio_fuel");
    assert_eq!(burn_time_provider(&item), Some(original));
    // `Item.Properties.cookingFuel(key)` usa `ContextFloatProviders.COOKING_DEFAULT_SPEED_MULTIPLIER` (`cooking/speed_default`).
    assert_eq!(
        item["components"]["minecraft:cooking_fuel"]["speed_multiplier"],
        "minecraft:cooking/speed_default"
    );
}

/// `AbstractFurnaceBlockEntity.serverTick`, a parte do combustível, com a entrada sempre presente e a saída com espaço (o cenário tem 64 `raw_iron`).
struct Furnace {
    lit_time: i64,
    lit_total: i64,
    fuel: i64,
    lit: bool,
}

impl Furnace {
    fn tick(&mut self, burn: i64) {
        let was_lit = self.lit_time > 0;
        let mut is_lit = false;
        if was_lit {
            self.lit_time -= 1;
            is_lit = self.lit_time > 0;
        }
        let has_fuel = self.fuel > 0;
        if (is_lit || has_fuel) && !is_lit {
            self.lit_time = burn;
            self.lit_total = burn;
            if burn > 0 {
                self.fuel -= 1;
                is_lit = true;
            }
        }
        self.lit = is_lit;
    }
}

fn replay(name: &str, fast: bool, expected_burn: i64, minimum_lines: usize) {
    let lines = trace(name);
    assert!(
        lines.len() > minimum_lines,
        "rastro curto demais: {}",
        lines.len()
    );
    let item = item_json("bio_fuel");
    let burn = i64::from(burn_time(&item, fast));
    // O que o forno lê (`getBurnDuration`) no contexto do bloco.
    assert_eq!(
        burn,
        lines[0]["burn"]["bio_fuel"].as_i64().unwrap(),
        "{name}: o tempo de queima"
    );
    assert_eq!(burn, expected_burn);
    let mut furnace = Furnace {
        lit_time: 0,
        lit_total: 0,
        fuel: 4,
        lit: false,
    };
    let mut lit_ticks = 0;
    for expected in &lines[2..] {
        furnace.tick(burn);
        let t = expected["t"].as_i64().unwrap();
        assert_eq!(
            furnace.lit_time,
            expected["lit_time"].as_i64().unwrap(),
            "{name} t={t}: relógio aceso"
        );
        assert_eq!(
            furnace.lit_total,
            expected["lit_total"].as_i64().unwrap(),
            "{name} t={t}: total da queima"
        );
        assert_eq!(
            furnace.fuel,
            expected["fuel"][1].as_i64().unwrap(),
            "{name} t={t}: combustível no slot"
        );
        assert_eq!(
            furnace.lit,
            expected["lit"].as_bool().unwrap(),
            "{name} t={t}: bloco aceso"
        );
        lit_ticks += i64::from(furnace.lit);
    }
    // O rastro tem de ter o efeito: os quatro `bio_fuel` queimam por inteiro e o forno apaga no fim.
    assert!(
        lit_ticks >= 4 * burn - 4,
        "{name}: ticks acesos {lit_ticks}"
    );
    assert!(!furnace.lit && furnace.fuel == 0, "{name}: o forno apaga");
}

#[test]
fn a_furnace_burns_a_bio_fuel_for_100_ticks() {
    replay("furnace_bio_fuel", false, 100, 450);
}

#[test]
fn a_blast_furnace_burns_it_for_50_ticks() {
    replay("blast_furnace_bio_fuel", true, 50, 250);
}
