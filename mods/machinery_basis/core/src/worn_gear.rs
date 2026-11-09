//! Equipamento que o corpo veste com modelo próprio (um traje de peças em 3D, uma mochila, botas com molas): o Horizon só desenha pelas camadas do vanilla (o modelo de
//! armadura de cubos do `equipment asset`), então o mod diz, por item, quais modelos vão em quais partes do corpo e com que transformações, e o jogo os desenha na pose
//! da parte (a de quem anda, agacha, nada e voa). É o que o `HumanoidArmorLayer` do original faz com o `ICustomArmor`: `part.translateAndRotate`, a transformação da peça
//! e o modelo do item. Tudo aqui é dado e conta pura; quem desenha é `worn_gear` em `glue`.
//!
//! O que a lib não conhece: nenhum id, modelo ou peça de um mod. Eles entram como uma tabela de [`Gear`].

/// Uma peça de armadura do corpo, na ordem de `EquipmentSlot` do original (`ordinal`: pés 2, pernas 3, peito 4, cabeça 5), que as regras de vizinhança leem.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Slot {
    Feet,
    Legs,
    Chest,
    Head,
}

impl Slot {
    /// Cabeça, peito, pernas e pés: a ordem dos quatro slots de armadura do jogo (`Worn`).
    pub const ALL: [Slot; 4] = [Slot::Head, Slot::Chest, Slot::Legs, Slot::Feet];

    /// A posição do slot em [`Worn`].
    pub const fn index(self) -> usize {
        match self {
            Slot::Head => 0,
            Slot::Chest => 1,
            Slot::Legs => 2,
            Slot::Feet => 3,
        }
    }

    /// `EquipmentSlot.ordinal()`.
    pub const fn ordinal(self) -> u8 {
        match self {
            Slot::Feet => 2,
            Slot::Legs => 3,
            Slot::Chest => 4,
            Slot::Head => 5,
        }
    }
}

/// O que cada slot de armadura veste, por id de item (cabeça, peito, pernas, pés).
pub type Worn<'a> = [Option<&'a str>; 4];

/// Um passo da transformação de uma peça, aplicado como o `PoseStack` aplica: cada passo se soma à direita do anterior.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    /// `poseStack.translate`, em blocos.
    Translate([f32; 3]),
    /// `poseStack.scale`.
    Scale([f32; 3]),
    /// `poseStack.scale(state.ageScale, state.ageScale, state.ageScale)`: a escala do bebê, que quem desenha diz.
    AgeScale,
}

/// Um modelo preso a uma parte do corpo.
#[derive(Clone, Copy, Debug)]
pub struct Attachment {
    /// A parte do modelo humanoide (`head`, `body`, `left_arm`, `right_arm`, `left_leg`, `right_leg`) de que a pose vem.
    pub part: &'static str,
    /// O modelo avulso (`mymod:item/jetpack`), que o mod registra em `standalone_models`.
    pub model: &'static str,
    /// Do centro da parte até o modelo, de um adulto.
    pub ops: &'static [Op],
    /// O mesmo, de um bebê (`isBaby` e não um armor stand).
    pub baby_ops: &'static [Op],
    /// Sem luz do mundo e sem sombra de face: o modelo brilha por inteiro (`RenderTypes.eyes`, os LEDs).
    pub unlit: bool,
    /// A peça some quando este slot veste uma peça da mesma família (o `excl_` e o `shared_` do MekaSuit).
    pub hidden_if_worn: Option<Slot>,
    /// Só se desenha com a parte visível (`ChestArmor.isVisible`: `body.visible`; as botas: `leg.visible`). O MekaSuit esconde as partes do corpo e desenha mesmo assim.
    pub needs_visible_part: bool,
}

/// Como o item vestido recebe o brilho de encantamento (`stack.hasFoil()`): é o tipo de desenho do original que decide a textura do brilho e a escala dela.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Foil {
    /// O brilho da armadura: `RenderTypes.armorCutoutNoCullGlint`, a textura `enchanted_glint_armor` com `ARMOR_ENTITY_GLINT_TEXTURING` (escala 0,16). É o do MekaSuit
    /// (`MekaSuitArmor.PartData.render`: `BASE_GLINT`, `MEKASUIT_GLINT`, `ARMOR_TRANSLUCENT_GLINT`).
    Armor,
    /// O brilho do item: `Sheets.cutoutItemGlintSheet` (`RenderTypes.itemCutoutGlint`), a textura `enchanted_glint_item` com `GLINT_TEXTURING` (escala 8). É o do jetpack, do
    /// tanque (o `ItemStackRenderState` do item) e das botas (`BaseModelCache.submitModel`).
    Item,
}

/// Um item vestido que tem modelo próprio.
#[derive(Clone, Copy, Debug)]
pub struct Gear {
    pub item: &'static str,
    /// Quem conta como vizinho em `hidden_if_worn`: as peças do mesmo traje.
    pub family: &'static str,
    pub slot: Slot,
    /// As partes do corpo do vanilla que somem enquanto o item é vestido (`overrideModelPartVisibility`).
    pub hides: &'static [&'static str],
    /// A capa do jogador some (`showCape = false`).
    pub hides_cape: bool,
    /// O brilho de encantamento das peças do item.
    pub foil: Foil,
    pub attachments: &'static [Attachment],
}

/// O que um jogo vê de um item vestido no slot `slot`: o item da tabela, se o id é de um e o slot é o dele.
pub fn gear_of<'a>(table: &'a [Gear], item: &str, slot: Slot) -> Option<&'a Gear> {
    table
        .iter()
        .find(|gear| gear.slot == slot && gear.item == item)
}

/// Os itens da tabela que o corpo veste, na ordem de `Slot::ALL`.
pub fn worn_gear<'a>(table: &'a [Gear], worn: &Worn<'_>) -> Vec<&'a Gear> {
    Slot::ALL
        .into_iter()
        .filter_map(|slot| worn[slot.index()].and_then(|item| gear_of(table, item, slot)))
        .collect()
}

/// As partes do vanilla que os itens vestidos escondem.
pub fn hidden_parts(table: &[Gear], worn: &Worn<'_>) -> Vec<&'static str> {
    let mut hidden = Vec::new();
    for gear in worn_gear(table, worn) {
        for part in gear.hides {
            if !hidden.contains(part) {
                hidden.push(*part);
            }
        }
    }
    hidden
}

/// Se algum item vestido esconde a capa.
pub fn hides_cape(table: &[Gear], worn: &Worn<'_>) -> bool {
    worn_gear(table, worn).iter().any(|gear| gear.hides_cape)
}

/// Os modelos a desenhar: cada anexo dos itens vestidos que aparece com o que o corpo veste (`hidden_if_worn`) e com as partes que sobram (`needs_visible_part`).
pub fn visible_attachments<'a>(table: &'a [Gear], worn: &Worn<'_>) -> Vec<&'a Attachment> {
    visible_pieces(table, worn)
        .into_iter()
        .map(|(_, attachment)| attachment)
        .collect()
}

/// [`visible_attachments`] com o item a que cada anexo pertence (o slot e o brilho dele).
pub fn visible_pieces<'a>(table: &'a [Gear], worn: &Worn<'_>) -> Vec<(&'a Gear, &'a Attachment)> {
    let gear = worn_gear(table, worn);
    let hidden: Vec<&str> = gear.iter().flat_map(|g| g.hides.iter().copied()).collect();
    let mut out = Vec::new();
    for item in &gear {
        for attachment in item.attachments {
            if let Some(neighbour) = attachment.hidden_if_worn
                && worn[neighbour.index()].is_some_and(|id| {
                    gear_of(table, id, neighbour).is_some_and(|other| other.family == item.family)
                })
            {
                continue;
            }
            if attachment.needs_visible_part && hidden.contains(&attachment.part) {
                continue;
            }
            out.push((*item, attachment));
        }
    }
    out
}

/// O brilho de encantamento de uma peça do item `gear`: o do item, se o que o corpo veste no slot dele tem brilho (`foil[slot]`, na ordem de [`Worn`]: `stack.hasFoil()`).
/// Cada peça de um item encantado brilha, a opaca, a emissiva e a translúcida (`PartData.render`), e a bota também (`FreeRunnerArmor.render`).
pub fn foil_of(gear: &Gear, foil: &[bool; 4]) -> Option<Foil> {
    foil[gear.slot.index()].then_some(gear.foil)
}

/// Os passos de uma peça compostos numa matriz 4x4 em colunas (a ordem de `glam::Mat4::from_cols_array`): a pose parte da identidade e cada passo se soma à direita,
/// como o `PoseStack`.
pub fn compose(ops: &[Op], age_scale: f32) -> [f32; 16] {
    let mut matrix = IDENTITY;
    for op in ops {
        let step = match *op {
            Op::Translate(by) => translation(by),
            Op::Scale(by) => scaling(by),
            Op::AgeScale => scaling([age_scale; 3]),
        };
        matrix = multiply(&matrix, &step);
    }
    matrix
}

const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

fn translation(by: [f32; 3]) -> [f32; 16] {
    let mut matrix = IDENTITY;
    matrix[12..15].copy_from_slice(&by);
    matrix
}

fn scaling(by: [f32; 3]) -> [f32; 16] {
    let mut matrix = IDENTITY;
    matrix[0] = by[0];
    matrix[5] = by[1];
    matrix[10] = by[2];
    matrix
}

/// `a * b`, em colunas.
fn multiply(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut out = [0.0; 16];
    for column in 0..4 {
        for row in 0..4 {
            out[column * 4 + row] = (0..4).map(|k| a[k * 4 + row] * b[column * 4 + k]).sum();
        }
    }
    out
}

/// Aplica a matriz a um ponto.
pub fn transform_point(matrix: &[f32; 16], point: [f32; 3]) -> [f32; 3] {
    let [x, y, z] = point;
    std::array::from_fn(|row| {
        matrix[row] * x + matrix[4 + row] * y + matrix[8 + row] * z + matrix[12 + row]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: Attachment = Attachment {
        part: "body",
        model: "t:pack",
        ops: &[Op::Translate([0.0, 0.3, 0.0]), Op::Scale([1.0, -1.0, -1.0])],
        baby_ops: &[Op::AgeScale, Op::Translate([0.0, -0.05, 0.0])],
        unlit: false,
        hidden_if_worn: None,
        needs_visible_part: true,
    };
    const LEG: Attachment = Attachment {
        part: "left_leg",
        model: "t:boot",
        ops: &[],
        baby_ops: &[],
        unlit: false,
        hidden_if_worn: Some(Slot::Legs),
        needs_visible_part: true,
    };
    const PACK: Gear = Gear {
        item: "t:pack",
        family: "pack",
        slot: Slot::Chest,
        hides: &["body"],
        hides_cape: true,
        foil: Foil::Item,
        attachments: &[BODY],
    };
    const BOOT: Gear = Gear {
        item: "t:boot",
        family: "suit",
        slot: Slot::Feet,
        hides: &[],
        hides_cape: false,
        foil: Foil::Armor,
        attachments: &[LEG],
    };
    const PANTS: Gear = Gear {
        item: "t:pants",
        family: "suit",
        slot: Slot::Legs,
        hides: &["left_leg", "right_leg"],
        hides_cape: false,
        foil: Foil::Armor,
        attachments: &[],
    };
    const TABLE: [Gear; 3] = [PACK, BOOT, PANTS];

    #[test]
    fn the_slots_follow_the_equipment_slot_order_of_the_original() {
        let ordinals: Vec<u8> = [Slot::Feet, Slot::Legs, Slot::Chest, Slot::Head]
            .iter()
            .map(|s| s.ordinal())
            .collect();
        assert_eq!(ordinals, [2, 3, 4, 5]);
        for (i, slot) in Slot::ALL.into_iter().enumerate() {
            assert_eq!(slot.index(), i);
        }
    }

    #[test]
    fn an_item_counts_only_in_its_own_slot() {
        assert!(gear_of(&TABLE, "t:pack", Slot::Chest).is_some());
        assert!(gear_of(&TABLE, "t:pack", Slot::Head).is_none());
        assert!(gear_of(&TABLE, "t:other", Slot::Chest).is_none());
    }

    #[test]
    fn worn_items_hide_their_parts_and_the_cape() {
        let worn = [None, Some("t:pack"), Some("t:pants"), None];
        assert_eq!(
            hidden_parts(&TABLE, &worn),
            ["body", "left_leg", "right_leg"]
        );
        assert!(hides_cape(&TABLE, &worn));
        let bare = [None; 4];
        assert!(hidden_parts(&TABLE, &bare).is_empty());
        assert!(!hides_cape(&TABLE, &bare));
    }

    #[test]
    fn an_attachment_needs_its_part_to_show() {
        // A mochila pede o corpo visível: ela mesma o esconde (`hides`), então não se desenha (a regra do `body.visible` do original).
        let worn = [None, Some("t:pack"), None, None];
        assert!(visible_attachments(&TABLE, &worn).is_empty());
        let worn = [None, None, None, Some("t:boot")];
        assert_eq!(visible_attachments(&TABLE, &worn).len(), 1);
        // As calças escondem a perna: a bota, que pede a perna visível, não se desenha.
        let worn = [None, None, Some("t:pants"), Some("t:boot")];
        assert!(visible_attachments(&TABLE, &worn).is_empty());
    }

    #[test]
    fn a_neighbour_of_the_same_family_hides_an_attachment() {
        const SHIN: Attachment = Attachment {
            needs_visible_part: false,
            ..LEG
        };
        const SUIT_BOOT: Gear = Gear {
            attachments: &[SHIN],
            ..BOOT
        };
        let table = [SUIT_BOOT, PANTS, PACK];
        // As pernas vestidas da mesma família escondem a peça; o item de outra família não.
        let worn = [None, None, Some("t:pants"), Some("t:boot")];
        assert!(visible_attachments(&table, &worn).is_empty());
        let worn = [None, Some("t:pack"), None, Some("t:boot")];
        assert_eq!(visible_attachments(&table, &worn).len(), 1);
        let worn = [None, None, None, Some("t:boot")];
        assert_eq!(visible_attachments(&table, &worn).len(), 1);
        // Calças de outra família no slot vizinho não contam como vizinhas: a peça aparece.
        const OTHER_PANTS: Gear = Gear {
            item: "t:other_pants",
            family: "other",
            ..PANTS
        };
        let table = [SUIT_BOOT, OTHER_PANTS];
        let worn = [None, None, Some("t:other_pants"), Some("t:boot")];
        assert_eq!(visible_attachments(&table, &worn).len(), 1);
    }

    #[test]
    fn a_piece_glints_with_the_style_of_its_item_when_its_slot_is_enchanted() {
        // A bota (a única peça que aparece: o corpo da mochila se esconde a si mesmo) sabe de que item é.
        let worn = [None, Some("t:pack"), None, Some("t:boot")];
        let pieces = visible_pieces(&TABLE, &worn);
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].0.item, "t:boot");
        // O peito encantado não faz a bota brilhar: o brilho é do slot do item.
        assert_eq!(foil_of(pieces[0].0, &[false, true, false, false]), None);
        assert_eq!(
            foil_of(pieces[0].0, &[false, false, false, true]),
            Some(Foil::Armor)
        );
        let all = [true; 4];
        assert_eq!(foil_of(&PACK, &all), Some(Foil::Item));
        assert_eq!(foil_of(&PACK, &[false; 4]), None);
        assert_eq!(
            foil_of(&PACK, &[false, true, false, false]),
            Some(Foil::Item)
        );
    }

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6)
    }

    #[test]
    fn the_steps_compose_as_a_pose_stack_does() {
        // translate(0, 0.3, 0) depois scale(1, -1, -1): o ponto (1, 2, 3) escala primeiro (1, -2, -3) e depois sobe 0,3.
        let matrix = compose(BODY.ops, 1.0);
        assert!(close(
            transform_point(&matrix, [1.0, 2.0, 3.0]),
            [1.0, -1.7, -3.0]
        ));
        // A escala do bebê vem primeiro na pilha, então escala o que vem depois dela, a translação inclusive: o ponto (2, 2, 2) desce 0,05 e depois escala por 0,5.
        let baby = compose(BODY.baby_ops, 0.5);
        assert!(close(
            transform_point(&baby, [2.0, 2.0, 2.0]),
            [1.0, 0.975, 1.0]
        ));
        let scaled_translation = compose(&[Op::AgeScale, Op::Translate([0.0, 1.0, 0.0])], 0.5);
        assert!(close(
            transform_point(&scaled_translation, [0.0, 0.0, 0.0]),
            [0.0, 0.5, 0.0]
        ));
        assert!(close(
            transform_point(&compose(&[], 1.0), [4.0, 5.0, 6.0]),
            [4.0, 5.0, 6.0]
        ));
    }
}
