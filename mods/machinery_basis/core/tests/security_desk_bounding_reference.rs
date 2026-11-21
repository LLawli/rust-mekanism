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

/// Um nível simulado para a fila do apoio: os blocos por posição, cada troca avisa a fila (o gancho `block_changed`) e, como o `with_level` do jogo, acaba com
/// a fila esvaziada (`flush_spill`, o fim de cada operação no mundo); a troca feita de dentro do `flush` não o chama de novo (`BOUNDING_FLUSH_BUSY` da cola).
struct Level<'a> {
    queue: &'a std::sync::Mutex<bounding::Queue>,
    blocks: std::collections::HashMap<[i32; 3], String>,
    busy: bool,
}

const DESK: &str = "mekanism:security_desk";
const BOUNDING: &str = "mekanism:bounding_block";
/// A mesa e um segundo principal com o mesmo apoio no mesmo lugar (como os outros seis blocos com apoio do original).
const OTHER: &str = "mekanism:other_main";
const SPECS: [bounding::Spec; 2] = [
    bounding::Spec {
        main: DESK,
        bounding: BOUNDING,
        offsets: &[[0, 1, 0]],
    },
    bounding::Spec {
        main: OTHER,
        bounding: BOUNDING,
        offsets: &[[0, 1, 0]],
    },
];

impl Level<'_> {
    /// `level.setBlock(pos, block, 3)`: uma operação no mundo.
    fn set(&mut self, pos: [i32; 3], block: &str) {
        let old = self
            .blocks
            .insert(pos, block.to_owned())
            .unwrap_or_else(|| "minecraft:air".to_owned());
        self.queue
            .lock()
            .unwrap()
            .changed(&SPECS, "minecraft:overworld", pos, &old, block);
        if !self.busy {
            self.busy = true;
            bounding::flush(self.queue, self);
            self.busy = false;
        }
    }

    fn at(&self, pos: [i32; 3]) -> &str {
        self.blocks
            .get(&pos)
            .map_or("minecraft:air", String::as_str)
    }
}

impl bounding::World for Level<'_> {
    fn level(&self) -> String {
        "minecraft:overworld".to_owned()
    }
    fn block(&self, pos: [i32; 3]) -> String {
        self.at(pos).to_owned()
    }
    fn remove(&mut self, pos: [i32; 3]) {
        self.set(pos, "minecraft:air");
    }
}

/// O nome do bloco como o rastro o escreve (`Block{mekanism:security_desk}[...]`).
fn block_name(value: &Value) -> &str {
    let text = value.as_str().unwrap();
    text.trim_start_matches("Block{").split('}').next().unwrap()
}

#[test]
fn the_main_block_leaves_in_the_same_operation_that_replaces_the_bounding_block() {
    // `TileEntityBoundingBlock.preRemoveSideEffects` (`:74-85`): o `setBlock` que troca o apoio já tira o principal; o rastro lê a mesa logo depois dele.
    let rows = rows();
    for (step, with) in [
        ("bounding_replaced_by_air", "minecraft:air"),
        ("bounding_replaced_by_stone", "minecraft:stone"),
    ] {
        let trace = row(&rows, step);
        let queue = std::sync::Mutex::new(bounding::Queue::new());
        let mut level = Level {
            queue: &queue,
            blocks: Default::default(),
            busy: false,
        };
        let desk = [3, -60, 3];
        level.blocks.insert(desk, DESK.to_owned());
        level.blocks.insert([3, -59, 3], BOUNDING.to_owned());
        assert_eq!(block_name(&trace["desk_before"]), level.at(desk), "{step}");
        level.set([3, -59, 3], with);
        assert_eq!(block_name(&trace["desk"]), level.at(desk), "{step}");
        assert_eq!(block_name(&trace["above"]), level.at([3, -59, 3]), "{step}");
        assert!(
            queue.lock().unwrap().is_empty(),
            "{step}: nada fica para a passada seguinte"
        );
    }
}

#[test]
fn the_main_block_that_leaves_takes_its_bounding_block_in_the_same_operation() {
    // `BlockMekanism.preRemoveSideEffects` com `removeBoundingBlocks`: a mesa trocada por pedra leva o apoio; o apoio que sai assim não procura o principal (no
    // original o tile dele sai antes), então um bloco que já está no lugar do principal fica.
    let queue = std::sync::Mutex::new(bounding::Queue::new());
    let mut level = Level {
        queue: &queue,
        blocks: Default::default(),
        busy: false,
    };
    level.blocks.insert([0, 0, 0], DESK.to_owned());
    level.blocks.insert([0, 1, 0], BOUNDING.to_owned());
    level.set([0, 0, 0], "minecraft:stone");
    assert_eq!(level.at([0, 0, 0]), "minecraft:stone");
    assert_eq!(level.at([0, 1, 0]), "minecraft:air");
    assert!(queue.lock().unwrap().is_empty());
    // Outra mesa posta no lugar, com o apoio de outra mesa abaixo trocado: só a mesa de baixo sai.
    level.blocks.insert([0, 0, 0], DESK.to_owned());
    level.blocks.insert([0, 1, 0], BOUNDING.to_owned());
    level.blocks.insert([0, -1, 0], DESK.to_owned());
    level.set([0, 1, 0], "minecraft:stone");
    assert_eq!(level.at([0, 0, 0]), "minecraft:air");
    assert_eq!(
        level.at([0, -1, 0]),
        DESK,
        "o principal só é procurado pelos lugares de apoio de quem o bloco era"
    );
    // A mesa trocada direto por outro principal (`setBlock`, sem `setPlacedBy`): o apoio dela sai e não leva o bloco novo, porque o apoio que sai pela fila não
    // procura principal.
    level.blocks.clear();
    level.blocks.insert([0, 0, 0], DESK.to_owned());
    level.blocks.insert([0, 1, 0], BOUNDING.to_owned());
    level.set([0, 0, 0], OTHER);
    assert_eq!(level.at([0, 0, 0]), OTHER);
    assert_eq!(level.at([0, 1, 0]), "minecraft:air");
}

#[test]
fn the_redstone_of_the_bounding_block_follows_the_neighbor_signal() {
    // `TileEntityBoundingBlock.onNeighborChange`: só muda quando o sinal é outro.
    assert_eq!(bounding::redstone_change(0, 15), Some(15));
    assert_eq!(bounding::redstone_change(15, 15), None);
    assert_eq!(bounding::redstone_change(15, 0), Some(0));
}

#[test]
fn the_bounding_block_saves_the_redstone_it_was_told_about() {
    // `bounding_redstone`: um bloco de redstone encostado no apoio avisa o apoio (`BlockBounding.neighborChanged`), o tile guarda 15 no NBT e no que vai ao
    // cliente; tirado, volta a 0.
    let rows = rows();
    let trace = row(&rows, "bounding_redstone");
    let before = trace["before_nbt"]["redstone"].as_i64().unwrap() as i32;
    assert_eq!(before, 0, "o apoio nasce com 0");
    let powered = bounding::redstone_change(before, 15).unwrap();
    assert_eq!(trace["powered_nbt"]["redstone"], powered);
    assert_eq!(trace["powered_update_tag"]["redstone"], powered);
    assert_eq!(
        trace["powered_update_tag"]["offset"],
        serde_json::json!([0, -1, 0])
    );
    assert_eq!(
        trace["unpowered_nbt"]["redstone"],
        bounding::redstone_change(powered, 0).unwrap()
    );
}

#[test]
fn ctrl_pick_on_the_bounding_block_carries_the_data_of_the_bounding_tile() {
    // `ctrl_pick_bounding`: `handlePickItemFromBlock` com `includeData` dá o item do principal (`BlockBounding.getCloneItemStack`) e acrescenta o
    // `block_entity_data` do tile da posição mirada, o apoio (`addBlockDataToItem`: `saveCustomOnly`, sem posição nem componentes, com o tipo).
    let rows = rows();
    let trace = row(&rows, "ctrl_pick_bounding");
    let without = &trace["without_data"]["components"];
    let with = &trace["with_data"]["components"];
    assert!(without.get("minecraft:block_entity_data").is_none());
    assert_eq!(
        with["mekanism:owner"], without["mekanism:owner"],
        "o resto do item é o do principal"
    );
    let data = &with["minecraft:block_entity_data"];
    assert_eq!(data["id"], "mekanism:bounding_block");
    assert_eq!(data["offset"], serde_json::json!([0, -1, 0]));
    assert_eq!(
        data["redstone"], 15,
        "o bloco de redstone ainda estava encostado"
    );
    let keys: std::collections::BTreeSet<&str> = data
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        ["id", "offset", "redstone"].into_iter().collect(),
        "sem x, y, z nem components"
    );
    // O pick com o Ctrl na mesa não traz `block_entity_data` (o que a mesa copia ao item é a frente do pick block da base).
    assert!(
        trace["desk_with_data"]["components"]
            .get("minecraft:block_entity_data")
            .is_none()
    );
}

#[test]
fn the_configurator_in_empty_drops_the_item_out_of_the_clicked_face() {
    // `empty_sneak`: o Crusher do slot sai pela face norte da mesa (`Block.popResourceFromFace`), na posição relativa à mesa que o rastro grava. Os dois sorteios
    // (x e y) saem da própria posição pela conta do Java; a face dá o resto.
    let rows = rows();
    let pos: Vec<f64> = row(&rows, "empty_sneak")["entities"][0]["pos"]
        .as_str()
        .unwrap()
        .split(' ')
        .map(|v| v.parse().unwrap())
        .collect();
    let draws = [(pos[0] - 0.25) / 0.5, (pos[1] + 0.125 - 0.25) / 0.5];
    assert!(draws.iter().all(|d| (0.0..1.0).contains(d)), "{draws:?}");
    let mut next = draws.into_iter().chain([0.5, 0.5]);
    let (at, _) = machinery_basis_core::pop_resource::from_face([0, 0, 0], [0, 0, -1], || {
        next.next().unwrap()
    });
    for axis in 0..3 {
        assert!((at[axis] - pos[axis]).abs() < 1e-12, "{at:?} {pos:?}");
    }
}
