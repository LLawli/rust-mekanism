//! Comandos de servidor de um mod: a árvore de literais com a exigência de cada nó (`requires`) e a conta do Brigadier que diz, para uma linha digitada, qual comando roda ou
//! qual erro o jogador lê (`CommandDispatcher.execute`, `dispatcherUnknownCommand` e `dispatcherUnknownArgument`). O código puro: quem liga a árvore ao jogo é a cola.
//!
//! O Horizon guarda o nível de um comando por nome da raiz (o menor dos usos) e dá ao mod só a linha depois da barra, então cada nó vira um uso (`usages`): o nível do nó fica
//! no caminho dele, e o nível de um pai é o dele mesmo e não o do filho mais baixo. Um nó sem `executes` é anunciado como executável (o formato dos usos não tem como dizer o
//! contrário), e a linha digitada nele dá o erro do Brigadier, igual ao original.

/// O nível de operador de `Commands.LEVEL_ALL`: qualquer jogador.
pub const LEVEL_ALL: u8 = 0;
/// `Commands.LEVEL_GAMEMASTERS`.
pub const LEVEL_GAMEMASTERS: u8 = 2;

/// Um literal da árvore (`Commands.literal(name).requires(..).executes(..).then(..)`).
#[derive(Clone, Debug)]
pub struct CommandNode<E> {
    pub name: &'static str,
    /// O nível de operador que o nó pede (`requires`, o `fallbackLevel` do `CommandPermissionNode`).
    pub level: u8,
    pub executes: Option<E>,
    pub children: Vec<CommandNode<E>>,
}

impl<E> CommandNode<E> {
    pub fn literal(name: &'static str, level: u8) -> Self {
        Self {
            name,
            level,
            executes: None,
            children: Vec::new(),
        }
    }

    pub fn executes(mut self, run: E) -> Self {
        self.executes = Some(run);
        self
    }

    pub fn then(mut self, child: CommandNode<E>) -> Self {
        self.children.push(child);
        self
    }
}

/// Um uso que o jogo anuncia: o caminho do nó (`mek debug`) e o nível dele.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Usage {
    pub text: String,
    pub level: u8,
}

/// Um uso por nó, o pai antes dos filhos.
pub fn usages<E>(root: &CommandNode<E>) -> Vec<Usage> {
    fn walk<E>(node: &CommandNode<E>, path: &str, out: &mut Vec<Usage>) {
        let text = if path.is_empty() {
            node.name.to_owned()
        } else {
            format!("{path} {}", node.name)
        };
        out.push(Usage {
            text: text.clone(),
            level: node.level,
        });
        for child in &node.children {
            walk(child, &text, out);
        }
    }
    let mut out = Vec::new();
    walk(root, "", &mut out);
    out
}

/// O erro de sintaxe do Brigadier: a chave de tradução, a linha e onde ela falha (`CommandSyntaxException.getInput` e `getCursor`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxError {
    /// `command.unknown.command` (a linha acaba num nó que não roda) ou `command.unknown.argument` (sobra texto que nenhum nó lê).
    pub key: &'static str,
    pub input: String,
    pub cursor: usize,
}

pub const UNKNOWN_COMMAND: &str = "command.unknown.command";
pub const UNKNOWN_ARGUMENT: &str = "command.unknown.argument";

/// O que a linha digitada dá.
#[derive(Debug)]
pub enum Parsed<'a, E> {
    /// Um nó que roda: o `executes` dele.
    Run(&'a E),
    Error(SyntaxError),
}

/// `CommandDispatcher.parse` e `execute` para a linha (sem a barra) de um jogador com `permission` (o nível de operador). `None` se a primeira palavra não é a raiz.
pub fn parse<'a, E>(root: &'a CommandNode<E>, line: &str, permission: u8) -> Option<Parsed<'a, E>> {
    if !word_is(line, 0, root.name) {
        return None;
    }
    let error = |key, cursor| {
        Some(Parsed::Error(SyntaxError {
            key,
            input: line.to_owned(),
            cursor,
        }))
    };
    if permission < root.level {
        return error(UNKNOWN_COMMAND, 0);
    }
    let mut node = root;
    let mut cursor = root.name.len();
    loop {
        let remaining = line.len() - cursor;
        if remaining == 0 {
            return match &node.executes {
                Some(run) => Some(Parsed::Run(run)),
                None => error(UNKNOWN_COMMAND, cursor),
            };
        }
        // `canRead(2)`: o separador sozinho no fim não abre o nó seguinte, e sobra a linha (o separador fica lido por quem o vê).
        if remaining < 2 {
            return error(UNKNOWN_ARGUMENT, cursor);
        }
        // Depois de um literal o Brigadier só aceita o separador.
        if !line[cursor..].starts_with(' ') {
            return error(UNKNOWN_ARGUMENT, cursor);
        }
        cursor += 1;
        // Só os filhos que a origem pode usar (`CommandNode.canUse`).
        match node
            .children
            .iter()
            .find(|child| permission >= child.level && word_is(line, cursor, child.name))
        {
            Some(child) => {
                node = child;
                cursor += child.name.len();
            }
            None => return error(UNKNOWN_ARGUMENT, cursor),
        }
    }
}

/// O literal `name` em `line` a partir de `at`, seguido do fim ou do separador.
fn word_is(line: &str, at: usize, name: &str) -> bool {
    line.get(at..).is_some_and(|rest| {
        rest.starts_with(name)
            && rest[name.len()..]
                .chars()
                .next()
                .is_none_or(|next| next == ' ')
    })
}
