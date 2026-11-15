//! `blockDeactivationDelay` e `maxUpgradeMultiplier` de `Mekanism/general.toml` são lidos no uso, como no original: o tile guarda o fornecedor do atraso
//! (`TileEntityMekanism.delaySupplier`) e `UpgradeUtils` lê o multiplicador a cada conta. Uma máquina que já existe vê a mudança do arquivo na próxima
//! vez que usa o valor. Um só teste neste arquivo: a configuração é do processo, e os testes de um arquivo andam em paralelo.
mod common;

use machinery_basis_core::cached_recipe::{ItemIngredient, ItemOutput, ItemToItemRecipe};
use machinery_basis_core::settings::{MachineSettings, machine_settings, set_machine_settings};
use machinery_basis_core::tile::ProgressMachine;
use mekanism_rules::{
    ENRICHMENT_USAGE, GENERAL_CONFIG, SPEED_UPGRADE, electric_config,
    machine_settings as settings_of,
};
use std::rc::Rc;

fn machine() -> ProgressMachine {
    let recipe = Rc::new(ItemToItemRecipe {
        id: "mekanism:test/clay_ball".to_owned(),
        input: ItemIngredient::item("clay"),
        input_count: 1,
        output: ItemOutput {
            resource: common::item("clay_ball"),
            count: 1,
        },
    });
    let mut config = electric_config(
        ENRICHMENT_USAGE,
        ItemToItemRecipe::cache_of(vec![recipe]),
        0,
        common::upgrade_data(),
    );
    config.items = common::item_energy();
    ProgressMachine::electric(config)
}

/// Instala ou tira velocidade até o total (cada mudança é um `recalculateUpgrades`).
fn install(machine: &ProgressMachine, total: i32) {
    let upgrades = machine.upgrades();
    while upgrades.get_upgrades(SPEED_UPGRADE) > total {
        upgrades.remove_upgrade(SPEED_UPGRADE, false);
    }
    let missing = total - upgrades.get_upgrades(SPEED_UPGRADE);
    if missing > 0 {
        upgrades.add_upgrades(SPEED_UPGRADE, missing);
    }
}

/// O atraso do bloco depois que a máquina para (`setActive(false)` põe `updateDelay = blockDeactivationDelay.getAsInt()`).
fn delay_after_stopping(machine: &ProgressMachine) -> i32 {
    let mut tile = machine.tile_mut();
    tile.set_active(true);
    tile.set_active(false);
    tile.update_delay()
}

#[test]
fn a_machine_that_exists_reads_the_general_config_each_time() {
    assert_eq!(machine_settings(), MachineSettings::DEFAULT);
    let machine = machine();
    assert_eq!(delay_after_stopping(&machine), 60);
    // 8 de velocidade com o multiplicador 10: `200 * 10^(-8/8)`.
    install(&machine, 8);
    assert_eq!(
        (machine.ticks_required(), machine.operations_per_tick()),
        (20, 1)
    );

    // O arquivo muda: `maxUpgradeMultiplier = 1000` e `blockDeactivationDelay = 5`, lidos como o jogo os lê do texto.
    let loaded = GENERAL_CONFIG.load("blockDeactivationDelay = 5\nmaxUpgradeMultiplier = 1000\n");
    set_machine_settings(settings_of(&loaded));
    // O atraso vale no próximo `setActive`; os upgrades, na próxima conta (`recalculateUpgrades`). Sete de oito: `200 * 1000^(-7/8) = 0,47`.
    install(&machine, 7);
    assert_eq!(machine.ticks_required(), 1);
    assert_eq!(
        machine.operations_per_tick(),
        2,
        "mais de uma operação por tick, o que o padrão 10 nunca dá"
    );
    install(&machine, 8);
    assert_eq!(machine.operations_per_tick(), 5);
    let other = self::machine();
    assert_eq!(delay_after_stopping(&other), 5);
    assert_eq!(
        delay_after_stopping(&machine),
        5,
        "a máquina velha lê o valor novo"
    );

    // Volta ao padrão: nada fica guardado nas máquinas.
    set_machine_settings(MachineSettings::DEFAULT);
    install(&machine, 7);
    assert_eq!(
        (machine.ticks_required(), machine.operations_per_tick()),
        (200.0_f64.mul_add(10_f64.powf(-7.0 / 8.0), 0.0) as i32, 1)
    );
    assert_eq!(delay_after_stopping(&machine), 60);
}
