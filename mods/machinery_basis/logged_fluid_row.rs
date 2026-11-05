fn machinery_basis_logged_fluid_row(row: &mut J, combo: &[(String, String)]) {
    if !combo.iter().any(|(name, value)| name == "fluid_logged" && value == "lava") {
        return;
    }
    if matches!(row, J::Obj(_)) {
        row.set("emission", J::Num(15u8.into()));
        return;
    }
    let J::Arr(parts) = row else {
        return;
    };
    if matches!(parts.get(4), Some(J::Bool(_))) {
        parts[2] = J::Num((horizon_world::fluid::FluidKind::Lava as u8).into());
        parts[3] = J::Num(8u8.into());
        parts[4] = J::Bool(false);
    }
}
