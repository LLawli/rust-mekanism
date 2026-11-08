fn machinery_basis_components_stash(object: &serde_json::Map<String, Value>, items: &Items) {
    machinery_components::stash(object, &items.item_tag_order);
}
