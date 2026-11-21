//! A ordem das coleções do livro quando o jogador conhece receitas de todo o jogo, contra o original: `RecipeBookKnownTraceTest.java`
//! (`dev/referencia/rastros/recipe_book_known_displays.jsonl` e `recipe_book_known_client.jsonl`) grava a numeração de todos os displays
//! (4.987, por receita) e cenários com o `ClientRecipeBook` de verdade recebendo `add`, `remove` e `clear` de displays de qualquer
//! receita, com as coleções das abas das quatro máquinas a cada `snap`. O cenário `login_*` grava a ordem do `IdentityHashSet` de
//! `ServerRecipeBook.sendInitialRecipeBook` com o `System.identityHashCode` de cada chave. O teste repete tudo com a lógica da lib.
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::PathBuf;

use machinery_basis_core::recipe_book::{ClientBook, Entry};
use machinery_basis_core::recipe_book_data::{DisplayRow, Displays, SendOrder};
use machinery_basis_core::sim::java_hash::identity_set_order;
use serde_json::Value;

fn read(name: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dev/referencia/rastros")
        .join(name);
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// As linhas do livro das quatro máquinas (`recipe_book_displays.jsonl`), por id.
fn book_rows() -> HashMap<i32, Entry> {
    read("recipe_book_displays.jsonl")
        .into_iter()
        .filter(|row| row.get("id").is_some())
        .map(|row| {
            let id = row["id"].as_i64().unwrap() as i32;
            (
                id,
                Entry {
                    id,
                    category: row["category"].as_str().unwrap().to_owned(),
                    group: row["group"].as_i64().map(|group| group as i32),
                },
            )
        })
        .collect()
}

/// Todas as receitas do jogo, na ordem de `getRecipes()`: o nome, o primeiro id e quantos displays.
fn recipes() -> Vec<(String, i32, u8)> {
    let mut next = 0;
    let mut out = Vec::new();
    for row in read("recipe_book_known_displays.jsonl") {
        if row.get("header").is_some() {
            assert_eq!(row["ascending"], true);
            continue;
        }
        let count = row["n"].as_u64().unwrap() as u8;
        out.push((row["recipe"].as_str().unwrap().to_owned(), next, count));
        next += i32::from(count);
    }
    assert_eq!(next, 4987);
    out
}

fn ids(value: &Value) -> Vec<i32> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_i64().unwrap() as i32)
        .collect()
}

fn tabs(book: &ClientBook, line: &Value, what: &str) -> usize {
    let mut compared = 0;
    for (category, lists) in line["tabs"].as_object().unwrap() {
        let ours: Vec<Vec<i32>> = book
            .tab(category)
            .iter()
            .map(|&index| {
                book.collection(index)
                    .entries
                    .iter()
                    .map(|entry| entry.id)
                    .collect()
            })
            .collect();
        let theirs: Vec<Vec<i32>> = lists.as_array().unwrap().iter().map(ids).collect();
        assert_eq!(ours, theirs, "{what}, aba {category}");
        compared += theirs.iter().map(Vec::len).sum::<usize>();
    }
    assert_eq!(
        compared as u64,
        line["listed"].as_u64().unwrap(),
        "{what}: displays listados"
    );
    compared
}

#[test]
fn the_collections_follow_the_table_of_everything_the_player_knows() {
    let rows = book_rows();
    let mut books: HashMap<String, ClientBook> = HashMap::new();
    let mut snaps = 0;
    let mut listed = 0;
    let mut capacities = BTreeSet::new();
    for line in read("recipe_book_known_client.jsonl") {
        assert!(line.get("error").is_none(), "o rastro falhou: {line}");
        let scene = line["scene"].as_str().unwrap().to_owned();
        let book = books.entry(scene.clone()).or_default();
        match line["op"].as_str().unwrap() {
            "add" => {
                for id in ids(&line["ids"]) {
                    match rows.get(&id) {
                        Some(entry) => book.add(entry.clone()),
                        None => book.add_other(id),
                    }
                }
            }
            "remove" => {
                for id in ids(&line["ids"]) {
                    book.remove(id);
                }
            }
            "clear" => book.clear(),
            "snap" => {
                book.rebuild_collections(&[]);
                listed += tabs(book, &line, &scene);
                capacities.insert(book.known_capacity());
                snaps += 1;
            }
            "identity" => {}
            other => panic!("operação {other}"),
        }
    }
    assert_eq!(snaps, 22);
    assert!(listed > 3500, "só {listed} displays comparados");
    // Os cenários passam pelas tabelas de 64 a 8.192.
    assert!(capacities.contains(&8192) && capacities.contains(&4096) && capacities.contains(&2048));
    assert!(capacities.iter().any(|&capacity| capacity <= 256));
}

/// Com tudo conhecido a tabela tem 8.192 e as coleções saem em ordem de id; só com as 614 do livro das máquinas (o que o port contava)
/// a tabela tem 1.024 e a ordem é outra.
#[test]
fn knowing_only_the_machine_books_gives_another_order() {
    let lines = read("recipe_book_known_client.jsonl");
    let snap = |scene: &str| {
        lines
            .iter()
            .find(|line| line["scene"] == scene && line["op"] == "snap")
            .unwrap()["tabs"]
            .clone()
    };
    let all = snap("all_ascending");
    let only = snap("book_only");
    assert_eq!(all, snap("all_shuffled"));
    let mut differ = 0;
    for (category, lists) in all.as_object().unwrap() {
        let firsts: Vec<i32> = lists
            .as_array()
            .unwrap()
            .iter()
            .map(|list| ids(list)[0])
            .collect();
        assert!(
            firsts.windows(2).all(|pair| pair[0] < pair[1]),
            "{category} fora de ordem"
        );
        if only[category] != *lists {
            differ += 1;
        }
    }
    assert!(differ >= 5, "só {differ} abas mudam");
}

#[test]
fn the_login_order_is_the_identity_set_of_the_recipe_keys() {
    let recipes = recipes();
    let table: &'static [DisplayRow] = Box::leak(
        recipes
            .iter()
            .filter(|row| row.2 > 0)
            .map(|row| (&*Box::leak(row.0.clone().into_boxed_str()), row.1, row.2))
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );
    let displays = Displays(table);
    let lines = read("recipe_book_known_client.jsonl");
    let mut scenes = 0;
    for (index, line) in lines.iter().enumerate() {
        if line["op"] != "identity" {
            continue;
        }
        let inserted: Vec<(usize, i32)> = line["inserted"]
            .as_array()
            .unwrap()
            .iter()
            .map(|pair| {
                (
                    pair[0].as_u64().unwrap() as usize,
                    pair[1].as_i64().unwrap() as i32,
                )
            })
            .collect();
        let hashes: HashMap<usize, i32> = inserted.iter().copied().collect();
        // A tabela do `IdentityHashMap`, direto.
        let order =
            identity_set_order(inserted.iter().map(|pair| pair.0), |recipe| hashes[&recipe]);
        let theirs: Vec<usize> = line["iterated"]
            .as_array()
            .unwrap()
            .iter()
            .map(|recipe| recipe.as_u64().unwrap() as usize)
            .collect();
        assert_eq!(order, theirs, "{}", line["scene"]);
        // O pacote de entrada do servidor: os displays das receitas conhecidas, na ordem do conjunto.
        let by_name: HashMap<&str, i32> = inserted
            .iter()
            .map(|(recipe, hash)| (recipes[*recipe].0.as_str(), *hash))
            .collect();
        let known: BTreeSet<i32> = inserted
            .iter()
            .flat_map(|(recipe, _)| {
                let (_, first, count) = recipes[*recipe];
                first..first + i32::from(count)
            })
            .collect();
        let send = SendOrder {
            displays,
            identity: &|name| by_name[name],
            award: &|_| 0,
            ascending: false,
        };
        let ours = send.login(&known);
        let sent = ids(&lines[index + 1]["ids"]);
        assert_eq!(lines[index + 1]["op"], "add");
        assert_eq!(ours.len(), sent.len());
        assert_eq!(
            ours.iter().collect::<BTreeSet<_>>(),
            sent.iter().collect::<BTreeSet<_>>()
        );
        // O port insere pela ordem do nome (a do que o Horizon grava), o rastro por outra: as posições ocupadas são as mesmas e só
        // muda quem fica com qual dentro de uma sondagem.
        let name_order: Vec<usize> = {
            // As receitas sem display (as especiais, que o jogador nunca conhece) não entram no conjunto do port.
            let mut sorted: Vec<usize> = inserted
                .iter()
                .map(|pair| pair.0)
                .filter(|recipe| recipes[*recipe].2 > 0)
                .collect();
            sorted.sort_by_key(|recipe| recipes[*recipe].0.clone());
            identity_set_order(sorted, |recipe| hashes[&recipe])
        };
        let expanded: Vec<i32> = name_order
            .iter()
            .flat_map(|recipe| {
                let (_, first, count) = recipes[*recipe];
                first..first + i32::from(count)
            })
            .collect();
        assert_eq!(ours, expanded);
        scenes += 1;
    }
    assert_eq!(scenes, 6);
}

/// O que entra fora de um comando vai na ordem de um `HashSet` dos hashes de quem premiou; o do `/recipe give` com várias, em ordem
/// de id.
#[test]
fn what_is_awarded_goes_in_hash_order_and_a_command_in_id_order() {
    let displays = Displays(&[
        ("m:a", 10, 1),
        ("m:b", 11, 2),
        ("m:c", 13, 1),
        ("m:d", 14, 1),
    ]);
    let hash = |name: &str| match name {
        "m:a" => 3,
        "m:b" => 18,
        "m:c" => 1,
        _ => 2,
    };
    let send = SendOrder {
        displays,
        identity: &|_| 1,
        award: &hash,
        ascending: false,
    };
    // Tabela de 16, as receitas postas pela ordem do nome: `m:c` (1), `m:b` (18, o balde 2) e depois dela `m:d` (2), `m:a` (3).
    assert_eq!(send.awarded(&[10, 11, 12, 13, 14]), [13, 11, 12, 14, 10]);
    let command = SendOrder {
        ascending: true,
        ..send
    };
    assert_eq!(command.awarded(&[10, 11, 12, 13, 14]), [10, 11, 12, 13, 14]);
}
