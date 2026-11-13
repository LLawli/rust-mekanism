//! O nível que o jogador deixa descarrega os chunks dele no tempo do original (decisão 193): o rastro `descarga_nivel.jsonl` (a cena `CenaReset` do original, passo `a4_no_nether`: o
//! jogador vai ao Nether e volta, tick a tick) e `descarga_fechamento.jsonl` (a ordem em que os níveis soltam os chunks quando o servidor fecha).
use machinery_basis_core::level_idle::{IdleLevels, UNLOAD_DELAY_TICKS, closing_order};
use serde_json::Value;

const OVERWORLD: &str = "minecraft:overworld";
const NETHER: &str = "minecraft:the_nether";
const TRACE: &str = include_str!("../../../../dev/referencia/rastros/descarga_nivel.jsonl");
const CLOSING: &str = include_str!("../../../../dev/referencia/rastros/descarga_fechamento.jsonl");

fn rows(text: &str) -> Vec<Value> {
    text.lines()
        .map(|line| serde_json::from_str(line).expect("linha do rastro"))
        .collect()
}

/// Os ticks de uma janela do rastro (`no_nether` ou `de_volta`) depois do marco, com o que o original descarregou do Overworld em cada um e se o chunk (0, 0) saiu nele.
fn window(tag: &str, mark: &str) -> (i64, Vec<(i64, i64, bool)>) {
    let rows = rows(TRACE);
    let marked = rows
        .iter()
        .find(|r| r["e"] == "marca" && r["nome"] == mark)
        .map_or(-1, |r| r["gt"].as_i64().unwrap_or(-1));
    let ticks = rows
        .iter()
        .filter(|r| r["e"] == "obs" && r["tag"] == tag && r["gt"].as_i64().unwrap_or(-1) > marked)
        .map(|r| {
            let unloaded = r["descarregados"][OVERWORLD].as_i64().unwrap_or(0);
            let origin = r["perto"].as_array().is_some_and(|near| {
                near.iter()
                    .any(|c| c["dim"] == OVERWORLD && c["x"] == 0 && c["z"] == 0)
            });
            (r["gt"].as_i64().unwrap_or(-1), unloaded, origin)
        })
        .collect();
    (marked, ticks)
}

#[test]
fn the_overworld_unloads_on_the_tick_the_original_unloads_the_chunk_with_the_transmitters() {
    let (marked, ticks) = window("no_nether", "tp_nether");
    // O original: o tick que segue o marco (442) não descarrega nada; o 443 descarrega o chunk (0, 0).
    let first = ticks
        .iter()
        .find(|(_, unloaded, _)| *unloaded > 0)
        .expect("o rastro descarrega o Overworld");
    let origin = ticks
        .iter()
        .find(|(_, _, origin)| *origin)
        .expect("o chunk (0, 0) sai");
    assert_eq!(first.0, marked + 2);
    assert_eq!(origin.0, marked + 2);
    let mut levels = IdleLevels::new();
    // Antes do marco o Overworld tinha o jogador.
    levels.end_of_tick(&[OVERWORLD, NETHER]);
    let mut unloaded_at = None;
    for (gt, _, _) in &ticks {
        let step = levels.end_of_tick(&[NETHER]);
        if step.unload.iter().any(|dim| dim == OVERWORLD) {
            unloaded_at = Some(*gt);
        }
    }
    assert_eq!(UNLOAD_DELAY_TICKS, 2);
    assert_eq!(
        unloaded_at,
        Some(origin.0),
        "o modelo descarrega no tick em que o original descarrega o chunk (0, 0)"
    );
}

#[test]
fn the_level_is_posted_while_it_waits_and_on_the_tick_it_unloads_and_not_after() {
    let mut levels = IdleLevels::new();
    levels.end_of_tick(&[OVERWORLD, NETHER]);
    let first = levels.end_of_tick(&[NETHER]);
    assert_eq!(first.waiting, vec![OVERWORLD.to_owned()]);
    assert!(first.unload.is_empty());
    let second = levels.end_of_tick(&[NETHER]);
    assert_eq!(second.unload, vec![OVERWORLD.to_owned()]);
    // O `Post` do tick que descarrega ainda trata o nível (o original trata os transmissores inválidos no mesmo tick).
    assert_eq!(
        IdleLevels::post_levels(&[NETHER], &second),
        vec![NETHER.to_owned(), OVERWORLD.to_owned()]
    );
    let third = levels.end_of_tick(&[NETHER]);
    assert!(third.unload.is_empty() && third.waiting.is_empty());
    assert_eq!(
        IdleLevels::post_levels(&[NETHER], &third),
        vec![NETHER.to_owned()]
    );
}

#[test]
fn a_player_back_before_the_delay_keeps_the_chunks() {
    let mut levels = IdleLevels::new();
    levels.end_of_tick(&[OVERWORLD]);
    let step = levels.end_of_tick(&[NETHER]);
    assert!(step.unload.is_empty());
    assert_eq!(levels.idle_ticks(OVERWORLD), Some(1));
    // O jogador volta: a espera recomeça do zero.
    levels.end_of_tick(&[OVERWORLD]);
    assert_eq!(levels.idle_ticks(OVERWORLD), Some(0));
    assert!(levels.end_of_tick(&[NETHER]).unload.is_empty());
}

#[test]
fn a_level_nobody_ever_held_never_unloads_and_a_closed_world_forgets_the_waiting() {
    let mut levels = IdleLevels::new();
    assert_eq!(
        levels.end_of_tick(&[OVERWORLD]).waiting,
        Vec::<String>::new()
    );
    levels.end_of_tick(&[]);
    levels.reset();
    assert!(levels.end_of_tick(&[]).waiting.is_empty());
}

#[test]
fn the_player_back_in_the_overworld_loads_it_again_on_the_first_tick_after_the_original_ticks() {
    // O original: o jogador volta no marco `tp_overworld` e o Overworld começa a carregar no primeiro tick depois dele (a carga do rastro, 837 chunks); o modelo apenas
    // entrega ao jogo a dimensão como ocupada no tick do retorno: o que carrega é o `update_world` do Horizon.
    let (marked, ticks) = window("de_volta", "tp_overworld");
    let loading = rows(TRACE)
        .into_iter()
        .filter(|r| {
            r["e"] == "obs" && r["tag"] == "de_volta" && r["gt"].as_i64().unwrap_or(-1) > marked
        })
        .find(|r| r["carregados"][OVERWORLD].as_i64().unwrap_or(0) > 0)
        .expect("o Overworld volta a carregar");
    assert_eq!(loading["gt"].as_i64(), Some(marked + 1));
    assert!(
        ticks.iter().all(|(_, unloaded, _)| *unloaded == 0),
        "nada descarrega na volta"
    );
}

#[test]
fn at_close_the_smaller_level_lets_go_of_its_chunks_first_as_the_original_does() {
    let rows = rows(CLOSING);
    for label in ["a1_sai", "a4_sai"] {
        let totals = rows
            .iter()
            .find(|r| r["e"] == "fechamento" && r["rotulo"] == label)
            .expect("totais do fechamento");
        let levels: Vec<(String, usize)> = [OVERWORLD, NETHER, "minecraft:the_end"]
            .iter()
            .map(|dim| {
                (
                    (*dim).to_owned(),
                    totals["descarregados"][*dim].as_u64().unwrap_or(0) as usize,
                )
            })
            .collect();
        let first_seen = rows
            .iter()
            .find(|r| r["e"] == "unload_perto" && r["rotulo"] == label)
            .map(|r| r["dim"].as_str().unwrap_or_default().to_owned())
            .expect("a primeira descarga do fechamento");
        assert_eq!(closing_order(&levels).first(), Some(&first_seen), "{label}");
    }
}

#[test]
fn at_close_every_transmitter_of_every_level_is_invalid_before_the_registry_resets() {
    // O original, medido em `a1_sai` e `a4_sai`: o registro tem 28 (e 32) transmissores inválidos e 5 (e 6) redes a mudar quando o `ServerStoppedEvent` chega.
    let rows = rows(CLOSING);
    let before = rows
        .iter()
        .rfind(|r| r["e"] == "stopped_antes" && r["rotulo"] == "a1_sai")
        .expect("stopped_antes");
    assert_eq!(before["registro"]["invalid"], 28);
    assert_eq!(before["registro"]["networksToChange"], 5);
    let before = rows
        .iter()
        .rfind(|r| r["e"] == "stopped_antes" && r["rotulo"] == "a4_sai")
        .expect("stopped_antes");
    assert_eq!(before["registro"]["invalid"], 32);
    assert_eq!(before["registro"]["networksToChange"], 6);
    let after = rows
        .iter()
        .rfind(|r| r["e"] == "stopped_depois" && r["rotulo"] == "a4_sai")
        .expect("stopped_depois");
    assert_eq!(after["registro"]["invalid"], 0);
}
