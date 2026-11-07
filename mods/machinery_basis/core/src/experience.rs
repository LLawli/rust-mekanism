//! A experiência que as receitas dão: `AbstractFurnaceBlockEntity.createExperience`, os pontos de uma remessa de receitas.

/// `Mth.floor(float)`.
fn floor(value: f32) -> i32 {
    let truncated = value as i32;
    if value < truncated as f32 {
        truncated - 1
    } else {
        truncated
    }
}

/// `AbstractFurnaceBlockEntity.createExperience`: os pontos de `crafted` operações de uma receita que dá `experience` cada uma.
/// A parte fracionária vira mais um ponto com essa chance: `random` é o `Math.random()` do Java, em `[0, 1)`.
pub fn experience_points(crafted: i32, experience: f32, random: f64) -> i32 {
    let total = crafted as f32 * experience;
    let mut points = floor(total);
    let fraction = total - floor(total) as f32;
    if fraction != 0.0 && random < f64::from(fraction) {
        points += 1;
    }
    points
}

/// Uma receita usada, com quantas operações dela saíram e quanto cada uma dá: o que `awardExperience` recebe.
#[derive(Clone, Debug, PartialEq)]
pub struct ExperienceDrop {
    pub recipe: String,
    pub crafted: i32,
    pub experience: f32,
}

impl ExperienceDrop {
    /// `awardExperience` só age com `experience > 0`; o resto vira `ExperienceOrb.award` com esses pontos.
    pub fn points(&self, random: f64) -> i32 {
        if self.experience > 0.0 {
            experience_points(self.crafted, self.experience, random)
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_amounts_never_draw() {
        assert_eq!(experience_points(2, 0.5, 0.0), 1);
        assert_eq!(experience_points(2, 0.5, 0.999), 1);
        assert_eq!(experience_points(4, 0.5, 0.5), 2);
    }

    #[test]
    fn the_fraction_is_the_chance_of_one_more_point() {
        // 2 vezes 0,1 dá 0,2: sorteio abaixo de 0,2 sobe para 1.
        assert_eq!(experience_points(2, 0.1, 0.19), 1);
        // 2 * 0.1f32 é 0.2f32, um pouco acima de 0.2 em double: 0.2 ainda sorteia o ponto.
        assert_eq!(experience_points(2, 0.1, 0.2), 1);
        assert_eq!(experience_points(2, 0.1, 0.21), 0);
        assert_eq!(experience_points(3, 0.5, 0.49), 2);
        assert_eq!(experience_points(3, 0.5, 0.5), 1);
    }

    #[test]
    fn a_recipe_without_experience_drops_nothing() {
        let drop = ExperienceDrop {
            recipe: "a".into(),
            crafted: 5,
            experience: 0.0,
        };
        assert_eq!(drop.points(0.0), 0);
    }
}
