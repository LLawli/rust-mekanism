//! Salvar e sair com a janela de upgrades aberta, sem o jogo: repete, tick a tick, os rastros `upgrade_quit_<máquina>.jsonl` que o Mekanism original
//! gravou (`dev/referencia/testes/UpgradeQuitTraceTest.java`). No original, `Save and Quit` salva o jogador (`PlayerList.remove`), tira a entidade
//! (`Player.remove`, que fecha o menu e a janela) e grava o tile; o mundo reaberto o lê (`BlockEntity.loadStatic`). O rastro faz isso com a janela
//! selecionada, upgrades no slot da janela (instalando, os 20 ticks de `TileComponentUpgrade.UPGRADE_TICKS_REQUIRED`), a máquina andando, e o
//! tile gravado e relido (`quit`).
//!
//! O que o rastro mostra, e o teste repete: os itens dos dois slots de upgrade, os upgrades instalados, a energia, a entrada, a saída, o tanque e o
//! progresso voltam como estavam; `upgradeTicks` (a barra de instalação) e `canCheckUpgrades` são só memória (`ITileComponent.serialize` não os
//! grava), então a instalação recomeça dos 20 ticks, e os 20 ticks são contados de novo do primeiro tick depois de reabrir.
mod common;

use machinery_basis_core::cached_recipe::{ItemIngredient, ItemOutput, ItemToItemRecipe};
use machinery_basis_core::containers::{AutomationType, InventorySlot, ItemResource};
use machinery_basis_core::item_handler::NoNeighbours;
use machinery_basis_core::tile::ProgressMachine;
use machinery_basis_core::transaction::Transaction;
use machinery_basis_core::upgrade::UpgradeSaved;
use mekanism_rules::{
    ENERGY_UPGRADE, ENRICHMENT_USAGE, SPEED_UPGRADE, electric_config, upgrade_type_of,
};
use serde_json::{Value, json};
use std::path::Path;
use std::rc::Rc;

/// `Trace.refill`: o cabo devolve energia para tantos ticks.
const REFILL_TICKS: i64 = 3;
const MUFFLING_UPGRADE: &str = "mekanism:muffling";

fn item(id: &str) -> ItemResource {
    ItemResource::new(id, 64)
}

/// A Enrichment Chamber com a receita do rastro (`clay` vira 4 `clay_ball`), posta por um jogador (`applyImplicitComponents`).
fn chamber() -> ProgressMachine {
    let recipe = Rc::new(ItemToItemRecipe {
        id: "mekanism:enriching/clay_ball".to_owned(),
        input: ItemIngredient::item("clay"),
        input_count: 1,
        output: ItemOutput {
            resource: item("clay_ball"),
            count: 4,
        },
    });
    let mut config = electric_config(
        ENRICHMENT_USAGE,
        ItemToItemRecipe::cache_of(vec![recipe]),
        0,
        common::upgrade_data(),
    );
    config.items = common::item_energy();
    placed(ProgressMachine::electric(config))
}

fn placed(mut machine: ProgressMachine) -> ProgressMachine {
    machine.apply_item_components(&UpgradeSaved::default());
    machine
}

fn upgrade_id(name: &str) -> String {
    format!("mekanism:{name}")
}

fn slot_json(slot: &InventorySlot) -> Value {
    json!([
        slot.resource().map_or(String::new(), |resource| resource
            .id
            .trim_start_matches("minecraft:")
            .to_owned()),
        slot.amount()
    ])
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

/// O que `UpgradeQuitTraceTest.Scene.record` grava de uma máquina de progresso, menos `menu` e `window`, que são do menu do jogador.
fn state(machine: &ProgressMachine, expected: &Value, tank: bool) -> Value {
    let upgrades = machine.upgrades();
    let mut line = json!({
        "t": expected["t"],
        "marks": expected["marks"],
        "energy": machine.energy().amount(),
        "capacity": machine.energy().capacity(),
        "progress": machine.recipe_progress(),
        "required": machine.ticks_required(),
        "active": machine.tile().active(),
        "input": slot_json(machine.input()),
        "output": slot_json(machine.output()),
        "speed": upgrades.get_upgrades(SPEED_UPGRADE),
        "energy_upgrades": upgrades.get_upgrades(ENERGY_UPGRADE),
        "muffling": upgrades.get_upgrades(MUFFLING_UPGRADE),
        "upgrade_ticks": upgrades.upgrade_ticks(),
        "upgrade_in": upgrade_slot_json(upgrades.upgrade_slot()),
        "upgrade_out": upgrade_slot_json(upgrades.upgrade_output_slot()),
    });
    if tank {
        line["tank"] = json!(machine.chemical_side().unwrap().tank.amount());
    }
    line
}

/// O tile gravado e lido de novo: o objeto nasce do que o Java persiste, com os itens, o tanque e a energia dos contêineres e o estado que
/// `saved_state` devolve (os upgrades instalados e os dois slots de upgrade entre eles); `upgradeTicks` e `canCheckUpgrades` voltam ao padrão.
fn reload(old: &ProgressMachine, fresh: ProgressMachine) -> ProgressMachine {
    let mut fresh = fresh;
    fresh.restore(&old.saved_state(), old.tile().block_active());
    let mut pairs = vec![
        (old.input(), fresh.input()),
        (old.output(), fresh.output()),
        (old.energy_slot(), fresh.energy_slot()),
    ];
    if let (Some(from), Some(to)) = (old.chemical_side(), fresh.chemical_side()) {
        pairs.push((&from.slot, &to.slot));
    }
    for (from, to) in pairs {
        let contents = from.contents();
        to.set_contents(contents.resource, contents.amount, None);
    }
    if let (Some(from), Some(to)) = (old.chemical_side(), fresh.chemical_side()) {
        let tank = from.tank.contents();
        to.tank.set_contents(tank.resource, tank.amount, None);
    }
    fresh.energy().set_energy(old.energy().amount(), None);
    fresh
}

/// Um clique do cursor num slot de upgrade (`PICKUP`, botão esquerdo): o que cabe entra.
fn click_slot(slot: &InventorySlot, resource: &ItemResource, amount: u32) {
    let transaction = Transaction::open_root();
    let inserted = slot.insert(resource, amount, &transaction, AutomationType::Manual);
    if inserted > 0 {
        transaction.commit();
    }
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

/// Repete o cenário. `fresh` faz uma máquina nova do mesmo tipo (o que o jogo faz ao ler o tile), e `quits` conta os `quit` vistos.
fn replay(
    scenario: &str,
    make: fn() -> ProgressMachine,
    tank: bool,
    minimum_lines: usize,
) -> (usize, Vec<Value>) {
    let lines = read_trace(scenario);
    assert!(
        lines.len() >= minimum_lines,
        "{scenario}: rastro curto demais: {}",
        lines.len()
    );
    let mut machine = make();
    let mut quits = 0;
    for expected in &lines {
        machine.tick_server(expected["t"].as_u64().unwrap(), &NoNeighbours);
        for mark in expected["marks"].as_array().unwrap() {
            let words: Vec<&str> = mark.as_str().unwrap().split(", ").collect();
            for action in words {
                let words: Vec<&str> = action.split_whitespace().collect();
                match words.as_slice() {
                    ["item", name, count] => {
                        machine.input().set_contents(
                            Some(item(name)),
                            count.parse().unwrap(),
                            None,
                        );
                    }
                    ["tank", "bio", amount] => {
                        let side = machine.chemical_side().unwrap();
                        side.tank.set_contents(
                            Some(machinery_basis_core::chemical::ChemicalResource::new("bio")),
                            amount.parse().unwrap(),
                            None,
                        );
                    }
                    ["open"] | ["window"] | ["carried", ..] => {}
                    ["slot", name, count] => {
                        let resource = machine.upgrades().data().resource_of(&upgrade_id(name));
                        click_slot(
                            machine.upgrades().upgrade_slot(),
                            &resource,
                            count.parse().unwrap(),
                        );
                    }
                    ["remove", name] => machine.upgrades().remove_upgrade(&upgrade_id(name), true),
                    ["quit"] => {
                        quits += 1;
                        let next = make();
                        machine = reload(&machine, next);
                    }
                    other => panic!("marca desconhecida: {other:?}"),
                }
            }
        }
        assert_eq!(
            &state(&machine, expected, tank),
            &strip(expected, tank),
            "{scenario}: tick {}",
            expected["t"]
        );
        machine.energy().set_energy(
            REFILL_TICKS * i64::from(machine.energy().energy_per_tick()),
            None,
        );
    }
    (quits, lines)
}

/// A linha do rastro sem `menu`, `window` e, nas máquinas sem tanque, nada mais: o que o teste compara.
fn strip(expected: &Value, tank: bool) -> Value {
    let mut line = expected.clone();
    let object = line.as_object_mut().unwrap();
    for key in ["menu", "window"] {
        object.remove(key);
    }
    if !tank {
        object.remove("tank");
    }
    line
}

/// A Enrichment Chamber: upgrades de velocidade e de energia no slot da janela, salvos e relidos no meio da instalação (aos 8, 19 e 21 ticks), com a
/// máquina andando e a saída enchendo; a instalação recomeça do zero depois de reabrir; e o que o botão de remover deixou no slot de saída fica.
#[test]
fn the_chamber_keeps_every_item_and_restarts_the_install_after_save_and_quit() {
    let (quits, lines) = replay("upgrade_quit_enrichment_chamber", chamber, false, 400);
    assert_eq!(quits, 5, "os cinco Save and Quit do cenário");
    // O efeito que o rastro diz ter: a barra de instalação volta a 0 no tick do `quit` e as instalações terminam depois.
    let reset = |at: usize| {
        lines[at]["upgrade_ticks"] == 0 && lines[at - 1]["upgrade_ticks"].as_i64().unwrap() > 0
    };
    assert!(
        reset(53) && reset(122) && reset(193),
        "o `quit` zera a barra de instalação"
    );
    let last = lines.last().unwrap();
    assert_eq!(
        last["upgrade_out"],
        json!(["speed", 6]),
        "os upgrades removidos continuam no slot de saída"
    );
    assert_eq!(last["energy_upgrades"], 8);
}

/// O Metallurgic Infuser, com o tanque de bio e a receita andando.
#[test]
fn the_infuser_keeps_the_tank_the_progress_and_the_upgrade_slot_after_save_and_quit() {
    fn infuser() -> ProgressMachine {
        placed(common::infuser())
    }
    let (quits, lines) = replay("upgrade_quit_metallurgic_infuser", infuser, true, 150);
    assert_eq!(quits, 2, "os dois Save and Quit do cenário");
    assert!(lines.iter().any(|line| line["tank"].as_i64().unwrap() > 0));
}
