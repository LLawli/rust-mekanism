//! A ordem do mapa de upgrades do tile para cada ordem de instalação (decisão 233) contra o original: `dev/referencia/rastros/upgrade_history_<n>.jsonl`
//! (`testes/UpgradeHistoryTraceTest.java`, duas execuções do `runGameTestServer`) grava, para as 2.072 ordens de 2 a 4 dos oito tipos, o hash de cada `Holder<Upgrade>` da execução, a ordem
//! do mapa do tile depois das instalações, a das chaves do `upgrades` do NBT gravado e lido de volta, a do tile depois de `deserialize` e a do mapa que o cliente decodifica da janela.
use machinery_basis_core::upgrade::{UpgradeComponent, UpgradeData};
use machinery_basis_core::upgrade_order::{
    UpgradeKey, entries_from_disk, file_order, fixed_hashes, loaded_map,
};
use machinery_basis_core::upgrade_window::UpgradeSync;
use serde_json::Value;
use std::path::PathBuf;
use std::rc::Rc;

const TYPES: [(&str, i32); 8] = [
    ("speed", 8),
    ("energy", 8),
    ("muffling", 4),
    ("filter", 1),
    ("chemical", 8),
    ("anchor", 1),
    ("stone_generator", 1),
    ("xp_containment", 1),
];

fn rows(run: u32) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../../dev/referencia/rastros/upgrade_history_{run}.jsonl"
    ));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    text.lines()
        .map(|line| serde_json::from_str(line).expect("linha JSON válida"))
        .collect()
}

fn names(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|name| name.as_str().unwrap().to_owned())
        .collect()
}

fn id(name: &str) -> String {
    format!("mekanism:{name}")
}

fn bare(id: &str) -> String {
    id.trim_start_matches("mekanism:").to_owned()
}

fn data() -> Rc<UpgradeData> {
    let types: Vec<(String, i32)> = TYPES.iter().map(|(name, max)| (id(name), *max)).collect();
    let borrowed: Vec<(&str, i32)> = types
        .iter()
        .map(|(name, max)| (name.as_str(), *max))
        .collect();
    let supported: Vec<&str> = borrowed.iter().map(|(name, _)| *name).collect();
    Rc::new(UpgradeData::from_parts(
        "mekanism:upgrade",
        "mekanism:upgrade_type",
        64,
        &borrowed,
        &supported,
    ))
}

/// Dá aos tipos os hashes que o original sorteou nesta execução.
fn use_hashes(row: &Value) {
    let hashes: Vec<(String, i32)> = TYPES
        .iter()
        .map(|(name, _)| (id(name), row["hashes"][name].as_i64().unwrap() as i32))
        .collect();
    fixed_hashes(&hashes);
}

/// O tile de uma máquina posta de um item sem upgrades, com os tipos instalados na ordem do rastro.
fn tile_of(row: &Value) -> UpgradeComponent {
    let component = UpgradeComponent::new(data(), Rc::new(|| {}), Box::new(|_, _, _| {}));
    component.restore_item(&Default::default());
    for name in names(&row["installed"]) {
        assert_eq!(component.add_upgrades(&id(&name), 1), 1);
    }
    component
}

fn in_order(component: &UpgradeComponent) -> Vec<String> {
    component
        .installed_in_order()
        .into_iter()
        .map(|(id, _)| bare(&id))
        .collect()
}

#[test]
fn the_tile_table_follows_the_install_order_of_the_session() {
    for run in [1, 2] {
        for (n, row) in rows(run).iter().enumerate() {
            use_hashes(row);
            assert_eq!(
                in_order(&tile_of(row)),
                names(&row["tile"]),
                "execução {run} amostra {n} ({:?})",
                names(&row["installed"])
            );
        }
    }
    fixed_hashes(&[]);
}

#[test]
fn the_window_lists_the_types_in_the_order_of_the_synced_map() {
    // Os ids na ordem do registro do port (a dos ids, alfabética).
    let ids = data().type_ids();
    for run in [1, 2] {
        for (n, row) in rows(run).iter().enumerate() {
            use_hashes(row);
            let sync = UpgradeSync::of(&tile_of(row));
            let listed: Vec<String> = sync
                .installed(&ids)
                .into_iter()
                .map(|index| bare(&ids[index]))
                .collect();
            assert_eq!(
                listed,
                names(&row["window"]),
                "execução {run} amostra {n} ({:?})",
                names(&row["installed"])
            );
        }
    }
    fixed_hashes(&[]);
}

#[test]
fn the_file_keeps_the_order_of_the_hash_map_of_the_nbt() {
    for run in [1, 2] {
        for (n, row) in rows(run).iter().enumerate() {
            let tile: Vec<String> = names(&row["tile"]).iter().map(|name| id(name)).collect();
            let file = names(&row["file"]);
            assert_eq!(file_order(&tile), file, "execução {run} amostra {n}");
        }
    }
}

#[test]
fn the_reloaded_table_is_the_one_built_from_the_order_of_the_file() {
    for run in [1, 2] {
        for (n, row) in rows(run).iter().enumerate() {
            use_hashes(row);
            let entries: Vec<(String, i32)> =
                names(&row["file"]).into_iter().map(|id| (id, 1)).collect();
            let reloaded: Vec<String> = loaded_map(&entries)
                .keys_in_order()
                .into_iter()
                .map(|key: UpgradeKey| bare(&key.id()))
                .collect();
            assert_eq!(
                reloaded,
                names(&row["reloaded"]),
                "execução {run} amostra {n}"
            );
        }
    }
    fixed_hashes(&[]);
}

/// O tile guarda junto do NBT a ordem em que o original escreveria o arquivo (`UpgradeSaved.file_order`, H259): o que `saved()` leva ao disco é a ordem `file` do rastro, e um tile novo que
/// lê esse NBT (os totais ordenados, como o do Horizon, e essa ordem) monta a tabela do tile recarregado do original, nas 4.144 amostras e sem exceção.
#[test]
fn a_tile_reloaded_with_the_written_order_is_the_original_in_every_sample() {
    let mut total = 0;
    for run in [1, 2] {
        for (n, row) in rows(run).iter().enumerate() {
            use_hashes(row);
            let saved = tile_of(row).saved();
            assert_eq!(
                saved.file_order,
                names(&row["file"]),
                "execução {run} amostra {n}"
            );
            let reloaded = UpgradeComponent::new(data(), Rc::new(|| {}), Box::new(|_, _, _| {}));
            reloaded.restore(&saved);
            assert_eq!(
                in_order(&reloaded),
                names(&row["reloaded"]),
                "execução {run} amostra {n} ({:?})",
                names(&row["installed"])
            );
            total += 1;
        }
    }
    fixed_hashes(&[]);
    assert_eq!(total, 4144);
}

/// Sem a ordem (um NBT que a perdeu, ou editado à mão) a leitura cai na ordem alfabética, e o que difere é o que o `HashMap` empata. Nenhum item guarda a ordem.
#[test]
fn a_nbt_without_the_written_order_falls_back_to_the_sorted_one() {
    let row = &rows(1)[0];
    use_hashes(row);
    let mut saved = tile_of(row).saved();
    saved.file_order.clear();
    let reloaded = UpgradeComponent::new(data(), Rc::new(|| {}), Box::new(|_, _, _| {}));
    reloaded.restore(&saved);
    assert_eq!(in_order(&reloaded).len(), names(&row["installed"]).len());
    // Uma ordem que não é a do conjunto (um tipo a mais, um a menos) vale a alfabética, como se não houvesse.
    saved.file_order = vec![id("speed")];
    let again = UpgradeComponent::new(data(), Rc::new(|| {}), Box::new(|_, _, _| {}));
    again.restore(&saved);
    assert_eq!(in_order(&again), in_order(&reloaded));
    fixed_hashes(&[]);
}

/// O NBT do port é ordenado e não guarda a ordem do arquivo: o `HashMap` parte da ordem alfabética, e a ordem de depois de recarregar só difere da do original quando duas chaves
/// caem no mesmo balde do `HashMap` e na mesma posição da tabela. Medido nas amostras: quantas são e que nenhuma outra difere.
#[test]
fn a_sorted_nbt_reloads_like_the_original_except_for_bucket_ties() {
    let mut total = 0;
    let mut different = 0;
    for run in [1, 2] {
        for row in rows(run) {
            use_hashes(&row);
            let mut sorted: Vec<(String, i32)> = names(&row["tile"])
                .iter()
                .map(|name| (id(name), 1))
                .collect();
            sorted.sort();
            let reloaded: Vec<String> = loaded_map(&entries_from_disk(&sorted))
                .keys_in_order()
                .into_iter()
                .map(|key: UpgradeKey| bare(&key.id()))
                .collect();
            total += 1;
            if reloaded != names(&row["reloaded"]) {
                different += 1;
            }
        }
    }
    fixed_hashes(&[]);
    eprintln!("recarregado do NBT ordenado: {different} de {total} amostras diferem do original");
    assert!(
        different * 20 < total,
        "{different} de {total} amostras diferem"
    );
}

/// O tile posto vazio, sem passar por um item (`UpgradeComponent::new` sem `restore_item`: a tabela de 32 posições do `new Object2IntOpenHashMap<>()`), com os tipos instalados
/// um a um: o rastro de `upgrade_history_3.jsonl` grava os campos `bare_*` de cada amostra (a ordem do tile, a da janela, a do arquivo e a do tile recarregado).
fn bare_tile_of(row: &Value) -> UpgradeComponent {
    let component = UpgradeComponent::new(data(), Rc::new(|| {}), Box::new(|_, _, _| {}));
    for name in names(&row["installed"]) {
        assert_eq!(component.add_upgrades(&id(&name), 1), 1);
    }
    component
}

#[test]
fn a_tile_placed_empty_keeps_the_order_of_the_original_in_every_sample() {
    let mut total = 0;
    for (n, row) in rows(3).iter().enumerate() {
        use_hashes(row);
        let tile = bare_tile_of(row);
        let installed = names(&row["installed"]);
        assert_eq!(
            in_order(&tile),
            names(&row["bare_tile"]),
            "amostra {n} ({installed:?})"
        );
        let ids = data().type_ids();
        let sync = UpgradeSync::of(&tile);
        let listed: Vec<String> = sync
            .installed(&ids)
            .into_iter()
            .map(|index| bare(&ids[index]))
            .collect();
        assert_eq!(
            listed,
            names(&row["bare_window"]),
            "janela, amostra {n} ({installed:?})"
        );
        let saved = tile.saved();
        assert_eq!(
            saved.file_order,
            names(&row["bare_file"]),
            "arquivo, amostra {n} ({installed:?})"
        );
        let reloaded = UpgradeComponent::new(data(), Rc::new(|| {}), Box::new(|_, _, _| {}));
        reloaded.restore(&saved);
        assert_eq!(
            in_order(&reloaded),
            names(&row["bare_reloaded"]),
            "recarregado, amostra {n} ({installed:?})"
        );
        total += 1;
    }
    fixed_hashes(&[]);
    assert_eq!(total, 2072);
}
