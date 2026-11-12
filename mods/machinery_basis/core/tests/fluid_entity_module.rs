//! O módulo `water-like` do `machinery_basis` (`water_like.rs`, injetado em `motion.rs`) compilado aqui por `include!`: as contas puras do contato da caixa com
//! o fluido de mod, que as injeções `fluid_entity_*` usam, contra as regras de `fluid_entity` (as do rastro `fluids_entity.jsonl`).

#[allow(dead_code)]
mod machinery_water_like {
    include!("../../water_like.rs");
}

use machinery_basis_core::fluid_entity;
use machinery_water_like::contact;

/// Uma coluna de fontes de `y0` a `y1` (inclusive), o `brine` do rastro.
fn column(y0: i32, y1: i32) -> impl Fn(i32, i32, i32) -> Option<u8> {
    move |x, y, z| (x == 2 && z == 2 && (y0..=y1).contains(&y)).then_some(0)
}

const PLAYER: ([f64; 3], [f64; 3]) = ([2.2, 3.0, 2.2], [2.8, 4.8, 2.8]);
const NONE: &dyn Fn(i32, i32, i32) -> bool = &|_, _, _| false;

#[test]
fn heights_agree_with_the_core_rules() {
    for level in 0..=15u8 {
        assert_eq!(machinery_water_like::amount(level), fluid_entity::amount(level));
        for same_above in [false, true] {
            assert_eq!(
                machinery_water_like::height(level, same_above),
                f64::from(fluid_entity::height(level, same_above))
            );
        }
    }
}

#[test]
fn a_player_in_the_source_column_touches_it_with_feet_and_eyes() {
    let c = contact(PLAYER.0, PLAYER.1, 3.0 + 1.62, &column(3, 6), NONE);
    assert!(c.touching && c.feet && c.eye && !c.real);
    assert!(c.mod_only());
}

#[test]
fn a_player_above_the_column_touches_nothing() {
    let (min, max) = ([2.2, 9.0, 2.2], [2.8, 10.8, 2.8]);
    let c = contact(min, max, 9.0 + 1.62, &column(3, 6), NONE);
    assert_eq!((c.touching, c.feet, c.eye), (false, false, false));
}

#[test]
fn the_top_of_the_fluid_must_reach_the_feet() {
    // A última fonte (y = 6) sem nada em cima vai a 6 + 8/9 = 6,889: os pés em 6,95 não alcançam, em 6,8 alcançam.
    let reach = |feet: f64| contact([2.2, feet, 2.2], [2.8, feet + 1.8, 2.8], feet + 1.62, &column(3, 6), NONE);
    assert!(!reach(6.95).touching && !reach(6.95).feet);
    assert!(reach(6.8).touching && reach(6.8).feet);
    // A célula dos pés (`blockPosition`) é de mod, mas a caixa de pé na fonte mais alta toca a coluna toda.
    assert!(reach(6.8).feet);
}

#[test]
fn real_fluid_in_the_box_is_reported_apart() {
    let water_at_feet = |x: i32, y: i32, z: i32| (x, y, z) == (3, 3, 2);
    let c = contact([2.6, 3.0, 2.2], [3.2, 4.8, 2.8], 4.6, &column(3, 6), &water_at_feet);
    assert!(c.touching && c.real && !c.mod_only());
}

#[test]
fn the_player_update_flag_is_per_thread() {
    machinery_water_like::set_player_update(true);
    assert!(machinery_water_like::player_update_mod_only());
    std::thread::spawn(|| assert!(!machinery_water_like::player_update_mod_only()))
        .join()
        .unwrap();
    machinery_water_like::set_player_update(false);
}
