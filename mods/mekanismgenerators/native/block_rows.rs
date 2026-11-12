// A linha de jogo de cada estado dos blocos do Mekanism Generators, no começo de `override_gameplay` (antes das `properties` declaradas): a tabela `mekanismgenerators_rows`
// (`block_rows_table.rs`, gerada do original em jogo) diz a luz do estado ativo (`Attributes.ACTIVE_MELT_LIGHT`, 13) e a da lava (15), a reação ao pistão (`IMMOVEABLE`,
// de `BlockMekanism.getPistonPushReaction`) e a oclusão (`isSolidRender`, `canOcclude`, `getLightDampening`, `propagatesSkylightDown`), que o gabarito de cubo cheio não dá
// a uma forma menor; `properties` não alcança nada disso. É o `mekanism_active_light` do Mekanism, com a tabela do Generators.
fn mekanismgenerators_block_rows(block: &NewBlock<'_>, combo: &[(String, String)], row: &mut J) {
    // O `heat_generator` segue com o gabarito do forno (`lit`): só a reação ao pistão e a oclusão do original (a tabela `FURNACE`; o gabarito dá `push` normal, cubo cheio e amortecimento 15, e o
    // `BlockTileModel` é de forma menor que o cubo: `isSolidRender` falso, amortecimento 0 e a luz do céu passa). A luz é a do gabarito e a da lava, que `logged-fluid-row` põe.
    let furnace = mekanismgenerators_rows::furnace_facts(block.decl.id.as_str());
    let Some(facts) = furnace.or_else(|| mekanismgenerators_rows::facts(block.decl.id.as_str())) else {
        return;
    };
    let wet = combo.iter().any(|(k, v)| k == "fluid_logged" && v != "empty");
    let (dampening, skylight_down) = if wet { (facts.wet_dampening, facts.wet_skylight_down) } else { (facts.dampening, facts.skylight_down) };
    if furnace.is_none() {
        row.set("emission", J::Num(mekanismgenerators_rows::light(facts, combo, 0).into()));
    }
    row.set("push", J::Str(facts.push.to_owned()));
    row.set("can_occlude", J::Bool(facts.can_occlude));
    row.set("solid_render", J::Bool(facts.solid_render));
    row.set("dampening", J::Num(dampening.into()));
    row.set("skylight_down", J::Bool(skylight_down));
    if let Some(resistance) = facts.explosion {
        row.set("explosion_resistance", J::f64(f64::from(resistance)));
    }
}
