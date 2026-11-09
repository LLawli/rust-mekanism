// Roda no fim de `ambient_occlusion`, o AO e a luz suave de um quad de modelo: um quad de bloco de mod leva o sombreamento do NeoForge (`EnhancedBlockModelLighter`, `enhanced_lighting.rs`) no lugar do do
// vanilla. O mesher multiplica o brilho pelo sombreamento direcional da face depois, e o NeoForge já o tem no brilho: `before_directional` devolve o brilho que, multiplicado por ele, dá o do NeoForge.
// Supõe a face de `CardinalLighting.DEFAULT` (o Nether a muda) e sem `shadeDirectionOverride`: nenhum modelo do Mekanism tem `shade: false`.
fn machinery_basis_enhanced_ao(value: ([f32; 4], [u32; 4]), pad: &Padded, info: &[Info], own: &Info, pos: [i32; 3], positions: &[[f32; 3]; 4], face: D) -> ([f32; 4], [u32; 4]) {
    use super::machinery_enhanced as enhanced;
    if !enhanced::uses(pad.blocks[pi(pos[0], pos[1], pos[2])]) {
        return value;
    }
    let from = |f: enhanced::Face| match f {
        enhanced::Face::Down => D::Down,
        enhanced::Face::Up => D::Up,
        enhanced::Face::North => D::North,
        enhanced::Face::South => D::South,
        enhanced::Face::West => D::West,
        enhanced::Face::East => D::East,
    };
    let to = match face {
        D::Down => enhanced::Face::Down,
        D::Up => enhanced::Face::Up,
        D::North => enhanced::Face::North,
        D::South => enhanced::Face::South,
        D::West => enhanced::Face::West,
        D::East => enhanced::Face::East,
    };
    let cell = |d: [i32; 3]| {
        let p = [pos[0] + d[0], pos[1] + d[1], pos[2] + d[2]];
        let s = state(pad, info, p);
        enhanced::Cell { light: coords(pad, s, p), shade: s.shade, permeable: s.permeable }
    };
    let cardinal = |f: enhanced::Face| cardinal(false, from(f));
    let (gray, light) = enhanced::ambient_occlusion(&cell, own.full, &cardinal, &enhanced::Quad { positions, direction: to, shade_override: None });
    let directional = cardinal(to);
    (gray.map(|g| enhanced::before_directional(g, directional)), light)
}
