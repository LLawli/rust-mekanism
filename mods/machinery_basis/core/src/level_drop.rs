//! O bloco com tile destruído pelo nível (`Level.destroyBlock(pos, true)`: `setblock ... destroy`, `fill ... destroy`, a água que leva o bloco).
//!
//! O original rola a tabela de loot com o tile (`Block.dropResources(state, level, pos, blockEntity, entity, tool)`, chamado por `Level.destroyBlock`), e o
//! `copy_components` dela leva para o item o que o tile guarda. No jogo, os itens que o bloco soltou esperam numa fila até o fim da operação; aqui fica a conta de
//! quais itens da fila são do bloco e de como eles voltam para ela depois de a tabela de loot dos mods falar.

/// O bloco em que um item nasceu. `Block.popResource` sorteia a posição dentro do bloco (o centro mais ou menos 0,25 em cada eixo), então o chão da posição é o bloco.
pub fn block_of(at: [f64; 3]) -> [i32; 3] {
    at.map(|c| c.floor() as i32)
}

/// Separa da fila os itens do bloco `block`: os que nasceram dentro dele e que `ours` reconhece (o item do próprio bloco). Devolve os do bloco e o resto, cada lista na
/// ordem da fila.
pub fn split<S>(
    spawns: Vec<S>,
    block: [i32; 3],
    at: impl Fn(&S) -> [f64; 3],
    ours: impl Fn(&S) -> bool,
) -> (Vec<S>, Vec<S>) {
    spawns
        .into_iter()
        .partition(|spawn| block_of(at(spawn)) == block && ours(spawn))
}

/// Os itens do bloco depois de a tabela de loot falar: cada pilha que ficou toma o lugar da que ela era, na ordem, e as que saíram somem com o lugar delas (como o
/// `retell_runs` do Horizon). Uma pilha a mais do que os lugares não tem onde nascer e não entra.
pub fn retell<S, T>(mut mine: Vec<S>, drops: Vec<T>, set: impl Fn(&mut S, T)) -> Vec<S> {
    mine.truncate(drops.len());
    for (spawn, stack) in mine.iter_mut().zip(drops) {
        set(spawn, stack);
    }
    mine
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_block_of_a_position_is_its_floor_also_below_zero() {
        assert_eq!(block_of([0.3, 64.9, 10.0]), [0, 64, 10]);
        // `Block.popResource` num bloco de coordenada negativa: -0,4 está no bloco -1, não no 0.
        assert_eq!(block_of([-0.4, -59.6, -3.75]), [-1, -60, -4]);
    }

    #[test]
    fn only_the_items_born_in_the_block_and_recognised_leave_the_queue() {
        let spawns = vec![
            ("tank", [5.4, -59.5, 3.6]),
            ("stone", [5.5, -59.5, 3.5]),
            ("tank", [6.4, -59.5, 3.6]),
            ("tank", [5.7, -59.3, 3.3]),
        ];
        let (mine, rest) = split(
            spawns,
            [5, -60, 3],
            |spawn| spawn.1,
            |spawn| spawn.0 == "tank",
        );
        assert_eq!(
            mine,
            vec![("tank", [5.4, -59.5, 3.6]), ("tank", [5.7, -59.3, 3.3])]
        );
        // O item de outro tipo dentro do bloco e o item igual do bloco vizinho ficam, na ordem em que estavam.
        assert_eq!(
            rest,
            vec![("stone", [5.5, -59.5, 3.5]), ("tank", [6.4, -59.5, 3.6])]
        );
    }

    #[test]
    fn each_kept_stack_takes_the_place_of_the_one_it_was() {
        let mine = vec![("a", 1), ("b", 2), ("c", 3)];
        let told = retell(mine.clone(), vec!["x", "y", "z"], |spawn, stack| {
            spawn.0 = stack
        });
        assert_eq!(told, vec![("x", 1), ("y", 2), ("z", 3)]);
        // A tabela tirou a última pilha: o lugar dela some.
        let told = retell(mine.clone(), vec!["x", "y"], |spawn, stack| spawn.0 = stack);
        assert_eq!(told, vec![("x", 1), ("y", 2)]);
        // Uma pilha a mais não tem lugar.
        let told = retell(mine, vec!["x", "y", "z", "w"], |spawn, stack| {
            spawn.0 = stack
        });
        assert_eq!(told, vec![("x", 1), ("y", 2), ("z", 3)]);
    }
}
