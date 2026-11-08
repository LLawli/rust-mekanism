//! Receitas de tipo próprio, lidas do JSON. A lib não conhece os campos de nenhum mod: quem usa diz
//! onde cada parte fica. Os ingredientes de item seguem como JSON, porque quem os resolve é o jogo.
use crate::recipe_cache::{IngredientShape, ShapedIngredient};
use crate::resource::{TAG_PREFIX, Tags};
use serde_json::Value;
use std::rc::Rc;
use std::sync::Arc;

const ITEM_COUNT: &str = "count";
const ITEM_INGREDIENT: &str = "ingredient";
const RESOURCE_AMOUNT: &str = "amount";
const RESOURCE_ID: &str = "id";
const OR_INGREDIENT: &str = "neoforge:or";
const PER_TICK_USAGE: &str = "per_tick_usage";

/// Um item de entrada: o ingrediente como o arquivo o escreve, e quantos.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemInput {
    pub ingredient: Value,
    pub count: u32,
}

/// Uma entrada de recurso: um id, ou uma tag com `#`, e quanto.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceInput {
    pub ingredient: String,
    pub amount: u64,
}

impl ResourceInput {
    pub fn accepts(&self, resource: &str, tags: &Tags) -> bool {
        match self.ingredient.strip_prefix(TAG_PREFIX) {
            Some(tag) => tags.contains(tag, resource),
            None => self.ingredient == resource,
        }
    }

    /// Os químicos que a entrada aceita, para o índice de receitas (`ChemicalStackIngredient.chemicals()`): o id, ou os membros
    /// da tag como estavam na carga de dados.
    pub fn chemicals(&self, tags: &Tags) -> Vec<Rc<str>> {
        match self.ingredient.strip_prefix(TAG_PREFIX) {
            Some(tag) => tags.members(tag).map(Rc::from).collect(),
            None => vec![Rc::from(self.ingredient.as_str())],
        }
    }

    /// O ingrediente do índice de receitas: a lista de [`ResourceInput::chemicals`] e o teste de tipo que
    /// [`ResourceInput::accepts`] faz, com as tags desta carga de dados.
    pub fn shaped(&self, tags: Arc<Tags>) -> ShapedIngredient {
        let shape = IngredientShape::Types(self.chemicals(&tags));
        let input = self.clone();
        ShapedIngredient::new(shape, move |chemical| {
            input.accepts(chemical.type_id(), &tags)
        })
    }
}

/// Onde ficam as partes de uma receita de máquina. Sem `resource`, ela só pede o item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecipeFields {
    pub item: &'static str,
    pub resource: Option<&'static str>,
    pub output: &'static str,
}

/// Uma receita de máquina: um item e, às vezes, um recurso, que dão um item.
#[derive(Clone, Debug, PartialEq)]
pub struct MachineRecipeData {
    pub item: ItemInput,
    pub resource: Option<ResourceInput>,
    pub output: Value,
    /// `per_tick_usage`: a receita gasta o recurso a cada tick, e não de uma vez no fim. Sem o campo, não gasta.
    pub per_tick_usage: bool,
}

/// Onde ficam as partes de uma receita que converte um item em recurso.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConversionFields {
    pub item: &'static str,
    pub output: &'static str,
}

/// Um item que vira recurso.
#[derive(Clone, Debug, PartialEq)]
pub struct Conversion {
    pub item: ItemInput,
    pub resource: String,
    pub amount: u64,
}

/// Um item que vira energia (`ItemStackToEnergyRecipe`): o campo de saída é um inteiro, `ExtraCodecs.POSITIVE_INT`.
#[derive(Clone, Debug, PartialEq)]
pub struct EnergyConversionData {
    pub item: ItemInput,
    pub amount: i32,
}

/// O `neoforge:or` é só uma lista, a forma que o jogo aceita de um mod. Um `or` dentro de outro `or` (ou de uma lista) é a
/// mesma união: o jogo só lê as cadeias de uma lista e descartaria o resto, então a lista sai achatada, na ordem dos filhos
/// (`CompoundIngredient.items()`).
pub fn plain_ingredient(json: &Value) -> Value {
    if json.get("type").and_then(Value::as_str) == Some(OR_INGREDIENT) {
        let mut flat = Vec::new();
        flatten_or(json, &mut flat);
        return Value::Array(flat);
    }
    json.clone()
}

fn flatten_or(json: &Value, flat: &mut Vec<Value>) {
    match json {
        Value::Array(list) => list.iter().for_each(|child| flatten_or(child, flat)),
        Value::Object(object)
            if json.get("type").and_then(Value::as_str) == Some(OR_INGREDIENT) =>
        {
            if let Some(values) = object.get("values") {
                flatten_or(values, flat);
            }
        }
        other => flat.push(other.clone()),
    }
}

fn item_input(json: &Value) -> Option<ItemInput> {
    let count = json.get(ITEM_COUNT).map_or(Some(1), Value::as_u64)?;
    Some(ItemInput {
        ingredient: plain_ingredient(json.get(ITEM_INGREDIENT)?),
        count: u32::try_from(count).ok().filter(|&count| count > 0)?,
    })
}

fn amount(json: &Value) -> Option<u64> {
    json.get(RESOURCE_AMOUNT)
        .and_then(Value::as_u64)
        .filter(|&amount| amount > 0)
}

fn resource_input(json: &Value) -> Option<ResourceInput> {
    Some(ResourceInput {
        ingredient: json.get(ITEM_INGREDIENT)?.as_str()?.to_owned(),
        amount: amount(json)?,
    })
}

/// Por que `item_input` não deu um [`ItemInput`], no texto de um erro de codec.
fn why_not_item(json: &Value, field: &str) -> Option<String> {
    let Some(input) = json.get(field) else {
        return Some(format!("falta `{field}`"));
    };
    if input.get(ITEM_INGREDIENT).is_none() {
        return Some(format!("falta `{field}.{ITEM_INGREDIENT}`"));
    }
    if item_input(input).is_none() {
        return Some(format!(
            "`{field}.{ITEM_COUNT}` tem de ser um inteiro positivo"
        ));
    }
    None
}

impl MachineRecipeData {
    /// Por que [`MachineRecipeData::parse`] devolveu `None`: a primeira parte que falta ou está errada, para o log da receita recusada.
    pub fn why_not(json: &Value, fields: &RecipeFields) -> String {
        if let Some(field) = fields.resource {
            match json.get(field) {
                None => return format!("falta `{field}`"),
                Some(input) if resource_input(input).is_none() => {
                    return format!(
                        "`{field}` precisa de `{ITEM_INGREDIENT}` (texto) e de `{RESOURCE_AMOUNT}` positivo"
                    );
                }
                Some(_) => {}
            }
        }
        why_not_item(json, fields.item)
            .or_else(|| {
                json.get(fields.output)
                    .is_none()
                    .then(|| format!("falta `{}`", fields.output))
            })
            .unwrap_or_else(|| "receita inválida".to_owned())
    }

    pub fn parse(json: &Value, fields: &RecipeFields) -> Option<Self> {
        let resource = match fields.resource {
            Some(field) => Some(resource_input(json.get(field)?)?),
            None => None,
        };
        Some(Self {
            item: item_input(json.get(fields.item)?)?,
            resource,
            output: json.get(fields.output)?.clone(),
            per_tick_usage: json
                .get(PER_TICK_USAGE)
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }
}

impl Conversion {
    /// Por que [`Conversion::parse`] devolveu `None`.
    pub fn why_not(json: &Value, fields: &ConversionFields) -> String {
        if let Some(why) = why_not_item(json, fields.item) {
            return why;
        }
        match json.get(fields.output) {
            None => format!("falta `{}`", fields.output),
            Some(output) if output.get(RESOURCE_ID).and_then(Value::as_str).is_none() => {
                format!("`{}.{RESOURCE_ID}` tem de ser um texto", fields.output)
            }
            Some(_) => format!(
                "`{}.{RESOURCE_AMOUNT}` tem de ser um inteiro positivo",
                fields.output
            ),
        }
    }

    pub fn parse(json: &Value, fields: &ConversionFields) -> Option<Self> {
        let output = json.get(fields.output)?;
        Some(Self {
            item: item_input(json.get(fields.item)?)?,
            resource: output.get(RESOURCE_ID)?.as_str()?.to_owned(),
            amount: amount(output)?,
        })
    }
}

impl EnergyConversionData {
    /// Por que [`EnergyConversionData::parse`] devolveu `None`.
    pub fn why_not(json: &Value, fields: &ConversionFields) -> String {
        why_not_item(json, fields.item).unwrap_or_else(|| {
            format!(
                "`{}` tem de ser um inteiro positivo de 32 bits (`ExtraCodecs.POSITIVE_INT`)",
                fields.output
            )
        })
    }

    /// `fields.item` e `fields.output` dizem onde ficam o item e a energia. Uma energia que não é um inteiro positivo não é
    /// receita (o codec do original recusa o arquivo).
    pub fn parse(json: &Value, fields: &ConversionFields) -> Option<Self> {
        Some(Self {
            item: item_input(json.get(fields.item)?)?,
            amount: i32::try_from(json.get(fields.output)?.as_i64()?)
                .ok()
                .filter(|&amount| amount > 0)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const FIELDS: RecipeFields = RecipeFields {
        item: "item_input",
        resource: Some("chemical_input"),
        output: "output",
    };
    const CONVERSION: ConversionFields = ConversionFields {
        item: "input",
        output: "output",
    };

    fn recipe() -> Value {
        json!({
            "chemical_input": {"amount": 10, "ingredient": "#a:carbon"},
            "item_input": {"count": 4, "ingredient": "#c:ingots/iron"},
            "output": {"id": "a:steel"},
        })
    }

    #[test]
    fn reads_a_recipe_by_the_fields_it_was_told() {
        let parsed = MachineRecipeData::parse(&recipe(), &FIELDS).unwrap();
        assert_eq!(parsed.item.count, 4);
        assert_eq!(parsed.item.ingredient, "#c:ingots/iron");
        assert_eq!(parsed.resource.unwrap().amount, 10);
        assert_eq!(parsed.output, json!({"id": "a:steel"}));
    }

    #[test]
    fn a_recipe_says_whether_it_spends_the_resource_per_tick() {
        let mut json = recipe();
        assert!(
            !MachineRecipeData::parse(&json, &FIELDS)
                .unwrap()
                .per_tick_usage
        );
        json["per_tick_usage"] = json!(true);
        assert!(
            MachineRecipeData::parse(&json, &FIELDS)
                .unwrap()
                .per_tick_usage
        );
        json["per_tick_usage"] = json!(false);
        assert!(
            !MachineRecipeData::parse(&json, &FIELDS)
                .unwrap()
                .per_tick_usage
        );
    }

    #[test]
    fn a_recipe_of_items_alone_has_no_resource() {
        const ITEMS: RecipeFields = RecipeFields {
            item: "input",
            resource: None,
            output: "output",
        };
        let json = json!({"input": {"ingredient": "a:b"}, "output": {"count": 2, "id": "a:c"}});
        let parsed = MachineRecipeData::parse(&json, &ITEMS).unwrap();
        assert_eq!(parsed.resource, None);
        assert_eq!(parsed.output, json!({"count": 2, "id": "a:c"}));
    }

    #[test]
    fn a_refused_recipe_says_what_is_wrong() {
        for (field, why) in [
            ("chemical_input", "falta `chemical_input`"),
            ("item_input", "falta `item_input`"),
            ("output", "falta `output`"),
        ] {
            let mut json = recipe();
            json.as_object_mut().unwrap().remove(field);
            assert_eq!(MachineRecipeData::why_not(&json, &FIELDS), why);
        }
        let mut json = recipe();
        json["item_input"]["count"] = json!(0);
        assert_eq!(
            MachineRecipeData::why_not(&json, &FIELDS),
            "`item_input.count` tem de ser um inteiro positivo"
        );
        let mut json = recipe();
        json["chemical_input"]["amount"] = json!(0);
        assert!(MachineRecipeData::why_not(&json, &FIELDS).starts_with("`chemical_input` precisa"));
        let bad = json!({"input": {"ingredient": "a:b"}, "output": {"id": "a:c", "amount": 0}});
        assert_eq!(
            Conversion::why_not(&bad, &CONVERSION),
            "`output.amount` tem de ser um inteiro positivo"
        );
        let bad = json!({"input": {"ingredient": "a:b"}, "output": {"amount": 1}});
        assert_eq!(
            Conversion::why_not(&bad, &CONVERSION),
            "`output.id` tem de ser um texto"
        );
        let bad = json!({"input": {"ingredient": "a:b"}, "output": -3});
        assert!(EnergyConversionData::why_not(&bad, &CONVERSION).contains("inteiro positivo"));
    }

    #[test]
    fn a_missing_part_is_no_recipe() {
        for field in ["chemical_input", "item_input", "output"] {
            let mut json = recipe();
            json.as_object_mut().unwrap().remove(field);
            assert_eq!(MachineRecipeData::parse(&json, &FIELDS), None);
        }
    }

    #[test]
    fn nothing_is_not_an_amount() {
        let mut json = recipe();
        json["chemical_input"]["amount"] = json!(0);
        assert_eq!(MachineRecipeData::parse(&json, &FIELDS), None);
        let mut json = recipe();
        json["item_input"]["count"] = json!(0);
        assert_eq!(MachineRecipeData::parse(&json, &FIELDS), None);
    }

    #[test]
    fn the_item_count_defaults_to_one() {
        let json = json!({"input": {"ingredient": "a:b"}, "output": {"id": "a:c", "amount": 5}});
        assert_eq!(Conversion::parse(&json, &CONVERSION).unwrap().item.count, 1);
    }

    #[test]
    fn a_conversion_names_what_it_gives() {
        let json = json!({
            "input": {"count": 1, "ingredient": "#c:dusts/redstone"},
            "output": {"amount": 90, "id": "a:redstone"},
        });
        let parsed = Conversion::parse(&json, &CONVERSION).unwrap();
        assert_eq!(parsed.resource, "a:redstone");
        assert_eq!(parsed.amount, 90);
    }

    #[test]
    fn an_or_ingredient_becomes_a_list() {
        let json = json!({
            "input": {"ingredient": {"type": "neoforge:or", "values": ["#c:dusts/coal", "minecraft:coal"]}},
            "output": {"amount": 10, "id": "a:carbon"},
        });
        let parsed = Conversion::parse(&json, &CONVERSION).unwrap();
        assert_eq!(
            parsed.item.ingredient,
            json!(["#c:dusts/coal", "minecraft:coal"])
        );
    }

    /// `chemical_conversion/fungi/from_mushrooms.json` do Mekanism: um `or` de `or`s, um deles com uma lista dentro. O jogo só
    /// lê as cadeias de uma lista, então o que ficasse aninhado sumia e a receita inteira com ele.
    #[test]
    fn nested_or_ingredients_become_one_flat_list() {
        let json = json!({
            "input": {"ingredient": {"type": "neoforge:or", "values": [
                {"type": "neoforge:or", "values": ["#c:mushrooms"]},
                {"type": "neoforge:or", "values": [["minecraft:crimson_fungus", "minecraft:warped_fungus"]]},
            ]}},
            "output": {"amount": 10, "id": "a:fungi"},
        });
        let parsed = Conversion::parse(&json, &CONVERSION).unwrap();
        assert_eq!(
            parsed.item.ingredient,
            json!([
                "#c:mushrooms",
                "minecraft:crimson_fungus",
                "minecraft:warped_fungus"
            ])
        );
    }

    #[test]
    fn a_resource_input_takes_an_id_or_a_tag() {
        let tags = Tags::load(&[("a:infuse".to_owned(), json!({"values": ["a:carbon"]}))]);
        let by_tag = ResourceInput {
            ingredient: "#a:infuse".into(),
            amount: 1,
        };
        assert!(by_tag.accepts("a:carbon", &tags));
        assert!(!by_tag.accepts("a:redstone", &tags));
        let by_id = ResourceInput {
            ingredient: "a:carbon".into(),
            amount: 1,
        };
        assert!(by_id.accepts("a:carbon", &tags));
        assert!(!by_id.accepts("a:redstone", &tags));
    }

    #[test]
    fn a_tag_input_lists_its_members_for_the_recipe_index() {
        use crate::chemical::ChemicalResource;
        use crate::recipe_cache::InputIngredient;
        let tags = Arc::new(Tags::load(&[
            (
                "a:infuse".to_owned(),
                json!({"values": ["a:carbon", "a:gold"]}),
            ),
            ("a:other".to_owned(), json!({"values": ["a:tin"]})),
        ]));
        let by_tag = ResourceInput {
            ingredient: "#a:infuse".into(),
            amount: 1,
        };
        let ingredient = by_tag.shaped(tags.clone());
        // Os membros que `accepts` aceita, e só eles.
        assert_eq!(
            ingredient.shape(),
            &IngredientShape::types(&["a:carbon", "a:gold"])
        );
        assert!(ingredient.test_type(&ChemicalResource::new("a:gold")));
        assert!(!ingredient.test_type(&ChemicalResource::new("a:tin")));
        // Uma tag que não existe não tem membro, e um id é só ele.
        let missing = ResourceInput {
            ingredient: "#a:missing".into(),
            amount: 1,
        };
        assert_eq!(
            missing.shaped(tags.clone()).shape(),
            &IngredientShape::types(&[])
        );
        let by_id = ResourceInput {
            ingredient: "a:carbon".into(),
            amount: 1,
        };
        assert_eq!(
            by_id.shaped(tags).shape(),
            &IngredientShape::types(&["a:carbon"])
        );
    }

    #[test]
    fn an_energy_conversion_has_an_integer_output() {
        let json =
            json!({"input": {"count": 1, "ingredient": "#c:dusts/redstone"}, "output": 10000});
        let parsed = EnergyConversionData::parse(&json, &CONVERSION).unwrap();
        assert_eq!(parsed.amount, 10_000);
        assert_eq!(parsed.item.ingredient, "#c:dusts/redstone");
    }

    #[test]
    fn an_energy_conversion_needs_a_positive_integer_output() {
        for bad in [
            json!(0),
            json!(-5),
            json!("10"),
            json!({"amount": 5}),
            json!(3.5),
            json!(3_000_000_000_i64),
        ] {
            let json = json!({"input": {"ingredient": "a:b"}, "output": bad});
            assert_eq!(
                EnergyConversionData::parse(&json, &CONVERSION),
                None,
                "{bad}"
            );
        }
        assert_eq!(
            EnergyConversionData::parse(&json!({"output": 5}), &CONVERSION),
            None
        );
    }
}
