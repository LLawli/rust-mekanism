//! A aba do livro de receitas (`GuiRecipeBookWindowTab`) contra o cliente oficial: o sprite copiado byte a byte (a soma FNV-1a do arquivo do `mekanism-ref`), o texto do
//! tooltip, onde a aba fica e se aparece (o `elements` de cada caso de `rastros/fora.jsonl`, gravado pela `CenaTelas`) e onde o clique cai (a grade de
//! `hasClickedOutside` do modo `completo`, com a aba visível). A janela do livro, que o clique abre no original, não está portada: ver o plano (decisão 260).
use machinery_basis_core::screen_elements;
use machinery_basis_core::side_tab::{RECIPE_BOOK_TAB_TINT, RECIPE_BOOK_TAB_Y, recipe_book_tab};
use mekanism_rules::{
    ELECTRIC_SCREEN, INFUSER_SCREEN, RECIPE_BOOK_TAB_ICON, RECIPE_BOOK_TAB_TOOLTIP,
    SCREEN_IMAGE_WIDTH, recipe_book_tab as tab, screen_elements as machine_elements,
};
use serde_json::Value;
use std::path::{Path, PathBuf};

mod common;
use common::Live;

fn content() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("content/assets/mekanism")
}

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn trace() -> Vec<Value> {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../dev/referencia/rastros/fora.jsonl");
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// O caso é de uma das quatro máquinas (as telas de `IRecipeLookupHandler`).
fn is_machine(line: &Value) -> bool {
    ["chamber", "crusher", "smelter", "infuser"]
        .iter()
        .any(|machine| line["case"].as_str().unwrap().starts_with(machine))
}

fn machines() -> Vec<Value> {
    trace()
        .into_iter()
        .filter(|line| line["mode"] == "completo" && is_machine(line))
        .collect()
}

/// Se o clique em (x, y), em pixels de GUI relativos ao canto da tela, não é "fora" no original.
fn over_of(line: &Value) -> impl Fn(i64, i64) -> bool {
    let rows: Vec<Vec<[i64; 2]>> = line["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            row.as_array()
                .unwrap()
                .iter()
                .map(|run| [run[0].as_i64().unwrap(), run[1].as_i64().unwrap()])
                .collect()
        })
        .collect();
    let y0 = line["y0"].as_i64().unwrap();
    move |x, y| {
        rows[(y - y0) as usize]
            .iter()
            .any(|[a, b]| (*a..*b).contains(&x))
    }
}

/// O ícone é o arquivo do original (`src/main/resources/assets/mekanism/textures/gui/sprites/button/recipe_book.png`: 217 bytes), e o suporte e o fundo do botão que a aba usa
/// são os de sempre.
#[test]
fn the_icon_is_the_one_of_the_original_byte_for_byte() {
    let bytes = std::fs::read(content().join("textures/gui/sprites/button/recipe_book.png")).unwrap();
    assert_eq!((bytes.len(), fnv(&bytes)), (217, 0xd5c1_5b7a_9f85_ebea));
    assert_eq!(
        RECIPE_BOOK_TAB_ICON,
        "mekanism:gui/sprites/button/recipe_book"
    );
    for name in ["holder_right", "button", "button_focus"] {
        assert!(
            content()
                .join(format!("textures/gui/sprites/{name}.png"))
                .is_file(),
            "{name}"
        );
    }
}

/// `MekanismLang.RECIPE_BOOK` do `en_us.json` que a datagen do original escreve (`gui.mekanism.recipe_book`).
#[test]
fn the_tooltip_is_the_text_of_the_original() {
    let json: Value =
        serde_json::from_str(&std::fs::read_to_string(content().join("lang/en_us.json")).unwrap())
            .unwrap();
    assert_eq!(RECIPE_BOOK_TAB_TOOLTIP, "gui.mekanism.recipe_book");
    assert_eq!(json[RECIPE_BOOK_TAB_TOOLTIP], "Recipe Book");
    let original = std::fs::read_to_string(
        "/mnt/dados/caches/rust-mekanism-ref/mekanism/src/datagen/generated/mekanism/assets/mekanism/lang/en_us.json",
    );
    if let Ok(original) = original {
        let original: Value = serde_json::from_str(&original).unwrap();
        assert_eq!(original[RECIPE_BOOK_TAB_TOOLTIP], json[RECIPE_BOOK_TAB_TOOLTIP]);
    }
}

/// Em cada estado das quatro máquinas o original põe a aba em (176, 62), 26 por 26, visível e ativa, e o port tem o mesmo suporte e o botão de dentro em (179, 66).
#[test]
fn the_tab_sits_where_the_original_puts_it_in_every_state() {
    let mut checked = 0;
    for line in machines() {
        let element = line["elements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|element| element["class"] == "GuiRecipeBookWindowTab")
            .unwrap_or_else(|| panic!("sem a aba em {}", line["case"]));
        let rect: Vec<f32> = ["x", "y", "w", "h"]
            .iter()
            .map(|key| element[*key].as_f64().unwrap() as f32)
            .collect();
        assert_eq!(tab().holder().to_vec(), rect, "{}", line["case"]);
        assert_eq!(element["visible"], true, "{}", line["case"]);
        assert_eq!(element["active"], true, "{}", line["case"]);
        checked += 1;
    }
    assert_eq!(checked, 16);
    assert_eq!(RECIPE_BOOK_TAB_Y, 62);
    assert_eq!(RECIPE_BOOK_TAB_TINT, 0xFFFF_FFFF);
    assert_eq!(recipe_book_tab(SCREEN_IMAGE_WIDTH).button(), [179.0, 66.0, 18.0, 18.0]);
}

/// A grade de `hasClickedOutside` do modo `completo`: o botão de 18 por 18 responde, e o suporte em volta (3 pixels à esquerda, 5 à direita, 4 em cima e embaixo), não.
/// O port declara o mesmo botão em `screen_elements`, então o clique nele não solta o item do cursor.
#[test]
fn the_button_is_what_the_original_finds_under_the_mouse() {
    let mut checked = 0;
    for line in machines() {
        let case = line["case"].as_str().unwrap();
        let spec = match case.split('_').next().unwrap() {
            "infuser" => INFUSER_SCREEN,
            _ => ELECTRIC_SCREEN,
        };
        let over = over_of(&line);
        let menu = format!("test:recipe_book_tab:{case}");
        screen_elements::declare(&menu, machine_elements(spec));
        let mirror = Live::default().mirror(&spec);
        for (x, y) in [(179, 66), (196, 83), (188, 75), (179, 83), (196, 66)] {
            assert!(over(x, y), "{case} em ({x}, {y}): o original responde no botão");
            assert!(
                screen_elements::over_element(&menu, [x as f32, y as f32], &mirror),
                "{case} em ({x}, {y}): o port declara o botão"
            );
        }
        // O suporte em volta não é elemento, nos dois jogos.
        for (x, y) in [(177, 75), (198, 75), (188, 64), (188, 85), (178, 75), (197, 75), (188, 65), (188, 84)] {
            assert!(!over(x, y), "{case} em ({x}, {y})");
            assert!(
                !screen_elements::over_element(&menu, [x as f32, y as f32], &mirror),
                "{case} em ({x}, {y})"
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 16);
}

/// O Heat Generator e o cubo de energia não são `IRecipeLookupHandler`: `createRecipeBookComponent` devolve nulo e a tela não tem a aba, nem o clique sobre ela.
#[test]
fn the_screens_without_recipes_have_no_tab_in_the_original() {
    let mut checked = 0;
    for line in trace().iter().filter(|line| line["mode"] == "completo" && !is_machine(line)) {
        let has = line["elements"]
            .as_array()
            .unwrap()
            .iter()
            .any(|element| element["class"] == "GuiRecipeBookWindowTab");
        assert!(!has, "{}", line["case"]);
        let over = over_of(line);
        assert!(!over(188, 75), "{}", line["case"]);
        checked += 1;
    }
    assert!(checked >= 2, "{checked}");
}
