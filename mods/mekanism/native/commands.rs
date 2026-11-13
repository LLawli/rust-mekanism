// `CommandMek` no jogo: o nó `mek` que o port tem (`debug`), sobre os comandos de `machinery_basis`. Código nosso no `Game`.
use crate::game::Game;
use crate::game::machinery_basis as basis;
use crate::game::machinery_commands as commands;
use crate::game::mekanism_rules as rules;
use horizon_client::mod_api::{NativeCommand, NativeServerPlayer};

/// O dono da árvore no registro de comandos.
const OWNER: &str = "mekanism";

/// `DebugCommand`: `MekanismAPI.debug = !MekanismAPI.debug` e `sendSuccess(() -> COMMAND_DEBUG.translateColored(GRAY, OnOff.of(MekanismAPI.debug, true)), true)`.
fn debug(game: &mut Game<'_>, _player: &NativeServerPlayer) -> bool {
    let on = basis::debug_toggle();
    let message = crate::game::mekanism_configurator::network_text(&rules::network_reader::debug_toggle_message(on));
    commands::send_success(game, message, true);
    true
}

/// O gancho `register_commands`: a árvore do `/mek` e os usos dela.
pub(in crate::game) fn register_commands(out: &mut Vec<NativeCommand>) {
    commands::register(OWNER, rules::command::tree::<commands::Run>(debug));
    commands::collect(OWNER, out);
}

/// O gancho `perform_command`.
pub(in crate::game) fn perform_command(game: &mut Game<'_>, player: &NativeServerPlayer, command: &str, ran: &mut bool) {
    commands::perform(OWNER, game, player, command, ran);
}
