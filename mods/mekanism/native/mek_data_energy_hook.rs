// `Recipes::find_with` (a receita que casou e o resultado do modelo): a `assemble` de `mekanism:mek_data` completa o resultado com a energia das entradas. O ponto onde o resultado
// nasce (`assemble_if_matches`) e o retorno de `find_with` têm fechamentos (`.then(|| ..)`, `.map(|r| ..)`) que a injeção recusa, então são dois HEAD da `find_with`. O primeiro
// guarda a grade se ela tem algum item com energia (`stash`); o segundo, cancelável, só age com a grade guardada: roda a `find_with` inteira por dentro (o `INSIDE` a faz seguir
// sozinha na segunda vez) e devolve o resultado dela completado. O helper cancelável não pode ter o tempo de vida como parâmetro, e o nome que volta é emprestado de `recipes`: por
// isso `recipes` é a única referência que ele leva, e a grade vem do primeiro. A escala de mapa (`map_scale`) não vale aqui: uma grade com item de energia nunca é a do mapa que se estende.
fn mekanism_mek_data_energy(recipes: &Recipes, size: usize) -> core::ops::ControlFlow<Option<(&str, ItemStack)>> {
    thread_local! {
        static INSIDE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if INSIDE.get() {
        return core::ops::ControlFlow::Continue(());
    }
    let Some(grid) = mekanism_mek_data::take_stash() else {
        return core::ops::ControlFlow::Continue(());
    };
    INSIDE.set(true);
    let found = recipes.find_with(&grid, size, &|_| None);
    INSIDE.set(false);
    core::ops::ControlFlow::Break(match found {
        Some((name, result)) => mekanism_mek_data::assemble(name, &grid, Some(result)).map(|result| (name, result)),
        None => None,
    })
}
