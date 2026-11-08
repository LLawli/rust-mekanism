//! O caminho dos dados da tela do Heat Generator do servidor ao cliente, sem o jogo: o `ContainerSync` do jogador manda `Property`, o
//! pacote passa pelos bytes, e o espelho entrega cada dado à posição dele. O que a tela lê do espelho tem de ser exatamente o que o
//! servidor tem, inclusive o calor e as perdas, que são `double` e chegam com todos os bits.
mod common;

use machinery_basis_core::container_sync::{ContainerSync, StackValue};
use mekanismgenerators_rules::{
    ENERGY_CAPACITY, LAVA, MAX_PRODUCTION, Produced, ScreenData, ScreenIndexes,
    heat_generator_trackers,
};

/// Valores que um arredondamento pelo caminho mostraria: nenhum cabe num `float`.
const HEAT: f64 = 300.123_456_789_012_3;
const TRANSFER_LOSS: f64 = 1.0 / 3.0;
const ENVIRONMENT_LOSS: f64 = 0.000_123_456_789_012_345_6;

fn busy_server() -> common::Server {
    let mut server = common::Server::new(300.0);
    server.state.heat.heat = HEAT;
    server.state.lava = 750;
    server.state.energy = ENERGY_CAPACITY - 1;
    server.last = Produced {
        energy: MAX_PRODUCTION,
        transfer_loss: TRANSFER_LOSS,
        environment_loss: ENVIRONMENT_LOSS,
        ..Produced::default()
    };
    server
}

#[test]
fn a_freshly_opened_screen_reads_exactly_what_the_server_has() {
    let server = busy_server();
    let mut sync = ContainerSync::<u8>::new(&heat_generator_trackers());
    let client = common::Client::open(&mut sync, &server);
    assert_eq!(
        client.screen(),
        ScreenData {
            energy: ENERGY_CAPACITY as u32 - 1,
            lava: 750,
            production: MAX_PRODUCTION as u32,
            heat: HEAT,
            transfer_loss: TRANSFER_LOSS,
            environment_loss: ENVIRONMENT_LOSS,
        }
    );
    // Os bits do `double`, e não só um valor próximo.
    let screen = client.screen();
    assert_eq!(screen.heat.to_bits(), HEAT.to_bits());
    assert_eq!(screen.transfer_loss.to_bits(), TRANSFER_LOSS.to_bits());
    assert_eq!(
        screen.environment_loss.to_bits(),
        ENVIRONMENT_LOSS.to_bits()
    );
    assert_eq!(
        client
            .mirror
            .stack(ScreenIndexes::of(&heat_generator_trackers()).lava),
        StackValue {
            resource: LAVA.to_owned(),
            amount: 750,
        }
    );
}

#[test]
fn only_the_dirty_data_reaches_the_screen_and_it_reads_the_new_values() {
    let mut server = busy_server();
    let mut sync = ContainerSync::<u8>::new(&heat_generator_trackers());
    let mut client = common::Client::open(&mut sync, &server);
    assert!(sync.broadcast_changes(&server.source()).is_empty());

    server.state.heat.heat = HEAT.next_up();
    server.last.environment_loss = 0.0;
    let dirty = sync.broadcast_changes(&server.source());
    assert_eq!(
        dirty.len(),
        2,
        "só o calor e a perda para o ambiente mudaram"
    );
    client.receive(&dirty, false);
    let screen = client.screen();
    assert_eq!(screen.heat.to_bits(), HEAT.next_up().to_bits());
    assert_eq!(screen.environment_loss, 0.0);
    // O que não mudou segue como estava.
    assert_eq!(screen.transfer_loss.to_bits(), TRANSFER_LOSS.to_bits());
    assert_eq!(screen.lava, 750);
}

#[test]
fn a_screen_with_no_data_yet_reads_the_defaults() {
    let server = common::Server::new(300.0);
    let (client, _) = common::client_of(&server, false);
    // O gerador recém-posto manda zero de energia, lava e produção, e o calor que o capacitor tem.
    let screen = client.screen();
    assert_eq!((screen.energy, screen.lava, screen.production), (0, 0, 0));
    assert_eq!(screen.heat.to_bits(), server.state.heat.heat.to_bits());
    assert_eq!((screen.transfer_loss, screen.environment_loss), (0.0, 0.0));
}
