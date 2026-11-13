//! As arestas do contorno de uma forma que é a união de caixas (`VoxelShape.forAllEdges`, o que o contorno do bloco desenha): a forma de um braço `PUSH` ou `PULL` dos
//! transmissores do Mekanism são três caixas empilhadas de larguras diferentes (`BlockSmallTransmitter.SIDES_PUSH`), e o contorno dela é o das arestas da união, com os degraus.
//!
//! O original (`DiscreteVoxelShape.forAllEdges(consumer, true)`) olha, para cada reta de vértices de uma grade de células, as quatro células em volta: a reta é aresta quando 1 ou 3
//! delas estão cheias, ou 2 em diagonal (`fullCount == 2` com `oddCount` par: as duas células ficam em cantos opostos), e os trechos seguidos de uma reta viram um segmento só. A
//! grade é a das coordenadas das caixas (`Shapes.or` junta as listas de coordenadas, e a `optimize` tira os planos repetidos, que não mudam as arestas: um plano que nada separa não
//! corta o segmento que o atravessa). Os três eixos entram na ordem do original (medida no rastro: o segmento anda em z, depois em y, depois em x).

/// Uma aresta: `[x0, y0, z0, x1, y1, z1]`, em coordenadas do bloco.
pub type Edge = [f64; 6];

/// As coordenadas distintas dos limites das caixas num eixo, em ordem crescente (a grade de `Shapes.or`).
fn coordinates(boxes: &[[f64; 6]], axis: usize) -> Vec<f64> {
    let mut values: Vec<f64> = boxes
        .iter()
        .flat_map(|bounds| [bounds[axis], bounds[axis + 3]])
        .collect();
    values.sort_by(f64::total_cmp);
    values.dedup();
    values
}

/// As arestas da união das `boxes`, na ordem de `VoxelShape.forAllEdges`. Sem caixas, nenhuma; uma caixa só dá as 12 arestas dela.
pub fn union_edges(boxes: &[[f64; 6]]) -> Vec<Edge> {
    if boxes.is_empty() {
        return Vec::new();
    }
    let grid = [
        coordinates(boxes, 0),
        coordinates(boxes, 1),
        coordinates(boxes, 2),
    ];
    let sizes = grid.clone().map(|values| values.len() - 1);
    // A célula `(i, j, k)` vai de `grid[0][i]` a `grid[0][i + 1]` e assim por diante: cheia se alguma caixa a cobre inteira. Fora da grade, vazia (`isFullWide`).
    let full = |cell: [i64; 3]| -> bool {
        if (0..3).any(|axis| cell[axis] < 0 || cell[axis] >= sizes[axis] as i64) {
            return false;
        }
        let at = cell.map(|index| index as usize);
        boxes.iter().any(|bounds| {
            (0..3).all(|axis| {
                bounds[axis] <= grid[axis][at[axis]] && grid[axis][at[axis] + 1] <= bounds[axis + 3]
            })
        })
    };
    let mut edges = Vec::new();
    // O segmento anda em z, depois em y, depois em x (a ordem do rastro do original); os dois eixos de fora, `a` e `b`, são os dois seguintes em ordem cíclica.
    for axis in [2usize, 1, 0] {
        let a = (axis + 1) % 3;
        let b = (axis + 2) % 3;
        for u in 0..=sizes[a] {
            for v in 0..=sizes[b] {
                let mut start: Option<usize> = None;
                for t in 0..=sizes[axis] {
                    let mut count = 0;
                    let mut odd = 0;
                    for du in 0..=1i64 {
                        for dv in 0..=1i64 {
                            let mut cell = [0i64; 3];
                            cell[axis] = t as i64;
                            cell[a] = u as i64 + du - 1;
                            cell[b] = v as i64 + dv - 1;
                            if full(cell) {
                                count += 1;
                                odd ^= du ^ dv;
                            }
                        }
                    }
                    let edge = count == 1 || count == 3 || (count == 2 && odd == 0);
                    if edge {
                        start.get_or_insert(t);
                    } else if let Some(first) = start.take() {
                        let mut from = [0.0; 3];
                        from[a] = grid[a][u];
                        from[b] = grid[b][v];
                        let mut to = from;
                        from[axis] = grid[axis][first];
                        to[axis] = grid[axis][t];
                        edges.push([from[0], from[1], from[2], to[0], to[1], to[2]]);
                    }
                }
            }
        }
    }
    edges
}

/// As arestas da união das `boxes` como o laço do jogo as desenha no contorno de uma caixa `bounds`: cada extremidade em fração da caixa (`posição = bounds.mín + fração *
/// (bounds.máx - bounds.mín)`, como o contorno de uma forma de contexto, a caixa única do `context_outline`, põe as arestas do cubo unitário). `bounds` tem de conter as caixas.
pub fn union_edges_in(boxes: &[[f64; 6]], bounds: &[f64; 6]) -> Vec<Edge> {
    union_edges(boxes)
        .into_iter()
        .map(|edge| {
            std::array::from_fn(|index| {
                let axis = index % 3;
                (edge[index] - bounds[axis]) / (bounds[axis + 3] - bounds[axis])
            })
        })
        .collect()
}
