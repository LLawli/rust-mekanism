//! O item de um bloco que o nível destrói contra o rastro do original (`dev/referencia/testes/TankDropTraceTest.java`,
//! `dev/referencia/rastros/tank_level_drop.jsonl`): `Level.destroyBlock(pos, true)` e uma explosão que quebra o bloco deixam no chão só o que a tabela de loot dá com
//! o tile (o conteúdo, os slots e a configuração vão dentro do item, nada solto ao lado), e a TNT (potência 4) não quebra um tanque.
use machinery_basis_core::level_drop::{retell, split};
use serde_json::Value;
use std::path::Path;

fn rows() -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dev/referencia/rastros/tank_level_drop.jsonl");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// Um item na fila do nível: a pilha e onde nasceu.
#[derive(Clone, Debug, PartialEq)]
struct Spawn {
    stack: Value,
    at: [f64; 3],
}

const BLOCK: [i32; 3] = [-7, 62, 3];

#[test]
fn the_level_leaves_only_what_the_loot_table_gives_with_the_tile() {
    let rows = rows();
    let mut broken = 0;
    for row in &rows {
        let case = row["case"].as_str().unwrap();
        let block = row["block"].as_str().unwrap();
        let loot = row["loot"].as_array().unwrap().clone();
        let entities = row["entities"].as_array().unwrap();
        if !row["broken"].as_bool().unwrap() {
            // A TNT não quebra um bloco de resistência 16: nada no chão.
            assert_eq!(row["how"], "tnt", "{case}");
            assert!(entities.is_empty(), "{case}");
            continue;
        }
        broken += 1;
        // A fila como o nível a deixa: o item do bloco sem nada (a tabela rolada sem o tile), nascido dentro do bloco, entre itens que não são dele (um item de
        // outro tipo no mesmo bloco e um igual no bloco vizinho). Os slots do tile não estão nela: vão dentro do item.
        let bare = serde_json::json!({ "id": block, "count": 1 });
        let stranger = Spawn {
            stack: serde_json::json!({ "id": "minecraft:stone", "count": 1 }),
            at: [-6.5, 62.5, 3.5],
        };
        let neighbour = Spawn {
            stack: bare.clone(),
            at: [-5.6, 62.5, 3.5],
        };
        let queue = vec![
            stranger.clone(),
            Spawn {
                stack: bare,
                at: [-6.7, 62.4, 3.6],
            },
            neighbour.clone(),
        ];
        let (mine, rest) = split(
            queue,
            BLOCK,
            |spawn| spawn.at,
            |spawn| spawn.stack["id"] == block,
        );
        assert_eq!(rest, vec![stranger, neighbour], "{case}");
        // A tabela de loot com o tile fala (`loot`), e cada pilha volta ao lugar da que ela era.
        let told = retell(mine, loot, |spawn, stack| spawn.stack = stack);
        let stacks: Vec<&Value> = told.iter().map(|spawn| &spawn.stack).collect();
        assert_eq!(stacks, entities.iter().collect::<Vec<_>>(), "{case}");
        assert!(
            told.iter().all(|spawn| spawn.at == [-6.7, 62.4, 3.6]),
            "{case}"
        );
    }
    // Os quatro tanques, destruídos pelo nível e pela explosão de potência 6.
    assert_eq!(broken, 8);
    assert_eq!(rows.len(), 12);
}

#[test]
fn the_item_of_a_full_tank_carries_the_contents_the_slots_and_the_mode() {
    let rows = rows();
    let of = |case: &str| {
        rows.iter()
            .find(|row| row["case"] == case)
            .unwrap_or_else(|| panic!("{case}"))["entities"][0]["components"]
            .clone()
    };
    for how in ["destroy", "explode"] {
        let fluid = of(&format!("fluid_full_{how}"));
        assert_eq!(
            fluid["mekanism:fluids"]["containers"][0],
            serde_json::json!({ "type": { "id": "minecraft:lava" }, "amount": 5000 })
        );
        assert_eq!(
            fluid["mekanism:items"]["containers"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(fluid["mekanism:edit_mode"], "fill");
        let chemical = of(&format!("chemical_full_{how}"));
        assert_eq!(
            chemical["mekanism:chemicals"]["containers"][0],
            serde_json::json!({ "type": "mekanism:hydrogen", "amount": 5000 })
        );
        assert_eq!(chemical["mekanism:dump_mode"], "dumping_excess");
        assert_eq!(
            chemical["mekanism:side_config"]["config"]["chemicals"],
            serde_json::json!({ "side": { "front": "output" }, "eject": true })
        );
        // Um tanque posto por `setBlock` e não mexido leva a configuração vazia (decisão 132).
        assert_eq!(
            of(&format!("chemical_plain_{how}"))["mekanism:side_config"]["config"]["chemicals"],
            serde_json::json!({ "side": {} })
        );
    }
}
