// HEAD cancelável de `LightTables::differs` (`LightEngine.hasDifferentLightProperties`, que decide se a troca de estado numa posição refaz a luz dela): um estado cuja emissão
// pergunta a posição (`machinery_light::set_states`) sempre refaz, porque a tabela por estado não vê a emissão da block entity (o tanque com lava que sai) e porque o
// `checkBlock` pedido pelo tile chega como a troca do estado por ele mesmo (`machinery_basis::position_light_check`).
fn machinery_basis_position_light_differs(old: u32, new: u32) -> core::ops::ControlFlow<bool> {
    if machinery_light::differs(old, new) {
        core::ops::ControlFlow::Break(true)
    } else {
        core::ops::ControlFlow::Continue(())
    }
}
