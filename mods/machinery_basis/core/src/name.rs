//! O nome de um tile (`TileEntityMekanism.getName`, `getDisplayName`, `getCustomName`, `setCustomName`, `isNameable`): o nome
//! guardado, o do item que o colocou, e o que vai para o item ao quebrar. O texto é o do componente, sem tipo do jogo.

/// `customName` e `nameable` de um tile.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TileName {
    nameable: bool,
    custom: Option<String>,
}

impl TileName {
    /// `nameable = hasGui() && !AttributeGui.hasCustomName()`.
    pub fn new(nameable: bool) -> Self {
        Self {
            nameable,
            custom: None,
        }
    }

    pub fn is_nameable(&self) -> bool {
        self.nameable
    }

    /// `getCustomName`: só um tile que aceita nome o devolve.
    pub fn custom_name(&self) -> Option<&str> {
        if self.nameable {
            self.custom.as_deref()
        } else {
            None
        }
    }

    /// `setCustomName`: ignorado se o tile não aceita nome.
    pub fn set_custom_name(&mut self, name: Option<String>) {
        if self.nameable {
            self.custom = name;
        }
    }

    /// `getName`: o nome guardado ou o do bloco.
    pub fn name<'a>(&'a self, block_name: &'a str) -> &'a str {
        self.custom_name().unwrap_or(block_name)
    }

    /// `getDisplayName`: com nome aceito, o guardado ou a descrição do contêiner; sem, o nome do bloco.
    pub fn display_name<'a>(
        &'a self,
        container_description: &'a str,
        block_name: &'a str,
    ) -> &'a str {
        if self.nameable {
            self.custom_name().unwrap_or(container_description)
        } else {
            block_name
        }
    }

    /// `applyImplicitComponents`: o item que colocou o bloco tinha `custom_name`?
    pub fn apply_implicit_components(&mut self, item_name: Option<String>) {
        if self.nameable {
            self.set_custom_name(item_name);
        }
    }

    /// `loadAdditional`: o `CustomName` salvo, só se o tile aceita nome.
    pub fn load(&mut self, saved: Option<String>) {
        if self.nameable {
            self.custom = saved;
        }
    }

    /// `saveAdditional`: `storeNullable` só grava com nome.
    pub fn saved(&self) -> Option<&str> {
        self.custom_name()
    }

    /// `copy_name` da tabela de loot: o nome que o item solto leva.
    pub fn item_name(&self) -> Option<&str> {
        self.custom_name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nameable_tile_takes_the_name_of_the_item_and_gives_it_back() {
        let mut name = TileName::new(true);
        name.apply_implicit_components(Some("Minha Camara".into()));
        assert_eq!(name.name("Enrichment Chamber"), "Minha Camara");
        assert_eq!(
            name.display_name("Enrichment Chamber", "Enrichment Chamber"),
            "Minha Camara"
        );
        assert_eq!(name.saved(), Some("Minha Camara"));
        assert_eq!(name.item_name(), Some("Minha Camara"));
    }

    #[test]
    fn without_a_name_the_block_and_the_container_names_show() {
        let name = TileName::new(true);
        assert_eq!(name.name("Block"), "Block");
        assert_eq!(name.display_name("Container", "Block"), "Container");
        assert_eq!(name.saved(), None);
    }

    #[test]
    fn a_tile_that_is_not_nameable_ignores_every_name() {
        let mut name = TileName::new(false);
        name.apply_implicit_components(Some("x".into()));
        name.load(Some("y".into()));
        assert_eq!(name.custom_name(), None);
        assert_eq!(name.display_name("Container", "Block"), "Block");
        assert_eq!(name.saved(), None);
    }

    #[test]
    fn loading_replaces_the_name() {
        let mut name = TileName::new(true);
        name.apply_implicit_components(Some("a".into()));
        name.load(None);
        assert_eq!(name.custom_name(), None);
    }
}
