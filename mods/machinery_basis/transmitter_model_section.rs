// Roda no começo de `Mesher::mesh_reusing`: anota, por thread, a origem da seção que o mesher vai desenhar (o `chunk.x * 16`, o `y` da base da seção e o `chunk.z * 16`), para o
// `StateModels::state` achar a posição de um bloco pelas coordenadas locais que `hidden` anota.
fn machinery_transmitter_model_section(region: &[[Option<&Chunk>; 3]; 3], sy: i32) {
    let origin = region[1][1].map_or([0, 0, 0], |chunk| [chunk.x * 16, chunk.min_y() + sy * 16, chunk.z * 16]);
    machinery_transmitter_model::note_section(origin);
}
