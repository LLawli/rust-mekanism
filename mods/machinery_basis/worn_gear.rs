// O equipamento vestido que o mod desenha por código (o MekaSuit, o jetpack, as botas com molas, o tanque de mergulho): o que o `HumanoidArmorLayer` do original faz com o
// `ICustomArmor` de cada item. As regras (quais modelos, em quais partes, com que passos) ficam em machinery_basis_core::worn_gear; este arquivo só as liga ao Horizon: a pose
// da parte do corpo sai do modelo do mob (`part_transform`, a mesma que o vanilla usa para a cabeça de abóbora e o item na mão), e os modelos avulsos (`native_draw_model`)
// entram no mesmo vetor de vértices do mob.
use crate::game::Game;
use glam::{DVec3, Mat4, Vec3};
use horizon_client::entities::ArmorStand;
use horizon_client::items::ItemStack;
use horizon_client::mesher::Vertex;
use horizon_client::mobs::anim::Poses;
use horizon_client::mobs::render::ARMOR_GLINT;
use horizon_client::mobs::{Mob, Part, render};
use machinery_basis_core::worn_gear::{self as worn, Attachment, Foil, Gear, Worn};
use std::ops::ControlFlow;
use std::sync::RwLock;

/// As tabelas que os mods registraram (`register`), uma por mod.
static TABLES: RwLock<Vec<&'static [Gear]>> = RwLock::new(Vec::new());

/// Luz cheia (céu e bloco 15), a dos LEDs: `RenderTypes.eyes` ignora o mapa de luz.
const FULL_BRIGHT: u32 = 240 | 240 << 8;
/// `LivingEntity.getAgeScale`: o bebê tem metade do tamanho.
const BABY_SCALE: f32 = 0.5;
/// `TextureTransform.GLINT_TEXTURING`, a escala do brilho de item (`RenderTypes.itemCutoutGlint`).
const ITEM_GLINT_SCALE: f32 = 8.0;
/// O `layer` do brilho de item em `entity.wgsl` (o que `WorldSink` dá a um quad de item com brilho).
const ITEM_GLINT_OVERLAY: u32 = 7;
/// O braço direito e o esquerdo do `PlayerModel`, de pé: a posição da parte em pixels (`PartPose.offset`), se o modelo do braço faltar.
const RIGHT_ARM_REST: [f32; 3] = [-5.0, 2.0, 0.0];
const LEFT_ARM_REST: [f32; 3] = [5.0, 2.0, 0.0];

/// O mod entrega a tabela dos seus itens: os modelos avulsos dela entram na lista que o jogo assa (`native_standalone_models`), e as partes do corpo que cada item esconde
/// vão para a pose do mob (`anim::setup`). Pode ser chamada de novo: a mesma tabela não entra duas vezes.
pub(in crate::game) fn register(table: &'static [Gear], models: &mut Vec<String>) {
    for gear in table {
        for attachment in gear.attachments {
            if !models.iter().any(|model| model == attachment.model) {
                models.push(attachment.model.to_owned());
            }
        }
        if !gear.hides.is_empty() {
            horizon_client::mobs::anim::machinery_basis_worn::hide_when_worn(gear.slot.index(), gear.item, gear.hides);
        }
    }
    let mut tables = TABLES.write().unwrap_or_else(|error| error.into_inner());
    let first = table.first().map(|gear| gear.item);
    if !tables.iter().any(|known| known.first().map(|gear| gear.item) == first) {
        tables.push(table);
    }
}

/// O que o mob veste em cabeça, peito, pernas e pés (`extra.head` a `extra.feet`; o jogador da janela também, o jogo os põe a cada tick).
fn worn_of(mob: &Mob) -> Worn<'_> {
    [mob.extra.head.as_deref(), mob.extra.chest.as_deref(), mob.extra.legs.as_deref(), mob.extra.feet.as_deref()]
}

/// `stack.hasFoil()`: o brilho do item vestido (encantado, ou o brilho do próprio item, ou a troca do componente `enchantment_glint_override`).
fn has_foil(game: &Game<'_>, stack: Option<&ItemStack>) -> bool {
    stack.is_some_and(|stack| {
        let item_glint = game.res.items().enchantments.props.get(&*stack.item).is_some_and(|props| props.glint);
        stack.has_foil(item_glint)
    })
}

/// O brilho do que o mob veste, na ordem de [`Worn`]: a pilha vestida (`extra.armor`, que o jogador da janela, os outros jogadores e `item replace` preenchem) ou, sem ela,
/// os encantamentos que o mob guarda por slot (`gear::enchanted`, os mobs que nascem com equipamento).
fn foil_of_mob(game: &Game<'_>, mob: &Mob) -> [bool; 4] {
    use horizon_client::mobs::gear;
    let slots = [gear::HEAD, gear::CHEST, gear::LEGS, gear::FEET];
    std::array::from_fn(|i| match &mob.extra.armor[i] {
        Some(stack) => has_foil(game, Some(stack)),
        None => gear::enchanted(mob, slots[i]),
    })
}

fn id_of(stack: &Option<ItemStack>) -> Option<&str> {
    stack.as_ref().map(|stack| &*stack.item)
}

/// Os modelos do equipamento vestido por um mob (um jogador, um zumbi, qualquer um de modelo humanoide): para cada anexo, a pose da parte, os passos do anexo e o modelo avulso.
/// Chamado onde o jogo desenha o que o mob veste na cabeça (`CustomHeadLayer`), que é o ponto por onde passam o jogador da janela, os outros jogadores, os mobs e o jogador
/// da tela de inventário; `light` é a luz do mob.
pub(in crate::game) fn mob_gear(game: &Game<'_>, mob: &Mob, camera: DVec3, alpha: f64, light: u32, out: &mut Vec<Vertex>) {
    let worn = worn_of(mob);
    if worn.iter().all(Option::is_none) {
        return;
    }
    let tables = TABLES.read().unwrap_or_else(|error| error.into_inner());
    let foil = foil_of_mob(game, mob);
    // `isBaby = state.isBaby && state.entityType != EntityTypes.ARMOR_STAND`: um armor stand pequeno não é um bebê.
    let baby = mob.is_baby();
    for table in tables.iter() {
        let attachments = worn::visible_pieces(table, &worn);
        if attachments.is_empty() {
            continue;
        }
        // Cada parte do corpo, uma vez só: o mob monta o corpo todo a cada pedido.
        let mut poses: Vec<(&str, Option<Mat4>)> = Vec::new();
        for (gear, attachment) in attachments {
            let pose = match poses.iter().find(|(part, _)| *part == attachment.part) {
                Some((_, pose)) => *pose,
                None => {
                    let pose = render::part_transform(mob, alpha as f32, camera, game.res.mob_data(), attachment.part);
                    poses.push((attachment.part, pose));
                    pose
                }
            };
            if let Some(pose) = pose {
                draw(game, attachment, worn::foil_of(gear, &foil), pose, baby, light, out);
            }
        }
    }
}

/// `ArmorStandRenderer`: o armor stand na pose dele. O armor stand nunca é bebê, e as partes do modelo do armor stand que o traje esconderia ficam (são varetas que cabem
/// dentro das peças do traje).
pub(in crate::game) fn stand_gear(game: &Game<'_>, stand: &ArmorStand, base: Mat4, light: u32, out: &mut Vec<Vertex>) {
    let worn: Worn<'_> = [id_of(&stand.equipment[5]), id_of(&stand.equipment[4]), id_of(&stand.equipment[3]), id_of(&stand.equipment[2])];
    if worn.iter().all(Option::is_none) {
        return;
    }
    // O modelo de armadura do armor stand (`ArmorStandArmorModel`): a pose das partes é a do armor stand.
    let Some(layer) = game.armor_stand_assets.layers.get(&("chestplate", stand.small)) else {
        return;
    };
    let mut poses = Poses::new(layer);
    apply_stand_pose(&mut poses, &stand.pose);
    let tables = TABLES.read().unwrap_or_else(|error| error.into_inner());
    let foil: [bool; 4] = std::array::from_fn(|i| has_foil(game, stand.equipment[5 - i].as_ref()));
    for table in tables.iter() {
        for (gear, attachment) in worn::visible_pieces(table, &worn) {
            if let Some(pose) = render::part_matrix(layer, base, &poses, attachment.part, 0.0) {
                draw(game, attachment, worn::foil_of(gear, &foil), pose, false, light, out);
            }
        }
    }
}

/// `ArmorStandArmorModel.setupAnim`: as rotações de `Pose` nas partes; o corpo vira as varetas também.
fn apply_stand_pose(poses: &mut Poses, pose: &[[f32; 3]; 6]) {
    let parts: [&[&str]; 6] = [&["head"], &["body", "right_body_stick", "left_body_stick", "shoulder_stick"], &["left_arm"], &["right_arm"], &["left_leg"], &["right_leg"]];
    for (names, rotation) in parts.into_iter().zip(pose) {
        for name in names {
            if poses.has(name) {
                let part = poses.get(name);
                part[3] = rotation[0].to_radians();
                part[4] = rotation[1].to_radians();
                part[5] = rotation[2].to_radians();
            }
        }
    }
}

/// O braço do jogador em primeira pessoa (`RenderArmEvent`): se o equipamento vestido esconde o braço, o jogo não o desenha e o equipamento desenha o dele, na pose do braço.
/// `m` é a pose do `renderPlayerArm`; o braço sem rotação (`arm.resetPose()`), na posição de descanso da parte.
pub(in crate::game) fn first_person_arm(game: &Game<'_>, left: bool, m: Mat4, light: u32, out: &mut Vec<Vertex>) -> ControlFlow<()> {
    let worn: Worn<'_> = [id_of(&game.armor[0]), id_of(&game.armor[1]), id_of(&game.armor[2]), id_of(&game.armor[3])];
    if worn.iter().all(Option::is_none) {
        return ControlFlow::Continue(());
    }
    let part = if left { "left_arm" } else { "right_arm" };
    let foil: [bool; 4] = std::array::from_fn(|i| has_foil(game, game.armor[i].as_ref()));
    let tables = TABLES.read().unwrap_or_else(|error| error.into_inner());
    let mut replaced = false;
    for table in tables.iter() {
        if !worn::hidden_parts(table, &worn).contains(&part) {
            continue;
        }
        replaced = true;
        let slim = game.player_model.as_ref().is_some_and(|model| model.slim);
        let rest = game.res.player_arm(slim, left).as_ref().map_or(if left { LEFT_ARM_REST } else { RIGHT_ARM_REST }, |arm: &Part| [arm.pose[0], arm.pose[1], arm.pose[2]]);
        let pose = m * Mat4::from_translation(Vec3::from_array(rest) / 16.0);
        for (gear, attachment) in worn::visible_pieces(table, &worn).into_iter().filter(|(_, attachment)| attachment.part == part) {
            draw(game, attachment, worn::foil_of(gear, &foil), pose, false, light, out);
        }
    }
    if replaced { ControlFlow::Break(()) } else { ControlFlow::Continue(()) }
}

/// A capa do jogador some com o equipamento que a esconde (`showCape = false`).
pub(in crate::game) fn cape(mob: &Mob) -> ControlFlow<()> {
    let worn = worn_of(mob);
    if worn.iter().all(Option::is_none) {
        return ControlFlow::Continue(());
    }
    let tables = TABLES.read().unwrap_or_else(|error| error.into_inner());
    if tables.iter().any(|table| worn::hides_cape(table, &worn)) { ControlFlow::Break(()) } else { ControlFlow::Continue(()) }
}

/// Um modelo avulso na pose `pose` (a da parte do corpo, no espaço do modelo do mob), com os passos do anexo. Os LEDs saem sem luz do mundo e sem sombra de face. Com `foil`,
/// o modelo leva o brilho de encantamento do original, que sempre amostra a textura do brilho com a UV0 do quad no atlas de itens (a textura do modelo avulso).
fn draw(game: &Game<'_>, attachment: &Attachment, foil: Option<Foil>, pose: Mat4, baby: bool, light: u32, out: &mut Vec<Vertex>) {
    let Some(model) = game.pack.render.model_catalog.standalone(attachment.model) else {
        return;
    };
    let ops = if baby { attachment.baby_ops } else { attachment.ops };
    let matrix = pose * Mat4::from_cols_array(&worn::compose(ops, BABY_SCALE));
    let start = out.len();
    game.baked_model_vertices([model], matrix, |_| light, true, |_| None, out);
    if attachment.unlit {
        for vertex in &mut out[start..] {
            vertex.light = FULL_BRIGHT;
            vertex.normal = [0; 4];
            vertex.color = 0xFFFF_FFFF;
        }
    }
    match foil {
        // `armorCutoutNoCullGlint`: o shader embaralha a UV do modelo (`ARMOR_ENTITY_GLINT_TEXTURING`, escala 0,16), então o vértice só leva a UV0.
        Some(Foil::Armor) => {
            for vertex in &mut out[start..] {
                vertex.layer = ARMOR_GLINT;
                vertex.glint_uv = vertex.uv;
            }
        }
        // `itemCutoutGlint`: `GLINT_TEXTURING` (escala 8) sobre a UV0, calculada aqui como o `WorldSink` faz.
        Some(Foil::Item) => {
            if let Some(glint) = game.glint_frame.glint(ITEM_GLINT_SCALE) {
                for vertex in &mut out[start..] {
                    vertex.layer = ITEM_GLINT_OVERLAY;
                    vertex.glint_uv = glint.uv(vertex.uv);
                }
            }
        }
        None => {}
    }
}
