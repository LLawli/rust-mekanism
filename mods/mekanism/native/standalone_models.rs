// O desenho vestido do MekaSuit, do jetpack, das botas e do tanque: a tabela do mod (`mekanism_rules::worn_gear`) e os modelos avulsos que o jogo assa para ela.
fn mekanism_standalone_models(models: &mut Vec<String>) {
    crate::game::machinery_basis_gear::register(&crate::game::mekanism_rules::worn_gear::WORN_GEAR, models);
}
