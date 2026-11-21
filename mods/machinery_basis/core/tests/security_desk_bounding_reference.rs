//! O bloco de apoio da Security Desk e os itens dentro dela contra o original: `dev/referencia/rastros/security_desk_bounding.jsonl`
//! (`SecurityDeskBoundingTraceTest.java`, um `runGameTestServer`). O que é regra pura da lib é conferido aqui: a forma do apoio (`BlockBounding.proxyShape`), o
//! principal que ele acha pelo deslocamento, o ponto do clique repassado (`withHitResultForMain`) e a ordem do shift-clique nos dois slots. O resto do rastro (o
//! item solto, o Configurator, a explosão) é medido em jogo contra o port.
use machinery_basis_core::bounding;
use machinery_basis_core::security_desk::DeskSlot;
use serde_json::Value;

fn rows() -> Vec<Value> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../dev/referencia/rastros/security_desk_bounding.jsonl"
    );
    std::fs::read_to_string(path)
        .expect("o rastro existe")
        .lines()
        .map(|line| serde_json::from_str(line).expect("linha JSON válida"))
        .collect()
}

fn row<'a>(rows: &'a [Value], step: &str) -> &'a Value {
    rows.iter()
        .find(|row| row["step"] == step)
        .unwrap_or_else(|| panic!("o passo {step} está no rastro"))
}

fn boxes(value: &Value) -> Vec<[f64; 6]> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|b| std::array::from_fn(|i| b[i].as_f64().unwrap()))
        .collect()
}

/// Os voxels de 1/32 de bloco que as caixas cobrem: as duas listas podem dividir a mesma forma em caixas diferentes.
fn voxels(boxes: &[[f64; 6]]) -> std::collections::BTreeSet<[i64; 3]> {
    let mut out = std::collections::BTreeSet::new();
    for b in boxes {
        let r = |v: f64| (v * 32.0).round() as i64;
        for x in r(b[0])..r(b[3]) {
            for y in r(b[1])..r(b[4]) {
                for z in r(b[2])..r(b[5]) {
                    out.insert([x, y, z]);
                }
            }
        }
    }
    out
}

#[test]
fn the_bounding_block_has_the_shape_of_the_main_block_moved() {
    let rows = rows();
    let placed = row(&rows, "placed");
    let desk = boxes(&placed["desk_shape"]);
    // O apoio fica um bloco acima: a forma dele é a da mesa deslocada um bloco para baixo, na seleção e na colisão.
    let moved = bounding::proxy_shape(&desk, [0, 1, 0], [0, 0, 0]);
    assert_eq!(voxels(&moved), voxels(&boxes(&placed["above_shape"])));
    assert_eq!(voxels(&moved), voxels(&boxes(&placed["above_collision"])));
    assert_eq!(voxels(&desk), voxels(&boxes(&placed["desk_collision"])));
    // O deslocamento gravado no tile do apoio leva ao principal.
    let offset: Vec<i32> = placed["above_nbt"]["offset"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap() as i32)
        .collect();
    assert_eq!(
        bounding::main_pos([0, 1, 0], [offset[0], offset[1], offset[2]]),
        [0, 0, 0]
    );
    // A resistência do apoio a explosões é a do principal (a mesa tem -1), não a do bloco de apoio sozinho.
    assert_eq!(placed["above_resistance"], placed["desk_resistance"]);
    assert_eq!(placed["desk_resistance"].as_f64(), Some(-1.0));
    assert_eq!(placed["above_block_resistance"].as_f64(), Some(4.8));
    // Quebrar pelo apoio tem o progresso da mesa: o dono quebra, o estranho não.
    assert_eq!(placed["above_destroy_owner"], placed["desk_destroy_owner"]);
    assert_eq!(placed["above_destroy_stranger"].as_f64(), Some(0.0));
}

#[test]
fn the_aim_at_the_upper_half_hits_the_bounding_block() {
    let rows = rows();
    let aim = row(&rows, "aim");
    assert_eq!(aim["upper"]["pos"], "0 1 0");
    assert_eq!(aim["upper"]["face"], "north");
    assert_eq!(aim["lower"]["pos"], "0 0 0");
    // O raio para no monitor (z = 0,6875), dentro do bloco de cima, e não na face do bloco.
    let point: Vec<f64> = aim["upper"]["point"]
        .as_str()
        .unwrap()
        .split(' ')
        .map(|v| v.parse().unwrap())
        .collect();
    assert!((point[2] - 0.6875).abs() < 1e-9, "{point:?}");
    // O clique repassado ao principal: o meio da mesma face dele.
    assert_eq!(
        bounding::main_hit_point([0, 0, 0], [0, 0, -1]),
        [0.5, 0.5, 0.0]
    );
}

#[test]
fn the_items_rest_on_the_shape_of_the_desk() {
    let rows = rows();
    let rest = row(&rows, "item_rest");
    let desk = boxes(&row(&rows, "placed")["desk_shape"]);
    let top_at = |x: f64, z: f64| {
        desk.iter()
            .filter(|b| b[0] <= x && x < b[3] && b[2] <= z && z < b[5])
            .map(|b| b[4])
            .fold(0.0, f64::max)
    };
    assert_eq!(rest["on_monitor_y"].as_f64(), Some(top_at(0.5, 0.75)));
    assert_eq!(rest["on_front_y"].as_f64(), Some(top_at(0.5, 0.2)));
}

#[test]
fn shift_click_goes_through_the_two_slots_in_the_order_of_the_menu() {
    let rows = rows();
    // O menu da mesa: os dois slots do tile primeiro (o de destravar e o de travar), depois o inventário e a barra.
    let order: Vec<&str> = row(&rows, "menu_slots")["order"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(&order[..2], &["tile:0", "tile:0"]);
    assert_eq!(DeskSlot::ORDER, [DeskSlot::Unlock, DeskSlot::Lock]);
    // O Crusher sem dono vai para o slot de travar, e com dono para o de destravar; a pedra fica no inventário.
    let slots = |step: &str| row(&rows, step)["slots"].clone();
    assert!(slots("shift_crusher_unowned")[0].is_null());
    assert_eq!(slots("shift_crusher_unowned")[1]["id"], "mekanism:crusher");
    assert_eq!(slots("shift_crusher_owned")[0]["id"], "mekanism:crusher");
    assert!(slots("shift_crusher_owned")[1].is_null());
    assert_eq!(
        slots("shift_heat_unowned")[1]["id"],
        "mekanismgenerators:heat_generator"
    );
    let stone = row(&rows, "shift_stone")["inventory"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["item"]["id"] == "minecraft:stone")
        .unwrap()["slot"]
        .clone();
    // Da barra (slot 1) para a primeira vaga do inventário (slot 9): a pedra não cabe na mesa e troca de região.
    assert_eq!(stone, 9);
}
