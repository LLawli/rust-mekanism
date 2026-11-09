// Entra em `crates/horizon-client/src/motion.rs` como `pub mod machinery_water_like` (`visibility = "pub"`). Os estados de bloco de líquido de mod, por número de
// estado, para a física de entidade (`FluidResources::fluid`, só água e lava): o nível mais 1 onde o estado é de um bloco de líquido declarado, 0 no resto. O
// `Game` (crate de cima) escreve a tabela a cada carga de dados e o RETURN de `FluidResources::fluid` (crate da biblioteca) a lê. Guarda também o que as injeções
// `fire_mob_flip` e `fire_mob_restore` combinam entre si para o mob em chamas dentro de um fluido de mod (`mob_flip`, `mob_restore`).
use std::cell::Cell;
use std::sync::RwLock;

static TABLE: RwLock<Vec<u8>> = RwLock::new(Vec::new());

/// Troca a tabela: a cada carga de dados.
pub fn set(table: Vec<u8>) {
    *TABLE.write().unwrap_or_else(|e| e.into_inner()) = table;
}

/// O nível (0 a 15) do estado `state`, se ele é de um bloco de líquido de mod.
pub fn get(state: u32) -> Option<u8> {
    match TABLE
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .get(state as usize)
    {
        Some(&entry) if entry > 0 => Some(entry - 1),
        _ => None,
    }
}

thread_local! {
    /// O mob cujo `in_water` `mob_flip` desligou neste tick e `mob_restore` ainda não devolveu (o mob ticka numa thread só, de `update_fluids` a `block_contact_damage`).
    static FLIPPED: Cell<Option<u32>> = const { Cell::new(None) };
}

/// Fim de `Mob::update_fluids`: um mob em chamas (`burning`) com os pés (`feet`) ou os olhos (`eye`) num bloco de líquido de mod deixa de estar na água até `mob_restore`, porque
/// o `FluidType` dos fluidos do Mekanism tem `canExtinguish` falso: o fogo do mob só apaga pela água do vanilla (`Fluid.entityInside`) e pela chuva. Sempre escreve a marca,
/// para uma chamada antiga (`check_fall_damage` também chama `update_fluids`) não deixar uma marca de outro tick.
pub fn mob_flip(id: u32, burning: bool, feet: u32, eye: u32, in_water: &mut bool) {
    let flip = burning && *in_water && (get(feet).is_some() || get(eye).is_some());
    if flip {
        *in_water = false;
    }
    FLIPPED.with(|flipped| flipped.set(flip.then_some(id)));
}

/// Começo de `block_contact_damage`, depois do fogo contado em `base_tick`: devolve a água ao mob que `mob_flip` tirou dela, para o resto do tick (a água, o ar, o movimento)
/// ver o que o fluido de mod dá.
pub fn mob_restore(id: u32, in_water: &mut bool) {
    FLIPPED.with(|flipped| {
        if flipped.get() == Some(id) {
            *in_water = true;
            flipped.set(None);
        }
    });
}
