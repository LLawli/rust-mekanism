fn mekanism_screen_click(block: &str, at: [f32; 2], right: bool, button: &mut Option<i32>) {
    crate::game::mekanism_glue::screen_click(block, at, right, button);
}
