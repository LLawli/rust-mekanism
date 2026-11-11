// `mekanism:mek_data` (`MekanismShapedRecipe`, `WrappedShapedRecipe`) é uma receita de mesa com o formato da `minecraft:crafting_shaped` (o mesmo `key`, `pattern` e `result`), e o
// Horizon manda toda receita de tipo de mod para a tabela `custom`, que a mesa de crafting não lê: as 86 receitas de bloco e de item que o usam (85 do Mekanism e a do vento do
// Generators) nunca apareceriam na mesa. O HEAD de `Recipes::from_sources` troca o tipo delas antes da carga, e a mesa as casa como casa as outras. O que a `assemble` do original
// copia da entrada (`RecipeUpgradeData`: energia, tanques, upgrades, segurança...) o port ainda não guarda no item, então não há o que copiar; a configuração de lados não é
// copiada pelo original (`RecipeUpgradeData.getSupportedTypes`, o TODO dele). Alvo cru, H79.
// O tipo do binding é o `Vec` do parâmetro de `from_sources`.
#[allow(clippy::ptr_arg)]
fn mekanism_mek_data_alias(sources: &mut Vec<(String, Value)>) {
    for (_, recipe) in sources.iter_mut() {
        if let Some(object) = recipe.as_object_mut()
            && object.get("type").and_then(Value::as_str) == Some("mekanism:mek_data")
        {
            object.insert("type".to_owned(), Value::String("minecraft:crafting_shaped".to_owned()));
        }
    }
}
