//! Repete, contra o original, o que o `MekanismContainer.broadcastChanges` manda a um jogador com a tela aberta, tick a tick, nas
//! quatro máquinas de progresso (`dev/referencia/testes/TrackerTraceTest.java`): o que o `sendAllDataToRemote` manda ao abrir, o que
//! o `startTrackingServer` da janela de upgrades manda, e a cada tick o conjunto sujo (índice, classe, tipo de sujeira e valor).
//!
//! O Java chama o `broadcastChanges` em `ServerTickEvent.Post`, depois do tick do bloco e do que o cenário faz nele; o teste faz o
//! mesmo no `finish_tick` do mundo simulado.
mod common;

use machinery_basis_core::cached_recipe::{ItemIngredient, ItemOutput, ItemToItemRecipe};
use machinery_basis_core::chemical::{ChemicalConversion, ChemicalResource};
use machinery_basis_core::container_sync::{
    ContainerSync, DirtyType, Kind as DataKind, Mirror, Property, PropertyData, Tracked, wire,
};
use machinery_basis_core::containers::{AutomationType, ItemResource};
use machinery_basis_core::item_handler::ItemCapabilities;
use machinery_basis_core::machine_sync::MachineSync;
use machinery_basis_core::sim::{SimBlockEntity, SimWorld};
use machinery_basis_core::tile::{ProgressMachine, RedstoneControl};
use machinery_basis_core::tile_trackers::UPGRADE_WINDOW_KEY;
use machinery_basis_core::transaction::Transaction;
use mekanism_rules::{
    CRUSHER_USAGE, ENRICHMENT_USAGE, INFUSER_USAGE, SMELTER_USAGE, electric_config,
    electric_trackers, infuser_config, infuser_trackers, infusing_cache_of, upgrade_window_data,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;

const MINIMUM_LINES: usize = 400;

/// A máquina e o `game_time` do mundo em que o rastro foi gravado: o recheck de erros depende de `game_time % 100`.
struct Sim(ProgressMachine, u64);

impl SimBlockEntity for Sim {
    fn tick_server(&mut self, game_time: u64, neighbours: &dyn ItemCapabilities) {
        self.0.tick_server(game_time + self.1, neighbours);
    }

    fn neighbor_changed(&mut self, powered: bool) {
        self.0.tile_mut().update_power(powered);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Chamber,
    Crusher,
    Smelter,
    Infuser,
}

impl Kind {
    fn scenario(self) -> &'static str {
        match self {
            Self::Chamber => "chamber_trackers",
            Self::Crusher => "crusher_trackers",
            Self::Smelter => "smelter_trackers",
            Self::Infuser => "infuser_trackers",
        }
    }

    fn trackers(self) -> Vec<Tracked> {
        match self {
            Self::Infuser => infuser_trackers(),
            _ => electric_trackers(),
        }
    }
}

fn item(id: &str) -> ItemResource {
    ItemResource::new(id, 64)
}

fn world(kind: Kind, offset: u64, base: u64) -> SimWorld<Sim> {
    let machine = match kind {
        Kind::Chamber | Kind::Crusher | Kind::Smelter => {
            let (usage, recipes) = match kind {
                Kind::Chamber => (
                    ENRICHMENT_USAGE,
                    common::item_recipes_of_type("mekanism:enriching"),
                ),
                Kind::Crusher => (
                    CRUSHER_USAGE,
                    common::item_recipes_of_type("mekanism:crushing"),
                ),
                _ => (
                    SMELTER_USAGE,
                    vec![Rc::new(ItemToItemRecipe {
                        id: "minecraft:iron_ingot_from_smelting_raw_iron".into(),
                        input: ItemIngredient::item("raw_iron"),
                        input_count: 1,
                        output: ItemOutput {
                            resource: item("iron_ingot"),
                            count: 1,
                        },
                    })],
                ),
            };
            let mut config = electric_config(
                usage,
                ItemToItemRecipe::cache_of(recipes),
                offset,
                common::upgrade_data(),
            );
            config.energy_conversion = common::energy_conversions();
            ProgressMachine::electric(config)
        }
        Kind::Infuser => {
            let (recipes, conversions) = common::infuser_recipes_by_type();
            let mut config = infuser_config(
                INFUSER_USAGE,
                infusing_cache_of(recipes),
                ChemicalConversion::cache_of(conversions),
                offset,
                common::upgrade_data(),
            );
            config.energy_conversion = common::energy_conversions();
            ProgressMachine::chemical(config)
        }
    };
    SimWorld::new(Sim(machine, base))
}

/// O menu do jogador e o que o servidor mandou a ele nesse tick.
struct Scene {
    world: SimWorld<Sim>,
    kind: Kind,
    base: u64,
    sync: Option<ContainerSync<u8>>,
    /// O menu do mesmo jogador no cliente: o que o servidor manda chega a ele em bytes, e cada dado vai para a posição dele.
    client: Option<Mirror>,
    window: bool,
    sent: Vec<Property>,
    /// O que o último `insert` deixou entrar, à espera de a marca `inserted` do Java conferir.
    inserted: Option<i32>,
}

impl Scene {
    fn machine(&self) -> &ProgressMachine {
        &self.world.block_entity.0
    }

    fn game_time(&self) -> u64 {
        self.world.game_time() + self.base
    }

    fn open(&mut self) {
        self.machine().open();
        let mut sync = ContainerSync::new(&self.kind.trackers());
        self.sent
            .extend(sync.send_all(&MachineSync::of(self.machine())));
        self.sync = Some(sync);
        self.client = Some(Mirror::new(&self.kind.trackers()));
    }

    fn close(&mut self) {
        self.machine().close();
        self.sync = None;
        self.client = None;
        self.window = false;
    }

    /// `PacketWindowSelect` e `CONTAINER_TRACK_UPGRADES`.
    fn open_window(&mut self) {
        self.window = true;
        let mut sync = self.sync.take().expect("sem menu aberto");
        let sent = sync.start_tracking_server(
            UPGRADE_WINDOW_KEY,
            &upgrade_window_data(),
            &MachineSync::of(self.machine()),
        );
        self.sent.extend(sent);
        self.sync = Some(sync);
        // O cliente acrescenta os dados da janela no mesmo lugar (`GuiUpgradeWindow` chama `startTracking`).
        self.client
            .as_mut()
            .expect("sem menu aberto")
            .start_tracking(UPGRADE_WINDOW_KEY, &upgrade_window_data());
    }

    /// `CONTAINER_STOP_TRACKING` e a janela deselecionada.
    fn close_window(&mut self) {
        self.window = false;
        self.client
            .as_mut()
            .expect("sem menu aberto")
            .stop_tracking(&UPGRADE_WINDOW_KEY);
        self.sync
            .as_mut()
            .expect("sem menu aberto")
            .stop_tracking(&UPGRADE_WINDOW_KEY);
    }

    /// Um clique com o item de upgrade no cursor no slot virtual: o que cabe entra.
    fn click_upgrade_slot(&self, name: &str, count: u32) {
        let machine = self.machine();
        let resource = machine
            .upgrades()
            .data()
            .resource_of(&format!("mekanism:{name}"));
        let transaction = Transaction::open_root();
        let inserted = machine.upgrades().upgrade_slot().insert(
            &resource,
            count,
            &transaction,
            AutomationType::Manual,
        );
        if inserted > 0 {
            transaction.commit();
        }
    }

    /// A energia pela capability: o que o contêiner deixa entrar passa pelo `TrackingEnergyHandler`.
    fn insert_energy(&self, amount: i32) -> i32 {
        let machine = self.machine();
        let transaction = Transaction::open_root();
        let inserted = machine
            .energy()
            .insert(amount, &transaction, AutomationType::External);
        if inserted > 0 {
            transaction.commit();
        }
        machine.energy_received(i64::from(inserted), self.game_time());
        inserted
    }

    /// Uma parte de uma marca do rastro: é o que o cenário em Java fez depois daquele tick.
    fn apply(&mut self, action: &str) {
        let parts: Vec<&str> = action.split_whitespace().collect();
        match parts.as_slice() {
            ["open"] => self.open(),
            ["close"] => self.close(),
            ["window", "upgrade", ..] => self.open_window(),
            ["stop", "tracking"] => self.close_window(),
            ["energy", amount] => self
                .machine()
                .energy()
                .set_energy(amount.parse().unwrap(), None),
            ["insert", amount] => {
                let amount: i32 = amount.parse().unwrap();
                self.inserted = Some(self.insert_energy(amount));
            }
            // O quanto a capability deixou entrar no Java tem de ser o mesmo que o contêiner deixa aqui.
            ["debug", ..] => {}
            ["inserted", accepted, ..] => {
                assert_eq!(
                    self.inserted.take(),
                    Some(accepted.parse().unwrap()),
                    "energia aceita pela capability"
                );
            }
            ["item", id, amount] => {
                self.machine()
                    .input()
                    .set_contents(Some(item(id)), amount.parse().unwrap(), None);
            }
            ["tank", id, amount] => {
                let amount: u64 = amount.parse().unwrap();
                let resource = (amount > 0).then(|| ChemicalResource::new(id));
                self.machine()
                    .chemical_side()
                    .unwrap()
                    .tank
                    .set_contents(resource, amount, None);
            }
            ["infusion", id, amount] => {
                self.machine().chemical_side().unwrap().slot.set_contents(
                    Some(item(id)),
                    amount.parse().unwrap(),
                    None,
                );
            }
            ["mode", mode] => self.machine().tile_mut().set_control_type(match *mode {
                "disabled" => RedstoneControl::Disabled,
                "high" => RedstoneControl::High,
                "low" => RedstoneControl::Low,
                "pulse" => RedstoneControl::Pulse,
                other => panic!("modo desconhecido: {other}"),
            }),
            ["signal", "on"] => self.world.set_signal(true),
            ["signal", "off"] => self.world.set_signal(false),
            ["slot", name, count] => self.click_upgrade_slot(name, count.parse().unwrap()),
            other => panic!("marca desconhecida: {other:?}"),
        }
    }
}

// ------------------------------------------------------------------------------------------------------------ comparação

/// Os bytes de um `ByteArrayPropertyData` como o teste compara. O mapa de upgrades sai do `Object2IntOpenHashMap` na ordem do hash,
/// que muda de uma execução do Java para a outra, então vira o conjunto de pares (id do registro, contagem) em ordem; o resto é
/// hexadecimal.
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

fn hex_bytes(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
        .collect()
}

/// Uma entrada do rastro (`[índice, classe, "DIRTY"|"SIZE", valor]`) na forma que o teste compara: sem o sufixo `$N` da classe anônima.
fn expected_entry(entry: &Value) -> Value {
    let [index, class, dirty, value] = entry.as_array().unwrap().as_slice() else {
        panic!("entrada fora de forma: {entry}");
    };
    let class = class.as_str().unwrap().split('$').next().unwrap();
    let value = if class == "SyncableStreamCodec" {
        bytes_json(&hex_bytes(value.as_str().unwrap()))
    } else if class == "SyncableLargeResourceStack" && value.is_object() {
        // Os ids de químico da simulação não levam o namespace do mod.
        let resource = value["resource"].as_str().unwrap();
        json!({"resource": resource.trim_start_matches("mekanism:"), "amount": value["amount"]})
    } else {
        value.clone()
    };
    json!([index, class, dirty, value])
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
        .join(format!("../../dev/referencia/rastros/{scenario}.jsonl"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
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

/// O que o cliente do Java guarda de cada dado depois de aplicar o que o rastro diz que o servidor mandou: o valor na forma do rastro. Uma
/// pilha mandada só pelo tamanho muda a quantidade da que ele já tem (`SyncableLargeResourceStack.set(long)`).
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

fn replay(kind: Kind) {
    let scenario = kind.scenario();
    let lines = read_trace(scenario);
    assert!(
        lines.len() >= MINIMUM_LINES,
        "rastro curto demais: {}",
        lines.len()
    );
    let offset = lines[0]["recheck_offset"].as_u64().unwrap();
    let base = lines[0]["game_time"].as_u64().unwrap() - lines[0]["t"].as_u64().unwrap();
    let mut scene = Scene {
        world: world(kind, offset, base),
        kind,
        base,
        sync: None,
        client: None,
        window: false,
        sent: Vec::new(),
        inserted: None,
    };
    let (mut sent_lines, mut dirty_lines, mut compared) = (0, 0, 0);
    let mut view: BTreeMap<u64, Value> = BTreeMap::new();
    for expected in &lines {
        let t = expected["t"].as_u64().unwrap();
        assert_eq!(
            scene.game_time(),
            expected["game_time"].as_u64().unwrap(),
            "{scenario}, tick {t}"
        );
        // O tick do nível, o que o cenário fez depois dele e o `ServerTickEvent.Post`, onde o `broadcastChanges` roda.
        scene.world.tick_level();
        for mark in expected["marks"].as_array().unwrap() {
            for action in mark.as_str().unwrap().split(", ") {
                scene.apply(action);
            }
        }
        scene.world.finish_tick();
        let dirty: Vec<Property> = match scene.sync.as_mut() {
            Some(sync) => sync.broadcast_changes(&MachineSync::of(&scene.world.block_entity.0)),
            None => Vec::new(),
        };
        let sent_properties: Vec<Property> = std::mem::take(&mut scene.sent);
        // O servidor manda ao jogador os dados da abertura e depois os sujos; o cliente de cada lado os aplica nessa ordem.
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
            "{scenario}, tick {t}: o que foi mandado ao abrir"
        );
        assert_eq!(
            actual_dirty, expected_dirty,
            "{scenario}, tick {t}: o conjunto sujo ({:?})",
            expected["marks"]
        );
        if let Some(tracked) = expected.get("tracked") {
            assert_eq!(
                scene.sync.as_ref().map(ContainerSync::count),
                tracked.as_u64().map(|count| count as usize),
                "{scenario}, tick {t}: quantos dados o menu sincroniza"
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
                    "{scenario}, tick {t}: o que a tela lê do dado {index} ({})",
                    tracked[position].kind.class()
                );
                compared += 1;
            }
        }
        sent_lines += usize::from(!expected_sent.is_empty());
        dirty_lines += usize::from(!expected_dirty.is_empty());
    }
    // O rastro tem de ter o que o teste confere: aberturas e muitos ticks sujos.
    assert!(sent_lines >= 3, "{scenario}: {sent_lines} envios iniciais");
    assert!(dirty_lines >= 150, "{scenario}: {dirty_lines} ticks sujos");
    assert!(
        compared >= 4_000,
        "{scenario}: {compared} leituras do espelho"
    );
}

#[test]
fn the_enrichment_chamber_container_sends_what_the_original_sends() {
    replay(Kind::Chamber);
}

#[test]
fn the_crusher_container_sends_what_the_original_sends() {
    replay(Kind::Crusher);
}

#[test]
fn the_energized_smelter_container_sends_what_the_original_sends() {
    replay(Kind::Smelter);
}

#[test]
fn the_metallurgic_infuser_container_sends_what_the_original_sends() {
    replay(Kind::Infuser);
}

/// A lista que o teste de cima confere é a que o mod declara, e tem o tamanho que os rastros `*_container_tracking` mediram.
#[test]
fn the_declared_lists_have_the_sizes_the_original_measured() {
    assert_eq!(electric_trackers().len(), 16);
    assert_eq!(infuser_trackers().len(), 18);
}

/// Os `*_container_tracking` gravam a classe de cada posição (o que `trackedData` tem); a lista do mod tem de ter as mesmas.
#[test]
fn the_declared_classes_match_the_container_tracking_traces() {
    for (scenario, list) in [
        ("chamber_container_tracking", electric_trackers()),
        ("infuser_container_tracking", infuser_trackers()),
    ] {
        let first = &read_trace(scenario)[0];
        let wanted: Vec<&str> = first["tracked"]
            .as_array()
            .unwrap()
            .iter()
            .map(|class| class.as_str().unwrap().split('$').next().unwrap())
            .collect();
        let declared: Vec<&str> = list.iter().map(|tracked| tracked.kind.class()).collect();
        assert_eq!(declared, wanted, "{scenario}");
    }
}

/// No jogo a energia vem do block entity, que pode passar da capacidade do objeto (uma chegada de fora do tick, `setEnergy` além do
/// máximo): `getCapacityAsLong` nunca fica abaixo dela, e o dado de capacidade acompanha.
#[test]
fn the_capacity_data_never_reads_below_the_stored_energy() {
    use machinery_basis_core::container_sync::{TrackedSource, Value};
    use machinery_basis_core::tile_trackers::source;
    let world = world(Kind::Chamber, 0, 0);
    let machine = &world.block_entity.0;
    let capacity = machine.energy().capacity();
    let view = MachineSync {
        machine,
        stored: capacity + 100,
        tank: None,
    };
    assert_eq!(view.value(source::MAX_ENERGY), Value::Long(capacity + 100));
    assert_eq!(view.value(source::ENERGY), Value::Long(capacity + 100));
    let below = MachineSync { stored: 1, ..view };
    assert_eq!(below.value(source::MAX_ENERGY), Value::Long(capacity));
}
