//! A radiação da lib contra os rastros do original (`dev/referencia/testes/RadiationTraceTest.java`, `dev/referencia/rastros/radiation_*.jsonl`).
//!
//! - `radiation_rules`: um `RadiationLevelData` solto do original e o [`RadiationLevelData`] da lib recebem os mesmos `radiate`, `decay`,
//!   `removeRadiationSource` e `removeRadiationSources`; a lista gravada (a ordem do `HashMap` do Java), o nível e o `maxMagnitude` em 191
//!   pontos saem iguais bit a bit, também depois de gravar e reler. A escala, a cor, a severidade, o tempo de decaimento, o texto com
//!   `Sv/h` e a regra de envio ao cliente também.
//! - `radiation_entity`: a dose com cada combinação do traje de hazmat, no criativo e em mobs, e o efeito de uma dose alta.
//! - `radiation_ticks`: tick a tick, a fonte e a dose de uma vaca, de um husk com máscara e avental e de um jogador. Os sorteios do
//!   original não se repetem, então o teste confere que cada passo é um dos que a regra permite (a fonte igual ou decaída uma vez; a
//!   dose igual, ou exposta e decaída com o nível daquele tick; o dano igual à força da dose).
//! - `radiation_sources`: a magnitude de `dumpRadiation` de cada químico (`radioactivity * amount`).
//! - `radiation_command`: o valor que cada subcomando devolve e a dose que fica.
use machinery_basis_core::radiation::*;
use machinery_basis_core::units::display_base;
use serde_json::Value;
use std::path::Path;

fn rows(name: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dev/referencia/rastros")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .lines()
        // O `TileEntityChemicalTank.getRadiationScale` do original dá infinito (ver `radiation_sources`), e o Gson o escreve cru.
        .map(|line| serde_json::from_str(&line.replace(":Infinity", ":\"Infinity\"")).unwrap())
        .collect()
}

fn pos(value: &Value) -> BlockPos {
    [0, 1, 2].map(|i| value[i].as_i64().unwrap() as i32)
}

/// A lista gravada (`{radiation:[{pos:[I;x,y,z],radiation:<double>d}, ...]}`), na ordem do texto.
fn saved(nbt: &str) -> Vec<RadiationSource> {
    nbt.split("{pos:[I;")
        .skip(1)
        .map(|part| {
            let (coords, rest) = part.split_once("],radiation:").unwrap();
            let coords: Vec<i32> = coords.split(',').map(|c| c.parse().unwrap()).collect();
            let magnitude = rest.split_once('d').unwrap().0.parse().unwrap();
            RadiationSource {
                pos: [coords[0], coords[1], coords[2]],
                magnitude,
            }
        })
        .collect()
}

const RADIUS: i32 = 80;
const RATE: f64 = 0.9995;

fn check_state(step: &Value, data: &RadiationLevelData) {
    let name = step["step"].as_str().unwrap();
    assert_eq!(data.is_empty(), step["empty"].as_bool().unwrap(), "{name}");
    assert_eq!(
        data.sources(),
        saved(step["nbt"].as_str().unwrap()),
        "{name}: a lista gravada"
    );
    for probe in step["levels"].as_array().unwrap() {
        let at = pos(probe);
        let got = data.level_and_max(at);
        assert_eq!(
            got.level.to_bits(),
            probe[3].as_f64().unwrap().to_bits(),
            "{name}: o nível em {at:?}"
        );
        assert_eq!(
            got.max_magnitude.to_bits(),
            probe[4].as_f64().unwrap().to_bits(),
            "{name}: o máximo em {at:?}"
        );
    }
}

#[test]
fn the_sources_the_levels_and_the_saved_list_match_the_original_bit_for_bit() {
    let rows = rows("radiation_rules.jsonl");
    let by_step = |name: &str| {
        rows.iter()
            .find(|row| row["step"] == name)
            .unwrap_or_else(|| panic!("{name}"))
    };
    let mut data = RadiationLevelData::new();
    check_state(by_step("empty"), &data);
    let mut radiated = 0;
    for row in rows.iter().filter(|row| row["step"] == "radiate") {
        data.radiate(pos(&row["pos"]), row["magnitude"].as_f64().unwrap(), RADIUS);
        assert_eq!(
            data.sources(),
            saved(row["nbt"].as_str().unwrap()),
            "radiate {radiated}"
        );
        radiated += 1;
    }
    assert_eq!(radiated, 27);
    check_state(by_step("radiated"), &data);
    for k in 1..=3 {
        data.decay(RATE);
        check_state(by_step(&format!("decay{k}")), &data);
    }
    for _ in 4..=50 {
        data.decay(RATE);
    }
    check_state(by_step("decay50"), &data);
    let removed = by_step("remove_at");
    assert_eq!(
        data.remove_at([1, 64, 0]),
        removed["missing"].as_bool().unwrap()
    );
    assert_eq!(
        data.remove_at([3, 64, 4]),
        removed["hit"].as_bool().unwrap()
    );
    check_state(by_step("after_remove_at"), &data);
    let chunk = by_step("remove_chunk");
    assert_eq!(
        data.remove_chunk(5, 5),
        chunk["empty_chunk"].as_bool().unwrap()
    );
    assert_eq!(data.remove_chunk(1, 1), chunk["hit"].as_bool().unwrap());
    check_state(by_step("after_remove_chunk"), &data);
    let reloaded = RadiationLevelData::load(&data.sources(), RADIUS);
    check_state(by_step("reloaded"), &reloaded);

    let mut reverse = RadiationLevelData::new();
    let pushes: Vec<&Value> = rows.iter().filter(|row| row["step"] == "radiate").collect();
    for row in pushes.iter().rev() {
        reverse.radiate(pos(&row["pos"]), row["magnitude"].as_f64().unwrap(), RADIUS);
    }
    check_state(by_step("reverse"), &reverse);
    check_state(
        by_step("reverse_reloaded"),
        &RadiationLevelData::load(&reverse.sources(), RADIUS),
    );
    reverse.clear();
    check_state(by_step("cleared"), &reverse);
}

/// O texto de um `Component` do rastro: o `text` e os `extra`, em ordem.
fn flat(component: &Value) -> String {
    match component {
        Value::String(text) => text.clone(),
        _ => {
            let mut out = component["text"].as_str().unwrap_or_default().to_owned();
            for extra in component["extra"].as_array().into_iter().flatten() {
                out += &flat(extra);
            }
            out
        }
    }
}

#[test]
fn the_scale_the_colour_the_severity_the_decay_time_and_the_display_match() {
    let rows = rows("radiation_rules.jsonl");
    let mut seen = 0;
    for row in rows.iter().filter(|row| row["step"] == "scale") {
        let value = row["value"].as_f64().unwrap();
        assert_eq!(RadiationScale::get(value).name(), row["scale"], "{value}");
        assert_eq!(SeverityColor::get(value).name(), row["color"], "{value}");
        assert_eq!(
            scaled_dose_severity(value).to_bits(),
            row["severity"].as_f64().unwrap().to_bits(),
            "{value}"
        );
        assert_eq!(
            decay_time(value, RATE),
            row["decay_source"].as_i64().unwrap(),
            "{value}"
        );
        assert_eq!(
            decay_time(value, RATE),
            row["decay_target"].as_i64().unwrap(),
            "{value}"
        );
        assert_eq!(
            display_base(value, Some("Sv/h"), 3, true),
            flat(&row["display"]),
            "{value}"
        );
        assert_eq!(
            display_base(value, Some("Sv"), 2, true),
            flat(&row["display_sv"]),
            "{value}"
        );
        seen += 1;
    }
    assert_eq!(seen, 27);
}

#[test]
fn the_level_goes_to_the_client_only_when_the_shown_digits_change() {
    let rows = rows("radiation_rules.jsonl");
    let mut last: Option<ClientSync> = None;
    let mut seen = 0;
    for row in rows.iter().filter(|row| row["step"] == "sync") {
        let value = row["value"].as_f64().unwrap();
        let next = ClientSync::compare(last.as_ref(), value);
        assert_eq!(next.is_some(), row["sync"].as_bool().unwrap(), "{value}");
        if next.is_some() {
            last = next;
        }
        assert_eq!(
            last.unwrap().magnitude,
            row["kept"].as_f64().unwrap(),
            "{value}"
        );
        seen += 1;
    }
    assert_eq!(seen, 27);
}

/// Os escudos do traje de hazmat por peça, na ordem em que o original os soma (`EquipmentSlotGroup.ARMOR`: pés, pernas, peito, cabeça).
fn hazmat(mask: i64) -> f64 {
    let pieces = [(8, 0.15), (4, 0.2), (2, 0.4), (1, 0.25)];
    resistance(
        pieces
            .iter()
            .filter(|(bit, _)| mask & bit != 0)
            .map(|(_, shield)| *shield),
    )
}

#[test]
fn the_dose_of_each_hazmat_combination_and_of_mobs_matches() {
    let rows = rows("radiation_entity.jsonl");
    let mut seen = 0;
    for row in rows.iter().filter(|row| row["step"] == "hazmat") {
        let mask = row["mask"].as_i64().unwrap();
        let resistance = hazmat(mask);
        assert_eq!(
            resistance.to_bits(),
            row["resistance"].as_f64().unwrap().to_bits(),
            "máscara {mask}"
        );
        let dose = radiate_dose(BASELINE, shielded(0.36, resistance));
        assert_eq!(
            dose.to_bits(),
            row["dose"].as_f64().unwrap().to_bits(),
            "máscara {mask}"
        );
        assert_eq!(
            decay_dose(dose, RATE).to_bits(),
            row["decayed"].as_f64().unwrap().to_bits(),
            "máscara {mask}"
        );
        seen += 1;
    }
    assert_eq!(seen, 16);
    let player = rows.iter().find(|row| row["step"] == "player").unwrap();
    assert_eq!(set_dose(1.0e-9), player["set_below"].as_f64().unwrap());
    assert_eq!(
        radiate_dose(BASELINE, -1.0),
        player["radiate_negative"].as_f64().unwrap()
    );
    let dose = radiate_dose(BASELINE, 0.002);
    assert_eq!(dose, player["cap_radiate"].as_f64().unwrap());
    let mut dose = radiate_dose(dose, shielded(0.001, 0.0));
    assert_eq!(dose, player["manager_radiate"].as_f64().unwrap());
    assert_eq!(
        player["attachments"],
        format!(
            "{{\"mekanism:radiation\":{{radiation:{}d}}}}",
            java_double(dose)
        )
    );
    assert_eq!(player["attachments_baseline"], "null");
    for expected in player["decays"].as_array().unwrap() {
        dose = decay_dose(dose, RATE);
        assert_eq!(dose.to_bits(), expected.as_f64().unwrap().to_bits());
    }
    // `RadiationManager.radiate(entity, m)` não toca o criativo; `IRadiationEntity.radiate` direto toca.
    assert_eq!(player["creative_manager"].as_f64().unwrap(), BASELINE);
    assert_eq!(
        player["creative_cap"].as_f64().unwrap(),
        radiate_dose(BASELINE, 0.5)
    );
    for row in rows.iter().filter(|row| row["step"] == "mob") {
        let resistance = if row["type"] == "minecraft:husk" {
            hazmat(3)
        } else {
            0.0
        };
        assert_eq!(resistance, row["resistance"].as_f64().unwrap());
        assert_eq!(
            radiate_dose(BASELINE, shielded(0.01, resistance)),
            row["dose"].as_f64().unwrap()
        );
        // A tag `mekanism:radiation_immune` (os golens) torna o dano de radiação inválido; a dose sobe igual.
        let immune = row["type"] == "minecraft:iron_golem" || row["type"] == "minecraft:snow_golem";
        assert_eq!(row["hurt"].as_bool().unwrap(), !immune);
    }
}

fn java_double(value: f64) -> String {
    machinery_basis_core::units::java_double_to_string(value)
}

/// Um sorteio fixo: `next_double` e `next_boolean` devolvem o que foi posto.
struct Fixed {
    double: f64,
    boolean: bool,
    int: i32,
}

impl RadiationRandom for Fixed {
    fn next_int(&mut self, _bound: i32) -> i32 {
        self.int
    }
    fn next_double(&mut self) -> f64 {
        self.double
    }
    fn next_boolean(&mut self) -> bool {
        self.boolean
    }
}

#[test]
fn a_high_dose_hurts_by_its_strength_and_a_low_one_never_does() {
    let rows = rows("radiation_entity.jsonl");
    let config = RadiationConfig::default();
    let mut hurts = 0;
    for row in rows.iter().filter(|row| row["step"] == "update") {
        let dose = row["dose"].as_f64().unwrap();
        assert_eq!(
            scaled_dose_severity(dose).to_bits(),
            row["severity"].as_f64().unwrap().to_bits()
        );
        let lost = row["health_lost"].as_f64().unwrap() as f32;
        let exhaustion = row["exhaustion_added"].as_f64().unwrap() as f32;
        // A vida é um `float`: 20 menos a força e de volta.
        // O sorteio mais favorável (a chance no mínimo, a moeda dando dano): se nem ele dá efeito, o original não pode ter dado.
        let best = dose_effect(
            &config,
            dose,
            &mut Fixed {
                double: 0.0,
                boolean: true,
                int: 0,
            },
        );
        match best.strength {
            None => assert_eq!((lost, exhaustion), (0.0, 0.0), "dose {dose}"),
            Some(strength) => {
                let health_step = 20f32 - (20f32 - strength);
                // A exaustão soma num `float` que já tem o que veio antes (a diferença lida perde os últimos bits) e para em 40
                // (`FoodData.addExhaustion`), então o que entra nunca passa da força.
                assert!(
                    exhaustion <= strength + 1.0e-5,
                    "dose {dose}: exaustão {exhaustion}, força {strength}"
                );
                // Com o golpe anterior ainda na janela de invulnerabilidade do jogador de teste, só a diferença para o último golpe entra
                // (`LivingEntity.hurtServer`, `amount - lastHurt`): o dano nunca passa da força.
                // O jogador de teste guarda a janela de invulnerabilidade e a exaustão em 40 de uma chamada para a outra, então este
                // rastro só limita o efeito; a vaca e o husk do `radiation_ticks` conferem o dano igual à força.
                assert!(lost <= health_step, "dose {dose}: dano {lost}");
                hurts += (lost > 0.0 || exhaustion > 0.0) as usize;
            }
        }
        assert_eq!(row["dose_after"].as_f64().unwrap(), dose);
    }
    assert!(hurts > 20, "{hurts}");
}

#[test]
fn tick_by_tick_the_source_and_each_dose_take_only_the_steps_the_rules_allow() {
    let rows = rows("radiation_ticks.jsonl");
    assert_eq!(rows.len(), 401);
    let mut source_decays = 0;
    let mut fires = [0; 3];
    let mut hurts = 0;
    for pair in rows.windows(2) {
        let (before, after) = (&pair[0], &pair[1]);
        let m0 = before["sources"][0][3].as_f64().unwrap();
        let m1 = after["sources"][0][3].as_f64().unwrap();
        if m1 != m0 {
            assert_eq!(m1, m0 * RATE, "tick {}", after["tick"]);
            source_decays += 1;
        }
        for (index, (name, resistance)) in [("cow", 0.0), ("husk", hazmat(3)), ("player", 0.0)]
            .into_iter()
            .enumerate()
        {
            let (e0, e1) = (&before[name], &after[name]);
            if !e1["alive"].as_bool().unwrap() {
                continue;
            }
            let d0 = e0["dose"].as_f64().unwrap();
            let d1 = e1["dose"].as_f64().unwrap();
            if d1 != d0 {
                let candidates: Vec<f64> =
                    [e0["level"].as_f64().unwrap(), e1["level"].as_f64().unwrap()]
                        .into_iter()
                        .map(|level| {
                            decay_dose(
                                radiate_dose(d0, shielded(level / 3_600.0, resistance)),
                                RATE,
                            )
                        })
                        .collect();
                assert!(
                    candidates.contains(&d1),
                    "{name}, tick {}: {d0} -> {d1}, candidatos {candidates:?}",
                    after["tick"]
                );
                fires[index] += 1;
            }
            let lost = (e0["health"].as_f64().unwrap() - e1["health"].as_f64().unwrap()) as f32;
            if lost > 0.0 {
                let health = e0["health"].as_f64().unwrap() as f32;
                let strengths: Vec<f32> = [d0, d1]
                    .iter()
                    .map(|dose| health - (health - f32::max(1.0, dose.ln_1p() as f32)))
                    .collect();
                assert!(
                    strengths.contains(&lost),
                    "{name}, tick {}: perdeu {lost}",
                    after["tick"]
                );
                hurts += 1;
            }
        }
    }
    // 1 em 20 por tick, em 400 ticks: perto de 20 de cada.
    assert!((5..=45).contains(&source_decays), "{source_decays}");
    // O jogador de teste do original (`makeTickingMockServerPlayerInLevel`) não passa pelo `PlayerExposure.tickServer`: a dose dele fica no
    // fundo o rastro inteiro. A vaca e o husk passam pelo `onLivingTick`.
    assert!(
        (5..=45).contains(&fires[0]) && (5..=45).contains(&fires[1]),
        "{fires:?}"
    );
    assert_eq!(fires[2], 0);
    assert!(hurts > 0);
}

#[test]
fn dump_radiation_leaves_radioactivity_times_amount_and_skips_the_rest() {
    let rows = rows("radiation_sources.jsonl");
    let mut seen = 0;
    for row in rows.iter().filter(|row| row["step"] == "dump") {
        let amount = row["amount"].as_u64().unwrap();
        let radioactivity = row["radioactivity"].as_f64().unwrap();
        let dumped = dump_magnitude(true, false, amount, radioactivity);
        assert_eq!(dumped.is_some(), row["dumped"].as_bool().unwrap());
        if let Some(magnitude) = dumped {
            let at = pos(&row["pos"]);
            let source = row["sources"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| pos(s) == at)
                .unwrap();
            assert_eq!(magnitude.to_bits(), source[3].as_f64().unwrap().to_bits());
        }
        seen += 1;
    }
    assert_eq!(seen, 7);
    // O Chemical Tank básico quebrado com 1.000 de polônio deixa 50 (`preRemoveSideEffects`); o criativo e o despejo não deixam nada.
    let tanks: Vec<&Value> = rows.iter().filter(|row| row["step"] == "tank").collect();
    assert_eq!(
        tanks[0]["sources"][0][3].as_f64().unwrap(),
        dump_magnitude(true, false, 1000, 0.05).unwrap()
    );
    assert_eq!(tanks.len(), 4);
    assert!(
        tanks
            .iter()
            .all(|row| row["sources"].as_array().unwrap().len() == 1)
    );
}

#[test]
fn each_radiation_command_returns_its_level_and_leaves_the_dose() {
    let rows = rows("radiation_command.jsonl");
    let mut dose = BASELINE;
    for row in rows.iter().filter(|row| row.get("command").is_some()) {
        let command = row["command"].as_str().unwrap();
        let words: Vec<&str> = command.split(' ').collect();
        let result = row["result"].as_i64().unwrap();
        let target = words.get(3).is_some_and(|word| word.starts_with('@'));
        let magnitude = || words[if target { 4 } else { 3 }].parse::<f64>().unwrap();
        match words[2] {
            "addEntity" if !target => {
                dose = radiate_dose(dose, magnitude());
                assert_eq!(
                    result,
                    command_return_level(magnitude()) as i64,
                    "{command}"
                );
            }
            "addEntity" => {
                if command.contains("distance=..20] ") && !command.contains("type=") {
                    dose = radiate_dose(dose, magnitude());
                }
            }
            "reduce" => {
                let applies = !target || !command.contains("type=");
                if applies {
                    let new_value = set_dose(dose - magnitude());
                    if !target {
                        assert_eq!(
                            result,
                            command_return_level(dose - new_value) as i64,
                            "{command}"
                        );
                    }
                    dose = new_value;
                }
            }
            "heal" => {
                if !target || !command.contains("type=") {
                    dose = BASELINE;
                }
            }
            "add" if row["success"].as_bool().unwrap() => {
                assert_eq!(
                    result,
                    command_return_level(magnitude()) as i64,
                    "{command}"
                );
            }
            _ => {}
        }
        assert_eq!(
            row["dose"].as_f64().unwrap().to_bits(),
            dose.to_bits(),
            "{command}"
        );
    }
}
