//! Os valores de configuração que todo tile de máquina lê, e como ele os guarda. O original os lê da configuração a cada uso
//! (`blockDeactivationDelay` é um `IntSupplier` que o tile guarda, `maxUpgradeMultiplier` é lido em cada conta de `UpgradeUtils`), então uma mudança do
//! arquivo, que o jogo relê a cada segundo, vale no uso seguinte. O mod que lê o arquivo (`Mekanism/general.toml`) diz o valor com
//! [`set_machine_settings`], e as máquinas de qualquer mod que use a `machinery_basis` o leem daqui.
use std::sync::RwLock;

/// Um valor de configuração como um objeto o guarda: fixo (os testes e os padrões) ou lido do valor de agora a cada uso, que é o fornecedor do original.
#[derive(Clone, Copy, Debug)]
pub enum ConfigValue<T: Copy> {
    Fixed(T),
    Live(fn() -> T),
}

impl<T: Copy> ConfigValue<T> {
    pub fn get(&self) -> T {
        match self {
            Self::Fixed(value) => *value,
            Self::Live(read) => read(),
        }
    }
}

impl<T: Copy> From<T> for ConfigValue<T> {
    fn from(value: T) -> Self {
        Self::Fixed(value)
    }
}

/// O que as máquinas leem de `GeneralConfig`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MachineSettings {
    /// `blockDeactivationDelay` (`GeneralConfig.java`: `defineInRange(..., 3 * TICKS_PER_SECOND, 0, TICKS_PER_MINUTE)`).
    pub block_deactivation_delay: i32,
    /// `maxUpgradeMultiplier` (`defineInRange(..., 10, 1, Integer.MAX_VALUE)`).
    pub max_upgrade_multiplier: i32,
    /// `transmitterAlloyUpgrade` (`define(..., true)`): `ItemAlloy.useOn` lê a cada clique; desligada, a liga passa por qualquer bloco.
    pub transmitter_alloy_upgrade: bool,
}

impl MachineSettings {
    /// Os padrões do original, que valem enquanto nenhum arquivo foi lido.
    pub const DEFAULT: Self = Self {
        block_deactivation_delay: 60,
        max_upgrade_multiplier: 10,
        transmitter_alloy_upgrade: true,
    };
}

static MACHINE: RwLock<MachineSettings> = RwLock::new(MachineSettings::DEFAULT);

/// Guarda o que o arquivo de configuração geral disse.
pub fn set_machine_settings(settings: MachineSettings) {
    *MACHINE.write().unwrap_or_else(|error| error.into_inner()) = settings;
}

/// O que vale agora.
pub fn machine_settings() -> MachineSettings {
    *MACHINE.read().unwrap_or_else(|error| error.into_inner())
}

/// `MekanismConfig.general.transmitterAlloyUpgrade`.
pub fn transmitter_alloy_upgrade() -> bool {
    machine_settings().transmitter_alloy_upgrade
}

/// `MekanismConfig.general.blockDeactivationDelay`.
pub fn block_deactivation_delay() -> i32 {
    machine_settings().block_deactivation_delay
}

/// `MekanismConfig.general.maxUpgradeMultiplier`.
pub fn max_upgrade_multiplier() -> i32 {
    machine_settings().max_upgrade_multiplier
}

/// `maxUpgradeMultiplier` como o `double` das contas de `UpgradeUtils`.
pub fn max_upgrade_multiplier_f64() -> f64 {
    f64::from(max_upgrade_multiplier())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fixed_value_stays_and_a_live_one_follows_the_settings() {
        let fixed: ConfigValue<i32> = 5.into();
        assert_eq!(fixed.get(), 5);
        assert_eq!(MachineSettings::DEFAULT.block_deactivation_delay, 60);
        assert_eq!(MachineSettings::DEFAULT.max_upgrade_multiplier, 10);
    }
}
