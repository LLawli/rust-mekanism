//! O módulo `water-like` que entra em `crates/horizon-client/src/motion.rs` (`mods/machinery_basis/water_like.rs`): a tabela de estados de líquido de mod e o par
//! `mob_flip` e `mob_restore` das injeções `fire_mob_flip` e `fire_mob_restore`. O arquivo é código de uma função só e de `std`, então o teste o compila como está.
#[path = "../../water_like.rs"]
mod water_like;

use water_like::{get, mob_flip, mob_restore, set};

/// Os estados 0 a 3 são de ar, o 4 é de líquido de mod no nível 0 (`entry` 1) e o 5 no nível 7.
fn table() {
    set(vec![0, 0, 0, 0, 1, 8]);
}

#[test]
fn the_table_gives_the_level_of_a_fluid_block_and_nothing_for_the_rest() {
    table();
    assert_eq!(get(4), Some(0));
    assert_eq!(get(5), Some(7));
    assert_eq!(get(0), None);
    assert_eq!(get(99), None);
}

#[test]
fn a_burning_mob_in_a_mod_fluid_leaves_the_water_until_it_is_restored() {
    table();
    // Os pés no fluido de mod: o mob em chamas deixa de estar na água, e a restauração a devolve.
    let mut in_water = true;
    mob_flip(7, true, 4, 0, &mut in_water);
    assert!(!in_water);
    mob_restore(7, &mut in_water);
    assert!(in_water);
    // Os olhos no fluido de mod valem também.
    mob_flip(7, true, 0, 5, &mut in_water);
    assert!(!in_water);
    mob_restore(7, &mut in_water);
    assert!(in_water);
}

#[test]
fn only_a_burning_mob_in_a_mod_fluid_and_in_the_water_is_flipped() {
    table();
    // Sem fogo, fora do fluido de mod (a água do vanilla apaga) ou já fora da água: nada muda e nada a devolver.
    for (burning, feet, eye, start) in [
        (false, 4, 4, true),
        (true, 0, 0, true),
        (true, 4, 4, false),
    ] {
        let mut in_water = start;
        mob_flip(9, burning, feet, eye, &mut in_water);
        assert_eq!(in_water, start, "{burning} {feet} {eye} {start}");
        mob_restore(9, &mut in_water);
        assert_eq!(in_water, start, "{burning} {feet} {eye} {start}");
    }
}

#[test]
fn the_restore_is_for_the_mob_that_was_flipped_and_a_stale_mark_does_not_survive() {
    table();
    let mut in_water = true;
    mob_flip(1, true, 4, 4, &mut in_water);
    // Outro mob não leva a água do primeiro.
    let mut other = false;
    mob_restore(2, &mut other);
    assert!(!other);
    assert!(!in_water);
    // Uma chamada nova de `update_fluids` do mesmo mob, agora sem fogo, apaga a marca: a restauração não liga uma água que o mob não tinha.
    let mut in_water = false;
    mob_flip(1, false, 4, 4, &mut in_water);
    mob_restore(1, &mut in_water);
    assert!(!in_water);
    // Depois de devolvida, a marca some: uma segunda restauração não faz nada.
    let mut in_water = true;
    mob_flip(3, true, 4, 4, &mut in_water);
    mob_restore(3, &mut in_water);
    in_water = false;
    mob_restore(3, &mut in_water);
    assert!(!in_water);
}
