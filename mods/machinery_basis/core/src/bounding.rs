//! O bloco de apoio (`BlockBounding` e `TileEntityBoundingBlock` do Mekanism): um bloco invisível que ocupa o espaço de um bloco maior que uma célula e passa quase
//! tudo ao principal. O tile guarda o deslocamento até o principal (`mainPosOffset`, `mainPos - pos`). Aqui ficam só as contas; quem é o apoio e quem é o principal
//! vem do mod.

/// `TileEntityBoundingBlock.getMainPos`: `worldPosition.offset(mainPosOffset)`.
pub fn main_pos(pos: [i32; 3], offset: [i32; 3]) -> [i32; 3] {
    std::array::from_fn(|axis| pos[axis] + offset[axis])
}

/// Os principais possíveis de um apoio em `pos`, um por lugar de apoio que o principal põe (`AttributeHasBounding`: `mainPos + lugar`). Serve onde o tile do apoio
/// já saiu do mundo e o deslocamento não se lê mais.
pub fn main_candidates(pos: [i32; 3], places: &[[i32; 3]]) -> Vec<[i32; 3]> {
    places
        .iter()
        .map(|place| std::array::from_fn(|axis| pos[axis] - place[axis]))
        .collect()
}

/// `BlockBounding.proxyShape`: a forma do principal (caixas `[x0, y0, z0, x1, y1, z1]` no bloco dele) vista do apoio, `shape.move(mainPos.subtract(pos))`.
pub fn proxy_shape(main_shape: &[[f64; 6]], pos: [i32; 3], main: [i32; 3]) -> Vec<[f64; 6]> {
    main_shape
        .iter()
        .map(|part| {
            std::array::from_fn(|index| part[index] + f64::from(main[index % 3] - pos[index % 3]))
        })
        .collect()
}

/// A menor caixa que contém as `boxes` (`VoxelShape.bounds`); sem caixas, nada.
pub fn bounds(boxes: &[[f64; 6]]) -> Option<[f64; 6]> {
    let first = *boxes.first()?;
    Some(boxes.iter().fold(first, |all, part| {
        std::array::from_fn(|index| {
            if index < 3 {
                all[index].min(part[index])
            } else {
                all[index].max(part[index])
            }
        })
    }))
}

/// As arestas de uma forma (`[x0, y0, z0, x1, y1, z1]` no mesmo espaço de `bounds`) em fração de `bounds`, como o contorno de uma forma de contexto as desenha
/// (`posição = bounds.mín + fração * (bounds.máx - bounds.mín)`). `bounds` não pode ser plana em nenhum eixo.
pub fn edges_in(edges: &[[f64; 6]], bounds: &[f64; 6]) -> Vec<[f64; 6]> {
    edges
        .iter()
        .map(|edge| {
            std::array::from_fn(|index| {
                let axis = index % 3;
                (edge[index] - bounds[axis]) / (bounds[axis + 3] - bounds[axis])
            })
        })
        .collect()
}

/// `BlockBounding.withHitResultForMain`: o clique no apoio vira um clique no principal, no meio da face clicada (`Vec3.atCenterOf(mainPos).relative(direction,
/// 0.5)`), com a mesma face.
pub fn main_hit_point(main: [i32; 3], face: [i32; 3]) -> [f64; 3] {
    std::array::from_fn(|axis| f64::from(main[axis]) + 0.5 + f64::from(face[axis]) * 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Três das nove caixas de `BlockShapes.SECURITY_DESK` voltada para o norte (as do rastro `blocos_formas.jsonl`): a base, o tampo e o monitor, que passa de
    /// `y = 1`.
    const DESK: [[f64; 6]; 3] = [
        [0.0, 0.0, 0.0, 1.0, 0.3125, 1.0],
        [0.0625, 0.0, 0.0625, 0.9375, 0.8125, 0.9375],
        [0.0625, 0.875, 0.6875, 0.9375, 1.5, 0.8125],
    ];

    #[test]
    fn the_main_is_the_position_plus_the_saved_offset() {
        // `AttributeHasBounding.ABOVE_ONLY` põe o apoio em `pos.above()` com `setMainLocationOffset(mainPos.subtract(boundingPos))`: (0, -1, 0).
        assert_eq!(main_pos([10, 65, -3], [0, -1, 0]), [10, 64, -3]);
        assert_eq!(
            main_candidates([10, 65, -3], &[[0, 1, 0]]),
            vec![[10, 64, -3]]
        );
        assert_eq!(
            main_candidates([1, 1, 1], &[[1, 0, 0], [0, 1, -1]]),
            vec![[0, 1, 1], [1, 0, 2]]
        );
    }

    #[test]
    fn the_shape_of_the_bounding_block_is_the_shape_of_the_main_moved() {
        // O apoio em cima do principal: tudo desce um bloco, e o que sobra dentro da célula do apoio é o monitor, de -0,125 a 0,5.
        let moved = proxy_shape(&DESK, [10, 65, -3], [10, 64, -3]);
        assert_eq!(moved[0], [0.0, -1.0, 0.0, 1.0, -0.6875, 1.0]);
        assert_eq!(moved[2], [0.0625, -0.125, 0.6875, 0.9375, 0.5, 0.8125]);
        // Um apoio ao lado e abaixo (o Digital Miner tem apoios nos três eixos).
        let side = proxy_shape(&DESK[..1], [3, 9, 7], [4, 10, 5]);
        assert_eq!(side[0], [1.0, 1.0, -2.0, 2.0, 1.3125, -1.0]);
        assert_eq!(bounds(&moved), Some([0.0, -1.0, 0.0, 1.0, 0.5, 1.0]));
        assert_eq!(bounds(&[]), None);
    }

    #[test]
    fn the_edges_are_fractions_of_the_bounds() {
        let bounds = [0.0, -1.0, 0.0, 1.0, 0.5, 1.0];
        let edges = edges_in(
            &[
                [0.0, -1.0, 0.0, 1.0, -1.0, 0.0],
                [0.5, -0.25, 0.25, 0.5, 0.5, 0.25],
            ],
            &bounds,
        );
        assert_eq!(edges[0], [0.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        assert_eq!(edges[1], [0.5, 0.5, 0.25, 0.5, 1.0, 0.25]);
    }

    #[test]
    fn a_click_on_the_bounding_block_lands_on_the_middle_of_the_same_face_of_the_main() {
        // `Vec3.atCenterOf((10, 64, -3))` é (10,5; 64,5; -2,5); `relative(UP, 0.5)` soma meio bloco em y.
        assert_eq!(main_hit_point([10, 64, -3], [0, 1, 0]), [10.5, 65.0, -2.5]);
        assert_eq!(main_hit_point([10, 64, -3], [-1, 0, 0]), [10.0, 64.5, -2.5]);
        assert_eq!(main_hit_point([10, 64, -3], [0, 0, 1]), [10.5, 64.5, -2.0]);
    }
}
