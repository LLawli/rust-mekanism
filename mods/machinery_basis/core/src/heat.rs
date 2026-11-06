//! Calor e temperatura ambiente, como o `HeatAPI` do Mekanism: temperatura é calor dividido pela
//! capacidade, e o que está mais quente que o ambiente perde calor para o ar por cada face.

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
