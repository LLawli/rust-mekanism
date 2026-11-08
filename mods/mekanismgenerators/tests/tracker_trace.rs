//! Repete, contra o original, o que o `MekanismContainer.broadcastChanges` manda a um jogador com a tela do Heat Generator aberta, tick a
//! tick (`dev/referencia/testes/TrackerTraceTest.java`, rastro `heat_generator_trackers`): o `sendAllDataToRemote` ao abrir, o
//! `startTrackingServer` da janela de upgrades e o conjunto sujo de cada tick (índice, classe, tipo de sujeira e valor). O gerador
//! queima lava, esquenta e gera, muda de modo de redstone e instala um upgrade de silêncio com a janela aberta e fechada.
mod common;

use machinery_basis_core::container_sync::{
    ContainerSync, DirtyType, Kind as DataKind, Mirror, Property, PropertyData, wire,
};
use machinery_basis_core::containers::AutomationType;
use machinery_basis_core::tile::RedstoneControl;
use machinery_basis_core::tile_trackers::UPGRADE_WINDOW_KEY;
use machinery_basis_core::transaction::Transaction;
use machinery_basis_core::upgrade::UpgradeComponent;
use machinery_basis_core::upgrade_window::UpgradeSync;
use mekanismgenerators_rules::{
    FuelItem, HeatGeneratorSync, Produced, State, Surroundings, fuel_to_tank,
    heat_generator_trackers, step, upgrade_window_data,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;

const SCENARIO: &str = "heat_generator_trackers";
const MINIMUM_LINES: usize = 300;

/// `Items.COAL` e `Items.LAVA_BUCKET` no `FuelValues` do jogo.
const COAL_BURN_TIME: i32 = 1_600;
const LAVA_BUCKET_BURN_TIME: i32 = 20_000;

/// O slot de combustível, como o rastro o grava: o caminho do item e a quantidade.
#[derive(Clone, Debug, PartialEq)]
struct Slot {
    item: String,
    count: u64,
}

impl Slot {
    fn empty() -> Self {
        Self {
            item: String::new(),
            count: 0,
        }
    }

    fn fuel(&self) -> Option<FuelItem> {
        match self.item.as_str() {
            "lava_bucket" => Some(FuelItem::LavaBucket),
            "coal" => Some(FuelItem::Solid {
                burn_time: COAL_BURN_TIME,
                lava_bucket_burn_time: LAVA_BUCKET_BURN_TIME,
            }),
            _ => None,
        }
    }
}

/// `FluidFuelInventorySlot.fillOrBurn`, antes da regra do tick, como o `onUpdateServer` do tile.
fn fill_or_burn(state: &mut State, slot: &mut Slot) {
    let Some(item) = slot.fuel() else {
        return;
    };
    let room = mekanismgenerators_rules::TANK_CAPACITY - state.lava;
    let Some(lava) = fuel_to_tank(item, room) else {
        return;
    };
    state.lava += lava;
    if item == FuelItem::LavaBucket {
        slot.item = "bucket".to_owned();
    } else {
        slot.count -= 1;
        if slot.count == 0 {
            *slot = Slot::empty();
        }
    }
}

/// O gerador e o menu do jogador.
struct Scene {
    state: State,
    slot: Slot,
    upgrades: UpgradeComponent,
    /// O que o último tick produziu: `producingEnergy`, `lastTransferLoss` e `lastEnvironmentLoss` ficam nos campos do tile.
    last: Produced,
    sync: Option<ContainerSync<u8>>,
    /// O menu do mesmo jogador no cliente: o que o servidor manda chega a ele em bytes, e cada dado vai para a posição dele.
    client: Option<Mirror>,
    sent: Vec<Property>,
}

impl Scene {
    fn view(&self) -> UpgradeSync {
        UpgradeSync::of(&self.upgrades)
    }

    fn open(&mut self) {
        let mut sync = ContainerSync::new(&heat_generator_trackers());
        let upgrades = self.view();
        self.sent.extend(sync.send_all(&HeatGeneratorSync {
            state: &self.state,
            last: &self.last,
            upgrades: &upgrades,
        }));
        self.sync = Some(sync);
        self.client = Some(Mirror::new(&heat_generator_trackers()));
    }

    fn open_window(&mut self) {
        let mut sync = self.sync.take().expect("sem menu aberto");
        let upgrades = self.view();
        self.sent.extend(sync.start_tracking_server(
            UPGRADE_WINDOW_KEY,
            &upgrade_window_data(),
            &HeatGeneratorSync {
                state: &self.state,
                last: &self.last,
                upgrades: &upgrades,
            },
        ));
        self.sync = Some(sync);
        // O cliente acrescenta os dados da janela no mesmo lugar (`GuiUpgradeWindow` chama `startTracking`).
        self.client
            .as_mut()
            .expect("sem menu aberto")
            .start_tracking(UPGRADE_WINDOW_KEY, &upgrade_window_data());
    }

    fn click_upgrade_slot(&self, name: &str, count: u32) {
        let resource = self
            .upgrades
            .data()
            .resource_of(&format!("mekanism:{name}"));
        let transaction = Transaction::open_root();
        let inserted = self.upgrades.upgrade_slot().insert(
            &resource,
            count,
            &transaction,
            AutomationType::Manual,
        );
        if inserted > 0 {
            transaction.commit();
        }
    }

    fn apply(&mut self, action: &str) {
        let words: Vec<&str> = action.split_whitespace().collect();
        match words.as_slice() {
            ["open"] => self.open(),
            ["close"] => {
                self.sync = None;
                self.client = None;
            }
            ["window", "upgrade", ..] => self.open_window(),
            ["stop", "tracking"] => {
                self.sync
                    .as_mut()
                    .expect("sem menu aberto")
                    .stop_tracking(&UPGRADE_WINDOW_KEY);
                self.client
                    .as_mut()
                    .expect("sem menu aberto")
                    .stop_tracking(&UPGRADE_WINDOW_KEY);
            }
            ["lava", amount] => self.state.lava = amount.parse().unwrap(),
            ["energy", amount] => self.state.energy = amount.parse().unwrap(),
            ["fuel", item, count] => {
                self.slot = Slot {
                    item: (*item).to_owned(),
                    count: count.parse().unwrap(),
                }
            }
            ["mode", mode] => self.state.tile.set_control_type(match *mode {
                "disabled" => RedstoneControl::Disabled,
                "high" => RedstoneControl::High,
                "low" => RedstoneControl::Low,
                "pulse" => RedstoneControl::Pulse,
                other => panic!("modo desconhecido: {other}"),
            }),
            ["signal", "on"] => self.state.tile.update_power(true),
            ["signal", "off"] => self.state.tile.update_power(false),
            ["slot", name, count] => self.click_upgrade_slot(name, count.parse().unwrap()),
            other => panic!("marca desconhecida: {other:?}"),
        }
    }
}

fn expected_entry(entry: &Value) -> Value {
    let [index, class, dirty, value] = entry.as_array().unwrap().as_slice() else {
        panic!("entrada fora de forma: {entry}");
    };
    let class = class.as_str().unwrap().split('$').next().unwrap();
    let value = if class == "SyncableStreamCodec" {
        bytes_json(&hex_bytes(value.as_str().unwrap()))
    } else {
        value.clone()
    };
    json!([index, class, dirty, value])
}

fn hex_bytes(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
        .collect()
}

/// Veja `mods/mekanism/tests/tracker_trace.rs`: o mapa de upgrades vira o conjunto de pares em ordem, porque o Java o escreve na ordem
/// do hash, que muda de uma execução para a outra.
fn bytes_json(bytes: &[u8]) -> Value {
    if bytes.len() <= 1 {
        return Value::String(bytes.iter().map(|byte| format!("{byte:02x}")).collect());
    }
    let mut at = 0;
    let mut var_int = || {
        let mut value = 0u32;
        let mut shift = 0;
        loop {
            let byte = bytes[at];
            at += 1;
            value |= u32::from(byte & 0x7F) << shift;
            if byte & 0x80 == 0 {
                return value;
            }
            shift += 7;
        }
    };
    let size = var_int();
    let mut pairs: Vec<(u32, u32)> = (0..size).map(|_| (var_int(), var_int())).collect();
    pairs.sort_unstable();
    assert_eq!(at, bytes.len(), "bytes do mapa sobrando");
    Value::String(format!("map:{pairs:?}"))
}

fn actual_entry(property: &Property) -> Value {
    let dirty = match property.dirty {
        DirtyType::Dirty => "DIRTY",
        DirtyType::Size => "SIZE",
        DirtyType::Clean => panic!("um dado limpo não é mandado"),
    };
    let value = match &property.data {
        PropertyData::Boolean(value) => json!(value),
        PropertyData::Int(value) => json!(value),
        PropertyData::Long(value) => json!(value),
        PropertyData::Double(value) => json!(value),
        PropertyData::Stack(_, stack) => json!({"resource": stack.resource, "amount": stack.amount}),
        PropertyData::Bytes(bytes) => bytes_json(bytes),
    };
    json!([property.index, property.class, dirty, value])
}

fn read_trace(scenario: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(format!("{scenario}.jsonl"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// O gerador no estado do primeiro tick gravado.
fn scene_from(first: &Value) -> Scene {
    let mut state = State::new(first["ambient"].as_f64().unwrap());
    // O primeiro tick gravado já aconteceu quando o cenário começa: o calor dele semeia o estado, e a regra roda a partir do seguinte.
    state.heat.heat = first["heat"].as_f64().unwrap();
    Scene {
        state,
        slot: Slot::empty(),
        upgrades: UpgradeComponent::new(
            common::upgrade_data(),
            Rc::new(|| {}),
            Box::new(|_, _, _| {}),
        ),
        last: Produced {
            energy: first["produced"].as_i64().unwrap() as i32,
            transfer_loss: first["transfer_loss"].as_f64().unwrap(),
            environment_loss: first["environment_loss"].as_f64().unwrap(),
            ..Produced::default()
        },
        sync: None,
        client: None,
        sent: Vec::new(),
    }
}

/// O pacote do tick: o que o servidor mandou, em bytes, e o `handleWindowProperty` de cada dado no menu do cliente.
fn deliver(client: &mut Mirror, properties: &[Property]) {
    if properties.is_empty() {
        return;
    }
    let packet = wire::Packet::new([0, 64, 0], None, properties);
    let packet = wire::decode(&wire::encode(&packet)).expect("o pacote volta");
    for (index, data) in &packet.data {
        client.apply(*index, data);
    }
}

/// O que o cliente do Java guarda de cada dado depois de aplicar o que o rastro diz que o servidor mandou, na forma do rastro. Uma pilha
/// mandada só pelo tamanho muda a quantidade da que ele já tem (`SyncableLargeResourceStack.set(long)`).
fn java_client_applies(view: &mut BTreeMap<u64, Value>, entry: &Value) {
    let [index, class, dirty, value] = entry.as_array().unwrap().as_slice() else {
        panic!("entrada fora de forma: {entry}");
    };
    let index = index.as_u64().unwrap();
    let class = class.as_str().unwrap().split('$').next().unwrap();
    if class == "SyncableLargeResourceStack" && dirty == "SIZE" {
        if let Some(stack) = view.get_mut(&index)
            && stack["resource"] != ""
        {
            *stack = if value.as_u64() == Some(0) {
                json!({"resource": "", "amount": 0})
            } else {
                json!({"resource": stack["resource"], "amount": value})
            };
        }
        return;
    }
    view.insert(index, expected_entry(entry)[3].clone());
}

/// O valor de um dado do espelho, na forma do rastro.
fn mirror_value(mirror: &Mirror, position: usize, kind: DataKind) -> Value {
    match kind {
        DataKind::Boolean => json!(mirror.boolean(position)),
        DataKind::Int => json!(mirror.int(position)),
        DataKind::Enum { .. } => json!(mirror.enum_ordinal(position)),
        DataKind::Long => json!(mirror.long(position)),
        DataKind::Double => json!(mirror.double(position)),
        DataKind::LargeResourceStack(_) => {
            let stack = mirror.stack(position);
            json!({"resource": stack.resource, "amount": stack.amount})
        }
        DataKind::StreamCodec => bytes_json(mirror.bytes(position)),
    }
}

#[test]
fn the_heat_generator_container_sends_what_the_original_sends() {
    let lines = read_trace(SCENARIO);
    assert!(
        lines.len() >= MINIMUM_LINES,
        "rastro curto demais: {}",
        lines.len()
    );
    assert_eq!(
        lines[0]["epsilon"].as_f64().unwrap(),
        machinery_basis_core::heat::EPSILON,
        "o limite de troca de calor"
    );
    let mut scene = scene_from(&lines[0]);
    let (mut sent_lines, mut dirty_lines, mut compared) = (0, 0, 0);
    let mut view: BTreeMap<u64, Value> = BTreeMap::new();
    for expected in &lines {
        let t = expected["t"].as_u64().unwrap();
        if t > 0 {
            // `TileEntityMekanism.tickServer`: o componente de upgrades anda primeiro, e `onUpdateServer` depois.
            scene.upgrades.tick_server();
            let around = Surroundings {
                lava_faces: expected["lava_faces"].as_u64().unwrap() as u32,
                in_nether: false,
                ambient: expected["ambient"].as_f64().unwrap(),
                face_ambient: expected["ambient_faces"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|face| face.as_f64().unwrap())
                    .collect::<Vec<f64>>()
                    .try_into()
                    .unwrap(),
            };
            fill_or_burn(&mut scene.state, &mut scene.slot);
            scene.last = step(&mut scene.state, &around);
            scene.state.tile.finish_tick(|| 0);
        }
        for mark in expected["marks"].as_array().unwrap() {
            for action in mark.as_str().unwrap().split(", ") {
                scene.apply(action);
            }
        }
        // `ServerTickEvent.Post`: o `broadcastChanges`.
        let upgrades = scene.view();
        let dirty = match scene.sync.as_mut() {
            Some(sync) => sync.broadcast_changes(&HeatGeneratorSync {
                state: &scene.state,
                last: &scene.last,
                upgrades: &upgrades,
            }),
            None => Vec::new(),
        };
        let sent_properties: Vec<Property> = std::mem::take(&mut scene.sent);
        // O servidor manda ao jogador os dados da abertura e depois os sujos; o cliente os aplica nessa ordem.
        if let Some(client) = scene.client.as_mut() {
            deliver(client, &sent_properties);
            deliver(client, &dirty);
        }
        let actual_sent: Vec<Value> = sent_properties.iter().map(actual_entry).collect();
        let actual_dirty: Vec<Value> = dirty.iter().map(actual_entry).collect();
        let expected_sent: Vec<Value> = expected["sent"]
            .as_array()
            .unwrap()
            .iter()
            .map(expected_entry)
            .collect();
        let expected_dirty: Vec<Value> = expected["dirty"]
            .as_array()
            .unwrap()
            .iter()
            .map(expected_entry)
            .collect();
        assert_eq!(
            actual_sent, expected_sent,
            "{SCENARIO}, tick {t}: o que foi mandado ao abrir"
        );
        assert_eq!(
            actual_dirty, expected_dirty,
            "{SCENARIO}, tick {t}: o conjunto sujo ({})",
            expected["marks"]
        );
        if let Some(tracked) = expected.get("tracked") {
            assert_eq!(
                scene.sync.as_ref().map(ContainerSync::count),
                tracked.as_u64().map(|count| count as usize),
                "{SCENARIO}, tick {t}: quantos dados o menu sincroniza"
            );
        }
        // O que a tela lê do espelho é o que o cliente do Java leria: cada dado que o rastro mandou, no valor e na classe que o cliente dele tem.
        if scene.client.is_none() {
            view.clear();
        }
        for entry in expected["sent"]
            .as_array()
            .unwrap()
            .iter()
            .chain(expected["dirty"].as_array().unwrap())
        {
            java_client_applies(&mut view, entry);
        }
        if let (Some(client), Some(sync)) = (scene.client.as_ref(), scene.sync.as_ref()) {
            let tracked = sync.tracked();
            view.retain(|index, _| (*index as usize) < tracked.len());
            for (index, wanted) in &view {
                let position = *index as usize;
                assert_eq!(
                    &mirror_value(client, position, tracked[position].kind),
                    wanted,
                    "{SCENARIO}, tick {t}: o que a tela lê do dado {index} ({})",
                    tracked[position].kind.class()
                );
                compared += 1;
            }
        }
        sent_lines += usize::from(!expected_sent.is_empty());
        dirty_lines += usize::from(!expected_dirty.is_empty());
    }
    assert!(sent_lines >= 2, "{sent_lines} envios iniciais");
    assert!(dirty_lines >= 150, "{dirty_lines} ticks sujos");
    assert!(compared >= 2_000, "{compared} leituras do espelho");
}

#[test]
fn the_declared_list_has_the_size_and_classes_the_original_measured() {
    let first = &read_trace("heat_generator_container_tracking")[0];
    let wanted: Vec<&str> = first["tracked"]
        .as_array()
        .unwrap()
        .iter()
        .map(|class| class.as_str().unwrap().split('$').next().unwrap())
        .collect();
    let declared: Vec<&str> = heat_generator_trackers()
        .iter()
        .map(|tracked| tracked.kind.class())
        .collect();
    assert_eq!(declared, wanted);
}

/// `redstoneLastTick` (posição 4, `WAS_POWERED`) é gravado no fim do `tickServer` (`TileEntityMekanism.java:652-655`): vira verdadeiro só no
/// tick seguinte ao sinal ligar, e falso no seguinte ao sinal desligar. Aqui só o fim de tick real do gerador, sem o menu: o valor do tile
/// confere com o último que o rastro mandou a cada tick.
#[test]
fn redstone_last_tick_follows_the_signal_one_tick_later() {
    let lines = read_trace(SCENARIO);
    let mut scene = scene_from(&lines[0]);
    let mut expected = false;
    let mut transitions = Vec::new();
    for line in &lines {
        let t = line["t"].as_u64().unwrap();
        if t > 0 {
            scene.state.tile.finish_tick(|| 0);
        }
        for mark in line["marks"].as_array().unwrap() {
            for action in mark.as_str().unwrap().split(", ") {
                if action.starts_with("signal") || action.starts_with("mode") {
                    scene.apply(action);
                }
            }
        }
        let sent = line["sent"].as_array().unwrap();
        let dirty = line["dirty"].as_array().unwrap();
        for entry in sent.iter().chain(dirty) {
            if entry[0] == 4 {
                expected = entry[3].as_bool().unwrap();
                transitions.push(t);
            }
        }
        let tile = scene.state.tile.redstone_sync();
        assert_eq!(tile.was_powered, expected, "tick {t}: redstoneLastTick");
    }
    // O sinal liga e desliga três vezes depois do envio inicial: cada mudança de `powered` chega a `wasPowered` um tick depois.
    assert_eq!(transitions, [3, 215, 219, 227, 231, 235, 239]);
}
