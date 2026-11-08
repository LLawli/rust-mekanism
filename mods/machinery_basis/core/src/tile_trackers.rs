//! A lista de dados sincronizados que as classes de tile do Mekanism acrescentam ao menu (`addContainerTrackers`), na ordem do
//! Java: `TileEntityMekanism`, `TileEntityRecipeMachine`, `TileEntityProgressMachine` e a janela de upgrades
//! (`TileComponentUpgrade.getSpecificSyncableData`). Cada classe de máquina acrescenta os dela depois (o Heat Generator, a produção e
//! as duas perdas). A lib não sabe de onde vem o valor de cada dado: a [`source`] só dá nomes às fontes que a estrutura do tile
//! tem, e quem usa implementa o [`crate::container_sync::TrackedSource`] sobre o tile vivo.
use crate::container_sync::{Kind, Tracked};

/// Os identificadores de fonte da estrutura do tile. As máquinas acrescentam as delas a partir de [`MOD`].
pub mod source {
    /// O `SyncableStreamCodec.frequency` do componente de frequência (a de segurança, que todo bloco com segurança tem).
    pub const SECURITY_FREQUENCY: u16 = 0;
    /// O `SyncableEnum` do `TileComponentSecurity`.
    pub const SECURITY_MODE: u16 = 1;
    /// `controlType`, `redstone` (`isPowered`) e `redstoneLastTick` (`wasPowered`).
    pub const CONTROL_TYPE: u16 = 2;
    pub const POWERED: u16 = 3;
    pub const WAS_POWERED: u16 = 4;
    /// `getHeat` e `getHeatCapacity` do capacitor.
    pub const HEAT: u16 = 5;
    pub const HEAT_CAPACITY: u16 = 6;
    /// `LastEnergyTracker.getLastEnergyReceived`, a capacidade (`getCapacityAsLong`), o consumo (`getEnergyPerTick`) e a energia.
    pub const LAST_ENERGY_RECEIVED: u16 = 7;
    pub const MAX_ENERGY: u16 = 8;
    pub const ENERGY_PER_TICK: u16 = 9;
    pub const ENERGY: u16 = 10;
    /// `hasXpContainment`.
    pub const HAS_XP_CONTAINMENT: u16 = 11;
    /// `operatingTicks` (o monitor) e `getTicksRequired`.
    pub const OPERATING_TICKS: u16 = 12;
    pub const TICKS_REQUIRED: u16 = 13;
    /// `upgradeTicks` e o mapa de upgrades da janela.
    pub const UPGRADE_TICKS: u16 = 14;
    pub const UPGRADE_MAP: u16 = 15;
    /// Os tanques de químico e de fluido, do primeiro (`+ 0`) em diante, e os erros rastreados (`trackedErrors[i]`).
    pub const CHEMICAL_TANK: u16 = 0x100;
    pub const FLUID_TANK: u16 = 0x200;
    pub const TRACKED_ERROR: u16 = 0x300;
    /// A primeira fonte que uma máquina define.
    pub const MOD: u16 = 0x1000;
}

/// O que `TileEntityMekanism.addContainerTrackers` olha no tile para decidir o que sincroniza.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TileTrackers {
    /// `hasSecurity()`: o componente de segurança (e com ele a frequência de segurança).
    pub security: bool,
    /// `supportsRedstone()`.
    pub redstone: bool,
    /// Os tanques que `syncs(CHEMICAL)` e `syncs(FLUID)` deixam sincronizar.
    pub chemical_tanks: usize,
    pub fluid_tanks: usize,
    /// `canHandleHeat() && syncs(HEAT)`.
    pub heat: bool,
    /// `canHandleEnergy() && syncs(ENERGY)`.
    pub energy: Option<EnergyTrackers>,
}

/// O que o contêiner de energia pede além da energia.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EnergyTrackers {
    /// O tile tem `LastEnergyTracker` (todo tile com `EnergyHandlerManager`).
    pub last_energy: bool,
    /// `MachineEnergyContainer` num tile que `supportsUpgrades()` (ou de taxas ajustáveis): a capacidade e o consumo vão antes da
    /// energia, porque os upgrades os mudam e o cliente não pode cortar a energia no máximo antigo.
    pub adjustable: bool,
}

impl TileTrackers {
    /// `TileEntityMekanism.addContainerTrackers`. O `SyncMapper` (`@ContainerSync`) não acrescenta nada: nenhuma classe de tile
    /// de máquina usa a anotação, só os dados de multibloco.
    pub fn trackers(&self) -> Vec<Tracked> {
        let mut list = Vec::new();
        // Os componentes, na ordem em que o construtor os cria: a frequência, depois a segurança.
        if self.security {
            list.push(Tracked::new(source::SECURITY_FREQUENCY, Kind::StreamCodec));
            list.push(Tracked::new(
                source::SECURITY_MODE,
                Kind::Enum { default: 0 },
            ));
        }
        if self.redstone {
            list.push(Tracked::new(
                source::CONTROL_TYPE,
                Kind::Enum { default: 0 },
            ));
            list.push(Tracked::new(source::POWERED, Kind::Boolean));
            list.push(Tracked::new(source::WAS_POWERED, Kind::Boolean));
        }
        for index in 0..self.chemical_tanks {
            list.push(Tracked::new(
                source::CHEMICAL_TANK + index as u16,
                Kind::LargeResourceStack,
            ));
        }
        for index in 0..self.fluid_tanks {
            list.push(Tracked::new(
                source::FLUID_TANK + index as u16,
                Kind::LargeResourceStack,
            ));
        }
        if self.heat {
            list.push(Tracked::new(source::HEAT, Kind::Double));
            list.push(Tracked::new(source::HEAT_CAPACITY, Kind::Double));
        }
        if let Some(energy) = self.energy {
            if energy.last_energy {
                list.push(Tracked::new(source::LAST_ENERGY_RECEIVED, Kind::Long));
            }
            if energy.adjustable {
                list.push(Tracked::new(source::MAX_ENERGY, Kind::Long));
                list.push(Tracked::new(source::ENERGY_PER_TICK, Kind::Int));
            }
            list.push(Tracked::new(source::ENERGY, Kind::Long));
        }
        list
    }
}

/// `TileEntityRecipeMachine.addContainerTrackers`: `trackArray(trackedErrors)` (um booleano por erro rastreado) e `hasXpContainment`.
pub fn recipe_machine_trackers(tracked_errors: usize) -> Vec<Tracked> {
    (0..tracked_errors)
        .map(|index| Tracked::new(source::TRACKED_ERROR + index as u16, Kind::Boolean))
        .chain([Tracked::new(source::HAS_XP_CONTAINMENT, Kind::Boolean)])
        .collect()
}

/// `TileEntityProgressMachine.addContainerTrackers`: `operatingTicks` e `ticksRequired`.
pub fn progress_machine_trackers() -> [Tracked; 2] {
    [
        Tracked::new(source::OPERATING_TICKS, Kind::Int),
        Tracked::new(source::TICKS_REQUIRED, Kind::Int),
    ]
}

/// `TileComponentUpgrade.getSpecificSyncableData`: o que a janela de upgrades acrescenta ao menu.
pub fn upgrade_window_trackers() -> [Tracked; 2] {
    [
        Tracked::new(source::UPGRADE_TICKS, Kind::Int),
        Tracked::new(source::UPGRADE_MAP, Kind::StreamCodec),
    ]
}

/// `MekanismContainer.UPGRADE_WINDOW`: a chave de `startTracking` da janela.
pub const UPGRADE_WINDOW_KEY: u8 = 2;

#[cfg(test)]
mod tests {
    use super::*;

    fn classes(list: &[Tracked]) -> Vec<&'static str> {
        list.iter().map(|tracked| tracked.kind.class()).collect()
    }

    #[test]
    fn the_tile_base_follows_the_order_of_add_container_trackers() {
        let electric = TileTrackers {
            security: true,
            redstone: true,
            energy: Some(EnergyTrackers {
                last_energy: true,
                adjustable: true,
            }),
            ..TileTrackers::default()
        };
        let list = electric.trackers();
        assert_eq!(
            classes(&list),
            [
                "SyncableStreamCodec",
                "SyncableEnum",
                "SyncableEnum",
                "SyncableBoolean",
                "SyncableBoolean",
                "SyncableLong",
                "SyncableLong",
                "SyncableInt",
                "SyncableLong",
            ]
        );
        assert_eq!(list[7].source, source::ENERGY_PER_TICK);
        assert_eq!(list[8].source, source::ENERGY);
    }

    #[test]
    fn tanks_and_heat_come_between_redstone_and_energy() {
        let generator = TileTrackers {
            security: true,
            redstone: true,
            fluid_tanks: 1,
            heat: true,
            energy: Some(EnergyTrackers {
                last_energy: true,
                adjustable: false,
            }),
            ..TileTrackers::default()
        };
        let sources: Vec<u16> = generator.trackers().iter().map(|t| t.source).collect();
        assert_eq!(
            sources,
            [
                source::SECURITY_FREQUENCY,
                source::SECURITY_MODE,
                source::CONTROL_TYPE,
                source::POWERED,
                source::WAS_POWERED,
                source::FLUID_TANK,
                source::HEAT,
                source::HEAT_CAPACITY,
                source::LAST_ENERGY_RECEIVED,
                source::ENERGY,
            ]
        );
    }

    #[test]
    fn a_tile_without_the_features_tracks_nothing() {
        assert!(TileTrackers::default().trackers().is_empty());
    }

    #[test]
    fn the_recipe_and_progress_layers_append_in_order() {
        let errors = recipe_machine_trackers(4);
        assert_eq!(errors.len(), 5);
        assert_eq!(errors[3].source, source::TRACKED_ERROR + 3);
        assert_eq!(errors[4].source, source::HAS_XP_CONTAINMENT);
        assert_eq!(
            progress_machine_trackers()[0].source,
            source::OPERATING_TICKS
        );
        assert_eq!(upgrade_window_trackers()[1].kind, Kind::StreamCodec);
    }
}
