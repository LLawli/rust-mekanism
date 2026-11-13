//! O `HostAcceptor` da cola (decisão 122) na lib: a conta de `SimpleEnergyHandler.insert` e `extract` sobre a foto do buffer do vizinho, que o Horizon não deixa desfazer.
use machinery_basis_core::distribution::EnergyInsert;
use machinery_basis_core::transaction::Transaction;
use machinery_basis_core::transmission::host_acceptor::{BufferSnapshot, SnapshotAcceptor};

fn acceptor(amount: i32, capacity: i32) -> SnapshotAcceptor {
    SnapshotAcceptor::new(BufferSnapshot {
        amount,
        capacity,
        max_insert: 1_000,
        max_extract: 400,
        can_insert: true,
        can_extract: true,
    })
}

#[test]
fn insert_takes_what_fits_up_to_the_offer_and_the_rate_and_counts_what_it_took() {
    let neighbour = acceptor(900, 1_500);
    let transaction = Transaction::open_root();
    // Cabem 600, o oferecido é maior.
    assert_eq!(neighbour.insert(5_000, &transaction), 600);
    // O que já entrou neste evento conta: não cabe mais nada.
    assert_eq!(neighbour.insert(5_000, &transaction), 0);
    assert_eq!(neighbour.take_pending(), (600, 0));
    assert_eq!(neighbour.take_pending(), (0, 0), "zera");
    // O máximo por inserção.
    let roomy = acceptor(0, 100_000);
    assert_eq!(roomy.insert(5_000, &transaction), 1_000);
    // O oferecido menor que o que cabe.
    assert_eq!(acceptor(0, 100_000).insert(70, &transaction), 70);
    // Um buffer que não deixa entrar, ou cheio.
    let closed = SnapshotAcceptor::new(BufferSnapshot {
        can_insert: false,
        ..acceptor(0, 100_000).snapshot.get()
    });
    assert_eq!(closed.insert(5_000, &transaction), 0);
    assert_eq!(acceptor(1_500, 1_500).insert(5_000, &transaction), 0);
    transaction.commit();
}

#[test]
fn a_probe_changes_nothing_and_the_commit_counts_what_the_probe_said() {
    let neighbour = acceptor(900, 1_500);
    let transaction = Transaction::open_root();
    assert_eq!(neighbour.probe_insert(5_000, &transaction), 600);
    assert!(!neighbour.has_pending(), "a simulação não entrega");
    neighbour.commit_probe(5_000, &transaction);
    assert!(neighbour.has_pending());
    // Depois do commit a simulação já vê o que entrou.
    assert_eq!(neighbour.probe_insert(5_000, &transaction), 0);
    assert_eq!(neighbour.take_pending(), (600, 0));
    transaction.commit();
}

#[test]
fn extract_gives_what_the_buffer_has_up_to_the_ask_and_the_rate_and_only_the_real_one_counts() {
    let neighbour = acceptor(1_000, 5_000);
    // Simular não tira.
    assert_eq!(neighbour.extract(300, true), 300);
    assert!(!neighbour.has_pending());
    assert_eq!(neighbour.extract(300, false), 300);
    assert!(neighbour.has_pending(), "a retirada de verdade conta");
    // O máximo por retirada.
    assert_eq!(neighbour.extract(5_000, false), 400);
    // Sobram 300, e o que já saiu neste evento conta.
    assert_eq!(neighbour.extract(5_000, false), 300);
    assert_eq!(neighbour.extract(5_000, false), 0);
    assert_eq!(neighbour.take_pending(), (0, 1_000));
    // Pedido zero ou negativo, ou um buffer que não deixa sair.
    assert_eq!(acceptor(1_000, 5_000).extract(0, false), 0);
    assert_eq!(acceptor(1_000, 5_000).extract(-5, false), 0);
    let closed = SnapshotAcceptor::new(BufferSnapshot {
        can_extract: false,
        ..acceptor(1_000, 5_000).snapshot.get()
    });
    assert_eq!(closed.extract(300, false), 0);
}

/// A capability que aparece no primeiro tick da máquina (ou muda de lado) avisa o cabo vizinho; a capacidade sozinha não.
#[test]
fn a_capability_that_appears_or_changes_side_tells_the_neighbor_cables() {
    use machinery_basis_core::transmission::owner::capability_changed;
    let none = [false; 6];
    let mut up = none;
    up[1] = true;
    let mut east = none;
    east[5] = true;
    let first = ("mekanism:enrichment_chamber", up, none);
    assert!(capability_changed(None, first), "a máquina que entra");
    assert!(!capability_changed(Some(first), first), "nada mudou");
    assert!(
        capability_changed(Some(first), ("mekanism:crusher", up, none)),
        "outro bloco"
    );
    assert!(
        capability_changed(Some(first), (first.0, east, none)),
        "outra face de entrada"
    );
    assert!(
        capability_changed(Some(first), (first.0, up, east)),
        "outra face de saída"
    );
}
