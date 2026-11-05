//! Receitas de tipo próprio, lidas do JSON. A lib não conhece os campos de nenhum mod: quem usa diz
//! onde cada parte fica. Os ingredientes de item seguem como JSON, porque quem os resolve é o jogo.
use crate::resource::{TAG_PREFIX, Tags};
use serde_json::{Value, json};

const ITEM_COUNT: &str = "count";
const ITEM_INGREDIENT: &str = "ingredient";
const RESOURCE_AMOUNT: &str = "amount";
const RESOURCE_ID: &str = "id";
const OR_INGREDIENT: &str = "neoforge:or";

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
}

/// Onde ficam as partes de uma receita de item e recurso que dá um item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemResourceFields {
    pub item: &'static str,
    pub resource: &'static str,
    pub output: &'static str,
}

/// Uma receita de item e recurso que dá um item.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemResourceRecipe {
    pub item: ItemInput,
    pub resource: ResourceInput,
    pub output: Value,
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

/// O `neoforge:or` é só uma lista, a forma que o jogo aceita de um mod.
fn plain_ingredient(json: &Value) -> Value {
    if json.get("type").and_then(Value::as_str) == Some(OR_INGREDIENT) {
        return json.get("values").cloned().unwrap_or_else(|| json!([]));
    }
    json.clone()
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

impl ItemResourceRecipe {
    pub fn parse(json: &Value, fields: &ItemResourceFields) -> Option<Self> {
        let resource = json.get(fields.resource)?;
        Some(Self {
            item: item_input(json.get(fields.item)?)?,
            resource: ResourceInput {
                ingredient: resource.get(ITEM_INGREDIENT)?.as_str()?.to_owned(),
                amount: amount(resource)?,
            },
            output: json.get(fields.output)?.clone(),
        })
    }
}

impl Conversion {
    pub fn parse(json: &Value, fields: &ConversionFields) -> Option<Self> {
        let output = json.get(fields.output)?;
        Some(Self {
            item: item_input(json.get(fields.item)?)?,
            resource: output.get(RESOURCE_ID)?.as_str()?.to_owned(),
            amount: amount(output)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIELDS: ItemResourceFields = ItemResourceFields {
        item: "item_input",
        resource: "chemical_input",
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
        let parsed = ItemResourceRecipe::parse(&recipe(), &FIELDS).unwrap();
        assert_eq!(parsed.item.count, 4);
        assert_eq!(parsed.item.ingredient, "#c:ingots/iron");
        assert_eq!(parsed.resource.amount, 10);
        assert_eq!(parsed.output, json!({"id": "a:steel"}));
    }

    #[test]
    fn a_missing_part_is_no_recipe() {
        for field in ["chemical_input", "item_input", "output"] {
            let mut json = recipe();
            json.as_object_mut().unwrap().remove(field);
            assert_eq!(ItemResourceRecipe::parse(&json, &FIELDS), None);
        }
    }

    #[test]
    fn nothing_is_not_an_amount() {
        let mut json = recipe();
        json["chemical_input"]["amount"] = json!(0);
        assert_eq!(ItemResourceRecipe::parse(&json, &FIELDS), None);
        let mut json = recipe();
        json["item_input"]["count"] = json!(0);
        assert_eq!(ItemResourceRecipe::parse(&json, &FIELDS), None);
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
}
