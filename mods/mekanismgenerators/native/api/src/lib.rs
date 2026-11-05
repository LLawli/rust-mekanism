//! A parte tipada do mod, que declara a dependência da machinery_basis para o launcher.
use horizon_client::mod_api::Hooks;

struct Mod;

impl Hooks for Mod {}

/// O que o launcher chama.
pub fn hooks() -> Box<dyn Hooks> {
    Box::new(Mod)
}
