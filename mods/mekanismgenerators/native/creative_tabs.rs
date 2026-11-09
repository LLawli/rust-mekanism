// `GeneratorsCreativeTabs.addToExistingTabs` (`BuildCreativeModeTabContentsEvent`): os geradores nos blocos funcionais, os blocos com comparador na redstone, os baldes nas
// ferramentas e o hohlraum e o painel solar nos ingredientes. O Horizon só deixa o mod declarar a aba própria (`creative_tab/mekanismgenerators.json`); o RETURN de
// `Plan::creative_tabs` põe a lista no fim de cada aba do vanilla (a tabela é `mekanismgenerators_tabs`, `creative_tabs_table.rs`). É o `mekanism_creative_tabs` do Mekanism, com
// `result` antes de `doc`: as duas injeções são RETURN do mesmo método, o Horizon aninha a segunda em volta da primeira, e `&mut *doc` antes do resultado interno (que também toma
// `doc`) é um segundo empréstimo mutável; com o resultado primeiro o interno acaba antes.
fn mekanismgenerators_creative_tabs(result: Result<()>, doc: &mut J) -> Result<()> {
    if result.is_err() {
        return result;
    }
    let Some(J::Arr(tabs)) = doc.at("tabs") else {
        return result;
    };
    for tab in tabs.iter_mut() {
        let id = match tab.get("id") {
            Some(J::Str(id)) => id.clone(),
            _ => continue,
        };
        let Some(J::Arr(items)) = tab.get_mut("items") else {
            continue;
        };
        let mut stacks: Vec<String> = items
            .iter()
            .filter_map(|item| match item {
                J::Str(stack) => Some(stack.clone()),
                _ => None,
            })
            .collect();
        if mekanismgenerators_tabs::edit_tab(&id, &mut stacks) {
            *items = stacks.into_iter().map(J::Str).collect();
        }
    }
    result
}
