// O desenho vestido do MekaSuit, do jetpack, das botas e do tanque: a tabela do mod (`mekanism_rules::worn_gear`) e os modelos avulsos que o jogo assa para ela. E, uma vez no começo do jogo, os
// transmissores do mod para o desenho de suas partes por instância (`TransmitterBlockStateModel`, `transmitter_model_state`).
fn mekanism_standalone_models(models: &mut Vec<String>) {
    crate::game::machinery_basis_gear::register(&crate::game::mekanism_rules::worn_gear::WORN_GEAR, &crate::game::mekanism_rules::worn_gear::ATLAS_SPRITES, models);
    crate::game::mekanism_cube::standalone_models(models);
    crate::game::machinery_transporters::stack_box_register(models, "mekanism:block/transporter_box");
    let families = crate::game::mekanism_rules::transmitter_models()
        .into_iter()
        .map(|model| crate::game::machinery_transmitter_client::ModelFamily { block: model.block, diversion: model.diversion, glass: model.glass })
        .collect();
    crate::game::machinery_transmitter_client::register_model_families(families);
    crate::game::mekanism_glue::register_transmitter_contents(models);
}
