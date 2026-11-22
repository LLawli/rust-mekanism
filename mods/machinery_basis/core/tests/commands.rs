//! A árvore de comandos de servidor: a conta do Brigadier para a linha digitada, e os usos que o jogo anuncia.
use machinery_basis_core::commands::{
    CommandNode, LEVEL_ALL, LEVEL_GAMEMASTERS, Parsed, SyntaxError, UNKNOWN_ARGUMENT,
    UNKNOWN_COMMAND, Usage, parse, usages,
};

fn tree() -> CommandNode<u32> {
    CommandNode::literal("mek", LEVEL_ALL)
        .then(CommandNode::literal("debug", LEVEL_GAMEMASTERS).executes(7))
        .then(
            CommandNode::literal("deep", LEVEL_GAMEMASTERS)
                .then(CommandNode::literal("leaf", LEVEL_GAMEMASTERS).executes(9)),
        )
}

fn error(line: &str, level: u8) -> Option<SyntaxError> {
    match parse(&tree(), line, level) {
        Some(Parsed::Error(error)) => Some(error),
        _ => None,
    }
}

fn ran(line: &str, level: u8) -> Option<u32> {
    match parse(&tree(), line, level) {
        Some(Parsed::Run(run)) => Some(*run),
        _ => None,
    }
}

#[test]
fn a_node_the_source_may_use_runs() {
    assert_eq!(ran("mek debug", 2), Some(7));
    assert_eq!(ran("mek debug", 4), Some(7));
    assert_eq!(ran("mek deep leaf", 2), Some(9));
}

#[test]
fn a_child_the_source_may_not_use_is_an_unknown_argument_at_the_word() {
    let wanted = SyntaxError {
        key: UNKNOWN_ARGUMENT,
        input: "mek debug".to_owned(),
        cursor: 4,
    };
    assert_eq!(error("mek debug", 0), Some(wanted.clone()));
    assert_eq!(error("mek debug", 1), Some(wanted));
}

#[test]
fn a_node_that_does_not_run_is_an_unknown_or_incomplete_command_at_the_end() {
    for (line, cursor) in [("mek", 3), ("mek deep", 8)] {
        assert_eq!(
            error(line, 4),
            Some(SyntaxError {
                key: UNKNOWN_COMMAND,
                input: line.to_owned(),
                cursor
            }),
            "{line}"
        );
    }
}

#[test]
fn text_that_no_node_reads_is_an_unknown_argument_where_it_starts() {
    for (line, cursor) in [
        ("mek nothing", 4),
        ("mek debug extra", 10),
        ("mek deep leaf more", 14),
        ("mek debugx", 4),
        ("mek debug ", 9),
    ] {
        assert_eq!(
            error(line, 4),
            Some(SyntaxError {
                key: UNKNOWN_ARGUMENT,
                input: line.to_owned(),
                cursor
            }),
            "{line}"
        );
    }
}

#[test]
fn another_root_is_not_ours() {
    assert!(parse(&tree(), "meka debug", 4).is_none());
    assert!(parse(&tree(), "tp", 4).is_none());
    assert!(parse(&tree(), "", 4).is_none());
}

#[test]
fn every_node_is_a_usage_with_its_own_level_parent_first() {
    let level = |text: &str, level| Usage {
        text: text.to_owned(),
        level,
    };
    assert_eq!(
        usages(&tree()),
        vec![
            level("mek", 0),
            level("mek debug", 2),
            level("mek deep", 2),
            level("mek deep leaf", 2),
        ]
    );
}

fn tree_with_arguments() -> CommandNode<u32> {
    CommandNode::literal("mek", LEVEL_ALL).then(
        CommandNode::literal("radiation", LEVEL_GAMEMASTERS)
            .then(
                CommandNode::literal("heal", LEVEL_GAMEMASTERS)
                    .executes(1)
                    .arguments("<targets>", 2),
            )
            .then(CommandNode::literal("add", LEVEL_GAMEMASTERS).arguments("<magnitude>", 3)),
    )
}

#[test]
fn a_node_with_arguments_hands_the_rest_of_the_line_to_its_reader() {
    let tree = tree_with_arguments();
    let run = |line: &'static str| match parse(&tree, line, 2) {
        Some(Parsed::Run(run)) => Some((*run, None)),
        Some(Parsed::RunArgs(run, rest)) => Some((*run, Some(rest))),
        _ => None,
    };
    assert_eq!(run("mek radiation heal"), Some((1, None)));
    assert_eq!(
        run("mek radiation heal @e[type=cow]"),
        Some((2, Some("@e[type=cow]")))
    );
    assert_eq!(
        run("mek radiation add 0.5 1 2 3"),
        Some((3, Some("0.5 1 2 3")))
    );
    // Sem argumento, o nó que só os lê não roda: `command.unknown.command` no fim da linha.
    match parse(&tree, "mek radiation add", 2) {
        Some(Parsed::Error(error)) => {
            assert_eq!(error.key, UNKNOWN_COMMAND);
            assert_eq!(error.cursor, "mek radiation add".len());
        }
        _ => panic!("add sem argumento"),
    }
    // Um operador abaixo do nível não chega nem ao literal.
    assert!(matches!(
        parse(&tree, "mek radiation heal @s", 0),
        Some(Parsed::Error(_))
    ));
    assert_eq!(
        usages(&tree)
            .into_iter()
            .map(|usage| usage.text)
            .collect::<Vec<_>>(),
        [
            "mek",
            "mek radiation",
            "mek radiation heal",
            "mek radiation heal <targets>",
            "mek radiation add",
            "mek radiation add <magnitude>",
        ]
    );
}
