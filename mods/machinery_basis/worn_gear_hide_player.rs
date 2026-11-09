// Roda no fim de `player_model_visibility`, que mostra o corpo, os braços e as pernas do jogador (e esconde o que as opções escondem): esconde de novo o que o equipamento
// vestido esconde (`worn_hides.rs`). O valor vem antes de `p` porque o RETURN passa a expressão final do corpo (o laço que usa `p`) como primeiro argumento.
fn machinery_basis_worn_hide_player(value: (), p: &mut crate::mobs::anim::Poses) -> () {
    crate::mobs::anim::machinery_basis_worn::again(p);
    value
}
