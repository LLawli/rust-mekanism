//! A machinery_basis para mods tipados: o núcleo puro, com a mesma cópia que o lado mixin usa.
use horizon_client::mod_api::Hooks;

pub use machinery_basis_core as core;

struct Mod;

impl Hooks for Mod {}

/// O que o launcher chama.
pub fn hooks() -> Box<dyn Hooks> {
    Box::new(Mod)
}
