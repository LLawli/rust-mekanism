//! A aba de controle de redstone contra o original: `dev/referencia/rastros/redstone_<bloco>.jsonl` (`RedstoneTabTraceTest.java`) traz, para o Crusher, o Heat Generator e o
//! Basic Energy Cube postos por um jogador, uma linha por modo de partida com o que o pacote de verdade (`PacketGuiInteract.handle`, `NEXT_REDSTONE_CONTROL` e
//! `PREVIOUS_REDSTONE_CONTROL`) deixa depois do clique do dono e de outro jogador com a máquina em cada modo de segurança, a volta inteira dos modos, o NBT, o item solto, o modo do
//! item posto de volta e o estado do tile com e sem sinal depois de cada clique.
use machinery_basis_core::item_content::redstone_collect;
use machinery_basis_core::redstone_tab::{self, can_click, server_click};
use machinery_basis_core::security::{SecurityMode, SecurityState};
use machinery_basis_core::tile::{RedstoneControl, RedstoneSync, TileBase};
use serde_json::Value;
use std::path::PathBuf;

const BLOCKS: [&str; 3] = ["crusher", "heat_generator", "basic_energy_cube"];

fn rows(block: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../../dev/referencia/rastros/redstone_{block}.jsonl"
    ));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let rows: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).expect("linha JSON válida"))
        .collect();
    assert_eq!(rows.len(), 4, "{block}: uma linha por modo de partida");
    rows
}

fn each(mut check: impl FnMut(&str, &Value)) {
    for block in BLOCKS {
        for row in rows(block) {
            check(block, &row);
        }
    }
}

fn uuid(text: &Value) -> u128 {
    let text = text.as_str().expect("uuid").replace('-', "");
    u128::from_str_radix(&text, 16).expect("uuid em hexadecimal")
}

fn mode_name(mode: SecurityMode) -> &'static str {
    match mode {
        SecurityMode::Public => "public",
        SecurityMode::Private => "private",
        SecurityMode::Trusted => "trusted",
    }
}

fn control(name: &str) -> RedstoneControl {
    RedstoneControl::from_serialized_name(&name.to_ascii_lowercase())
}

fn name(control: RedstoneControl) -> String {
    control.serialized_name().to_ascii_uppercase()
}

/// A máquina posta pelo dono no modo `start` (`setControlType`, que troca o que ela não aceita).
fn placed(start: RedstoneControl) -> TileBase {
    let mut tile = TileBase::new(0);
    tile.set_control_type(start);
    tile
}

#[test]
fn the_starting_mode_is_the_one_the_machine_accepts() {
    each(|block, row| {
        let start = control(row["start"].as_str().unwrap());
        assert_eq!(
            name(placed(start).control_type()),
            row["effective_start"].as_str().unwrap(),
            "{block} {}",
            row["start"]
        );
    });
}

/// O clique do dono, e o de outro jogador com a máquina pública, privada e confiável, a partir de cada modo, para os dois botões.
#[test]
fn the_click_of_each_player_leaves_the_mode_the_original_leaves() {
    each(|block, row| {
        let owner = uuid(&row["owner"]);
        let other = uuid(&row["other"]);
        let start = placed(control(row["start"].as_str().unwrap())).control_type();
        for (way, forward) in [("next", true), ("previous", false)] {
            let clicks = &row["clicks"];
            let owner_state = |mode| SecurityState {
                owner: Some(owner),
                mode,
            };
            let want = |key: String| {
                clicks[&key]
                    .as_str()
                    .unwrap_or_else(|| panic!("sem {key}"))
                    .to_owned()
            };
            assert_eq!(
                name(server_click(
                    &owner_state(SecurityMode::Public),
                    owner,
                    start,
                    forward
                )),
                want(format!("owner_{way}")),
                "{block} dono {way} de {start:?}"
            );
            assert_eq!(
                name(server_click(
                    &owner_state(SecurityMode::Private),
                    owner,
                    start,
                    forward
                )),
                want(format!("owner_private_{way}")),
                "{block} dono privado {way} de {start:?}"
            );
            for mode in SecurityMode::VALUES {
                let key = format!("other_{}_{way}", mode_name(mode));
                assert_eq!(
                    name(server_click(&owner_state(mode), other, start, forward)),
                    want(key.clone()),
                    "{block} {key} de {start:?}"
                );
            }
        }
    });
}

/// Quem tem acesso (`canAccess` do servidor, sem Security Desk) é o que o rastro diz para o dono e para o outro em cada modo de segurança.
#[test]
fn the_access_that_lets_the_click_through_is_the_one_of_the_original() {
    each(|block, row| {
        let owner = uuid(&row["owner"]);
        let other = uuid(&row["other"]);
        for mode in SecurityMode::VALUES {
            let state = SecurityState {
                owner: Some(owner),
                mode,
            };
            let access = &row["access"];
            assert_eq!(
                can_click(&state, owner),
                access[format!("owner_{}", mode_name(mode))]
                    .as_bool()
                    .unwrap(),
                "{block} dono {}",
                mode_name(mode)
            );
            assert_eq!(
                can_click(&state, other),
                access[format!("other_{}", mode_name(mode))]
                    .as_bool()
                    .unwrap(),
                "{block} outro {}",
                mode_name(mode)
            );
        }
    });
}

/// Seis cliques seguidos a partir de `DISABLED`, nos dois sentidos: o ciclo de três modos, sem `PULSE`.
#[test]
fn six_clicks_in_a_row_walk_the_cycle_of_the_original() {
    each(|block, row| {
        for (key, forward) in [("forward_cycle", true), ("backward_cycle", false)] {
            let mut mode = RedstoneControl::Disabled;
            let mut walked = vec![name(mode)];
            let mut tile = placed(mode);
            for _ in 0..6 {
                mode = redstone_tab::cycle(mode, forward);
                tile.set_control_type(mode);
                walked.push(name(tile.control_type()));
            }
            let want: Vec<String> = row[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|mode| mode.as_str().unwrap().to_owned())
                .collect();
            assert_eq!(walked, want, "{block} {key}");
        }
    });
}

/// O NBT do tile (`control_type`), o item solto (`mekanism:redstone_control`, que o `DISABLED` não grava) e o modo do tile que o item põe de volta, depois de um clique do dono.
#[test]
fn the_nbt_the_item_and_the_placed_back_tile_follow_the_clicked_mode() {
    each(|block, row| {
        let start = placed(control(row["start"].as_str().unwrap())).control_type();
        let clicked = redstone_tab::cycle(start, true);
        assert_eq!(
            row["nbt_control_type"].as_str(),
            Some(clicked.serialized_name()),
            "{block} {}",
            row["start"]
        );
        let component = redstone_collect(clicked.serialized_name());
        let dropped = &row["drop"]["components"]["mekanism:redstone_control"];
        match component {
            Some(Value::String(mode)) => {
                assert_eq!(dropped.as_str(), Some(mode.as_str()), "{block}")
            }
            _ => assert!(dropped.is_null(), "{block} {}: {dropped}", row["start"]),
        }
        let back = dropped.as_str().map_or(
            RedstoneControl::Disabled,
            RedstoneControl::from_serialized_name,
        );
        assert_eq!(name(back), row["placed_back"].as_str().unwrap(), "{block}");
    });
}

/// O estado do tile depois de cada clique do dono, com e sem sinal: `isRedstoneActivated`, `canFunction`, `isPowered`, `wasPowered`.
#[test]
fn the_tile_state_after_each_click_is_the_one_of_the_original() {
    each(|block, row| {
        for (key, powered) in [("no_signal", false), ("signal", true)] {
            let steps = row["effect"][key].as_array().unwrap();
            let mut mode = placed(control(row["start"].as_str().unwrap())).control_type();
            assert_eq!(steps.len(), 4);
            for (index, step) in steps.iter().enumerate() {
                if index > 0 {
                    mode = redstone_tab::cycle(mode, true);
                }
                let sync = RedstoneSync {
                    control_type: mode,
                    powered,
                    was_powered: false,
                };
                assert_eq!(
                    name(mode),
                    step["control"].as_str().unwrap(),
                    "{block} {key} {index}"
                );
                assert_eq!(
                    sync.is_activated(),
                    step["activated"].as_bool().unwrap(),
                    "{block} {key} {index}"
                );
                assert_eq!(
                    sync.is_activated(),
                    step["can_function"].as_bool().unwrap(),
                    "{block} {key} {index}"
                );
                assert_eq!(
                    powered,
                    step["powered"].as_bool().unwrap(),
                    "{block} {key} {index}"
                );
                assert!(
                    !step["was_powered"].as_bool().unwrap(),
                    "{block} {key} {index}"
                );
                assert_eq!(
                    redstone_tab::warning(sync),
                    mode != RedstoneControl::Disabled && !sync.is_activated()
                );
            }
        }
    });
}
