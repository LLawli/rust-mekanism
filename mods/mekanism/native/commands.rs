// `CommandMek` no jogo: o nó `mek` que o port tem (`debug` e `radiation`), sobre os comandos de `machinery_basis`. Código nosso no `Game`.
use crate::game::Game;
use crate::game::commands::Target;
use crate::game::machinery_basis as basis;
use crate::game::machinery_commands as commands;
use crate::game::machinery_radiation as radiation;
use crate::game::mekanism_configurator::network_text_with;
use crate::game::mekanism_rules as rules;
use horizon_client::command::args::{Arg as CmdArg, ArgType};
use horizon_client::command::{CommandError, Reader};
use horizon_client::mod_api::{NativeCommand, NativeServerPlayer};
use horizon_client::text::Text;
use rules::network_reader::{Arg, Body, Comp};
use rules::radiation_command as messages;
use std::cell::RefCell;

/// O dono da árvore no registro de comandos.
const OWNER: &str = "mekanism";
/// `DoubleArgumentType.doubleArg(Double.MIN_VALUE, 10_000)`.
const MAGNITUDE_MIN: f64 = f64::from_bits(1);
const MAGNITUDE_MAX: f64 = 10_000.0;

thread_local! {
    /// A linha inteira do comando que roda, para a linha de contexto de um erro num argumento.
    static LINE: RefCell<String> = const { RefCell::new(String::new()) };
}

/// `DebugCommand`: `MekanismAPI.debug = !MekanismAPI.debug` e `sendSuccess(() -> COMMAND_DEBUG.translateColored(GRAY, OnOff.of(MekanismAPI.debug, true)), true)`.
fn debug(game: &mut Game<'_>, _player: &NativeServerPlayer, _args: &str) -> bool {
    let on = basis::debug_toggle();
    let message = crate::game::mekanism_configurator::network_text(&rules::network_reader::debug_toggle_message(on));
    commands::send_success(game, message, true);
    true
}

// ---------------------------------------------------------------------------------------------------------------------------
// `RadiationCommand`.

/// Os argumentos depois do literal, lidos como o Brigadier os lê (`ArgumentType.parse` de cada um, o separador entre eles).
struct Args<'a> {
    reader: Reader<'a>,
    registries: horizon_client::command::Registries,
}

impl<'a> Args<'a> {
    fn new(game: &Game<'_>, player: &NativeServerPlayer, line: &'a str) -> Self {
        let mut registries = game.ui.registries.clone();
        registries.permission = player.permission;
        Self { reader: Reader::new(line), registries }
    }

    fn read(&mut self, ty: ArgType) -> Result<CmdArg, CommandError> {
        ty.parse(&mut self.reader, &self.registries)
    }

    fn magnitude(&mut self) -> Result<f64, CommandError> {
        match self.read(ArgType::Double { min: MAGNITUDE_MIN, max: MAGNITUDE_MAX })? {
            CmdArg::Double(value) => Ok(value),
            _ => Err(CommandError::tr("argument.double.invalid", &[])),
        }
    }

    /// O separador antes do próximo argumento: `true` se há um.
    fn more(&mut self) -> Result<bool, CommandError> {
        if !self.reader.can_read() {
            return Ok(false);
        }
        if self.reader.peek() != ' ' || !self.reader.can_read_n(2) {
            return Err(CommandError::at(Text::translate("command.unknown.argument", &[]), &self.reader));
        }
        self.reader.skip();
        Ok(true)
    }

    /// O fim: sobra texto que nenhum nó lê.
    fn end(&mut self) -> Result<(), CommandError> {
        if self.more()? {
            return Err(CommandError::at(Text::translate("command.unknown.argument", &[]), &self.reader));
        }
        Ok(())
    }
}

/// O erro de um argumento: a mensagem em vermelho e a linha de contexto, com o cursor contado na linha inteira.
fn fail(game: &mut Game<'_>, args: &str, error: &CommandError) {
    let line = LINE.with_borrow(Clone::clone);
    let offset = line.len().saturating_sub(args.len());
    let message = double_bound_message(args, error).unwrap_or_else(|| error.message.clone());
    match error.input.as_ref() {
        Some(_) => commands::argument_failure(game, message, &line, offset + error.cursor),
        None => commands::send_failure(game, message),
    }
}

/// `DoubleArgumentType` fora da faixa (`argument.double.low` e `argument.double.big`): o Brigadier escreve os dois números com o `Double.toString` do Java
/// (`4.9E-324` é o `Double.MIN_VALUE`); o leitor do Horizon os escreve com a forma curta do Rust (`5.0E-324`). A mensagem sai de novo com a do Java.
fn double_bound_message(args: &str, error: &CommandError) -> Option<Text> {
    let bound = match error.key.as_str() {
        "argument.double.low" => MAGNITUDE_MIN,
        "argument.double.big" => MAGNITUDE_MAX,
        _ => return None,
    };
    let value: f64 = args.get(error.cursor..)?.split(' ').next()?.parse().ok()?;
    let java = machinery_basis_core::units::java_double_to_string;
    Some(Text::translate(&error.key, &[Text::literal(java(bound)), Text::literal(java(value))]))
}

fn block_of(position: [f64; 3]) -> [i32; 3] {
    position.map(|v| v.floor() as i32)
}

/// `Vec3Argument.vec3()` e, se vier, `DimensionArgument.dimension()` (um nível que o servidor tem).
fn location(game: &Game<'_>, player: &NativeServerPlayer, args: &mut Args<'_>) -> Result<([i32; 3], String), CommandError> {
    let start = args.reader.cursor;
    let coords = match args.read(ArgType::Vec3 { center: true }) {
        Ok(CmdArg::Coords(coords)) => coords,
        // `WorldCoordinates.parseDouble` volta o cursor ao começo do argumento antes do `ERROR_NOT_COMPLETE` (o original aponta o começo; o leitor do
        // Horizon, o fim do que leu).
        Err(mut error) if error.key == "argument.pos3d.incomplete" => {
            error.cursor = start;
            return Err(error);
        }
        Err(error) => return Err(error),
        Ok(_) => return Err(CommandError::tr("argument.pos3d.incomplete", &[])),
    };
    let block = block_of(coords.position(player.position, [player.yaw, player.pitch]));
    if !args.more()? {
        return Ok((block, player.dimension.clone()));
    }
    let CmdArg::Resource(id) = args.read(ArgType::Dimension)? else {
        return Err(CommandError::tr("argument.dimension.invalid", &[]));
    };
    let id = if id.contains(':') { id } else { format!("minecraft:{id}") };
    if !game.ui.registries.dimensions.iter().any(|known| *known == id) {
        return Err(CommandError::tr("argument.dimension.invalid", &[&id as &dyn std::fmt::Display]));
    }
    args.end()?;
    Ok((block, id))
}

/// `EntityArgument.entities()` e `getEntities`: nenhuma entidade é o erro `argument.entity.notfound.entity`.
fn targets(game: &mut Game<'_>, args: &mut Args<'_>) -> Result<Vec<Target>, CommandError> {
    let CmdArg::Entity(selector) = args.read(ArgType::Entity { single: false, players: false })? else {
        return Err(CommandError::tr("argument.entity.invalid", &[]));
    };
    let found = game.select(&selector);
    if found.is_empty() {
        return Err(CommandError::tr("argument.entity.notfound.entity", &[]));
    }
    Ok(found)
}

/// `targets`, o separador e a magnitude: `<targets> <magnitude>`.
fn targets_and_magnitude(game: &mut Game<'_>, args: &mut Args<'_>) -> Result<(Vec<Target>, f64), CommandError> {
    let found = targets(game, args)?;
    if !args.more()? {
        return Err(CommandError::at(Text::translate("command.unknown.command", &[]), &args.reader));
    }
    let magnitude = args.magnitude()?;
    args.end()?;
    Ok((found, magnitude))
}

/// Só a magnitude, com o fim da linha logo depois (o ramo `<magnitude>` de quem roda).
fn only_magnitude(game: &Game<'_>, player: &NativeServerPlayer, line: &str) -> Option<f64> {
    let mut args = Args::new(game, player, line);
    let magnitude = args.magnitude().ok()?;
    args.end().ok()?;
    Some(magnitude)
}

fn say(game: &mut Game<'_>, message: &Comp, names: &[Text]) {
    commands::send_success(game, network_text_with(message, names), true);
}

/// O nome de uma entidade na mensagem: o componente que o jogo monta (`getDisplayName`).
fn name_slot() -> Arg {
    Arg::Comp(Comp { body: Body::Slot(0), color: None })
}

/// `radiation add <magnitude> [<location> [<dimension>]]`.
fn radiation_add(game: &mut Game<'_>, player: &NativeServerPlayer, line: &str) -> bool {
    let mut args = Args::new(game, player, line);
    let parsed = args.magnitude().and_then(|magnitude| {
        let (pos, level) = if args.more()? { location(game, player, &mut args)? } else { (block_of(player.position), player.dimension.clone()) };
        Ok((magnitude, pos, level))
    });
    let (magnitude, pos, level) = match parsed {
        Ok(parsed) => parsed,
        Err(error) => {
            fail(game, line, &error);
            return false;
        }
    };
    radiation::radiate(game, &level, pos, magnitude);
    say(game, &messages::add(magnitude, pos, &level), &[]);
    true
}

/// `radiation get [<location> [<dimension>]]`.
fn radiation_get(game: &mut Game<'_>, player: &NativeServerPlayer, line: &str) -> bool {
    let (pos, level) = if line.is_empty() {
        (block_of(player.position), player.dimension.clone())
    } else {
        let mut args = Args::new(game, player, line);
        match location(game, player, &mut args) {
            Ok(found) => found,
            Err(error) => {
                fail(game, line, &error);
                return false;
            }
        }
    };
    let magnitude = radiation::level_at(game, &level, pos);
    say(game, &messages::get(pos, &level, magnitude), &[]);
    true
}

/// `radiation addEntity <magnitude>` (quem roda) e `radiation addEntity <targets> <magnitude>`: o Brigadier tenta o número antes do seletor.
fn radiation_add_entity(game: &mut Game<'_>, player: &NativeServerPlayer, line: &str) -> bool {
    if let Some(magnitude) = only_magnitude(game, player, line) {
        radiation::radiate_target(game, Target::Player, magnitude);
        say(game, &messages::add_entity(magnitude), &[]);
        return true;
    }
    let mut args = Args::new(game, player, line);
    let (found, magnitude) = match targets_and_magnitude(game, &mut args) {
        Ok(parsed) => parsed,
        Err(error) => {
            fail(game, line, &error);
            return false;
        }
    };
    let mut living = Vec::new();
    for target in found {
        if radiation::target_dose(game, target).is_some() {
            radiation::radiate_target(game, target, magnitude);
            living.push(target);
        }
    }
    let names: Vec<Text> = living.first().map(|&target| game.target_name(target)).into_iter().collect();
    say(game, &messages::add_entity_targets(magnitude, living.len(), Some(name_slot())), &names);
    true
}

/// `radiation heal` (quem roda) e `radiation heal <targets>`.
fn radiation_heal(game: &mut Game<'_>, player: &NativeServerPlayer, line: &str) -> bool {
    if line.is_empty() {
        radiation::set_target_dose(game, Target::Player, machinery_basis_core::radiation::BASELINE);
        say(game, &messages::clear(), &[]);
        return true;
    }
    let mut args = Args::new(game, player, line);
    let found = match targets(game, &mut args).and_then(|found| args.end().map(|()| found)) {
        Ok(found) => found,
        Err(error) => {
            fail(game, line, &error);
            return false;
        }
    };
    let mut living = Vec::new();
    for target in found {
        if radiation::target_dose(game, target).is_some() {
            radiation::set_target_dose(game, target, machinery_basis_core::radiation::BASELINE);
            living.push(target);
        }
    }
    let names: Vec<Text> = living.first().map(|&target| game.target_name(target)).into_iter().collect();
    say(game, &messages::clear_targets(living.len(), Some(name_slot())), &names);
    true
}

/// `radiation reduce <magnitude>` (quem roda) e `radiation reduce <targets> <magnitude>`: uma mensagem por entidade.
fn radiation_reduce(game: &mut Game<'_>, player: &NativeServerPlayer, line: &str) -> bool {
    if let Some(magnitude) = only_magnitude(game, player, line) {
        if let Some(dose) = radiation::target_dose(game, Target::Player) {
            let (after, reduced) = machinery_basis_core::radiation::reduce_dose(dose, magnitude);
            radiation::set_target_dose(game, Target::Player, after);
            say(game, &messages::reduce(reduced), &[]);
        }
        return true;
    }
    let mut args = Args::new(game, player, line);
    let (found, magnitude) = match targets_and_magnitude(game, &mut args) {
        Ok(parsed) => parsed,
        Err(error) => {
            fail(game, line, &error);
            return false;
        }
    };
    for target in found {
        if let Some(dose) = radiation::target_dose(game, target) {
            let (after, reduced) = machinery_basis_core::radiation::reduce_dose(dose, magnitude);
            radiation::set_target_dose(game, target, after);
            let name = game.target_name(target);
            say(game, &messages::reduce_target(name_slot(), reduced), &[name]);
        }
    }
    true
}

/// `radiation removeAll`.
fn radiation_remove_all(game: &mut Game<'_>, _player: &NativeServerPlayer, _line: &str) -> bool {
    radiation::clear_sources();
    say(game, &messages::remove_all(), &[]);
    true
}

/// O gancho `register_commands`: a árvore do `/mek` e os usos dela.
pub(in crate::game) fn register_commands(out: &mut Vec<NativeCommand>) {
    let radiation = rules::command::RadiationRuns {
        add: radiation_add as commands::Run,
        add_entity: radiation_add_entity,
        get: radiation_get,
        heal: radiation_heal,
        reduce: radiation_reduce,
        remove_all: radiation_remove_all,
    };
    commands::register(OWNER, rules::command::tree::<commands::Run>(debug, radiation));
    commands::collect(OWNER, out);
}

/// O gancho `perform_command`.
pub(in crate::game) fn perform_command(game: &mut Game<'_>, player: &NativeServerPlayer, command: &str, ran: &mut bool) {
    LINE.with_borrow_mut(|line| command.clone_into(line));
    commands::perform(OWNER, game, player, command, ran);
}
