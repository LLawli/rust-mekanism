fn mekanismgenerators_analog_output(dimension: LevelDimension, pos: BlockPos, output: &mut i32) {
    crate::game::mekanismgenerators_glue::analog_output(dimension_id(dimension), [pos.x, pos.y, pos.z], output);
}
