//! A prova de que o aviso de vizinhança de uma máquina não dispara outro aviso (`dev/referencia/rastros/neighbors_nested.jsonl`, escrito por
//! `NeighborTraceTest.nestedUpdates`). O `CollectingNeighborUpdater` do original enfileira o aviso que um aviso dispara em camadas
//! (`CollectingNeighborUpdater.java:69-100`); o simulador (`sim::neighbor_replay`) não tem essa fila porque nenhum bloco portado a usa.
//!
//! O rastro conta, para cada operação ao lado de uma máquina, as posições que o `NeighborUpdater` anunciou (o `debugListener` da fila): as
//! mesmas de um bloco de pedra (sem gancho), 12 por operação (os seis vizinhos da fonte e as seis atualizações de forma). Um fio de redstone,
//! que avisa em cadeia, anuncia 72 na mesma operação, o que prova que a contagem enxerga o aninhamento. O rastro também grava de que classe
//! vêm `onNeighborChange`, `updatePower` e `onPowerChange` em cada tile: `onPowerChange` não é sobrescrito em nenhum (o padrão de `ITileRedstone`
//! é vazio), então `updatePower` só lê o sinal e guarda `redstone`.
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

const MACHINES: [&str; 5] = [
    "enrichment_chamber",
    "crusher",
    "energized_smelter",
    "metallurgic_infuser",
    "heat_generator",
];
/// Operações que passam pela fila (as outras, `chest_changed` e `output_signal`, chamam `onNeighborChange` direto e não anunciam nada).
const QUEUED: [&str; 7] = ["poke_on", "poke_off", "src_on", "src_off", "setblock", "clear", "place"];

fn load() -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../dev/referencia/rastros/neighbors_nested.jsonl");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn announced(lines: &[Value], machine: &str) -> BTreeMap<String, Vec<Value>> {
    lines
        .iter()
        .filter(|line| line["machine"] == machine && line.get("op").is_some())
        .map(|line| (line["op"].as_str().unwrap().to_owned(), line["announced"].as_array().unwrap().clone()))
        .collect()
}

#[test]
fn a_machine_announces_what_a_plain_block_does() {
    let lines = load();
    let stone = announced(&lines, "stone");
    for op in ["poke_on", "poke_off", "src_on", "src_off"] {
        assert_eq!(stone[op].len(), 12, "{op}");
    }
    for machine in MACHINES {
        let own = announced(&lines, machine);
        for op in ["poke_on", "poke_off", "src_on", "src_off"] {
            assert_eq!(own[op], stone[op], "{machine} {op}");
        }
        // Pôr, tirar e repor a máquina: o aviso do `setBlock` é o de qualquer bloco (seis vizinhos e seis formas), sem fila a mais.
        for op in ["clear", "setblock", "clear_again", "place"] {
            assert_eq!(own[op].len(), 12, "{machine} {op}");
        }
        // Os dois avisos diretos não passam pela fila.
        for op in ["chest_changed", "output_signal"] {
            assert!(own[op].is_empty(), "{machine} {op}");
        }
    }
    assert!(QUEUED.iter().all(|op| MACHINES.iter().all(|machine| announced(&lines, machine).contains_key(*op))));
}

/// Uma guarda que pode falhar: o fio de redstone avisa em cadeia, e a contagem o vê.
#[test]
fn the_count_sees_a_chain_of_notices() {
    let wire = announced(&load(), "redstone_wire");
    assert!(wire["src_on"].len() > 12, "{}", wire["src_on"].len());
    assert!(wire["src_off"].len() > 12, "{}", wire["src_off"].len());
}

#[test]
fn no_machine_overrides_the_power_change() {
    let lines = load();
    let mut seen = 0;
    for line in lines.iter().filter(|line| line.get("tile_class").is_some()) {
        seen += 1;
        let machine = line["machine"].as_str().unwrap();
        assert_eq!(line["on_power_change"], "", "{machine}");
        assert_eq!(line["update_power"], "mekanism.common.tile.base.TileEntityMekanism", "{machine}");
        assert_eq!(line["on_neighbor_change"], "mekanism.common.tile.base.TileEntityMekanism", "{machine}");
    }
    assert_eq!(seen, MACHINES.len());
}
