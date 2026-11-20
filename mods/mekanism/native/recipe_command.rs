fn mekanism_recipe_command(game: &mut crate::game::Game<'_>, give: bool, recipes: &[String]) {
    crate::game::mekanism_glue::recipe_command(game, give, recipes);
}
