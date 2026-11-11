//! O `ItemStack` do Horizon em miniatura, só com o que o módulo `mek_data_energy.rs` (de `crafting.rs`) usa: o item, os dados de mod (`other`/`set_other`) e o `tidy`. Quem
//! inclui este arquivo o põe em `crate::items`, onde o módulo o procura.
#![allow(dead_code)]
use std::collections::BTreeMap;
use std::sync::Arc;

/// `horizon_world::nbt::Tag`, nos tipos que o módulo lê e escreve.
#[derive(Clone, Debug, PartialEq)]
pub enum Tag {
    Long(i64),
    Int(i32),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StackData {
    other: BTreeMap<String, Tag>,
}

impl StackData {
    pub fn other(&self, id: &str) -> Option<&Tag> {
        self.other.get(id)
    }

    pub fn set_other(&mut self, id: &str, value: Option<Tag>) {
        match value {
            Some(value) => {
                self.other.insert(id.to_owned(), value);
            }
            None => {
                self.other.remove(id);
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.other.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ItemStack {
    pub item: Arc<str>,
    pub count: u32,
    extra: Option<Box<StackData>>,
}

impl ItemStack {
    pub fn new(item: &str, count: u32) -> Self {
        Self {
            item: Arc::from(item),
            count,
            extra: None,
        }
    }

    pub fn data(&self) -> Option<&StackData> {
        self.extra.as_deref()
    }

    pub fn data_mut(&mut self) -> &mut StackData {
        self.extra.get_or_insert_with(Box::default)
    }

    pub fn tidy(&mut self) {
        if self.extra.as_ref().is_some_and(|data| data.is_empty()) {
            self.extra = None;
        }
    }
}
