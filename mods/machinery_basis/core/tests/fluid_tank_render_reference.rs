//! O desenho do Fluid Tank contra o Java (`dev/referencia/testes/FluidTankRenderReference.java`, as fórmulas do original copiadas, em `rastros/fluid_tank_render.jsonl`): a
//! altura do conteúdo e a da válvula pelos bits do `float`, a tinta, e quando cada caixa aparece, no bloco e no item.
use machinery_basis_core::fluid_tank_render::*;
use serde_json::Value;

fn trace() -> Vec<Value> {
    let text = include_str!("../../../../dev/referencia/rastros/fluid_tank_render.jsonl");
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn bits(value: &Value) -> u32 {
    value.as_i64().unwrap() as i32 as u32
}

#[test]
fn the_block_draws_what_the_original_draws() {
    let mut seen = 0;
    for line in trace().iter().filter(|line| line["kind"] == "tile") {
        let empty = line["empty"].as_bool().unwrap();
        let gaseous = line["gaseous"].as_bool().unwrap();
        let scale = f32::from_bits(bits(&line["scale"]));
        let frame = tile_frame(
            (!empty).then(|| bits(&line["color"])),
            gaseous,
            scale,
            line["valve"].as_bool().unwrap(),
        );
        assert_eq!(frame.tint, bits(&line["tint"]), "{line}");
        assert_eq!(
            frame.contents.is_some(),
            line["contents"].as_bool().unwrap(),
            "{line}"
        );
        if let Some(contents) = frame.contents {
            assert_eq!(contents.max[1].to_bits(), bits(&line["max_y"]), "{line}");
            assert_eq!(
                contents.min,
                [CONTENTS_MIN_XZ, CONTENTS_MIN_Y, CONTENTS_MIN_XZ]
            );
            assert_eq!([contents.max[0], contents.max[2]], [CONTENTS_MAX_XZ; 2]);
            assert_eq!(contents.sides, 0b11_1110, "todos os lados menos o de baixo");
        }
        // No Java o tanque vazio nunca é mais leve que o ar: o rastro grava o `gaseous` da pergunta, e a válvula de um tanque vazio aparece.
        let draws_valve = line["valve"].as_bool().unwrap() && !(gaseous && !empty);
        assert_eq!(frame.valve.is_some(), draws_valve, "{line}");
        if let Some(valve) = frame
            .valve
            .filter(|_| line["draws_valve"].as_bool().unwrap())
        {
            assert_eq!(valve.min[1].to_bits(), bits(&line["valve_min_y"]), "{line}");
            assert_eq!(valve.max, [VALVE_MAX_XZ, VALVE_MAX_Y, VALVE_MAX_XZ]);
            assert_eq!([valve.min[0], valve.min[2]], [VALVE_MIN_XZ; 2]);
            assert_eq!(valve.sides, 0b11_1100, "só os quatro lados");
        }
        seen += 1;
    }
    assert!(seen >= 3000, "{seen}");
}

#[test]
fn the_item_draws_what_the_original_draws() {
    let mut seen = 0;
    for line in trace().iter().filter(|line| line["kind"] == "item") {
        let frame = item_frame(
            bits(&line["color"]),
            line["gaseous"].as_bool().unwrap(),
            line["amount"].as_i64().unwrap(),
            line["capacity"].as_i64().unwrap(),
        );
        assert_eq!(
            frame.is_some(),
            line["contents"].as_bool().unwrap(),
            "{line}"
        );
        if let Some((tint, contents)) = frame {
            assert_eq!(tint, bits(&line["tint"]), "{line}");
            assert_eq!(contents.max[1].to_bits(), bits(&line["max_y"]), "{line}");
        }
        seen += 1;
    }
    assert!(seen >= 400, "{seen}");
}

#[test]
fn the_view_packet_round_trips() {
    let view = TankView {
        scale: 0.618,
        fluid: Some("minecraft:lava".to_owned()),
        valve: None,
    };
    let bytes = encode_view("minecraft:overworld", [-3, 64, 1 << 20], &view);
    assert_eq!(
        decode_view(&bytes),
        Some((
            "minecraft:overworld".to_owned(),
            [-3, 64, 1 << 20],
            view.clone()
        ))
    );
    let both = TankView {
        scale: 1.0,
        fluid: Some("minecraft:water".to_owned()),
        valve: Some("minecraft:water".to_owned()),
    };
    assert_eq!(
        decode_view(&encode_view("d", [0; 3], &both)).unwrap().2,
        both
    );
    assert_eq!(
        decode_view(&encode_view("d", [0; 3], &TankView::default()))
            .unwrap()
            .2,
        TankView::default()
    );
    assert_eq!(decode_view(&bytes[..bytes.len() - 1]), None);
    let mut longer = bytes.clone();
    longer.push(0);
    assert_eq!(decode_view(&longer), None);
}
