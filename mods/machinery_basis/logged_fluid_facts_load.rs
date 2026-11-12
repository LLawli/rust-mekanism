// Roda no começo de `gameplay_tables` (`game.rs`), a carga das fatos de bloco que o jogo faz, e cuida do `StateFacts.fluid` de cada estado: ele vem do nome do bloco (`minecraft:water`,
// `minecraft:lava`, as plantas de água) e de `waterlogged`, calculado em `BlockFacts::decode_with_callback`, e é ele que o mob, o item solto e o raio do balde leem em 164 lugares
// (`mobs/living.rs`, `entities/fluids.rs`, ...). Um estado com `fluid_logged=water` ou `lava` de um bloco de mod ficava sem fluido para eles, e é uma fonte no original
// (`BlockMekanism.getFluidState`). As cinco entradas de `BlockFacts` (`load`, `load_pair`, `load_with_behaviours`, `load_with_behaviour_json`, `load_with_projection`) devolvem `Self` dentro do
// resultado, que o horizon-mixins recusa (RETURN e HEAD cancelável: `E_UNSUPPORTED_SIGNATURE`), e `decode_with_callback` é genérico. Este HEAD cancelável chama a função de novo por dentro (a
// marca de reentrada deixa o corpo original rodar) e dá a fonte à tabela já pronta, antes de qualquer leitura (a luz e o movimento, que nascem dentro da função, não leem `fluid`).
fn machinery_logged_fluid_facts(
    root: &std::path::Path,
    assets: &Assets,
    parallel: bool,
    progress: &Progress,
) -> core::ops::ControlFlow<std::result::Result<GameplayTables, String>> {
    use machinery_logged_fluid_facts as logged;
    std::thread_local! {
        static INSIDE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if INSIDE.with(|inside| inside.replace(true)) {
        return core::ops::ControlFlow::Continue(());
    }
    let loaded = gameplay_tables(root, assets, parallel, progress);
    INSIDE.with(|inside| inside.set(false));
    let mut tables = match loaded {
        Ok(tables) => tables,
        Err(error) => return core::ops::ControlFlow::Break(Err(error)),
    };
    for (index, state) in tables.facts.states.iter_mut().enumerate() {
        let value = assets
            .registry
            .state(horizon_world::registry::BlockStateId(index as u32))
            .and_then(|found| found.properties.get("fluid_logged"))
            .map(String::as_str);
        match logged::source(value) {
            Some(logged::Source::Water) => state.fluid = horizon_client::blocks::Fluid::Water,
            Some(logged::Source::Lava) => state.fluid = horizon_client::blocks::Fluid::Lava,
            None => continue,
        }
        state.fluid_level = logged::SOURCE_LEVEL;
        state.fluid_falling = false;
    }
    core::ops::ControlFlow::Break(Ok(tables))
}
