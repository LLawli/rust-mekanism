fn mekanism_analog_output(dimension: LevelDimension, pos: BlockPos, output: &mut i32) {
    crate::game::mekanism_glue::analog_output(dimension_id(dimension), [pos.x, pos.y, pos.z], output);
}
