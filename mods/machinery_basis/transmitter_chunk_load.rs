fn machinery_basis_transmitter_chunk_load(game: &mut crate::game::Game<'_>, dimension: &str, chunk: [i32; 2]) {
    crate::game::machinery_basis::transmitters_chunk_loaded(game, dimension, chunk);
}
