fn mekanism_creative_tabs(doc: &mut J, result: Result<()>) -> Result<()> {
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
        if mekanism_tabs::edit_tab(&id, &mut stacks) {
            *items = stacks.into_iter().map(J::Str).collect();
        }
    }
    result
}
