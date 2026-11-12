//! O contêiner de itens de uma instância: o que cada face de um bloco com `ConfigHolder` alcança e deixa fazer (H2).
//!
//! O Horizon sabe o `ModContainer` por tipo de bloco; o original dá a cada face a capability de item do `DataType` do lado
//! (`ConfigHolder`, `ProxyItemHandler`). Aqui está a conta pura que o glue usa para dizer ao Horizon, por instância e por face:
//! os slots que a face alcança (`faces`), por onde um item entra (`inputs`, que o Horizon lê sem face) e por onde sai. O que a face
//! deixa fazer (`ISlotInfo.canInput` e `canOutput`) vale por face, e a regra do slot (`isValidForInsertion` e `isValidForExtraction`
//! de quem está de fora) é do mod; as duas se somam.
use crate::side_config::TileComponentConfig;
use crate::sides::Face;

/// O que uma face alcança: os slots (na ordem do `ISlotInfo`) e se deixa entrar e sair.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FaceItems {
    pub slots: Vec<usize>,
    pub can_input: bool,
    pub can_output: bool,
}

/// As seis faces de uma instância, na ordem de `Face::ALL` (a do `Direction` do Horizon).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InstanceItems {
    pub faces: [FaceItems; 6],
}

impl InstanceItems {
    /// As faces que a configuração dá a uma máquina virada para `facing`. `all_slots` vale só sem configuração de itens
    /// (`NO_CONFIG`, o `ConfigHolder` sem `ISlotInfo`: todos os slots entram e saem).
    pub fn of_config(config: &TileComponentConfig, facing: Face, all_slots: &[usize]) -> Self {
        let mut items = Self::default();
        for face in Face::ALL {
            let reached = config.item_face(facing, face, all_slots);
            let can_input = !reached.inputs.is_empty();
            let can_output = !reached.outputs.is_empty();
            // Os dois vêm do mesmo `ISlotInfo`: a lista é a mesma, e só a que a face deixa usar não está vazia.
            let slots = if can_input { reached.inputs } else { reached.outputs };
            items.faces[face as usize] = FaceItems { slots, can_input, can_output };
        }
        items
    }

    /// O `ModContainer.faces`: os slots que cada face alcança (`getSlotsForFace`).
    pub fn face_slots(&self) -> [Vec<usize>; 6] {
        std::array::from_fn(|i| self.faces[i].slots.clone())
    }

    /// O `ModContainer.inputs`: os slots que alguma face deixa encher. O Horizon pergunta sem a face, então a face é conferida de
    /// novo por [`Self::accepts_through`] na hora de pôr o item.
    pub fn inputs(&self) -> Vec<usize> {
        self.union(|face| face.can_input)
    }

    /// O `ModContainer.outputs`: os slots que alguma face deixa esvaziar (a face é conferida por [`Self::releases_through`]).
    pub fn outputs(&self) -> Vec<usize> {
        self.union(|face| face.can_output)
    }

    fn union(&self, wanted: impl Fn(&FaceItems) -> bool) -> Vec<usize> {
        let mut slots: Vec<usize> = Vec::new();
        for face in self.faces.iter().filter(|face| wanted(face)) {
            for &slot in &face.slots {
                if !slots.contains(&slot) {
                    slots.push(slot);
                }
            }
        }
        slots.sort_unstable();
        slots
    }

    /// Se a capability de item da face existe (`ConfigHolder.isCapabilityDisabled` falso): algum slot ao alcance.
    pub fn is_present(&self, face: Face) -> bool {
        !self.faces[face as usize].slots.is_empty()
    }

    /// Se um item pode entrar em `slot` por `face`, sem a regra do slot: a face alcança o slot e deixa entrar.
    pub fn accepts_through(&self, face: Face, slot: usize) -> bool {
        let reached = &self.faces[face as usize];
        reached.can_input && reached.slots.contains(&slot)
    }

    /// Se um item pode sair de `slot` por `face`, sem a regra do slot: a face alcança o slot e deixa sair.
    pub fn releases_through(&self, face: Face, slot: usize) -> bool {
        let reached = &self.faces[face as usize];
        reached.can_output && reached.slots.contains(&slot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> InstanceItems {
        let mut items = InstanceItems::default();
        items.faces[Face::Up as usize] = FaceItems { slots: vec![0], can_input: true, can_output: false };
        items.faces[Face::Down as usize] = FaceItems { slots: vec![1, 2], can_input: false, can_output: true };
        items.faces[Face::North as usize] = FaceItems { slots: vec![0, 1], can_input: true, can_output: true };
        items
    }

    #[test]
    fn a_face_only_does_what_its_flags_allow() {
        let items = items();
        assert!(items.accepts_through(Face::Up, 0));
        assert!(!items.releases_through(Face::Up, 0));
        assert!(!items.accepts_through(Face::Down, 1));
        assert!(items.releases_through(Face::Down, 1));
        assert!(items.accepts_through(Face::North, 1) && items.releases_through(Face::North, 1));
        assert!(!items.accepts_through(Face::South, 0), "uma face sem slot não deixa nada");
    }

    #[test]
    fn the_container_lists_are_the_union_of_the_faces_that_allow_it() {
        let items = items();
        assert_eq!(items.inputs(), vec![0, 1]);
        assert_eq!(items.outputs(), vec![0, 1, 2]);
        assert_eq!(items.face_slots()[Face::Down as usize], vec![1, 2]);
        assert!(items.is_present(Face::North) && !items.is_present(Face::East));
    }
}
