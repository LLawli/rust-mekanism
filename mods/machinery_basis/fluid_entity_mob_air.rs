fn machinery_basis_fluid_entity_mob_air(entity: u32, kind: &str, value: Breath) -> Breath {
    // `CommonHooks.onLivingBreathe`: com os olhos num fluido que não é ar e em que a entidade se afoga (`canDrown`), o ar acaba. O mob em fluido de mod
    // não tem `eye_in_water` (a tabela de fatos não conhece o fluido): `fluid_entity_mob_update` guardou os olhos do mob deste `base_tick`.
    if kind != "minecraft:player" && crate::motion::machinery_water_like::mob_eye(entity) {
        Breath {
            can_breathe: false,
            refill: 0,
            ..value
        }
    } else {
        value
    }
}
