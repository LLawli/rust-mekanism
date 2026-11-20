//! O helper da injeção `recipe_book_settings_save` (`recipe_book_settings_save.rs`, HEAD cancelável de `Level::player_tag`) compilado aqui por
//! `include!`, com um `horizon_world::nbt` de mentira do tamanho do que ele usa: os campos dos tipos de livro de mod passam do composto
//! lido do jogador ao composto gravado, os do vanilla não (são do Horizon, que omite o que vale falso), e o que não é campo de
//! configuração do livro fica de fora.
#![allow(dead_code)]

mod horizon_world {
    pub mod nbt {
        use std::collections::BTreeMap;

        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub struct JavaString(pub String);

        impl JavaString {
            pub fn to_string_lossy(&self) -> String {
                self.0.clone()
            }
        }

        pub type Compound = BTreeMap<JavaString, Tag>;

        #[derive(Clone, Debug, PartialEq)]
        pub enum Tag {
            Byte(i8),
            Int(i32),
            List(Vec<Tag>),
            Compound(Compound),
        }

        impl Tag {
            pub fn get(&self, key: &str) -> Option<&Tag> {
                match self {
                    Tag::Compound(map) => map.get(&JavaString(key.to_owned())),
                    _ => None,
                }
            }

            pub fn get_mut(&mut self, key: &str) -> Option<&mut Tag> {
                match self {
                    Tag::Compound(map) => map.get_mut(&JavaString(key.to_owned())),
                    _ => None,
                }
            }
        }
    }
}

mod helper {
    use super::horizon_world;
    use super::horizon_world::nbt::Tag;
    use std::cell::Cell;

    /// O `Level` do Horizon, do tamanho do que o helper usa: o composto lido do jogador e o que o corpo original de `player_tag` grava.
    pub struct Level {
        pub player_extra: Option<horizon_world::nbt::Tag>,
        pub written: horizon_world::nbt::Tag,
        pub calls: Cell<u32>,
    }

    impl Level {
        /// O alvo com a injeção no começo: o helper cancela ou deixa o corpo rodar.
        pub fn player_tag(&self) -> horizon_world::nbt::Tag {
            if let core::ops::ControlFlow::Break(value) =
                machinery_basis_recipe_book_settings_save(self)
            {
                return value;
            }
            self.calls.set(self.calls.get() + 1);
            self.written.clone()
        }
    }

    include!("../../recipe_book_settings_save.rs");

    pub fn save(
        extra: &Option<horizon_world::nbt::Tag>,
        value: horizon_world::nbt::Tag,
    ) -> horizon_world::nbt::Tag {
        let level = Level {
            player_extra: extra.clone(),
            written: value,
            calls: Cell::new(0),
        };
        let saved = level.player_tag();
        // O corpo original roda uma vez só por chamada, e a marca de reentrada volta: a segunda chamada dá o mesmo.
        assert_eq!(level.calls.get(), 1);
        assert_eq!(level.player_tag(), saved);
        assert_eq!(level.calls.get(), 2);
        saved
    }
}

use horizon_world::nbt::{JavaString, Tag};

fn compound(entries: &[(&str, Tag)]) -> Tag {
    Tag::Compound(
        entries
            .iter()
            .map(|(name, tag)| (JavaString((*name).to_owned()), tag.clone()))
            .collect(),
    )
}

fn player(book: &[(&str, Tag)]) -> Tag {
    compound(&[("recipeBook", compound(book)), ("XpLevel", Tag::Int(3))])
}

#[test]
fn the_mod_book_type_settings_survive_the_save_and_the_vanilla_ones_stay_with_the_game() {
    // O composto lido: um tipo de mod aberto e filtrando, outro com o campo falso, a fornalha aberta (que o jogo fechou depois), uma lista.
    let extra = Some(player(&[
        ("isMekanismEnrichingGuiOpen", Tag::Byte(1)),
        ("isMekanismEnrichingFilteringCraftable", Tag::Byte(1)),
        ("isMekanismCrushingGuiOpen", Tag::Byte(0)),
        ("isFurnaceGuiOpen", Tag::Byte(1)),
        ("isGuiOpen", Tag::Byte(1)),
        ("recipes", Tag::List(vec![Tag::Int(1)])),
        ("isSomethingElse", Tag::Byte(1)),
    ]));
    // O que o jogo grava: só o livro da bancada aberto e as listas dele.
    let written = player(&[
        ("isGuiOpen", Tag::Byte(1)),
        ("recipes", Tag::List(Vec::new())),
        ("toBeDisplayed", Tag::List(Vec::new())),
    ]);
    let saved = helper::save(&extra, written);
    let expected = player(&[
        ("isGuiOpen", Tag::Byte(1)),
        ("isMekanismEnrichingGuiOpen", Tag::Byte(1)),
        ("isMekanismEnrichingFilteringCraftable", Tag::Byte(1)),
        ("recipes", Tag::List(Vec::new())),
        ("toBeDisplayed", Tag::List(Vec::new())),
    ]);
    assert_eq!(saved, expected);
}

#[test]
fn a_player_without_a_saved_book_is_written_as_the_game_writes() {
    let written = player(&[("recipes", Tag::List(Vec::new()))]);
    assert_eq!(helper::save(&None, written.clone()), written);
    assert_eq!(
        helper::save(
            &Some(compound(&[("XpLevel", Tag::Int(1))])),
            written.clone()
        ),
        written
    );
    // Sem o `recipeBook` no que é gravado não há onde pôr.
    let bare = compound(&[("XpLevel", Tag::Int(1))]);
    let extra = Some(player(&[("isMekanismEnrichingGuiOpen", Tag::Byte(1))]));
    assert_eq!(helper::save(&extra, bare.clone()), bare);
}
