//! A emissão de luz por posição (`position_light.rs`, o módulo `machinery_light` que entra no `light.rs` do Horizon): os estados que perguntam a posição, a tabela
//! partilhada com as threads de luz e o `getLightEmission(state, level, pos)` que as cópias do motor de luz leem. O arquivo do módulo é o mesmo que vai para o jogo.
//! A referência é `BlockFluidTank.getLightEmission` (`BlockFluidTank.java:33-45`) e `TileEntityFluidTank.checkLight` (`TileEntityFluidTank.java:138-149`).

#[path = "../../position_light.rs"]
mod position_light;

use machinery_basis_core::fluid_tank::{ClientLight, block_light_emission};
use position_light as light;
use std::sync::Mutex;

/// A tabela é uma só no processo: os testes que a escrevem passam um por vez.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

const TANK: u32 = 7;
const STONE: u32 = 3;

fn fresh() -> std::sync::MutexGuard<'static, ()> {
    let guard = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    for level in ["minecraft:overworld", "minecraft:the_nether"] {
        light::clear_level(level);
    }
    let mut states = vec![false; 16];
    states[TANK as usize] = true;
    light::set_states(states);
    // O teste faz o papel do cliente, no overworld: só o mundo do cliente lê a posição.
    light::mark_client_thread();
    light::set_client_level("minecraft:overworld");
    guard
}

#[test]
fn the_emission_of_a_position_is_the_larger_of_the_state_and_the_block_entity() {
    let _guard = fresh();
    let at = (10, 64, -3);
    // Sem nada guardado, a emissão é a do estado.
    assert_eq!(light::emission(TANK, 0, at), 0);
    assert_eq!(light::emission(TANK, 4, at), 4);
    assert!(light::set("minecraft:overworld", at, 15));
    assert_eq!(light::emission(TANK, 0, at), 15);
    // `Math.max(ambientLight, fluidLight)`.
    assert!(light::set("minecraft:overworld", at, 6));
    assert_eq!(light::emission(TANK, 9, at), 9);
    assert_eq!(light::emission(TANK, 2, at), 6);
    // Só a posição guardada.
    assert_eq!(light::emission(TANK, 0, (10, 65, -3)), 0);
    // O estado que não pergunta a posição fica com a tabela por estado: o que sobrou de um bloco que saiu não acende o que veio no lugar.
    assert_eq!(light::emission(STONE, 0, at), 0);
    assert_eq!(light::emission(99, 0, at), 0);
    // Acima de 15 não há luz.
    light::set("minecraft:overworld", at, 200);
    assert_eq!(light::emission(TANK, 0, at), 15);
}

#[test]
fn the_table_follows_what_the_block_entities_say() {
    let _guard = fresh();
    assert!(!light::any());
    let a = (0, 70, 0);
    let b = (17, 70, 5);
    assert!(light::set("minecraft:overworld", a, 15));
    // O mesmo valor de novo não muda nada.
    assert!(!light::set("minecraft:overworld", a, 15));
    assert!(light::set("minecraft:overworld", b, 3));
    assert!(light::any());
    assert_eq!(light::get("minecraft:overworld", a), 15);
    assert_eq!(light::get("minecraft:the_nether", a), 0);
    // O trabalho de luz de um chunk lê o 3x3 em volta dele.
    assert!(light::any_around(0, 0));
    assert!(light::any_around(2, 1));
    assert!(light::any_around(-1, -1));
    assert!(!light::any_around(3, 0));
    assert!(!light::any_around(0, 2));
    assert_eq!(light::column(0, 0), vec![(a, 15)]);
    assert_eq!(light::column(1, 0), vec![(b, 3)]);
    assert!(light::column(1, 1).is_empty());
    assert_eq!(light::emission_in(&light::column(1, 0), 0, b), 3);
    assert_eq!(light::emission_in(&light::column(1, 0), 5, b), 5);
    assert_eq!(light::emission_in(&light::column(1, 0), 0, a), 0);
    // Zero tira a posição, e a coluna some com a última dela.
    assert!(light::set("minecraft:overworld", a, 0));
    assert!(!light::set("minecraft:overworld", a, 0));
    assert!(!light::any_around(-1, -1));
    assert!(
        light::any_around(0, 0),
        "o chunk (1, 0) ainda tem uma posição"
    );
    assert!(light::set("minecraft:overworld", b, 0));
    assert!(!light::any());
    assert_eq!(light::emission(TANK, 0, a), 0);
}

#[test]
fn only_the_level_the_client_is_in_counts() {
    let _guard = fresh();
    let at = (5, 40, 5);
    light::set("minecraft:overworld", at, 15);
    light::set("minecraft:the_nether", at, 4);
    assert_eq!(light::get("minecraft:overworld", at), 15);
    assert_eq!(light::get("minecraft:the_nether", at), 4);
    // O cliente está no overworld: a posição do Nether nas mesmas coordenadas não entra.
    assert_eq!(light::emission(TANK, 0, at), 15);
    assert_eq!(light::column(0, 0), vec![(at, 15)]);
    light::set_client_level("minecraft:the_nether");
    assert_eq!(light::emission(TANK, 0, at), 4);
    assert_eq!(light::column(0, 0), vec![(at, 4)]);
    // Num nível sem nada guardado a emissão é a do estado.
    light::set_client_level("minecraft:the_end");
    assert_eq!(light::emission(TANK, 2, at), 2);
    assert!(light::column(0, 0).is_empty());
    // Tirar a de um nível deixa a do outro.
    light::set_client_level("minecraft:overworld");
    light::set("minecraft:overworld", at, 0);
    assert_eq!(light::emission(TANK, 0, at), 0);
    assert_eq!(light::get("minecraft:the_nether", at), 4);
    light::set("minecraft:overworld", at, 15);
    light::clear_level("minecraft:the_nether");
    assert_eq!(light::get("minecraft:the_nether", at), 0);
    assert_eq!(light::emission(TANK, 0, at), 15);
    light::clear_level("minecraft:overworld");
    assert!(!light::any());
}

/// `Level.getBlockEntity` (`Level.java:665-673`): no servidor, fora da thread do nível, a block entity não responde, e o motor de luz do servidor roda em outra thread. Só o
/// mundo do cliente ganha a luz do fluido.
#[test]
fn the_world_of_the_server_never_asks_the_position() {
    let _guard = fresh();
    light::set("minecraft:overworld", (1, 64, 1), 15);
    assert!(light::client_thread());
    assert!(light::differs(TANK, TANK));
    // Uma thread que não se marcou (a do servidor): a troca de estado do tanque não refaz a luz por causa da posição.
    let other = std::thread::spawn(|| {
        (
            light::client_thread(),
            light::differs(TANK, TANK),
            light::differs(TANK, 0),
        )
    })
    .join()
    .unwrap();
    assert_eq!(other, (false, false, false));
    // Os trabalhos de luz de chunk inteiro: só os do `Streaming` do cliente (`remote`) ficam anotados, e cada um vale uma vez.
    light::note_job(0x1000, false);
    assert!(!light::take_job(0x1000));
    light::note_job(0x2000, true);
    assert!(!light::take_job(0x1000));
    assert!(light::take_job(0x2000));
    assert!(!light::take_job(0x2000));
    // Sem nenhuma posição emitindo nada é anotado.
    light::set("minecraft:overworld", (1, 64, 1), 0);
    light::note_job(0x3000, true);
    light::set("minecraft:overworld", (1, 64, 1), 15);
    assert!(!light::take_job(0x3000));
    light::set("minecraft:overworld", (1, 64, 1), 0);
}

#[test]
fn a_state_that_asks_the_position_always_checks_the_light_again() {
    let _guard = fresh();
    // O `checkBlock` do tile chega como a troca do estado por ele mesmo, e o tanque com lava que sai tem a mesma linha da tabela por estado que o ar.
    assert!(light::differs(TANK, TANK));
    assert!(light::differs(TANK, 0));
    assert!(light::differs(0, TANK));
    assert!(!light::differs(STONE, 0));
    assert!(!light::differs(STONE, STONE));
    assert!(!light::differs(99, 98));
    assert!(light::positional(TANK));
    assert!(!light::positional(STONE));
    light::set_states(Vec::new());
    assert!(!light::differs(TANK, TANK));
    assert!(!light::positional(TANK));
}

#[test]
fn the_block_emission_follows_block_fluid_tank() {
    // `BlockFluidTank.java:35-44`.
    assert_eq!(block_light_emission(0, None), 0);
    assert_eq!(block_light_emission(0, Some(15)), 15);
    assert_eq!(block_light_emission(0, Some(0)), 0);
    assert_eq!(block_light_emission(7, Some(3)), 7);
    assert_eq!(block_light_emission(7, Some(9)), 9);
    // Com 15 no estado o tile nem é lido.
    assert_eq!(block_light_emission(15, Some(3)), 15);
    assert_eq!(block_light_emission(15, None), 15);
}

/// O tile do cliente (`handleUpdateTag` e `onUpdateClient`, `TileEntityFluidTank.java:127-149` e `:329-343`), com o `blockDeactivationDelay` padrão de 60.
#[test]
fn the_client_checks_the_light_after_the_scale_leaves_or_reaches_zero() {
    let mut light = ClientLight::default();
    // Sem pacote, nada.
    assert!(!light.tick(0));
    // O tanque deixou de estar vazio: a luz é conferida no próximo tick do cliente.
    light.update_tag(0.1, 60);
    assert_eq!(light.delay(), 1);
    assert!(light.tick(15));
    // A escala que sobe sem passar por zero não arma nada.
    light.update_tag(0.5, 60);
    assert_eq!(light.delay(), 0);
    assert!(!light.tick(15));
    // Esvaziou: 60 ticks depois, e o pedido só sai no último.
    light.update_tag(0.0, 60);
    assert_eq!(light.delay(), 60);
    for _ in 0..10 {
        assert!(!light.tick(0));
    }
    // Com o atraso correndo, outra mudança não o rearma.
    light.update_tag(0.2, 60);
    assert_eq!(light.delay(), 50);
    light.update_tag(0.0, 60);
    assert_eq!(light.delay(), 50);
    for _ in 0..49 {
        assert!(!light.tick(0));
    }
    assert!(light.tick(0));
    assert_eq!(light.delay(), 0);
    // Um fluido sem luz (a água) arma o atraso e não pede nada: a emissão é a mesma da última leitura.
    light.update_tag(1.0, 60);
    assert!(!light.tick(0));
    assert_eq!(light.delay(), 0);
    // O mesmo valor de escala não é mudança.
    light.update_tag(1.0, 60);
    assert_eq!(light.delay(), 0);
}
