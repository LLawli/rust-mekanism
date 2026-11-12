// O bloco que o clique do balde enche (`BucketItem.use`: o bloco clicado, se é um contêiner de água que aceita o fluido, senão o bloco ao lado). O gancho `fill_bucket` ouve esse bloco e, quando o
// clicado é o contêiner, também o bloco ao lado (a conferência `mayUseItemAt`, que não passa fluido por ele); sem saber qual é qual, um transmissor ao lado de uma laje recusaria encher a laje. Este RETURN
// de `Game::bucket_target` só anota o resultado para o gancho (`mekanism_glue::fill_bucket`) e o devolve como veio.
fn mekanism_bucket_target(value: [i32; 3]) -> [i32; 3] {
    crate::game::mekanism_glue::bucket_target_seen(value);
    value
}
