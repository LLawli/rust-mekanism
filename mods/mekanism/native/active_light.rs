// A luz do estado ativo das máquinas `Machine` do Mekanism: `Attributes.ACTIVE_LIGHT`, 8 (`Attributes.java:27`), só quando ativas
// (`AttributeStateActive.adjustProperties`). Desligadas, 0, como o gabarito `minecraft:furnace` já dá; acesas, o gabarito dá 13.
// Roda no começo de `override_gameplay`, linha a linha da tabela de jogo, antes das `properties` declaradas (que não têm `light`).
fn mekanism_active_light(block: &NewBlock<'_>, row: &mut J) {
    const ACTIVE_LIGHT: u8 = 8;
    const MACHINES: [&str; 4] = ["mekanism:crusher", "mekanism:enrichment_chamber", "mekanism:energized_smelter", "mekanism:metallurgic_infuser"];
    if !MACHINES.contains(&block.decl.id.as_str()) {
        return;
    }
    if row.get("emission").and_then(J::as_u32).is_some_and(|emission| emission > 0) {
        row.set("emission", J::Num(ACTIVE_LIGHT.into()));
    }
}
