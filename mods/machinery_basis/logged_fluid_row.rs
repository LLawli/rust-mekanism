fn machinery_basis_logged_fluid_row(row: &mut J, combo: &[(String, String)]) {
    let kind = match combo.iter().find(|(name, _)| name == "fluid_logged").map(|(_, value)| value.as_str()) {
        Some("lava") => horizon_world::fluid::FluidKind::Lava,
        Some("water") => horizon_world::fluid::FluidKind::Water,
        _ => return,
    };
    if matches!(row, J::Obj(_)) {
        if kind == horizon_world::fluid::FluidKind::Lava {
            row.set("emission", J::Num(15u8.into()));
        }
        return;
    }
    let J::Arr(parts) = row else {
        return;
    };
    if matches!(parts.get(4), Some(J::Bool(_))) {
        parts[2] = J::Num((kind as u8).into());
        parts[3] = J::Num(8u8.into());
        parts[4] = J::Bool(false);
    }
}
