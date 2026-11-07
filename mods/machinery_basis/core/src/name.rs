//! O nome de um tile (`TileEntityMekanism.getName`, `getDisplayName`, `getCustomName`, `setCustomName`, `isNameable`): o nome
//! guardado, o do item que o colocou, e o que vai para o item ao quebrar. O nome é o componente inteiro (`Component`: texto, cor,
//! negrito e filhos), que passa de um lugar a outro sem ser lido: `C` é o tipo dele no jogo, e esta regra nunca olha dentro.

/// `customName` e `nameable` de um tile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TileName<C> {
    nameable: bool,
    custom: Option<C>,
}

impl<C> TileName<C> {
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
    pub fn custom_name(&self) -> Option<&C> {
        if self.nameable {
            self.custom.as_ref()
        } else {
            None
        }
    }

    /// `setCustomName`: ignorado se o tile não aceita nome.
    pub fn set_custom_name(&mut self, name: Option<C>) {
        if self.nameable {
            self.custom = name;
        }
    }

    /// `getName`: o nome guardado ou o do bloco.
    pub fn name<'a>(&'a self, block_name: &'a C) -> &'a C {
        self.custom_name().unwrap_or(block_name)
    }

    /// `getDisplayName`: com nome aceito, o guardado ou a descrição do contêiner; sem, o nome do bloco.
    pub fn display_name<'a>(&'a self, container_description: &'a C, block_name: &'a C) -> &'a C {
        if self.nameable {
            self.custom_name().unwrap_or(container_description)
        } else {
            block_name
        }
    }

    /// `applyImplicitComponents`: o item que colocou o bloco tinha `custom_name`?
    pub fn apply_implicit_components(&mut self, item_name: Option<C>) {
        if self.nameable {
            self.set_custom_name(item_name);
        }
    }

    /// `loadAdditional`: o `CustomName` salvo, só se o tile aceita nome.
    pub fn load(&mut self, saved: Option<C>) {
        if self.nameable {
            self.custom = saved;
        }
    }

    /// `saveAdditional`: `storeNullable` só grava com nome.
    pub fn saved(&self) -> Option<&C> {
        self.custom_name()
    }

    /// `copy_name` da tabela de loot: o nome que o item solto leva.
    pub fn item_name(&self) -> Option<&C> {
        self.custom_name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> String {
        value.to_owned()
    }

    #[test]
    fn a_nameable_tile_takes_the_name_of_the_item_and_gives_it_back() {
        let mut name = TileName::new(true);
        name.apply_implicit_components(Some(text("Minha Camara")));
        assert_eq!(name.name(&text("Enrichment Chamber")), "Minha Camara");
        assert_eq!(
            name.display_name(&text("Enrichment Chamber"), &text("Enrichment Chamber")),
            "Minha Camara"
        );
        assert_eq!(name.saved(), Some(&text("Minha Camara")));
        assert_eq!(name.item_name(), Some(&text("Minha Camara")));
    }

    #[test]
    fn without_a_name_the_block_and_the_container_names_show() {
        let name = TileName::<String>::new(true);
        assert_eq!(name.name(&text("Block")), "Block");
        assert_eq!(
            name.display_name(&text("Container"), &text("Block")),
            "Container"
        );
        assert_eq!(name.saved(), None);
    }

    #[test]
    fn a_tile_that_is_not_nameable_ignores_every_name() {
        let mut name = TileName::new(false);
        name.apply_implicit_components(Some(text("x")));
        name.load(Some(text("y")));
        assert_eq!(name.custom_name(), None);
        assert_eq!(
            name.display_name(&text("Container"), &text("Block")),
            "Block"
        );
        assert_eq!(name.saved(), None);
    }

    #[test]
    fn loading_replaces_the_name() {
        let mut name = TileName::new(true);
        name.apply_implicit_components(Some(text("a")));
        name.load(None);
        assert_eq!(name.custom_name(), None);
    }

    /// O componente passa inteiro: o que entra pelo item é o que o tile grava, mostra no título e devolve ao item, sem ser lido.
    #[test]
    fn a_styled_component_comes_out_exactly_as_it_went_in() {
        #[derive(Clone, Debug, PartialEq, Eq)]
        struct Styled {
            text: &'static str,
            color: &'static str,
            bold: bool,
        }
        let component = Styled {
            text: "Camara",
            color: "red",
            bold: true,
        };
        let block = Styled {
            text: "Block",
            color: "",
            bold: false,
        };
        let mut name = TileName::new(true);
        name.apply_implicit_components(Some(component.clone()));
        assert_eq!(name.name(&block), &component);
        assert_eq!(name.display_name(&block, &block), &component);
        assert_eq!(name.saved(), Some(&component));
        assert_eq!(name.item_name(), Some(&component));
        let mut reloaded = TileName::new(true);
        reloaded.load(name.saved().cloned());
        assert_eq!(reloaded, name);
    }
}
