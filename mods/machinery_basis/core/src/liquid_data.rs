//! Os fluidos de mod que viram bloco de líquido, declarados como dados: `data/<ns>/machinery_basis/liquid/<nome>.json`. A lib não conhece
//! nenhum fluido: cada arquivo diz qual é o bloco, qual é o balde, os números do `BaseFlowingFluid.Properties` e do `FluidType` que o jogo
//! precisa, e o jogo faz o resto (a física é de `liquid`). O motor do Horizon só conhece água e lava (`FluidKind`), então um fluido de mod
//! aqui é um bloco com a propriedade `level` do `LiquidBlock`, e este arquivo é o que o `FluidDeferredRegister` do original guarda no
//! fluido, no tipo e no balde.
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex, RwLock};

/// A pasta dos fluidos, dentro de `data/<ns>/`.
pub const FOLDER: &str = "machinery_basis/liquid";

/// A propriedade do estado do bloco que guarda o nível (`LiquidBlock.LEVEL`, 0 a 15).
pub const LEVEL_PROPERTY: &str = "level";
/// A propriedade que diz que há o mesmo fluido na célula de cima (a altura do bloco passa a ser a cheia, `FlowingFluid.getHeight`).
pub const UP_PROPERTY: &str = "up";

/// Os números de `BaseFlowingFluid.Properties` quando o mod não os muda.
pub const DEFAULT_TICK_DELAY: i32 = 5;
pub const DEFAULT_SLOPE_FIND_DISTANCE: i32 = 4;
pub const DEFAULT_DROP_OFF: i32 = 1;

/// Um fluido declarado.
#[derive(Clone, Debug, PartialEq)]
pub struct LiquidSpec {
    /// O id do arquivo, `<ns>:<nome>`: o do fluido.
    pub id: String,
    /// O bloco de líquido (`LiquidBlock`), com as propriedades `level` e `up`.
    pub block: String,
    /// O balde cheio (`BucketItem`).
    pub bucket: String,
    /// `BaseFlowingFluid.getTickDelay`.
    pub tick_delay: i32,
    /// `BaseFlowingFluid.getSlopeFindDistance`.
    pub slope_find_distance: i32,
    /// `BaseFlowingFluid.getDropOff` (`levelDecreasePerBlock`).
    pub drop_off: i32,
    /// `FluidType.canConvertToSource`.
    pub can_convert_to_source: bool,
    /// A luz do `FluidType`, de 0 a 15.
    pub light: u8,
    /// A cor do fluido (`FluidTypeRenderProperties.tint`), sem o alfa: 0xRRGGBB.
    pub color: u32,
    /// `SoundActions.BUCKET_FILL` e `BUCKET_EMPTY`.
    pub fill_sound: String,
    pub empty_sound: String,
    /// O balde também se bebe: com fome (`Player.canEat(false)`) o uso é beber o item, e só sem fome ele esvazia o balde
    /// (`ItemNutritionalPasteBucket.use`). Falso nos outros.
    pub drink_when_hungry: bool,
}

/// Por que um arquivo foi recusado.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiquidError {
    BadId,
    MissingField(&'static str),
    BadNumber(&'static str),
}

impl fmt::Display for LiquidError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadId => f.write_str("o id precisa ser <ns>:<nome>"),
            Self::MissingField(field) => write!(f, "falta `{field}`"),
            Self::BadNumber(field) => write!(f, "`{field}` não é um número válido"),
        }
    }
}

fn valid_id(id: &str) -> bool {
    id.split_once(':')
        .is_some_and(|(namespace, path)| !namespace.is_empty() && !path.is_empty())
}

fn text(json: &Value, field: &'static str) -> Result<String, LiquidError> {
    let value = json.get(field).ok_or(LiquidError::MissingField(field))?;
    value
        .as_str()
        .filter(|id| valid_id(id))
        .map(str::to_owned)
        .ok_or(LiquidError::MissingField(field))
}

fn number(
    json: &Value,
    field: &'static str,
    default: i32,
    min: i32,
    max: i32,
) -> Result<i32, LiquidError> {
    match json.get(field) {
        None => Ok(default),
        Some(value) => value
            .as_i64()
            .and_then(|n| i32::try_from(n).ok())
            .filter(|n| (min..=max).contains(n))
            .ok_or(LiquidError::BadNumber(field)),
    }
}

impl LiquidSpec {
    pub fn parse(id: &str, json: &Value) -> Result<Self, LiquidError> {
        if !valid_id(id) {
            return Err(LiquidError::BadId);
        }
        Ok(Self {
            id: id.to_owned(),
            block: text(json, "block")?,
            bucket: text(json, "bucket")?,
            tick_delay: number(json, "tick_delay", DEFAULT_TICK_DELAY, 1, i32::MAX)?,
            slope_find_distance: number(
                json,
                "slope_find_distance",
                DEFAULT_SLOPE_FIND_DISTANCE,
                0,
                127,
            )?,
            drop_off: number(json, "drop_off", DEFAULT_DROP_OFF, 1, 8)?,
            can_convert_to_source: json.get("can_convert_to_source").map_or(Ok(false), |v| {
                v.as_bool()
                    .ok_or(LiquidError::BadNumber("can_convert_to_source"))
            })?,
            light: u8::try_from(number(json, "light", 0, 0, 15)?).unwrap_or(0),
            color: u32::try_from(number(json, "color", 0xFF_FF_FF, 0, 0xFF_FF_FF)?)
                .unwrap_or(0xFF_FF_FF),
            fill_sound: text(json, "fill_sound")?,
            empty_sound: text(json, "empty_sound")?,
            drink_when_hungry: json.get("drink_when_hungry").map_or(Ok(false), |v| {
                v.as_bool()
                    .ok_or(LiquidError::BadNumber("drink_when_hungry"))
            })?,
        })
    }
}

/// Os fluidos de uma carga de dados.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Liquids {
    specs: Vec<LiquidSpec>,
}

impl Liquids {
    /// Um arquivo ruim fica de fora e volta na lista de erros, sem derrubar os outros. A ordem é a dos arquivos, que é a do id.
    pub fn load(files: &[(String, Value)]) -> (Self, Vec<(String, LiquidError)>) {
        let mut liquids = Self::default();
        let mut errors = Vec::new();
        for (id, json) in files {
            match LiquidSpec::parse(id, json) {
                Ok(spec) => liquids.specs.push(spec),
                Err(error) => errors.push((id.clone(), error)),
            }
        }
        (liquids, errors)
    }

    pub fn len(&self) -> usize {
        self.specs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &LiquidSpec> {
        self.specs.iter()
    }

    /// O fluido cujo bloco de líquido é `block`.
    pub fn by_block(&self, block: &str) -> Option<&LiquidSpec> {
        self.specs.iter().find(|spec| spec.block == block)
    }

    /// O fluido cujo balde cheio é `bucket`.
    pub fn by_bucket(&self, bucket: &str) -> Option<&LiquidSpec> {
        self.specs.iter().find(|spec| spec.bucket == bucket)
    }

    /// O índice do fluido de `block` entre os declarados, o `LiquidId` do núcleo.
    pub fn index_of_block(&self, block: &str) -> Option<usize> {
        self.specs.iter().position(|spec| spec.block == block)
    }
}

static CURRENT: RwLock<Option<Arc<Liquids>>> = RwLock::new(None);
static SOURCES: Mutex<BTreeMap<String, Liquids>> = Mutex::new(BTreeMap::new());

/// Troca os fluidos de uma fonte (um mod): a cada carga de dados, inclusive no `/reload`.
pub fn set_source(source: &str, liquids: Liquids) {
    let mut sources = SOURCES.lock().unwrap_or_else(|e| e.into_inner());
    sources.insert(source.to_owned(), liquids);
    let mut merged = Liquids::default();
    for liquids in sources.values() {
        merged.specs.extend(liquids.specs.iter().cloned());
    }
    *CURRENT.write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(merged));
}

/// Os fluidos em uso: nenhum antes da primeira carga.
pub fn current() -> Arc<Liquids> {
    CURRENT
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn file() -> Value {
        json!({
            "block": "mod:brine",
            "bucket": "mod:brine_bucket",
            "color": 0xFEEF9C,
            "light": 3,
            "fill_sound": "minecraft:item.bucket.fill",
            "empty_sound": "minecraft:item.bucket.empty"
        })
    }

    #[test]
    fn a_file_without_numbers_gets_the_base_flowing_fluid_defaults() {
        let spec = LiquidSpec::parse("mod:brine", &file()).unwrap();
        assert_eq!(
            (
                spec.tick_delay,
                spec.slope_find_distance,
                spec.drop_off,
                spec.can_convert_to_source
            ),
            (5, 4, 1, false)
        );
        assert_eq!((spec.light, spec.color), (3, 0xFEEF9C));
    }

    #[test]
    fn a_bad_file_is_refused_and_the_others_are_kept() {
        let files = vec![
            ("mod:brine".to_owned(), file()),
            ("mod:broken".to_owned(), json!({"block": "mod:broken"})),
            ("nonamespace".to_owned(), file()),
        ];
        let (liquids, errors) = Liquids::load(&files);
        assert_eq!(liquids.len(), 1);
        assert_eq!(
            errors,
            vec![
                ("mod:broken".to_owned(), LiquidError::MissingField("bucket")),
                ("nonamespace".to_owned(), LiquidError::BadId),
            ]
        );
    }

    #[test]
    fn the_block_and_the_bucket_find_the_fluid() {
        let (liquids, _) = Liquids::load(&[("mod:brine".to_owned(), file())]);
        assert_eq!(
            liquids.by_block("mod:brine").map(|s| s.id.as_str()),
            Some("mod:brine")
        );
        assert_eq!(
            liquids.by_bucket("mod:brine_bucket").map(|s| s.id.as_str()),
            Some("mod:brine")
        );
        assert_eq!(liquids.by_bucket("mod:brine"), None);
        assert_eq!(liquids.index_of_block("mod:brine"), Some(0));
    }

    #[test]
    fn out_of_range_numbers_are_refused() {
        let mut broken = file();
        broken["drop_off"] = json!(0);
        assert_eq!(
            LiquidSpec::parse("mod:brine", &broken),
            Err(LiquidError::BadNumber("drop_off"))
        );
        let mut broken = file();
        broken["light"] = json!(16);
        assert_eq!(
            LiquidSpec::parse("mod:brine", &broken),
            Err(LiquidError::BadNumber("light"))
        );
    }
}
