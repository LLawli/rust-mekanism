//! O dono dos condutores (`HeatTransmitters`) contra o original, tick a tick, com os eventos do jogo: `dev/referencia/rastros/heat_<cenário>.jsonl` e
//! `heat_run2_<cenário>.jsonl` (`testes/HeatNetworkTraceTest.java`, duas execuções). O mundo simulado é o de `tests/heat_owner_world/`: os chunks
//! falsos entregam ao dono o `clearRemoved`, o ticker, o aviso do vizinho, a capability invalidada e o `ServerTickEvent.Post` na ordem do
//! jogo, e um Heat Generator mínimo é o vizinho. A cada tick o teste repete as ações do cenário (`mark`, no `ServerTickEvent.Pre`), roda o tick e
//! compara os eventos da rede e o `state` do rastro, bit a bit.
mod heat_owner_world;

use heat_owner_world::{World, tier_of};
use machinery_basis_core::sim::chunks::BlockPos;
use machinery_basis_core::transmission::NetworkId;
use machinery_basis_core::transmission::heat_owner::TraceEvent as Event;
use machinery_basis_core::transmission::thermodynamic_conductor::ConductorTier;
use serde_json::Value;
use std::path::PathBuf;

fn read(name: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dev/referencia/rastros")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn tier(name: &str) -> ConductorTier {
    tier_of(name)
}

fn number(value: &Value) -> Option<f64> {
    value.as_f64()
}

/// Uma ação do cenário, no `ServerTickEvent.Pre`.
fn act(world: &mut World, mark: &str) {
    let words: Vec<&str> = mark.split_whitespace().collect();
    assert_eq!(words[0], "pre", "{mark}");
    match words[1] {
        "place" => world.place_conductor(words[2].parse().unwrap(), tier(words[3])),
        "break" => world.break_conductor(words[2].parse().unwrap()),
        "heat" => world.heat_conductor(words[2].parse().unwrap(), words[3].parse().unwrap()),
        "gheat" => world.heat_generator(words[2].parse().unwrap()),
        "generator" => world.place_generator(words[2].parse().unwrap()),
        "finish" => {}
        other => panic!("ação {other}"),
    }
}

/// Os eventos de rede de uma linha do rastro, na forma do mundo.
fn event_of(row: &Value, origin: BlockPos) -> Option<Event> {
    let net = |key: &str| NetworkId(row[key].as_u64().unwrap() as u32);
    Some(match row["e"].as_str().unwrap() {
        "update" => Event::Update(BlockPos::new(
            origin.x + row["x"].as_i64().unwrap() as i32,
            origin.y,
            origin.z,
        )),
        "commit" => Event::Commit(net("net")),
        "invalidate" => Event::Invalidate(net("net")),
        "net_update" => Event::NetUpdate(net("net")),
        "created" => Event::Created {
            network: net("net"),
            hash: row["hash"].as_i64().unwrap() as i32,
            from: row["from"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| NetworkId(n.as_u64().unwrap() as u32))
                .collect(),
        },
        _ => return None,
    })
}

fn same(label: &str, got: Option<f64>, expected: &Value) {
    match (got, expected) {
        (None, Value::Null) => {}
        (Some(got), expected) => {
            let want = number(expected).unwrap_or_else(|| panic!("{label}: esperava {expected}"));
            assert!(
                got.to_bits() == want.to_bits() || (got.is_nan() && want.is_nan()),
                "{label}: {got:?} contra {want:?}"
            );
        }
        (None, expected) => panic!("{label}: nulo contra {expected}"),
    }
}

fn compare_state(world: &World, row: &Value, label: &str) {
    let owner = world.owner();
    for (x, expected) in row["cond"].as_array().unwrap().iter().enumerate() {
        let label = format!("{label} condutor {x}");
        let pos = world.pos(x as i32, 0);
        let state = owner.state_at(pos);
        if expected.is_null() {
            assert!(state.is_none(), "{label}: devia estar vazio");
            continue;
        }
        let state = state.unwrap_or_else(|| panic!("{label}: devia existir"));
        let tr = owner.transmitter(pos).expect("transmissor");
        assert_eq!(
            state.network.map(|n| n.0 as u64),
            expected["net"].as_u64(),
            "{label}: rede"
        );
        assert_eq!(
            state.orphan,
            expected["orphan"].as_bool().unwrap(),
            "{label}: órfão"
        );
        assert_eq!(
            state.valid,
            expected["valid"].as_bool().unwrap(),
            "{label}: válido"
        );
        same(&format!("{label} calor"), state.heat, &expected["heat"]);
        same(
            &format!("{label} capacidade"),
            Some(state.heat_capacity),
            &expected["cap"],
        );
        same(
            &format!("{label} ct"),
            Some(state.client_temperature),
            &expected["ct"],
        );
        assert_eq!(
            u64::from(tr.get_all_current_connections()),
            expected["all"].as_u64().unwrap(),
            "{label}: conexões"
        );
        assert_eq!(
            u64::from(tr.acceptor_cache.current_acceptor_connections),
            expected["acc"].as_u64().unwrap(),
            "{label}: aceitadores"
        );
    }
    let registry = owner.registry();
    for expected in row["nets"].as_array().unwrap() {
        let id = NetworkId(expected["net"].as_u64().unwrap() as u32);
        let label = format!("{label} rede {}", id.0);
        let network = registry.network(id);
        assert_eq!(
            network.uuid().hash_code(),
            expected["hash"].as_i64().unwrap() as i32,
            "{label}: hash"
        );
        assert_eq!(
            network.transmitters_size() as u64,
            expected["size"].as_u64().unwrap(),
            "{label}: tamanho"
        );
        assert_eq!(
            network.acceptor_count() as u64,
            expected["acceptors"].as_u64().unwrap(),
            "{label}: aceitadores"
        );
        let members: Vec<i64> = network
            .transmitters()
            .iter()
            .map(|t| i64::from(world.x_of(owner.pos_of(*t))))
            .collect();
        let want: Vec<i64> = expected["transmitters"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_i64().unwrap())
            .collect();
        assert_eq!(members, want, "{label}: membros");
        let heat = owner.heat_networks().get(id).expect("rede de calor");
        same(
            &format!("{label} média"),
            Some(heat.mean_temp()),
            &expected["mean"],
        );
        same(
            &format!("{label} perda"),
            Some(heat.heat_lost()),
            &expected["lost"],
        );
        same(
            &format!("{label} transferido"),
            Some(heat.heat_transferred()),
            &expected["transferred"],
        );
    }
    let in_registry: Vec<u64> = registry
        .networks_in_order()
        .into_iter()
        .filter(|n| registry.network(*n).transmitters_size() > 0)
        .map(|n| u64::from(n.0))
        .collect();
    let want: Vec<u64> = row["registry"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_u64().unwrap())
        .collect();
    assert_eq!(in_registry, want, "{label}: registro");
    if let Some(heater_row) = row.get("gen").filter(|g| !g.is_null()) {
        let generator = world.level.hooks.heater.as_ref().expect("gerador");
        same(
            &format!("{label} gerador calor"),
            generator.stored_heat(),
            &heater_row["heat"],
        );
        same(
            &format!("{label} gerador transferido"),
            Some(*generator.last_transfer.borrow()),
            &heater_row["transfer"],
        );
        same(
            &format!("{label} gerador ar"),
            Some(*generator.last_env.borrow()),
            &heater_row["env"],
        );
    }
}

fn replay(file: &str) {
    let rows = read(file);
    let setup = &rows[0];
    assert_eq!(setup["e"], "setup");
    let origin = BlockPos::new(
        setup["x"].as_i64().unwrap() as i32,
        setup["y"].as_i64().unwrap() as i32,
        setup["z"].as_i64().unwrap() as i32,
    );
    let ambient: Vec<Vec<[f64; 7]>> = setup["ambient"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            row.as_array()
                .unwrap()
                .iter()
                .map(|cell| {
                    let mut values = [0.0; 7];
                    for (slot, value) in values.iter_mut().zip(cell.as_array().unwrap()) {
                        *slot = value.as_f64().unwrap();
                    }
                    values
                })
                .collect()
        })
        .collect();
    let uuid_hashes: Vec<i32> = rows
        .iter()
        .filter(|r| r["e"] == "created")
        .map(|r| r["hash"].as_i64().unwrap() as i32)
        .collect();
    let tile_hashes: Vec<i32> = rows
        .iter()
        .filter(|r| r["e"] == "tile")
        .map(|r| r["hash"].as_i64().unwrap() as i32)
        .collect();
    let mut world = World::new(origin, ambient, uuid_hashes, tile_hashes);
    let mut expected_events: Vec<Event> = Vec::new();
    let mut checked_states = 0;
    for row in &rows[1..] {
        let t = row["t"].as_i64().unwrap();
        let kind = row["e"].as_str().unwrap();
        match kind {
            "begin" => compare_state(&world, row, &format!("{file} begin")),
            "mark" => act(&mut world, row["m"].as_str().unwrap()),
            "tile" => {}
            "state" => {
                if t == 0 {
                    continue;
                }
                world.tick();
                let got = world.take_events();
                assert_eq!(got, expected_events, "{file} t={t}: eventos");
                expected_events.clear();
                compare_state(&world, row, &format!("{file} t={t}"));
                checked_states += 1;
            }
            _ => {
                if let Some(event) = event_of(row, world.origin) {
                    expected_events.push(event);
                }
            }
        }
    }
    assert!(
        checked_states >= 60,
        "{file}: só {checked_states} ticks conferidos"
    );
}

#[test]
fn a_line_of_basic_conductors_with_a_heated_end() {
    replay("heat_line.jsonl");
    replay("heat_run2_line.jsonl");
}

#[test]
fn conductors_of_the_four_tiers() {
    replay("heat_tiers.jsonl");
    replay("heat_run2_tiers.jsonl");
}

#[test]
fn lone_conductors_lose_heat_to_the_air_and_gain_it_back() {
    replay("heat_ambient.jsonl");
    replay("heat_run2_ambient.jsonl");
}

#[test]
fn two_lines_that_merge_and_split_again() {
    replay("heat_merge_split.jsonl");
    replay("heat_run2_merge_split.jsonl");
}

#[test]
fn a_heat_generator_on_top_of_the_last_conductor() {
    replay("heat_generator.jsonl");
    replay("heat_run2_generator.jsonl");
}
