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
    CRUSHER_USAGE, ENRICHMENT_USAGE, INFUSER_USAGE, SMELTER_USAGE, electric_config, infuser_config,
    infusing_cache_of,
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
        out.push(
            ingredient
                .trim_start_matches("minecraft:")
                .trim_start_matches("mekanism:")
                .to_owned(),
        );
        return;
    };
    let (namespace, path) = tag.split_once(':').unwrap();
    // As tags do vanilla (`minecraft:`) não estão no conteúdo: o jogo as dá. As que as receitas de que os testes de slot dependem usam.
    if namespace == "minecraft" {
        if let Some(members) = vanilla_tag(path) {
            out.extend(members.iter().map(|id| (*id).to_owned()));
        }
        return;
    }
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

/// Os itens de uma tag de item do vanilla (`#minecraft:<path>`), onde o conteúdo não a traz.
fn vanilla_tag(path: &str) -> Option<&'static [&'static str]> {
    match path {
        "coals" => Some(&["coal", "charcoal"]),
        _ => None,
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
                id: String::new(),
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
        id: String::new(),
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

/// Os químicos de um ingrediente de receita (`"mekanism:bio"` ou `"#mekanism:bio"`), com as tags de químico do conteúdo resolvidas
/// e sem o `mekanism:` (os testes dão aos químicos o id sem namespace).
fn chemical_ids(ingredient: &str, out: &mut Vec<String>) {
    let Some(tag) = ingredient.strip_prefix('#') else {
        out.push(ingredient.trim_start_matches("mekanism:").to_owned());
        return;
    };
    let (namespace, path) = tag.split_once(':').unwrap();
    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("content/data")
        .join(namespace)
        .join("tags")
        .join(namespace)
        .join("chemical")
        .join(format!("{path}.json"));
    let text = std::fs::read_to_string(&file)
        .unwrap_or_else(|error| panic!("{}: {error}", file.display()));
    let json: Value = serde_json::from_str(&text).unwrap();
    for entry in json["values"].as_array().unwrap() {
        chemical_ids(entry.as_str().unwrap(), out);
    }
}

fn chemical_ingredient(ingredient: &str) -> ChemicalIngredient {
    let mut ids = Vec::new();
    chemical_ids(ingredient, &mut ids);
    assert!(
        !ids.is_empty(),
        "{ingredient}: o ingrediente ficou sem químico"
    );
    let shape = IngredientShape::Types(ids.iter().map(|id| Rc::from(id.as_str())).collect());
    ChemicalIngredient::shaped(ShapedIngredient::new(
        shape,
        move |input: &dyn TypedInstance| ids.iter().any(|id| id == input.type_id()),
    ))
}

/// O Infuser com as receitas `metallurgic_infusing` e as conversões `chemical_conversion` que o port entrega, lidas dos mesmos
/// arquivos que o jogo lê (com as tags do conteúdo), na ordem de pasta. Nada escrito à mão.
pub fn infuser_from_content() -> ProgressMachine {
    let (recipes, conversions) = infuser_recipes_from_content();
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

/// As receitas `metallurgic_infusing` e as conversões `chemical_conversion` do conteúdo, na ordem de pasta.
pub fn infuser_recipes_from_content() -> (Vec<Rc<ItemChemicalToItemRecipe>>, Vec<ChemicalConversion>)
{
    infuser_recipes(
        load("recipe/metallurgic_infusing"),
        load("recipe/chemical_conversion"),
        19,
        17,
    )
}

/// O mesmo com todos os arquivos de `recipe/` cujo `type` é o da máquina, em qualquer pasta (o jogo lê o `type`, e o conteúdo guarda
/// receitas de infusão em `control_circuit/` e `processing/`).
pub fn infuser_recipes_by_type() -> (Vec<Rc<ItemChemicalToItemRecipe>>, Vec<ChemicalConversion>) {
    infuser_recipes(
        load_of_type("mekanism:metallurgic_infusing"),
        load_of_type("mekanism:chemical_conversion"),
        30,
        19,
    )
}

/// Os arquivos de `recipe/` (todas as pastas) cujo `type` é `kind`, na ordem de pasta.
pub fn load_of_type(kind: &str) -> Vec<(String, Value)> {
    load("recipe")
        .into_iter()
        .filter(|(_, json)| json["type"] == kind)
        .collect()
}

/// Uma receita `metallurgic_infusing` como o jogo a lê do JSON: entrada, químico, quantidades, saída e `per_tick_usage`.
pub fn infusing_recipe(id: &str, json: &Value) -> Rc<ItemChemicalToItemRecipe> {
    let item = &json["item_input"];
    let chemical = &json["chemical_input"];
    let output = &json["output"];
    Rc::new(ItemChemicalToItemRecipe {
        id: id.to_owned(),
        item_input: item_ingredient(&item["ingredient"]),
        item_count: item["count"].as_u64().unwrap() as u32,
        chemical_input: chemical_ingredient(chemical["ingredient"].as_str().unwrap()),
        chemical_amount: chemical["amount"].as_u64().unwrap() as u32,
        output: ItemOutput {
            resource: item_of(output["id"].as_str().unwrap()),
            count: output["count"].as_u64().unwrap_or(1) as u32,
        },
        per_tick_usage: json["per_tick_usage"].as_bool().unwrap(),
    })
}

fn infuser_recipes(
    infusing: Vec<(String, Value)>,
    converting: Vec<(String, Value)>,
    minimum_recipes: usize,
    minimum_conversions: usize,
) -> (Vec<Rc<ItemChemicalToItemRecipe>>, Vec<ChemicalConversion>) {
    let recipes: Vec<Rc<ItemChemicalToItemRecipe>> = infusing
        .iter()
        .map(|(id, json)| infusing_recipe(id, json))
        .collect();
    assert!(
        recipes.len() >= minimum_recipes,
        "receitas de infusão: {}",
        recipes.len()
    );
    let conversions: Vec<ChemicalConversion> = converting
        .iter()
        .map(|(id, json)| {
            let output = &json["output"];
            ChemicalConversion {
                id: String::new(),
                input: item_ingredient(&json["input"]["ingredient"]),
                input_count: json["input"]["count"].as_u64().unwrap() as u32,
                output: ChemicalResource::new(
                    output["id"]
                        .as_str()
                        .unwrap()
                        .trim_start_matches("mekanism:"),
                ),
                amount: i32::try_from(output["amount"].as_i64().unwrap())
                    .unwrap_or_else(|_| panic!("{id}")),
            }
        })
        .collect();
    assert!(
        conversions.len() >= minimum_conversions,
        "conversões: {}",
        conversions.len()
    );
    (recipes, conversions)
}

/// As receitas de um tipo (`mekanism:enriching`, `mekanism:crushing`), de qualquer pasta, como o jogo as lê: entrada, quantidade e saída, na ordem de pasta. O que as
/// regras de slot perguntam é só se um item entra em alguma.
pub fn item_recipes_of_type(kind: &str) -> Vec<Rc<ItemToItemRecipe>> {
    let recipes: Vec<Rc<ItemToItemRecipe>> = load_of_type(kind)
        .iter()
        .map(|(id, json)| {
            let input = &json["input"];
            Rc::new(ItemToItemRecipe {
                id: id.clone(),
                input: item_ingredient(&input["ingredient"]),
                input_count: input["count"].as_u64().unwrap_or(1) as u32,
                output: ItemOutput {
                    resource: item_of(json["output"]["id"].as_str().unwrap()),
                    count: json["output"]["count"].as_u64().unwrap_or(1) as u32,
                },
            })
        })
        .collect();
    assert!(recipes.len() >= 10, "{kind}: {} receitas", recipes.len());
    recipes
}

/// Um item de saída de receita pelo id, sem o namespace.
fn item_of(id: &str) -> ItemResource {
    item(id.trim_start_matches("minecraft:"))
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

/// A máquina nova da lib, como o construtor a deixa (capacidade de quatro ticks de consumo), com o uso de energia da máquina e a
/// receita vazia: o que os testes de capacidade e de upgrade medem não depende da receita.
pub fn bare_machine(name: &str, data: &Rc<UpgradeData>) -> ProgressMachine {
    let electric = |usage| {
        let mut config = electric_config(
            usage,
            ItemToItemRecipe::cache_of(Vec::new()),
            0,
            data.clone(),
        );
        config.energy_conversion = energy_conversions();
        ProgressMachine::electric(config)
    };
    match name {
        "enrichment_chamber" => electric(ENRICHMENT_USAGE),
        "crusher" => electric(CRUSHER_USAGE),
        "energized_smelter" => electric(SMELTER_USAGE),
        "metallurgic_infuser" => {
            let mut config = infuser_config(
                INFUSER_USAGE,
                infusing_cache_of(Vec::new()),
                ChemicalConversion::cache_of(Vec::new()),
                0,
                data.clone(),
            );
            config.energy_conversion = energy_conversions();
            ProgressMachine::chemical(config)
        }
        other => panic!("máquina fora da lista: {other}"),
    }
}

// ------------------------------------------------------------------------------------------------------------ a tela do cliente

use machinery_basis_core::container_sync::{
    ContainerSync, Mirror, StackValue, TrackedSource, Value as Synced, wire,
};
use machinery_basis_core::machine_sync::MachineSync;
use machinery_basis_core::tile::RedstoneControl;
use machinery_basis_core::tile_trackers::source;
use mekanism_rules::ScreenSpec;

/// O que o servidor lê de uma máquina para o menu de um jogador (o tile vivo, no jogo): os valores que a tela mostra, na mão do teste.
/// `mirror` faz o caminho inteiro até o cliente: `sendAllDataToRemote`, o pacote em bytes e o `handleWindowProperty` de cada dado.
#[derive(Clone, Debug, Default)]
pub struct Live {
    pub energy: i64,
    pub capacity: i64,
    pub energy_per_tick: i32,
    pub progress: i32,
    pub ticks_required: i32,
    pub chemical: StackValue,
    /// `trackedErrors`, na ordem da lista da máquina.
    pub errors: Vec<bool>,
    pub control_type: RedstoneControl,
    pub powered: bool,
    pub was_powered: bool,
    pub xp_containment: bool,
}

impl TrackedSource for Live {
    fn value(&self, id: u16) -> Synced {
        match id {
            source::SECURITY_FREQUENCY => Synced::Codec {
                hash: 0,
                bytes: vec![0],
            },
            source::SECURITY_MODE => Synced::Enum(0),
            source::CONTROL_TYPE => Synced::Enum(self.control_type.ordinal()),
            source::POWERED => Synced::Boolean(self.powered),
            source::WAS_POWERED => Synced::Boolean(self.was_powered),
            source::LAST_ENERGY_RECEIVED => Synced::Long(0),
            source::MAX_ENERGY => Synced::Long(self.capacity),
            source::ENERGY_PER_TICK => Synced::Int(self.energy_per_tick),
            source::ENERGY => Synced::Long(self.energy),
            source::HAS_XP_CONTAINMENT => Synced::Boolean(self.xp_containment),
            source::OPERATING_TICKS => Synced::Int(self.progress),
            source::TICKS_REQUIRED => Synced::Int(self.ticks_required),
            source::CHEMICAL_TANK => Synced::Stack(self.chemical.clone()),
            error if (source::TRACKED_ERROR..source::MOD).contains(&error) => {
                let index = usize::from(error - source::TRACKED_ERROR);
                Synced::Boolean(self.errors.get(index).copied().unwrap_or(false))
            }
            other => panic!("fonte {other} desconhecida"),
        }
    }
}

impl Live {
    /// O menu como o cliente o tem logo depois de abrir: todos os dados, pelo pacote que o servidor manda.
    pub fn mirror(&self, spec: &ScreenSpec) -> Mirror {
        opened_mirror(spec, self)
    }
}

/// O menu de uma máquina como o cliente o tem logo depois de abrir, com os valores que ela tem agora: o que o servidor lê dela
/// (`MachineSync`) vai em bytes, e cada dado entra na posição dele.
pub fn machine_mirror(spec: &ScreenSpec, machine: &ProgressMachine) -> Mirror {
    opened_mirror(spec, &MachineSync::of(machine))
}

fn opened_mirror(spec: &ScreenSpec, source: &dyn TrackedSource) -> Mirror {
    let list = (spec.trackers)();
    let mut sync = ContainerSync::<u8>::new(&list);
    let packet = wire::Packet::new(
        [0, 64, 0],
        Some("mekanism:test".into()),
        &sync.send_all(source),
    );
    let packet = wire::decode(&wire::encode(&packet)).expect("o pacote volta");
    let mut mirror = Mirror::new(&list);
    for (index, data) in &packet.data {
        assert!(mirror.apply(*index, data), "o dado {index} não foi aceito");
    }
    mirror
}
