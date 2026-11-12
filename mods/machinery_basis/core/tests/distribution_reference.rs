//! A distribuição genérica contra o original: `dev/referencia/rastros/distribution.jsonl` (`testes/DistributionTraceTest.java`).
//! Cada caso do rastro é repetido com handlers de mentira que seguem o `SimpleEnergyHandler` do NeoForge (com diário), e a
//! conferência olha o total enviado, o estado final de cada handler e cada chamada de `insert`, na ordem.
//! O resto do arquivo cobre o que o rastro não alcança: os alvos de transmissor, o recurso repassado, o `emit` de um contêiner.

use machinery_basis_core::containers::{Access, AutomationType, EnergyContainer};
use machinery_basis_core::distribution::{
    EnergyHandlerKind, EnergyInsert, EnergySaveKind, EnergySaveShare, EnergyTransmitterSaveKind,
    ResourceHandlerKind, ResourceInsert, ResourceSaveShare, ResourceTransmitterSaveKind,
    SaveHandler, SplitInfo, Target, emit, emit_from_container, handlers_from_caches,
    saturated_cast, send_to_acceptors, send_to_acceptors_int,
};
use machinery_basis_core::transaction::{Callback, JournalCell, Journaled, Transaction};
use serde_json::Value;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

fn trace(name: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dev/referencia/rastros")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|line: &Value| line.get("meta").is_none())
        .collect()
}

type Log = Rc<RefCell<Vec<(usize, i64, i64)>>>;

struct Stored {
    energy: i64,
}

impl Journaled for Stored {
    type Snapshot = i64;

    fn revert_to_snapshot(&mut self, snapshot: i64) {
        self.energy = snapshot;
    }

    fn on_root_commit(&mut self, _original: i64) -> Vec<Callback> {
        Vec::new()
    }
}

/// O `SimpleEnergyHandler` (e o `FluidStacksResourceHandler` de um slot, com `max_insert` igual à capacidade).
struct Fake {
    index: usize,
    capacity: i64,
    max_insert: i64,
    cell: Rc<JournalCell<Stored>>,
    log: Log,
}

impl Fake {
    fn new(index: usize, capacity: i64, max_insert: i64, energy: i64, log: &Log) -> Rc<Self> {
        Rc::new(Self {
            index,
            capacity,
            max_insert,
            cell: JournalCell::new(Stored { energy }),
            log: log.clone(),
        })
    }

    fn energy(&self) -> i64 {
        self.cell.borrow().energy
    }

    fn put(&self, amount: i32, transaction: &Transaction) -> i32 {
        let energy = self.energy();
        let inserted = (self.capacity - energy)
            .min(i64::from(amount))
            .min(self.max_insert);
        let accepted = if inserted > 0 {
            self.cell.update_snapshots(transaction, || energy);
            self.cell.borrow_mut().energy += inserted;
            inserted as i32
        } else {
            0
        };
        self.log
            .borrow_mut()
            .push((self.index, i64::from(amount), i64::from(accepted)));
        accepted
    }
}

impl EnergyInsert for Fake {
    fn insert(&self, amount: i32, transaction: &Transaction) -> i32 {
        self.put(amount, transaction)
    }
}

impl ResourceInsert<str> for Fake {
    fn insert(&self, resource: &str, amount: i32, transaction: &Transaction) -> i32 {
        assert_eq!(resource, "water");
        self.put(amount, transaction)
    }
}

fn longs(value: &Value, key: &str) -> Vec<i64> {
    value[key]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry.as_i64().unwrap())
        .collect()
}

fn replay(case: &Value) -> (i64, Vec<i64>, Vec<(usize, i64, i64)>) {
    let kind = case["kind"].as_str().unwrap();
    let amount = case["amount"].as_i64().unwrap();
    let order: Vec<usize> = case["order"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry.as_u64().unwrap() as usize)
        .collect();
    let specs = case["handlers"].as_array().unwrap();
    let spec = |index: usize, key: &str| specs[index][key].as_i64().unwrap();
    let log: Log = Rc::default();
    let fakes: Vec<Rc<Fake>> = (0..specs.len())
        .map(|index| {
            Fake::new(
                index,
                spec(index, "cap"),
                if kind == "fluid" {
                    spec(index, "cap")
                } else {
                    spec(index, "max_insert")
                },
                // Os tanques de fluido do rastro sempre começam vazios.
                if kind == "fluid" {
                    0
                } else {
                    spec(index, "energy")
                },
                &log,
            )
        })
        .collect();
    let listed = || -> Vec<Rc<Fake>> { order.iter().map(|&index| fakes[index].clone()).collect() };
    let (sent, finals) = match kind {
        "target" => {
            let mut target = Target::with_handlers(EnergyHandlerKind::new(), listed());
            let sent = send_to_acceptors(Some(&mut target), amount, &(), None);
            (sent, fakes.iter().map(|fake| fake.energy()).collect())
        }
        "target_int" => {
            let mut target = Target::with_handlers(EnergyHandlerKind::new(), listed());
            let sent = send_to_acceptors_int(Some(&mut target), amount as i32, &(), None);
            (
                i64::from(sent),
                fakes.iter().map(|fake| fake.energy()).collect(),
            )
        }
        "emit_list" => {
            let sent = emit(listed(), amount, None);
            (sent, fakes.iter().map(|fake| fake.energy()).collect())
        }
        "fluid" => {
            let mut target =
                Target::with_handlers(ResourceHandlerKind::<Fake, str>::new(), listed());
            let sent = send_to_acceptors(Some(&mut target), amount, "water", None);
            (sent, fakes.iter().map(|fake| fake.energy()).collect())
        }
        "save" => {
            let containers: Vec<EnergyContainer> = (0..specs.len())
                .map(|index| {
                    let container =
                        EnergyContainer::new(spec(index, "cap"), 0, Access::Any, Access::Any, None);
                    container.set_energy(spec(index, "energy"), None);
                    container
                })
                .collect();
            let transaction = Transaction::open_root();
            let saves: Vec<_> = containers
                .iter()
                .map(|container| SaveHandler::start_save_handling(container.clone(), &transaction))
                .collect();
            let mut target = Target::new(EnergySaveKind::new());
            for &index in &order {
                target.add_handler(saves[index].clone());
            }
            let sent = send_to_acceptors(Some(&mut target), amount, &(), Some(&transaction));
            transaction.commit();
            (
                sent,
                containers
                    .iter()
                    .map(|container| container.amount())
                    .collect(),
            )
        }
        other => panic!("tipo {other}"),
    };
    let calls = log.borrow().clone();
    (sent, finals, calls)
}

#[test]
fn every_case_of_the_original_repeats_with_the_same_calls() {
    let cases = trace("distribution.jsonl");
    assert!(cases.len() >= 700, "só {} casos", cases.len());
    let mut kinds = std::collections::BTreeMap::<String, usize>::new();
    let mut differences = Vec::new();
    for (position, case) in cases.iter().enumerate() {
        let (sent, finals, calls) = replay(case);
        let kind = case["kind"].as_str().unwrap().to_string();
        *kinds.entry(kind.clone()).or_default() += 1;
        let mut wrong = Vec::new();
        if sent != case["sent"].as_i64().unwrap() {
            wrong.push(format!("sent {sent} contra {}", case["sent"]));
        }
        if finals != longs(case, "finals") {
            wrong.push(format!("finals {finals:?} contra {}", case["finals"]));
        }
        if let Some(expected) = case.get("calls") {
            let expected: Vec<(usize, i64, i64)> = expected
                .as_array()
                .unwrap()
                .iter()
                .map(|call| {
                    let call = call.as_array().unwrap();
                    (
                        call[0].as_u64().unwrap() as usize,
                        call[1].as_i64().unwrap(),
                        call[2].as_i64().unwrap(),
                    )
                })
                .collect();
            if calls != expected {
                wrong.push(format!("chamadas {calls:?} contra {expected:?}"));
            }
        }
        if !wrong.is_empty() {
            differences.push(format!("caso {position} ({kind}): {}", wrong.join("; ")));
        }
    }
    // O rastro cobre os cinco caminhos; sem isso um tipo some sem ninguém ver.
    for kind in ["target", "target_int", "emit_list", "save", "fluid"] {
        assert!(
            kinds.get(kind).copied().unwrap_or(0) >= 20,
            "{kind}: {kinds:?}"
        );
    }
    assert!(
        differences.is_empty(),
        "{} diferenças:\n{}",
        differences.len(),
        differences
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn split_info_keeps_the_remainder_like_the_original() {
    let mut split = SplitInfo::new(100, 3);
    assert_eq!(
        (
            split.share_amount(),
            split.remainder_amount(),
            split.unsent()
        ),
        (33, 34, 100)
    );
    split.send(34, true);
    assert_eq!((split.share_amount(), split.remainder_amount()), (33, 33));
    assert_eq!(split.total_sent(), 34);
    // Sem destinos a conta é zero, sem divisão por zero.
    let empty = SplitInfo::new(100, 0);
    assert_eq!((empty.share_amount(), empty.remainder_amount()), (0, 0));
}

#[test]
fn the_empty_and_missing_cases_send_nothing() {
    let mut empty: Target<EnergyHandlerKind<Fake>> =
        Target::with_handlers(EnergyHandlerKind::new(), Vec::new());
    assert_eq!(send_to_acceptors(Some(&mut empty), 50, &(), None), 0);
    assert_eq!(
        send_to_acceptors::<EnergyHandlerKind<Fake>>(None, 50, &(), None),
        0
    );
    let log: Log = Rc::default();
    let mut one = Target::with_handlers(
        EnergyHandlerKind::new(),
        vec![Fake::new(0, 100, 100, 0, &log)],
    );
    assert_eq!(send_to_acceptors(Some(&mut one), 0, &(), None), 0);
    assert!(log.borrow().is_empty());
    assert_eq!(send_to_acceptors(Some(&mut one), 30, &(), None), 30);
    assert_eq!(saturated_cast(i64::MAX), i32::MAX);
    assert_eq!(saturated_cast(i64::MIN), i32::MIN);
}

#[test]
#[should_panic(expected = "single handler")]
fn the_singular_send_refuses_more_than_one_handler() {
    let log: Log = Rc::default();
    let mut two = Target::with_handlers(
        EnergyHandlerKind::new(),
        vec![Fake::new(0, 1, 1, 0, &log), Fake::new(1, 1, 1, 0, &log)],
    );
    two.send_to_singular_acceptor(&(), 5, None);
}

#[test]
#[should_panic(expected = "só de leitura")]
fn a_target_built_from_a_collection_does_not_take_more() {
    let log: Log = Rc::default();
    let mut target = Target::with_handlers(EnergyHandlerKind::new(), Vec::new());
    target.add_handler(Fake::new(0, 1, 1, 0, &log));
}

impl EnergySaveShare for Fake {
    fn accept(&self, amount: i64, transaction: &Transaction) -> i64 {
        i64::from(self.put(saturated_cast(amount), transaction))
    }
}

impl ResourceSaveShare<str> for Fake {
    fn accept(&self, resource: &str, amount: i64, transaction: &Transaction) -> i64 {
        assert_eq!(resource, "steam");
        i64::from(self.put(saturated_cast(amount), transaction))
    }
}

#[test]
fn the_transmitter_targets_hand_the_amount_to_each_share_in_order() {
    let log: Log = Rc::default();
    let shares: Vec<Rc<Fake>> = [10, 1000, 1000]
        .iter()
        .enumerate()
        .map(|(index, &capacity)| Fake::new(index, capacity, capacity, 0, &log))
        .collect();
    let mut target = Target::new(EnergyTransmitterSaveKind::new());
    for share in &shares {
        target.add_handler(share.clone());
    }
    assert_eq!(send_to_acceptors(Some(&mut target), 100, &(), None), 100);
    assert_eq!(
        shares
            .iter()
            .map(|share| share.energy())
            .collect::<Vec<_>>(),
        vec![10, 45, 45]
    );
    let resource_shares: Vec<Rc<Fake>> = [10, 1000]
        .iter()
        .enumerate()
        .map(|(index, &capacity)| Fake::new(index, capacity, capacity, 0, &log))
        .collect();
    let mut target = Target::new(ResourceTransmitterSaveKind::<Fake, str>::new());
    for share in &resource_shares {
        target.add_handler(share.clone());
    }
    assert_eq!(
        send_to_acceptors(Some(&mut target), 100, "steam", None),
        100
    );
    assert_eq!(
        resource_shares
            .iter()
            .map(|share| share.energy())
            .collect::<Vec<_>>(),
        vec![10, 90]
    );
}

#[test]
fn the_same_handler_listed_twice_keeps_its_first_place_in_needed() {
    // A repete: o `put` do mapa ligado troca o valor sem mudar a posição, então A (lugar 0) ainda vem antes de B.
    let log: Log = Rc::default();
    let a = Fake::new(0, 1000, 1000, 0, &log);
    let b = Fake::new(1, 1000, 1000, 0, &log);
    let mut target = Target::with_handlers(
        EnergyHandlerKind::new(),
        vec![a.clone(), b.clone(), a.clone()],
    );
    send_to_acceptors(Some(&mut target), 100, &(), None);
    // As três consultas (A, B, A), depois a divisão do resto: o primeiro envio de verdade é o de A.
    let calls = log.borrow().clone();
    assert_eq!(
        calls[..3].iter().map(|c| c.0).collect::<Vec<_>>(),
        vec![0, 1, 0]
    );
    assert_eq!(calls[3].0, 0);
    assert_eq!(calls[4].0, 1);
}

#[test]
fn emit_from_a_container_takes_out_only_what_was_sent() {
    let log: Log = Rc::default();
    let near = Fake::new(0, 30, 30, 0, &log);
    let far = Fake::new(1, 1000, 1000, 0, &log);
    let container = EnergyContainer::new(10_000, 0, Access::Any, Access::Any, None);
    container.set_energy(500, None);
    let sent = emit_from_container(vec![near.clone(), far.clone()], &container, 200, None);
    assert_eq!(sent, 200);
    assert_eq!(container.amount(), 300);
    assert_eq!((near.energy(), far.energy()), (30, 170));
    // Contêiner vazio, taxa zero e destinos cheios não tiram nada.
    let empty = EnergyContainer::new(10, 0, Access::Any, Access::Any, None);
    assert_eq!(emit_from_container(vec![near.clone()], &empty, 5, None), 0);
    assert_eq!(
        emit_from_container(vec![far.clone()], &container, 0, None),
        0
    );
    let full = Fake::new(2, 5, 5, 5, &log);
    assert_eq!(emit_from_container(vec![full], &container, 50, None), 0);
    assert_eq!(container.amount(), 300);
    // O contêiner que não entrega (extração só de dentro, `Access::InternalOnly` recusa o externo, aqui é interno): nada sai de um vazio.
    let _ = AutomationType::Internal;
    assert_eq!(
        handlers_from_caches(&[1, 2, 3], |&n| (n != 2).then_some(n * 10)),
        vec![10, 30]
    );
}

#[test]
fn amounts_above_the_int_range_reach_the_handlers_saturated() {
    // `Ints.saturatedCast` na consulta (`sendPossible`) e no envio de verdade, e no único destino.
    let log: Log = Rc::default();
    let a = Fake::new(0, i64::from(i32::MAX), i64::from(i32::MAX), 0, &log);
    let b = Fake::new(1, i64::from(i32::MAX), i64::from(i32::MAX), 0, &log);
    let mut target = Target::with_handlers(EnergyHandlerKind::new(), vec![a.clone(), b.clone()]);
    send_to_acceptors(Some(&mut target), 5_000_000_000, &(), None);
    let requested: Vec<i64> = log.borrow().iter().map(|call| call.1).collect();
    assert!(
        requested
            .iter()
            .all(|&amount| amount <= i64::from(i32::MAX))
    );
    assert_eq!(requested[0], i64::from(i32::MAX), "a consulta");
    assert_eq!(a.energy() + b.energy(), 4_294_967_294);
    let single = Fake::new(2, i64::from(i32::MAX), i64::from(i32::MAX), 0, &log);
    let mut one = Target::with_handlers(EnergyHandlerKind::new(), vec![single.clone()]);
    assert_eq!(
        send_to_acceptors(Some(&mut one), 5_000_000_000, &(), None),
        i64::from(i32::MAX)
    );
    assert_eq!(log.borrow().last().unwrap().1, i64::from(i32::MAX));
}

#[test]
fn a_split_that_already_sent_adds_the_singular_send_to_its_total() {
    // `Target.sendToAcceptors(splitInfo)` com um só handler: `getTotalSent() + sendToSingularAcceptor(getUnsent())`.
    let log: Log = Rc::default();
    let mut target = Target::with_handlers(
        EnergyHandlerKind::new(),
        vec![Fake::new(0, 1000, 1000, 0, &log)],
    );
    let mut split = SplitInfo::new(100, 2);
    split.send(40, true);
    assert_eq!(target.send_to_acceptors(&mut split, &(), None), 100);
    assert_eq!(log.borrow()[0].1, 60);
    let mut empty: Target<EnergyHandlerKind<Fake>> = Target::new(EnergyHandlerKind::new());
    let mut split = SplitInfo::new(100, 2);
    split.send(40, true);
    assert_eq!(empty.send_to_acceptors(&mut split, &(), None), 40);
}
