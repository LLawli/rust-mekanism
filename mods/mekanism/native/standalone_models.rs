// O desenho vestido do MekaSuit, do jetpack, das botas e do tanque: a tabela do mod (`mekanism_rules::worn_gear`) e os modelos avulsos que o jogo assa para ela. E as partes do modelo dos
// transmissores (`TransmitterBlockStateModel`): um modelo por lado e por estado visual, que o mesher pede por posição (`transmitter_model_parts`).
fn mekanism_standalone_models(models: &mut Vec<String>) {
    crate::game::machinery_basis_gear::register(&crate::game::mekanism_rules::worn_gear::WORN_GEAR, &crate::game::mekanism_rules::worn_gear::ATLAS_SPRITES, models);
    crate::game::mekanism_cube::standalone_models(models);
    let families = crate::game::mekanism_rules::transmitter_models()
        .into_iter()
        .map(|model| crate::game::machinery_transmitter_client::ModelFamily {
            block: model.block,
            prefix: model.prefix,
            hide_contiguous_join: model.hide_contiguous_join,
            diversion: model.diversion,
        })
        .collect();
    crate::game::machinery_transmitter_client::register_model_families(families, models);
}
