//! O que os testes de rastro partilham: os dados de upgrade e as receitas de energia lidos dos mesmos arquivos que o jogo lê. Cada teste
//! usa uma parte.
#![allow(dead_code)]
use machinery_basis_core::cached_recipe::{ItemIngredient, ItemOutput, ItemToItemRecipe};
use machinery_basis_core::chemical::{ChemicalConversion, ChemicalIngredient, ChemicalResource};
use machinery_basis_core::containers::ItemResource;
use machinery_basis_core::energy_conversion::{EnergyConversion, EnergyConversionCache};
use machinery_basis_core::recipe::{ConversionFields, EnergyConversionData};
use machinery_basis_core::recipe_cache::{IngredientShape, ShapedIngredient, TypedInstance};
use machinery_basis_core::tile::ProgressMachine;
use machinery_basis_core::two_input_recipe::ItemChemicalToItemRecipe;
use machinery_basis_core::upgrade::{UpgradeData, UpgradeTags, UpgradeTypes};
use mekanism_rules::upgrade_data as build_upgrade_data;
use mekanism_rules::{
    ENRICHMENT_USAGE, INFUSER_USAGE, electric_config, infuser_config, infusing_cache_of,
};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// A pilha do item de upgrade, que não tem `max_stack_size` próprio.
const UPGRADE_STACK_SIZE: u32 = 64;
const NAMESPACE: &str = "mekanism";

fn content(folder: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("content/data/mekanism")
        .join(folder)
}

/// Os `.json` de uma pasta de dados, cada um com o id que o jogo lhe dá (`FileToIdConverter`): `<ns>:<caminho sem .json>`.
fn read_folder(root: &Path, folder: &Path, files: &mut Vec<(String, Value)>) {
    let mut entries: Vec<_> = std::fs::read_dir(folder)
        .unwrap_or_else(|error| panic!("{}: {error}", folder.display()))
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            read_folder(root, &path, files);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            let relative = path.strip_prefix(root).unwrap().with_extension("");
            let id = format!("{NAMESPACE}:{}", relative.to_str().unwrap());
            let text = std::fs::read_to_string(&path).unwrap();
            files.push((id, serde_json::from_str(&text).unwrap()));
        }
    }
}

fn load(folder: &str) -> Vec<(String, Value)> {
    let root = content(folder);
    let mut files = Vec::new();
    read_folder(&root, &root, &mut files);
    files
}

/// Os tipos de upgrade que o mod entrega, lidos dos mesmos arquivos que o jogo lê.
pub fn upgrade_types() -> UpgradeTypes {
    let (types, errors) = UpgradeTypes::load(&load(mekanism_rules::UPGRADE_FOLDER));
    assert!(errors.is_empty(), "tipos recusados: {errors:?}");
    types
}

/// Os dados de upgrade das máquinas portadas: os tipos e as tags que o mod entrega, e nada escrito à mão.
pub fn upgrade_data() -> Rc<UpgradeData> {
    let (types, errors) = UpgradeTypes::load(&load(mekanism_rules::UPGRADE_FOLDER));
    assert!(errors.is_empty(), "tipos recusados: {errors:?}");
    assert!(types.len() >= 4, "faltam tipos: {}", types.len());
    let tags = UpgradeTags::load(&load(mekanism_rules::UPGRADE_TAGS_FOLDER));
    build_upgrade_data(&types, &tags, UPGRADE_STACK_SIZE)
}

/// Os itens de um ingrediente de receita (`"id"`, `"#tag"`, lista, ou `neoforge:or` com `values`), com as tags do conteúdo
/// resolvidas na ordem do arquivo, e sem o `minecraft:` (os testes dão aos itens do jogo o id sem namespace).
fn ingredient_items_of(ingredient: &Value, out: &mut Vec<String>) {
    match ingredient {
        Value::String(text) => ingredient_items(text, out),
        Value::Array(list) => list
            .iter()
            .for_each(|child| ingredient_items_of(child, out)),
        Value::Object(object) => {
            if let Some(children) = object.get("values") {
                ingredient_items_of(children, out);
            }
            if let Some(id) = object.get("item") {
                ingredient_items_of(id, out);
            }
        }
        other => panic!("ingrediente {other}"),
    }
}

/// O ingrediente de item que o jogo daria a uma receita do conteúdo.
pub fn item_ingredient(ingredient: &Value) -> ItemIngredient {
    let mut items = Vec::new();
    ingredient_items_of(ingredient, &mut items);
    item_ingredient_of(items)
}

fn item_ingredient_of(items: Vec<String>) -> ItemIngredient {
    let shape = IngredientShape::Types(items.iter().map(|id| Rc::from(id.as_str())).collect());
    ItemIngredient::shaped(ShapedIngredient::new(
        shape,
        move |input: &dyn TypedInstance| items.iter().any(|id| id == input.type_id()),
    ))
}

fn ingredient_items(ingredient: &str, out: &mut Vec<String>) {
    let Some(tag) = ingredient.strip_prefix('#') else {
        out.push(ingredient.trim_start_matches("minecraft:").to_owned());
        return;
    };
    let (namespace, path) = tag.split_once(':').unwrap();
    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("content/data")
        .join(namespace)
        .join("tags/item")
        .join(format!("{path}.json"));
    // Uma tag que o conteúdo não tem (as do vanilla, ou de itens que o port não tem) não dá item nenhum, e o Horizon larga a receita
    // que fica sem nenhum.
    if !file.exists() {
        return;
    }
    let text = std::fs::read_to_string(&file)
        .unwrap_or_else(|error| panic!("{}: {error}", file.display()));
    let json: Value = serde_json::from_str(&text).unwrap();
    for entry in json["values"].as_array().unwrap() {
        ingredient_items(entry.as_str().unwrap(), out);
    }
}

/// O `getInputCache` de `ENERGY_CONVERSION` lido dos arquivos que o port entrega (`recipe/energy_conversion`), na ordem de pasta
/// do jogo (o nome do arquivo), com as tags `c:` do conteúdo. Nada escrito à mão.
pub fn energy_conversions() -> Rc<EnergyConversionCache> {
    const FIELDS: ConversionFields = ConversionFields {
        item: "input",
        output: "output",
    };
    let root = content("recipe/energy_conversion");
    let mut files = Vec::new();
    read_folder(&root, &root, &mut files);
    assert!(files.len() >= 2, "receitas de energia: {}", files.len());
    let conversions = files
        .iter()
        .map(|(id, json)| {
            let data = EnergyConversionData::parse(json, &FIELDS)
                .unwrap_or_else(|| panic!("{id}: receita de energia inválida"));
            EnergyConversion {
                input: {
                    let mut items = Vec::new();
                    ingredient_items_of(&data.item.ingredient, &mut items);
                    assert!(!items.is_empty(), "{id}: o ingrediente ficou sem item");
                    item_ingredient_of(items)
                },
                input_count: data.item.count,
                output: data.amount,
            }
        })
        .collect();
    EnergyConversion::cache_of(conversions)
}

/// A pilha de um item de máquina nos rastros, que não tem limite próprio.
const STACK: u32 = 64;

pub fn item(id: &str) -> ItemResource {
    ItemResource::new(id.trim_start_matches("mekanism:"), STACK)
}

pub fn recipe(input: &str, chemical: &str, output: &str) -> Rc<ItemChemicalToItemRecipe> {
    Rc::new(ItemChemicalToItemRecipe {
        id: format!("mekanism:metallurgic_infusing/{output}"),
        item_input: ItemIngredient::item(input),
        item_count: 1,
        chemical_input: ChemicalIngredient::chemical(chemical),
        chemical_amount: 10,
        output: ItemOutput {
            resource: item(output),
            count: 1,
        },
        per_tick_usage: false,
    })
}

pub fn conversion(input: &str, chemical: &str) -> ChemicalConversion {
    ChemicalConversion {
        input: ItemIngredient::item(input),
        input_count: 1,
        output: ChemicalResource::new(chemical),
        amount: 10,
    }
}

/// Uma máquina elétrica com uma receita que usa `valid`.
pub fn chamber(valid: &str) -> ProgressMachine {
    let recipe = Rc::new(ItemToItemRecipe {
        id: valid.to_owned(),
        input: ItemIngredient::item(valid),
        input_count: 1,
        output: ItemOutput {
            resource: item("clay_ball"),
            count: 1,
        },
    });
    let mut config = electric_config(
        ENRICHMENT_USAGE,
        ItemToItemRecipe::cache_of(vec![recipe]),
        0,
        upgrade_data(),
    );
    config.energy_conversion = energy_conversions();
    ProgressMachine::electric(config)
}

/// O Infuser com uma receita por químico que o original usa (`bio`, `fungi`, `carbon`, `redstone`, `diamond`, `gold`, `tin` e
/// `refined_obsidian`), e as conversões de item em químico do original para os itens do rastro.
pub fn infuser() -> ProgressMachine {
    let recipes = vec![
        recipe("dirt", "bio", "podzol"),
        recipe("dirt", "fungi", "mycelium"),
        recipe("iron_ingot", "carbon", "steel_ingot"),
        recipe("copper_ingot", "redstone", "alloy_infused"),
        recipe("alloy_infused", "diamond", "alloy_reinforced"),
        recipe("blackstone", "gold", "gilded_blackstone"),
        recipe("dust_iron", "tin", "dust_bronze"),
        recipe("alloy_reinforced", "refined_obsidian", "alloy_atomic"),
    ];
    let conversions = vec![
        conversion("bio_fuel", "bio"),
        conversion("coal", "carbon"),
        conversion("redstone", "redstone"),
        conversion("redstone_block", "redstone"),
        conversion("red_mushroom", "fungi"),
    ];
    let mut config = infuser_config(
        INFUSER_USAGE,
        infusing_cache_of(recipes),
        ChemicalConversion::cache_of(conversions),
        0,
        upgrade_data(),
    );
    config.energy_conversion = energy_conversions();
    ProgressMachine::chemical(config)
}

/// O item que a receita de cada máquina usa, o mesmo do cenário em Java.
pub fn valid_input(name: &str) -> &'static str {
    match name {
        "enrichment_chamber" => "clay",
        "crusher" => "cobblestone",
        "energized_smelter" => "sand",
        "metallurgic_infuser" => "dirt",
        other => panic!("{other}"),
    }
}

pub fn machine(name: &str) -> ProgressMachine {
    if name == "metallurgic_infuser" {
        infuser()
    } else {
        chamber(valid_input(name))
    }
}
