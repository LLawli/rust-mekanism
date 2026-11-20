//! O que o livro de receitas guarda e confere, contra o original (`dev/referencia/testes/RecipeBookStateTraceTest.java`):
//! - a tabela `REQUIREMENTS` é o rastro `recipe_book_requirements.jsonl`;
//! - o `canCraft` do livro (`MekanismRecipeBookComponent.canCraft` sobre o `StackedItemContents`) marca, em cada cenário de
//!   inventário de `recipe_book_craft.jsonl`, os mesmos displays que o Java;
//! - os campos dos `RecipeBookSettings` no NBT do jogador (`recipe_book_nbt.jsonl`) são os que a `machinery_basis` escreve e lê
//!   para os tipos de livro das quatro máquinas.
use machinery_basis_core::recipe_book_data::{Settings, settings_fields, upper_camel};
use machinery_basis_core::recipe_book_view::{BookData, can_craft};
use machinery_basis_core::stacked_contents::StackedContents;
use mekanism_rules::recipe_book_rows::{REQUIREMENTS, ROWS};
use mekanism_rules::recipe_books::{CRUSHING, ENRICHING, METALLURGIC_INFUSING, SMELTING};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn trace(name: &str) -> Vec<Value> {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(name);
    std::fs::read_to_string(file)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

const BOOKS: [&BookData; 4] = [&ENRICHING, &CRUSHING, &SMELTING, &METALLURGIC_INFUSING];

#[test]
fn the_requirements_table_is_the_trace_of_the_original() {
    let lines = trace("recipe_book_requirements.jsonl");
    assert_eq!(lines.len(), REQUIREMENTS.len());
    assert_eq!(ROWS.len(), REQUIREMENTS.len());
    for (line, (requirement, row)) in lines.iter().zip(REQUIREMENTS.iter().zip(ROWS)) {
        assert_eq!(line["id"], requirement.0);
        assert_eq!(row.0, requirement.0);
        assert_eq!(line["recipe"], row.1);
        let req: Option<Vec<Vec<String>>> = serde_json::from_value(line["req"].clone()).unwrap();
        let ours = requirement.1.map(|list| {
            list.iter()
                .map(|items| items.iter().map(|item| (*item).to_owned()).collect::<Vec<_>>())
                .collect::<Vec<_>>()
        });
        assert_eq!(req, ours, "{}", row.1);
        let counts: Option<Vec<i32>> = serde_json::from_value(line["counts"].clone()).unwrap();
        assert_eq!(counts, requirement.2.map(<[i32]>::to_vec), "{}", row.1);
        // O número de requisitos da tabela dos displays é o tamanho desta lista.
        assert_eq!(row.8, ours.map_or(-1, |list| list.len() as i32));
        // Todas as receitas dos quatro tipos avisam ao entrar no livro (`showNotification`) e nenhuma é especial.
        assert_eq!(line["notify"], true);
        assert_eq!(line["special"], false);
    }
    // Em ordem de id, para a busca binária de `BookData::requirement`.
    assert!(REQUIREMENTS.windows(2).all(|pair| pair[0].0 < pair[1].0));
    assert!(ENRICHING.requirement(REQUIREMENTS[7].0).is_some());
    assert!(ENRICHING.requirement(-5).is_none());
}

#[test]
fn can_craft_marks_the_displays_the_original_marks() {
    let scenarios = trace("recipe_book_craft.jsonl");
    assert_eq!(scenarios.len(), 10);
    let mut marked = 0;
    for scenario in &scenarios {
        let mut contents = StackedContents::default();
        for stack in scenario["stacks"].as_array().unwrap() {
            // A contagem do rastro já passou pelo máximo da pilha do item.
            let count = stack[1].as_i64().unwrap() as i32;
            contents.account_stack(stack[0].as_str().unwrap(), count, count);
        }
        let expected: Vec<i32> = serde_json::from_value(scenario["craftable"].clone()).unwrap();
        let ours: Vec<i32> = REQUIREMENTS
            .iter()
            .filter(|requirement| can_craft(requirement, &contents))
            .map(|requirement| requirement.0)
            .collect();
        assert_eq!(ours, expected, "cenário {}", scenario["scenario"]);
        marked += ours.len();
    }
    // Os cenários marcam de fato alguma coisa, e o de pilhas de um item só marca menos do que o de pilhas cheias.
    assert!(marked > 100, "{marked}");
    assert!(scenarios[6]["craftable"].as_array().unwrap().len() < scenarios[5]["craftable"].as_array().unwrap().len());
}

#[test]
fn a_display_that_asks_for_several_of_an_item_needs_that_many() {
    // `mekanism:enriching/...` com `counts` maior que um: o ingrediente entra tantas vezes quantas o display pede.
    let several = REQUIREMENTS
        .iter()
        .find(|requirement| requirement.2.is_some_and(|counts| counts.first().copied().unwrap_or(0) >= 3))
        .expect("há displays que pedem três ou mais");
    let needed = several.2.unwrap()[0];
    let item = several.1.unwrap()[0][0];
    let mut contents = StackedContents::default();
    contents.account_stack(item, needed - 1, 64);
    assert!(!can_craft(several, &contents));
    contents.account_stack(item, 1, 64);
    assert!(can_craft(several, &contents));
    // Sem a lista de requisitos não dá, e com a lista vazia (só químico ou fluido) dá.
    assert!(!can_craft(&(0, None, None), &contents));
    assert!(can_craft(&(0, Some(&[]), Some(&[0])), &StackedContents::default()));
}

#[test]
fn the_settings_fields_in_the_player_nbt_are_the_ones_of_the_original() {
    let lines = trace("recipe_book_nbt.jsonl");
    let types: Vec<&str> = BOOKS.iter().map(|book| book.settings_type).collect();
    assert_eq!(types, ["MEKANISM_ENRICHING", "MEKANISM_CRUSHING", "MEKANISM_SMELTING", "MEKANISM_METALLURGIC_INFUSING"]);
    // Cada tipo de livro do port é um `RecipeBookType` do original (os de mod que o NeoForge acrescenta ao enum).
    let known: Vec<String> = serde_json::from_value(lines.last().unwrap()["types"].clone()).unwrap();
    for book in BOOKS {
        assert!(known.iter().any(|name| name == book.settings_type), "{}", book.settings_type);
        assert_eq!(book.settings_type, format!("MEKANISM_{}", book.book_type.to_uppercase()));
    }
    let vanilla = ["isGuiOpen", "isFilteringCraftable", "isFurnaceGuiOpen", "isFurnaceFilteringCraftable"];
    let mut cases = 0;
    for line in lines.iter().filter(|line| line.get("case").is_some()) {
        cases += 1;
        let mut settings = Settings::default();
        for change in line["changes"].as_str().unwrap().split_whitespace() {
            let (book_type, what) = change.split_once(':').unwrap();
            if !types.contains(&book_type) {
                continue;
            }
            if what == "open" {
                settings.set_open(book_type, true);
            } else {
                settings.set_filtering(book_type, true);
            }
        }
        // Na escrita o codec omite o campo que vale o padrão (`optionalFieldOf(nome, false)`): só os verdadeiros vão ao NBT.
        let ours: BTreeMap<String, bool> = settings.write(&types).into_iter().filter(|(_, value)| *value).collect();
        let fields: BTreeMap<String, bool> = serde_json::from_value(line["fields"].clone()).unwrap();
        let theirs: BTreeMap<String, bool> = fields.iter().filter(|(name, _)| !vanilla.contains(&name.as_str())).map(|(name, value)| (name.clone(), *value)).collect();
        assert_eq!(ours, theirs, "caso {}", line["case"]);
        // Na leitura o campo ausente vale falso.
        let mut back = Settings::default();
        back.read(&types, |name| fields.get(name).copied());
        for book_type in &types {
            let read = &line["read"][book_type];
            assert_eq!(back.is_open(book_type), read[0].as_bool().unwrap(), "{book_type}");
            assert_eq!(back.is_filtering(book_type), read[1].as_bool().unwrap(), "{book_type}");
        }
        // As listas `recipes` e `toBeDisplayed` levam o id inteiro da receita, e a destacada é sempre conhecida.
        let recipes: Vec<String> = serde_json::from_value(line["recipes"].clone()).unwrap();
        let highlight: Vec<String> = serde_json::from_value(line["toBeDisplayed"].clone()).unwrap();
        assert!(recipes.iter().all(|name| ROWS.iter().any(|row| row.1 == name)));
        assert!(highlight.iter().all(|name| recipes.contains(name)));
    }
    assert_eq!(cases, 4);
    assert_eq!(upper_camel("MEKANISM_METALLURGIC_INFUSING"), "MekanismMetallurgicInfusing");
    assert_eq!(settings_fields("MEKANISM_SMELTING").0, "isMekanismSmeltingGuiOpen");
}
