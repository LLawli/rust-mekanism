// A linha de jogo de cada estado dos blocos do Mekanism, no começo de `override_gameplay` (antes das `properties` declaradas).
//
// 1. As quatro máquinas que o port já tinha (`lit` do gabarito `minecraft:furnace`, a tabela `FURNACE`): `Attributes.ACTIVE_LIGHT`, 8 (`Attributes.java:27`), só quando ativas
//    (`AttributeStateActive.adjustProperties`). Desligadas, 0, como o gabarito já dá; acesas, o gabarito dá 13. E a reação ao pistão e a oclusão do original (`BlockMekanism.getPistonPushReaction`,
//    `isSolidRender`, `canOcclude`, `getLightDampening`, `propagatesSkylightDown`), que o gabarito de cubo cheio não dá ao Metallurgic Infuser (de forma menor que o cubo).
// 2. Os blocos de máquina e os blocos avulsos com tile (gabarito `minecraft:lodestone`, estados `active` e `fluid_logged` próprios): a tabela `mekanism_rows`
//    (`block_rows_table.rs`, gerada do original em jogo) diz a luz do estado ativo (8, 13 ou 15) e a da lava (15), a reação ao pistão (`IMMOVEABLE`, de
//    `BlockMekanism.getPistonPushReaction`) a oclusão (`isSolidRender`, `canOcclude`, `getLightDampening`, `propagatesSkylightDown`), que o gabarito de cubo cheio
//    não dá a uma forma menor, e a resistência negativa do `AttributeCustomResistance`, que o arquivo do Horizon recusa; `properties` não alcança nada disso.
fn mekanism_active_light(block: &NewBlock<'_>, combo: &[(String, String)], row: &mut J) {
    const ACTIVE_LIGHT: u8 = 8;
    if let Some(facts) = mekanism_rows::facts(block.decl.id.as_str()) {
        let wet = combo.iter().any(|(k, v)| k == "fluid_logged" && v != "empty");
        let (dampening, skylight_down) = if wet { (facts.wet_dampening, facts.wet_skylight_down) } else { (facts.dampening, facts.skylight_down) };
        row.set("emission", J::Num(mekanism_rows::light(facts, combo, 0).into()));
        row.set("push", J::Str(facts.push.to_owned()));
        row.set("can_occlude", J::Bool(facts.can_occlude));
        row.set("solid_render", J::Bool(facts.solid_render));
        row.set("dampening", J::Num(dampening.into()));
        row.set("skylight_down", J::Bool(skylight_down));
        if let Some(resistance) = facts.explosion {
            row.set("explosion_resistance", J::f64(f64::from(resistance)));
        }
        return;
    }
    let Some(facts) = mekanism_rows::furnace_facts(block.decl.id.as_str()) else {
        return;
    };
    // O gabarito do forno dá `push` normal, `solid_render` e `can_occlude` de cubo cheio, amortecimento 15 e `skylight_down` falso: o original dá a reação ao pistão e a oclusão da tabela (o Metallurgic
    // Infuser, `BlockFactoryMachineModel`, é de forma menor que o cubo: `isSolidRender` falso, amortecimento 0 e a luz do céu passa). A luz do estado ativo é a do original (8), no lugar da do forno (13).
    let wet = combo.iter().any(|(k, v)| k == "fluid_logged" && v != "empty");
    let (dampening, skylight_down) = if wet { (facts.wet_dampening, facts.wet_skylight_down) } else { (facts.dampening, facts.skylight_down) };
    row.set("push", J::Str(facts.push.to_owned()));
    row.set("can_occlude", J::Bool(facts.can_occlude));
    row.set("solid_render", J::Bool(facts.solid_render));
    row.set("dampening", J::Num(dampening.into()));
    row.set("skylight_down", J::Bool(skylight_down));
    if row.get("emission").and_then(J::as_u32).is_some_and(|emission| emission > 0) {
        row.set("emission", J::Num(ACTIVE_LIGHT.into()));
    }
}
