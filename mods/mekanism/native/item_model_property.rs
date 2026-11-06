fn mekanism_item_model_property(property: &str, stack: &StackView, value: &mut bool) {
    crate::game::mekanism_glue::item_model_property(property, stack, value);
}
