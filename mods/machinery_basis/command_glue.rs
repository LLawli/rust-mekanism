// Os comandos de servidor de um mod no jogo (`Commands.register` e `CommandSourceStack`), sobre os ganchos `register_commands` e `perform_command` do Horizon. Código nosso no `Game`, módulo do
// `horizon-client`: a árvore e a conta do Brigadier são de `machinery_basis_core::commands`; aqui ficam o registro por dono (o mod), a lista de usos que o jogo anuncia, o `sendSuccess` e o
// `sendFailure` do Horizon e o erro de sintaxe com a linha de contexto.
use crate::game::Game;
use horizon_client::mod_api::{NativeCommand, NativeServerPlayer};
use horizon_client::text::{self, Style, Text};
use machinery_basis_core::commands::{CommandNode, Parsed, SyntaxError, parse, usages};
use std::sync::{Arc, Mutex};

/// O que um nó que roda faz: o `executes` do Brigadier (o valor de volta é `Command.SINGLE_SUCCESS`). O `&str` é o resto da linha de um nó com argumentos
/// (`CommandNode::arguments`), vazio num nó sem eles.
pub type Run = fn(&mut Game<'_>, &NativeServerPlayer, &str) -> bool;

/// A raiz de cada mod (`dono`), um comando por mod.
static ROOTS: Mutex<Vec<(&'static str, Arc<CommandNode<Run>>)>> = Mutex::new(Vec::new());

fn root_of(owner: &str) -> Option<Arc<CommandNode<Run>>> {
    let roots = ROOTS.lock().unwrap_or_else(|e| e.into_inner());
    roots.iter().find(|(name, _)| *name == owner).map(|(_, root)| Arc::clone(root))
}

/// Registra (ou troca) a árvore do mod.
pub(in crate::game) fn register(owner: &'static str, root: CommandNode<Run>) {
    let mut roots = ROOTS.lock().unwrap_or_else(|e| e.into_inner());
    roots.retain(|(name, _)| *name != owner);
    roots.push((owner, Arc::new(root)));
}

/// Os usos da árvore do mod, no formato do `register_commands`.
pub(in crate::game) fn collect(owner: &'static str, out: &mut Vec<NativeCommand>) {
    if let Some(root) = root_of(owner) {
        out.extend(usages(&root).iter().map(|usage| NativeCommand::op(&usage.text, usage.level)));
    }
}

/// O comando de um jogador (`perform_command`): `ran` quando a primeira palavra é a raiz do mod, rodando o nó ou escrevendo o erro do Brigadier.
pub(in crate::game) fn perform(owner: &'static str, game: &mut Game<'_>, player: &NativeServerPlayer, command: &str, ran: &mut bool) {
    let Some(root) = root_of(owner) else {
        return;
    };
    match parse(&root, command, player.permission) {
        None => {}
        Some(Parsed::Run(run)) => {
            *ran = true;
            run(game, player, "");
        }
        Some(Parsed::RunArgs(run, rest)) => {
            *ran = true;
            run(game, player, rest);
        }
        Some(Parsed::Error(error)) => {
            *ran = true;
            syntax_failure(game, &error);
        }
    }
}

/// `CommandSourceStack.sendSuccess(() -> message, broadcast)`: a origem, se aceita o retorno (`send_command_feedback`), e com `broadcast` os outros operadores (`[nome: mensagem]`).
pub(in crate::game) fn send_success(game: &mut Game<'_>, message: Text, broadcast: bool) {
    if broadcast {
        game.feedback(message);
    } else {
        game.feedback_unbroadcast(message);
    }
}

/// `CommandSourceStack.sendFailure`.
pub(in crate::game) fn send_failure(game: &mut Game<'_>, message: Text) {
    game.command_failure(message);
}

/// O erro de `Commands.performCommand` para uma `CommandSyntaxException`: a mensagem em vermelho e a linha de contexto (o trecho lido, o que sobra em vermelho sublinhado e
/// `command.context.here`), que sugere o comando ao clicar. É a mesma conta que o Horizon faz nos comandos dele.
fn syntax_failure(game: &mut Game<'_>, error: &SyntaxError) {
    argument_failure(game, Text::translate(error.key, &[]), &error.input, error.cursor);
}

/// Um erro de sintaxe com a linha de contexto (`CommandSyntaxException` com a entrada e o cursor): o de um nó, ou o de um argumento que o mod leu.
pub(in crate::game) fn argument_failure(game: &mut Game<'_>, message: Text, input: &str, cursor: usize) {
    game.command_failure(message);
    let cursor = cursor.min(input.len());
    let suggest = Style {
        click: Some(text::ClickEvent::SuggestCommand(format!("/{input}"))),
        ..Style::default()
    };
    let mut context = Text::empty();
    if cursor > 10 {
        context.push("...");
    }
    let start = (0..=cursor.saturating_sub(10)).rev().find(|&i| input.is_char_boundary(i)).unwrap_or(0);
    context.push(&input[start..cursor]);
    if cursor < input.len() {
        context.push(Text::literal(&input[cursor..]).with_color(text::RED).underlined());
    }
    context.push(Text::translate("command.context.here", &[]).with_color(text::RED).italic());
    game.command_failure(context.with_color(text::GRAY).with_style(&suggest));
}
