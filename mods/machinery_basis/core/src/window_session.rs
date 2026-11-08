//! O que o servidor sabe de cada jogador com um contêiner aberto: a janela que ele escolheu (`MekanismContainer.selectedWindows`) e
//! a lista de dados sincronizados dele com o último valor que cada um mandou (`trackedData`, `startTracking` e `stopTracking`). No Java
//! isso mora no objeto do menu, um por jogador. No Horizon o contêiner é do bloco e vários jogadores o compartilham, então a tabela é
//! por (contêiner, jogador). Um jogador com sessão é um jogador no `playersUsing` do tile (`MekanismTileContainer.openInventory`).
use crate::container_sync::{ContainerSync, Property, Tracked, TrackedSource};
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

    /// Os ids dos dados, na ordem de `trackedData`: quem guarda o estado de cada um (`ContainerSync`) o acha por eles.
    pub fn ids(&self) -> &[u32] {
        &self.tracked
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
#[derive(Clone, Debug)]
pub struct Session<K: Ord> {
    /// `selectedWindows.get(player)`.
    pub selected: Option<SelectedWindow>,
    /// O nível (a dimensão) do contêiner: o servidor entra nele para ler o tile.
    pub level: String,
    /// O bloco do menu: a lista de dados é a dele, e um bloco que deu lugar a outro na posição leva o menu junto (`stillValid`).
    pub block: String,
    /// `trackedData` do menu dele, com o último valor que cada dado mandou.
    pub sync: ContainerSync<K>,
    /// Se os dados já foram todos ao jogador (`sendAllDataToRemote`): enquanto o tile não dá um valor para ler, a abertura espera.
    pub sent_all: bool,
}

/// O que um `broadcast` manda a um jogador: se abre o menu dele (os dados ainda não tinham ido) e os dados.
#[derive(Clone, Debug, PartialEq)]
pub struct Broadcast<P> {
    pub player: P,
    pub opens: bool,
    pub properties: Vec<Property>,
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
    pub fn open(&mut self, container: C, player: P, level: &str, block: &str, base: &[Tracked]) {
        self.map.insert(
            (container, player),
            Session {
                selected: None,
                level: level.to_owned(),
                block: block.to_owned(),
                sync: ContainerSync::new(base),
                sent_all: false,
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
            .any(|((c, _), session)| *c == container && session.sync.has_extra())
    }

    /// Larga as sessões do contêiner cujo jogador `keep` não reconhece mais: quem saiu sem avisar (conexão perdida, longe demais).
    /// Devolve quantas saíram.
    pub fn retain_players(&mut self, container: C, keep: impl Fn(P) -> bool) -> usize {
        let before = self.map.len();
        self.map.retain(|(c, p), _| *c != container || keep(*p));
        before - self.map.len()
    }

    /// `sendAllDataToRemote` do menu de `player`: todos os dados com o valor vivo. `None` sem sessão.
    pub fn send_initial(
        &mut self,
        container: C,
        player: P,
        source: &dyn TrackedSource,
    ) -> Option<Vec<Property>> {
        let session = self.map.get_mut(&(container, player))?;
        session.sent_all = true;
        Some(session.sync.send_all(source))
    }

    /// `MekanismContainer.broadcastChanges` de cada menu do contêiner: o que mudou desde o último envio a cada jogador. O menu que ainda não
    /// recebeu os dados (o tile não tinha objeto para ler na abertura) os recebe todos agora. Cada jogador tem a lista dele, então um dado
    /// que só um acompanha (a janela de upgrades) só chega a ele.
    pub fn broadcast(
        &mut self,
        container: C,
        block: &str,
        source: &dyn TrackedSource,
    ) -> Vec<Broadcast<P>> {
        // O bloco da posição mudou desde que o menu abriu: o menu não vale mais (`MekanismTileContainer.stillValid`), e a lista dele não é a deste bloco.
        self.map
            .retain(|(c, _), session| *c != container || session.block == block);
        let mut sent = Vec::new();
        for ((c, player), session) in &mut self.map {
            if *c != container {
                continue;
            }
            if session.sent_all {
                sent.push(Broadcast {
                    player: *player,
                    opens: false,
                    properties: session.sync.broadcast_changes(source),
                });
            } else {
                session.sent_all = true;
                sent.push(Broadcast {
                    player: *player,
                    opens: true,
                    properties: session.sync.send_all(source),
                });
            }
        }
        sent
    }

    /// `startTrackingServer(key, tracker)` no menu de `player`, e só no dele: os dados novos entram no fim da lista dele e vão logo.
    pub fn start_tracking_server(
        &mut self,
        container: C,
        player: P,
        key: K,
        list: &[Tracked],
        source: &dyn TrackedSource,
    ) -> Vec<Property> {
        self.map
            .get_mut(&(container, player))
            .map(|session| session.sync.start_tracking_server(key, list, source))
            .unwrap_or_default()
    }

    /// `startTracking(key, tracker)` sem envio, quando não há valor vivo para ler.
    pub fn start_tracking(&mut self, container: C, player: P, key: K, list: &[Tracked]) {
        if let Some(session) = self.map.get_mut(&(container, player)) {
            session.sync.start_tracking(key, list);
        }
    }

    /// `stopTracking(key)` no menu de `player`.
    pub fn stop_tracking(&mut self, container: C, player: P, key: &K) {
        if let Some(session) = self.map.get_mut(&(container, player)) {
            session.sync.stop_tracking(key);
        }
    }

    /// O contêiner deixou de existir (bloco quebrado): nenhuma sessão fica.
    pub fn close_container(&mut self, container: C) {
        self.map.retain(|(c, _), _| *c != container);
    }

    /// Quantos jogadores têm o contêiner aberto: o `playersUsing` do tile.
    pub fn players(&self, container: C) -> usize {
        self.map.keys().filter(|(c, _)| *c == container).count()
    }

    /// Os contêineres com algum jogador, cada um uma vez, e o nível dele.
    pub fn containers(&self) -> Vec<(C, String)> {
        let mut found: Vec<(C, String)> = Vec::new();
        for ((container, _), session) in &self.map {
            if !found.iter().any(|(known, _)| known == container) {
                found.push((*container, session.level.clone()));
            }
        }
        found
    }

    /// Os jogadores com o contêiner aberto.
    pub fn players_of(&self, container: C) -> Vec<P> {
        self.map
            .keys()
            .filter(|(c, _)| *c == container)
            .map(|(_, player)| *player)
            .collect()
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

    /// Os 16 dados de uma máquina elétrica, no que o tipo deles importa para estes testes.
    fn base() -> Vec<Tracked> {
        (0..16)
            .map(|source| Tracked::new(source, crate::container_sync::Kind::Int))
            .collect()
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
        sessions.open(1, 7, "overworld", "test:machine", &base());
        sessions.open(1, 8, "overworld", "test:machine", &base());
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
        sessions.open(1, 7, "overworld", "test:machine", &base());
        assert_eq!(sessions.selected(1, 7), None);
    }

    #[test]
    fn extra_tracking_is_per_container_and_per_player() {
        let mut sessions = Sessions::<u32, u128, u8>::default();
        sessions.open(1, 7, "overworld", "test:machine", &base());
        sessions.open(2, 7, "overworld", "test:machine", &base());
        assert!(!sessions.any_extra(1));
        sessions
            .session_mut(1, 7)
            .unwrap()
            .sync
            .start_tracking(2, &[Tracked::new(40, crate::container_sync::Kind::Int); 2]);
        assert!(sessions.any_extra(1));
        assert!(!sessions.any_extra(2));
        sessions.session_mut(1, 7).unwrap().sync.stop_tracking(&2);
        assert!(!sessions.any_extra(1));
    }

    /// Um valor por fonte, mudado à mão: o tile vivo do teste.
    #[derive(Default)]
    struct Live(std::cell::RefCell<std::collections::HashMap<u16, i32>>);

    impl Live {
        fn set(&self, source: u16, value: i32) {
            self.0.borrow_mut().insert(source, value);
        }
    }

    impl TrackedSource for Live {
        fn value(&self, source: u16) -> crate::container_sync::Value {
            crate::container_sync::Value::Int(self.0.borrow().get(&source).copied().unwrap_or(0))
        }
    }

    fn window() -> [Tracked; 2] {
        [100, 101].map(|source| Tracked::new(source, crate::container_sync::Kind::Int))
    }

    #[test]
    fn the_first_broadcast_of_a_menu_that_had_nothing_to_read_sends_everything_then_only_the_changes()
     {
        let live = Live::default();
        let mut sessions = Sessions::<u32, u128, u8>::default();
        sessions.open(1, 7, "overworld", "test:machine", &base());
        live.set(3, 9);
        let first = sessions.broadcast(1, "test:machine", &live);
        assert_eq!(first.len(), 1);
        assert!(first[0].opens);
        assert_eq!(
            first[0].properties.len(),
            16,
            "todos os dados, o padrão também"
        );
        assert!(
            sessions.broadcast(1, "test:machine", &live)[0]
                .properties
                .is_empty()
        );
        live.set(3, 10);
        let next = sessions.broadcast(1, "test:machine", &live);
        assert!(!next[0].opens);
        assert_eq!(next[0].properties.len(), 1);
        assert_eq!(next[0].properties[0].index, 3);
    }

    #[test]
    fn the_window_data_go_only_to_the_player_who_asked_for_them() {
        let live = Live::default();
        live.set(100, 4);
        let mut sessions = Sessions::<u32, u128, u8>::default();
        sessions.open(1, 7, "overworld", "test:machine", &base());
        sessions.open(1, 8, "overworld", "test:machine", &base());
        sessions.send_initial(1, 7, &live);
        sessions.send_initial(1, 8, &live);
        let sent = sessions.start_tracking_server(1, 7, 2, &window(), &live);
        assert_eq!(sent.iter().map(|p| p.index).collect::<Vec<_>>(), [16, 17]);
        assert!(sessions.session(1, 7).unwrap().sync.has_extra());
        assert!(!sessions.session(1, 8).unwrap().sync.has_extra());
        // O valor da janela muda: só quem a abriu recebe.
        live.set(100, 5);
        let sent = sessions.broadcast(1, "test:machine", &live);
        let of = |player: u128| sent.iter().find(|b| b.player == player).unwrap();
        assert_eq!(of(7).properties.len(), 1);
        assert_eq!(of(7).properties[0].index, 16);
        assert!(of(8).properties.is_empty());
        // Parar tira os dados dele e de mais ninguém.
        sessions.stop_tracking(1, 8, &2);
        assert!(sessions.session(1, 7).unwrap().sync.has_extra());
        sessions.stop_tracking(1, 7, &2);
        assert!(!sessions.session(1, 7).unwrap().sync.has_extra());
    }

    #[test]
    fn a_menu_of_a_block_that_gave_its_place_to_another_is_dropped() {
        let live = Live::default();
        let mut sessions = Sessions::<u32, u128, u8>::default();
        sessions.open(1, 7, "overworld", "test:machine", &base());
        assert_eq!(sessions.broadcast(1, "test:machine", &live).len(), 1);
        assert!(sessions.broadcast(1, "test:other", &live).is_empty());
        assert_eq!(sessions.players(1), 0);
    }

    #[test]
    fn a_menu_is_a_player_using_whatever_the_players_game_mode() {
        let mut sessions = Sessions::<u32, u128, u8>::default();
        sessions.open(1, 7, "overworld", "test:machine", &base());
        sessions.open(1, 8, "overworld", "test:machine", &base());
        assert_eq!(sessions.players(1), 2);
        assert_eq!(sessions.players_of(1).len(), 2);
        sessions.close(1, 8);
        assert_eq!(sessions.players(1), 1);
    }

    #[test]
    fn players_who_left_without_closing_are_dropped() {
        let mut sessions = Sessions::<u32, u128, u8>::default();
        for player in [1, 2, 3] {
            sessions.open(9, player, "overworld", "test:machine", &base());
            sessions.select(9, player, Some(upgrade()));
        }
        sessions.open(10, 1, "overworld", "test:machine", &base());
        let dropped = sessions.retain_players(9, |player| player == 2);
        assert_eq!(dropped, 2);
        assert_eq!(sessions.players(9), 1);
        assert_eq!(sessions.selected(9, 2), Some(upgrade()));
        assert!(sessions.is_open(10, 1), "outro contêiner não é tocado");
        sessions.close_container(9);
        assert_eq!(sessions.players(9), 0);
    }
}
