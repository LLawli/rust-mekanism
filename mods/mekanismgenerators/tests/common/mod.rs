//! O que os testes de rastro do Heat Generator partilham: os dados de upgrade lidos dos mesmos arquivos que o jogo lê. O Generators usa
//! os tipos e as tags do Mekanism (`data/mekanism/mekanism/upgrade` e `data/mekanism/tags/mekanism/upgrade`), então vêm do conteúdo dele.
#![allow(dead_code)]
use machinery_basis_core::upgrade::{UpgradeData, UpgradeTags, UpgradeTypes};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// A pilha do item de upgrade, que não tem `max_stack_size` próprio.
const UPGRADE_STACK_SIZE: u32 = 64;
const NAMESPACE: &str = "mekanism";
/// `Upgrade.DIRECT_CODEC`: a pasta dos tipos, e a das tags de suporte.
const UPGRADE_FOLDER: &str = "mekanism/upgrade";
const UPGRADE_TAGS_FOLDER: &str = "tags/mekanism/upgrade";

fn content(folder: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../mekanism/content/data/mekanism")
        .join(folder)
}

/// Os `.json` de uma pasta de dados, cada um com o id que o jogo lhe dá (`FileToIdConverter`): `<ns>:<caminho sem .json>`.
fn read_folder(root: &Path, folder: &Path, files: &mut Vec<(String, Value)>) {
    let mut entries: Vec<_> = std::fs::read_dir(folder)
        .unwrap_or_else(|error| panic!("{}: {error}", folder.display()))
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            read_folder(root, &path, files);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            let relative = path.strip_prefix(root).unwrap().with_extension("");
            let id = format!("{NAMESPACE}:{}", relative.to_str().unwrap());
            let text = std::fs::read_to_string(&path).unwrap();
            files.push((id, serde_json::from_str(&text).unwrap()));
        }
    }
}

fn load(folder: &str) -> Vec<(String, Value)> {
    let root = content(folder);
    let mut files = Vec::new();
    read_folder(&root, &root, &mut files);
    files
}

/// Os dados de upgrade do Heat Generator: os tipos e as tags que o Mekanism entrega, e nada escrito à mão.
pub fn upgrade_data() -> Rc<UpgradeData> {
    let (types, errors) = UpgradeTypes::load(&load(UPGRADE_FOLDER));
    assert!(errors.is_empty(), "tipos recusados: {errors:?}");
    assert!(types.len() >= 4, "faltam tipos: {}", types.len());
    let tags = UpgradeTags::load(&load(UPGRADE_TAGS_FOLDER));
    mekanismgenerators_rules::upgrade_data(&types, &tags, UPGRADE_STACK_SIZE)
}

/// Quantos tipos de upgrade o Mekanism declara.
pub fn declared_types() -> usize {
    UpgradeTypes::load(&load(UPGRADE_FOLDER)).0.len()
}

// ------------------------------------------------------------------------------------------------------------ a tela do cliente

use machinery_basis_core::container_sync::{ContainerSync, Mirror, Property, TrackedSource, wire};
use machinery_basis_core::machine_sync::read_upgrade_map;
use machinery_basis_core::tile_trackers::{
    UPGRADE_WINDOW_KEY, position_of, source as tracked_source,
};
use machinery_basis_core::upgrade_window::UpgradeSync;
use mekanismgenerators_rules::{
    HeatGeneratorSync, Produced, ScreenData, State, heat_generator_trackers, upgrade_window_data,
};

/// O Heat Generator visto pelo servidor, com o que o `broadcastChanges` lê. `Default` dá o gerador recém-posto, sem upgrades.
pub struct Server {
    pub state: State,
    pub last: Produced,
    pub upgrades: UpgradeSync,
}

impl Server {
    pub fn new(ambient: f64) -> Self {
        Self {
            state: State::new(ambient),
            last: Produced::default(),
            upgrades: UpgradeSync::default(),
        }
    }

    pub fn source(&self) -> impl TrackedSource + '_ {
        HeatGeneratorSync {
            state: &self.state,
            last: &self.last,
            upgrades: &self.upgrades,
        }
    }
}

/// O menu como o cliente de um jogador o tem: o espelho, que recebe cada dado pelo pacote em bytes que o servidor manda.
pub struct Client {
    pub mirror: Mirror,
}

impl Client {
    /// O menu recém-aberto: `sendAllDataToRemote` do servidor (`sync` é o `ContainerSync` do jogador) e o espelho que o cliente monta com
    /// os dados que chegam.
    pub fn open(sync: &mut ContainerSync<u8>, server: &Server) -> Self {
        let mut client = Self {
            mirror: Mirror::new(&heat_generator_trackers()),
        };
        client.receive(&sync.send_all(&server.source()), true);
        client
    }

    /// O `PacketUpdateContainer`: os dados passam pelos bytes do pacote antes de o espelho os aplicar na posição de cada um.
    pub fn receive(&mut self, properties: &[Property], opens: bool) {
        let opens = opens.then(|| "mekanismgenerators:heat_generator".to_owned());
        let packet = wire::Packet::new([0, 64, 0], opens, properties);
        let packet = wire::decode(&wire::encode(&packet)).expect("o pacote volta");
        for (index, data) in &packet.data {
            assert!(
                self.mirror.apply(*index, data),
                "o dado {index} não foi aceito"
            );
        }
    }

    /// A janela de upgrades abre: o cliente também faz `startTracking(UPGRADE_WINDOW_KEY, ..)`, e só então chegam os dados novos.
    pub fn track_window(&mut self, properties: &[Property]) -> Vec<usize> {
        let positions = self
            .mirror
            .start_tracking(UPGRADE_WINDOW_KEY, &upgrade_window_data());
        self.receive(properties, false);
        positions
    }

    pub fn stop_window(&mut self) -> Vec<usize> {
        self.mirror.stop_tracking(&UPGRADE_WINDOW_KEY)
    }

    /// O que a janela de upgrades lê do espelho: `upgradeTicks` e o mapa, nas posições que `startTracking` deu.
    pub fn upgrades(&self, window: &[usize]) -> UpgradeSync {
        let list = upgrade_window_data();
        let ticks = window[position_of(&list, tracked_source::UPGRADE_TICKS)];
        let map = window[position_of(&list, tracked_source::UPGRADE_MAP)];
        read_upgrade_map(self.mirror.int(ticks), self.mirror.bytes(map))
    }

    /// O que a tela lê do espelho.
    pub fn screen(&self) -> ScreenData {
        ScreenData::read(&self.mirror)
    }
}

/// O espelho de um menu aberto, com a janela de upgrades aberta ou não, para os testes que só precisam do cliente num estado.
pub fn client_of(server: &Server, window: bool) -> (Client, Vec<usize>) {
    let mut sync = ContainerSync::<u8>::new(&heat_generator_trackers());
    let mut client = Client::open(&mut sync, server);
    let positions = if window {
        let sent = sync.start_tracking_server(
            UPGRADE_WINDOW_KEY,
            &upgrade_window_data(),
            &server.source(),
        );
        client.track_window(&sent)
    } else {
        Vec::new()
    };
    (client, positions)
}
