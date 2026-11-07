//! Os elementos de uma tela de contêiner que recebem o mouse, e por isso não deixam um clique ser "fora" da imagem
//! (`GuiMekanism.hasClickedOutside` é `super.hasClickedOutside(..) && findElementOver(..) == null`, `GuiMekanism.java:397-399`).
//!
//! O mod declara por tipo de menu, uma vez, os retângulos relativos ao canto da tela de cada elemento ativo e visível que fica fora
//! dos 176 por 166 da imagem (as abas laterais). `findElementOver` só considera quem está ativo e visível no instante
//! (`GuiElement.isMouseOver`, que exige `active && visible`), então cada elemento pode trazer uma condição que pergunta ao estado da
//! tela, os data slots que o servidor sincroniza, se ele está ativo agora: a aba de experiência só com o upgrade instalado, a de
//! avisos só com algum aviso. O registro é um só no processo, porque o servidor e o cliente rodam em threads diferentes e precisam da
//! mesma resposta. A lib não conhece id de menu, de químico ou de máquina: tudo chega por parâmetro.
use crate::gui::{Rect, contains};
use std::fmt;
use std::sync::{Arc, Mutex};

/// A condição de um elemento: recebe os data slots da tela e diz se ele está ativo e visível agora.
type Condition = Arc<dyn Fn(&[i32]) -> bool + Send + Sync>;

/// Um elemento de tela: o retângulo (relativo ao canto da tela) e, se ele nem sempre está ativo, a condição.
#[derive(Clone)]
pub struct Element {
    rect: Rect,
    condition: Option<Condition>,
}

impl Element {
    /// Um elemento sempre ativo e visível (`GuiEnergyTab`, `GuiHeatTab`).
    pub fn always(rect: Rect) -> Self {
        Self {
            rect,
            condition: None,
        }
    }

    /// Um elemento que só conta quando `active` diz que sim para o estado da tela (`GuiXpContainmentTab.visible`, `GuiWarningTab.visible`).
    pub fn when(rect: Rect, active: impl Fn(&[i32]) -> bool + Send + Sync + 'static) -> Self {
        Self {
            rect,
            condition: Some(Arc::new(active)),
        }
    }

    pub fn rect(&self) -> Rect {
        self.rect
    }

    /// Se o elemento está ativo e visível com os data slots `state`.
    pub fn is_active(&self, state: &[i32]) -> bool {
        self.condition
            .as_ref()
            .is_none_or(|condition| condition(state))
    }

    /// `GuiElement.isMouseOver`: ativo, visível e com o ponto dentro do retângulo.
    pub fn is_mouse_over(&self, state: &[i32], at: [f32; 2]) -> bool {
        contains(self.rect, at) && self.is_active(state)
    }
}

impl fmt::Debug for Element {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Element")
            .field("rect", &self.rect)
            .field("conditional", &self.condition.is_some())
            .finish()
    }
}

static REGISTRY: Mutex<Vec<(String, Vec<Element>)>> = Mutex::new(Vec::new());

fn with<R>(f: impl FnOnce(&mut Vec<(String, Vec<Element>)>) -> R) -> R {
    let mut registry = REGISTRY
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    f(&mut registry)
}

/// Declara (ou troca) os elementos do tipo de menu `menu`. Pode ser chamada a qualquer momento e quantas vezes quiser.
pub fn declare(menu: &str, elements: Vec<Element>) {
    with(
        |registry| match registry.iter_mut().find(|(name, _)| name == menu) {
            Some((_, existing)) => *existing = elements,
            None => registry.push((menu.to_owned(), elements)),
        },
    );
}

/// Declara só se o tipo ainda não tem declaração: o mod pode chamar a cada consulta sem montar a lista de novo.
pub fn declare_once(menu: &str, elements: impl FnOnce() -> Vec<Element>) {
    if !declared(menu) {
        declare(menu, elements());
    }
}

/// Se o tipo de menu declarou elementos (mesmo uma lista vazia).
pub fn declared(menu: &str) -> bool {
    with(|registry| registry.iter().any(|(name, _)| name == menu))
}

/// `GuiMekanism.findElementOver(..) != null` para os elementos declarados: o ponto `at` (relativo ao canto da tela) está sobre algum
/// elemento ativo e visível com os data slots `state`. A condição roda fora da trava do registro.
pub fn over_element(menu: &str, at: [f32; 2], state: &[i32]) -> bool {
    let elements = with(|registry| {
        registry
            .iter()
            .find(|(name, _)| name == menu)
            .map(|(_, elements)| elements.clone())
            .unwrap_or_default()
    });
    elements
        .iter()
        .any(|element| element.is_mouse_over(state, at))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_menu_without_a_declaration_has_no_element() {
        assert!(!declared("test:elements_none"));
        assert!(!over_element("test:elements_none", [-10.0, 150.0], &[]));
    }

    #[test]
    fn an_always_active_element_is_hit_inside_its_rectangle_only() {
        declare(
            "test:elements_a",
            vec![Element::always([-26.0, 137.0, 26.0, 26.0])],
        );
        let over = |x: f32, y: f32| over_element("test:elements_a", [x, y], &[]);
        // O canto de cima e da esquerda entram, o de baixo e o da direita não (`GuiElement.isMouseOver`).
        assert!(over(-26.0, 137.0));
        assert!(over(-0.1, 162.9));
        assert!(!over(0.0, 150.0));
        assert!(!over(-10.0, 163.0));
        assert!(!over(-27.0, 150.0));
        assert!(!over(-10.0, 136.9));
    }

    #[test]
    fn a_conditional_element_asks_the_screen_state() {
        declare(
            "test:elements_b",
            vec![
                Element::always([-26.0, 137.0, 26.0, 26.0]),
                Element::when([179.0, 94.0, 18.0, 18.0], |state| {
                    state.get(2).copied().unwrap_or(0) != 0
                }),
            ],
        );
        let over = |state: &[i32]| over_element("test:elements_b", [188.0, 103.0], state);
        assert!(!over(&[0, 0, 0]));
        assert!(over(&[0, 0, 1]));
        // Antes do primeiro valor do servidor os data slots estão vazios: a condição vê zero.
        assert!(!over(&[]));
        // O elemento sempre ativo não depende do estado.
        assert!(over_element("test:elements_b", [-10.0, 150.0], &[]));
    }

    #[test]
    fn declaring_again_replaces_and_declare_once_keeps() {
        declare(
            "test:elements_c",
            vec![Element::always([-26.0, 137.0, 26.0, 26.0])],
        );
        declare_once("test:elements_c", || panic!("já declarado"));
        declare("test:elements_c", Vec::new());
        assert!(declared("test:elements_c"));
        assert!(!over_element("test:elements_c", [-10.0, 150.0], &[]));
        declare_once("test:elements_d", || {
            vec![Element::always([0.0, 0.0, 1.0, 1.0])]
        });
        assert!(over_element("test:elements_d", [0.5, 0.5], &[]));
    }

    #[test]
    fn the_condition_may_read_the_registry_without_deadlocking() {
        declare(
            "test:elements_e",
            vec![Element::when([0.0, 0.0, 4.0, 4.0], |_| {
                declared("test:elements_e")
            })],
        );
        assert!(over_element("test:elements_e", [1.0, 1.0], &[]));
    }
}
