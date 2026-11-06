//! O que o servidor sabe de cada jogador com um contêiner aberto: a janela que ele escolheu (`MekanismContainer.selectedWindows`) e
//! os dados que ele pediu para acompanhar (`startTracking` e `stopTracking`). No Java isso mora no objeto do menu, um por jogador.
//! No Horizon o contêiner é do bloco e vários jogadores o compartilham, então a tabela é por (contêiner, jogador).
use crate::window::SelectedWindow;
use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;

/// `MekanismContainer.trackedData` e `specificTrackedData`: o que o menu sincroniza, por posição, e as listas que cada chave
/// acrescentou. Só se contam os itens: o conteúdo é de quem sincroniza.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tracking<K: Ord> {
    /// Um id por dado sincronizado, na ordem de `trackedData`.
    tracked: Vec<u32>,
    /// `specificTrackedData`: os ids da última lista de cada chave.
    specific: BTreeMap<K, Vec<u32>>,
    next_id: u32,
    base: usize,
}

impl<K: Ord> Tracking<K> {
    /// O menu recém-criado já sincroniza `base` dados (os `track` do construtor: 16 na Enrichment Chamber, 18 no Metallurgic Infuser).
    pub fn new(base: usize) -> Self {
        Self {
            tracked: (0..base as u32).collect(),
            specific: BTreeMap::new(),
            next_id: base as u32,
            base,
        }
    }

    /// `trackedData.size()`.
    pub fn count(&self) -> usize {
        self.tracked.len()
    }

    /// O número de dados do construtor.
    pub fn base(&self) -> usize {
        self.base
    }

    /// Se há dado acompanhado além dos do construtor. Dados que ficaram para trás (veja [`start`](Self::start)) continuam contando.
    pub fn has_extra(&self) -> bool {
        self.tracked.len() > self.base
    }

    /// `startTracking(key, tracker)` com `items` dados novos: entram no fim, e a lista da chave é trocada. Uma lista antiga da mesma
    /// chave NÃO sai de `trackedData`: pedir de novo sem parar antes deixa os dados antigos sincronizando (o rastro
    /// `CONTAINER_TRACK_UPGRADES a second time, without stopping` vai de 18 para 20). Devolve as posições dos que entraram.
    pub fn start(&mut self, key: K, items: usize) -> Vec<usize> {
        let first = self.tracked.len();
        let ids: Vec<u32> = (0..items as u32).map(|i| self.next_id + i).collect();
        self.next_id += items as u32;
        self.tracked.extend(ids.iter().copied());
        self.specific.insert(key, ids);
        (first..first + items).collect()
    }

    /// `stopTracking(key)`: tira de `trackedData` a lista que a chave tem agora. Devolve as posições que ocupavam.
    pub fn stop(&mut self, key: &K) -> Vec<usize> {
        let Some(ids) = self.specific.remove(key) else {
            return Vec::new();
        };
        let removed: Vec<usize> = self
            .tracked
            .iter()
            .enumerate()
            .filter(|(_, id)| ids.contains(id))
            .map(|(position, _)| position)
            .collect();
        self.tracked.retain(|id| !ids.contains(id));
        removed
    }
}

/// O que um jogador tem num contêiner aberto.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session<K: Ord> {
    /// `selectedWindows.get(player)`.
    pub selected: Option<SelectedWindow>,
    pub tracking: Tracking<K>,
}

/// As sessões de todos os contêineres abertos: a chave `C` identifica o contêiner (o bloco) e `P` o jogador.
#[derive(Clone, Debug)]
pub struct Sessions<C, P, K: Ord> {
    map: HashMap<(C, P), Session<K>>,
}

impl<C, P, K: Ord> Default for Sessions<C, P, K> {
    fn default() -> Self {
        Self {
            map: HashMap::new(),
        }
    }
}

impl<C: Hash + Eq + Copy, P: Hash + Eq + Copy, K: Ord> Sessions<C, P, K> {
    /// O menu foi criado para o jogador: sem janela e com os dados do construtor. Reabrir começa de novo.
    pub fn open(&mut self, container: C, player: P, base: usize) {
        self.map.insert(
            (container, player),
            Session {
                selected: None,
                tracking: Tracking::new(base),
            },
        );
    }

    /// `removed(player)`: o menu fechou. A seleção e os dados vão embora com ele.
    pub fn close(&mut self, container: C, player: P) {
        self.map.remove(&(container, player));
    }

    pub fn is_open(&self, container: C, player: P) -> bool {
        self.map.contains_key(&(container, player))
    }

    pub fn session(&self, container: C, player: P) -> Option<&Session<K>> {
        self.map.get(&(container, player))
    }

    pub fn session_mut(&mut self, container: C, player: P) -> Option<&mut Session<K>> {
        self.map.get_mut(&(container, player))
    }

    /// `setSelectedWindow(player, window)`: `None` limpa. Sem menu aberto para o jogador não há onde guardar.
    pub fn select(&mut self, container: C, player: P, window: Option<SelectedWindow>) -> bool {
        match self.map.get_mut(&(container, player)) {
            Some(session) => {
                session.selected = window;
                true
            }
            None => false,
        }
    }

    /// `getSelectedWindow(player)`.
    pub fn selected(&self, container: C, player: P) -> Option<SelectedWindow> {
        self.map
            .get(&(container, player))
            .and_then(|session| session.selected)
    }

    /// Se algum jogador acompanha dados extras do contêiner: é quando o servidor precisa mandar o que o Java manda só a quem pediu.
    pub fn any_extra(&self, container: C) -> bool {
        self.map
            .iter()
            .any(|((c, _), session)| *c == container && session.tracking.has_extra())
    }

    /// Larga as sessões do contêiner cujo jogador `keep` não reconhece mais: quem saiu sem avisar (conexão perdida, longe demais).
    /// Devolve quantas saíram.
    pub fn retain_players(&mut self, container: C, keep: impl Fn(P) -> bool) -> usize {
        let before = self.map.len();
        self.map.retain(|(c, p), _| *c != container || keep(*p));
        before - self.map.len()
    }

    /// O contêiner deixou de existir (bloco quebrado): nenhuma sessão fica.
    pub fn close_container(&mut self, container: C) {
        self.map.retain(|(c, _), _| *c != container);
    }

    /// Quantos jogadores têm o contêiner aberto.
    pub fn players(&self, container: C) -> usize {
        self.map.keys().filter(|(c, _)| *c == container).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window::WindowType;

    const UPGRADE: WindowType = WindowType {
        ordinal: 9,
        save_name: Some("upgrade"),
        can_pin: true,
        max_data: 1,
    };

    fn upgrade() -> SelectedWindow {
        SelectedWindow::new(UPGRADE, 0)
    }

    #[test]
    fn tracking_follows_the_java_lists_including_the_leak() {
        let mut tracking = Tracking::<u8>::new(16);
        assert_eq!(tracking.count(), 16);
        assert!(!tracking.has_extra());
        assert_eq!(tracking.start(2, 2), [16, 17]);
        assert_eq!(tracking.count(), 18);
        assert_eq!(tracking.stop(&2), [16, 17]);
        assert_eq!(tracking.count(), 16);
        assert_eq!(tracking.start(2, 2), [16, 17]);
        // Pedir de novo sem parar: a lista antiga fica, e conta.
        assert_eq!(tracking.start(2, 2), [18, 19]);
        assert_eq!(tracking.count(), 20);
        // Parar tira só a lista nova, e os dados que vazaram seguem sincronizando.
        assert_eq!(tracking.stop(&2), [18, 19]);
        assert_eq!(tracking.count(), 18);
        assert!(tracking.has_extra());
        // Parar sem ter pedido não muda nada.
        assert_eq!(tracking.stop(&2), Vec::<usize>::new());
        assert_eq!(tracking.count(), 18);
    }

    #[test]
    fn a_session_lives_between_open_and_close_and_holds_one_selection_per_player() {
        let mut sessions = Sessions::<u32, u128, u8>::default();
        // Sem menu aberto, a escolha não fica.
        assert!(!sessions.select(1, 7, Some(upgrade())));
        sessions.open(1, 7, 16);
        sessions.open(1, 8, 16);
        assert!(sessions.select(1, 7, Some(upgrade())));
        assert_eq!(sessions.selected(1, 7), Some(upgrade()));
        assert_eq!(sessions.selected(1, 8), None);
        assert_eq!(sessions.players(1), 2);
        assert!(sessions.select(1, 7, None));
        assert_eq!(sessions.selected(1, 7), None);
        sessions.select(1, 8, Some(upgrade()));
        sessions.close(1, 8);
        assert_eq!(sessions.selected(1, 8), None);
        assert_eq!(sessions.players(1), 1);
        // Reabrir começa sem janela.
        sessions.select(1, 7, Some(upgrade()));
        sessions.open(1, 7, 16);
        assert_eq!(sessions.selected(1, 7), None);
    }

    #[test]
    fn extra_tracking_is_per_container_and_per_player() {
        let mut sessions = Sessions::<u32, u128, u8>::default();
        sessions.open(1, 7, 16);
        sessions.open(2, 7, 16);
        assert!(!sessions.any_extra(1));
        sessions.session_mut(1, 7).unwrap().tracking.start(2, 2);
        assert!(sessions.any_extra(1));
        assert!(!sessions.any_extra(2));
        sessions.session_mut(1, 7).unwrap().tracking.stop(&2);
        assert!(!sessions.any_extra(1));
    }

    #[test]
    fn players_who_left_without_closing_are_dropped() {
        let mut sessions = Sessions::<u32, u128, u8>::default();
        for player in [1, 2, 3] {
            sessions.open(9, player, 16);
            sessions.select(9, player, Some(upgrade()));
        }
        sessions.open(10, 1, 16);
        let dropped = sessions.retain_players(9, |player| player == 2);
        assert_eq!(dropped, 2);
        assert_eq!(sessions.players(9), 1);
        assert_eq!(sessions.selected(9, 2), Some(upgrade()));
        assert!(sessions.is_open(10, 1), "outro contêiner não é tocado");
        sessions.close_container(9);
        assert_eq!(sessions.players(9), 0);
    }
}
