//! A ordem das receitas de cada tipo que as máquinas portadas usam, contra a do Mekanism original
//! (`dev/referencia/rastros/recipe_order.jsonl`, escrito por `SlotsTraceTest.recipeOrder`).
//!
//! O original lê as listas de `MekanismRecipeType.getRecipes`, que vêm do `RecipeMap` do 26.3: o `RecipeManager` registra as
//! receitas na ordem em que `ResourceManagerRegistryLoadTask` as entrega (`sorted(Entry.comparingByKey())`, a chave sendo o
//! caminho do arquivo, com `.json`, e depois o namespace), e `RecipeMap.create` e `order` mantêm essa ordem por tipo. Não é a ordem
//! de um mapa de hash. O Horizon (`crafting.rs`, `registration_key`) ordena do mesmo jeito antes de entregar
//! `native_recipes_of_type`, então a lista do port é o conteúdo ordenado por essa chave.
//!
//! A medida do original tem 8 listas. Aqui se confere que cada uma está nessa ordem, que o conteúdo do port (as receitas copiadas da
//! datagen) sai na mesma ordem relativa, e que a receita que o índice escolhe para um item que mais de uma receita casa é a mesma.
mod common;

use machinery_basis_core::cached_recipe::{ItemOutput, ItemToItemRecipe};
use machinery_basis_core::containers::ItemResource;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// `registration_key` de `crates/horizon-client/src/crafting.rs` no pin `f319701`: o caminho do arquivo (`.json` incluso) e depois o
/// namespace, como o `Identifier` de arquivo do vanilla compara.
fn registration_key(id: &str) -> (String, String) {
    let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", id));
    (format!("{path}.json"), namespace.to_owned())
}

/// A chave errada que um mapa por namespace daria, para provar que o teste distingue.
fn namespace_first_key(id: &str) -> (String, String) {
    let (namespace, path) = id.split_once(':').unwrap();
    (namespace.to_owned(), path.to_owned())
}

fn trace() -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/recipe_order.jsonl");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn line(trace: &[Value], kind: &str) -> Value {
    trace
        .iter()
        .find(|line| line["type"] == kind)
        .unwrap_or_else(|| panic!("o rastro não tem {kind}"))
        .clone()
}

fn ids(line: &Value) -> Vec<String> {
    line["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .collect()
}

/// Cada receita do conteúdo dos dois mods: o id (`<namespace>:<caminho sem .json>`) e o arquivo.
fn content_recipes() -> Vec<(String, Value)> {
    let mods = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut found = Vec::new();
    for (mod_dir, namespace) in [
        ("mekanism", "mekanism"),
        ("mekanismgenerators", "mekanismgenerators"),
    ] {
        let root = mods
            .join(mod_dir)
            .join("content/data")
            .join(namespace)
            .join("recipe");
        let mut pending: Vec<PathBuf> = vec![root.clone()];
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    pending.push(path);
                } else if path.extension().is_some_and(|ext| ext == "json") {
                    let relative = path.strip_prefix(&root).unwrap().with_extension("");
                    let id = format!("{namespace}:{}", relative.to_str().unwrap());
                    let json = serde_json::from_str(&std::fs::read_to_string(&path).unwrap());
                    found.push((id, json.unwrap()));
                }
            }
        }
    }
    found
}

/// Os ids do conteúdo cujo `type` é `kind`, na ordem do jogo.
fn port_order(kind: &str) -> Vec<String> {
    let mut found: Vec<String> = content_recipes()
        .into_iter()
        .filter(|(_, json)| json["type"] == format!("mekanism:{kind}"))
        .map(|(id, _)| id)
        .collect();
    found.sort_by_cached_key(|id| registration_key(id));
    found
}

/// As oito listas do original estão na ordem do arquivo (caminho, depois namespace): nenhuma é de mapa de hash, e a de
/// `smelting` (as receitas de cozimento do vanilla vistas pelo Mekanism) segue a do vanilla.
#[test]
fn every_list_of_the_original_is_in_registration_order() {
    let trace = trace();
    assert_eq!(trace.len(), 8, "tipos no rastro");
    for list in &trace {
        let found = ids(list);
        assert_eq!(found.len() as u64, list["count"].as_u64().unwrap());
        let mut sorted = found.clone();
        sorted.sort_by_cached_key(|id| registration_key(id.trim_start_matches("minecraft:/")));
        // Os ids sintéticos de `smelting` (`RegistryUtils.syntheticKey`) trazem `/mekanism_generated/` depois do namespace: sem
        // isso são os do cozimento do vanilla, um a um, na mesma ordem.
        if list["type"] == "smelting" {
            let plain: Vec<String> = found
                .iter()
                .map(|id| id.replace(":/mekanism_generated/", ":"))
                .collect();
            let vanilla = ids(&line(&trace, "vanilla_smelting"));
            assert_eq!(plain, vanilla, "smelting segue o cozimento do vanilla");
        } else {
            assert_eq!(found, sorted, "{}", list["type"]);
        }
    }
}

/// Para provar que o teste enxerga: a chave por namespace daria outra ordem em pelo menos uma lista com dois namespaces.
#[test]
fn the_wrong_key_would_have_been_caught() {
    let trace = trace();
    let mut differing = Vec::new();
    for kind in ["enriching", "metallurgic_infusing", "vanilla_blasting"] {
        let found = ids(&line(&trace, kind));
        let mut wrong = found.clone();
        wrong.sort_by_cached_key(|id| namespace_first_key(id));
        if wrong != found {
            differing.push(kind);
        }
    }
    assert_eq!(
        differing,
        ["enriching", "metallurgic_infusing", "vanilla_blasting"]
    );
}

/// O conteúdo do port, ordenado como o Horizon o entrega, sai na mesma ordem relativa da lista do original, para os cinco tipos de
/// receita próprios do Mekanism que as máquinas portadas usam. As do original que o port não tem (os 18 `enriching` do Mekanism
/// Additions, que o port não porta) ficam de fora da comparação, e nenhuma receita do port falta no original.
#[test]
fn the_port_lists_each_type_in_the_order_of_the_original() {
    let trace = trace();
    let mut compared = 0;
    for (kind, minimum) in [
        ("enriching", 150),
        ("crushing", 200),
        ("metallurgic_infusing", 25),
        ("chemical_conversion", 15),
        ("energy_conversion", 2),
    ] {
        let original = ids(&line(&trace, kind));
        let port = port_order(kind);
        assert!(port.len() >= minimum, "{kind}: só {} receitas", port.len());
        let in_port: BTreeSet<&String> = port.iter().collect();
        let missing: Vec<&String> = port.iter().filter(|id| !original.contains(id)).collect();
        assert!(missing.is_empty(), "{kind}: o original não tem {missing:?}");
        let relative: Vec<String> = original
            .iter()
            .filter(|id| in_port.contains(id))
            .cloned()
            .collect();
        assert_eq!(port, relative, "{kind}");
        compared += port.len();
    }
    assert!(compared >= 400, "só {compared} receitas comparadas");
}

/// O que o índice escolhe quando mais de uma receita casa o mesmo item: o original grava todo item do jogo que isso acontece, e a
/// receita que `findTypeBasedRecipeHolder` devolve. No port, o índice de `crushing` montado com o conteúdo na ordem do jogo devolve a
/// mesma. É o único caso do original nos tipos de uma entrada (`pitcher_pod` casa a própria receita e `seeds`), e as duas dão o
/// mesmo `bio_fuel`, então a ordem não muda nada que o jogador veja hoje.
#[test]
fn the_recipe_picked_for_an_item_two_recipes_match_is_the_same() {
    let trace = trace();
    let mut checked = 0;
    for kind in [
        "enriching",
        "crushing",
        "chemical_conversion",
        "energy_conversion",
    ] {
        let list = line(&trace, kind);
        let conflicts = list["conflicts"].as_array().unwrap();
        if kind != "crushing" {
            assert!(conflicts.is_empty(), "{kind}: {conflicts:?}");
            continue;
        }
        let recipes: Vec<(String, Value)> = {
            let wanted: BTreeSet<String> = port_order(kind).into_iter().collect();
            let mut all: Vec<(String, Value)> = content_recipes()
                .into_iter()
                .filter(|(id, _)| wanted.contains(id))
                .collect();
            all.sort_by_cached_key(|(id, _)| registration_key(id));
            all
        };
        let built = recipes
            .iter()
            .map(|(id, json)| {
                Rc::new(ItemToItemRecipe {
                    id: id.clone(),
                    input: common::item_ingredient(&json["input"]["ingredient"]),
                    input_count: json["input"]["count"].as_u64().unwrap() as u32,
                    output: ItemOutput {
                        resource: ItemResource::new(
                            json["output"]["id"]
                                .as_str()
                                .unwrap()
                                .trim_start_matches("mekanism:"),
                            64,
                        ),
                        count: json["output"]["count"].as_u64().unwrap_or(1) as u32,
                    },
                })
            })
            .collect();
        let cache = ItemToItemRecipe::cache_of(built);
        assert!(!conflicts.is_empty(), "o rastro não tem o caso");
        for conflict in conflicts {
            let item = conflict["item"]
                .as_str()
                .unwrap()
                .trim_start_matches("minecraft:");
            let listed = conflict["ids"].as_array().unwrap();
            let in_port: Vec<&str> = listed
                .iter()
                .map(|id| id.as_str().unwrap())
                .filter(|id| recipes.iter().any(|(own, _)| own == id))
                .collect();
            if in_port.len() < 2 {
                continue;
            }
            let found = cache
                .find_type_based_recipe(&ItemResource::new(item, 64))
                .unwrap();
            assert_eq!(
                Some(found.id.as_str()),
                conflict["first"].as_str(),
                "{kind} {item}"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 1, "casos conferidos");
}
