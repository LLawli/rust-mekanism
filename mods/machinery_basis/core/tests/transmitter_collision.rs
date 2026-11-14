//! O módulo `transmitter-collision` da `machinery_basis` (`mods/machinery_basis/transmitter_collision.rs`), que o launcher enxerta em `collisions.rs` do `horizon-client-render`, compilado
//! aqui como é, com um `horizon_core::geometry` de mentira (uma forma é a lista das caixas): a tabela é por dimensão e posição, a dimensão que a varredura lê é a do nível no lugar da
//! thread, e sem nenhuma forma guardada a varredura não toca a trava. A forma de cada combinação de tipos é conferida contra o original em `mekanism/tests/transmitter_shape_all.rs`.
#![allow(dead_code)]

mod horizon_core {
    pub mod geometry {
        #[derive(Clone, Copy)]
        pub struct Vec3(pub f64, pub f64, pub f64);
        impl Vec3 {
            pub fn new(x: f64, y: f64, z: f64) -> Self {
                Self(x, y, z)
            }
        }
        #[derive(Clone, Copy)]
        pub struct Aabb(pub Vec3, pub Vec3);
        impl Aabb {
            pub fn new(min: Vec3, max: Vec3) -> Self {
                Self(min, max)
            }
        }
        #[derive(Clone, Copy)]
        pub enum ShapeOp {
            OR,
        }
        #[derive(Clone, Debug, PartialEq)]
        pub struct VoxelShape(pub Vec<[f64; 6]>);
        impl VoxelShape {
            pub fn empty() -> Self {
                Self(Vec::new())
            }
            pub fn from_box(aabb: Aabb) -> Result<Self, ()> {
                Ok(Self(vec![[aabb.0.0, aabb.0.1, aabb.0.2, aabb.1.0, aabb.1.1, aabb.1.2]]))
            }
            pub fn join(&self, other: &Self, _: ShapeOp) -> Result<Self, ()> {
                Ok(Self([self.0.clone(), other.0.clone()].concat()))
            }
            pub fn optimized(&self) -> Result<Self, ()> {
                Ok(self.clone())
            }
        }
    }
}

mod machinery_transmitter_collision {
    use super::horizon_core;
    include!("../../transmitter_collision.rs");
}

use machinery_transmitter_collision as collision;

const OVERWORLD: &str = "minecraft:overworld";
const NETHER: &str = "minecraft:the_nether";
const POS: [i32; 3] = [4, -60, 9];
const CENTER: [f64; 6] = [0.3125, 0.3125, 0.3125, 0.6875, 0.6875, 0.6875];
const ARM: [f64; 6] = [0.4375, 0.0, 0.4375, 0.5625, 0.0625, 0.5625];

/// O estado é do processo: cada teste guarda as formas dele numa posição só dele e as tira ao fim, sem esvaziar a tabela dos outros.
fn at(test: i32) -> [i32; 3] {
    [POS[0] + test, POS[1], POS[2]]
}

#[test]
fn a_stored_shape_is_the_union_of_its_boxes_in_the_level_in_place() {
    let pos = at(0);
    collision::enter(OVERWORLD);
    collision::set(OVERWORLD, pos, Some(vec![CENTER, ARM]));
    let shape = collision::shape_at(pos).expect("a forma guardada");
    assert_eq!(shape.0, vec![CENTER, ARM]);
    collision::set(OVERWORLD, pos, None);
    assert!(collision::shape_at(pos).is_none());
}

#[test]
fn the_shape_is_per_dimension_and_per_position() {
    let pos = at(1);
    collision::set(OVERWORLD, pos, Some(vec![CENTER]));
    collision::enter(NETHER);
    assert!(collision::shape_at(pos).is_none(), "o Nether não tem o cabo do overworld");
    collision::enter(OVERWORLD);
    assert!(collision::shape_at(pos).is_some());
    assert!(collision::shape_at(at(2)).is_none(), "outra posição");
    collision::set(OVERWORLD, pos, None);
}

#[test]
fn a_thread_reads_the_level_it_is_in() {
    let pos = at(3);
    collision::set(NETHER, pos, Some(vec![CENTER]));
    collision::enter(OVERWORLD);
    let other = std::thread::spawn(move || {
        collision::enter(NETHER);
        collision::shape_at(pos).is_some()
    })
    .join()
    .unwrap();
    assert!(other, "a outra thread está no Nether");
    assert!(collision::shape_at(pos).is_none(), "esta está no overworld");
    collision::set(NETHER, pos, None);
}

#[test]
fn a_thread_that_never_entered_a_level_reads_nothing() {
    let pos = at(4);
    collision::set(OVERWORLD, pos, Some(vec![CENTER]));
    let silent = std::thread::spawn(move || collision::shape_at(pos).is_none()).join().unwrap();
    assert!(silent);
    collision::set(OVERWORLD, pos, None);
}
