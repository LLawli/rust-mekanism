//! Repete, tick a tick, os rastros do fluido do Mekanism original (`dev/referencia/rastros/fluids_flow_<cenário>.jsonl`, gravados por
//! `FluidsTraceTest` em `dev/referencia/testes/`) no mundo de teste do port (`liquid::sim::SimLiquidWorld`).
//!
//! Cada cenário é montado como o Java monta: o chão de pedra do gabarito (`template`, sem avisos), os blocos de `build` com
//! `setBlock(..., 2)`, o fluido de `place` com `setBlockAndUpdate` e o do balde com as flags 11 (`BucketItem.emptyContents`). O rastro
//! é gravado no `ServerTickEvent.Post`: depois do tick do mundo, e depois das ações do tick (`at`). O gametest cerca o volume de
//! barreiras, embaixo, nos lados e no teto (`TestInstanceBlockEntity.encaseStructure`): sem elas, o fluido acharia buracos fora do
//! volume. A cada tick o teste compara todas as células do volume, a contagem de ticks de fluido agendados dentro dele e os itens
//! soltos (sem a semente de trigo, que o Java tira).
use machinery_basis_core::liquid::shape::PixelShape;
use machinery_basis_core::liquid::sim::{
    SimBlockType, SimCell, SimDrop, SimLiquid, SimLiquidWorld, SimShape,
};
use machinery_basis_core::liquid::{BlockPos, LiquidId, LiquidParams, UPDATE_ALL_IMMEDIATE};
use machinery_basis_core::neighbors::flags::{UPDATE_ALL, UPDATE_CLIENTS};
use machinery_basis_core::sides::Face;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

const BRINE: LiquidId = LiquidId(0);
const HYDROGEN: LiquidId = LiquidId(1);
const WATER: LiquidId = LiquidId(2);
const LAVA: LiquidId = LiquidId(3);

const STONE: usize = 0;
const SHORT_GRASS: usize = 1;
const TALL_GRASS_LOWER: usize = 2;
const TALL_GRASS_UPPER: usize = 3;
const TORCH: usize = 4;
const OAK_SIGN: usize = 5;
const IRON_DOOR_LOWER: usize = 6;
const IRON_DOOR_UPPER: usize = 7;
const IRON_TRAPDOOR: usize = 8;
const POPPY: usize = 9;
const OAK_LEAVES: usize = 10;
const GLASS: usize = 11;
const GRASS_BLOCK: usize = 12;
const BARRIER: usize = 13;

/// A semente que a grama solta ao acaso; o `FluidsTraceTest` não a grava.
const RANDOM_DROP: &str = "minecraft:wheat_seeds";

/// Menos linhas do que isto é um rastro cortado.
const MINIMUM_LINES: usize = 3;
/// Menos células de fluido do que isto, somadas as linhas, é um rastro em que o fluido não correu.
const MINIMUM_FLUID_CELLS: usize = 20;

/// Os fluidos do cenário. A água e a lava são as do vanilla no overworld (`WaterFluid`, `LavaFluid` sem `ultraWarm`), no que os
/// cenários `meets_water` e `lava_control` precisam: a lava nunca encontra a água, e nenhuma célula de lava sobe de nível (o atraso
/// sorteado de `LavaFluid.getSpreadDelay` não entra).
fn liquids() -> Vec<SimLiquid> {
    let liquid = |name: &str, params, washed_away| SimLiquid {
        name: name.to_string(),
        params,
        washed_away,
    };
    vec![
        liquid("mekanism:brine", LiquidParams::BASE_FLOWING_FLUID, false),
        liquid("mekanism:hydrogen", LiquidParams::BASE_FLOWING_FLUID, false),
        liquid(
            "minecraft:water",
            LiquidParams {
                can_convert_to_source: true,
                ..LiquidParams::BASE_FLOWING_FLUID
            },
            true,
        ),
        liquid(
            "minecraft:lava",
            LiquidParams {
                tick_delay: 30,
                slope_find_distance: 2,
                drop_off: 2,
                can_convert_to_source: false,
            },
            true,
        ),
    ]
}

/// `VegetationBlock.canSurvive` (`VegetationBlock.java:40-46`): o bloco de baixo está na tag `SUPPORTS_VEGETATION`. Do catálogo, só a
/// grama (`minecraft:grass_block`) está.
fn supports_vegetation(world: &SimLiquidWorld, pos: BlockPos) -> bool {
    world.cell(pos.relative(Face::Down)) == SimCell::block(GRASS_BLOCK)
}

/// `VegetationBlock.updateShape` (`VegetationBlock.java:24-37`): a planta cai a qualquer aviso de forma quando não sobrevive.
fn vegetation(world: &SimLiquidWorld, pos: BlockPos, _: Face, _: SimCell) -> SimCell {
    if supports_vegetation(world, pos) {
        world.cell(pos)
    } else {
        SimCell::Air
    }
}

/// `DoublePlantBlock.updateShape` da metade de baixo (`DoublePlantBlock.java:36-56`): cai quando o aviso vem de cima e não é a
/// metade de cima, ou quando não sobrevive (`VegetationBlock.updateShape`, pelo `super`).
fn tall_grass_lower(
    world: &SimLiquidWorld,
    pos: BlockPos,
    direction: Face,
    neighbour: SimCell,
) -> SimCell {
    if direction == Face::Up && neighbour != SimCell::block(TALL_GRASS_UPPER) {
        SimCell::Air
    } else {
        vegetation(world, pos, direction, neighbour)
    }
}

/// `DoublePlantBlock.updateShape` da metade de cima, com o `canSurvive` dela (`DoublePlantBlock.java:36-56,58-66`): fica enquanto a
/// metade de baixo está embaixo.
fn tall_grass_upper(world: &SimLiquidWorld, pos: BlockPos, _: Face, _: SimCell) -> SimCell {
    if world.cell(pos.relative(Face::Down)) == SimCell::block(TALL_GRASS_LOWER) {
        world.cell(pos)
    } else {
        SimCell::Air
    }
}

/// `BaseTorchBlock.updateShape` e `StandingSignBlock.updateShape`: só caem sem o bloco de baixo, que aqui é sempre a pedra.
fn needs_floor(world: &SimLiquidWorld, pos: BlockPos, direction: Face, _: SimCell) -> SimCell {
    if direction == Face::Down && world.cell(pos.relative(Face::Down)) != SimCell::block(STONE) {
        SimCell::Air
    } else {
        world.cell(pos)
    }
}

fn blocks() -> Vec<SimBlockType> {
    let mut stone = SimBlockType::full("minecraft:stone");
    stone.drops = vec![("minecraft:cobblestone".into(), 1)];
    let mut short_grass = SimBlockType::washable("minecraft:short_grass", &[]);
    short_grass.update_shape = Some(vegetation);
    let mut tall_lower = SimBlockType::washable("minecraft:tall_grass", &[]);
    tall_lower.update_shape = Some(tall_grass_lower);
    let mut tall_upper = SimBlockType::washable("minecraft:tall_grass", &[]);
    tall_upper.update_shape = Some(tall_grass_upper);
    let mut torch = SimBlockType::washable("minecraft:torch", &[("minecraft:torch", 1)]);
    torch.update_shape = Some(needs_floor);
    // A placa: sem colisão, `SimpleWaterloggedBlock` (só a água entra) e fora da tag `WASHED_AWAY_BY_FLUIDS`.
    let sign = SimBlockType {
        container: Some(WATER),
        washed_away: false,
        update_shape: Some(needs_floor),
        ..SimBlockType::washable("minecraft:oak_sign", &[("minecraft:oak_sign", 1)])
    };
    // A porta fechada para o norte: a folha de 3 pixels do lado sul (`DoorBlock`, `Block.boxZ(16, 13, 16)` girado).
    let door_shape = SimShape::Pixels(PixelShape::new(&[[0, 0, 13, 16, 16, 16]]));
    let door = |name: &str| SimBlockType {
        collision: door_shape.clone(),
        solid: true,
        sturdy_faces: 0,
        blocks_fluid_flow: true,
        ..SimBlockType::full(name)
    };
    let trapdoor = SimBlockType {
        collision: SimShape::Pixels(PixelShape::new(&[[0, 0, 0, 16, 3, 16]])),
        container: Some(WATER),
        sturdy_faces: 1 << Face::Down as u8,
        ..SimBlockType::full("minecraft:iron_trapdoor")
    };
    let mut poppy = SimBlockType::washable("minecraft:poppy", &[("minecraft:poppy", 1)]);
    poppy.update_shape = Some(vegetation);
    let leaves = SimBlockType {
        container: Some(WATER),
        ..SimBlockType::full("minecraft:oak_leaves")
    };
    let glass = SimBlockType {
        solid: true,
        ..SimBlockType::full("minecraft:glass")
    };
    vec![
        stone,
        short_grass,
        tall_lower,
        tall_upper,
        torch,
        sign,
        door("minecraft:iron_door"),
        door("minecraft:iron_door"),
        trapdoor,
        poppy,
        leaves,
        glass,
        SimBlockType::full("minecraft:grass_block"),
        SimBlockType::full("minecraft:barrier"),
    ]
}

/// O cenário em montagem: as chamadas de `FluidsTraceTest.Flow`.
struct Flow {
    world: SimLiquidWorld,
    size: [i32; 3],
}

impl Flow {
    fn new(size: [i32; 3]) -> Self {
        let mut world = SimLiquidWorld::new(liquids(), blocks());
        let [sx, sy, sz] = size;
        // `TestInstanceBlockEntity.encaseStructure` (`TestInstanceBlockEntity.java:367-399`): barreira nas quatro paredes e embaixo, um
        // bloco fora do volume, e no teto (o teste não pede `skyAccess`).
        for x in -1..=sx {
            for y in -1..=sy {
                for z in -1..=sz {
                    let edge = x == -1 || x == sx || z == -1 || z == sz || y == -1 || y == sy;
                    if edge {
                        world.put(BlockPos::new(x, y, z), SimCell::block(BARRIER));
                    }
                }
            }
        }
        // `template`: o chão de pedra da estrutura.
        for x in 0..sx {
            for z in 0..sz {
                world.put(BlockPos::new(x, 0, z), SimCell::block(STONE));
            }
        }
        Self { world, size }
    }

    /// `build`: `setBlock(..., 2)`.
    fn build(&mut self, x: i32, y: i32, z: i32, cell: SimCell) {
        self.world
            .set_cell(BlockPos::new(x, y, z), cell, UPDATE_CLIENTS);
    }

    fn fill(&mut self, from: [i32; 3], to: [i32; 3], cell: SimCell) {
        for x in from[0]..=to[0] {
            for y in from[1]..=to[1] {
                for z in from[2]..=to[2] {
                    self.build(x, y, z, cell);
                }
            }
        }
    }

    /// `place`: `setBlockAndUpdate`.
    fn place(&mut self, x: i32, y: i32, z: i32, cell: SimCell) {
        self.world
            .set_cell(BlockPos::new(x, y, z), cell, UPDATE_ALL);
    }

    /// `bucket`: o jogador mira o topo do bloco `floor`; o fluido não tem forma, então o raio passa por ele e o balde despeja em
    /// cima do chão (`BucketItem.emptyContents`, `BucketItem.java:157`), por cima do que estiver lá (ar ou outro fluido).
    fn bucket(&mut self, liquid: LiquidId, x: i32, y: i32, z: i32) {
        self.world.set_cell(
            BlockPos::new(x, y + 1, z),
            source(liquid),
            UPDATE_ALL_IMMEDIATE,
        );
    }
}

fn source(liquid: LiquidId) -> SimCell {
    SimCell::Liquid { liquid, level: 0 }
}

type Action = (u32, fn(&mut Flow));

struct Scenario {
    name: &'static str,
    size: [i32; 3],
    setup: fn(&mut Flow),
    actions: Vec<Action>,
}

fn scenario(name: &str) -> Scenario {
    match name {
        "flat" => Scenario {
            name: "flat",
            size: [31, 4, 31],
            setup: |f| f.place(15, 1, 15, source(BRINE)),
            actions: vec![],
        },
        "flat_hydrogen" => Scenario {
            name: "flat_hydrogen",
            size: [31, 4, 31],
            setup: |f| f.place(15, 1, 15, source(HYDROGEN)),
            actions: vec![],
        },
        "flat_bucket" => Scenario {
            name: "flat_bucket",
            size: [31, 4, 31],
            setup: |f| f.bucket(BRINE, 15, 0, 15),
            actions: vec![],
        },
        "waterfall" => Scenario {
            name: "waterfall",
            size: [25, 12, 25],
            setup: |f| {
                f.fill([10, 1, 10], [14, 6, 14], SimCell::block(STONE));
                f.place(10, 7, 10, source(BRINE));
            },
            actions: vec![],
        },
        "hole" => Scenario {
            name: "hole",
            size: [31, 6, 31],
            setup: |f| {
                let stone = SimCell::block(STONE);
                f.fill([0, 1, 0], [30, 1, 30], stone);
                f.build(11, 1, 15, SimCell::Air);
                f.build(14, 1, 15, SimCell::Air);
                f.fill([9, 2, 13], [13, 3, 13], stone);
                f.fill([13, 2, 13], [13, 3, 17], stone);
                f.place(8, 2, 15, source(BRINE));
            },
            actions: vec![],
        },
        "slope" => Scenario {
            name: "slope",
            size: [30, 8, 11],
            setup: |f| {
                let stone = SimCell::block(STONE);
                f.fill([0, 1, 0], [9, 3, 10], stone);
                f.fill([10, 1, 0], [19, 2, 10], stone);
                f.fill([20, 1, 0], [29, 1, 10], stone);
                f.place(2, 4, 5, source(BRINE));
            },
            actions: vec![],
        },
        "obstacles" => Scenario {
            name: "obstacles",
            size: [21, 6, 21],
            setup: |f| {
                let stone = SimCell::block(STONE);
                f.fill([12, 1, 10], [12, 3, 10], stone);
                f.fill([10, 1, 13], [10, 3, 13], stone);
                f.fill([15, 1, 4], [15, 3, 9], stone);
                f.fill([15, 1, 11], [15, 3, 16], stone);
                f.build(12, 0, 8, SimCell::block(GRASS_BLOCK));
                f.build(11, 0, 7, SimCell::block(GRASS_BLOCK));
                f.build(9, 0, 7, SimCell::block(GRASS_BLOCK));
                f.build(12, 1, 8, SimCell::block(SHORT_GRASS));
                // `DoublePlantBlock.placeAt(..., 2)`: as duas metades com `setBlock(..., 2)`.
                f.build(11, 1, 7, SimCell::block(TALL_GRASS_LOWER));
                f.build(11, 2, 7, SimCell::block(TALL_GRASS_UPPER));
                f.build(9, 1, 12, SimCell::block(TORCH));
                f.build(12, 1, 12, SimCell::block(OAK_SIGN));
                f.build(8, 1, 8, SimCell::block(IRON_DOOR_LOWER));
                f.build(8, 2, 8, SimCell::block(IRON_DOOR_UPPER));
                f.build(7, 1, 10, SimCell::block(IRON_TRAPDOOR));
                f.build(9, 1, 7, SimCell::block(POPPY));
                f.build(13, 1, 12, SimCell::block(OAK_LEAVES));
                f.build(13, 1, 8, SimCell::block(GLASS));
                f.place(10, 1, 10, source(BRINE));
            },
            actions: vec![],
        },
        "remove_source" => Scenario {
            name: "remove_source",
            size: [31, 4, 31],
            setup: |f| f.place(15, 1, 15, source(BRINE)),
            actions: vec![(60, |f| f.place(15, 1, 15, SimCell::Air))],
        },
        "two_sources" => Scenario {
            name: "two_sources",
            size: [31, 4, 31],
            setup: |f| {
                f.place(5, 1, 7, source(BRINE));
                f.place(7, 1, 7, source(BRINE));
                f.place(20, 1, 24, source(BRINE));
                f.place(23, 1, 24, source(BRINE));
            },
            actions: vec![],
        },
        "ceiling" => Scenario {
            name: "ceiling",
            size: [31, 6, 31],
            setup: |f| {
                let stone = SimCell::block(STONE);
                f.fill([11, 3, 11], [19, 3, 19], stone);
                f.fill([20, 2, 14], [22, 2, 16], stone);
                f.place(15, 1, 15, source(BRINE));
            },
            actions: vec![],
        },
        "column" => Scenario {
            name: "column",
            size: [31, 14, 31],
            setup: |f| {
                f.place(6, 11, 15, source(BRINE));
                f.place(24, 1, 15, source(BRINE));
            },
            actions: vec![
                (30, |f| f.place(24, 2, 15, SimCell::block(STONE))),
                (60, |f| f.place(24, 2, 15, SimCell::Air)),
            ],
        },
        "lava_control" => Scenario {
            name: "lava_control",
            size: [31, 4, 31],
            setup: |f| f.place(15, 1, 15, source(LAVA)),
            actions: vec![],
        },
        "meets_water" => Scenario {
            name: "meets_water",
            size: [48, 6, 48],
            setup: |f| {
                f.place(10, 1, 10, source(BRINE));
                f.place(11, 1, 10, source(WATER));
                f.place(34, 1, 10, source(BRINE));
                f.place(10, 1, 34, source(BRINE));
                f.place(11, 1, 34, source(LAVA));
                f.place(34, 1, 34, source(BRINE));
            },
            actions: vec![
                (20, |f| f.bucket(WATER, 34, 0, 10)),
                (25, |f| f.bucket(WATER, 37, 0, 34)),
            ],
        },
        other => panic!("cenário desconhecido: {other}"),
    }
}

type Cell = (String, Option<i64>);

fn cell_of(world: &SimLiquidWorld, cell: SimCell) -> Cell {
    let level = match cell {
        SimCell::Liquid { level, .. } => Some(i64::from(level)),
        _ => None,
    };
    (world.name(cell).to_string(), level)
}

fn drop_key(drop: &SimDrop) -> String {
    format!(
        "[{},{},{},\"{}\",{}]",
        drop.pos.x, drop.pos.y, drop.pos.z, drop.item, drop.count
    )
}

/// O que a comparação de um cenário mediu.
struct Checked {
    ticks: usize,
    cells: usize,
}

fn read_trace(name: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../../dev/referencia/rastros/fluids_flow_{name}.jsonl"
    ));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let lines: Vec<Value> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(lines.len() >= MINIMUM_LINES, "{name}: rastro cortado");
    let fluid_cells = lines
        .iter()
        .flat_map(|line| line["cells"].as_array().unwrap())
        .filter(|cell| !cell[4].is_null())
        .count();
    assert!(
        fluid_cells >= MINIMUM_FLUID_CELLS,
        "{name}: só {fluid_cells} células de fluido no rastro"
    );
    lines
}

fn check(name: &str) -> Checked {
    let lines = read_trace(name);
    let scenario = scenario(name);
    assert_eq!(scenario.name, name);
    let size: Vec<i32> = lines[0]["size"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap() as i32)
        .collect();
    assert_eq!(size, scenario.size.to_vec(), "{name}: tamanho");
    let mut flow = Flow::new(scenario.size);
    (scenario.setup)(&mut flow);

    let by_tick: BTreeMap<u32, &Value> = lines
        .iter()
        .map(|line| (line["tick"].as_u64().unwrap() as u32, line))
        .collect();
    let last = *by_tick.keys().last().unwrap();
    assert!(
        by_tick[&last]["last"].as_bool().unwrap_or(false),
        "{name}: sem a linha final"
    );

    let [sx, sy, sz] = flow.size;
    let min = BlockPos::new(0, 0, 0);
    let max = BlockPos::new(sx - 1, sy - 1, sz - 1);
    let mut expected: BTreeMap<(i32, i32, i32), Cell> = BTreeMap::new();
    let mut expected_pending = 0usize;
    let mut differences: Vec<String> = Vec::new();
    let mut checked = Checked { ticks: 0, cells: 0 };

    for tick in 0..=last {
        if tick > 0 {
            flow.world.tick();
            for (at, action) in &scenario.actions {
                if *at == tick {
                    action(&mut flow);
                }
            }
        }
        let mut expected_drops: Vec<String> = Vec::new();
        if let Some(line) = by_tick.get(&tick) {
            for cell in line["cells"].as_array().unwrap() {
                let c = cell.as_array().unwrap();
                let pos = (
                    c[0].as_i64().unwrap() as i32,
                    c[1].as_i64().unwrap() as i32,
                    c[2].as_i64().unwrap() as i32,
                );
                let id = c[3].as_str().unwrap().to_string();
                if id == "minecraft:air" {
                    expected.remove(&pos);
                } else {
                    expected.insert(pos, (id, c[4].as_i64()));
                }
            }
            expected_pending = line["fluid_ticks"].as_u64().unwrap() as usize;
            if let Some(drops) = line["drops"].as_array() {
                expected_drops = drops.iter().map(|drop| drop.to_string()).collect();
            }
        }
        for x in 0..sx {
            for y in 0..sy {
                for z in 0..sz {
                    let ours = cell_of(&flow.world, flow.world.cell(BlockPos::new(x, y, z)));
                    let theirs = expected
                        .get(&(x, y, z))
                        .cloned()
                        .unwrap_or(("minecraft:air".to_string(), None));
                    if ours != theirs && differences.len() < 20 {
                        differences.push(format!(
                            "tick {tick} ({x}, {y}, {z}): Java {theirs:?}, port {ours:?}"
                        ));
                    }
                    checked.cells += 1;
                }
            }
        }
        let pending = flow.world.pending_fluid_ticks_in(min, max);
        if pending != expected_pending && differences.len() < 20 {
            differences.push(format!(
                "tick {tick}: ticks de fluido Java {expected_pending}, port {pending}"
            ));
        }
        let mut drops: Vec<String> = flow
            .world
            .drops
            .drain(..)
            .filter(|drop| drop.item != RANDOM_DROP)
            .map(|drop| drop_key(&drop))
            .collect();
        drops.sort();
        expected_drops.sort();
        if drops != expected_drops && differences.len() < 20 {
            differences.push(format!(
                "tick {tick}: itens Java {expected_drops:?}, port {drops:?}"
            ));
        }
        checked.ticks += 1;
    }
    assert!(
        differences.is_empty(),
        "{name}: {} diferenças (as primeiras):\n{}",
        differences.len(),
        differences.join("\n")
    );
    checked
}

macro_rules! reference {
    ($($test:ident => $name:literal),* $(,)?) => {
        $(
            #[test]
            fn $test() {
                let checked = check($name);
                println!("{}: {} ticks, {} células", $name, checked.ticks, checked.cells);
            }
        )*
    };
}

reference! {
    liquid_flow_flat => "flat",
    liquid_flow_flat_hydrogen => "flat_hydrogen",
    liquid_flow_flat_bucket => "flat_bucket",
    liquid_flow_waterfall => "waterfall",
    liquid_flow_hole => "hole",
    liquid_flow_slope => "slope",
    liquid_flow_obstacles => "obstacles",
    liquid_flow_remove_source => "remove_source",
    liquid_flow_two_sources => "two_sources",
    liquid_flow_ceiling => "ceiling",
    liquid_flow_column => "column",
    liquid_flow_lava_control => "lava_control",
    liquid_flow_meets_water => "meets_water",
}
