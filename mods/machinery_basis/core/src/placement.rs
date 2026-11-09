//! O estado em que um bloco é posto: virado para o jogador ou para a face clicada, e com o fluido que estava no lugar.
//!
//! Um bloco declara em `data/<ns>/machinery_basis/placement/<bloco>.json` o que o estado dele escolhe ao ser posto, e o Mekanism escreve isso a partir de
//! `AttributeStateFacing.getStateForPlacement` (`AttributeStateFacing.java`) e de `BlockStateHelper.getStateForPlacement` (`BlockStateHelper.java`):
//!
//! ```json
//! { "facing": { "property": "facing", "mode": "player_location", "values": ["north", "south", "west", "east"] }, "fluid_logged": "fluid_logged" }
//! ```
//!
//! `values` são os valores da propriedade, porque o original só vira para cima ou para baixo quando a propriedade os tem (`supportsDirection`).
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

pub const FOLDER: &str = "machinery_basis/placement";

const NORTH: &str = "north";
const EAST: &str = "east";
const SOUTH: &str = "south";
const WEST: &str = "west";
const UP: &str = "up";
const DOWN: &str = "down";
/// `Math.round(pitch) >= 65` vira o bloco para cima e `<= -65` para baixo (`AttributeStateFacing.java`).
const VERTICAL_PITCH: i32 = 65;

/// `FacePlacementType`: de onde o bloco tira a direção.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// A posição do jogador: a guinada, e a inclinação para cima e para baixo quando a propriedade as tem.
    PlayerLocation,
    /// A face em que se clicou.
    SelectedFace,
}

/// A propriedade de direção de um bloco (`AttributeStateFacing`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FacingRule {
    pub property: String,
    pub mode: Mode,
    pub values: Vec<String>,
}

/// O nome fixo de uma direção.
fn canonical(direction: &str) -> Option<&'static str> {
    [NORTH, EAST, SOUTH, WEST, UP, DOWN]
        .into_iter()
        .find(|name| *name == direction)
}

/// `Math.round(float)`: o inteiro mais próximo, o meio para cima.
fn java_round(value: f32) -> i32 {
    (f64::from(value) + 0.5).floor() as i32
}

impl FacingRule {
    fn supports(&self, direction: &str) -> bool {
        self.values.iter().any(|value| value == direction)
    }

    /// O valor da propriedade em que o bloco é posto, ou `None` se o estado fica como está (`setDirection` recusa a direção que a propriedade não tem).
    /// `yaw` e `pitch` são os do jogador, em graus, e `face` a face clicada.
    pub fn direction(&self, yaw: f32, pitch: f32, face: &str) -> Option<&'static str> {
        let chosen = match self.mode {
            Mode::PlayerLocation => {
                let mut chosen = SOUTH;
                if self.supports(DOWN) && self.supports(UP) {
                    let height = java_round(pitch);
                    if height >= VERTICAL_PITCH {
                        chosen = UP;
                    } else if height <= -VERTICAL_PITCH {
                        chosen = DOWN;
                    }
                }
                if chosen != DOWN && chosen != UP {
                    // `Mth.floor((placementYaw * 4.0F / 360.0F) + 0.5D) & 3`.
                    let side = ((f64::from(yaw * 4.0 / 360.0)) + 0.5).floor() as i32 & 3;
                    chosen = match side {
                        0 => NORTH,
                        1 => EAST,
                        2 => SOUTH,
                        _ => WEST,
                    };
                }
                chosen
            }
            Mode::SelectedFace => {
                match canonical(face) {
                    Some(face) if self.supports(face) => face,
                    _ => NORTH,
                }
            }
        };
        self.supports(chosen).then_some(chosen)
    }
}

/// O que um bloco escolhe ao ser posto.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rule {
    pub facing: Option<FacingRule>,
    /// A propriedade que guarda o fluido de dentro do bloco (`IStateFluidLoggable`), com `empty`, `water` e `lava`.
    pub fluid_logged: Option<String>,
}

impl Rule {
    pub fn parse(json: &Value) -> Self {
        let facing = json.get("facing").and_then(|facing| {
            let mode = match facing.get("mode")?.as_str()? {
                "player_location" => Mode::PlayerLocation,
                "selected_face" => Mode::SelectedFace,
                _ => return None,
            };
            Some(FacingRule {
                property: facing.get("property")?.as_str()?.to_owned(),
                mode,
                values: facing
                    .get("values")?
                    .as_array()?
                    .iter()
                    .filter_map(|value| value.as_str().map(str::to_owned))
                    .collect(),
            })
        });
        let fluid_logged = json
            .get("fluid_logged")
            .and_then(Value::as_str)
            .map(str::to_owned);
        Self {
            facing,
            fluid_logged,
        }
    }
}

/// O valor de `fluid_logged` para o fluido que estava no lugar: `IStateFluidLoggable.setState` só acha o valor cujo fluido é o mesmo, então a água corrente e a lava
/// corrente (outros fluidos) deixam o estado vazio.
pub fn fluid_logged_value(fluid: &str) -> &'static str {
    match fluid {
        "minecraft:water" | "water" => "water",
        "minecraft:lava" | "lava" => "lava",
        _ => "empty",
    }
}

/// As regras de uma carga de dados, por id de bloco.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rules {
    blocks: BTreeMap<String, Rule>,
}

impl Rules {
    pub fn load(files: &[(String, Value)]) -> Self {
        Self {
            blocks: files
                .iter()
                .map(|(id, json)| (id.clone(), Rule::parse(json)))
                .collect(),
        }
    }

    pub fn get(&self, block: &str) -> Option<&Rule> {
        self.blocks.get(block)
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
}

static CURRENT: RwLock<Option<Arc<Rules>>> = RwLock::new(None);

/// Troca as regras em uso: a cada carga de dados, inclusive no `/reload`.
pub fn set_current(rules: Rules) {
    let mut current = CURRENT.write().unwrap_or_else(|e| e.into_inner());
    *current = Some(Arc::new(rules));
}

/// As regras em uso: nenhuma antes da primeira carga.
pub fn current() -> Arc<Rules> {
    let current = CURRENT.read().unwrap_or_else(|e| e.into_inner());
    current.clone().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn horizontal(mode: &str) -> FacingRule {
        match Rule::parse(&json!({"facing": {"property": "facing", "mode": mode, "values": ["north", "south", "west", "east"]}})).facing {
            Some(rule) => rule,
            None => panic!("regra sem direção"),
        }
    }

    fn all(mode: &str) -> FacingRule {
        match Rule::parse(&json!({"facing": {"property": "facing", "mode": mode, "values": ["north", "east", "south", "west", "up", "down"]}})).facing {
            Some(rule) => rule,
            None => panic!("regra sem direção"),
        }
    }

    #[test]
    fn the_player_location_picks_the_side_from_the_yaw() {
        let rule = horizontal("player_location");
        // `side = floor(yaw * 4 / 360 + 0.5) & 3`: guinada 0 (olhando para o sul) vira o bloco para o norte, 90 para o leste, 180 para o sul, 270 para o oeste.
        assert_eq!(rule.direction(0.0, 0.0, "up"), Some("north"));
        assert_eq!(rule.direction(90.0, 0.0, "up"), Some("east"));
        assert_eq!(rule.direction(180.0, 0.0, "up"), Some("south"));
        assert_eq!(rule.direction(270.0, 0.0, "up"), Some("west"));
        assert_eq!(rule.direction(-90.0, 0.0, "up"), Some("west"));
        assert_eq!(rule.direction(44.9, 0.0, "up"), Some("north"));
        assert_eq!(rule.direction(45.0, 0.0, "up"), Some("east"));
    }

    #[test]
    fn a_horizontal_block_ignores_the_pitch() {
        let rule = horizontal("player_location");
        assert_eq!(rule.direction(0.0, 90.0, "up"), Some("north"));
        assert_eq!(rule.direction(0.0, -90.0, "up"), Some("north"));
    }

    #[test]
    fn a_block_with_up_and_down_looks_up_or_down_from_65_degrees() {
        let rule = all("player_location");
        assert_eq!(rule.direction(0.0, 64.0, "up"), Some("north"));
        assert_eq!(rule.direction(0.0, 64.5, "up"), Some("up"));
        assert_eq!(rule.direction(0.0, 65.0, "up"), Some("up"));
        assert_eq!(rule.direction(0.0, -64.0, "up"), Some("north"));
        assert_eq!(rule.direction(0.0, -64.5, "up"), Some("north"));
        assert_eq!(rule.direction(0.0, -65.0, "up"), Some("down"));
    }

    #[test]
    fn the_selected_face_is_the_clicked_one_when_the_property_has_it() {
        let rule = all("selected_face");
        assert_eq!(rule.direction(0.0, 0.0, "down"), Some("down"));
        assert_eq!(rule.direction(123.0, 80.0, "east"), Some("east"));
        let horizontal = horizontal("selected_face");
        assert_eq!(horizontal.direction(0.0, 0.0, "up"), Some("north"));
        assert_eq!(horizontal.direction(0.0, 0.0, "west"), Some("west"));
    }

    #[test]
    fn only_a_source_fluid_fills_the_block() {
        assert_eq!(fluid_logged_value("minecraft:water"), "water");
        assert_eq!(fluid_logged_value("minecraft:lava"), "lava");
        assert_eq!(fluid_logged_value("minecraft:flowing_water"), "empty");
        assert_eq!(fluid_logged_value("minecraft:flowing_lava"), "empty");
        assert_eq!(fluid_logged_value("minecraft:empty"), "empty");
    }

    #[test]
    fn the_rules_are_read_from_the_data_files() {
        let rules = Rules::load(&[(
            "mekanism:laser".to_owned(),
            json!({"facing": {"property": "facing", "mode": "selected_face", "values": ["north", "up"]}, "fluid_logged": "fluid_logged"}),
        )]);
        let rule = match rules.get("mekanism:laser") {
            Some(rule) => rule,
            None => panic!("sem regra"),
        };
        assert_eq!(rule.fluid_logged.as_deref(), Some("fluid_logged"));
        assert!(rules.get("mekanism:other").is_none());
    }
}
