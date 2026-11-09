//! A aba criativa do Mekanism Generators (`GeneratorsCreativeTabs`): a aba do mod, com os itens, os blocos e os baldes dos fluidos na ordem de registro
//! (`CreativeTabDeferredRegister.addToDisplay` sobre `GeneratorsItems.ITEMS`, `GeneratorsBlocks.BLOCKS` e `GeneratorsFluids.FLUIDS`), e o que o original acrescenta às abas do
//! vanilla (`addToExistingTabs`). O Horizon põe a aba do mod pelo arquivo `creative_tab/mekanismgenerators.json` (só ids); as abas do vanilla vêm da injeção
//! `creative_tabs`, cuja tabela é `native/creative_tabs_table.rs`, compilada aqui.
#[path = "common/generators.rs"]
mod generators;

#[path = "../native/creative_tabs_table.rs"]
mod table;

use generators::{ITEMS, all_blocks, blocos, content, datagen, declared, json};
use serde_json::Value;

/// Os baldes dos quatro fluidos do Generators na ordem de registro (`GeneratorsFluids`), lidos do rastro dos fluidos (`rastros/fluids.jsonl`, da frente `fluidos`).
fn buckets() -> Vec<String> {
    let path = generators::rastro_path("fluids.jsonl");
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|row| row["mod"] == "mekanismgenerators")
        .map(|row| row["bucket"]["id"].as_str().unwrap().to_owned())
        .collect()
}

/// A aba do mod: os itens simples (`solar_panel`, `hohlraum`, `turbine_blade`), os 24 blocos (o `heat_generator` primeiro) e os 4 baldes, o ícone é o Heat Generator
/// (`registerMain(.., HEAT_GENERATOR.getItemHolder(), ..)`) e o título a chave `constants.mekanismgenerators.mod_name`.
#[test]
fn the_mod_tab_lists_the_registration_order_of_the_original() {
    let tab = json(
        &content().join("data/mekanismgenerators/horizon/creative_tab/mekanismgenerators.json"),
    );
    assert_eq!(tab["title"], "constants.mekanismgenerators.mod_name");
    assert_eq!(tab["icon"], "mekanismgenerators:heat_generator");
    let mut expected: Vec<String> = ITEMS
        .iter()
        .map(|id| format!("mekanismgenerators:{id}"))
        .collect();
    expected.extend(
        all_blocks()
            .iter()
            .map(|id| format!("mekanismgenerators:{id}")),
    );
    let buckets = buckets();
    assert_eq!(buckets.len(), 4);
    expected.extend(buckets);
    let ours: Vec<String> = tab["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(ours, expected);
    assert_eq!(ours.len(), 31);
    // Todo item da aba existe no port, e a aba não tem item repetido.
    let have: std::collections::BTreeSet<String> = declared("item")
        .into_iter()
        .chain(declared("block"))
        .map(|id| format!("mekanismgenerators:{id}"))
        .collect();
    for id in &ours {
        assert!(have.contains(id), "{id}");
    }
    assert_eq!(
        ours.iter().collect::<std::collections::BTreeSet<_>>().len(),
        ours.len()
    );
    // A aba do rastro (o gerador) é a mesma.
    assert_eq!(datagen()["creative"]["tab"], tab);
}

/// O que `addToExistingTabs` acrescenta às abas do vanilla: os seis geradores nos blocos funcionais (na ordem da lista do original, não na do registro), os blocos com
/// comparador (`AttributeComparator`: os seis geradores e a `turbine_valve`) na redstone, na ordem do registro, os quatro baldes nas ferramentas e o hohlraum e o painel solar nos ingredientes.
#[test]
fn the_vanilla_tabs_get_what_the_original_adds() {
    let list = |tab: &str| -> Vec<&'static str> {
        table::VANILLA_TABS
            .iter()
            .find(|(id, _)| *id == tab)
            .unwrap_or_else(|| panic!("sem a aba {tab}"))
            .1
            .to_vec()
    };
    assert_eq!(table::VANILLA_TABS.len(), 4);
    assert_eq!(
        list("minecraft:functional_blocks"),
        [
            "mekanismgenerators:heat_generator",
            "mekanismgenerators:solar_generator",
            "mekanismgenerators:advanced_solar_generator",
            "mekanismgenerators:wind_generator",
            "mekanismgenerators:bio_generator",
            "mekanismgenerators:gas_burning_generator",
        ]
    );
    let redstone: Vec<String> = blocos()
        .iter()
        .filter(|row| row["redstone_tab"] == true)
        .map(|row| row["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(list("minecraft:redstone_blocks"), redstone);
    assert_eq!(redstone.len(), 7);
    assert_eq!(redstone.last().unwrap(), "mekanismgenerators:turbine_valve");
    assert_eq!(list("minecraft:tools_and_utilities"), buckets());
    assert_eq!(
        list("minecraft:ingredients"),
        [
            "mekanismgenerators:hohlraum",
            "mekanismgenerators:solar_panel"
        ]
    );
    // As abas do vanilla que o Generators não toca ficam como estão.
    let mut untouched = vec!["{count:1,id:\"minecraft:stone\"}".to_owned()];
    assert!(!table::edit_tab(
        "minecraft:building_blocks",
        &mut untouched
    ));
    assert_eq!(untouched.len(), 1);
    // A conta: o fim da aba ganha as pilhas simples, na ordem da lista.
    let mut stacks = vec!["{count:1,id:\"minecraft:stone\"}".to_owned()];
    assert!(table::edit_tab("minecraft:ingredients", &mut stacks));
    assert_eq!(
        stacks,
        [
            "{count:1,id:\"minecraft:stone\"}",
            "{count:1,id:\"mekanismgenerators:hohlraum\"}",
            "{count:1,id:\"mekanismgenerators:solar_panel\"}",
        ]
    );
    // Todo id da tabela existe no port.
    let have: std::collections::BTreeSet<String> = declared("item")
        .into_iter()
        .chain(declared("block"))
        .map(|id| format!("mekanismgenerators:{id}"))
        .collect();
    for (_, ids) in table::VANILLA_TABS {
        for id in ids {
            assert!(have.contains(*id), "{id}");
        }
    }
    let _ = datagen();
}
