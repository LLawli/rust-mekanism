fn machinery_basis_components_resolve(kind: &str, data: &serde_json::Map<String, Value>, items: &mut Vec<String>) {
    machinery_components::resolve(kind, data, items);
}
