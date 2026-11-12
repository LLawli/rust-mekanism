// O instrumento do bloco de nota em cima do sal e do bloco de bio fuel (decisão 17 do plano). O original registra `block_salt` com `instrument(NoteBlockInstrument.SNARE)` e
// `block_bio_fuel` com `BANJO` (`MekanismBlocks.java:980-982`); o Horizon não tem `instrument` em `properties` (H44), então os dois blocos usam o gabarito `minecraft:lapis_block`
// (`HARP`). O instrumento mora na posição 8 da linha de cada estado de `facts.json` e de `block-behaviours.json`, que `Plan::states` copia do gabarito e renomeia com `J::rename`
// (`row.rename(r, ids)`). Este RETURN de `J::rename` troca o `HARP` copiado pelo instrumento do original, só na linha dos dois blocos: `to` é o id para onde a linha foi renomeada.
// Outro nó do JSON (um número, uma lista curta, uma linha de jogo que é objeto) ou outro bloco passa sem mudança.
fn mekanism_note_instrument(row: &mut J, to: &str, value: ()) {
    // Posição do instrumento na linha do estado (`data.rs`, `StateFacts::instrument`: `row.get(8)`), e o do gabarito `minecraft:lapis_block`.
    const INSTRUMENT_AT: usize = 8;
    const TEMPLATE_INSTRUMENT: &str = "HARP";
    const INSTRUMENTS: [(&str, &str); 2] = [("mekanism:block_salt", "SNARE"), ("mekanism:block_bio_fuel", "BANJO")];
    let () = value;
    let J::Arr(cells) = row else {
        return;
    };
    if cells.len() <= INSTRUMENT_AT || !matches!(cells[0], J::Num(_)) {
        return;
    }
    let Some((_, instrument)) = INSTRUMENTS.iter().find(|(id, _)| *id == to) else {
        return;
    };
    if matches!(&cells[INSTRUMENT_AT], J::Str(name) if name == TEMPLATE_INSTRUMENT) {
        cells[INSTRUMENT_AT] = J::Str((*instrument).to_owned());
    }
}
