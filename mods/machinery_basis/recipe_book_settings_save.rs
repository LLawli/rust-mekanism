// `Level::player_tag` (`save.rs`) monta o composto do jogador num `vec![...]` com controle de fluxo por dentro, que o horizon-mixins recusa num RETURN
// (`E_UNSUPPORTED_MACRO`). Este HEAD cancelável chama a função de novo por dentro (a marca de reentrada deixa o corpo original rodar) e devolve o
// composto com os campos dos tipos de livro de mod.
fn machinery_basis_recipe_book_settings_save(level: &Level) -> core::ops::ControlFlow<Tag> {
    std::thread_local! {
        static INSIDE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if INSIDE.with(|inside| inside.replace(true)) {
        return core::ops::ControlFlow::Continue(());
    }
    let mut value = level.player_tag();
    INSIDE.with(|inside| inside.set(false));
    let extra = &level.player_extra;
    // Os oito campos do vanilla são do Horizon (`recipe_book::Settings::write`, que omite o que vale falso): só os dos tipos de livro de mod
    // (`is<Tipo>GuiOpen` e `is<Tipo>FilteringCraftable`, `CommonHooks.makeModdedRecipeBookTypesSettingsCodec`) passam do composto lido ao gravado.
    const VANILLA: [&str; 8] = [
        "isGuiOpen",
        "isFilteringCraftable",
        "isFurnaceGuiOpen",
        "isFurnaceFilteringCraftable",
        "isBlastingFurnaceGuiOpen",
        "isBlastingFurnaceFilteringCraftable",
        "isSmokerGuiOpen",
        "isSmokerFilteringCraftable",
    ];
    let Some(horizon_world::nbt::Tag::Compound(saved)) = extra.as_ref().and_then(|tag| tag.get("recipeBook")) else {
        return core::ops::ControlFlow::Break(value);
    };
    let Some(horizon_world::nbt::Tag::Compound(book)) = value.get_mut("recipeBook") else {
        return core::ops::ControlFlow::Break(value);
    };
    for (name, tag) in saved {
        let text = name.to_string_lossy();
        let setting = text.starts_with("is") && (text.ends_with("GuiOpen") || text.ends_with("FilteringCraftable"));
        if setting && !VANILLA.contains(&text.as_str()) && matches!(tag, horizon_world::nbt::Tag::Byte(flag) if *flag != 0) {
            book.insert(name.clone(), tag.clone());
        }
    }
    core::ops::ControlFlow::Break(value)
}
