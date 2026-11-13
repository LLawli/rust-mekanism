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
