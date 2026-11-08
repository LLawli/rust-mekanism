//! A aba e a janela de upgrades do Heat Generator sem o jogo: repete, passo a passo, os rastros que o Mekanism original gravou em
//! `dev/referencia/rastros` (`heat_generator_upgrades`, `_upgrade_slot`, `_upgrade_window_install`, `_upgrade_remove_by_packet`,
//! `_upgrade_slot_quick_move` e `_container_tracking`), com a lógica pura da `machinery_basis` por baixo. O gerador só aceita o upgrade
//! de silêncio (`AttributeUpgradeSupport.MUFFLING_ONLY`) e não tem objeto vivo: o glue refaz o componente do NBT e dos slots do menu a
//! cada tick (`upgrade::rebuild`), e `HeatTile` faz o mesmo aqui.
mod common;

use machinery_basis_core::containers::{AutomationType, InventorySlot, ItemResource, SlotContents};
use machinery_basis_core::quick_move::{QuickMenu, Region, quick_move};
use machinery_basis_core::sound::{fully_muffled, volume_factor};
use machinery_basis_core::tile_trackers::UPGRADE_WINDOW_KEY;
use machinery_basis_core::transaction::Transaction;
use machinery_basis_core::upgrade::{
    self, UpgradeComponent, UpgradeData, UpgradeSaved, UpgradeTransient,
};
use machinery_basis_core::upgrade_window::{
    UpgradeSync, decode_remove, encode_remove, type_at, type_index,
};
use machinery_basis_core::window::{SelectedWindow, WindowType};
use machinery_basis_core::window_session::Sessions;
use mekanismgenerators_rules::{
    CONTAINER_ORDER, ENERGY_CAPACITY, ENERGY_SLOT, FUEL_SLOT, MENU_SLOTS, MUFFLING_UPGRADE,
    UPGRADE_OUTPUT_SLOT, UPGRADE_SLOT, UPGRADE_WINDOW, WINDOW_TYPES, accepts_upgrade,
    heat_generator_trackers, slot_exists, upgrade_window_data,
};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;

const MINIMUM_LINES: usize = 10;

/// O contêiner e o jogador do cenário: o rastro tem um só.
const CONTAINER: u32 = 1;
const PLAYER: u128 = 1;

const SPEED: &str = "mekanism:speed";
const ENERGY: &str = "mekanism:energy";

/// O Heat Generator visto pelos upgrades, como `tick_upgrades` e `act_upgrades` do glue o veem: o filho `component_upgrade` do NBT, os
/// dois slots virtuais do menu (`entity.items`) e o que o Java guarda só na memória.
struct HeatTile {
    data: Rc<UpgradeData>,
    nbt: UpgradeSaved,
    menu: [SlotContents; 2],
    transient: UpgradeTransient,
}

impl HeatTile {
    fn new() -> Self {
        Self {
            data: common::upgrade_data(),
            nbt: UpgradeSaved::default(),
            menu: [SlotContents::default(), SlotContents::default()],
            transient: UpgradeTransient::default(),
        }
    }

    /// O que o componente deixou vai para o NBT, para o menu e para a memória.
    fn store(&mut self, component: &UpgradeComponent) {
        self.nbt = component.saved();
        self.menu = [self.nbt.input.clone(), self.nbt.output.clone()];
        self.transient = component.transient();
    }

    /// Um tick do tile (`tick_upgrades`): o componente se refaz, anda e volta para os três.
    fn tick(&mut self) {
        let component = upgrade::rebuild(
            self.data.clone(),
            &self.nbt,
            Some(&self.menu),
            self.transient,
        );
        component.tick_server();
        self.store(&component);
    }

    /// Fora do tick (`act_upgrades`): o pacote de remoção, o upgrade na mão, um clique no slot virtual.
    fn act<R>(&mut self, act: impl FnOnce(&UpgradeComponent) -> R) -> R {
        let component = upgrade::rebuild(
            self.data.clone(),
            &self.nbt,
            Some(&self.menu),
            self.transient,
        );
        let result = act(&component);
        self.store(&component);
        result
    }

    /// O componente como o tile o mostra agora: o menu por cima do NBT.
    fn component(&self) -> UpgradeComponent {
        upgrade::rebuild(
            self.data.clone(),
            &self.nbt,
            Some(&self.menu),
            self.transient,
        )
    }

    fn resource(&self, upgrade: &str) -> ItemResource {
        self.data.resource_of(upgrade)
    }

    /// `setContents` num dos dois slots virtuais (o menu manda sobre o NBT no tick seguinte).
    fn set_menu(&mut self, index: usize, upgrade: Option<&str>, count: u32) {
        self.menu[index] = SlotContents::new(upgrade.map(|id| self.resource(id)), count);
    }
}

fn lines(scenario: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(format!("{scenario}.jsonl"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// Um rastro de tick: com linhas suficientes para não ser um arquivo vazio que passa por engano.
fn ticks(scenario: &str) -> Vec<Value> {
    let lines = lines(scenario);
    assert!(
        lines.len() >= MINIMUM_LINES,
        "{scenario}: rastro curto demais: {}",
        lines.len()
    );
    lines
}

fn marks(line: &Value) -> Vec<&str> {
    line["marks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|mark| mark.as_str().unwrap())
        .collect()
}

/// Uma marca que junta várias ações separadas por vírgula (`select upgrade window, track upgrades`).
fn actions(line: &Value) -> Vec<&str> {
    marks(line)
        .into_iter()
        .flat_map(|mark| mark.split(", "))
        .collect()
}

/// Um slot de upgrade como o rastro o grava: o tipo sem o namespace e a quantidade.
fn upgrade_slot_json(slot: &InventorySlot) -> Value {
    let kind = slot
        .resource()
        .and_then(|resource| upgrade_type_of(&resource))
        .map_or(String::new(), |kind| {
            kind.trim_start_matches("mekanism:").to_owned()
        });
    json!([kind, slot.amount()])
}

/// O tipo que um item de upgrade carrega no componente `mekanism:upgrade_type`.
fn upgrade_type_of(resource: &ItemResource) -> Option<String> {
    machinery_basis_core::components::Components::parse_text_value(
        resource
            .components
            .get(mekanismgenerators_rules::UPGRADE_TYPE_COMPONENT)?,
    )
}

/// O `float` que o Java grava, pelo texto mais curto que o lê de volta, que o `{:?}` do `f32` também dá.
fn float_json(value: f32) -> Value {
    serde_json::from_str(&format!("{value:?}")).unwrap()
}

fn upgrade_id(name: &str) -> String {
    format!("mekanism:{name}")
}

/// Os campos de uma linha do rastro que o componente do gerador dá, calculados do tile. Energia, `active` e `block_active` são o gerador
/// parado, sem lava nem combustível, que é o que os cenários gravam.
fn state(tile: &HeatTile, t: &Value, marks: &Value) -> Value {
    let component = tile.component();
    let sync = UpgradeSync::of(&component);
    let muffling = tile
        .data
        .supports(MUFFLING_UPGRADE)
        .then(|| tile.data.max(MUFFLING_UPGRADE))
        .flatten()
        .map(|max| (component.get_upgrades(MUFFLING_UPGRADE), max));
    let map: Map<String, Value> = tile
        .data
        .type_ids()
        .iter()
        .zip(&sync.counts)
        .filter(|(_, count)| **count > 0)
        .map(|(id, count)| (id.trim_start_matches("mekanism:").to_owned(), json!(count)))
        .collect();
    // O que a janela lê do espelho do cliente é o que o servidor tem: o pacote em bytes leva `upgradeTicks` e o mapa de upgrades sem perda.
    let server = common::Server {
        upgrades: sync.clone(),
        ..common::Server::new(0.0)
    };
    let (client, window) = common::client_of(&server, true);
    let read = client.upgrades(&window);
    assert_eq!(read.ticks, sync.ticks);
    // O mapa só leva os tipos instalados: os que o cliente não recebeu valem zero.
    for index in 0..sync.counts.len() {
        assert_eq!(read.count(index), sync.count(index), "tipo {index}");
    }
    assert_eq!(read.installed(), sync.installed());
    json!({
        "t": t,
        "marks": marks,
        "energy": 0,
        "capacity": ENERGY_CAPACITY,
        "active": false,
        "block_active": false,
        "muffling": component.get_upgrades(MUFFLING_UPGRADE),
        "upgrade_progress": float_json(component.scaled_upgrade_progress()),
        "upgrade_in": upgrade_slot_json(component.upgrade_slot()),
        "upgrade_out": upgrade_slot_json(component.upgrade_output_slot()),
        "volume_factor": float_json(volume_factor(muffling)),
        "fully_muffled": fully_muffled(muffling),
        "upgrade_ticks": sync.ticks,
        "upgrade_map": Value::Object(map),
    })
}

/// Tira de uma linha do rastro os campos do contêiner, que só existem depois que ele abriu.
fn without_window_fields(mut line: Value) -> Value {
    let object = line.as_object_mut().unwrap();
    for key in ["window", "tracked", "carried", "players"] {
        object.remove(key);
    }
    line
}

#[test]
fn the_component_driven_directly_follows_the_original_trace() {
    let mut tile = HeatTile::new();
    for expected in ticks("heat_generator_upgrades") {
        tile.tick();
        for mark in marks(&expected) {
            match mark {
                "muffling 1" | "muffling 1 more, past the maximum" => {
                    tile.act(|c| c.add_upgrades(MUFFLING_UPGRADE, 1));
                }
                "speed 3, which the generator does not support" => {
                    // `addUpgrades` não confere o suporte, só o máximo: quem chama é que confere.
                    tile.act(|c| c.add_upgrades(SPEED, 3));
                }
                "remove muffling all" => tile.act(|c| c.remove_upgrade(MUFFLING_UPGRADE, true)),
                "remove speed all" => tile.act(|c| c.remove_upgrade(SPEED, true)),
                other => panic!("marca desconhecida: {other:?}"),
            }
        }
        let actual = state(&tile, &expected["t"], &expected["marks"]);
        assert_eq!(
            without_window_fields(actual),
            expected,
            "tick {}",
            expected["t"]
        );
    }
}

#[test]
fn the_slots_follow_the_original_trace() {
    let mut tile = HeatTile::new();
    for expected in ticks("heat_generator_upgrade_slot") {
        tile.tick();
        for mark in marks(&expected) {
            let words: Vec<&str> = mark.split_whitespace().collect();
            match words.as_slice() {
                ["upgrade", "slot", "clear"] => tile.set_menu(0, None, 0),
                ["upgrade", "slot", name, count] => {
                    tile.set_menu(0, Some(&upgrade_id(name)), count.parse().unwrap());
                }
                ["remove", "muffling"] => tile.act(|c| c.remove_upgrade(MUFFLING_UPGRADE, false)),
                ["remove", name, "all"] => tile.act(|c| c.remove_upgrade(&upgrade_id(name), true)),
                other => panic!("marca desconhecida: {other:?}"),
            }
        }
        let actual = state(&tile, &expected["t"], &expected["marks"]);
        assert_eq!(
            without_window_fields(actual),
            expected,
            "tick {}",
            expected["t"]
        );
    }
}

/// `PacketRemoveUpgrade` pelo fio: o tipo vira índice no registro, os bytes saem e voltam, e o servidor remove.
fn remove_by_packet(tile: &mut HeatTile, name: &str, remove_all: bool) {
    let order = tile.data.type_ids();
    let index = type_index(&order, &upgrade_id(name)).expect("tipo do registro");
    let (index, remove_all) = decode_remove(&encode_remove(index, remove_all)).unwrap();
    if let Some(upgrade) = type_at(&order, index) {
        tile.act(|c| c.remove_upgrade(upgrade, remove_all));
    }
}

#[test]
fn the_remove_packet_follows_the_original_trace() {
    let mut tile = HeatTile::new();
    for expected in ticks("heat_generator_upgrade_remove_by_packet") {
        tile.tick();
        for mark in marks(&expected) {
            let words: Vec<&str> = mark
                .split_whitespace()
                .map(|w| w.trim_end_matches(','))
                .collect();
            match words.as_slice() {
                [
                    "install",
                    "muffling",
                    muffling,
                    "speed",
                    speed,
                    "energy",
                    energy,
                ] => {
                    for (name, total) in
                        [("muffling", muffling), ("speed", speed), ("energy", energy)]
                    {
                        tile.act(|c| c.add_upgrades(&upgrade_id(name), total.parse().unwrap()));
                    }
                }
                ["output", "energy", count, "full"]
                | ["output", "energy", count, "with", "room", "for", _] => {
                    tile.set_menu(1, Some(ENERGY), count.parse().unwrap());
                }
                ["output", "cleared", "packet", "remove", name, "all"] => {
                    tile.set_menu(1, None, 0);
                    remove_by_packet(&mut tile, name, true);
                }
                ["packet", "remove", name, "all", ..] => remove_by_packet(&mut tile, name, true),
                ["packet", "remove", name, "one", ..] => remove_by_packet(&mut tile, name, false),
                ["packet", "remove", name, "not", "installed"] => {
                    remove_by_packet(&mut tile, name, false)
                }
                other => panic!("marca desconhecida: {other:?}"),
            }
        }
        let actual = state(&tile, &expected["t"], &expected["marks"]);
        assert_eq!(
            without_window_fields(actual),
            expected,
            "tick {}",
            expected["t"]
        );
    }
}

/// Um clique com o cursor num slot de upgrade, como o `AbstractContainerMenu.clicked` trata `PICKUP` com o botão esquerdo: o que o slot
/// aceita (`InventoryContainerSlot.mayPlace`: `isValidForInsertion(.., MANUAL)`) entra até a capacidade, o resto fica no cursor; um item
/// de outro tipo que o slot aceita troca de lugar com o que ele guarda; um que ele não aceita não faz nada.
fn click_slot(slot: &InventorySlot, carried: &mut Option<(ItemResource, u32)>) {
    let Some((resource, amount)) = carried.clone() else {
        return;
    };
    if !slot.is_valid_for_insertion(&resource, AutomationType::Manual) {
        return;
    }
    let transaction = Transaction::open_root();
    let inserted = slot.insert(&resource, amount, &transaction, AutomationType::Manual);
    if inserted > 0 {
        transaction.commit();
        *carried = (amount > inserted).then(|| (resource, amount - inserted));
        return;
    }
    drop(transaction);
    if let Some(held) = slot.resource() {
        let held_amount = slot.amount();
        if held != resource {
            slot.set_contents(Some(resource), amount, None);
            *carried = Some((held, held_amount));
        }
    }
}

fn carried_json(carried: &Option<(ItemResource, u32)>) -> Value {
    match carried {
        None => json!(["", 0]),
        Some((resource, amount)) => {
            let id = match upgrade_type_of(resource) {
                Some(kind) => format!(
                    "{}:{}",
                    resource.id.trim_start_matches("mekanism:"),
                    kind.trim_start_matches("mekanism:")
                ),
                None => resource.id.to_string(),
            };
            json!([id, amount])
        }
    }
}

#[test]
fn installing_through_the_window_follows_the_original_trace() {
    let mut tile = HeatTile::new();
    let mut sessions = Sessions::<u32, u128, u8>::default();
    let base = heat_generator_trackers();
    let mut carried: Option<(ItemResource, u32)> = None;
    // O que o contêiner tinha ao fechar: o rastro continua gravando a contagem depois disso.
    let mut last_tracked = 0;
    let window = SelectedWindow::new(UPGRADE_WINDOW, 0);
    for expected in ticks("heat_generator_upgrade_window_install") {
        tile.tick();
        for mark in actions(&expected) {
            let words: Vec<&str> = mark.split_whitespace().collect();
            match words.as_slice() {
                ["open", "container"] => sessions.open(
                    CONTAINER,
                    PLAYER,
                    "overworld",
                    "mekanismgenerators:heat_generator",
                    &base,
                ),
                ["select", "upgrade", "window"] => {
                    sessions.select(CONTAINER, PLAYER, Some(window));
                }
                ["track", "upgrades"] => {
                    let session = sessions.session_mut(CONTAINER, PLAYER).unwrap();
                    session
                        .sync
                        .start_tracking(UPGRADE_WINDOW_KEY, &upgrade_window_data());
                }
                ["stop", "tracking"] => {
                    let session = sessions.session_mut(CONTAINER, PLAYER).unwrap();
                    session.sync.stop_tracking(&UPGRADE_WINDOW_KEY);
                }
                ["deselect", "window"] => {
                    sessions.select(CONTAINER, PLAYER, None);
                }
                ["close", "container"] => {
                    last_tracked = sessions.session(CONTAINER, PLAYER).unwrap().sync.count();
                    sessions.close(CONTAINER, PLAYER);
                }
                ["click", "upgrade", "slot", "with", name, count, ..] => {
                    carried = Some((tile.resource(&upgrade_id(name)), count.parse().unwrap()));
                    tile.act(|c| click_slot(c.upgrade_slot(), &mut carried));
                }
                ["drop", "what", "is", "carried"] => carried = None,
                // Só descreve o cenário: o clique ignora a janela, como o `clicked` do contêiner em Java.
                ["no", "window"] => {}
                other => panic!("marca desconhecida: {other:?}"),
            }
        }
        let mut actual = state(&tile, &expected["t"], &expected["marks"]);
        // Os campos do contêiner só existem no rastro depois que ele abriu.
        if expected.get("window").is_some() {
            let session = sessions.session(CONTAINER, PLAYER);
            let selected = sessions.selected(CONTAINER, PLAYER);
            actual["window"] = json!(if selected == Some(window) {
                "upgrade"
            } else {
                "none"
            });
            actual["tracked"] = json!(session.map_or(last_tracked, |s| s.sync.count()));
            actual["carried"] = carried_json(&carried);
            actual["players"] = json!(sessions.players(CONTAINER));
        } else {
            actual = without_window_fields(actual);
        }
        assert_eq!(actual, expected, "tick {}", expected["t"]);
    }
}

/// O contêiner do rastro `heat_generator_container_tracking`: uma sessão, e as posições que cada pedido acrescentou ou tirou.
#[test]
fn the_container_tracking_follows_the_original_trace() {
    let mut sessions = Sessions::<u32, u128, u8>::default();
    let window = SelectedWindow::new(UPGRADE_WINDOW, 0);
    let base = heat_generator_trackers();
    let mut count = base.len();
    let rows = lines("heat_generator_container_tracking");
    assert_eq!(rows.len(), 8, "o rastro do contêiner tem oito linhas");
    for expected in rows {
        let mut added: Vec<usize> = Vec::new();
        let mut removed: Vec<usize> = Vec::new();
        for mark in marks(&expected) {
            match mark {
                "container created" => {
                    sessions.open(
                        CONTAINER,
                        PLAYER,
                        "overworld",
                        "mekanismgenerators:heat_generator",
                        &base,
                    );
                    added = (0..base.len()).collect();
                }
                "PacketWindowSelect upgrade" => {
                    sessions.select(CONTAINER, PLAYER, Some(window));
                }
                "PacketWindowSelect none" => {
                    sessions.select(CONTAINER, PLAYER, None);
                }
                mark if mark.starts_with("PacketGuiInteract CONTAINER_TRACK_UPGRADES") => {
                    let sync = &mut sessions.session_mut(CONTAINER, PLAYER).unwrap().sync;
                    added = sync.start_tracking(UPGRADE_WINDOW_KEY, &upgrade_window_data());
                }
                "PacketGuiInteract CONTAINER_STOP_TRACKING" => {
                    let sync = &mut sessions.session_mut(CONTAINER, PLAYER).unwrap().sync;
                    removed = sync.stop_tracking(&UPGRADE_WINDOW_KEY);
                }
                "container closed" => {
                    count = sessions.session(CONTAINER, PLAYER).unwrap().sync.count();
                    sessions.close(CONTAINER, PLAYER);
                }
                other => panic!("marca desconhecida: {other:?}"),
            }
        }
        if let Some(session) = sessions.session(CONTAINER, PLAYER) {
            count = session.sync.count();
        }
        let t = &expected["t"];
        assert_eq!(json!(count), expected["count"], "tick {t}: count");
        assert_eq!(
            expected["tracked"].as_array().unwrap().len(),
            count,
            "tick {t}: tracked"
        );
        let positions = |key: &str| -> Vec<usize> {
            expected[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| entry[0].as_u64().unwrap() as usize)
                .collect()
        };
        assert_eq!(added, positions("added"), "tick {t}: added");
        assert_eq!(removed, positions("removed"), "tick {t}: removed");
        let selected = sessions.selected(CONTAINER, PLAYER);
        assert_eq!(
            json!(if selected == Some(window) {
                "upgrade"
            } else {
                "none"
            }),
            expected["window"],
            "tick {t}: window"
        );
        assert_eq!(
            json!(sessions.players(CONTAINER)),
            expected["players"],
            "tick {t}: players"
        );
    }
}

/// Cada jogador tem o seu `ContainerSync` e o seu espelho: a janela de upgrades que um abre acrescenta dados só à lista dele, e o outro
/// não recebe nada dela. Pedir de novo sem parar vaza a lista antiga, como o `startTracking` do Java (que não tira a anterior).
#[test]
fn the_upgrade_window_adds_data_only_to_the_player_who_opened_it() {
    const OTHER: u128 = 2;
    let base = heat_generator_trackers();
    let mut server = common::Server::new(0.0);
    let mut sessions = Sessions::<u32, u128, u8>::default();
    let mut clients = BTreeMap::new();
    for player in [PLAYER, OTHER] {
        sessions.open(
            CONTAINER,
            player,
            "overworld",
            "mekanismgenerators:heat_generator",
            &base,
        );
        let sync = &mut sessions.session_mut(CONTAINER, player).unwrap().sync;
        clients.insert(player, common::Client::open(sync, &server));
    }
    assert!(!sessions.any_extra(CONTAINER));

    // O primeiro jogador abre a janela: o servidor manda os dois dados novos a ele, e só a ele.
    let sent = sessions
        .session_mut(CONTAINER, PLAYER)
        .unwrap()
        .sync
        .start_tracking_server(UPGRADE_WINDOW_KEY, &upgrade_window_data(), &server.source());
    let positions = clients.get_mut(&PLAYER).unwrap().track_window(&sent);
    assert_eq!(positions, [base.len(), base.len() + 1]);
    assert_eq!(
        sent.iter()
            .map(|p| usize::from(p.index))
            .collect::<Vec<_>>(),
        positions
    );
    let count_of = |sessions: &Sessions<u32, u128, u8>, player| {
        sessions.session(CONTAINER, player).unwrap().sync.count()
    };
    assert_eq!(count_of(&sessions, PLAYER), base.len() + 2);
    assert_eq!(count_of(&sessions, OTHER), base.len());
    assert_eq!(clients[&OTHER].mirror.count(), base.len());

    // Um upgrade instalado: só quem tem a janela vê o mapa mudar, e o outro jogador não recebe nenhum dado.
    let mut counts = vec![0; common::declared_types()];
    counts[0] = 1;
    server.upgrades = UpgradeSync { ticks: 7, counts };
    let mut dirty = BTreeMap::new();
    for player in [PLAYER, OTHER] {
        let sync = &mut sessions.session_mut(CONTAINER, player).unwrap().sync;
        dirty.insert(player, sync.broadcast_changes(&server.source()));
    }
    assert_eq!(
        dirty[&PLAYER]
            .iter()
            .map(|p| usize::from(p.index))
            .collect::<Vec<_>>(),
        positions
    );
    assert!(dirty[&OTHER].is_empty());
    clients
        .get_mut(&PLAYER)
        .unwrap()
        .receive(&dirty[&PLAYER], false);
    let seen = clients[&PLAYER].upgrades(&positions);
    assert_eq!((seen.ticks, seen.count(0)), (7, 1));

    // Pedir de novo sem parar: a lista antiga fica e continua sincronizando, e parar tira só a nova.
    let again = sessions
        .session_mut(CONTAINER, PLAYER)
        .unwrap()
        .sync
        .start_tracking_server(UPGRADE_WINDOW_KEY, &upgrade_window_data(), &server.source());
    assert_eq!(
        again
            .iter()
            .map(|p| usize::from(p.index))
            .collect::<Vec<_>>(),
        [base.len() + 2, base.len() + 3]
    );
    assert_eq!(count_of(&sessions, PLAYER), base.len() + 4);
    let sync = &mut sessions.session_mut(CONTAINER, PLAYER).unwrap().sync;
    assert_eq!(
        sync.stop_tracking(&UPGRADE_WINDOW_KEY),
        [base.len() + 2, base.len() + 3]
    );
    assert_eq!(sync.count(), base.len() + 2);
    assert!(sessions.any_extra(CONTAINER));
    assert_eq!(count_of(&sessions, OTHER), base.len());
    // Fechar o menu de quem o abriu desfaz a lista dele, e a do outro jogador nunca existiu.
    sessions.close(CONTAINER, PLAYER);
    assert!(!sessions.any_extra(CONTAINER));
    assert_eq!(sessions.players(CONTAINER), 1);
}

/// O menu do rastro de shift-clique, com os índices do Java: a classe de cada slot vem da primeira linha do rastro, e o papel dos
/// quatro do contêiner é o do `getInitialInventory` (0 e 1 os virtuais de upgrade, 2 o combustível, 3 a energia). O gerador do port põe
/// os virtuais no fim do menu, mas a ordem de `slots_of` é a do Java (`CONTAINER_ORDER`).
struct HeatMenu {
    tile: HeatTile,
    window: Option<SelectedWindow>,
    /// O combustível, como o rastro o grava.
    fuel: Option<(ItemResource, u32)>,
    /// A classe do slot de cada índice, do rastro.
    classes: Vec<String>,
    /// O inventário do jogador por índice do menu.
    player: BTreeMap<usize, (ItemResource, u32)>,
}

/// O papel de cada slot do contêiner do Java no menu do port.
const JAVA_TO_PORT: [usize; 4] = [UPGRADE_SLOT, UPGRADE_OUTPUT_SLOT, FUEL_SLOT, ENERGY_SLOT];

impl HeatMenu {
    fn port_slot(java: usize) -> usize {
        JAVA_TO_PORT[java]
    }

    fn java_slot(port: usize) -> usize {
        JAVA_TO_PORT.iter().position(|slot| *slot == port).unwrap()
    }

    fn indices(&self, class: &str) -> Vec<usize> {
        self.classes
            .iter()
            .enumerate()
            .filter(|(_, c)| c.as_str() == class)
            .map(|(index, _)| index)
            .collect()
    }

    fn is_container(&self, slot: usize) -> bool {
        slot < JAVA_TO_PORT.len()
    }

    fn item(&self, id: &str) -> ItemResource {
        ItemResource::new(id, 64)
    }

    /// O conteúdo de um slot do contêiner, com o tipo do upgrade se ele é um.
    fn container_stack(&self, java: usize) -> Option<(ItemResource, u32)> {
        let component = self.tile.component();
        let held = |slot: &InventorySlot| slot.resource().map(|resource| (resource, slot.amount()));
        match Self::port_slot(java) {
            UPGRADE_SLOT => held(component.upgrade_slot()),
            UPGRADE_OUTPUT_SLOT => held(component.upgrade_output_slot()),
            FUEL_SLOT => self.fuel.clone(),
            _ => None,
        }
    }
}

impl QuickMenu for HeatMenu {
    type Item = ItemResource;

    fn slots_of(&self, region: Region) -> Vec<usize> {
        match region {
            Region::Container => CONTAINER_ORDER
                .iter()
                .map(|port| Self::java_slot(*port))
                .collect(),
            Region::Armor => self.indices("ArmorSlot"),
            Region::Offhand => self.indices("OffhandSlot"),
            Region::Main => self.indices("MainInventorySlot"),
            Region::Hotbar => self.indices("HotBarSlot"),
        }
    }

    fn region(&self, slot: usize) -> Region {
        match self.classes[slot].as_str() {
            "ArmorSlot" => Region::Armor,
            "OffhandSlot" => Region::Offhand,
            "MainInventorySlot" => Region::Main,
            "HotBarSlot" => Region::Hotbar,
            _ => Region::Container,
        }
    }

    fn exists(&self, slot: usize) -> bool {
        !self.is_container(slot) || slot_exists(Self::port_slot(slot), self.window)
    }

    fn stack(&self, slot: usize) -> Option<(ItemResource, u32)> {
        if self.is_container(slot) {
            self.container_stack(slot)
        } else {
            self.player.get(&slot).cloned()
        }
    }

    fn insert(&mut self, slot: usize, resource: &ItemResource, amount: u32) -> u32 {
        if !self.is_container(slot) {
            // Só os slots comuns do inventário aceitam: a armadura e a mão pedem um item equipável, e nenhum do cenário é.
            if !matches!(self.region(slot), Region::Main | Region::Hotbar) {
                return 0;
            }
            return match self.player.get_mut(&slot) {
                Some((held, count)) if held == resource => {
                    let moved = amount.min(resource.max_stack_size - *count);
                    *count += moved;
                    moved
                }
                Some(_) => 0,
                None => {
                    let moved = amount.min(resource.max_stack_size);
                    self.player.insert(slot, (resource.clone(), moved));
                    moved
                }
            };
        }
        match Self::port_slot(slot) {
            UPGRADE_SLOT => {
                // A regra do glue (`accepts_upgrade`) e a do slot do componente têm de dizer o mesmo.
                let kind = upgrade_type_of(resource);
                let accepted = accepts_upgrade(&self.tile.data, &resource.id, kind.as_deref());
                let slot_accepts = self
                    .tile
                    .component()
                    .upgrade_slot()
                    .is_valid_for_insertion(resource, AutomationType::Manual);
                assert_eq!(
                    accepted, slot_accepts,
                    "a regra do glue e a do slot, para {:?}",
                    resource.id
                );
                if !accepted {
                    return 0;
                }
                self.tile.act(|c| {
                    let transaction = Transaction::open_root();
                    let inserted = c.upgrade_slot().insert(
                        resource,
                        amount,
                        &transaction,
                        AutomationType::Manual,
                    );
                    transaction.commit();
                    inserted
                })
            }
            // A saída de upgrades só aceita o próprio componente.
            UPGRADE_OUTPUT_SLOT | ENERGY_SLOT => 0,
            FUEL_SLOT => {
                // `FluidFuelInventorySlot.forFuel`: o que queima (carvão, balde de lava); o resto fica de fora.
                if !matches!(&*resource.id, "coal" | "lava_bucket") {
                    return 0;
                }
                match &mut self.fuel {
                    Some((held, count)) if held == resource => {
                        let moved = amount.min(resource.max_stack_size - *count);
                        *count += moved;
                        moved
                    }
                    Some(_) => 0,
                    empty => {
                        let moved = amount.min(resource.max_stack_size);
                        *empty = Some((resource.clone(), moved));
                        moved
                    }
                }
            }
            _ => unreachable!(),
        }
    }

    fn remove(&mut self, slot: usize, amount: u32) {
        if !self.is_container(slot) {
            if let Some((resource, count)) = self.player.remove(&slot)
                && count > amount
            {
                self.player.insert(slot, (resource, count - amount));
            }
            return;
        }
        match Self::port_slot(slot) {
            UPGRADE_SLOT | UPGRADE_OUTPUT_SLOT => {
                let port = Self::port_slot(slot);
                self.tile.act(|c| {
                    let target = if port == UPGRADE_SLOT {
                        c.upgrade_slot()
                    } else {
                        c.upgrade_output_slot()
                    };
                    if let Some(resource) = target.resource() {
                        let transaction = Transaction::open_root();
                        target.extract(&resource, amount, &transaction, AutomationType::Manual);
                        transaction.commit();
                    }
                });
            }
            FUEL_SLOT => {
                if let Some((resource, count)) = self.fuel.take() {
                    self.fuel = (count > amount).then(|| (resource, count - amount));
                }
            }
            _ => {}
        }
    }
}

/// O rastro grava só os slots com algo: `[índice, classe, tipo, quantidade]`, com o tipo `upgrade:muffling` para um upgrade.
fn menu_slots_json(menu: &HeatMenu) -> Value {
    let name = |resource: &ItemResource| match upgrade_type_of(resource) {
        Some(kind) => format!(
            "{}:{}",
            resource.id.trim_start_matches("mekanism:"),
            kind.trim_start_matches("mekanism:")
        ),
        None => resource.id.to_string(),
    };
    // O rastro lista os virtuais mesmo escondidos: `getSlot(i).getItem()` não olha a janela.
    let slots: Vec<Value> = (0..menu.classes.len())
        .filter_map(|slot| {
            menu.stack(slot).map(|(resource, amount)| {
                json!([slot, menu.classes[slot], name(&resource), amount])
            })
        })
        .collect();
    Value::Array(slots)
}

fn window_name(window: Option<SelectedWindow>) -> Value {
    json!(if window.is_some() { "upgrade" } else { "none" })
}

#[test]
fn the_quick_move_follows_the_original_trace() {
    let rows = lines("heat_generator_upgrade_slot_quick_move");
    assert_eq!(rows.len(), 14);
    // O primeiro passo só descreve o menu: a classe de cada slot, na ordem do Java, e nada dentro.
    let layout: Vec<(usize, String)> = rows[0]["layout"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                entry[0].as_u64().unwrap() as usize,
                entry[1].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    for (position, (index, _)) in layout.iter().enumerate() {
        assert_eq!(*index, position, "o layout vem em ordem de índice");
    }
    let classes: Vec<String> = layout.into_iter().map(|(_, class)| class).collect();
    // Os quatro primeiros são o contêiner: os dois virtuais e os dois do gerador.
    assert_eq!(&classes[..2], ["VirtualInventoryContainerSlot"; 2]);
    assert!(
        classes[2..4]
            .iter()
            .all(|class| class.starts_with("InventoryContainer")),
        "{classes:?}"
    );
    assert!(
        !classes[4..]
            .iter()
            .any(|class| class.contains("Virtual") || class.starts_with("InventoryContainer")),
        "{classes:?}"
    );
    let mut menu = HeatMenu {
        tile: HeatTile::new(),
        window: None,
        fuel: None,
        classes,
        player: BTreeMap::new(),
    };
    // A numeração do inventário do jogador no menu do Java: o principal (9 a 35) e a barra (0 a 8), cada região na ordem dos índices.
    let main = menu.indices("MainInventorySlot");
    let hotbar = menu.indices("HotBarSlot");
    assert_eq!((main.len(), hotbar.len()), (27, 9));
    let player_index = |inventory: usize| {
        if inventory < 9 {
            hotbar[inventory]
        } else {
            main[inventory - 9]
        }
    };
    let window = SelectedWindow::new(UPGRADE_WINDOW, 0);
    let upgrade = |menu: &HeatMenu, name: &str| menu.tile.resource(&upgrade_id(name));
    for (step, expected) in rows.iter().enumerate().skip(1) {
        let t = expected["t"].as_u64().unwrap();
        assert_eq!(t as usize, step);
        // A marca diz o que o cenário em Java fez antes do shift-clique deste passo.
        let mark = marks(expected).join(", ");
        let put = |menu: &mut HeatMenu, inventory: usize, resource: ItemResource, count: u32| {
            menu.player
                .insert(player_index(inventory), (resource, count));
        };
        match t {
            1 => {
                assert_eq!(mark, "no window: upgrade muffling 1 from main slot 9");
                let resource = upgrade(&menu, "muffling");
                put(&mut menu, 9, resource, 1);
            }
            2 => {
                assert_eq!(mark, "no window: coal 10 from main slot 10");
                let coal = menu.item("coal");
                put(&mut menu, 10, coal, 10);
            }
            3 => {
                assert_eq!(
                    mark,
                    "select upgrade window, window: upgrade muffling 1 from main slot 11"
                );
                menu.window = Some(window);
                let resource = upgrade(&menu, "muffling");
                put(&mut menu, 11, resource, 1);
            }
            4 => {
                assert_eq!(
                    mark,
                    "window: upgrade muffling 2 from main slot 12, input slot already has muffling"
                );
                let resource = upgrade(&menu, "muffling");
                put(&mut menu, 12, resource, 2);
            }
            5 => {
                assert!(
                    mark.starts_with("window: upgrade the generator does not support from main slot 13, unsupported upgrade is mekanism:"),
                    "{mark}"
                );
                let name = mark.rsplit("mekanism:").next().unwrap();
                let resource = upgrade(&menu, name);
                put(&mut menu, 13, resource, 1);
            }
            6 => {
                assert_eq!(mark, "window: coal 10 from main slot 14");
                let coal = menu.item("coal");
                put(&mut menu, 14, coal, 10);
            }
            7 => {
                assert_eq!(mark, "window: upgrade muffling 1 from hotbar slot 0");
                let resource = upgrade(&menu, "muffling");
                put(&mut menu, 0, resource, 1);
            }
            8 => assert_eq!(
                mark,
                "window: from the virtual input slot back to the inventory"
            ),
            9 => {
                assert_eq!(
                    mark,
                    "window: output slot holds muffling 1, quick move it out"
                );
                menu.tile.set_menu(1, Some(&upgrade_id("muffling")), 1);
            }
            10 => {
                assert_eq!(mark, "deselect window, input slot holds muffling 2");
                menu.window = None;
                menu.tile.set_menu(0, Some(&upgrade_id("muffling")), 2);
            }
            11 => assert_eq!(mark, "no window: quick move the virtual input slot"),
            12 => {
                assert_eq!(
                    mark,
                    "no window: output slot holds muffling 1, quick move it"
                );
                menu.tile.set_menu(1, Some(&upgrade_id("muffling")), 1);
            }
            13 => assert_eq!(
                mark,
                "machine slots: quick move the coal in the fuel slot out"
            ),
            other => panic!("passo desconhecido: {other}"),
        }
        let returned = match expected.get("from").and_then(Value::as_u64) {
            Some(from) => {
                let before = menu.stack(from as usize);
                match quick_move(&mut menu, from as usize) {
                    Some(moved) => {
                        carried_json(&Some((before.expect("o slot tinha algo").0, moved)))
                    }
                    None => json!(["", 0]),
                }
            }
            None => Value::Null,
        };
        if expected.get("returned").is_some() {
            assert_eq!(returned, expected["returned"], "passo {t}: o que voltou");
        }
        assert_eq!(
            expected["window"],
            window_name(menu.window),
            "passo {t}: janela"
        );
        assert_eq!(
            menu_slots_json(&menu),
            expected["slots"],
            "passo {t}: {mark}"
        );
        // O `upgrade_ticks` gravado no passo 10 é o do componente, parado porque o slot de entrada ainda não passou por um tick.
        if let Some(ticks) = expected.get("upgrade_ticks") {
            assert_eq!(
                json!(menu.tile.component().upgrade_ticks()),
                *ticks,
                "passo {t}: upgradeTicks"
            );
        }
    }
}

#[test]
fn the_window_type_table_matches_the_java_enum() {
    assert_eq!(WINDOW_TYPES.len(), 11);
    for (index, window) in WINDOW_TYPES.iter().enumerate() {
        assert_eq!(usize::from(window.ordinal), index);
    }
    assert_eq!(UPGRADE_WINDOW.save_name, Some("upgrade"));
    assert!(std::hint::black_box(UPGRADE_WINDOW).can_pin);
    let _: WindowType = UPGRADE_WINDOW;
}

/// O mapa de upgrades leva um tipo por índice do registro: o servidor conta todos os tipos que o Mekanism declara.
#[test]
fn the_upgrade_map_covers_every_declared_type() {
    let tile = HeatTile::new();
    let sync = UpgradeSync::of(&tile.component());
    assert_eq!(sync.counts.len(), common::declared_types());
}

/// O que o gerador aceita é o que a tag `muffling_only` diz: o silêncio, e nenhum dos outros sete tipos.
#[test]
fn the_generator_supports_only_muffling() {
    let data = common::upgrade_data();
    let supported: Vec<String> = data.supported().map(|id| id.to_string()).collect();
    assert_eq!(supported, [MUFFLING_UPGRADE]);
    assert_eq!(data.max(MUFFLING_UPGRADE), Some(1));
    assert_eq!(data.type_ids().len(), common::declared_types());
}

/// O arquivo do menu só declara os slots: os dados da tela vêm da lista de trackers do `MekanismContainer`, não de campos do arquivo.
#[test]
fn the_menu_file_declares_only_the_slots() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("content/data/mekanismgenerators/horizon/menu/heat_generator.json");
    let menu: Value = serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
    assert_eq!(menu, json!({ "slots": MENU_SLOTS }));
    assert_eq!(
        (UPGRADE_SLOT, UPGRADE_OUTPUT_SLOT),
        (2, 3),
        "os virtuais vêm depois dos dois do gerador"
    );
}

/// O gerador não precisa do componente para guardar os slots: o menu que não tem os dois virtuais (um mundo antigo) é refeito do NBT.
#[test]
fn the_shift_click_order_puts_the_virtual_slots_first() {
    assert_eq!(
        CONTAINER_ORDER,
        [UPGRADE_SLOT, UPGRADE_OUTPUT_SLOT, FUEL_SLOT, ENERGY_SLOT]
    );
}

/// Sem janela escolhida os dois virtuais não existem para o shift-clique, e com ela existem; os do gerador sempre existem.
#[test]
fn the_virtual_slots_exist_only_with_the_upgrade_window() {
    let window = SelectedWindow::new(UPGRADE_WINDOW, 0);
    for slot in [UPGRADE_SLOT, UPGRADE_OUTPUT_SLOT] {
        assert!(!slot_exists(slot, None));
        assert!(slot_exists(slot, Some(window)));
        // Outra janela (a de cores) não é a de upgrades.
        assert!(!slot_exists(
            slot,
            Some(SelectedWindow::new(WINDOW_TYPES[0], 0))
        ));
    }
    for slot in [FUEL_SLOT, ENERGY_SLOT] {
        assert!(slot_exists(slot, None));
    }
}

/// `UpgradeInventorySlot.input` só aceita o upgrade da tag do gerador: o de silêncio, não o de velocidade, não outro item.
#[test]
fn the_input_slot_accepts_only_the_muffling_upgrade() {
    let data = common::upgrade_data();
    assert!(accepts_upgrade(
        &data,
        "mekanism:upgrade",
        Some(MUFFLING_UPGRADE)
    ));
    assert!(!accepts_upgrade(&data, "mekanism:upgrade", Some(SPEED)));
    assert!(!accepts_upgrade(&data, "mekanism:upgrade", Some(ENERGY)));
    assert!(!accepts_upgrade(&data, "mekanism:upgrade", None));
    assert!(!accepts_upgrade(
        &data,
        "minecraft:coal",
        Some(MUFFLING_UPGRADE)
    ));
}

fn mekanism_source() -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../mekanism/native/runtime.rs"),
    )
    .unwrap()
}

#[test]
fn every_declared_upgrade_type_has_a_known_color() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../mekanism/content/data/mekanism/mekanism/upgrade");
    let mut seen = 0;
    for entry in std::fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        let json: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let color = json["color"].as_str().unwrap();
        assert_ne!(
            mekanismgenerators_rules::enum_color(color),
            0xFFFF_FFFF,
            "{}: cor {color}",
            path.display()
        );
        seen += 1;
    }
    assert_eq!(seen, common::declared_types());
}

/// A tabela de janelas e a de cores são as do Mekanism, copiadas porque o Generators não depende do crate dele: as duas cópias não
/// podem divergir. Lê as linhas do `runtime.rs` do Mekanism e compara.
#[test]
fn the_copied_tables_are_the_ones_the_mekanism_has() {
    let source = mekanism_source();
    // `window_type(9, Some("upgrade"), true, 1),` e `window_type(10, None, false, 1),`.
    let windows: Vec<&str> = source
        .lines()
        .map(str::trim)
        .filter(|line| {
            line.starts_with("window_type(")
                && line.ends_with("),")
                && line.chars().nth(12).is_some_and(|c| c.is_ascii_digit())
        })
        .collect();
    assert_eq!(windows.len(), WINDOW_TYPES.len());
    for (line, window) in windows.iter().zip(WINDOW_TYPES) {
        let name = window
            .save_name
            .map_or("None".to_owned(), |name| format!("Some(\"{name}\")"));
        let expected = format!(
            "window_type({}, {name}, {}, {}),",
            window.ordinal, window.can_pin, window.max_data
        );
        assert_eq!(*line, expected);
    }
    // `"green" => 0xFF_59_C1_5F,`.
    let colors: Vec<(&str, u32)> = source
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('"') && line.contains("\" => 0xFF_") && line.ends_with(','))
        .map(|line| {
            let (name, value) = line.split_once("\" => 0x").unwrap();
            (
                &name[1..],
                u32::from_str_radix(&value.trim_end_matches(',').replace('_', ""), 16).unwrap(),
            )
        })
        .collect();
    assert!(colors.len() >= 7, "{colors:?}");
    for (name, value) in colors {
        assert_eq!(mekanismgenerators_rules::enum_color(name), value, "{name}");
    }
}

/// `Save and Quit` com a janela de upgrades aberta (`upgrade_quit_heat_generator`, `UpgradeQuitTraceTest.java`): o tile é gravado com o menu
/// aberto e relido, e os upgrades no slot da janela, os instalados e o slot de saída voltam como estavam; `upgradeTicks` e `canCheckUpgrades`
/// são só memória, então a instalação recomeça dos 20 ticks (`UpgradeTransient::default`). A queima do combustível e a energia são
/// `heat_trace.rs`: este rastro grava só o que o componente de upgrades faz, e o teste compara só isso.
#[test]
fn the_generator_keeps_the_upgrade_slot_and_restarts_the_install_after_save_and_quit() {
    let mut tile = HeatTile::new();
    let mut quits = 0;
    let lines = ticks("upgrade_quit_heat_generator");
    for expected in &lines {
        tile.tick();
        for action in actions(expected) {
            let words: Vec<&str> = action.split_whitespace().collect();
            match words.as_slice() {
                ["item", ..] | ["open"] | ["window"] | ["carried", ..] => {}
                ["slot", name, count] => {
                    let mut carried =
                        Some((tile.resource(&upgrade_id(name)), count.parse().unwrap()));
                    tile.act(|c| click_slot(c.upgrade_slot(), &mut carried));
                }
                ["quit"] => {
                    quits += 1;
                    // O mundo reaberto: o NBT e os itens do menu voltam, e a memória do componente não.
                    tile.transient = UpgradeTransient::default();
                }
                other => panic!("marca desconhecida: {other:?}"),
            }
        }
        let actual = state(&tile, &expected["t"], &expected["marks"]);
        for key in ["muffling", "upgrade_in", "upgrade_out", "upgrade_ticks"] {
            assert_eq!(actual[key], expected[key], "{key}, tick {}", expected["t"]);
        }
    }
    assert_eq!(quits, 2, "os dois Save and Quit do cenário");
    assert_eq!(
        lines[54]["upgrade_ticks"], 0,
        "o `quit` zera a barra de instalação"
    );
    assert_eq!(lines[53]["upgrade_ticks"], 8);
    assert_eq!(lines.last().unwrap()["muffling"], 1);
}

/// O cursor nos dois slots virtuais da janela de upgrades do Heat Generator (`slot_click_upgrade_heat_generator`, `SlotClickTraceTest.java`): a entrada
/// aceita o upgrade de silêncio, o único tipo que a tag do gerador tem (`isValidForInsertion(.., MANUAL)`: o item tem o componente do tipo e o tipo está
/// na tag), e nenhum outro tipo nem item comum; a saída não aceita nada. É a regra que `menu_click_before` deixa em `slot_rules` para o slot de upgrades.
#[test]
fn the_cursor_puts_only_muffling_in_the_upgrade_input_and_nothing_in_the_output() {
    let data = common::upgrade_data();
    let rows: Vec<Value> = ticks("slot_click_upgrade_heat_generator")
        .into_iter()
        .skip(1)
        .collect();
    assert!(rows.len() >= 10 * 2 * 8, "só {} linhas", rows.len());
    let mut accepted = std::collections::BTreeSet::new();
    for row in &rows {
        let item = row["item"].as_str().unwrap();
        let (kind, may) = (
            row["kind"].as_str().unwrap(),
            row["may_place"].as_bool().unwrap(),
        );
        let wanted = kind == "upgrade_in"
            && item.strip_prefix("upgrade:").is_some_and(|name| {
                mekanismgenerators_rules::accepts_upgrade(
                    &data,
                    "mekanism:upgrade",
                    Some(&format!("mekanism:{name}")),
                )
            });
        assert_eq!(may, wanted, "{row}");
        if may {
            accepted.insert(item.to_owned());
        }
    }
    assert_eq!(accepted, ["upgrade:muffling".to_owned()].into());
}
