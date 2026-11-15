//! O que a receita `mekanism:mek_data` copia das entradas para o resultado (`MekanismShapedRecipe.assemble`, `RecipeUpgradeData` e as classes `*RecipeData`).
//!
//! A receita junta, por tipo de dado (`RecipeUpgradeType`, na ordem do enum), o que cada entrada com componente carrega, funde os dados do mesmo tipo (`merge`) na ordem das
//! entradas, e aplica o resultado à saída (`applyToStack`). Um `merge` que dá `null` ou um `applyToStack` que dá `false` zera a receita (`ItemStack.EMPTY`), e a receita
//! deixa de casar (`WrappedShapedRecipe.matches` confere que a montagem não é vazia).
//!
//! Aqui está a regra de cada tipo que não depende de como o item guarda o dado: a ordem, a energia (soma saturada e o teto da capacidade da saída), a segurança (o mesmo
//! dono, o modo mais restritivo), a trava e a ordenação. O que o item carrega (o componente de energia, o dono) e os tipos que precisam dos contêineres do item
//! (tanques de fluido e de químico, itens do depósito, upgrades, o drive do QIO) esperam o dado existir no port (energia do item: Fase 2; segurança: Fase 5; tanques e
//! depósito: Fase 6; QIO: Fase 8). O original não copia a configuração de lados nem o ejetor (o `TODO` de `RecipeUpgradeData.getSupportedTypes`): só os tipos abaixo.

/// `RecipeUpgradeType`, na ordem do enum, que é a ordem em que a receita aplica os tipos (`EnumMap`): a segurança antes do item, a trava depois dele.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RecipeUpgradeType {
    Security,
    Energy,
    Fluid,
    Chemical,
    Item,
    Lock,
    Sorting,
    Upgrade,
    QioDrive,
}

impl RecipeUpgradeType {
    /// A ordem do enum.
    pub const ORDER: [RecipeUpgradeType; 9] = [
        RecipeUpgradeType::Security,
        RecipeUpgradeType::Energy,
        RecipeUpgradeType::Fluid,
        RecipeUpgradeType::Chemical,
        RecipeUpgradeType::Item,
        RecipeUpgradeType::Lock,
        RecipeUpgradeType::Sorting,
        RecipeUpgradeType::Upgrade,
        RecipeUpgradeType::QioDrive,
    ];
}

/// `MathUtils.addClamped(long, long)`: a soma, ou `Long.MAX_VALUE` se estoura.
pub fn add_clamped(x: i64, y: i64) -> i64 {
    x.checked_add(y).unwrap_or(i64::MAX)
}

/// `RecipeUpgradeData.getUpgradeData` do tipo `ENERGY`: uma entrada sem energia não dá dado (`energy == 0 ? null`).
pub fn energy_of_input(energy: i64) -> Option<i64> {
    (energy != 0).then_some(energy)
}

/// `EnergyRecipeData.merge`, em cadeia (`RecipeUpgradeData.mergeUpgradeData`): a soma saturada das energias das entradas que têm dado, na ordem das entradas. `None` sem nenhum dado.
pub fn merge_energy(inputs: &[i64]) -> Option<i64> {
    let mut data = inputs.iter().copied().filter_map(energy_of_input);
    let first = data.next()?;
    Some(data.fold(first, add_clamped))
}

/// `EnergyRecipeData.applyToStack`: a energia da saída depois de a receita pôr `total`. Com `total == 0` não mexe (`return true`); senão soma o que cabe
/// (`min(capacidade - guardado, total)`, só se positivo) e o excedente se perde, sem falhar ("we allow voiding excess energy for upgrade recipes").
pub fn apply_energy(total: i64, capacity: i64, stored: i64) -> i64 {
    if total == 0 {
        return stored;
    }
    let to_add = (capacity - stored).min(total);
    if to_add > 0 { stored + to_add } else { stored }
}

/// `SecurityMode`, na ordem do enum. A conta do modo (próximo, nome, cor) está em `security`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecurityMode {
    Public,
    Private,
    Trusted,
}

/// `SecurityUtils.moreRestrictive(base, overridden)`: se `overridden` é mais restritivo que `base`. O `PUBLIC` nunca é; o `PRIVATE` é se `base` não é `PRIVATE`; o `TRUSTED` só
/// se `base` é `PUBLIC`.
pub fn more_restrictive(base: SecurityMode, overridden: SecurityMode) -> bool {
    match overridden {
        SecurityMode::Public => false,
        SecurityMode::Private => base != SecurityMode::Private,
        SecurityMode::Trusted => base == SecurityMode::Public,
    }
}

/// `SecurityRecipeData`: o dono e o modo de uma entrada.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecurityData<Owner> {
    pub owner: Owner,
    pub mode: SecurityMode,
}

impl<Owner: PartialEq + Clone> SecurityData<Owner> {
    /// `SecurityRecipeData.merge`: o mesmo dono, com o modo mais restritivo (`moreRestrictive(mode, other.mode) ? other : this`); donos diferentes dão `None` e a receita falha.
    pub fn merge(&self, other: &Self) -> Option<Self> {
        if self.owner == other.owner {
            Some(if more_restrictive(self.mode, other.mode) {
                other.clone()
            } else {
                self.clone()
            })
        } else {
            None
        }
    }
}

/// `RecipeUpgradeData.mergeUpgradeData` para os tipos cujo `merge` pode falhar: `None` se a lista é vazia ou se algum `merge` falha.
pub fn merge_all<T>(data: &[T], merge: impl Fn(&T, &T) -> Option<T>) -> Option<T>
where
    T: Clone,
{
    let mut merged = data.first()?.clone();
    for next in &data[1..] {
        merged = merge(&merged, next)?;
    }
    Some(merged)
}

/// `LockRecipeData.merge`: duas travas iguais dão a primeira; senão a receita falha.
pub fn merge_lock<Lock: PartialEq + Clone>(first: &Lock, other: &Lock) -> Option<Lock> {
    (first == other).then(|| first.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_energy_adds_up_saturated() {
        assert_eq!(merge_energy(&[0, 0]), None);
        assert_eq!(merge_energy(&[1000, 0, 5000, 2500]), Some(8500));
        assert_eq!(merge_energy(&[i64::MAX, 5]), Some(i64::MAX));
    }

    #[test]
    fn the_output_takes_what_fits_and_loses_the_rest() {
        assert_eq!(apply_energy(1234, 30, 0), 30);
        assert_eq!(apply_energy(0, 30, 7), 7);
        assert_eq!(apply_energy(10, 30, 25), 30);
        assert_eq!(apply_energy(10, 30, 30), 30);
    }

    #[test]
    fn the_security_mode_only_gets_stricter() {
        use SecurityMode::*;
        let a = SecurityData {
            owner: 1,
            mode: Public,
        };
        let b = SecurityData {
            owner: 1,
            mode: Trusted,
        };
        let c = SecurityData {
            owner: 1,
            mode: Private,
        };
        assert_eq!(a.merge(&b).unwrap().mode, Trusted);
        assert_eq!(b.merge(&c).unwrap().mode, Private);
        assert_eq!(c.merge(&b).unwrap().mode, Private);
        assert_eq!(c.merge(&a).unwrap().mode, Private);
        assert_eq!(
            a.merge(&SecurityData {
                owner: 2,
                mode: Public
            }),
            None
        );
    }
}
