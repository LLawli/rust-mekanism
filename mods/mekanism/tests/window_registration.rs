//! A janela de upgrades de cada bloco é registrada quando o jogo cria o tile (o hook `block_entity`), e não quando a tela abre (o hook `screen`):
//! `ItemUpgrade.useOn` instala desde o primeiro uso do item, e o registro da `machinery_basis` (`adder_of`) só acha o bloco se ele já foi registrado.
//! O glue só compila dentro do jogo, então o teste lê o fonte: a chamada tem de estar no corpo de `block_entity` e fora do de `screen`.
use std::path::Path;

/// O corpo de `pub(in crate::game) fn <name>(` até o fecha-chaves que começa uma linha.
fn body(source: &str, name: &str) -> String {
    let head = format!("pub(in crate::game) fn {name}(");
    let start = source.find(&head).unwrap_or_else(|| panic!("sem {name}"));
    let rest = &source[start..];
    let end = rest.find("\n}\n").expect("fim da função");
    rest[..end].to_owned()
}

fn glue(mod_dir: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(mod_dir)
        .join("native/glue.rs");
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

#[test]
fn the_machines_register_their_upgrade_window_when_the_tile_is_created() {
    for mod_dir in ["mekanism", "mekanismgenerators"] {
        let source = glue(mod_dir);
        assert!(
            body(&source, "block_entity").contains("mb_window_register"),
            "{mod_dir}: o registro da janela de upgrades tem de estar em `block_entity`"
        );
        assert!(
            !body(&source, "screen").contains("mb_window_register"),
            "{mod_dir}: registrar só na tela faz o upgrade com shift falhar antes de a tela abrir"
        );
    }
}
