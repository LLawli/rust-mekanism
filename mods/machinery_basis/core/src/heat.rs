//! Calor e temperatura ambiente, como o `HeatAPI` do Mekanism: temperatura é calor dividido pela
//! capacidade, e o que está mais quente que o ambiente perde calor para o ar por cada face.

use crate::sides::Face;

/// A temperatura de um bioma de 0,8, como planícies, em kelvin.
pub const AMBIENT_TEMP: f64 = 300.0;
/// A troca de calor com o ar (`HeatAPI.AIR_INVERSE_COEFFICIENT`).
pub const AIR_INVERSE_COEFFICIENT: f64 = 10_000.0;
const PLAINS_BIOME_TEMP: f64 = 0.8;
const KELVIN_PER_BIOME_DEGREE: f64 = 25.0;
const BIOME_TEMP_LIMIT: f64 = 5.0;
/// Abaixo disto uma troca de calor é ignorada (`HeatAPI.EPSILON`, que é `com.mojang.math.Constants.EPSILON`: um `float` de 1e-6
/// ampliado para `double`).
pub const EPSILON: f64 = 1.0e-6_f32 as f64;

/// A temperatura do ar, em kelvin, num bioma de temperatura `biome`.
pub fn ambient(biome: f64) -> f64 {
    let biome = biome.clamp(-BIOME_TEMP_LIMIT, BIOME_TEMP_LIMIT);
    AMBIENT_TEMP + KELVIN_PER_BIOME_DEGREE * (biome - PLAINS_BIOME_TEMP)
}

/// `CachedAmbientTemperature`: o ar de uma máquina, lido do mundo na primeira vez que alguém o pede e guardado pelo resto da vida do
/// objeto. Há um valor por face (o do bloco vizinho, na ordem das direções do jogo) e um para o próprio bloco (`side == null`). Um
/// bioma que muda depois (`/fillbiome`) não muda o que o tile lê: só um tile novo (o chunk que descarrega e carrega) lê de novo.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AmbientCache {
    /// Seis faces e, no fim, o próprio bloco.
    values: [Option<f64>; 7],
}

impl AmbientCache {
    /// O índice do próprio bloco (`side == null`).
    pub const SELF_INDEX: usize = 6;

    /// O valor da face `index` (0 a 5) ou do bloco (`SELF_INDEX`): o guardado, ou o que `read` lê do mundo agora. Sem mundo para ler (`read`
    /// devolve `None`: o `world == null` do original) vale `AMBIENT_TEMP` e nada é guardado.
    pub fn get(&mut self, index: usize, read: impl FnOnce() -> Option<f64>) -> f64 {
        if let Some(value) = self.values[index] {
            return value;
        }
        match read() {
            Some(value) => *self.values[index].insert(value),
            None => AMBIENT_TEMP,
        }
    }
}

/// O que guarda calor numa máquina.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Capacitor {
    pub heat: f64,
    pub capacity: f64,
    pub inverse_conduction: f64,
    pub inverse_insulation: f64,
}

impl Capacitor {
    /// Um capacitor na temperatura do ambiente.
    pub fn new(
        capacity: f64,
        inverse_conduction: f64,
        inverse_insulation: f64,
        ambient: f64,
    ) -> Self {
        Self {
            heat: (capacity * ambient).max(0.0),
            capacity,
            inverse_conduction,
            inverse_insulation,
        }
    }

    pub fn temperature(&self) -> f64 {
        self.heat / self.capacity
    }

    pub fn handle(&mut self, transfer: f64) {
        if transfer.abs() > EPSILON {
            self.heat = (self.heat + transfer).max(0.0);
        }
    }

    /// Perde calor para o ar por cada face, com a isolação daquela face. Devolve a perda.
    pub fn lose_to_environment(&mut self, ambient: f64, insulation: [f64; 6]) -> f64 {
        self.lose_to_faces([ambient; 6], insulation)
    }

    /// Como `lose_to_environment`, com o ar de cada face na própria temperatura: `ITileHeatHandler.simulateEnvironment` pergunta
    /// `getAmbientTemperature(side)`, que é a do bioma do bloco vizinho. A ordem é a das direções do jogo, cada face já vê o calor
    /// que a anterior tirou.
    pub fn lose_to_faces(&mut self, ambient: [f64; 6], insulation: [f64; 6]) -> f64 {
        let mut lost = 0.0;
        for (face_ambient, face_insulation) in ambient.into_iter().zip(insulation) {
            let inverse = AIR_INVERSE_COEFFICIENT + face_insulation + self.inverse_conduction;
            let transfer = (self.temperature() - face_ambient) / inverse;
            self.handle(-transfer * self.capacity);
            if transfer > 0.0 {
                lost += transfer;
            }
        }
        lost
    }
}

/// `HeatAPI.HeatTransfer`: o que um `simulate` moveu, para a tela e o leitor de rede.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HeatTransfer {
    /// A temperatura que passou para os vizinhos (`adjacentTransfer`).
    pub adjacent: f64,
    /// A temperatura que o ar levou, só das faces mais quentes que ele (`environmentTransfer`).
    pub environment: f64,
}

/// Os `IHeatHandler` do mundo, cada um por um identificador (a referência ao objeto, no Java): o tile que simula, o vizinho que recebe
/// calor e o próprio capacitor de um condutor são todos um `Handle`. Quem implementa é o dono dos objetos (o jogo, ou o mundo simulado de
/// um teste), porque o mesmo objeto é o vizinho de um e o capacitor de outro.
pub trait HeatHandlers {
    type Handle: Copy;
    /// `IHeatHandler.getTemperature`.
    fn temperature(&self, handle: Self::Handle) -> f64;
    /// `IHeatHandler.getInverseConduction`.
    fn inverse_conduction(&self, handle: Self::Handle) -> f64;
    /// `IHeatHandler.getHeatCapacity`.
    fn heat_capacity(&self, handle: Self::Handle) -> f64;
    /// `IHeatHandler.handleHeat(transfer, transaction)`, dentro da transação da rede.
    fn handle_heat(&mut self, handle: Self::Handle, transfer: f64);
}

/// Os `IHeatCapacitor`: um `IHeatHandler` que também diz a isolação (`getInverseInsulation`).
pub trait HeatCapacitors: HeatHandlers {
    fn inverse_insulation(&self, handle: Self::Handle) -> f64;
}

/// Um tile que troca calor (`ITileHeatHandler`). `H` é o identificador dos `IHeatHandler` do mundo ([`HeatHandlers::Handle`]). O que o
/// original dá por `default` na interface está em [`simulate`], [`simulate_adjacent`] e [`simulate_environment`].
pub trait TileHeatHandler<H: Copy> {
    /// `getHeatCapacitor(side)`: o capacitor visto desse lado, ou nenhum.
    fn heat_capacitor(&self, side: Face) -> Option<H>;
    /// `getAdjacent(side)`: o vizinho desse lado que recebe calor (o padrão é nenhum).
    fn adjacent(&self, _side: Face) -> Option<H> {
        None
    }
    /// `getAmbientTemperature(side)`: o ar daquele lado (o padrão é `HeatAPI.AMBIENT_TEMP`). `&self`: o cache de quem implementa é interior.
    fn ambient_temperature(&self, _side: Face) -> f64 {
        AMBIENT_TEMP
    }
    /// `countsAsAdjacent(side)`: se a troca desse lado entra em `adjacentTransfer` (o padrão é sim).
    fn counts_as_adjacent(&self, _side: Face) -> bool {
        true
    }
}

/// `ITileHeatHandler.simulate`: a troca com os vizinhos e depois a com o ar.
pub fn simulate<W: HeatCapacitors, T: TileHeatHandler<W::Handle>>(
    tile: &T,
    world: &mut W,
) -> HeatTransfer {
    let adjacent = simulate_adjacent(tile, world);
    let environment = simulate_environment(tile, world);
    HeatTransfer {
        adjacent,
        environment,
    }
}

/// `ITileHeatHandler.simulateEnvironment`: cada face perde calor para o ar na própria temperatura. A ordem das leituras é a do Java: a capacidade, a
/// isolação e a condução do capacitor, e só então a temperatura dele e a do ar (a temperatura de um capacitor ainda não inicializado lê o ar).
pub fn simulate_environment<W: HeatCapacitors, T: TileHeatHandler<W::Handle>>(
    tile: &T,
    world: &mut W,
) -> f64 {
    let mut environment_transfer = 0.0;
    for side in Face::ALL {
        let Some(capacitor) = tile.heat_capacitor(side) else {
            continue;
        };
        let heat_capacity = world.heat_capacity(capacitor);
        let inverse_conduction = AIR_INVERSE_COEFFICIENT
            + world.inverse_insulation(capacitor)
            + world.inverse_conduction(capacitor);
        let temperature_to_transfer =
            (world.temperature(capacitor) - tile.ambient_temperature(side)) / inverse_conduction;
        world.handle_heat(capacitor, -temperature_to_transfer * heat_capacity);
        if temperature_to_transfer > 0.0 {
            environment_transfer += temperature_to_transfer;
        }
    }
    environment_transfer
}

/// `ITileHeatHandler.simulateAdjacent`: só o lado mais quente cede calor ao vizinho (o mais frio é problema do vizinho, quando ele
/// simula), pela temperatura final de uma calorimetria dividida pela condução dos dois.
pub fn simulate_adjacent<W: HeatCapacitors, T: TileHeatHandler<W::Handle>>(
    tile: &T,
    world: &mut W,
) -> f64 {
    let mut adjacent_transfer = 0.0;
    for side in Face::ALL {
        let Some(capacitor) = tile.heat_capacitor(side) else {
            continue;
        };
        let Some(sink) = tile.adjacent(side) else {
            continue;
        };
        let temperature = world.temperature(capacitor);
        let sink_temperature = world.temperature(sink);
        if temperature <= sink_temperature {
            continue;
        }
        let heat_capacity = world.heat_capacity(capacitor);
        let sink_heat_capacity = world.heat_capacity(sink);
        let final_temperature = (temperature * heat_capacity
            + sink_temperature * sink_heat_capacity)
            / (heat_capacity + sink_heat_capacity);
        let inverse_conduction =
            world.inverse_conduction(sink) + world.inverse_conduction(capacitor);
        let temperature_to_transfer = (temperature - final_temperature) / inverse_conduction;
        let heat_to_transfer = temperature_to_transfer * heat_capacity;
        world.handle_heat(capacitor, -heat_to_transfer);
        world.handle_heat(sink, heat_to_transfer);
        if temperature_to_transfer != 0.0 && tile.counts_as_adjacent(side) {
            adjacent_transfer += temperature_to_transfer;
        }
    }
    adjacent_transfer
}

/// O ar de um bloco: `HeatAPI.getAmbientTemp(world, pos)` do bloco (`None`) ou do vizinho de um lado (`Some`). `None` na resposta é o
/// `world == null` do original (vale `AMBIENT_TEMP` e o [`AmbientCache`] não guarda).
pub trait AmbientSource {
    fn read(&self, side: Option<Face>) -> Option<f64>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ambient_cache_reads_once_per_index() {
        let mut cache = AmbientCache::default();
        assert_eq!(cache.get(0, || Some(281.5)), 281.5);
        assert_eq!(cache.get(0, || Some(330.0)), 281.5);
        assert_eq!(cache.get(AmbientCache::SELF_INDEX, || Some(330.0)), 330.0);
        assert_eq!(cache.get(1, || Some(299.0)), 299.0);
    }

    #[test]
    fn without_a_world_the_air_is_300_and_nothing_is_kept() {
        let mut cache = AmbientCache::default();
        assert_eq!(cache.get(2, || None), AMBIENT_TEMP);
        assert_eq!(cache.get(2, || Some(285.0)), 285.0);
        assert_eq!(cache.get(2, || None), 285.0);
    }

    #[test]
    fn plains_are_the_reference() {
        assert_eq!(ambient(0.8), AMBIENT_TEMP);
    }

    #[test]
    fn colder_and_hotter_biomes_shift_the_air() {
        assert!((ambient(0.0) - 280.0).abs() < 1e-9);
        assert!((ambient(2.0) - 330.0).abs() < 1e-9);
    }

    #[test]
    fn biome_temperature_is_limited() {
        assert_eq!(ambient(50.0), ambient(5.0));
        assert_eq!(ambient(-50.0), ambient(-5.0));
    }

    #[test]
    fn heat_never_goes_negative() {
        let mut capacitor = Capacitor::new(10.0, 5.0, 100.0, 300.0);
        capacitor.handle(-1_000_000.0);
        assert_eq!(capacitor.heat, 0.0);
    }

    #[test]
    fn tiny_transfers_are_ignored() {
        let mut capacitor = Capacitor::new(10.0, 5.0, 100.0, 300.0);
        capacitor.handle(0.000_000_5);
        assert_eq!(capacitor.heat, 3_000.0);
        capacitor.handle(EPSILON);
        assert_eq!(capacitor.heat, 3_000.0);
    }

    #[test]
    fn a_transfer_just_over_epsilon_counts() {
        let mut capacitor = Capacitor::new(10.0, 5.0, 100.0, 300.0);
        capacitor.handle(0.000_005);
        assert_eq!(capacitor.heat, 3_000.000_005);
    }

    #[test]
    fn at_ambient_nothing_is_lost() {
        let mut capacitor = Capacitor::new(10.0, 5.0, 100.0, 300.0);
        assert_eq!(capacitor.lose_to_environment(300.0, [100.0; 6]), 0.0);
        assert_eq!(capacitor.temperature(), 300.0);
    }

    #[test]
    fn each_face_loses_heat_to_its_own_air() {
        let mut warm = Capacitor::new(10.0, 5.0, 100.0, 300.0);
        let mut cold = warm;
        warm.handle(1_000.0);
        cold.handle(1_000.0);
        let mut faces = [300.0; 6];
        faces[1] = 280.0;
        let uniform = warm.lose_to_environment(300.0, [100.0; 6]);
        let uneven = cold.lose_to_faces(faces, [100.0; 6]);
        assert!(uneven > uniform);
        assert!(cold.temperature() < warm.temperature());
        let mut same = Capacitor::new(10.0, 5.0, 100.0, 300.0);
        same.handle(1_000.0);
        assert_eq!(same.lose_to_faces([300.0; 6], [100.0; 6]), uniform);
        assert_eq!(same, warm);
    }

    #[test]
    fn something_hotter_cools_down() {
        let mut capacitor = Capacitor::new(10.0, 5.0, 100.0, 300.0);
        capacitor.handle(1_000.0);
        let before = capacitor.temperature();
        assert!(capacitor.lose_to_environment(300.0, [100.0; 6]) > 0.0);
        assert!(capacitor.temperature() < before);
    }

    /// Capacitores de teste por índice, para o `simulate` genérico.
    struct Arena(Vec<Capacitor>);

    impl HeatHandlers for Arena {
        type Handle = usize;

        fn temperature(&self, handle: usize) -> f64 {
            self.0[handle].temperature()
        }

        fn inverse_conduction(&self, handle: usize) -> f64 {
            self.0[handle].inverse_conduction
        }

        fn heat_capacity(&self, handle: usize) -> f64 {
            self.0[handle].capacity
        }

        fn handle_heat(&mut self, handle: usize, transfer: f64) {
            self.0[handle].handle(transfer);
        }
    }

    impl HeatCapacitors for Arena {
        fn inverse_insulation(&self, handle: usize) -> f64 {
            self.0[handle].inverse_insulation
        }
    }

    /// Um tile com um capacitor em todo lado, um vizinho só por cima e o ar fixo.
    struct Tile {
        own: usize,
        up: Option<usize>,
        ambient: f64,
        counts: bool,
    }

    impl TileHeatHandler<usize> for Tile {
        fn heat_capacitor(&self, _side: Face) -> Option<usize> {
            Some(self.own)
        }

        fn adjacent(&self, side: Face) -> Option<usize> {
            (side == Face::Up).then_some(self.up).flatten()
        }

        fn ambient_temperature(&self, _side: Face) -> f64 {
            self.ambient
        }

        fn counts_as_adjacent(&self, _side: Face) -> bool {
            self.counts
        }
    }

    #[test]
    fn the_generic_environment_loss_is_the_capacitor_one() {
        let mut direct = Capacitor::new(10.0, 5.0, 100.0, 300.0);
        direct.handle(1_000.0);
        let mut arena = Arena(vec![direct]);
        let tile = Tile {
            own: 0,
            up: None,
            ambient: 290.0,
            counts: true,
        };
        let lost = direct.lose_to_environment(290.0, [100.0; 6]);
        let transfer = simulate(&tile, &mut arena);
        assert_eq!(transfer.environment, lost);
        assert_eq!(transfer.adjacent, 0.0);
        assert_eq!(arena.0[0], direct);
    }

    #[test]
    fn the_hotter_side_gives_heat_by_calorimetry_over_both_conductions() {
        let mut hot = Capacitor::new(2.0, 5.0, 1.0e12, 300.0);
        hot.handle(2.0 * 100.0);
        let cold = Capacitor::new(6.0, 7.0, 1.0e12, 300.0);
        let mut arena = Arena(vec![hot, cold]);
        let tile = Tile {
            own: 0,
            up: Some(1),
            ambient: 300.0,
            counts: true,
        };
        let (t_hot, t_cold) = (400.0, 300.0);
        let final_temperature = (t_hot * 2.0 + t_cold * 6.0) / (2.0 + 6.0);
        let to_transfer = (t_hot - final_temperature) / (7.0 + 5.0);
        let adjacent = simulate_adjacent(&tile, &mut arena);
        assert_eq!(adjacent, to_transfer);
        assert_eq!(arena.0[0].heat, 2.0 * 400.0 - to_transfer * 2.0);
        assert_eq!(arena.0[1].heat, 6.0 * 300.0 + to_transfer * 2.0);
    }

    #[test]
    fn the_colder_side_waits_for_the_other_to_simulate() {
        let cold = Capacitor::new(2.0, 5.0, 1.0e12, 300.0);
        let mut hot = Capacitor::new(6.0, 7.0, 1.0e12, 300.0);
        hot.handle(600.0);
        let mut arena = Arena(vec![cold, hot]);
        let tile = Tile {
            own: 0,
            up: Some(1),
            ambient: 300.0,
            counts: true,
        };
        assert_eq!(simulate_adjacent(&tile, &mut arena), 0.0);
        assert_eq!(arena.0[0], cold);
    }

    #[test]
    fn equal_temperatures_do_not_transfer() {
        let a = Capacitor::new(2.0, 5.0, 1.0e12, 300.0);
        let mut arena = Arena(vec![a, a]);
        let tile = Tile {
            own: 0,
            up: Some(1),
            ambient: 300.0,
            counts: true,
        };
        assert_eq!(simulate_adjacent(&tile, &mut arena), 0.0);
    }

    #[test]
    fn a_transfer_that_does_not_count_as_adjacent_still_moves_heat() {
        let mut hot = Capacitor::new(1.0, 5.0, 1.0e12, 300.0);
        hot.handle(100.0);
        let cold = Capacitor::new(1.0, 5.0, 1.0e12, 300.0);
        let mut arena = Arena(vec![hot, cold]);
        let tile = Tile {
            own: 0,
            up: Some(1),
            ambient: 300.0,
            counts: false,
        };
        assert_eq!(simulate_adjacent(&tile, &mut arena), 0.0);
        assert!(arena.0[1].heat > 300.0);
    }

    #[test]
    fn simulate_is_adjacent_then_environment() {
        let mut hot = Capacitor::new(1.0, 5.0, 10.0, 300.0);
        hot.handle(1_000.0);
        let cold = Capacitor::new(1.0, 5.0, 10.0, 300.0);
        let mut arena = Arena(vec![hot, cold]);
        let tile = Tile {
            own: 0,
            up: Some(1),
            ambient: 300.0,
            counts: true,
        };
        let mut by_parts = Arena(vec![hot, cold]);
        let adjacent = simulate_adjacent(&tile, &mut by_parts);
        let environment = simulate_environment(&tile, &mut by_parts);
        assert_eq!(
            simulate(&tile, &mut arena),
            HeatTransfer {
                adjacent,
                environment
            }
        );
        assert_eq!(arena.0, by_parts.0);
    }
}
