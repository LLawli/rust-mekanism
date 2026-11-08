fn mekanismgenerators_has_analog_output(level: &GameLevel<'_>, state: BlockStateId, output: &mut bool) {
    crate::game::mekanismgenerators_glue::has_analog_output(level, state, output);
}
