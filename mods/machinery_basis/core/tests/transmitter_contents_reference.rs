//! O conteúdo dos transmissores contra o Java (`dev/referencia/testes/TransmitterContentsReference.java`, as fórmulas do original copiadas, em `rastros/transmitter_contents.jsonl`):
//! o alfa do cabo, o do tubo, o do fluido leve, o estágio do cano, o brilho do conteúdo e a cor do condutor pela temperatura, com o `colorCache` do processo.
use machinery_basis_core::transmission::connection::ConnectionType as Kind;
use machinery_basis_core::transmitter_contents::*;
use serde_json::Value;

fn trace() -> Vec<Value> {
    let text = include_str!("../../../../dev/referencia/rastros/transmitter_contents.jsonl");
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn of_kind<'a>(lines: &'a [Value], kind: &'a str) -> impl Iterator<Item = &'a Value> {
    lines.iter().filter(move |line| line["kind"] == kind)
}

fn argb(value: &Value) -> u32 {
    value.as_i64().unwrap() as i32 as u32
}

#[test]
fn the_scale_maps_to_the_original_tints_and_stages() {
    let lines = trace();
    let mut seen = 0;
    for line in of_kind(&lines, "scale") {
        let scale = line["scale"].as_f64().unwrap() as f32;
        assert_eq!(energy_tint(scale), argb(&line["energy"]), "energia {scale}");
        assert_eq!(
            tube_tint(scale, 0xF2CD67),
            argb(&line["tube"]),
            "tubo {scale}"
        );
        assert_eq!(
            fluid_tint(0xFFFFFF, true, scale),
            argb(&line["gas"]),
            "gás {scale}"
        );
        assert_eq!(
            fluid_tint(0x3366CC, true, scale),
            argb(&line["gas_tint"]),
            "gás com cor {scale}"
        );
        assert_eq!(
            fluid_tint(0xFFFFFF, false, scale),
            argb(&line["liquid"]),
            "líquido {scale}"
        );
        assert_eq!(
            pipe_stage(false, scale),
            line["stage"].as_i64().unwrap() as i32,
            "estágio {scale}"
        );
        assert_eq!(
            pipe_stage(true, scale),
            line["stage_gas"].as_i64().unwrap() as i32,
            "estágio do gás {scale}"
        );
        seen += 1;
    }
    assert_eq!(seen, 16);
}

#[test]
fn the_emission_raises_both_light_channels() {
    let lines = trace();
    let mut seen = 0;
    for line in of_kind(&lines, "light") {
        let (sky, block, emission) = (
            line["sky"].as_u64().unwrap() as u32,
            line["block"].as_u64().unwrap() as u32,
            line["emission"].as_u64().unwrap() as u32,
        );
        let (out_sky, out_block) = light_with_emission(sky, block, emission);
        let packed = line["packed"].as_i64().unwrap() as u32;
        assert_eq!(
            (out_block << 16) | out_sky,
            packed,
            "{sky} {block} {emission}"
        );
        seen += 1;
    }
    assert_eq!(seen, 6);
}

#[test]
fn the_conductor_color_follows_the_temperature_with_the_process_cache() {
    let lines = trace();
    let mut cache = HeatColorCache::default();
    let mut seen = 0;
    for line in of_kind(&lines, "heat") {
        let temperature = line["temperature"].as_f64().unwrap();
        assert_eq!(
            cache
                .color_from_temp(temperature, CONDUCTOR_BASE_COLOR)
                .argb(),
            argb(&line["argb"]),
            "temperatura {temperature}"
        );
        seen += 1;
    }
    assert_eq!(seen, 26);
}

#[test]
fn a_bucket_keeps_the_alpha_of_the_first_temperature_that_filled_it() {
    // O balde de 300 K a 399 K nasce com 300 K (alfa 0,3): 350 K usa a mesma cor em vez de a dele (alfa 0,35).
    let mut cache = HeatColorCache::default();
    let first = cache.color_from_temp(300.0, CONDUCTOR_BASE_COLOR);
    assert_eq!(cache.color_from_temp(350.0, CONDUCTOR_BASE_COLOR), first);
    let mut fresh = HeatColorCache::default();
    assert_ne!(fresh.color_from_temp(350.0, CONDUCTOR_BASE_COLOR), first);
}

#[test]
fn the_contents_gates_follow_the_network_and_the_scale() {
    assert!(!cable_shows_contents(false, 1.0));
    assert!(!cable_shows_contents(true, 0.0));
    assert!(cable_shows_contents(true, 0.001));
    assert!(!resource_shows_contents(true, false, false, 1.0));
    assert!(!resource_shows_contents(true, true, true, 1.0));
    assert!(!resource_shows_contents(true, true, false, 0.0));
    assert!(resource_shows_contents(true, true, false, 0.5));
}

#[test]
fn the_contents_model_keeps_one_group_per_side_type() {
    let all = all_sides([
        Kind::Normal,
        Kind::Push,
        Kind::Pull,
        Kind::None,
        Kind::Normal,
        Kind::Normal,
    ]);
    let groups = contents_groups(all);
    assert_eq!(
        groups,
        [
            "downNORMAL",
            "upPUSH",
            "northPULL",
            "southNONE",
            "westNORMAL",
            "eastNORMAL"
        ]
    );
    assert_eq!(all_groups().len(), 24);
    // O cano só põe `PUSH` e `PULL` no modelo, e sem nenhum o modelo é vazio.
    let pipe = pipe_contents_types([
        Kind::Normal,
        Kind::Push,
        Kind::Pull,
        Kind::None,
        Kind::Normal,
        Kind::Normal,
    ]);
    assert_eq!(contents_groups(pipe), ["upPUSH", "northPULL"]);
    assert!(contents_groups(pipe_contents_types([Kind::Normal; 6])).is_empty());
}

#[test]
fn a_pipe_with_only_normal_arms_draws_the_core_and_one_cube_per_arm() {
    let frame = pipe_frame([Kind::Normal; 6], 50);
    assert!(frame.render_base);
    // O núcleo desenha os lados de cima e de baixo (a base e o estágio não é o cheio), e os `NORMAL` têm cubo.
    assert_eq!(
        frame.core_sides,
        side_bit(machinery_basis_core::sides::Face::Down)
            | side_bit(machinery_basis_core::sides::Face::Up)
    );
    let cuboids = pipe_cuboids(&frame);
    assert_eq!(cuboids.len(), 7);
    let ratio = 50.0 / 100.0 * 0.45;
    let core = cuboids.last().unwrap();
    assert_eq!(core.min, [0.27, 0.27, 0.27]);
    assert_eq!(core.max, [0.73, 0.27 + ratio, 0.73]);
}

#[test]
fn a_full_pipe_draws_no_cap_faces_on_the_core() {
    let frame = pipe_frame([Kind::Normal; 6], 99);
    assert_eq!(frame.core_sides, 0);
    let frame = pipe_frame([Kind::None; 6], 50);
    assert!(frame.render_base);
    assert_eq!(frame.core_sides, ALL_FACES);
}

#[test]
fn a_cuboid_inside_a_block_has_one_quad_per_side_with_the_original_uv() {
    let cuboid = Cuboid {
        min: [0.27, 0.27, 0.27],
        max: [0.73, 0.5, 0.73],
        sides: ALL_FACES,
    };
    let quads = cuboid_quads(&cuboid);
    assert_eq!(quads.len(), 6);
    let up = quads
        .iter()
        .find(|quad| quad.side == machinery_basis_core::sides::Face::Up)
        .unwrap();
    // `drawFace`: (minU, maxV), (minU, minV), (maxU, minV), (maxU, maxV). O V do eixo Y vem de `minBound(z2, z1)` (o maior primeiro) e entra como `1 - bound`.
    assert_eq!(
        up.corners,
        [
            [0.27, 0.5, 0.27],
            [0.27, 0.5, 0.73],
            [0.73, 0.5, 0.73],
            [0.73, 0.5, 0.27]
        ]
    );
    assert_eq!(
        up.uv,
        [
            [0.27, 1.0 - 0.73],
            [0.27, 1.0 - 0.27],
            [0.73, 1.0 - 0.27],
            [0.73, 1.0 - 0.73]
        ]
    );
    let north = quads
        .iter()
        .find(|quad| quad.side == machinery_basis_core::sides::Face::North)
        .unwrap();
    // O U do eixo Z vem de `minBound(x2, x1)`: o maior primeiro.
    assert_eq!(north.uv[0][0], 0.73);
    assert_eq!(north.uv[2][0], 0.27);
}
