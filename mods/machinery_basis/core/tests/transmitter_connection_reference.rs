//! A conexão por lado do `Transmitter` contra o original, tick a tick: `dev/referencia/rastros/transmitter_neighbors.jsonl`,
//! `transmitter_configurator.jsonl` e `transmitter_redstone.jsonl` (`testes/TransmitterConnectionTraceTest.java`).
//!
//! O mundo é uma grade pequena que aplica os efeitos que o transmissor devolve na ordem em que o original os faz (um vizinho
//! relê o lado de volta, um bloco posto avisa os vizinhos, `onUpdateServer` relê uma vez). Os transmissores de teste não têm
//! rede: a conexão de um transmissor, no original, só depende dos vizinhos, e a rede entra na frente `rede-registro`.

use machinery_basis_core::sides::Face;
use machinery_basis_core::transmission::connection::{
    ConnectionType, TransmissionSet, TransmissionType, connection_map_contains_side, face_ordinal,
};
use machinery_basis_core::transmission::transmitter::{
    BlockPos, Compound, Effect, InteractionResult, NeighborTransmitter, Overlay, Tag, Transmitter,
    TransmitterWorld, relative,
};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

pub fn trace(name: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dev/referencia/rastros")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|line: &Value| line.get("meta").is_none())
        .collect()
}

fn face(name: &str) -> Face {
    Face::ALL
        .into_iter()
        .find(|face| face.name() == name)
        .unwrap_or_else(|| panic!("lado {name}"))
}

/// O que há numa célula além de um transmissor.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Other {
    /// Um bloco qualquer (máquina, cubo, tanque, pedra, baú): o que ele oferece vem da tabela de capabilities.
    Block,
    Redstone,
}

/// A grade: os transmissores, os outros blocos e as capabilities que o original achou em cada lado de cada transmissor.
struct Sim {
    transmitters: BTreeMap<BlockPos, Transmitter<()>>,
    others: HashMap<BlockPos, Other>,
    /// `(posição do transmissor, lado dele)` para "o vizinho oferece a capability": o dado de entrada, lido do rastro.
    caps: HashMap<(BlockPos, usize), bool>,
    /// Os efeitos que não mexem na grade, na ordem em que saíram, por transmissor.
    log: Vec<(BlockPos, Effect)>,
}

struct View<'a>(&'a Sim);

impl TransmitterWorld for View<'_> {
    type Acceptor = ();

    fn is_block_loaded(&self, _pos: BlockPos) -> bool {
        true
    }

    fn is_getting_powered(&self, pos: BlockPos) -> bool {
        Face::ALL
            .iter()
            .any(|side| self.0.others.get(&relative(pos, *side)) == Some(&Other::Redstone))
    }

    fn transmitter_at(&self, pos: BlockPos, side: Face) -> Option<NeighborTransmitter> {
        self.0
            .transmitters
            .get(&pos)
            .map(|other| NeighborTransmitter {
                supported: other.supported_transmission_types(),
                can_connect_opposite: other.can_connect(side.opposite(), self),
            })
    }

    fn acceptor(&self, pos: BlockPos, from_side: Face) -> Option<()> {
        let exists = self.0.others.contains_key(&pos) || self.0.transmitters.contains_key(&pos);
        let asking = relative(pos, from_side);
        (exists
            && self
                .0
                .caps
                .get(&(asking, face_ordinal(from_side.opposite())))
                .copied()
                .unwrap_or(false))
        .then_some(())
    }
}

impl Sim {
    fn new() -> Self {
        Sim {
            transmitters: BTreeMap::new(),
            others: HashMap::new(),
            caps: HashMap::new(),
            log: Vec::new(),
        }
    }

    /// Roda uma operação no transmissor de `pos` (tirado da grade, para o mundo ver os outros) e aplica os efeitos.
    fn with<R>(
        &mut self,
        pos: BlockPos,
        op: impl FnOnce(&mut Transmitter<()>, &View, &mut Vec<Effect>) -> R,
    ) -> Option<R> {
        let mut transmitter = self.transmitters.remove(&pos)?;
        let mut out = Vec::new();
        let result = op(&mut transmitter, &View(self), &mut out);
        self.transmitters.insert(pos, transmitter);
        self.apply(pos, out);
        Some(result)
    }

    fn apply(&mut self, pos: BlockPos, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::NotifyTileChange => self.notify_neighbors(pos),
                Effect::RefreshNeighbor { pos: other, side } => {
                    self.with(other, |t, world, out| {
                        t.refresh_connections_side(side, world, out)
                    });
                }
                Effect::RecheckConnections(mask) => {
                    self.with(pos, |t, world, out| {
                        t.base_recheck_connections(mask, world, out)
                    });
                }
                Effect::RecheckConnection(_) => {}
                Effect::RequestsUpdate => self.log.push((pos, Effect::SendUpdatePacket)),
                other => self.log.push((pos, other)),
            }
        }
    }

    /// `neighborChanged` sem orientação nos vizinhos que são transmissores.
    fn notify_neighbors(&mut self, pos: BlockPos) {
        for side in Face::ALL {
            let neighbor = relative(pos, side);
            self.with(neighbor, |t, world, out| {
                t.on_neighbor_block_change(None, world, out)
            });
        }
    }

    /// A capability do bloco em `pos` mudou: o `BlockCapabilityCache` de cada transmissor vizinho, no lado que o olha, se invalida.
    fn invalidate_capabilities(&mut self, pos: BlockPos) {
        for side in Face::ALL {
            if let Some(neighbor) = self.transmitters.get(&relative(pos, side)) {
                let mut out = Vec::new();
                if let Some(dirty) = neighbor
                    .acceptor_cache
                    .refresh_listener_run(side.opposite(), neighbor.is_valid())
                {
                    neighbor.mark_dirty_acceptor(dirty, &mut out);
                }
            }
        }
    }

    fn set_other(&mut self, pos: BlockPos, other: Option<Other>) {
        match other {
            Some(other) => {
                self.others.insert(pos, other);
            }
            None => {
                self.others.remove(&pos);
            }
        }
        self.invalidate_capabilities(pos);
        self.notify_neighbors(pos);
    }

    fn add_transmitter(&mut self, pos: BlockPos, types: &[TransmissionType]) {
        let mut transmitter = Transmitter::new(pos, TransmissionSet::of(types), true);
        transmitter.set_level(false);
        transmitter.loaded = true;
        self.transmitters.insert(pos, transmitter);
        self.invalidate_capabilities(pos);
        // `onAdded`: relê as conexões. Os vizinhos recebem `neighborChanged` da colocação.
        self.with(pos, |t, world, out| t.refresh_connections(world, out));
        self.notify_neighbors(pos);
    }

    /// Um tick do servidor: `onUpdateServer` de cada transmissor.
    fn tick(&mut self) {
        let positions: Vec<BlockPos> = self.transmitters.keys().copied().collect();
        for pos in positions {
            self.with(pos, |t, world, out| t.on_update_server(world, out));
        }
    }

    fn ticks(&mut self, count: usize) {
        for _ in 0..count {
            self.tick();
        }
    }
}

const ORIGIN: BlockPos = [2, 2, 2];

fn kind_types(kind: &str) -> Option<Vec<TransmissionType>> {
    match kind {
        "mekanism:basic_universal_cable" | "mekanism:advanced_universal_cable" => {
            Some(vec![TransmissionType::Energy])
        }
        "mekanism:basic_mechanical_pipe" | "mekanism:advanced_mechanical_pipe" => {
            Some(vec![TransmissionType::Fluid])
        }
        _ => None,
    }
}

/// Põe um bloco do rastro: um transmissor, um bloco qualquer, ar ou o bloco de redstone.
fn place(sim: &mut Sim, kind: &str, pos: BlockPos) {
    if kind == "air" {
        return;
    }
    if kind == "redstone" {
        sim.set_other(pos, Some(Other::Redstone));
    } else if let Some(types) = kind_types(kind) {
        sim.add_transmitter(pos, &types);
    } else {
        sim.set_other(pos, Some(Other::Block));
    }
}

/// O estado que o rastro grava, lido do transmissor da grade, na mesma ordem em que o teste Java o lê (o `canConnect` lê o redstone antes de `redstone_set`).
fn snapshot(sim: &Sim, pos: BlockPos) -> Value {
    let Some(t) = sim.transmitters.get(&pos) else {
        return serde_json::json!({"present": false});
    };
    let world = View(sim);
    let names = |list: Vec<ConnectionType>| -> Vec<&'static str> {
        list.iter().map(|k| k.serialized_name()).collect()
    };
    let raw = names(
        Face::ALL
            .iter()
            .map(|s| t.get_connection_type_raw(*s))
            .collect(),
    );
    let types = names(
        Face::ALL
            .iter()
            .map(|s| t.get_connection_type(*s))
            .collect(),
    );
    let can_connect: Vec<bool> = Face::ALL
        .iter()
        .map(|s| t.can_connect(*s, &world))
        .collect();
    let to_acceptor: Vec<bool> = Face::ALL
        .iter()
        .map(|s| t.can_connect_to_acceptor(*s))
        .collect();
    let extract: Vec<bool> = Face::ALL.iter().map(|s| t.can_extract(Some(*s))).collect();
    let insert: Vec<bool> = Face::ALL.iter().map(|s| t.can_insert(Some(*s))).collect();
    let powered = t.redstone_powered();
    let set = t.redstone_set();
    let activated = t.is_redstone_activated(&world);
    serde_json::json!({
        "present": true,
        "cur_t": t.current_transmitter_connections,
        "cur_a": t.acceptor_cache.current_acceptor_connections,
        "all": t.get_all_current_connections(),
        "raw": raw,
        "types": types,
        "can_connect": can_connect,
        "can_connect_to_acceptor": to_acceptor,
        "can_extract": extract,
        "can_insert": insert,
        "can_extract_null": t.can_extract(None),
        "can_insert_null": t.can_insert(None),
        "has_pull": t.has_pull_side(),
        "reactive": t.redstone_reactive,
        "powered": powered,
        "redstone_set": set,
        "activated": activated,
    })
}

/// Compara o que o rastro grava (menos `block`, `caps` e `has_network`, que não são da lógica de conexão) com a grade.
fn assert_state(sim: &Sim, pos: BlockPos, expected: &Value, label: &str) {
    let got = snapshot(sim, pos);
    assert_eq!(got["present"], expected["present"], "{label}: presente");
    if expected["present"] == false {
        return;
    }
    for key in [
        "cur_t",
        "cur_a",
        "all",
        "raw",
        "types",
        "can_connect",
        "can_connect_to_acceptor",
        "can_extract",
        "can_insert",
        "can_extract_null",
        "can_insert_null",
        "has_pull",
        "reactive",
        "powered",
        "redstone_set",
        "activated",
    ] {
        assert_eq!(got[key], expected[key], "{label}: {key}");
    }
}

/// As capabilities que o original achou nos lados do transmissor de `pos`.
fn load_caps(sim: &mut Sim, pos: BlockPos, state: &Value) {
    if let Some(caps) = state["caps"].as_array() {
        for (index, side) in Face::ALL.iter().enumerate() {
            sim.caps
                .insert((pos, face_ordinal(*side)), caps[index].as_bool().unwrap());
        }
    }
}

fn pos_at(side: &str) -> BlockPos {
    relative(ORIGIN, face(side))
}

fn transmitter_kind(name: &str) -> &'static str {
    if name.starts_with("basic_universal_cable") {
        "mekanism:basic_universal_cable"
    } else {
        "mekanism:basic_mechanical_pipe"
    }
}

/// O NBT do transmissor no formato do rastro (`write` e o pacote reduzido, este sem a rede).
fn nbt_json(sim: &Sim, pos: BlockPos) -> Value {
    let t = &sim.transmitters[&pos];
    let typed = |compound: Compound| -> Value {
        Value::Object(
            compound
                .into_iter()
                .map(|(key, tag)| {
                    let value = match tag {
                        Tag::Byte(v) => serde_json::json!({ "byte": v }),
                        Tag::IntArray(v) => serde_json::json!({ "int_array": v }),
                        other => panic!("etiqueta {other:?}"),
                    };
                    (key, value)
                })
                .collect(),
        )
    };
    let mut reduced = t.write_reduced_updated_tag();
    reduced.remove("network");
    serde_json::json!({ "write": typed(t.write()), "reduced": typed(reduced) })
}

/// O NBT que o rastro grava só com as quatro chaves da base: o cabo e o cano acrescentam as da própria subclasse (o buffer), que é de outra frente.
fn base_keys(nbt: &Value) -> Value {
    let keep = ["redstone", "connections", "acceptors", "connection"];
    let only = |value: &Value| -> Value {
        Value::Object(
            value
                .as_object()
                .unwrap()
                .iter()
                .filter(|(key, _)| keep.contains(&key.as_str()))
                .map(|(key, v)| (key.clone(), v.clone()))
                .collect(),
        )
    };
    serde_json::json!({ "write": only(&nbt["write"]), "reduced": only(&nbt["reduced"]) })
}

/// A mensagem de overlay como o Java a manda (`translateColored`): o texto em cinza (`EnumColor.GRAY`) com o argumento na cor dele.
fn overlay_message(overlay: &Overlay) -> Value {
    const GRAY: &str = "#CFCFCF";
    const INDIGO: &str = "#559EFF";
    match overlay {
        Overlay::ConnectionType { connection } => {
            let colors = trace("transmitter_types.jsonl");
            let color = colors
                .iter()
                .find(|line| {
                    line["kind"] == "connection_type"
                        && line["serialized"] == connection.serialized_name()
                })
                .map(|line| line["text"]["color"].clone())
                .unwrap();
            serde_json::json!({
                "translate": "configurator.mekanism.mode_change",
                "with": [{"translate": format!("connection.mekanism.{}", connection.lang_name()), "color": color}],
                "color": GRAY,
            })
        }
        Overlay::RedstoneSensitivity { on } => serde_json::json!({
            "translate": "configurator.mekanism.redstone_sensitivity",
            "with": [{"translate": if *on { "boolean.mekanism.on" } else { "boolean.mekanism.off" }, "color": INDIGO}],
            "color": GRAY,
        }),
        Overlay::ToggleColor { .. } | Overlay::CurrentColor { .. } | Overlay::ToggleDiverter { .. } => {
            unreachable!("o transportador não passa pelo rastro das conexões")
        }
    }
}

#[test]
fn neighbors_placed_before_and_after_match_the_original() {
    let mut cases = 0;
    let mut with_acceptor = 0;
    for line in trace("transmitter_neighbors.jsonl") {
        let name = line["case"].as_str().unwrap();
        let neighbor_first = name.ends_with("neighbor_first");
        let mut sim = Sim::new();
        load_caps(&mut sim, ORIGIN, &line["t"]);
        for placement in line["placements"].as_array().unwrap() {
            let side = placement["side"].as_str().unwrap();
            if let Some(state) = line["n"].get(side) {
                load_caps(&mut sim, pos_at(side), state);
            }
        }
        let placements: Vec<(String, String)> = line["placements"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                (
                    p["side"].as_str().unwrap().to_owned(),
                    p["kind"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        let transmitter = transmitter_kind(name);
        if neighbor_first {
            for (side, kind) in &placements {
                place(&mut sim, kind, pos_at(side));
            }
            place(&mut sim, transmitter, ORIGIN);
        } else {
            place(&mut sim, transmitter, ORIGIN);
            sim.ticks(2);
            for (side, kind) in &placements {
                place(&mut sim, kind, pos_at(side));
            }
        }
        sim.ticks(6);
        assert_state(&sim, ORIGIN, &line["t"], name);
        for (side, _) in &placements {
            if let Some(expected) = line["n"].get(side.as_str()) {
                assert_state(
                    &sim,
                    pos_at(side),
                    expected,
                    &format!("{name} vizinho {side}"),
                );
            }
        }
        if line["t"]["cur_a"].as_u64().unwrap() != 0 {
            with_acceptor += 1;
        }
        cases += 1;
    }
    assert!(cases >= 250, "só {cases} casos");
    assert!(
        with_acceptor >= 40,
        "só {with_acceptor} casos com aceitador"
    );
}

#[test]
fn configurator_cycle_matches_the_original() {
    let mut clicks = 0;
    let mut sim = Sim::new();
    let mut scene = String::new();
    for line in trace("transmitter_configurator.jsonl") {
        let name = line["case"].as_str().unwrap();
        let mut parts = name.split('/');
        let transmitter = parts.next().unwrap();
        let setup = parts.next().unwrap();
        let side = parts.next().unwrap();
        let number: usize = parts.next().unwrap().parse().unwrap();
        let sneak = parts.next().unwrap() == "sneak";
        let key = format!("{transmitter}/{setup}/{side}/{sneak}");
        if number == 1 {
            // Uma cena nova: o rastro grava a primeira linha com a cena montada.
            sim = Sim::new();
            scene = key.clone();
            load_caps(&mut sim, ORIGIN, &line["before"]);
            let kind = transmitter_kind(transmitter);
            if setup != "alone" {
                place(&mut sim, kind, pos_at("east"));
                place(&mut sim, "machine", pos_at("up"));
                place(&mut sim, "chest", pos_at("north"));
                place(&mut sim, kind, pos_at("west"));
                place(&mut sim, "stone", pos_at("south"));
            }
            place(&mut sim, kind, ORIGIN);
            sim.ticks(6);
            assert_state(&sim, ORIGIN, &line["before"], &format!("{name}: antes"));
        }
        assert_eq!(scene, key, "{name}: cena");
        sim.log.clear();
        let clicked = face(side);
        let changed = line["changed"].as_array().unwrap();
        if sneak {
            assert_eq!(changed.len(), 1, "{name}: um lado muda");
        }
        let result = if sneak {
            // O olhar vem da célula `aim_from`: se há um braço ali ele é a caixa tocada, senão é o centro (a última caixa).
            let aim_from = if setup == "arm_east" {
                Face::East
            } else {
                clicked
            };
            let (all, aimed) = {
                let t = &sim.transmitters[&ORIGIN];
                let all = t.get_all_current_connections();
                (all, connection_map_contains_side(all, aim_from))
            };
            let box_index = if aimed {
                Face::ALL
                    .iter()
                    .filter(|s| connection_map_contains_side(all, **s))
                    .position(|s| *s == aim_from)
            } else {
                Some(all.count_ones() as usize)
            };
            let hit_side = sim.transmitters[&ORIGIN].side_looking_at(box_index);
            assert_eq!(hit_side.is_some(), aimed, "{name}: braço");
            sim.with(ORIGIN, |t, world, out| {
                t.on_sneak_right_click(clicked, hit_side, |t, _| t.base_on_configure(), world, out)
            })
            .unwrap()
        } else {
            sim.with(ORIGIN, |t, world, out| t.on_right_click(world, out))
                .unwrap()
        };
        let _ = changed;
        let expected_result = &line["result"];
        // O clique agachado e o sem agachar devolvem `Success` (o `consumes_action` do rastro).
        assert_eq!(
            result == InteractionResult::Success,
            expected_result["consumes_action"],
            "{name}: resultado"
        );
        // Os lados que mudaram de tipo cru.
        let after = &line["after"];
        let before = &line["before"];
        let _ = before;
        assert_state(&sim, ORIGIN, after, &format!("{name}: depois"));
        assert_eq!(
            nbt_json(&sim, ORIGIN),
            base_keys(&line["nbt"]),
            "{name}: NBT"
        );
        // A mensagem do overlay.
        let overlays: Vec<&Overlay> = sim
            .log
            .iter()
            .filter_map(|(_, effect)| {
                if let Effect::Overlay(o) = effect {
                    Some(o)
                } else {
                    None
                }
            })
            .collect();
        let expected_overlay = line["overlay"].as_array().unwrap();
        assert_eq!(overlays.len(), expected_overlay.len(), "{name}: mensagens");
        for (overlay, expected) in overlays.iter().zip(expected_overlay) {
            assert_eq!(&overlay_message(overlay), expected, "{name}: texto");
        }
        sim.ticks(4);
        assert_state(
            &sim,
            ORIGIN,
            &line["settled"],
            &format!("{name}: assentado"),
        );
        clicks += 1;
    }
    assert!(clicks >= 200, "só {clicks} cliques");
}

#[test]
fn redstone_reactive_matches_the_original() {
    let mut scenarios = 0;
    let mut steps_seen = 0;
    let mut saw_powered_off = false;
    for line in trace("transmitter_redstone.jsonl") {
        let name = line["case"].as_str().unwrap();
        let mut parts = name.split('/');
        let transmitter = parts.next().unwrap();
        let power = parts.next().unwrap();
        let kind = transmitter_kind(transmitter);
        let steps = line["steps"].as_array().unwrap();
        let mut sim = Sim::new();
        load_caps(&mut sim, ORIGIN, &steps[0]["t"]);
        load_caps(&mut sim, pos_at("east"), &steps[0]["east"]);
        place(&mut sim, kind, pos_at("east"));
        place(
            &mut sim,
            if kind.contains("cable") {
                "machine"
            } else {
                "tank"
            },
            pos_at("west"),
        );
        place(&mut sim, "chest", pos_at("up"));
        place(&mut sim, kind, ORIGIN);
        sim.ticks(6);
        for step in steps {
            let what = step["step"].as_str().unwrap();
            sim.log.clear();
            match what {
                "start" => {}
                "power_on" => sim.set_other(pos_at(power), Some(Other::Redstone)),
                "power_off" => sim.set_other(pos_at(power), None),
                "toggle" => {
                    sim.with(ORIGIN, |t, world, out| t.on_right_click(world, out));
                }
                other => panic!("passo {other}"),
            }
            sim.ticks(4);
            let label = format!("{name} {what}");
            assert_state(&sim, ORIGIN, &step["t"], &label);
            assert_eq!(
                nbt_json(&sim, ORIGIN),
                base_keys(&step["nbt"]),
                "{label}: NBT"
            );
            assert_state(
                &sim,
                pos_at("east"),
                &step["east"],
                &format!("{label} leste"),
            );
            let overlays: Vec<Value> = sim
                .log
                .iter()
                .filter_map(|(_, e)| match e {
                    Effect::Overlay(o) => Some(overlay_message(o)),
                    _ => None,
                })
                .collect();
            assert_eq!(
                Value::Array(overlays),
                step["overlay"],
                "{label}: mensagens"
            );
            if what == "power_off" && step["t"]["reactive"] == true {
                saw_powered_off = true;
            }
            steps_seen += 1;
        }
        scenarios += 1;
    }
    assert!(scenarios >= 12, "só {scenarios} cenários");
    assert!(steps_seen >= 80);
    assert!(saw_powered_off);
}
