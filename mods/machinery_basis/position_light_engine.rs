// Entra em `crates/horizon-client-render/src/light.rs` como `pub mod machinery_light_engine` (`visibility = "pub"`). As cópias das funções do motor de luz do Horizon que leem a
// emissão de um bloco, com a emissão por posição de `machinery_light` no lugar da tabela por estado (decisão 274 do plano, opção A; pedido H283).
//
// CÓPIA DO PIN `f319701` (`crates/horizon-client-render/src/light.rs`):
// - `light_chunk` é o corpo de `light_chunk_with_scratch` (`:324-532`); a única mudança é a semente da luz de bloco (`:387-391`), marcada com `POSIÇÃO`.
// - `relight_layer` é o corpo de `relight_layer` (`:1065-1169`); as duas mudanças são o `checkNode` (`:1085-1087`) e as fontes da área limpa (`:1136-1138`), marcadas com `POSIÇÃO`.
// Na troca do pin, compare as duas funções do Horizon com as daqui e repita aqui o que mudou lá: sem isso a luz perto de um bloco que emite por posição segue a conta velha, em
// silêncio. Os HEAD canceláveis (`position_light_chunk.rs`, `position_relight_layer.rs`) só chamam as cópias no mundo do cliente e quando alguma posição emite; sem isso o corpo
// do jogo roda.
use super::{Chunk, FxHashMap, Light, LightChanges, LightScratch, LightTables, OFFSETS, Pos, Region, SCRATCH, VecDeque, W, level, lowest_sky_source, machinery_light, pack_lighting, set_level};

/// `light_chunk` com a emissão por posição. `None` quando nenhuma posição do 3x3 emite: o corpo do jogo roda.
pub fn light_chunk(region: &[[Option<&Chunk>; 3]; 3], tables: &LightTables, sky_light: bool) -> Option<Vec<Light>> {
    let centre = region[1][1]?;
    // Só os trabalhos do `Streaming` do cliente (`position_light_submit`): o mundo do servidor segue a tabela por estado.
    if !machinery_light::take_job(std::ptr::from_ref(centre) as usize) || !machinery_light::any_around(centre.x, centre.z) {
        return None;
    }
    Some(SCRATCH.with_borrow_mut(|scratch| light_chunk_with_scratch(region, tables, sky_light, scratch)))
}

fn light_chunk_with_scratch(
    region: &[[Option<&Chunk>; 3]; 3],
    tables: &LightTables,
    sky_light: bool,
    scratch: &mut LightScratch,
) -> Vec<Light> {
    let sections = region[1][1].expect("centre chunk").sections.len();
    // Highest occupied section in the region bounds the work.
    let mut top_section = 0;
    for chunk in region.iter().flatten().flatten() {
        for (i, s) in chunk.sections.iter().enumerate().rev() {
            if !s.is_uniform(0) {
                top_section = top_section.max(i + 1);
                break;
            }
        }
    }
    // One extra section above the highest occupied one: block light rises
    // up to 14 blocks into it (sky light there is always 15).
    let h = ((top_section + 1) * 16).min(sections * 16);
    let total = W * W * h;

    scratch.states.resize(total, 0);
    scratch.states.fill(0);
    scratch.sky.resize(total, 0);
    scratch.sky.fill(0);
    scratch.block.resize(total, 0);
    scratch.block.fill(0);
    scratch.clear_queues();

    let states = &mut scratch.states[..total];
    let sky = &mut scratch.sky[..total];
    let block = &mut scratch.block[..total];
    let queue = &mut scratch.queue;
    let sky_queue = &mut scratch.sky_queue;
    let ids = &mut scratch.ids;

    for (dz, row) in region.iter().enumerate() {
        let z_base = dz * 16;
        for (dx, chunk) in row.iter().enumerate() {
            let Some(chunk) = chunk else { continue };
            let x_base = dx * 16;
            // POSIÇÃO: as posições deste chunk que emitem, lidas uma vez.
            let cells = machinery_light::column(chunk.x, chunk.z);
            for (si, section) in chunk.sections.iter().enumerate().take(top_section) {
                if section.is_uniform(0) {
                    continue;
                }
                section.unpack(ids);
                let y_base = si * 16;
                for ly in 0..16 {
                    let y = y_base + ly;
                    let y_offset = y * (W * W);
                    for lz in 0..16 {
                        let z = z_base + lz;
                        let p_row = y_offset + z * W + x_base;
                        let i_row = (ly << 8) | (lz << 4);
                        for lx in 0..16 {
                            let id = ids[i_row + lx];
                            if id == 0 {
                                continue;
                            }
                            let p = p_row + lx;
                            let info = tables.info[id as usize];
                            states[p] = info as u16;
                            let mut emission = info >> 16;
                            // POSIÇÃO: `getLightEmission(state, level, pos)` no lugar da emissão do estado.
                            if !cells.is_empty() {
                                let at = (chunk.x * 16 + lx as i32, chunk.min_y() + y as i32, chunk.z * 16 + lz as i32);
                                let here = machinery_light::emission_in(&cells, emission as u8, at);
                                if u32::from(here) != emission && machinery_light::positional(id) {
                                    emission = u32::from(here);
                                }
                            }
                            if emission > 0 {
                                block[p] = emission as u8;
                                queue.push(p as u32);
                            }
                        }
                    }
                }
            }
        }
    }

    // Columns of missing neighbour chunks neither hold nor pass light.
    scratch.present.resize(W * W, false);
    for z in 0..W {
        for x in 0..W {
            scratch.present[z * W + x] = region[z / 16][x / 16].is_some();
        }
    }
    let present = &scratch.present[..W * W];

    scratch.lowest.resize(W * W, h as i32);
    let lowest = &mut scratch.lowest[..W * W];

    for z in 0..W {
        for x in 0..W {
            let col = z * W + x;
            if !present[col] || !sky_light {
                lowest[col] = h as i32;
                continue;
            }
            // The top layer (above the occupied range) is always open sky.
            let mut y = h as i32 - 1;
            let mut p = (h - 1) * (W * W) + col;
            sky[p] = 15;
            while y > 0 {
                let next_p = p - (W * W);
                if tables.packed_edge_occluded(states[p], states[next_p]) {
                    break;
                }
                y -= 1;
                p = next_p;
                sky[p] = 15;
            }
            lowest[col] = y;
        }
    }

    if sky_light {
        for z in 0..W {
            for x in 0..W {
                let col = z * W + x;
                if !present[col] {
                    continue;
                }
                let own = lowest[col];
                let mut deepest = own;
                for (nx, nz) in [
                    (x.wrapping_sub(1), z),
                    (x + 1, z),
                    (x, z.wrapping_sub(1)),
                    (x, z + 1),
                ] {
                    if nx < W && nz < W && present[nz * W + nx] {
                        deepest = deepest.max(lowest[nz * W + nx]);
                    }
                }
                // Seed propagation from the source cells that border darker cells.
                let from = own.max(0) as usize;
                let to = (deepest as usize).min(h.saturating_sub(1));
                if from <= to {
                    let mut p = from * (W * W) + col;
                    for _ in from..=to {
                        if sky[p] == 15 {
                            sky_queue.push(p as u32);
                        }
                        p += W * W;
                    }
                }
            }
        }
    }

    let block_masks = &tables.block_masks;
    let types = tables.types;

    let spread = |light: &mut [u8], queue: &mut Vec<u32>| {
        let mut head = 0;
        while head < queue.len() {
            let p = queue[head] as usize;
            head += 1;
            let level = light[p];
            if level <= 1 {
                continue;
            }
            let min_threshold = level - 1;
            let y = p / (W * W);
            let col = p % (W * W);
            let z = col / W;
            let x = col % W;
            let from = states[p];
            let from_row = (from >> 4) as usize * types;

            // Neighbours by linear offset; `present` only depends on the
            // column, and the axis bounds rule out wrapping.
            macro_rules! step_neighbour {
                ($d:expr, $q:expr) => {{
                    let q = $q;
                    if light[q] < min_threshold {
                        let to = states[q];
                        let damp = (to & 0xF).max(1) as u8;
                        if level > light[q] + damp {
                            if block_masks[from_row + (to >> 4) as usize] & (1 << $d) == 0 {
                                light[q] = level - damp;
                                queue.push(q as u32);
                            }
                        }
                    }
                }};
            }
            if y > 0 {
                step_neighbour!(0, p - W * W);
            }
            if y + 1 < h {
                step_neighbour!(1, p + W * W);
            }
            if z > 0 && present[col - W] {
                step_neighbour!(2, p - W);
            }
            if z + 1 < W && present[col + W] {
                step_neighbour!(3, p + W);
            }
            if x > 0 && present[col - 1] {
                step_neighbour!(4, p - 1);
            }
            if x + 1 < W && present[col + 1] {
                step_neighbour!(5, p + 1);
            }
        }
    };
    spread(sky, sky_queue);
    spread(block, queue);
    let result = pack_lighting(sections, h, sky, block, sky_light);
    scratch.clear_queues();
    result
}

/// `relight_layer` com a emissão por posição. `false` quando nenhuma posição emite, a camada é a do céu ou a thread não é a do cliente (o mundo do servidor segue a
/// tabela por estado): o corpo do jogo roda.
pub fn relight_layer(world: &mut Region, tables: &LightTables, sky: bool, seeds: &[Pos], force: bool, changed: &mut LightChanges) -> bool {
    if sky || !machinery_light::any() || !machinery_light::client_thread() {
        return false;
    }
    relight_layer_copy(world, tables, sky, seeds.to_vec(), force, changed);
    true
}

fn relight_layer_copy(
    world: &mut Region,
    tables: &LightTables,
    sky: bool,
    seeds: Vec<Pos>,
    force: bool,
    changed: &mut LightChanges,
) {
    let mut decrease: VecDeque<(Pos, u8)> = VecDeque::new();
    let mut cleared: Vec<Pos> = Vec::new();
    let mut increase: VecDeque<Pos> = VecDeque::new();
    for p in seeds {
        let Some(old) = level(world, sky, p) else {
            continue;
        };
        // `BlockLightEngine.checkNode`: a block emitting at least the light
        // already there keeps it and pulls its neighbours' light in. Clearing
        // it would darken and relight everything it lit (flowing lava next to
        // lava: thousands of cells per block).
        if !sky && !force && old > 0 {
            // POSIÇÃO: `getLightEmission(state, level, pos)` no lugar da emissão do estado.
            let emission = world
                .state(p.0, p.1, p.2)
                .map_or(0, |s| machinery_light::emission(s, tables.cells[s as usize].emission, p));
            if emission >= old {
                if emission > old {
                    set_level(world, sky, p, emission);
                    changed.push(p);
                }
                decrease.push_back((p, 0));
                increase.push_back(p);
                continue;
            }
        }
        if old > 0 {
            set_level(world, sky, p, 0);
            changed.push(p);
        }
        cleared.push(p);
        decrease.push_back((p, if force { 15 } else { old }));
    }
    while let Some((p, from)) = decrease.pop_front() {
        for (ox, oy, oz) in OFFSETS {
            let q = (p.0 + ox, p.1 + oy, p.2 + oz);
            let Some(l) = level(world, sky, q) else {
                continue;
            };
            if l == 0 {
                continue;
            }
            // Sky light runs straight down without loss, so a 15 below a
            // cleared 15 may have come from it too.
            if l < from || (sky && oy == -1 && l == 15 && from == 15) {
                set_level(world, sky, q, 0);
                changed.push(q);
                cleared.push(q);
                decrease.push_back((q, l));
            } else {
                increase.push_back(q);
            }
        }
    }
    // Sources inside the cleared area light up again.
    let mut sources: FxHashMap<(i32, i32), i32> = FxHashMap::default();
    crate::spans::time("lt_sources", || {
        for p in cleared {
            let seed = if sky {
                let lowest = *sources
                    .entry((p.0, p.2))
                    .or_insert_with(|| lowest_sky_source(world, tables, p.0, p.2));
                if p.1 >= lowest { 15 } else { 0 }
            } else {
                // POSIÇÃO: `getLightEmission(state, level, pos)` no lugar da emissão do estado.
                world
                    .state(p.0, p.1, p.2)
                    .map_or(0, |s| machinery_light::emission(s, tables.cells[s as usize].emission, p))
            };
            if seed > level(world, sky, p).unwrap_or(15) {
                set_level(world, sky, p, seed);
                changed.push(p);
                increase.push_back(p);
            }
        }
    });
    while let Some(p) = increase.pop_front() {
        let Some((from, packed)) = world.cell(p.0, p.1, p.2) else {
            continue;
        };
        let l = if sky { packed >> 4 } else { packed & 15 };
        if l <= 1 {
            continue;
        }
        for (d, (ox, oy, oz)) in OFFSETS.iter().enumerate() {
            let q = (p.0 + ox, p.1 + oy, p.2 + oz);
            let Some((to, packed)) = world.cell(q.0, q.1, q.2) else {
                continue;
            };
            let ql = if sky { packed >> 4 } else { packed & 15 };
            let next = l.saturating_sub(tables.cells[to as usize].dampening.max(1));
            if next > ql && !tables.blocked(from as usize, to as usize, d) {
                set_level(world, sky, q, next);
                changed.push(q);
                increase.push_back(q);
            }
        }
    }
}
