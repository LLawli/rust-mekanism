// Janelas de GUI e a janela de upgrades da machinery_basis: o `GuiWindow` e o `GuiUpgradeWindow` do Mekanism, a aba que a abre e os dois
// slots virtuais dela. Este arquivo é o módulo `machinery_basis_windows`, enxertado em `game/native_hooks.rs` porque o jogo guarda a
// tela aberta (`OpenNativeScreen`) em campos privados de lá. As contas (posição, arrasto, lista, seleção por jogador, shift-clique)
// são da `machinery-basis-core`; aqui só se liga isso ao jogo e se desenha. O que é do mod (blocos, slots, sprites, textos, cores)
// chega por `UpgradeWindowSpec`.
use crate::game::Game;
use crate::game::machinery_basis as basis;
use horizon_client::containers::BlockEntity;
use horizon_client::hud::HudBatch;
use horizon_client::items::ItemStack;
use horizon_client::lang;
use horizon_client::mod_api::NativeScreenView;
use horizon_world::nbt::{Compound, JavaString, Tag};
use machinery_basis_core::config_window::{self as cw, ConfigPacket, ConfigView};
use machinery_basis_core::container_sync::{Mirror, Property, TrackedSource, wire};
use machinery_basis_core::ejector::TileComponentEjector;
use machinery_basis_core::gui::NineSlice;
use machinery_basis_core::machine_sync::read_upgrade_map;
use machinery_basis_core::recipe_book as rb;
use machinery_basis_core::recipe_book_data::Settings as BookSettings;
use machinery_basis_core::recipe_book_data::{ChangeSettings, ToClient, ToServer, Unlocked, settings_fields};
use machinery_basis_core::recipe_book_view as rbv;
use machinery_basis_core::stacked_contents::StackedContents;
use machinery_basis_core::resource;
use machinery_basis_core::scroll_list::ScrollList;
use machinery_basis_core::side_config::{DataType, RelativeSide, TileComponentConfig, TransmissionType};
use machinery_basis_core::side_tab::InsetTab;
use machinery_basis_core::sides::Face;
use machinery_basis_core::tile_trackers::UPGRADE_WINDOW_KEY;
use machinery_basis_core::upgrade::{self, UpgradeData};
use machinery_basis_core::upgrade_window::{self as uw, SupportedLayout, UpgradeSync, UpgradeWindowSpec};
use machinery_basis_core::window::{
    self, Click, Screen, SelectedWindow, Window, WindowPositions,
};
use machinery_basis_core::window_session::Sessions;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// `Tooltip.MAX_WIDTH`: a largura em que o jogo quebra o texto de um tooltip de widget.
const TOOLTIP_WRAP: i32 = 170;
/// `GameType.CREATIVE` como o Horizon o numera.
const CREATIVE: i32 = 1;

/// O canal das mensagens entre cliente e servidor: o primeiro byte diz qual (`OP_*`), o resto é o corpo.
const CHANNEL: &str = "machinery_basis:window";
const OP_SELECT: u8 = 0;
const OP_TRACK: u8 = 1;
const OP_STOP: u8 = 2;
const OP_REMOVE: u8 = 3;
/// Um pacote de configuração de lados ou de transporte (`ConfigPacket`).
const OP_CONFIG: u8 = 4;
/// O cliente abriu (1) ou fechou (0) a última janela de configuração: o servidor passa a mandar, ou para de mandar, o estado dela.
const OP_CONFIG_TRACK: u8 = 5;
/// O canal em que o servidor leva ao cliente a configuração de lados e o ejetor (`encode_snapshot`), depois da posição do bloco.
const CONFIG_CHANNEL: &str = "machinery_basis:config";

/// O canal do `PacketUpdateContainer`: o servidor manda a cada jogador, por tick, os dados sujos do menu dele (`machinery_basis_core::container_sync::wire`).
const CONTAINER_CHANNEL: &str = "machinery_basis:container";

/// O canal em que o servidor leva os tipos e as tags de upgrade a um cliente que não os carregou (o registro sincronizado do original).
const TYPES_CHANNEL: &str = "machinery_basis:upgrade_types";
/// O canal em que o servidor leva os recursos (os químicos: cor e sprite) pelo mesmo motivo.
const RESOURCES_CHANNEL: &str = "machinery_basis:resources";
/// A fonte dos tipos que vieram do servidor, ao lado das que a carga de dados local põe.
const SYNC_SOURCE: &str = "sync";

/// `PacketRemoveUpgrade.handle`: o servidor tira um ou todos os upgrades de um tipo do bloco em `pos`.
type RemoveFn = fn(&mut Game<'_>, [i32; 3], &str, bool);
/// `TileComponentUpgrade.addUpgrades`: põe até `max_available` do tipo no bloco em `pos` e devolve quantos entraram.
type AddFn = fn(&mut Game<'_>, [i32; 3], &str, i32) -> i32;
/// O valor vivo dos dados do menu do bloco em `pos` (`ISyncableData.getValue`): o dono do tile entrega a fonte a `act` e diz se havia tile
/// vivo para ler. É o que `broadcastChanges` e `sendAllDataToRemote` leem.
pub(in crate::game) type SourceFn = fn(&mut Game<'_>, [i32; 3], &mut dyn FnMut(&dyn TrackedSource)) -> bool;

/// O que o dono do bloco entrega à janela: a remoção, a entrada e a fonte dos dados sincronizados.
#[derive(Clone, Copy)]
struct Owner {
    remove: RemoveFn,
    add: AddFn,
    source: SourceFn,
}

static SPECS: Mutex<Vec<(&'static UpgradeWindowSpec, Owner)>> = Mutex::new(Vec::new());

/// O mod diz que blocos têm a janela, como o servidor remove um upgrade (`PacketRemoveUpgrade`), como o item de upgrade entra no bloco
/// (`ItemUpgrade.useOn`) e de onde vêm os dados do menu. Pode repetir: a mesma spec entra uma vez.
fn register(spec: &'static UpgradeWindowSpec, remove: RemoveFn, add: AddFn, source: SourceFn) {
    let mut specs = SPECS.lock().unwrap_or_else(|e| e.into_inner());
    if !specs.iter().any(|(known, _)| known.block == spec.block) {
        specs.push((spec, Owner { remove, add, source }));
    }
}

fn spec_of(block: &str) -> Option<&'static UpgradeWindowSpec> {
    let specs = SPECS.lock().unwrap_or_else(|e| e.into_inner());
    specs.iter().find(|(spec, _)| spec.block == block).map(|(spec, _)| *spec)
}

fn owner_of(block: &str) -> Option<Owner> {
    let specs = SPECS.lock().unwrap_or_else(|e| e.into_inner());
    specs.iter().find(|(spec, _)| spec.block == block).map(|(_, owner)| *owner)
}

fn remover_of(block: &str) -> Option<RemoveFn> {
    owner_of(block).map(|owner| owner.remove)
}

/// A spec do bloco `block` e como o upgrade entra nele, se o `item` é o item de upgrade de algum bloco registrado.
fn adder_of(block: &str, item: &str) -> Option<(&'static UpgradeWindowSpec, AddFn)> {
    let specs = SPECS.lock().unwrap_or_else(|e| e.into_inner());
    specs
        .iter()
        .find(|(spec, _)| spec.upgrades && spec.block == block && spec.item == item)
        .map(|(spec, owner)| (*spec, owner.add))
}

/// Se `item` é o item de upgrade de algum bloco registrado: a pergunta barata, antes de olhar o bloco.
fn is_upgrade_item(item: &str) -> bool {
    let specs = SPECS.lock().unwrap_or_else(|e| e.into_inner());
    specs.iter().any(|(spec, _)| spec.item == item)
}

/// Lê o `ConfigView` do bloco em `pos`.
pub(in crate::game) type ViewFn = fn(&Game<'_>, [i32; 3]) -> Option<ConfigView>;
/// O `handle` de um `ConfigPacket` no servidor, sobre o tile vivo em `pos`.
pub(in crate::game) type ApplyFn = fn(&mut Game<'_>, [i32; 3], ConfigPacket);
/// O estado vivo da configuração e do ejetor em `pos` (`encode_snapshot`), que o servidor manda a quem tem a janela aberta.
pub(in crate::game) type SnapshotFn = fn(&mut Game<'_>, [i32; 3]) -> Option<Vec<u8>>;

#[derive(Clone, Copy)]
struct ConfigOwner {
    view: ViewFn,
    apply: ApplyFn,
    snapshot: SnapshotFn,
}

static CONFIG_OWNERS: Mutex<Vec<(&'static str, ConfigOwner)>> = Mutex::new(Vec::new());

/// O mod diz que o bloco tem as abas de configuração e de transporte (`ISideConfiguration`), como o cliente lê a configuração dele e como o
/// servidor aplica os pacotes. Pode repetir.
fn register_config(block: &'static str, view: ViewFn, apply: ApplyFn, snapshot: SnapshotFn) {
    let mut owners = CONFIG_OWNERS.lock().unwrap_or_else(|e| e.into_inner());
    if !owners.iter().any(|(known, _)| *known == block) {
        owners.push((block, ConfigOwner { view, apply, snapshot }));
    }
}

fn config_owner(block: &str) -> Option<ConfigOwner> {
    let owners = CONFIG_OWNERS.lock().unwrap_or_else(|e| e.into_inner());
    owners.iter().find(|(known, _)| *known == block).map(|(_, owner)| *owner)
}

// ---------------------------------------------------------------------------------------------------------------------------
// Constantes do desenho (`GuiUpgradeWindow`, `GuiUpgradeWindowTab`).

/// Os filhos da janela, relativos a ela.
const CLOSE: [i32; 4] = [6, 6, 8, 8];
const PIN: [i32; 4] = [16, 6, 16, 8];
const LIST: [i32; 4] = [6, 18, 108, 50];
const SUPPORTED: [i32; 2] = [6, 68];
const INNER_SCREEN: [i32; 4] = [114, 18, 59, 50];
const INSTALLING: [i32; 4] = [176, 37, 10, 14];
const UNINSTALLING: [i32; 4] = [176, 58, 12, 12];
const UNINSTALL: [i32; 4] = [115, 54, 56, 12];
/// Onde o item de cada slot virtual fica (a moldura começa um pixel antes).
const SLOT_ITEMS: [[i32; 2]; 2] = [[176, 19], [176, 73]];
/// A aba: o elemento (`imageWidth`, 6, 26, 26) e o botão de dentro, 18 por 18 quatro pixels para dentro.
const TAB: [i32; 4] = [176, 6, 26, 26];
const TAB_BUTTON: [i32; 4] = [179, 10, 18, 18];

const WHITE: u32 = 0xFF_FF_FF_FF;
const TITLE_COLOR: u32 = 0xFF_40_40_40;
/// `SpecialColors.TEXT_SUBHEADING`.
const SUBHEADING_COLOR: u32 = 0xFF_78_78_78;
const SCREEN_COLOR: u32 = 0xFF_3C_FE_9A;
const FADE: u32 = 0x80_78_78_78;
const SLOT_HOVER: u32 = 0x80_FF_FF_FF;
const YELLOW: u32 = 0xFF_DD_4F;
const RED: u32 = 0xFF_38_3C;
const SHADOW_TINT: u32 = 0xBF_FF_FF_FF;
/// `ARGB.color(0.3F, WHITE)`.
const BLUR_TINT: u32 = 0x4C_FF_FF_FF;
const DIGITAL_HOVER: u32 = 0xFF_3C_FE_9A;
const DIGITAL_ACTIVE: u32 = 0xFF_30_CB_7B;
const DIGITAL_INACTIVE: u32 = 0xFF_24_98_5C;
const LINE_HEIGHT: f32 = 9.0;

// ---------------------------------------------------------------------------------------------------------------------------
// Estado do cliente. É um `Mutex` e não um campo do jogo porque os hooks de entrada só têm `&Game`.

/// As janelas desta lib: a de upgrades e as duas de configuração de lados.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Upgrade,
    Side,
    Transporter,
    /// `GuiRecipeBookWindow`.
    RecipeBook,
}

impl Kind {
    /// A ordem em que `initPinnedWindows` as abre (`GuiMekanismTile.addGenericTabs` põe a aba de upgrades, `GuiConfigurableTile`
    /// as duas de configuração depois).
    const ALL: [Kind; 4] = [Kind::Upgrade, Kind::Side, Kind::Transporter, Kind::RecipeBook];

    /// As duas janelas de configuração de lados (`ISideConfiguration`): as que o servidor acompanha com o estado do ejetor.
    fn is_config(self) -> bool {
        matches!(self, Kind::Side | Kind::Transporter)
    }

    /// Se a máquina tem a aba que abre esta janela.
    fn offered(self, spec: &UpgradeWindowSpec) -> bool {
        match self {
            Kind::Upgrade => true,
            Kind::Side | Kind::Transporter => has_config(spec),
            Kind::RecipeBook => spec.recipe_book,
        }
    }

    fn selection(self, spec: &UpgradeWindowSpec) -> SelectedWindow {
        SelectedWindow::new(
            match self {
                Kind::Upgrade => spec.window_type,
                Kind::Side => spec.window_types[SIDE_CONFIG_ORDINAL],
                Kind::Transporter => spec.window_types[TRANSPORTER_CONFIG_ORDINAL],
                Kind::RecipeBook => spec.window_types[RECIPE_BOOK_ORDINAL],
            },
            0,
        )
    }
}

/// `WindowType.SIDE_CONFIG` e `TRANSPORTER_CONFIG`: o `ordinal` no enum, que é o índice na tabela do mod.
const SIDE_CONFIG_ORDINAL: usize = 7;
const TRANSPORTER_CONFIG_ORDINAL: usize = 8;
/// `WindowType.RECIPE_BOOK`.
const RECIPE_BOOK_ORDINAL: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Open(Kind),
    Close(Kind),
    Remove { index: u16, all: bool },
    /// O clique em um botão de uma janela de configuração: o pacote vai ao servidor.
    Config(ConfigPacket),
}

struct Open {
    kind: Kind,
    window: Window,
    list: ScrollList,
    /// O tipo escolhido na lista, por índice do registro.
    selected: Option<usize>,
    opened_ms: i64,
    selected_ms: i64,
    /// A janela de lados: o `currentType`, por índice em `getTransmissions()`.
    current: usize,
    /// `SideDataButton.otherBlockItem`: o item do bloco vizinho de cada lado, tirado quando a janela abre (o construtor do botão o lê uma vez).
    neighbours: [Option<ItemStack>; 6],
    /// A janela do livro: a aba, a busca, o filtro e a página (`VirtualRecipeBookComponent`).
    book: Option<rbv::View>,
    /// `EditBox.focusedTime` da caixa de busca (o pisca do cursor).
    book_focus_ms: i64,
    /// O relógio do `SlotSelectTime`: o último quadro que somou ao `time`.
    book_clock_ms: i64,
    /// O `stackedContents` da última conferência do que dá para fazer (`updateStackedContents`).
    book_contents: StackedContents,
}

struct Client {
    /// O bloco da tela a que o estado pertence; trocar de tela começa de novo.
    screen_block: Option<String>,
    /// Onde esse bloco fica: o espelho do menu dele mora ali.
    screen_pos: Option<[i32; 3]>,
    positions: Option<WindowPositions>,
    /// As janelas abertas, a do topo primeiro (`GuiMekanism.windows`, uma `LRU`).
    windows: Vec<Open>,
    pending: Vec<Action>,
    /// A seleção que o servidor já sabe, para só mandar a mudança.
    sent_selection: Option<Option<SelectedWindow>>,
    /// Um botão foi clicado neste quadro (`AbstractWidget.playDownSound`).
    click_sound: bool,
}

static CLIENT: Mutex<Client> = Mutex::new(Client {
    screen_block: None,
    screen_pos: None,
    positions: None,
    windows: Vec::new(),
    pending: Vec::new(),
    sent_selection: None,
    click_sound: false,
});

fn client() -> std::sync::MutexGuard<'static, Client> {
    CLIENT.lock().unwrap_or_else(|e| e.into_inner())
}

/// O menu de uma tela aberta no cliente: o espelho de `trackedData` e onde, nele, a janela de upgrades pôs os dados dela.
struct ScreenMirror {
    mirror: Mirror,
    /// As posições de `upgradeTicks` e do mapa, enquanto a janela acompanha (`startTracking`).
    window: Option<[usize; 2]>,
}

/// Os espelhos por posição do bloco. É um `Mutex` porque o desenho e a entrada leem, e a rede do jogo escreve.
static MIRRORS: Mutex<Option<HashMap<[i32; 3], ScreenMirror>>> = Mutex::new(None);

fn mirrors<R>(act: impl FnOnce(&mut HashMap<[i32; 3], ScreenMirror>) -> R) -> R {
    let mut guard = MIRRORS.lock().unwrap_or_else(|e| e.into_inner());
    act(guard.get_or_insert_with(HashMap::new))
}

/// `PacketUpdateContainer.handle` no cliente: cada dado vai para a posição dele no menu aberto (`handleWindowProperty`). Um pacote que
/// abre o menu traz o id do bloco, que diz a lista de dados; o menu que o cliente já tinha para a posição (uma janela fixa abriu
/// antes do primeiro pacote) fica.
fn container_from_server(data: &[u8]) {
    let Some(packet) = wire::decode(data) else {
        eprintln!("machinery_basis: o servidor mandou dados de menu ilegíveis");
        return;
    };
    mirrors(|table| {
        if let Some(block) = &packet.opens
            && let Some(spec) = spec_of(block)
        {
            table.entry(packet.pos).or_insert_with(|| ScreenMirror {
                mirror: Mirror::new(&(spec.trackers)()),
                window: None,
            });
        }
        if let Some(entry) = table.get_mut(&packet.pos) {
            for (index, value) in &packet.data {
                entry.mirror.apply(*index, value);
            }
        }
    });
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as i64)
}

/// A posição guardada no cliente: o `client.toml` do Mekanism (`config/Mekanism/client.toml`, que o mod diz), onde o original guarda as posições de todas
/// as janelas, nas seções `window.<nome>`.
fn positions_path(spec: &UpgradeWindowSpec) -> Option<std::path::PathBuf> {
    Some(Game::native_config_dir().ok()?.join(spec.client_config))
}

fn load_positions(spec: &UpgradeWindowSpec) -> WindowPositions {
    positions_path(spec)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| WindowPositions::from_toml(&text))
        .unwrap_or_default()
}

fn save_positions(spec: &UpgradeWindowSpec, positions: &WindowPositions) {
    if let Some(path) = positions_path(spec) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // O arquivo é o `client.toml` inteiro do mod: as seções `window.*` são nossas e as outras (`sounds`) não.
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let _ = std::fs::write(path, machinery_basis_core::client_config::replace_sections(&existing, "window", &positions.to_toml(spec.window_types)));
    }
}

/// O tamanho da janela do jogo em pixels de GUI (`getGuiScaledWidth` e `getGuiScaledHeight`).
fn gui_size(game: &Game<'_>) -> Option<[i32; 2]> {
    let renderer = game.renderer.as_ref()?;
    let batch = HudBatch::new([renderer.config.width, renderer.config.height]);
    Some([batch.width as i32, batch.height as i32])
}

/// A spec do bloco da tela aberta, quando é uma tela nossa.
fn open_spec(game: &Game<'_>) -> Option<&'static UpgradeWindowSpec> {
    if !game.native_screen_shown() {
        return None;
    }
    spec_of(&game.mod_screen.as_ref()?.block)
}

fn screen_of(view: &NativeScreenView<'_>, game: &Game<'_>) -> Option<Screen> {
    let [width, height] = gui_size(game)?;
    Some(Screen {
        left: view.rect[0] as i32,
        top: view.rect[1] as i32,
        scaled_width: width,
        scaled_height: height,
    })
}

fn mouse_of(view: &NativeScreenView<'_>) -> [f64; 2] {
    [f64::from(view.mouse[0]), f64::from(view.mouse[1])]
}

/// `ScrollList` e as posições dos filhos estão relativos à janela; o mouse também fica assim.
fn relative(open: &Open, screen: Screen, mouse: [f64; 2]) -> [f64; 2] {
    let [x, y, _, _] = open.window.bounds(screen);
    [mouse[0] - f64::from(x), mouse[1] - f64::from(y)]
}

fn in_rect(rect: [i32; 4], at: [f64; 2]) -> bool {
    at[0] >= f64::from(rect[0])
        && at[1] >= f64::from(rect[1])
        && at[0] < f64::from(rect[0] + rect[2])
        && at[1] < f64::from(rect[1] + rect[3])
}

// ---------------------------------------------------------------------------------------------------------------------------
// O que muda a cada quadro: ações pedidas pela entrada, slots virtuais no lugar e mensagens ao servidor.

fn send(game: &mut Game<'_>, body: Vec<u8>) {
    if game.remote.is_some() {
        let _ = game.native_net_send_to_server(CHANNEL, &body);
    } else {
        // Sem servidor remoto o jogador da janela é o anfitrião, que a tabela guarda como 0.
        server_message(game, 0, &body);
    }
}

/// O índice da janela de `kind` na pilha (0 é o topo).
fn index_of(state: &Client, kind: Kind) -> Option<usize> {
    state.windows.iter().position(|open| open.kind == kind)
}

/// A janela de upgrades aberta, se há.
fn upgrade_open(state: &Client) -> Option<&Open> {
    state.windows.iter().find(|open| open.kind == Kind::Upgrade)
}

/// A seleção de janela que o servidor deve ter: a do topo da pilha (`onFocused` → `setSelectedWindow`), ou nenhuma
/// (`lastWindowRemoved`).
fn wanted_selection(state: &Client, spec: &UpgradeWindowSpec) -> Option<SelectedWindow> {
    state.windows.first().map(|open| open.kind.selection(spec))
}

/// Manda ao servidor a seleção, quando muda. Roda depois das ações do quadro.
fn sync_selection(game: &mut Game<'_>, spec: &UpgradeWindowSpec) {
    let (wanted, known) = {
        let state = client();
        (wanted_selection(&state, spec), state.sent_selection)
    };
    if known != Some(wanted) {
        let mut body = vec![OP_SELECT];
        body.extend(window::encode_selection(wanted));
        send(game, body);
        client().sent_selection = Some(wanted);
    }
}

/// `GuiWindow.close`: tira a janela, grava onde ela estava e, a de upgrades, avisa o servidor que parou de acompanhar os dados.
fn close_window(game: &mut Game<'_>, spec: &UpgradeWindowSpec, kind: Kind, tell_server: bool) {
    let pos = {
        let mut guard = client();
        let state = &mut *guard;
        let Some(index) = index_of(state, kind) else {
            return;
        };
        let open = state.windows.remove(index);
        if kind == Kind::RecipeBook && !open.window.pinned {
            // `GuiRecipeBookWindow.close`: sem fixar, `toggleVisibility` fecha o livro nos dados.
            set_book_open(spec, false);
        }
        let positions = state.positions.get_or_insert_with(|| load_positions(spec));
        if open.window.close(positions) {
            save_positions(spec, positions);
        }
        (state.screen_pos, kind.is_config() && !state.windows.iter().any(|open| open.kind.is_config()))
    };
    let (pos, last_config) = pos;
    if last_config {
        if let Some(pos) = pos {
            config_mirror(|table| table.remove(&pos));
        }
        if tell_server {
            send(game, vec![OP_CONFIG_TRACK, 0]);
        }
    }
    if kind != Kind::Upgrade {
        return;
    }
    // `stopTracking`: os dados da janela saem do menu, no cliente e (pelo pedido) no servidor.
    if let Some(pos) = pos {
        mirrors(|table| {
            if let Some(entry) = table.get_mut(&pos) {
                entry.mirror.stop_tracking(&UPGRADE_WINDOW_KEY);
                entry.window = None;
            }
        });
    }
    if tell_server {
        send(game, vec![OP_STOP]);
    }
}

/// `GuiWindowCreatorTab.openWindow`.
fn open_window(game: &mut Game<'_>, spec: &UpgradeWindowSpec, kind: Kind) {
    let Some(size) = gui_size(game) else {
        return;
    };
    let Some(rect) = game.mod_screen.as_ref().map(|screen| screen.screen.width) else {
        return;
    };
    let screen = {
        let origin = game_origin(game);
        Screen {
            left: origin[0],
            top: origin[1],
            scaled_width: size[0],
            scaled_height: size[1],
        }
    };
    let (default, dimensions) = match kind {
        Kind::Upgrade => {
            let types = spec_types(spec);
            let layout = SupportedLayout::new(game.native_text_width(&lang::get("gui.mekanism.upgrades.supported")) as i32);
            let rows = layout.needed_rows(types.len());
            (uw::default_position(rect as i32), [uw::WINDOW_WIDTH, uw::window_height(rows)])
        }
        // `GuiSideConfigurationTab.createWindow`: `(getGuiWidth() - 156) / 2` na horizontal e 15 na vertical.
        Kind::Side => ([(rect as i32 - cw::SIDE_SIZE[0]) / 2, 15], cw::SIDE_SIZE),
        Kind::Transporter => ([(rect as i32 - cw::TRANSPORTER_SIZE[0]) / 2, 15], cw::TRANSPORTER_SIZE),
        // `GuiRecipeBookWindowTab.createWindow`: `((getGuiWidth() - WIDTH_WITH_TABS) / 2, HEIGHT)`, com `IMAGE_WIDTH` por `HEIGHT`.
        Kind::RecipeBook => ([(rect as i32 - rb::WIDTH_WITH_TABS) / 2, rb::WINDOW_HEIGHT], [rb::IMAGE_WIDTH, rb::WINDOW_HEIGHT]),
    };
    let neighbours: [Option<ItemStack>; 6] = match (kind, config_view(game, spec)) {
        (Kind::Side | Kind::Transporter, Some(view)) => RelativeSide::VALUES.map(|side| neighbour_item(game, &view, side)),
        _ => Default::default(),
    };
    let contents = if kind == Kind::RecipeBook { book_contents(game, spec) } else { StackedContents::default() };
    let mut guard = client();
    let state = &mut *guard;
    let positions = state.positions.get_or_insert_with(|| load_positions(spec));
    let window = Window::new(kind.selection(spec), positions, screen, default, dimensions);
    let list = ScrollList::new(LIST[0], LIST[1], LIST[2], LIST[3]);
    let now = now_ms();
    // `GuiMekanism.addWindow`: a nova fica no topo.
    state.windows.insert(
        0,
        Open {
            kind,
            window,
            list,
            selected: None,
            opened_ms: now,
            selected_ms: now,
            current: 0,
            neighbours,
            book: (kind == Kind::RecipeBook).then(|| new_book_view(spec, &contents)).flatten(),
            book_focus_ms: now,
            book_clock_ms: now,
            book_contents: contents,
        },
    );
    if kind == Kind::RecipeBook {
        // `GuiRecipeBookWindow`: a janela que nasce com o livro aberto nos dados vale como fixada (antes de `adoptPosition`, que o abre).
        let open = &mut state.windows[0];
        if !open.window.pinned && book_open(spec) {
            open.window.toggle_pinned();
        }
        set_book_open(spec, true);
    }
    let first_config = kind.is_config() && state.windows.iter().filter(|open| open.kind.is_config()).count() == 1;
    drop(guard);
    if first_config {
        send(game, vec![OP_CONFIG_TRACK, 1]);
    }
    if kind != Kind::Upgrade {
        return;
    }
    // `startTracking`: a janela acrescenta os dados dela ao menu, no cliente e (pelo pedido) no servidor, nas mesmas posições.
    if let Some(pos) = client().screen_pos {
        mirrors(|table| {
            if let Some(entry) = table.get_mut(&pos) {
                let positions = entry.mirror.start_tracking(UPGRADE_WINDOW_KEY, &(spec.window_trackers)());
                entry.window = positions.try_into().ok();
            }
        });
    }
    // O `PacketGuiInteract` do `startTracking`.
    send(game, vec![OP_TRACK]);
}

/// O canto da tela do jogo em pixels de GUI, como o último desenho o deixou.
fn game_origin(game: &Game<'_>) -> [i32; 2] {
    match game.renderer.as_ref() {
        Some(renderer) => {
            let batch = HudBatch::new([renderer.config.width, renderer.config.height]);
            let (x, y) = game.inventory_origin(&batch);
            [x as i32, y as i32]
        }
        None => [0, 0],
    }
}

fn spec_types(spec: &UpgradeWindowSpec) -> Vec<String> {
    (spec.data)().type_ids()
}

/// O que o servidor mandou da configuração e do ejetor, por bloco, enquanto uma janela de configuração está aberta.
static CONFIG_MIRROR: Mutex<Option<HashMap<[i32; 3], Vec<u8>>>> = Mutex::new(None);

fn config_mirror<R>(act: impl FnOnce(&mut HashMap<[i32; 3], Vec<u8>>) -> R) -> R {
    let mut guard = CONFIG_MIRROR.lock().unwrap_or_else(|e| e.into_inner());
    act(guard.get_or_insert_with(HashMap::new))
}

/// A configuração de lados e o ejetor do bloco da tela aberta, quando o bloco os tem: o que o tile guarda, e por cima o que o servidor
/// mandou por último.
fn config_view(game: &Game<'_>, spec: &UpgradeWindowSpec) -> Option<ConfigView> {
    let owner = config_owner(spec.block)?;
    let pos = game.mb_open_screen_pos()?;
    let mut view = (owner.view)(game, pos)?;
    if let Some(bytes) = config_mirror(|table| table.get(&pos).cloned()) {
        let ConfigView { config, ejector, .. } = &mut view;
        cw::apply_snapshot(&bytes, config, ejector);
    }
    Some(view)
}

/// Se o bloco tem as abas de configuração de lados e de transporte (`ISideConfiguration`).
fn has_config(spec: &UpgradeWindowSpec) -> bool {
    config_owner(spec.block).is_some()
}

/// O suporte da aba que abre a janela `kind` (`GuiSideConfigurationTab` em (-26, 6), `GuiTransporterConfigTab` em (-26, 34), 26 por 26).
fn left_tab(kind: Kind) -> Option<InsetTab> {
    let y = match kind {
        Kind::Side => 6,
        Kind::Transporter => 34,
        Kind::Upgrade | Kind::RecipeBook => return None,
    };
    Some(InsetTab { x: -26, y, height: 26, left: true })
}

/// O hook `frame`: cuida do que a entrada pediu, põe os slots virtuais onde a janela está e esconde-os quando ela fecha.
pub(in crate::game) fn frame(game: &mut Game<'_>) {
    let Some(spec) = open_spec(game) else {
        // A tela fechou: `GuiMekanism.removed` fecha as janelas e grava a posição, sem precisar avisar o servidor, que larga o menu.
        let had_window = {
            let state = client();
            !state.windows.is_empty() || state.screen_block.is_some()
        };
        if had_window {
            let block = client().screen_block.clone();
            if let Some(spec) = block.and_then(|block| spec_of(&block)) {
                for kind in Kind::ALL {
                    close_window(game, spec, kind, false);
                }
            }
            let mut guard = client();
            guard.screen_block = None;
            guard.pending.clear();
            guard.sent_selection = None;
            // `MekanismContainer.removed`: o menu some com a tela.
            if let Some(pos) = guard.screen_pos.take() {
                mirrors(|table| table.remove(&pos));
                config_mirror(|table| table.remove(&pos));
            }
        }
        // O que o fechamento da tela mudou no livro (`RecipeBookComponent.removed` não manda nada; o `toggleVisibility` do fechar, sim).
        book_flush(game);
        return;
    };
    // Uma tela nova: `initPinnedWindows` reabre a que ficou fixa.
    {
        let mut guard = client();
        let state = &mut *guard;
        let at = game.mb_open_screen_pos();
        let moved = matches!((state.screen_pos, at), (Some(old), Some(new)) if old != new);
        if state.screen_block.as_deref() != Some(spec.block) || moved {
            // Um menu que substitui outro de um bloco igual (o vizinho): o espelho do anterior vai embora.
            if moved && let Some(old) = state.screen_pos {
                mirrors(|table| table.remove(&old));
            }
            state.screen_block = Some(spec.block.to_owned());
            state.screen_pos = at;
            state.windows.clear();
            state.pending.clear();
            // Um menu novo começa sem janela escolhida no servidor.
            state.sent_selection = Some(None);
            let positions = state.positions.get_or_insert_with(|| load_positions(spec));
            for kind in Kind::ALL {
                // `GuiRecipeBookWindowTab.wasPinned`: o livro aberto nos dados.
            let pinned = if kind == Kind::RecipeBook { book_open(spec) } else { positions.was_pinned(kind.selection(spec)) };
            if kind.offered(spec) && pinned {
                    state.pending.push(Action::Open(kind));
                }
            }
        }
    }
    // `SlotSelectTime`: o `time` do livro soma o quadro (`extractRenderState`, `time += a`, em ticks) só sem o Ctrl.
    {
        let ctrl = game.has_control_down();
        let now = now_ms();
        let book_window = index_of(&client(), Kind::RecipeBook).is_some();
        // `RecipeBookComponent.tick`: o inventário mudou (`getTimesChanged`), então o que dá para fazer é conferido de novo; e o
        // `recipesUpdated`, quando o servidor mandou ou tirou receitas.
        let contents = book_window.then(|| book_contents(game, spec));
        let dirty = BOOK_DIRTY.swap(false, Ordering::Relaxed);
        let mut guard = client();
        for open in guard.windows.iter_mut() {
            if let Some(view) = open.book.as_mut() {
                let ticks = (now - open.book_clock_ms) as f32 / 50.0;
                if !ctrl {
                    view.time += ticks;
                }
                view.advance_animations(ticks);
                open.book_clock_ms = now;
                if let Some(contents) = &contents {
                    let data = view.data();
                    if dirty {
                        let (known, highlighted): (Vec<i32>, Vec<i32>) = unlocked(|book| (book.known().collect(), book.highlighted().collect()));
                        view.update_book(known.into_iter(), highlighted.into_iter(), &|row| data.can_craft(row.0, contents));
                        open.book_contents = contents.clone();
                    } else if open.book_contents != *contents {
                        view.reselect(&|row| data.can_craft(row.0, contents));
                        open.book_contents = contents.clone();
                    }
                }
                // `recipeShown`: o que os botões mostraram deixa de ser destacado no livro do jogador, aqui e no servidor.
                for id in view.seen.drain(..) {
                    book_seen(id);
                }
            }
        }
    }
    let actions: Vec<Action> = std::mem::take(&mut client().pending);
    for action in actions {
        match action {
            Action::Open(kind) => {
                let closed = index_of(&client(), kind).is_none();
                if closed {
                    open_window(game, spec, kind);
                }
            }
            Action::Close(kind) => close_window(game, spec, kind, true),
            Action::Remove { index, all } => {
                send(game, [&[OP_REMOVE][..], &uw::encode_remove(index, all)].concat());
            }
            Action::Config(packet) => {
                send(game, [&[OP_CONFIG][..], &packet.encode()].concat());
            }
        }
    }
    book_flush(game);
    sync_selection(game, spec);
    place_virtual_slots(game, spec);
    // `MekanismButton`, `GuiInsetElement` e `GuiCloseButton` tocam `UI_BUTTON_CLICK` (volume 0,25) ao clicar.
    if std::mem::take(&mut client().click_sound) {
        game.audio.play_button_click();
    }
}

/// Os slots virtuais ficam na posição que a janela dá a eles, ou somem (`None`) sem janela.
fn place_virtual_slots(game: &mut Game<'_>, spec: &UpgradeWindowSpec) {
    if !spec.upgrades {
        return;
    }
    let wanted: [Option<[f32; 2]>; 2] = {
        let state = client();
        match upgrade_open(&state) {
            Some(open) => [0, 1].map(|n| {
                Some([
                    (open.window.relative_x + SLOT_ITEMS[n][0]) as f32,
                    (open.window.relative_y + SLOT_ITEMS[n][1]) as f32,
                ])
            }),
            None => [None, None],
        }
    };
    if let Some(screen) = game.mod_screen.as_mut() {
        let slots = &mut screen.screen.slots;
        let needed = spec.virtual_slots[0].max(spec.virtual_slots[1]) + 1;
        if slots.len() < needed {
            slots.resize(needed, None);
        }
        for (index, want) in spec.virtual_slots.iter().zip(wanted) {
            if slots[*index] != want {
                slots[*index] = want;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// Entrada.

/// `screen_key_pressed_pre`: Esc fecha a janela que não está fixa e consome a tecla; fixa, a tecla segue para a de baixo
/// (`GuiMekanism.keyPressed` oferece a tecla às janelas, da do topo para baixo) e, sem nenhuma, a tela fecha.
pub(in crate::game) fn key_pressed(game: &Game<'_>, key: &str, canceled: &mut bool) {
    if open_spec(game).is_none() {
        return;
    }
    let mut guard = client();
    let state = &mut *guard;
    // `GuiMekanism.keyPressed`: as janelas, da do topo para baixo; a do livro passa a tecla antes ao componente.
    for open in state.windows.iter_mut() {
        if open.kind == Kind::RecipeBook && book_key(open, key) {
            *canceled = true;
            return;
        }
    }
    if key != "key.keyboard.escape" {
        return;
    }
    if let Some(open) = state.windows.iter().find(|open| open.window.escape()) {
        let kind = open.kind;
        state.pending.push(Action::Close(kind));
        *canceled = true;
    }
}

/// `screen_char_typed_pre`: o caractere vai às janelas, da do topo para baixo (`GuiMekanism.charTyped`); a caixa de busca do livro o toma.
pub(in crate::game) fn char_typed(game: &Game<'_>, ch: char, canceled: &mut bool) {
    if open_spec(game).is_none() {
        return;
    }
    let mut guard = client();
    for open in guard.windows.iter_mut() {
        if open.kind == Kind::RecipeBook && book_char(open, ch) {
            *canceled = true;
            return;
        }
    }
}

/// O botão de dentro de uma aba da janela `kind` que o mouse (relativo ao canto da tela) acerta, com a janela fechada.
fn tab_hit(state: &Client, spec: &UpgradeWindowSpec, rel: [f64; 2]) -> Option<Kind> {
    let hit = |button: [f32; 4]| in_rect([button[0] as i32, button[1] as i32, button[2] as i32, button[3] as i32], rel);
    if spec.upgrades && index_of(state, Kind::Upgrade).is_none() && hit(shifted(TAB_BUTTON, 0.0, 0.0)) {
        return Some(Kind::Upgrade);
    }
    if has_config(spec) {
        for kind in [Kind::Side, Kind::Transporter] {
            if index_of(state, kind).is_none()
                && let Some(tab) = left_tab(kind)
                && hit(tab.button())
            {
                return Some(kind);
            }
        }
    }
    None
}

/// O clique em `rel` cai no botão da aba do livro de receitas (`GuiInsetElement.isMouseOver`: só o botão de dentro, e a aba sempre ativa enquanto o port não tem a janela).
fn recipe_book_hit(spec: &UpgradeWindowSpec, rel: [f64; 2]) -> bool {
    let [x, y, w, h] = machinery_basis_core::side_tab::recipe_book_tab(RECIPE_BOOK_IMAGE_WIDTH).button();
    spec.recipe_book && in_rect([x as i32, y as i32, w as i32, h as i32], rel)
}

/// `AbstractContainerScreen.imageWidth` das telas de máquina.
const RECIPE_BOOK_IMAGE_WIDTH: i32 = 176;

/// `screen_mouse_button_pressed_pre`: a aba abre a janela, e na janela o clique é dela. Fora, a tela o recebe.
pub(in crate::game) fn mouse_pressed(
    game: &Game<'_>,
    view: &NativeScreenView<'_>,
    button: i32,
    modifiers: u32,
    canceled: &mut bool,
) {
    let Some(spec) = open_spec(game) else {
        return;
    };
    let Some(screen) = screen_of(view, game) else {
        return;
    };
    let mouse = mouse_of(view);
    let sync = read_sync(game);
    let config = if client().windows.iter().any(|open| open.kind.is_config()) {
        config_view(game, spec)
    } else {
        None
    };
    let mut guard = client();
    let state = &mut *guard;
    let shift = modifiers & 1 != 0;
    // As abas dos tipos são filhas da janela de lados e ficam fora dela: o clique nelas é da janela.
    if button == 0
        && let Some(view) = config.as_ref()
    {
        let rel = [mouse[0] - f64::from(screen.left), mouse[1] - f64::from(screen.top)];
        if let Some((index, tab)) = type_tab_at(state, view, rel) {
            state.windows[index].current = tab;
            let open = state.windows.remove(index);
            state.windows.insert(0, open);
            state.click_sound = true;
            *canceled = true;
            return;
        }
    }
    // `GuiMekanism.mouseClicked` oferece o clique a todas as janelas, da do topo para baixo, e `GuiRecipeBookWindow.mouseClicked` chama
    // `RecipeBookComponent.mouseClicked` antes de olhar se o mouse está nela: o livro recebe o clique em qualquer lugar (as abas dele ficam
    // fora da janela, 30 à esquerda), a menos que uma janela acima dele o tome.
    if let Some(book) = index_of(state, Kind::RecipeBook)
        && !(0..book).any(|above| state.windows[above].window.mouse_over(screen, mouse))
    {
        let rel = relative(&state.windows[book], screen, mouse);
        if book_click(&mut state.windows[book], rel, button, &mut state.click_sound, spec) == BookClick::Handled {
            if book != 0 {
                let open = state.windows.remove(book);
                state.windows.insert(0, open);
            }
            *canceled = true;
            return;
        }
    }
    // `GuiMekanism.mouseClicked`: as janelas, da do topo para baixo; a primeira que trata o clique vai para o topo.
    let mut hit = None;
    for index in 0..state.windows.len() {
        if let Click::Inside { .. } = state.windows[index].window.press(screen, mouse) {
            hit = Some(index);
            break;
        }
    }
    let Some(index) = hit else {
        // `GuiInsetElement.isMouseOver`: só com a aba ativa e sobre o botão de dentro, e só o botão esquerdo.
        let rel = [mouse[0] - f64::from(screen.left), mouse[1] - f64::from(screen.top)];
        if button == 0
            && let Some(kind) = tab_hit(state, spec, rel)
        {
            state.pending.push(Action::Open(kind));
            state.click_sound = true;
            *canceled = true;
        } else if button == 0 && recipe_book_hit(spec, rel) {
            // `GuiRecipeBookWindowTab.onClick` abre a janela do livro; com ela aberta a aba está inativa e o clique não é dela.
            if index_of(state, Kind::RecipeBook).is_none() {
                state.pending.push(Action::Open(Kind::RecipeBook));
                state.click_sound = true;
                *canceled = true;
            }
        }
        return;
    };
    let rel = relative(&state.windows[index], screen, mouse);
    let left_or_right = button == 0 || button == 1;
    let kind = state.windows[index].kind;
    match kind {
        Kind::Upgrade => {
            let open = &mut state.windows[index];
            // Os filhos, do último ao primeiro: os slots virtuais ficam com o clique do jogo (`slotClicked`).
            let on_slot = SLOT_ITEMS
                .iter()
                .any(|item| in_rect([item[0] - 1, item[1] - 1, 18, 18], rel));
            if on_slot {
                return;
            }
            *canceled = true;
            if left_or_right && open.selected.is_some() && in_rect(UNINSTALL, rel) {
                if let Some(index) = open.selected.and_then(|n| u16::try_from(n).ok()) {
                    state.pending.push(Action::Remove { index, all: shift });
                    state.click_sound = true;
                }
            } else if left_or_right && in_rect(PIN, rel) {
                open.window.toggle_pinned();
                state.click_sound = true;
            } else if left_or_right && in_rect(CLOSE, rel) {
                state.pending.push(Action::Close(Kind::Upgrade));
                state.click_sound = true;
            } else if button == 0 {
                let installed = sync.installed(&spec_types(spec));
                if open.list.press_thumb(rel, installed.len()) {
                    // O polegar tratou o clique.
                } else if let Some(row) = open.list.click_row(rel, installed.len()) {
                    let chosen = row.map(|i| installed[i]);
                    if open.selected != chosen {
                        open.selected = chosen;
                        open.selected_ms = now_ms();
                    }
                }
            }
        }
        Kind::RecipeBook => {
            *canceled = true;
            let pinned = state.windows[index].window.pinned;
            if left_or_right && in_rect(PIN, rel) {
                state.windows[index].window.toggle_pinned();
            } else if left_or_right && in_rect(CLOSE, rel) {
                state.pending.push(Action::Close(Kind::RecipeBook));
            }
            if state.windows[index].window.pinned != pinned || state.pending.contains(&Action::Close(Kind::RecipeBook)) {
                state.click_sound = true;
            }
        }
        Kind::Side | Kind::Transporter => {
            *canceled = true;
            let mut actions = Vec::new();
            let before = state.windows[index].current;
            let pinned = state.windows[index].window.pinned;
            press_config(&mut state.windows[index], rel, button, shift, config.as_ref(), &mut actions);
            // Um botão tratou o clique: uma ação, a troca da aba de tipo ou o fixar.
            if !actions.is_empty() || state.windows[index].current != before || state.windows[index].window.pinned != pinned {
                state.click_sound = true;
            }
            state.pending.extend(actions);
        }
    }
    // O clique tratado traz a janela para o topo (`windows.moveUp`), a menos que ela tenha pedido para fechar.
    if index != 0 && !state.pending.contains(&Action::Close(kind)) {
        let open = state.windows.remove(index);
        state.windows.insert(0, open);
    }
}

/// O clique `button` (0 esquerdo, 1 direito) em `rel`, na janela de lados ou de transporte, com o que ele manda ao servidor.
fn press_config(
    open: &mut Open,
    rel: [f64; 2],
    button: i32,
    shift: bool,
    view: Option<&ConfigView>,
    out: &mut Vec<Action>,
) {
    let left_or_right = button == 0 || button == 1;
    if !left_or_right {
        return;
    }
    if in_rect(PIN, rel) {
        open.window.toggle_pinned();
        return;
    }
    if in_rect(CLOSE, rel) {
        out.push(Action::Close(open.kind));
        return;
    }
    let Some(view) = view else {
        return;
    };
    let click = if button == 1 { cw::ClickType::Right } else { cw::ClickType::left(shift) };
    match open.kind {
        Kind::Side => {
            let transmissions = view.config.transmissions();
            let Some(current) = transmissions.get(open.current).copied() else {
                return;
            };
            let info = view.config.config(current);
            // Os botões de lado, depois o de limpar, o de ejetar, e as abas dos tipos.
            for side in RelativeSide::VALUES {
                let [x, y] = cw::side_button(side);
                let enabled = info.is_some_and(|info| info.is_side_enabled(side));
                if enabled && in_rect([x, y, cw::SIDE_BUTTON_SIZE, cw::SIDE_BUTTON_SIZE], rel) {
                    out.push(Action::Config(ConfigPacket::SideData { click, side, transmission: current }));
                    return;
                }
            }
            if in_rect(cw::CLEAR_BUTTON, rel) {
                let forward = button == 0;
                let target = cw::target_type(info, shift, forward);
                // Com shift o pacote vale para todos os tipos.
                let transmission = if shift { None } else { Some(current) };
                out.push(Action::Config(ConfigPacket::Batch { transmission, target }));
                return;
            }
            if info.is_some_and(|info| info.can_eject()) && in_rect(cw::EJECT_BUTTON, rel) {
                out.push(Action::Config(ConfigPacket::Eject { transmission: current }));
                return;
            }
            if button == 0 {
                for index in 0..transmissions.len() {
                    if index != open.current && in_rect(inset_rect(index, open), rel) {
                        open.current = index;
                        return;
                    }
                }
            }
        }
        Kind::Transporter => {
            if in_rect(cw::STRICT_BUTTON, rel) {
                out.push(Action::Config(ConfigPacket::StrictInput));
                return;
            }
            if in_rect(cw::OUTPUT_COLOR_BUTTON, rel) {
                let click = if button == 1 { cw::ClickType::Right } else { cw::ClickType::left(shift) };
                out.push(Action::Config(ConfigPacket::EjectColor { click }));
                return;
            }
            for side in RelativeSide::VALUES {
                let [x, y] = cw::transporter_button(side);
                let enabled = TileComponentEjector::is_input_side_enabled(&view.config, side);
                if enabled && in_rect([x, y, cw::SIDE_BUTTON_SIZE, cw::SIDE_BUTTON_SIZE], rel) {
                    out.push(Action::Config(ConfigPacket::InputColor { click, side }));
                    return;
                }
            }
        }
        Kind::Upgrade | Kind::RecipeBook => {}
    }
}

/// A aba de tipo (`GuiConfigTypeTab`, filha da janela de lados e fora dela) que o ponto `at` (relativo ao canto da tela) acerta: o índice da
/// janela na pilha e o do tipo. Só valem as abas visíveis (a do tipo atual fica escondida).
fn type_tab_at(state: &Client, view: &ConfigView, at: [f64; 2]) -> Option<(usize, usize)> {
    for (index, open) in state.windows.iter().enumerate() {
        if open.kind != Kind::Side {
            continue;
        }
        let rel = [at[0] - f64::from(open.window.relative_x), at[1] - f64::from(open.window.relative_y)];
        for tab in 0..view.config.transmissions().len() {
            if tab != open.current && in_rect(inset_rect(tab, open), rel) {
                return Some((index, tab));
            }
        }
    }
    None
}

/// O botão de dentro da aba do tipo `index` da janela de lados, relativo à janela.
fn inset_rect(index: usize, open: &Open) -> [i32; 4] {
    let tab = cw::type_tab(index, open.window.width);
    cw::inset_button([tab[0], tab[1]], index < 4)
}

/// `screen_mouse_button_released_pre`: solta o arrasto da janela e o da barra.
pub(in crate::game) fn mouse_released(game: &Game<'_>, canceled: &mut bool) {
    if open_spec(game).is_none() {
        return;
    }
    let mut state = client();
    for open in state.windows.iter_mut() {
        let was_dragging = open.window.dragging() || open.list.dragging();
        open.window.release();
        open.list.release();
        if was_dragging {
            *canceled = true;
        }
    }
}

/// `screen_mouse_dragged_pre`: a janela anda pela faixa de cima, e a barra da lista pelo polegar.
pub(in crate::game) fn mouse_dragged(game: &Game<'_>, view: &NativeScreenView<'_>, canceled: &mut bool) {
    if open_spec(game).is_none() {
        return;
    }
    let Some(screen) = screen_of(view, game) else {
        return;
    };
    let mouse = mouse_of(view);
    let mut state = client();
    for open in state.windows.iter_mut() {
        if open.window.dragging() {
            open.window.drag_to(screen, mouse);
            *canceled = true;
        }
        if open.list.dragging() {
            let rel = relative(open, screen, mouse);
            open.list.drag_to(rel[1]);
            *canceled = true;
        }
    }
}

/// `screen_mouse_scrolled_pre`: a roda sobre a lista a rola; sem barra o gesto segue para a tela (`InteractionStrategy.ALL`).
pub(in crate::game) fn mouse_scrolled(
    game: &Game<'_>,
    view: &NativeScreenView<'_>,
    scroll: [f64; 2],
    canceled: &mut bool,
) {
    if open_spec(game).is_none() {
        return;
    }
    let Some(screen) = screen_of(view, game) else {
        return;
    };
    let mouse = mouse_of(view);
    let total = read_sync(game).installed_count();
    let mut state = client();
    // `GuiMekanism.mouseScrolled` só oferece a roda à janela do topo.
    let Some(open) = state.windows.first_mut() else {
        return;
    };
    if open.kind != Kind::Upgrade {
        return;
    }
    let rel = relative(open, screen, mouse);
    if open.window.mouse_over(screen, mouse)
        && open.list.contains(rel)
        && open.list.adjust_scroll(scroll[1], total)
    {
        *canceled = true;
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// Slots: a ordem de desenho e o alvo do mouse.

/// O que o servidor sincronizou da janela de upgrades, lido do espelho do menu aberto: o `upgradeTicks` e o mapa, nas posições em que a
/// janela os acompanha. Sem janela acompanhando, ou antes do primeiro valor, o padrão.
fn read_sync(game: &Game<'_>) -> UpgradeSync {
    let Some(pos) = game.mb_open_screen_pos() else {
        return UpgradeSync::default();
    };
    mirrors(|table| {
        let Some(entry) = table.get(&pos) else {
            return UpgradeSync::default();
        };
        let Some([ticks, map]) = entry.window else {
            return UpgradeSync::default();
        };
        read_upgrade_map(entry.mirror.int(ticks), entry.mirror.bytes(map))
    })
}

fn virtual_ids(spec: &UpgradeWindowSpec) -> [usize; 2] {
    spec.virtual_slots.map(|index| 300 + index)
}

/// `Game::slot_at`, depois do Horizon: sob a janela só os slots virtuais dela são alvo, e um slot da máquina coberto não é
/// (`GuiMekanism.isHovering`), esteja ele antes ou depois do virtual na ordem dos slots.
pub(in crate::game) fn slot_at(game: &Game<'_>, found: Option<usize>, x: f32, y: f32) -> Option<usize> {
    let Some(spec) = open_spec(game) else {
        return found;
    };
    let state = client();
    if state.windows.is_empty() {
        return found;
    }
    let Some(renderer) = game.renderer.as_ref() else {
        return None;
    };
    let batch = HudBatch::new([renderer.config.width, renderer.config.height]);
    let (ox, oy) = game.inventory_origin(&batch);
    let rel = [x - ox, y - oy];
    if let Some(open) = upgrade_open(&state) {
        for (id, item) in virtual_ids(spec).into_iter().zip(SLOT_ITEMS) {
            let frame = [
                (open.window.relative_x + item[0] - 1) as f32,
                (open.window.relative_y + item[1] - 1) as f32,
            ];
            if rel[0] >= frame[0] && rel[0] < frame[0] + 18.0 && rel[1] >= frame[1] && rel[1] < frame[1] + 18.0 {
                return Some(id);
            }
        }
    }
    let inside = state.windows.iter().any(|open| {
        let [wx, wy] = [open.window.relative_x as f32, open.window.relative_y as f32];
        rel[0] >= wx && rel[0] < wx + open.window.width as f32 && rel[1] >= wy && rel[1] < wy + open.window.height as f32
    });
    if inside { None } else { found }
}

/// `Game::draw_inventory_slots`, no lugar do Horizon enquanto há janela: os itens dos slots normais, a janela por cima, e os
/// itens dos slots virtuais. O tooltip e o item no cursor o Horizon desenha depois, por cima de tudo. Devolve se tomou o desenho.
pub(in crate::game) fn draw_slots(game: &mut Game<'_>, batch: &mut HudBatch, ox: f32, oy: f32) -> bool {
    let Some(spec) = open_spec(game) else {
        return false;
    };
    if client().windows.is_empty() {
        return false;
    }
    let virtual_ids = virtual_ids(spec);
    let drag = game.drag_view();
    for i in game.slots() {
        if virtual_ids.contains(&i) {
            continue;
        }
        let (sx, sy) = game.slot_position(i);
        // Os fundos dos slots vazios da armadura e da mão (`ArmorSlot.getNoItemIcon`, `InventoryMenu.EMPTY_ARMOR_SLOT_SHIELD`), como o laço do Horizon.
        if i == 504 && game.offhand.is_none() {
            batch.sprite(&game.gui.atlas, "gui/sprites/container/slot/shield", ox + sx, oy + sy);
        }
        if (500..504).contains(&i) && game.armor[i - 500].is_none() {
            let icon = ["helmet", "chestplate", "leggings", "boots"][i - 500];
            batch.sprite(&game.gui.atlas, &format!("gui/sprites/container/slot/{icon}"), ox + sx, oy + sy);
        }
        let stack = game.slot_view(i);
        game.draw_slot_item(batch, drag.as_ref(), i, &stack, ox + sx, oy + sy);
    }
    draw_windows(game, spec, batch, ox, oy, &virtual_ids);
    true
}

/// `GuiMekanism.findElementOver` para o que a lib desenha: a janela de upgrades aberta, ou o botão da aba dela quando não há janela
/// (`GuiWindow` é ativo, e a aba fica inativa com a janela aberta). O ponto `at` é relativo ao canto da tela.
pub(in crate::game) fn over_element(game: &Game<'_>, at: [f32; 2]) -> bool {
    let Some(spec) = open_spec(game) else {
        return false;
    };
    let point = [f64::from(at[0]), f64::from(at[1])];
    if covers(at) || tab_hit(&client(), spec, point).is_some() {
        return true;
    }
    // As abas dos tipos da janela de lados são elementos dela, fora dos limites.
    let has_side = client().windows.iter().any(|open| open.kind == Kind::Side);
    has_side && config_view(game, spec).is_some_and(|view| type_tab_at(&client(), &view, point).is_some())
}

// ---------------------------------------------------------------------------------------------------------------------------
// Desenho.

fn sprite(spec: &UpgradeWindowSpec, name: &str) -> String {
    format!("{}:gui/sprites/{name}", spec.ns)
}

fn nine(game: &Game<'_>, batch: &mut HudBatch, spec: &UpgradeWindowSpec, name: &str, slice: NineSlice, rect: [f32; 4], argb: u32) {
    let name = sprite(spec, name);
    for cell in machinery_basis_core::gui::nine_slice_cells(slice, rect) {
        basis::sprite_tinted(game, batch, &name, cell.src, cell.dst, argb);
    }
}

fn plain(game: &Game<'_>, batch: &mut HudBatch, spec: &UpgradeWindowSpec, name: &str, size: [f32; 2], dst: [f32; 4]) {
    let _ = size;
    basis::sprite_tinted(game, batch, &sprite(spec, name), [0.0, 0.0, size[0], size[1]], dst, WHITE);
}

fn shifted(rect: [i32; 4], ox: f32, oy: f32) -> [f32; 4] {
    [ox + rect[0] as f32, oy + rect[1] as f32, rect[2] as f32, rect[3] as f32]
}

/// `GuiElement.drawButton`: o fundo do botão, `inactive`, `focus` com o mouse em cima, ou o comum.
fn button_background(
    game: &Game<'_>,
    batch: &mut HudBatch,
    spec: &UpgradeWindowSpec,
    rect: [f32; 4],
    active: bool,
    hovered: bool,
) {
    let name = if !active {
        "button_inactive"
    } else if hovered {
        "button_focus"
    } else {
        "button"
    };
    nine(game, batch, spec, name, uw::BUTTON_SLICE, rect, WHITE);
}

fn hovering(rect: [f32; 4], mouse: (f32, f32)) -> bool {
    mouse.0 >= rect[0] && mouse.1 >= rect[1] && mouse.0 < rect[0] + rect[2] && mouse.1 < rect[1] + rect[3]
}

/// Um trecho de texto na linha `(min_x, min_y)` a `(max_x, max_y)`, centrado ou à esquerda, que rola se passar da área
/// (`drawScaledScrollingString`). `since` é o instante de que a rolagem conta.
#[allow(clippy::too_many_arguments)]
fn scrolling_text(
    game: &Game<'_>,
    batch: &mut HudBatch,
    text: &str,
    area: [f32; 4],
    center: bool,
    color: u32,
    scale: f32,
    since: i64,
) {
    let [min_x, min_y, max_x, max_y] = area;
    let width = game.native_text_width(text) * scale;
    let area_width = max_x - min_x;
    let scrolls = width > area_width;
    let target_y = (min_y + max_y - LINE_HEIGHT) / 2.0;
    let target_x = if scrolls {
        min_x - uw::scroll_overflow(f64::from(width - area_width), now_ms() - since) as f32
    } else if center {
        min_x + (area_width - width) / 2.0
    } else {
        min_x
    };
    let start = batch.vertices.len();
    let y = if (scale - 1.0).abs() < f32::EPSILON {
        target_y
    } else {
        target_y + LINE_HEIGHT / 2.0 - LINE_HEIGHT / 2.0 * scale
    };
    game.native_text(batch, text, [target_x, y], color, false);
    if (scale - 1.0).abs() >= f32::EPSILON {
        batch.scale_since(start, target_x, y, scale, scale);
    }
    if scrolls {
        batch.clip_since(start, min_x, min_y, max_x, max_y);
    }
}

/// `WrappedTextRenderer.renderWithScale`: quebra em `floor(max / scale)` e desenha cada linha. Devolve quantas.
fn wrapped_text(game: &Game<'_>, batch: &mut HudBatch, text: &str, x: f32, y: f32, max: i32, scale: f32, color: u32) -> usize {
    let limit = if (scale - 1.0).abs() < f32::EPSILON { max } else { (max as f32 / scale).floor() as i32 };
    let lines = uw::wrap_lines(text, limit, &|piece: &str| game.native_text_width(piece));
    let y_add = LINE_HEIGHT / 2.0 - LINE_HEIGHT / 2.0 * scale;
    for (n, line) in lines.iter().enumerate() {
        let line_y = y + y_add + LINE_HEIGHT * n as f32 * scale;
        let start = batch.vertices.len();
        game.native_text(batch, line, [x, line_y], color, false);
        batch.scale_since(start, x, line_y, scale, scale);
    }
    lines.len()
}

fn upgrade_stack(game: &Game<'_>, spec: &UpgradeWindowSpec, id: &str) -> ItemStack {
    let mut stack = ItemStack::new(spec.item, 1);
    let _ = game.native_set_component(&mut stack, spec.item_component, Tag::String(JavaString::from(id)));
    stack
}

/// `IGuiWrapper.renderItem(stack, x, y, scale)`: o item no tamanho `16 * scale`, sem decorações.
fn small_item(game: &Game<'_>, batch: &mut HudBatch, stack: &ItemStack, at: [f32; 2], scale: f32) {
    game.native_item_stack(batch, stack, at, 16.0 * scale);
}

fn type_name(id: &str) -> String {
    let (ns, path) = id.split_once(':').unwrap_or(("minecraft", id));
    lang::get(&format!("upgrade.{ns}.{path}"))
}

fn type_description(id: &str) -> String {
    let (ns, path) = id.split_once(':').unwrap_or(("minecraft", id));
    lang::get(&format!("upgrade.{ns}.{path}.description"))
}

fn type_label(id: &str) -> String {
    uw::translate(&lang::get("gui.mekanism.upgrade"), &[type_name(id)])
}

/// As janelas abertas, da de baixo para a do topo (`getWindowsDescendingIterator`); a que não está no topo leva o `blur` por cima
/// (`GuiWindow.renderBlur`: o sprite `blur` com 30% de opacidade).
fn draw_windows(
    game: &mut Game<'_>,
    spec: &UpgradeWindowSpec,
    batch: &mut HudBatch,
    ox: f32,
    oy: f32,
    virtual_ids: &[usize; 2],
) {
    let drag = game.drag_view();
    let drag = drag.as_ref();
    let game = &*game;
    // `slot_at` passa pelo filtro desta mesma janela, que toma a trava do estado: pergunta antes de segurá-la.
    let hovered_slot = game.slot_at(batch, game.cursor.0, game.cursor.1);
    let config = config_view(game, spec);
    let state = client();
    for (index, open) in state.windows.iter().enumerate().rev() {
        match open.kind {
            Kind::Upgrade => draw_upgrade_window(game, spec, batch, open, ox, oy, virtual_ids, hovered_slot, drag),
            Kind::Side | Kind::Transporter => {
                draw_config_window(game, spec, batch, &state, open, config.as_ref(), ox, oy);
            }
            Kind::RecipeBook => draw_recipe_book_window(game, spec, batch, &state, open, ox, oy),
        }
        if index != 0 {
            let (wx, wy) = (ox + open.window.relative_x as f32, oy + open.window.relative_y as f32);
            nine(
                game,
                batch,
                spec,
                "blur",
                uw::BLUR_SLICE,
                [wx, wy, open.window.width as f32, open.window.height as f32],
                BLUR_TINT,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_upgrade_window(
    game: &Game<'_>,
    spec: &UpgradeWindowSpec,
    batch: &mut HudBatch,
    open: &Open,
    ox: f32,
    oy: f32,
    virtual_ids: &[usize; 2],
    hovered_slot: Option<usize>,
    drag: Option<&crate::game::DragView>,
) {
    let data = (spec.data)();
    let order = data.type_ids();
    let types = upgrade::current_types();
    let sync = read_sync(game);
    let installed = sync.installed(&order);
    let (wx, wy) = (ox + open.window.relative_x as f32, oy + open.window.relative_y as f32);
    let (w, h) = (open.window.width as f32, open.window.height as f32);
    let cursor = game.native_cursor();
    let mouse = (cursor[0], cursor[1]);
    let at = |rect: [i32; 4]| shifted(rect, wx, wy);
    let supported_layout = SupportedLayout::new(game.native_text_width(&lang::get("gui.mekanism.upgrades.supported")) as i32);

    // `renderBackgroundOverlay`: a sombra e o painel.
    nine(game, batch, spec, "shadow", uw::SHADOW_SLICE, [wx - 3.0, wy - 3.0, w + 6.0, h + 6.0], SHADOW_TINT);
    nine(game, batch, spec, "base", spec.base_slice, [wx, wy, w, h], WHITE);

    // Os slots vêm antes dos outros fundos: o `GuiSlot` desenha em `renderWidget`.
    for (n, id) in virtual_ids.iter().enumerate() {
        let item = [wx + SLOT_ITEMS[n][0] as f32, wy + SLOT_ITEMS[n][1] as f32];
        let frame = [item[0] - 1.0, item[1] - 1.0, 18.0, 18.0];
        plain(game, batch, spec, "slot/normal", [18.0, 18.0], frame);
        plain(game, batch, spec, "slot/overlay/upgrade", [18.0, 18.0], frame);
        let hovered = hovered_slot == Some(*id);
        if hovered {
            game.draw_slot_highlight(batch, false, item[0], item[1]);
        }
        let stack = match game.container.and_then(|key| game.world.block_entities.get(&key)) {
            Some(BlockEntity::Mod(m)) => m.items.get(spec.virtual_slots[n]).cloned().flatten(),
            _ => None,
        };
        game.draw_slot_item(batch, drag, *id, &stack, item[0], item[1]);
        if hovered {
            game.draw_slot_highlight(batch, true, item[0], item[1]);
        }
    }

    // Fechar e fixar.
    let close = at(CLOSE);
    button_background(game, batch, spec, close, true, hovering(close, mouse));
    plain(game, batch, spec, "button/close", [12.0, 12.0], close);
    let pin = at(PIN);
    button_background(game, batch, spec, pin, true, hovering(pin, mouse));
    plain(
        game,
        batch,
        spec,
        if open.window.pinned { "button/pinned" } else { "button/unpinned" },
        [24.0, 12.0],
        pin,
    );

    // A lista: o fundo, a moldura da barra, o polegar e as linhas.
    let list = at(LIST);
    nine(game, batch, spec, "element_holder", uw::ELEMENT_HOLDER_SLICE, list, WHITE);
    nine(game, batch, spec, "scroll/list_border", uw::LIST_BORDER_SLICE, [list[0] + 101.0, list[1] + 1.0, 6.0, 48.0], WHITE);
    let thumb = open.list.thumb_rect();
    plain(
        game,
        batch,
        spec,
        "scroll/list_bar",
        [4.0, 4.0],
        [wx + thumb[0] as f32, wy + thumb[1] as f32, 4.0, 4.0],
    );
    let first = open.list.first_visible(installed.len());
    let rel_mouse = [f64::from(mouse.0 - wx), f64::from(mouse.1 - wy)];
    let hovered_row = open.list.hovered_row(rel_mouse);
    for (row, index) in installed.iter().skip(first).take(open.list.focused() as usize).enumerate() {
        let tint = order
            .get(*index)
            .and_then(|id| types.get(id))
            .map_or(WHITE, |declared| (spec.color)(&declared.color));
        let (row_sprite, slot_sprite) = if open.selected == Some(*index) {
            ("list/selected", "list/slot_selected")
        } else if hovered_row == Some(row as i32) {
            ("list/hovered", "list/slot_hovered")
        } else {
            ("list/base", "list/slot")
        };
        let top = wy + 19.0 + 12.0 * row as f32;
        nine(game, batch, spec, row_sprite, uw::LIST_ROW_SLICE, [wx + 7.0, top, 100.0, 12.0], tint);
        let name = sprite(spec, slot_sprite);
        basis::sprite_tinted(game, batch, &name, [0.0, 0.0, 10.0, 10.0], [wx + 8.0, top + 1.0, 10.0, 10.0], tint);
        if let Some(id) = order.get(*index) {
            small_item(game, batch, &upgrade_stack(game, spec, id), [wx + 9.0, top + 2.0], 0.5);
        }
    }

    // `GuiSupportedUpgrades`: o fundo, e os ícones dos tipos, esmaecidos quando a máquina não os aceita.
    let supported = [wx + SUPPORTED[0] as f32, wy + SUPPORTED[1] as f32, 167.0, (12 * supported_layout.needed_rows(order.len()) + 2) as f32];
    nine(game, batch, spec, "element_holder", uw::ELEMENT_HOLDER_SLICE, supported, WHITE);
    for (n, id) in order.iter().enumerate() {
        let [px, py] = supported_layout.position(n);
        let (x, y) = (supported[0] + 1.0 + px as f32, supported[1] + 1.0 + py as f32);
        small_item(game, batch, &upgrade_stack(game, spec, id), [x, y], 0.75);
        if !data.supports(id) {
            game.native_fill(batch, [x, y, 12.0, 12.0], FADE);
        }
    }

    // A tela interna, e o progresso de instalação e o de remoção.
    nine(game, batch, spec, "inner_screen", uw::INNER_SCREEN_SLICE, at(INNER_SCREEN), WHITE);
    let installing = at(INSTALLING);
    plain(game, batch, spec, "progress/installing/empty", [10.0, 14.0], installing);
    let progress = (sync.ticks as f32 / upgrade::UPGRADE_TICKS_REQUIRED as f32).min(1.0);
    if progress > 0.0 {
        let fill = (progress * 13.0).ceil() + 1.0;
        basis::sprite_tinted(
            game,
            batch,
            &sprite(spec, "progress/installing/full"),
            [0.0, 0.0, 10.0, fill],
            [installing[0], installing[1], 10.0, fill],
            WHITE,
        );
    }
    plain(game, batch, spec, "progress/uninstalling/empty", [12.0, 12.0], at(UNINSTALLING));

    // O botão Uninstall: um botão digital, de centro vazado.
    let uninstall = at(UNINSTALL);
    let active = open.selected.is_some();
    let over = active && hovering(uninstall, mouse);
    let name = if !active {
        "button_digital_inactive"
    } else if over {
        "button_digital_focus"
    } else {
        "button_digital"
    };
    nine(game, batch, spec, name, uw::DIGITAL_SLICE, uninstall, WHITE);

    // O primeiro plano: o título, o texto do botão, os nomes da lista, "Supported:", e a tela interna.
    let title = lang::get("gui.mekanism.upgrades");
    scrolling_text(
        game,
        batch,
        &title,
        [wx + 34.0, wy + 5.0, wx + 194.0, wy + 5.0 + LINE_HEIGHT],
        true,
        TITLE_COLOR,
        1.0,
        open.opened_ms,
    );
    let button_color = if !active {
        DIGITAL_INACTIVE
    } else if over {
        DIGITAL_HOVER
    } else {
        DIGITAL_ACTIVE
    };
    scrolling_text(
        game,
        batch,
        &lang::get("gui.mekanism.upgrade.uninstall"),
        [uninstall[0] + 2.0, uninstall[1], uninstall[0] + 54.0, uninstall[1] + 13.0],
        true,
        button_color,
        1.0,
        open.opened_ms,
    );
    for (row, index) in installed.iter().skip(first).take(open.list.focused() as usize).enumerate() {
        if let Some(id) = order.get(*index) {
            let top = wy + 21.0 + 12.0 * row as f32;
            scrolling_text(game, batch, &type_name(id), [wx + 19.0, top, wx + 105.0, top + LINE_HEIGHT], false, TITLE_COLOR, 1.0, open.opened_ms);
        }
    }
    scrolling_text(
        game,
        batch,
        &lang::get("gui.mekanism.upgrades.supported"),
        [supported[0] + 2.0, supported[1] + 3.0, supported[0] + 165.0, supported[1] + 3.0 + LINE_HEIGHT],
        false,
        TITLE_COLOR,
        1.0,
        open.opened_ms,
    );
    draw_screen_text(game, spec, batch, open, &order, &sync, &data, wx, wy);

    // O realce do slot virtual sob o mouse fica por cima do item.
    for (n, id) in virtual_ids.iter().enumerate() {
        if hovered_slot == Some(*id) {
            game.native_fill(batch, [wx + SLOT_ITEMS[n][0] as f32, wy + SLOT_ITEMS[n][1] as f32, 16.0, 16.0], SLOT_HOVER);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_screen_text(
    game: &Game<'_>,
    spec: &UpgradeWindowSpec,
    batch: &mut HudBatch,
    open: &Open,
    order: &[String],
    sync: &UpgradeSync,
    data: &Rc<UpgradeData>,
    wx: f32,
    wy: f32,
) {
    let Some(id) = open.selected.and_then(|n| order.get(n)) else {
        wrapped_text(game, batch, &lang::get("gui.mekanism.upgrades.no_selection"), wx + 116.0, wy + 20.0, 56, 0.8, SCREEN_COLOR);
        return;
    };
    let amount = sync.count(open.selected.unwrap_or(0));
    let max = data.max(id).unwrap_or(0);
    let lines = wrapped_text(game, batch, &type_label(id), wx + 116.0, wy + 20.0, 55, 0.6, SCREEN_COLOR);
    let mut text_y = 4.0 + 6.0 * lines as f32;
    let mut line = |text: &str, text_y: f32| {
        let top = wy + 18.0 + text_y;
        scrolling_text(game, batch, text, [wx + 116.0, top, wx + 169.0, top + LINE_HEIGHT], false, SCREEN_COLOR, 0.6, open.selected_ms);
    };
    line(&uw::translate(&lang::get("gui.mekanism.upgrades.amount"), &[amount.to_string(), max.to_string()]), text_y);
    if let Some(effect) = uw::effect_text(data.supports(id), amount, max, (spec.max_multiplier)()) {
        text_y += 6.0;
        line(&uw::translate(&lang::get("gui.mekanism.upgrades.effect"), &[effect]), text_y);
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// A janela do livro de receitas (`GuiRecipeBookWindow`).

/// O canal do livro de receitas: os pacotes de `machinery_basis_core::recipe_book_data` nos dois sentidos (os do vanilla são
/// `ClientboundRecipeBookAddPacket`, `ClientboundRecipeBookRemovePacket`, `ClientboundRecipeBookSettingsPacket`,
/// `ServerboundRecipeBookChangeSettingsPacket` e `ServerboundRecipeBookSeenRecipePacket`).
const BOOK_CHANNEL: &str = "machinery_basis:recipe_book";

/// `ClientRecipeBook.bookSettings`: os `RecipeBookSettings` do jogador neste cliente, por tipo de livro. Chegam do servidor ao entrar
/// (`sendInitialRecipeBook`) e cada mudança volta a ele (`sendUpdateSettings`), que a grava no NBT do jogador.
static BOOK_SETTINGS: Mutex<Option<BookSettings>> = Mutex::new(None);

/// O tipo de livro da máquina: o `RecipeBookType` do tipo de receita dela (`MekanismRecipeType.getRecipeBookType`), compartilhado entre
/// as máquinas que o usam.
fn book_type(spec: &UpgradeWindowSpec) -> &'static str {
    spec.book.map_or(spec.block, |book| book.settings_type)
}

/// `ClientRecipeBook.known` e `highlight`: os displays que o servidor disse que o jogador conhece, por id. O servidor é quem guarda
/// (`ServerRecipeBook`, no NBT do jogador) e manda o que muda.
static BOOK_UNLOCKED: Mutex<Option<Unlocked>> = Mutex::new(None);

/// O livro do cliente mudou (um pacote do servidor): a janela aberta refaz as coleções (`RecipeBookComponent.recipesUpdated`).
static BOOK_DIRTY: AtomicBool = AtomicBool::new(false);

/// O que o cliente tem a dizer ao servidor (a configuração que mudou, a receita vista), mandado no quadro.
static BOOK_OUTBOX: Mutex<Vec<ToServer>> = Mutex::new(Vec::new());

/// No servidor: o livro que cada jogador já recebeu (0 é o anfitrião), para mandar só o que muda.
static BOOK_SENT: Mutex<Option<HashMap<u128, Unlocked>>> = Mutex::new(None);

fn unlocked<R>(act: impl FnOnce(&mut Unlocked) -> R) -> R {
    let mut guard = BOOK_UNLOCKED.lock().unwrap_or_else(|e| e.into_inner());
    act(guard.get_or_insert_with(Unlocked::default))
}

fn book_sent<R>(act: impl FnOnce(&mut HashMap<u128, Unlocked>) -> R) -> R {
    let mut guard = BOOK_SENT.lock().unwrap_or_else(|e| e.into_inner());
    act(guard.get_or_insert_with(HashMap::new))
}

/// Os livros que os mods registraram, um por tipo de livro, na ordem do registro (a mesma no servidor e no cliente).
fn books() -> Vec<&'static rbv::BookData> {
    let specs = SPECS.lock().unwrap_or_else(|e| e.into_inner());
    let mut out: Vec<&'static rbv::BookData> = Vec::new();
    for (spec, _) in specs.iter() {
        if let Some(book) = spec.book
            && !out.iter().any(|other| other.settings_type == book.settings_type)
        {
            out.push(book);
        }
    }
    out
}

fn book_types() -> Vec<&'static str> {
    books().iter().map(|book| book.settings_type).collect()
}

/// As linhas de todos os livros registrados: cada display com a receita dele.
fn book_rows() -> Vec<&'static rbv::Row> {
    books().into_iter().flat_map(|book| book.rows.iter().filter(move |row| row.2 == book.book_type)).collect()
}

/// `Inventory.fillStackedContents` e `MekanismContainer.fillCraftSlotsStackedContents`: os 36 slots do jogador (só o que serve para
/// montar: sem dano, encanto ou nome) e os slots de item de entrada e de saída da máquina (`IRecipeLookupHandler.fillStackedContents`,
/// que usa `accountStack`, sem esse filtro), cada pilha até o máximo do item.
fn book_contents(game: &mut Game<'_>, spec: &UpgradeWindowSpec) -> StackedContents {
    let mut contents = StackedContents::default();
    let mut stacks: Vec<ItemStack> =
        game.inventory.iter().take(36).flatten().filter(|stack| horizon_client::menu::place::usable_for_crafting(stack)).cloned().collect();
    for index in spec.book_slots {
        stacks.extend(game.slot_view(300 + index));
    }
    for stack in stacks {
        let limit = basis::stack_limit(game, &stack) as i32;
        contents.account_stack(&stack.item.to_string(), stack.count as i32, limit);
    }
    contents
}

fn new_book_view(spec: &UpgradeWindowSpec, contents: &StackedContents) -> Option<rbv::View> {
    let data = spec.book?;
    let (known, highlighted): (Vec<i32>, Vec<i32>) = unlocked(|book| (book.known().collect(), book.highlighted().collect()));
    let filtering = book_settings(|settings| settings.is_filtering(data.settings_type));
    Some(rbv::View::new(data, known.into_iter(), highlighted.into_iter(), filtering, &|row| data.can_craft(row.0, contents)))
}

fn book_settings<R>(act: impl FnOnce(&mut BookSettings) -> R) -> R {
    let mut guard = BOOK_SETTINGS.lock().unwrap_or_else(|e| e.into_inner());
    act(guard.get_or_insert_with(BookSettings::default))
}

/// `book.isOpen(type)`.
fn book_open(spec: &UpgradeWindowSpec) -> bool {
    book_settings(|settings| settings.is_open(book_type(spec)))
}

/// `RecipeBookComponent.sendUpdateSettings`: o tipo de livro com o aberto e o filtro de agora, para o servidor.
fn send_book_settings(book_type: &str) {
    let change = book_settings(|settings| ChangeSettings {
        book_type: book_type.to_owned(),
        open: settings.is_open(book_type),
        filtering: settings.is_filtering(book_type),
    });
    BOOK_OUTBOX.lock().unwrap_or_else(|e| e.into_inner()).push(ToServer::ChangeSettings(change));
}

/// `book.setOpen(type, open)` e o `sendUpdateSettings` que o segue (`RecipeBookComponent.setVisible`).
fn set_book_open(spec: &UpgradeWindowSpec, open: bool) {
    book_settings(|settings| settings.set_open(book_type(spec), open));
    send_book_settings(book_type(spec));
}

/// `book.setFiltering(type, filtering)` e o `sendUpdateSettings` do botão de filtro.
fn set_book_filtering(spec: &UpgradeWindowSpec, filtering: bool) {
    book_settings(|settings| settings.set_filtering(book_type(spec), filtering));
    send_book_settings(book_type(spec));
}

/// `LocalPlayer.removeRecipeHighlight`: o destaque sai do livro do cliente e o servidor fica sabendo
/// (`ServerboundRecipeBookSeenRecipePacket`).
fn book_seen(id: i32) {
    unlocked(|book| book.seen(id));
    BOOK_OUTBOX.lock().unwrap_or_else(|e| e.into_inner()).push(ToServer::SeenRecipe(id));
}

/// O que o cliente guardou para o servidor vai: pelo canal quando há servidor remoto, e direto quando o jogador é o anfitrião.
fn book_flush(game: &mut Game<'_>) {
    let messages: Vec<ToServer> = std::mem::take(&mut *BOOK_OUTBOX.lock().unwrap_or_else(|e| e.into_inner()));
    for message in messages {
        let body = message.encode();
        if game.remote.is_some() {
            let _ = game.native_net_send_to_server(BOOK_CHANNEL, &body);
        } else {
            book_server_message(game, 0, &body);
        }
    }
}

/// O cliente recebe um pacote do livro: as configurações (`ClientRecipeBook.setBookSettings`) ou as receitas (`add`, `remove`).
fn book_from_server(data: &[u8]) {
    match ToClient::decode(data) {
        Some(ToClient::Settings(bytes)) => match BookSettings::decode(&book_types(), &bytes) {
            Some(settings) => {
                book_settings(|mine| mine.replace_from(&settings));
                BOOK_DIRTY.store(true, Ordering::Relaxed);
            }
            None => eprintln!("machinery_basis: o servidor mandou configurações do livro ilegíveis"),
        },
        Some(packet) => {
            if unlocked(|book| book.apply(&packet)) {
                BOOK_DIRTY.store(true, Ordering::Relaxed);
            }
        }
        None => eprintln!("machinery_basis: o servidor mandou um pacote do livro ilegível"),
    }
}

// O servidor: o livro de cada jogador mora no `recipe_book::Settings` do Horizon (as receitas conhecidas e as destacadas, por nome, que ele
// grava em `recipeBook.recipes` e `toBeDisplayed` como o `ServerRecipeBook`) e, para os `RecipeBookSettings` dos tipos de livro de mod, no
// composto cru do jogador (`Level::player_extra`, `Guest::saved`), que a injeção `recipe_book_settings_save` leva ao que é gravado.

/// O livro do jogador `player` no servidor (0 é o anfitrião): o do nível quando é ele quem age, senão o do convidado guardado no hub.
fn with_server_book<R>(
    game: &mut Game<'_>,
    player: u128,
    act: impl FnOnce(&mut horizon_client::recipe_book::Settings, &mut Option<Tag>) -> R,
) -> Option<R> {
    if game.lan_acting_uuid == Some(player) || (player == 0 && game.lan_acting_uuid.is_none()) {
        return Some(act(&mut game.level.recipe_book, &mut game.level.player_extra));
    }
    let guest = game.lan.as_mut()?.hub.guests.iter_mut().find(|guest| guest.uuid == player)?;
    Some(act(&mut guest.recipe_book, &mut guest.saved))
}

/// O `recipeBook` do composto cru do jogador, criado se falta.
fn extra_book(extra: &mut Option<Tag>) -> Option<&mut Compound> {
    if !matches!(extra, Some(Tag::Compound(_))) {
        *extra = Some(Tag::Compound(Compound::new()));
    }
    let Some(Tag::Compound(root)) = extra else {
        return None;
    };
    let book = root.entry(JavaString::from("recipeBook")).or_insert_with(|| Tag::Compound(Compound::new()));
    if !matches!(book, Tag::Compound(_)) {
        *book = Tag::Compound(Compound::new());
    }
    match book {
        Tag::Compound(map) => Some(map),
        _ => None,
    }
}

/// `RecipeBookSettings.MAP_CODEC` na leitura, para os tipos de livro de mod: o campo ausente vale falso.
fn server_book_settings(extra: &Option<Tag>, types: &[&str]) -> BookSettings {
    let book = extra.as_ref().and_then(|tag| tag.get("recipeBook"));
    let mut settings = BookSettings::default();
    settings.read(types, |name| book.and_then(|book| book.get(name)).map(|tag| matches!(tag, Tag::Byte(flag) if *flag != 0)));
    settings
}

/// `ServerGamePacketListenerImpl.handleRecipeBookChangeSettingsPacket` (`setBookSetting`) e `handleRecipeBookSeenRecipePacket`
/// (`removeHighlight` da receita do display).
fn book_server_message(game: &mut Game<'_>, player: u128, data: &[u8]) {
    match ToServer::decode(data) {
        Some(ToServer::ChangeSettings(change)) => {
            if !book_types().contains(&change.book_type.as_str()) {
                return;
            }
            let (open, filtering) = settings_fields(&change.book_type);
            with_server_book(game, player, |_, extra| {
                let Some(book) = extra_book(extra) else {
                    return;
                };
                // O codec omite o campo que vale o padrão (`optionalFieldOf(nome, false)`).
                for (name, value) in [(open, change.open), (filtering, change.filtering)] {
                    if value {
                        book.insert(JavaString::from(name.as_str()), Tag::Byte(1));
                    } else {
                        book.remove(&JavaString::from(name.as_str()));
                    }
                }
            });
        }
        Some(ToServer::SeenRecipe(id)) => {
            let Some(recipe) = book_rows().into_iter().find(|row| row.0 == id).map(|row| row.1) else {
                return;
            };
            with_server_book(game, player, |book, _| book.remove_highlight(recipe));
        }
        None => {}
    }
}

/// O livro de cada jogador vai ao cliente dele: tudo na primeira vez (`ServerRecipeBook.sendInitialRecipeBook`: as configurações e as
/// receitas com `replace`) e depois só o que mudou (`addRecipes` e `removeRecipes`), seja qual for o caminho que mudou o livro (a máquina
/// que premiou, o `/recipe`, um avanço). Roda no começo do tick do servidor, fora do turno de um convidado.
fn book_server_sync(game: &mut Game<'_>) {
    if game.remote.is_some() || game.lan_acting_uuid.is_some() {
        return;
    }
    let rows = book_rows();
    if rows.is_empty() {
        return;
    }
    let types = book_types();
    let mut players: Vec<u128> = vec![0];
    if let Some(host) = game.lan.as_ref() {
        players.extend(host.hub.guests.iter().filter(|guest| guest.placed).map(|guest| guest.uuid));
    }
    book_sent(|sent| sent.retain(|player, _| players.contains(player)));
    for player in players {
        let state = with_server_book(game, player, |book, extra| {
            let now = Unlocked::from_recipes(
                rows.iter().map(|row| (row.0, row.1)),
                |recipe| book.recipes.contains(recipe),
                |recipe| book.to_be_displayed.contains(recipe),
            );
            (now, server_book_settings(extra, &types))
        });
        let Some((now, settings)) = state else {
            continue;
        };
        let before = book_sent(|sent| sent.get(&player).cloned());
        if before.as_ref() == Some(&now) {
            continue;
        }
        let mut packets = Vec::new();
        if before.is_none() {
            packets.push(ToClient::Settings(settings.encode(&types)));
        }
        packets.extend(now.sync(before.as_ref(), |id| {
            rows.iter().find(|row| row.0 == id).map(|row| rb::Entry {
                id: row.0,
                category: row.4.to_owned(),
                group: (row.3 >= 0).then_some(row.3),
            })
        }));
        book_sent(|sent| sent.insert(player, now));
        for packet in packets {
            let body = packet.encode();
            if player == 0 {
                book_from_server(&body);
            } else if let Err(error) = game.native_net_send_to_player(player, BOOK_CHANNEL, &body) {
                eprintln!("machinery_basis: o livro de receitas não chegou a {player:032x}: {error}");
            }
        }
    }
}

/// `GuiWindow.renderBackgroundOverlay` e `renderForeground` da janela do livro: a sombra, o painel, o fechar, o fixar e o título.
fn draw_recipe_book_window(
    game: &Game<'_>,
    spec: &UpgradeWindowSpec,
    batch: &mut HudBatch,
    state: &Client,
    open: &Open,
    ox: f32,
    oy: f32,
) {
    let (wx, wy) = (ox + open.window.relative_x as f32, oy + open.window.relative_y as f32);
    let (w, h) = (open.window.width as f32, open.window.height as f32);
    let cursor = game.native_cursor();
    let mouse = (cursor[0], cursor[1]);
    let under = state.windows.iter().position(|other| {
        let rel = [mouse.0 - ox - other.window.relative_x as f32, mouse.1 - oy - other.window.relative_y as f32];
        rel[0] >= 0.0 && rel[1] >= 0.0 && rel[0] < other.window.width as f32 && rel[1] < other.window.height as f32
    });
    let own = state.windows.iter().position(|other| std::ptr::eq(other, open));
    let hover_ok = under.is_some() && under == own;
    let hov = |rect: [f32; 4]| hover_ok && hovering(rect, mouse);
    let at = |rect: [i32; 4]| shifted(rect, wx, wy);

    nine(game, batch, spec, "shadow", uw::SHADOW_SLICE, [wx - 3.0, wy - 3.0, w + 6.0, h + 6.0], SHADOW_TINT);
    nine(game, batch, spec, "base", spec.base_slice, [wx, wy, w, h], WHITE);
    let close = at(CLOSE);
    button_background(game, batch, spec, close, true, hov(close));
    plain(game, batch, spec, "button/close", [12.0, 12.0], close);
    let pin = at(PIN);
    button_background(game, batch, spec, pin, true, hov(pin));
    plain(game, batch, spec, if open.window.pinned { "button/pinned" } else { "button/unpinned" }, [24.0, 12.0], pin);
    // `drawTitleText(RECIPE_BOOK, 5)`: de `14 + GuiPinButton.WIDTH` até a largura da janela, com o `maxLengthPad` de 4 dos dois lados.
    let title = lang::get("gui.mekanism.recipe_book");
    // `drawScrollingString(x + maxLengthPad, ..., x + width - maxLengthPad)`: a área vai de 34 a 143.
    scrolling_text(game, batch, &title, [wx + 34.0, wy + 5.0, wx + w - 4.0, wy + 5.0 + LINE_HEIGHT], true, TITLE_COLOR, 1.0, open.opened_ms);
    if let Some(view) = &open.book {
        draw_book_contents(game, batch, view, open.book_focus_ms, wx, wy + rb::Y_OFFSET as f32, hover_ok, mouse);
    }
}

/// A aba do livro sob `rel` (relativo à janela): o índice entre as visíveis.
fn book_tab_at(open: &Open, rel: [f64; 2]) -> Option<rbv::Tab> {
    let view = open.book.as_ref()?;
    view.visible_tabs()
        .into_iter()
        .enumerate()
        .find(|(index, _)| rb::contains(rb::tab_rect(BOOK_ORIGIN[0], BOOK_ORIGIN[1], *index), rel[0], rel[1]))
        .map(|(_, tab)| tab)
}

/// A origem do livro dentro da janela: `adoptPosition(getX(), getY() + Y_OFFSET)`.
const BOOK_ORIGIN: [i32; 2] = [0, rb::Y_OFFSET];

/// O que o clique no livro fez (`RecipeBookComponent.mouseClicked`), para quem o chama.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BookClick {
    /// O livro tratou o clique (`return true`): a janela vai para o topo e o clique não segue.
    Handled,
    /// Nada do livro sob o mouse: o clique segue para a janela (`GuiWindow.mouseClicked`).
    Missed,
}

/// `RecipeBookComponent.mouseClicked` (`button` do Horizon: 0 esquerdo, 1 direito, 2 do meio; no Java 26.3 `MOUSE_BUTTON_LEFT` é 1 e
/// `RIGHT` é 3), com `rel` relativo à janela. Na ordem do Java: a página (as setas e os botões das receitas), a lupa e a caixa de busca
/// (o foco), o filtro e as abas. O `GuiMekanism` oferece todo clique a todas as janelas, então um clique em qualquer lugar fora da caixa
/// tira o foco dela. Os cliques nas receitas só ficam registrados nesta rodada: o esquerdo (`getLastClickedRecipe` e `tryPlaceRecipe`)
/// e o direito (o overlay das variantes, `isOnlyOption`) são da rodada seguinte.
fn book_click(open: &mut Open, rel: [f64; 2], button: i32, click_sound: &mut bool, spec: &UpgradeWindowSpec) -> BookClick {
    let [xo, yo] = BOOK_ORIGIN;
    let left = button == 0;
    let tab = book_tab_at(open, rel);
    let Some(view) = open.book.as_mut() else {
        return BookClick::Missed;
    };
    let inside = |rect: rb::Rect| rb::contains(rect, rel[0], rel[1]);
    // `RecipeBookPage.mouseClicked`: as setas só com o esquerdo e visíveis; os botões das receitas com o esquerdo e o direito (`isValidClickButton`).
    if left && view.page.forward_visible() && inside(rb::forward_rect(xo, yo)) {
        view.forward();
        *click_sound = true;
        return BookClick::Handled;
    }
    if left && view.page.back_visible() && inside(rb::back_rect(xo, yo)) {
        view.back();
        *click_sound = true;
        return BookClick::Handled;
    }
    let shown = view.page.shown().len();
    if (button == 0 || button == 1) && (0..shown).any(|index| inside(rb::button_rect(xo, yo, index))) {
        *click_sound = true;
        return BookClick::Handled;
    }
    // A lupa foca a caixa com qualquer botão (`magnifierIconPlacement.containsPoint`); a caixa, só com o esquerdo, e sem som (`EditBox.playDownSound`).
    if inside(rb::magnifier_rect(xo, yo)) || (left && inside(rb::search_rect(xo, yo))) {
        // `EditBox.setFocused(true)` reinicia o `focusedTime` (o pisca do cursor) a cada vez.
        open.book_focus_ms = now_ms();
        view.focused = true;
        return BookClick::Handled;
    }
    view.focused = false;
    if left && inside(rb::filter_rect(xo, yo)) {
        let filtering = view.toggle_filtering();
        set_book_filtering(spec, filtering);
        *click_sound = true;
        return BookClick::Handled;
    }
    if left && let Some(tab) = tab {
        view.select_tab(tab);
        *click_sound = true;
        return BookClick::Handled;
    }
    BookClick::Missed
}

/// A tecla com o livro aberto (`GuiRecipeBookWindow.keyPressed` chama `RecipeBookComponent.keyPressed` antes do resto). Esc não é do
/// livro (`isOffsetNextToMainGUI`: o componente virtual tem o `xOffset` de 86). Com a caixa focada, o Backspace apaga e as outras teclas
/// param nela (`capturesInput`); sem foco, a tecla do chat (`T`) foca a caixa e o caractere dela não entra (`ignoreTextInput`).
fn book_key(open: &mut Open, key: &str) -> bool {
    let Some(view) = open.book.as_mut() else {
        return false;
    };
    view.ignore_text = false;
    if key == "key.keyboard.escape" {
        return false;
    }
    if view.focused {
        if key == "key.keyboard.backspace" {
            view.backspace();
        }
        return true;
    }
    if key == "key.keyboard.t" {
        view.ignore_text = true;
        view.focused = true;
        open.book_focus_ms = now_ms();
        return true;
    }
    false
}

/// O caractere com o livro aberto (`RecipeBookComponent.charTyped`): entra na caixa focada.
fn book_char(open: &mut Open, ch: char) -> bool {
    let Some(view) = open.book.as_mut() else {
        return false;
    };
    if view.ignore_text || !view.focused {
        return false;
    }
    view.type_char(ch)
}

/// `VirtualRecipeBookComponent.extractRenderState`: o fundo recortado em 7, a caixa de busca, as abas, o botão de filtro e a página
/// (`RecipeBookPage.extractRenderState`: a grade, o `Page n/m` e as setas), na origem (`xo`, `yo`) do livro.
#[allow(clippy::too_many_arguments)]
fn draw_book_contents(game: &Game<'_>, batch: &mut HudBatch, view: &rbv::View, focus_ms: i64, xo: f32, yo: f32, hover_ok: bool, mouse: (f32, f32)) {
    let atlas = &game.gui.atlas;
    let (xi, yi) = (xo as i32, yo as i32);
    let over = |rect: rb::Rect| hover_ok && hovering([rect[0] as f32, rect[1] as f32, rect[2] as f32, rect[3] as f32], mouse);
    let sprite_at = |batch: &mut HudBatch, name: &str, x: i32, y: i32| {
        batch.sprite(atlas, &format!("gui/sprites/{name}"), x as f32, y as f32);
    };
    // O fundo (`blit(RECIPE_BOOK_LOCATION, xo + 7, yo + 7, 1 + 7, 1 + 7, IMAGE_WIDTH - 14, IMAGE_HEIGHT - 14, 256, 256)`).
    let offset = 7;
    basis::sprite_tinted(
        game,
        batch,
        "gui/recipe_book",
        [(1 + offset) as f32, (1 + offset) as f32, (rb::IMAGE_WIDTH - 2 * offset) as f32, (rb::IMAGE_HEIGHT - 2 * offset) as f32],
        [xo + offset as f32, yo + offset as f32, (rb::IMAGE_WIDTH - 2 * offset) as f32, (rb::IMAGE_HEIGHT - 2 * offset) as f32],
        WHITE,
    );
    // A caixa de busca (`EditBox.extractWidgetRenderState`): o fundo `text_field` (`text_field_highlighted` com o foco), o texto em
    // (`x + 4`, `y + (altura - 8) / 2`), branco com sombra, e o cursor `_` no fim, que pisca a cada 300 ms desde o foco; vazia e sem foco,
    // a dica `Search...` (cinza e itálico, `SEARCH_HINT_STYLE`). O texto que não cabe nos 73 pixels de dentro mostra o fim (o cursor fica
    // sempre no fim nesta rodada).
    let search = rb::search_rect(xi, yi);
    let field = if view.focused { "gui/sprites/widget/text_field_highlighted" } else { "gui/sprites/widget/text_field" };
    horizon_client::ui::nine_slice(batch, atlas, field, search[0] as f32, search[1] as f32, search[2] as f32, search[3] as f32, u32::MAX);
    let text_at = [(search[0] + 4) as f32, (search[1] + (search[3] - 8) / 2) as f32];
    let inner = (search[2] - 8) as f32;
    let mut shown = view.search.as_str();
    while game.native_text_width(shown) > inner {
        let mut chars = shown.chars();
        chars.next();
        shown = chars.as_str();
    }
    let mut cursor_x = text_at[0];
    if !shown.is_empty() {
        game.native_text(batch, shown, text_at, WHITE, true);
        cursor_x += game.native_text_width(shown) + 1.0;
    }
    if view.search.is_empty() && !view.focused {
        let hint = horizon_client::text::Text::styled(
            lang::get("gui.recipebook.search_hint"),
            horizon_client::text::Style { color: Some(0xAAAAAA), italic: Some(true), ..Default::default() },
        );
        batch.text_styled(atlas, &hint, text_at[0], text_at[1], WHITE, true);
    }
    // `insert` (o cursor de 1 pixel) só vale com o cursor antes do fim ou com os 50 caracteres; senão, o `_`.
    let full = view.search.chars().count() >= rb::SEARCH_MAX_LENGTH;
    if view.focused && ((now_ms() - focus_ms) / 300) % 2 == 0 {
        if full {
            let x = cursor_x - 1.0;
            batch.fill(atlas, x, text_at[1] - 1.0, 1.0, 10.0, WHITE);
        } else {
            game.native_text(batch, "_", [cursor_x, text_at[1]], WHITE, true);
        }
    }
    // As abas: a bússola e as categorias com receita, uma sobre a outra, a escolhida 2 para a esquerda; a que abriu com receita destacada
    // aperta na vertical em volta de (`x + 8`, `y + 12`) enquanto a animação dura (`RecipeBookTabButton.extractContents`).
    for (index, tab) in view.visible_tabs().into_iter().enumerate() {
        let rect = rb::tab_rect(xi, yi, index);
        let selected = tab == view.tab;
        let left = if selected { -2 } else { 0 };
        let start = batch.vertices.len();
        sprite_at(batch, if selected { "recipe_book/tab_selected" } else { "recipe_book/tab" }, rect[0] + left, rect[1]);
        let (primary, secondary) = view.icons(tab);
        let item = |batch: &mut HudBatch, id: &str, x: i32| game.native_item_stack(batch, &ItemStack::new(id, 1), [x as f32, (rect[1] + 5) as f32], 16.0);
        if let Some(second) = secondary {
            item(batch, second, rect[0] + 14 + left);
            item(batch, primary, rect[0] + 3 + left);
        } else {
            item(batch, primary, rect[0] + 9 + left);
        }
        if let rbv::Tab::Category(category) = tab
            && view.tab_anim[category] > 0.0
        {
            batch.scale_since(start, (rect[0] + 8) as f32, (rect[1] + 12) as f32, 1.0, rbv::squeeze(view.tab_anim[category]));
        }
    }
    // O filtro: os sprites do forno (`MekanismRecipeBookComponent.filterButtonTextures`).
    let filter = rb::filter_rect(xi, yi);
    let state = if view.filtering { "furnace_filter_enabled" } else { "furnace_filter_disabled" };
    sprite_at(batch, &format!("recipe_book/{state}{}", if over(filter) { "_highlighted" } else { "" }), filter[0], filter[1]);
    // A página.
    if let Some((current, total)) = view.page_label() {
        // `gui.recipebook.page` é `%s/%s` (ou `%1$s`/`%2$s` numa tradução que os numera).
        let label = lang::get("gui.recipebook.page")
            .replacen("%1$s", &current.to_string(), 1)
            .replacen("%2$s", &total.to_string(), 1)
            .replacen("%s", &current.to_string(), 1)
            .replacen("%s", &total.to_string(), 1);
        let x = rb::page_text_x(xi, game.native_text_width(&label) as i32);
        game.native_text(batch, &label, [x as f32, (yi + 141) as f32], WHITE, true);
    }
    for (index, button) in view.buttons().iter().enumerate() {
        let rect = rb::button_rect(xi, yi, index);
        let name = match (button.craftable, button.many) {
            (true, true) => "slot_many_craftable",
            (true, false) => "slot_craftable",
            (false, true) => "slot_many_uncraftable",
            (false, false) => "slot_uncraftable",
        };
        // `RecipeButton.extractWidgetRenderState`: o botão que mostrou uma receita destacada cresce e volta em volta de (`x + 8`, `y + 12`).
        let start = batch.vertices.len();
        sprite_at(batch, &format!("recipe_book/{name}"), rect[0], rect[1]);
        let stack = ItemStack::new(button.item, 1);
        let mut at = 4;
        if button.stacked {
            game.native_item_stack(batch, &stack, [(rect[0] + at + 1) as f32, (rect[1] + at + 1) as f32], 16.0);
            at -= 1;
        }
        game.native_item_stack(batch, &stack, [(rect[0] + at) as f32, (rect[1] + at) as f32], 16.0);
        if view.button_anim[index] > 0.0 {
            let squeeze = rbv::squeeze(view.button_anim[index]);
            batch.scale_since(start, (rect[0] + 8) as f32, (rect[1] + 12) as f32, squeeze, squeeze);
        }
    }
    for (visible, rect, name) in [
        (view.page.forward_visible(), rb::forward_rect(xi, yi), "page_forward"),
        (view.page.back_visible(), rb::back_rect(xi, yi), "page_backward"),
    ] {
        if visible {
            sprite_at(batch, &format!("recipe_book/{name}{}", if over(rect) { "_highlighted" } else { "" }), rect[0], rect[1]);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// As janelas de configuração de lados e de transporte (`GuiSideConfiguration`, `GuiTransporterConfig`).

/// `SpecialColors.TAB_*_CONFIG`: o tom do suporte da aba de cada tipo de transmissão.
fn transmission_tint(kind: TransmissionType) -> u32 {
    match kind {
        TransmissionType::Energy => 0xFF_59_C1_5F,
        TransmissionType::Fluid => 0xFF_36_6B_D0,
        TransmissionType::Chemical => 0xFF_FF_DD_4F,
        TransmissionType::Item => 0xFF_CF_CF_CF,
        TransmissionType::Heat => 0xFF_FF_A1_60,
    }
}

/// O nome do sprite e da chave de tradução de um tipo (`TransmissionType.transmission`).
fn transmission_name(kind: TransmissionType) -> &'static str {
    match kind {
        TransmissionType::Energy => "energy",
        TransmissionType::Fluid => "fluids",
        TransmissionType::Chemical => "chemicals",
        TransmissionType::Item => "items",
        TransmissionType::Heat => "heat",
    }
}

fn transmission_label(kind: TransmissionType) -> String {
    lang::get(&format!("transmission.mekanism.{}", transmission_name(kind)))
}

fn side_label(side: RelativeSide) -> String {
    lang::get(&format!("side.mekanism.{}", side.serialized_name()))
}

fn data_type_label(data_type: DataType) -> String {
    lang::get(&format!("side_data.mekanism.{}", data_type.serialized_name()))
}

fn on_off(value: bool) -> String {
    lang::get(if value { "boolean.mekanism.on" } else { "boolean.mekanism.off" })
}

/// O bloco vizinho do lado `side` da máquina em `pos`, como item (`SideDataButton.otherBlockItem`: `getCloneItemStack` do estado vizinho).
/// Ar não tem.
fn neighbour_item(game: &Game<'_>, view: &ConfigView, side: RelativeSide) -> Option<ItemStack> {
    let pos = game.mb_open_screen_pos()?;
    let offset = side.direction(view.facing).offset();
    let at = [pos[0] + offset[0], pos[1] + offset[1], pos[2] + offset[2]];
    // `BlockState.getCloneItemStack(pos, level, false, player)`: o item do Pick Block, sem os dados do tile. Ar não tem.
    game.picked_stack(at, false)
}

/// O nome do item para o tooltip (`ItemStack.getHoverName`).
fn item_name(stack: &ItemStack) -> String {
    let (ns, path) = stack.item.split_once(':').unwrap_or(("minecraft", &stack.item));
    let key = format!("block.{ns}.{path}");
    let name = lang::get(&key);
    if name == key { lang::get(&format!("item.{ns}.{path}")) } else { name }
}

/// `GuiElement.drawButton` com o tom do botão (`getButtonBlitColor`).
fn button_tinted(game: &Game<'_>, batch: &mut HudBatch, spec: &UpgradeWindowSpec, rect: [f32; 4], active: bool, hovered: bool, tint: u32) {
    let name = if !active {
        "button_inactive"
    } else if hovered {
        "button_focus"
    } else {
        "button"
    };
    nine(game, batch, spec, name, uw::BUTTON_SLICE, rect, tint);
}

/// A janela de lados ou de transporte, com o que `GuiWindow` e os filhos desenham: a sombra e o painel, o fechar e o fixar, os botões, e
/// por último os textos.
#[allow(clippy::too_many_arguments)]
fn draw_config_window(
    game: &Game<'_>,
    spec: &UpgradeWindowSpec,
    batch: &mut HudBatch,
    state: &Client,
    open: &Open,
    view: Option<&ConfigView>,
    ox: f32,
    oy: f32,
) {
    let (wx, wy) = (ox + open.window.relative_x as f32, oy + open.window.relative_y as f32);
    let (w, h) = (open.window.width as f32, open.window.height as f32);
    let cursor = game.native_cursor();
    let mouse = (cursor[0], cursor[1]);
    // `isMouseOverCheckWindows`: um filho só reage com o mouse se esta é a janela de cima sob ele.
    let under = state.windows.iter().position(|other| {
        let rel = [mouse.0 - ox - other.window.relative_x as f32, mouse.1 - oy - other.window.relative_y as f32];
        rel[0] >= 0.0 && rel[1] >= 0.0 && rel[0] < other.window.width as f32 && rel[1] < other.window.height as f32
    });
    let own = state.windows.iter().position(|other| std::ptr::eq(other, open));
    let hover_ok = under.is_some() && under == own;
    let hov = |rect: [f32; 4]| hover_ok && hovering(rect, mouse);
    let at = |rect: [i32; 4]| shifted(rect, wx, wy);

    nine(game, batch, spec, "shadow", uw::SHADOW_SLICE, [wx - 3.0, wy - 3.0, w + 6.0, h + 6.0], SHADOW_TINT);
    nine(game, batch, spec, "base", spec.base_slice, [wx, wy, w, h], WHITE);
    let close = at(CLOSE);
    button_background(game, batch, spec, close, true, hov(close));
    plain(game, batch, spec, "button/close", [12.0, 12.0], close);
    let pin = at(PIN);
    button_background(game, batch, spec, pin, true, hov(pin));
    plain(game, batch, spec, if open.window.pinned { "button/pinned" } else { "button/unpinned" }, [24.0, 12.0], pin);

    let mut title = String::new();
    let mut texts: Vec<([f32; 4], bool, u32, String)> = Vec::new();
    if let Some(view) = view {
        match open.kind {
            Kind::Side => {
                let transmissions = view.config.transmissions();
                let current = transmissions.get(open.current).copied().unwrap_or(TransmissionType::Item);
                let info = view.config.config(current);
                title = uw::translate(&lang::get("configuration.mekanism.config_type"), &[transmission_label(current)]);
                // A tela interna do ejetor.
                nine(game, batch, spec, "inner_screen", uw::INNER_SCREEN_SLICE, at(cw::SIDE_INNER_SCREEN), WHITE);
                let can_eject = info.is_some_and(|info| info.can_eject());
                let line = if can_eject {
                    uw::translate(&lang::get("gui.mekanism.eject"), &[on_off(info.is_some_and(|info| info.is_ejecting()))])
                } else {
                    lang::get("configuration.mekanism.no_eject")
                };
                let [ix, iy, iw, _] = cw::SIDE_INNER_SCREEN;
                texts.push((
                    [wx + (ix + 5) as f32, wy + (iy + 1) as f32, wx + (ix + iw - 5) as f32, wy + (iy + 11) as f32],
                    false,
                    SCREEN_COLOR,
                    line,
                ));
                // As abas dos tipos (a do tipo atual fica escondida).
                for (index, kind) in transmissions.iter().enumerate() {
                    if index == open.current {
                        continue;
                    }
                    let tab = cw::type_tab(index, open.window.width);
                    let holder = InsetTab { x: tab[0], y: tab[1], height: cw::TAB_SIZE, left: index < 4 };
                    let button = shifted(
                        [holder.button()[0] as i32, holder.button()[1] as i32, cw::TAB_INNER, cw::TAB_INNER],
                        wx,
                        wy,
                    );
                    let icon = sprite(spec, &format!("transmission/{}", transmission_name(*kind)));
                    basis::inset_tab(
                        game,
                        batch,
                        spec.ns,
                        holder,
                        [wx, wy],
                        transmission_tint(*kind),
                        &icon,
                        machinery_basis_core::side_tab::ButtonLook::of(true, hovering(button, mouse) && (under.is_none() || under == own)),
                    );
                }
                // O botão de ejetar e o de limpar.
                let eject = at(cw::EJECT_BUTTON);
                button_background(game, batch, spec, eject, can_eject, can_eject && hov(eject));
                plain(game, batch, spec, "button/auto_eject", [14.0, 14.0], eject);
                let clear = at(cw::CLEAR_BUTTON);
                button_background(game, batch, spec, clear, true, hov(clear));
                plain(game, batch, spec, "button/clear_sides", [14.0, 14.0], clear);
                // Os botões de lado.
                for side in RelativeSide::VALUES {
                    let [x, y] = cw::side_button(side);
                    let data_type = info.map(|info| info.data_type(side));
                    let enabled = info.is_some_and(|info| info.is_side_enabled(side));
                    let tint = data_type.map_or(WHITE, |kind| cw::button_tint(cw::ENUM_COLOR_RGB[usize::from(kind.color())]));
                    side_button(game, batch, spec, open, side, at([x, y, cw::SIDE_BUTTON_SIZE, cw::SIDE_BUTTON_SIZE]), enabled, hov, tint);
                }
                texts.push(([wx + 4.0, wy + 120.0, wx + w - 4.0, wy + 129.0], true, SUBHEADING_COLOR, lang::get("configuration.mekanism.slots")));
            }
            Kind::Transporter => {
                title = lang::get("configuration.mekanism.transporter");
                nine(game, batch, spec, "inner_screen", uw::INNER_SCREEN_SLICE, at(cw::TRANSPORTER_INNER_SCREEN), WHITE);
                let [ix, iy, iw, _] = cw::TRANSPORTER_INNER_SCREEN;
                texts.push((
                    [wx + (ix + 5) as f32, wy + (iy + 1) as f32, wx + (ix + iw - 5) as f32, wy + (iy + 11) as f32],
                    false,
                    SCREEN_COLOR,
                    uw::translate(&lang::get("configuration.mekanism.strict_input.enabled"), &[on_off(view.ejector.has_strict_input())]),
                ));
                // O slot da cor de saída, e o botão de cor dentro dele.
                plain(game, batch, spec, "slot/normal", [18.0, 18.0], at(cw::OUTPUT_SLOT));
                if let Some(color) = view.ejector.output_color() {
                    game.native_fill(batch, at(cw::OUTPUT_COLOR_BUTTON), cw::color_argb(color));
                }
                let strict = at(cw::STRICT_BUTTON);
                button_background(game, batch, spec, strict, true, hov(strict));
                plain(game, batch, spec, "button/exclamation", [16.0, 16.0], strict);
                for side in RelativeSide::VALUES {
                    let [x, y] = cw::transporter_button(side);
                    let enabled = TileComponentEjector::is_input_side_enabled(&view.config, side);
                    let tint = view
                        .ejector
                        .input_color(side)
                        .map_or(WHITE, |color| cw::button_tint(cw::ENUM_COLOR_RGB[usize::from(color)]));
                    side_button(game, batch, spec, open, side, at([x, y, cw::SIDE_BUTTON_SIZE, cw::SIDE_BUTTON_SIZE]), enabled, hov, tint);
                }
                texts.push(([wx + 18.0, wy + 105.0, wx + 18.0 + 68.0, wy + 114.0], true, SUBHEADING_COLOR, lang::get("boolean.mekanism.input")));
                texts.push((
                    [wx + 90.0, wy + 68.0, wx + 86.0 + 70.0 - 4.0, wy + 77.0],
                    true,
                    SUBHEADING_COLOR,
                    lang::get("boolean.mekanism.output"),
                ));
            }
            Kind::Upgrade | Kind::RecipeBook => {}
        }
    }
    // O primeiro plano: o título e os rótulos.
    scrolling_text(game, batch, &title, [wx + 34.0, wy + 5.0, wx + w - 18.0 - 4.0, wy + 5.0 + LINE_HEIGHT], true, TITLE_COLOR, 1.0, open.opened_ms);
    for (area, center, color, text) in texts {
        let scale = if color == SCREEN_COLOR { 0.8 } else { 1.0 };
        scrolling_text(game, batch, &text, area, center, color, scale, open.opened_ms);
    }
}

/// `SideDataButton`: o botão de 22 por 22 com o tom da cor e, por cima, o item do bloco vizinho.
#[allow(clippy::too_many_arguments)]
fn side_button(
    game: &Game<'_>,
    batch: &mut HudBatch,
    spec: &UpgradeWindowSpec,
    open: &Open,
    side: RelativeSide,
    rect: [f32; 4],
    active: bool,
    hov: impl Fn([f32; 4]) -> bool,
    tint: u32,
) {
    button_tinted(game, batch, spec, rect, active, active && hov(rect), tint);
    if let Some(stack) = &open.neighbours[side.ordinal()] {
        game.native_item_stack(batch, stack, [rect[0] + 3.0, rect[1] + 3.0], 16.0);
    }
}

/// O tooltip do botão sob o mouse `rel` (relativo à janela) de uma janela de lados ou de transporte: as linhas, o retângulo do botão
/// relativo à janela, e se o texto é de `Tooltip.create` (sem título).
#[allow(clippy::type_complexity)]
fn config_tooltip(
    game: &Game<'_>,
    open: &Open,
    view: Option<&ConfigView>,
    rel: [f64; 2],
) -> Option<(Vec<(String, Option<u32>)>, [i32; 4], bool)> {
    let one = |text: String, rect: [i32; 4]| Some((vec![(text, None)], rect, true));
    if in_rect(CLOSE, rel) {
        return one(lang::get("gui.mekanism.close"), CLOSE);
    }
    if in_rect(PIN, rel) {
        return one(lang::get(if open.window.pinned { "gui.mekanism.unpin" } else { "gui.mekanism.pin" }), PIN);
    }
    let view = view?;
    let wrap = |text: &str| -> Vec<(String, Option<u32>)> {
        uw::wrap_lines(text, TOOLTIP_WRAP, &|piece: &str| game.native_text_width(piece)).into_iter().map(|line| (line, None)).collect()
    };
    let side_lines = |side: RelativeSide, second: String, color: Option<u32>| {
        let mut lines = vec![(side_label(side), None), (second, color)];
        if let Some(stack) = &open.neighbours[side.ordinal()] {
            lines.push((item_name(stack), None));
        }
        lines
    };
    match open.kind {
        Kind::Side => {
            let transmissions = view.config.transmissions();
            let current = transmissions.get(open.current).copied()?;
            let info = view.config.config(current)?;
            for side in RelativeSide::VALUES {
                let [x, y] = cw::side_button(side);
                let rect = [x, y, cw::SIDE_BUTTON_SIZE, cw::SIDE_BUTTON_SIZE];
                if in_rect(rect, rel) && info.is_side_enabled(side) {
                    let kind = info.data_type(side);
                    let rgb = cw::ENUM_COLOR_RGB[usize::from(kind.color())];
                    return Some((side_lines(side, data_type_label(kind), Some(rgb)), rect, true));
                }
            }
            if in_rect(cw::CLEAR_BUTTON, rel) {
                // `TooltipToggleButton`: com o alvo `NONE` (ou shift) o texto de limpar, senão o de avançar.
                let clearing = cw::target_type(Some(info), game.has_shift_down(), true) == DataType::None;
                let mut lines = Vec::new();
                if clearing {
                    lines.extend(wrap(&lang::get("configuration.mekanism.side.clear")));
                    lines.extend(wrap(&lang::get("configuration.mekanism.side.clear.all")));
                } else {
                    lines.extend(wrap(&lang::get("configuration.mekanism.side.increment")));
                }
                return Some((lines, cw::CLEAR_BUTTON, true));
            }
            if in_rect(cw::EJECT_BUTTON, rel) && info.can_eject() {
                return one(lang::get("auto.mekanism.eject"), cw::EJECT_BUTTON);
            }
            if in_rect(cw::SIDE_INNER_SCREEN, rel) && !info.can_eject() {
                return Some((wrap(&lang::get("configuration.mekanism.no_eject.tooltip")), cw::SIDE_INNER_SCREEN, true));
            }
            for (index, kind) in transmissions.iter().enumerate() {
                let rect = inset_rect(index, open);
                if index != open.current && in_rect(rect, rel) {
                    return one(transmission_label(*kind), rect);
                }
            }
            None
        }
        Kind::Transporter => {
            for side in RelativeSide::VALUES {
                let [x, y] = cw::transporter_button(side);
                let rect = [x, y, cw::SIDE_BUTTON_SIZE, cw::SIDE_BUTTON_SIZE];
                if in_rect(rect, rel) && TileComponentEjector::is_input_side_enabled(&view.config, side) {
                    let (name, rgb) = match view.ejector.input_color(side) {
                        Some(color) => (lang::get(&cw::color_key(color)), Some(cw::ENUM_COLOR_RGB[usize::from(color)])),
                        None => (lang::get("gui.mekanism.none"), None),
                    };
                    return Some((side_lines(side, name, rgb), rect, true));
                }
            }
            if in_rect(cw::OUTPUT_COLOR_BUTTON, rel) {
                return Some((
                    vec![match view.ejector.output_color() {
                        Some(color) => (lang::get(&cw::color_key(color)), Some(cw::ENUM_COLOR_RGB[usize::from(color)])),
                        None => (lang::get("gui.mekanism.none"), None),
                    }],
                    cw::OUTPUT_COLOR_BUTTON,
                    true,
                ));
            }
            if in_rect(cw::STRICT_BUTTON, rel) {
                return one(lang::get("configuration.mekanism.strict_input"), cw::STRICT_BUTTON);
            }
            None
        }
        Kind::Upgrade | Kind::RecipeBook => None,
    }
}

/// As abas de cada quadro, na ordem de `GuiMekanism.init`: a de upgrades na lateral direita (fundo do botão, a aba tingida por cima dele, o
/// fundo outra vez e o ícone), e as duas de configuração na esquerda. `origin` é o canto da tela.
pub(in crate::game) fn draw_tab(game: &Game<'_>, batch: &mut HudBatch, block: &str, origin: [f32; 2]) {
    let Some(spec) = spec_of(block) else {
        return;
    };
    let state = client();
    let cursor = game.native_cursor();
    let at = [cursor[0] - origin[0], cursor[1] - origin[1]];
    if spec.upgrades {
        let active = index_of(&state, Kind::Upgrade).is_none();
        let button = shifted(TAB_BUTTON, origin[0], origin[1]);
        let hovered = active && !covered(&state, at) && hovering(shifted(TAB_BUTTON, 0.0, 0.0), (at[0], at[1]));
        button_background(game, batch, spec, button, active, hovered);
        nine(game, batch, spec, "holder_right", uw::HOLDER_SLICE, shifted(TAB, origin[0], origin[1]), WHITE);
        button_background(game, batch, spec, button, active, hovered);
        plain(game, batch, spec, "button/upgrade", [18.0, 18.0], button);
    }
    if !has_config(spec) {
        return;
    }
    for kind in [Kind::Side, Kind::Transporter] {
        let Some(tab) = left_tab(kind) else {
            continue;
        };
        let active = index_of(&state, kind).is_none();
        let hovered = active && !covered(&state, at) && hovering(tab.button(), (at[0], at[1]));
        let icon = sprite(spec, if kind == Kind::Side { "button/configuration" } else { "button/transporter_config" });
        basis::inset_tab(game, batch, spec.ns, tab, origin, WHITE, &icon, machinery_basis_core::side_tab::ButtonLook::of(active, hovered));
    }
}

/// `GuiElement.isMouseOverCheckWindows`: o ponto `at` (relativo ao canto da tela) está sobre alguma janela aberta, e então o elemento de
/// baixo não o recebe.
fn covered(state: &Client, at: [f32; 2]) -> bool {
    state.windows.iter().any(|open| {
        let rel = [at[0] - open.window.relative_x as f32, at[1] - open.window.relative_y as f32];
        rel[0] >= 0.0 && rel[1] >= 0.0 && rel[0] < open.window.width as f32 && rel[1] < open.window.height as f32
    })
}

pub(in crate::game) fn covers(at: [f32; 2]) -> bool {
    covered(&client(), at)
}

/// Os tooltips das abas e das janelas, no mesmo estilo do `WidgetTooltipHolder`. `at` é o mouse relativo ao canto da tela.
pub(in crate::game) fn tooltip(
    game: &Game<'_>,
    batch: &mut HudBatch,
    block: &str,
    at: [f32; 2],
    cursor: (f32, f32),
) -> bool {
    let Some(spec) = spec_of(block) else {
        return false;
    };
    let config = config_view(game, spec);
    let state = client();
    let origin = [cursor.0 - at[0], cursor.1 - at[1]];
    let mut lines: Vec<(String, Option<u32>)> = Vec::new();
    let mut flat = false;
    let rect: [f32; 4];
    // `GuiMekanism.getWindowHovering`: a primeira janela, do topo para baixo, que está sob o mouse.
    let hovered_window = state.windows.iter().find(|open| {
        let rel = [at[0] - open.window.relative_x as f32, at[1] - open.window.relative_y as f32];
        rel[0] >= 0.0 && rel[1] >= 0.0 && rel[0] < open.window.width as f32 && rel[1] < open.window.height as f32
    });
    match hovered_window {
        None => {
            if spec.upgrades && index_of(&state, Kind::Upgrade).is_none() && hovering(shifted(TAB_BUTTON, 0.0, 0.0), (at[0], at[1])) {
                lines.push((lang::get("gui.mekanism.upgrades"), None));
                rect = shifted(TAB_BUTTON, origin[0], origin[1]);
            } else if let Some((kind, tab)) = [Kind::Side, Kind::Transporter]
                .into_iter()
                .filter(|kind| has_config(spec) && index_of(&state, *kind).is_none())
                .filter_map(|kind| left_tab(kind).map(|tab| (kind, tab)))
                .find(|(_, tab)| hovering(tab.button(), (at[0], at[1])))
            {
                let key = if kind == Kind::Side { "configuration.mekanism.side" } else { "configuration.mekanism.transporter" };
                lines.push((lang::get(key), None));
                let [bx, by, bw, bh] = tab.button();
                rect = [origin[0] + bx, origin[1] + by, bw, bh];
            } else if let Some(view) = config.as_ref()
                && let Some((index, tab)) = type_tab_at(&state, view, [f64::from(at[0]), f64::from(at[1])])
            {
                let open = &state.windows[index];
                let kind = view.config.transmissions()[tab];
                lines.push((transmission_label(kind), None));
                let r = inset_rect(tab, open);
                rect = [origin[0] + (open.window.relative_x + r[0]) as f32, origin[1] + (open.window.relative_y + r[1]) as f32, r[2] as f32, r[3] as f32];
                flat = true;
            } else {
                return false;
            }
        }
        Some(open) if open.kind.is_config() => {
            let rel = [f64::from(at[0] - open.window.relative_x as f32), f64::from(at[1] - open.window.relative_y as f32)];
            let Some((text, area, is_flat)) = config_tooltip(game, open, config.as_ref(), rel) else {
                return false;
            };
            lines = text;
            flat = is_flat;
            let [wx, wy] = [open.window.relative_x as f32, open.window.relative_y as f32];
            rect = [origin[0] + wx + area[0] as f32, origin[1] + wy + area[1] as f32, area[2] as f32, area[3] as f32];
        }
        Some(open) if open.kind == Kind::RecipeBook => {
            let [wx, wy] = [open.window.relative_x as f32, open.window.relative_y as f32];
            let rel = [f64::from(at[0] - wx), f64::from(at[1] - wy)];
            let win = |r: [i32; 4]| shifted(r, origin[0] + wx, origin[1] + wy);
            if in_rect(CLOSE, rel) {
                lines.push((lang::get("gui.mekanism.close"), None));
                rect = win(CLOSE);
            } else if in_rect(PIN, rel) {
                let key = if open.window.pinned { "gui.mekanism.unpin" } else { "gui.mekanism.pin" };
                lines.push((lang::get(key), None));
                rect = win(PIN);
            } else {
                return false;
            }
        }
        Some(open) => {
            let [wx, wy] = [open.window.relative_x as f32, open.window.relative_y as f32];
            let rel = [at[0] - wx, at[1] - wy];
            let rel_i = [f64::from(rel[0]), f64::from(rel[1])];
            let win = |r: [i32; 4]| shifted(r, origin[0] + wx, origin[1] + wy);
            let order = spec_types(spec);
            let sync = read_sync(game);
            let installed = sync.installed(&order);
            if in_rect(CLOSE, rel_i) {
                lines.push((lang::get("gui.mekanism.close"), None));
                rect = win(CLOSE);
            } else if in_rect(PIN, rel_i) {
                let key = if open.window.pinned { "gui.mekanism.unpin" } else { "gui.mekanism.pin" };
                lines.push((lang::get(key), None));
                rect = win(PIN);
            } else if open.selected.is_some() && in_rect(UNINSTALL, rel_i) {
                // `Tooltip.create` quebra o texto em 170 pixels (`Tooltip.MAX_WIDTH`).
                let text = lang::get("gui.mekanism.upgrade.uninstall.tooltip");
                for line in uw::wrap_lines(&text, TOOLTIP_WRAP, &|piece: &str| game.native_text_width(piece)) {
                    lines.push((line, None));
                }
                flat = true;
                rect = win(UNINSTALL);
            } else if let Some(row) = open.list.hovered_row(rel_i)
                && let Some(index) = installed.get(open.list.first_visible(installed.len()) + row as usize)
                && let Some(id) = order.get(*index)
            {
                // `GuiUpgradeScrollList.updateTooltip`: `TooltipUtils.create(upgrade.description())`, que é `Tooltip.create` e quebra em 170 pixels.
                for line in uw::wrap_lines(&type_description(id), TOOLTIP_WRAP, &|piece: &str| game.native_text_width(piece)) {
                    lines.push((line, None));
                }
                // As linhas de um `Tooltip.create` têm o mesmo espaçamento (sem a folga do título de um tooltip de item): 10 pixels por linha, 26 de caixa com duas.
                flat = true;
                rect = win([LIST[0] + 1, LIST[1] + 1 + 12 * row, 100, 12]);
            } else {
                let layout = SupportedLayout::new(game.native_text_width(&lang::get("gui.mekanism.upgrades.supported")) as i32);
                let local = [rel[0] as i32 - SUPPORTED[0] - 1, rel[1] as i32 - SUPPORTED[1] - 1];
                let Some(index) = layout.index_at(local, order.len()) else {
                    return false;
                };
                let id = &order[index];
                let name = type_label(id);
                if (spec.data)().supports(id) {
                    lines.push((name, Some(YELLOW)));
                } else {
                    lines.push((uw::translate(&lang::get("gui.mekanism.upgrade.not_supported"), &[name]), Some(RED)));
                }
                lines.push((type_description(id), None));
                let [px, py] = layout.position(index);
                rect = win([SUPPORTED[0] + 1 + px, SUPPORTED[1] + 1 + py, 12, 12]);
            }
        }
    }
    if flat {
        basis::menu_tooltip_flat(game, batch, &lines, cursor, rect);
    } else {
        basis::menu_tooltip(game, batch, &lines, cursor, rect);
    }
    true
}

/// `ItemUpgrade.useOn`: com shift, o upgrade na mão entra no bloco que o aceita (`IUpgradeTile.supportsUpgrade`), e o que entrou sai da
/// pilha (`stack.consume(added, player)`, que o criativo não gasta). O clique é do upgrade, tenha entrado algum ou não
/// (`InteractionResult.SUCCESS_SERVER`); em bloco que não aceita o tipo segue o caminho normal (`PASS`).
pub(in crate::game) fn use_item_on(game: &mut Game<'_>, stack: &ItemStack, pos: [i32; 3], claimed: &mut bool) {
    if *claimed || !is_upgrade_item(&stack.item) || !game.native_sneaking() {
        return;
    }
    let block = game.native_block(pos);
    let Some((spec, add)) = adder_of(&block, &stack.item) else {
        return;
    };
    let upgrade = match game.native_component(stack, spec.item_component) {
        Ok(Some(Tag::String(upgrade))) => upgrade.to_string_lossy(),
        _ => return,
    };
    if !(spec.data)().supports(&upgrade) {
        return;
    }
    *claimed = true;
    let available = i32::try_from(stack.count).unwrap_or(i32::MAX);
    let added = add(game, pos, &upgrade, available);
    if added > 0 {
        let creative = game.native_acting_player().is_some_and(|player| player.game_mode == CREATIVE);
        if !creative {
            let held = game.held_mut();
            let emptied = held.as_mut().is_some_and(|in_hand| {
                in_hand.count = in_hand.count.saturating_sub(added.unsigned_abs());
                in_hand.count == 0
            });
            if emptied {
                *held = None;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// Servidor.

type ContainerSessions = Sessions<[i32; 3], u128, u8>;
static SESSIONS: Mutex<Option<ContainerSessions>> = Mutex::new(None);

fn sessions() -> std::sync::MutexGuard<'static, Option<ContainerSessions>> {
    SESSIONS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Onde o jogador `player` tem um contêiner aberto: o anfitrião (0) pelo `container` do jogo, um convidado pelo menu dele. O espectador
/// também (`MekanismTileContainer.openInventory` não olha o modo de jogo).
fn open_container(game: &Game<'_>, player: u128) -> Option<[i32; 3]> {
    if player == 0 {
        return game.container.map(|key| [key.0, key.1, key.2]);
    }
    let host = game.lan.as_ref()?;
    let guest = host.hub.guests.iter().find(|guest| guest.uuid == player)?;
    match guest.menu.as_ref()?.slots {
        horizon_client::lan::menu::Slots::Block(pos) => Some([pos.0, pos.1, pos.2]),
        _ => None,
    }
}

/// A janela escolhida por quem clica, no contêiner em `pos` (`getSelectedWindow(player)`), para o shift-clique.
pub(in crate::game) fn selected_for(pos: [i32; 3], player: u128) -> Option<SelectedWindow> {
    sessions().as_ref().and_then(|table| table.selected(pos, player))
}

/// `playersUsing.size()` do tile em `pos`: os jogadores com o menu dele aberto, o espectador incluso (`MekanismTileContainer.openInventory`
/// chama `tile.open(player)` sem olhar o modo de jogo).
pub(in crate::game) fn players_using(pos: [i32; 3]) -> usize {
    sessions().as_ref().map_or(0, |table| table.players(pos))
}

/// Quem chama de fora age: o jogador do turno (`lan_acting_uuid`) ou o anfitrião (0).
pub(in crate::game) fn acting(game: &Game<'_>) -> u128 {
    game.lan_acting_uuid.unwrap_or(0)
}

/// `native_net_from_client`: a seleção da janela, o pedido de acompanhar os upgrades, o de parar, e a remoção.
pub(in crate::game) fn net_from_client(game: &mut Game<'_>, player: u128, channel: &str, data: &[u8]) {
    if channel == CHANNEL {
        server_message(game, player, data);
    } else if channel == BOOK_CHANNEL {
        book_server_message(game, player, data);
    }
}

/// `PacketUpdateContainer`: os dados `properties` do menu do bloco em `pos` vão ao jogador. Um pacote que abre o menu leva o id do bloco.
fn deliver(game: &mut Game<'_>, player: u128, pos: [i32; 3], opens: Option<&str>, properties: &[Property]) {
    if properties.is_empty() {
        return;
    }
    let packet = wire::Packet::new(pos, opens.map(str::to_owned), properties);
    if let Err(error) = game.native_net_send_to_player(player, CONTAINER_CHANNEL, &wire::encode(&packet)) {
        eprintln!("machinery_basis: os dados do menu não chegaram a {player:032x}: {error}");
    }
}

/// O menu do jogador no bloco `block` em `pos` nasce (`MekanismContainer.<init>` com `openInventory`): o jogador entra no `playersUsing` do
/// tile e recebe todos os dados (`sendAllDataToRemote`). Um tile sem objeto vivo ainda (acabou de nascer) não tem o que ler: os dados vão no
/// primeiro `broadcast` depois que ele tiver.
pub(in crate::game) fn container_open(game: &mut Game<'_>, player: u128, pos: [i32; 3], block: &str) {
    let Some(spec) = spec_of(block) else {
        return;
    };
    let level = game.native_dimension();
    sessions()
        .get_or_insert_with(Default::default)
        .open(pos, player, level, block, &(spec.trackers)());
    let Some(owner) = owner_of(block) else {
        return;
    };
    let mut sent = Vec::new();
    (owner.source)(game, pos, &mut |source| {
        sent = sessions()
            .as_mut()
            .and_then(|table| table.send_initial(pos, player, source))
            .unwrap_or_default();
    });
    deliver(game, player, pos, Some(block), &sent);
}

/// `MekanismContainer.broadcastChanges` de cada menu aberto: só os dados sujos de cada jogador, num pacote por jogador. Roda no começo do
/// tick do servidor (`ServerTickStart`), com o estado do fim do tick anterior, como o `ServerPlayer.tick` do original (que o lê antes dos
/// block entities), e a cada passada com o servidor parado, porque o jogador continua andando nele.
pub(in crate::game) fn broadcast(game: &mut Game<'_>) {
    book_server_sync(game);
    adopt_open_menus(game);
    send_config_snapshots(game);
    let containers = sessions().as_ref().map(ContainerSessions::containers).unwrap_or_default();
    if containers.is_empty() {
        return;
    }
    game.mb_in_levels(
        |dimension| containers.iter().any(|(_, level)| level == dimension),
        |game, dimension| {
            for (pos, _) in containers.iter().filter(|(_, level)| level == dimension) {
                broadcast_container(game, *pos);
            }
        },
    );
}

/// Um jogador com o menu de um bloco nosso aberto e sem sessão (o menu abriu por um caminho que não passou por `container_opened`) ganha a sessão
/// aqui, com todos os dados: o `MekanismContainer` do Java nasce com o menu, não importa quem o abriu.
fn adopt_open_menus(game: &mut Game<'_>) {
    let Some(host) = game.lan.as_ref() else {
        return;
    };
    let open: Vec<(u128, [i32; 3])> = host
        .hub
        .guests
        .iter()
        .filter(|guest| guest.placed)
        .filter_map(|guest| match guest.menu.as_ref()?.slots {
            horizon_client::lan::menu::Slots::Block(pos) => Some((guest.uuid, [pos.0, pos.1, pos.2])),
            _ => None,
        })
        .collect();
    for (player, pos) in open {
        if sessions().as_ref().is_some_and(|table| table.is_open(pos, player)) {
            continue;
        }
        let block = match game.world.block_entities.get(&(pos[0], pos[1], pos[2])) {
            Some(BlockEntity::Mod(entity)) => entity.id.clone(),
            _ => continue,
        };
        container_open(game, player, pos, &block);
    }
}

fn broadcast_container(game: &mut Game<'_>, pos: [i32; 3]) {
    let block = match game.world.block_entities.get(&(pos[0], pos[1], pos[2])) {
        Some(BlockEntity::Mod(entity)) => entity.id.clone(),
        _ => return,
    };
    let Some(owner) = owner_of(&block) else {
        return;
    };
    let mut packets = Vec::new();
    (owner.source)(game, pos, &mut |source| {
        packets = sessions().as_mut().map(|table| table.broadcast(pos, &block, source)).unwrap_or_default();
    });
    for packet in packets {
        deliver(game, packet.player, pos, packet.opens.then_some(block.as_str()), &packet.properties);
    }
}

/// `MekanismContainer.removed`: quem fechou o menu sai do `playersUsing` e perde a sessão. O Java chama `close(player)` na hora; o jogo só
/// diz que o menu mudou, então a conta é refeita no começo de cada passada do servidor (`server_tick`), com o menu que cada jogador tem agora.
pub(in crate::game) fn recount(game: &Game<'_>) {
    let mut guard = sessions();
    let Some(table) = guard.as_mut() else {
        return;
    };
    for (pos, _) in table.containers() {
        table.retain_players(pos, |player| open_container(game, player) == Some(pos));
    }
}

fn server_message(game: &mut Game<'_>, player: u128, data: &[u8]) {
    let Some((&op, body)) = data.split_first() else {
        return;
    };
    let Some(pos) = open_container(game, player) else {
        return;
    };
    let block = match game.world.block_entities.get(&(pos[0], pos[1], pos[2])) {
        Some(BlockEntity::Mod(m)) => m.id.clone(),
        _ => return,
    };
    let Some(spec) = spec_of(&block) else {
        return;
    };
    // Uma mensagem de quem ainda não tem sessão (a abertura não passou por `container_open`): cria a sessão, e os dados esperam o `broadcast`.
    let level = game.native_dimension();
    {
        let mut guard = sessions();
        let table = guard.get_or_insert_with(Default::default);
        if !table.is_open(pos, player) {
            table.open(pos, player, level, &block, &(spec.trackers)());
        }
    }
    match op {
        OP_SELECT => {
            let Some(selected) = window::decode_selection(body, spec.window_types) else {
                return;
            };
            sessions().get_or_insert_with(Default::default).select(pos, player, selected);
            // O menu do convidado esconde os slots virtuais sem janela, e recomeça do bloco no clique seguinte.
            let shown = selected == Some(Kind::Upgrade.selection(spec));
            if let Some(host) = game.lan.as_mut()
                && let Some(conn) = host.hub.guests.iter().find(|g| g.uuid == player).map(|g| g.conn)
            {
                let hidden = if shown || !spec.upgrades { Vec::new() } else { spec.virtual_slots.to_vec() };
                host.hub.hide_menu_slots(conn, hidden);
                if let Some(play) = host.play.get_mut(&conn) {
                    play.crafting.machine = None;
                }
            }
        }
        OP_TRACK => {
            // `startTrackingServer`: os dados da janela entram no fim e vão logo ao jogador.
            let window = (spec.window_trackers)();
            let mut sent = Vec::new();
            let read = owner_of(&block).is_some_and(|owner| {
                (owner.source)(game, pos, &mut |source| {
                    sent = sessions()
                        .as_mut()
                        .map(|table| table.start_tracking_server(pos, player, UPGRADE_WINDOW_KEY, &window, source))
                        .unwrap_or_default();
                })
            });
            if !read {
                // Sem tile vivo para ler, os dados entram sem o envio inicial: o primeiro `broadcast` os manda se mudarem do padrão.
                if let Some(table) = sessions().as_mut() {
                    table.start_tracking(pos, player, UPGRADE_WINDOW_KEY, &window);
                }
            }
            deliver(game, player, pos, None, &sent);
        }
        OP_STOP => {
            if let Some(table) = sessions().as_mut() {
                table.stop_tracking(pos, player, &UPGRADE_WINDOW_KEY);
            }
        }
        OP_CONFIG_TRACK => {
            let mut table = config_tracking();
            if body.first() == Some(&1) {
                table.insert((player, pos), Vec::new());
            } else {
                table.remove(&(player, pos));
            }
        }
        OP_CONFIG => {
            // Os pacotes de configuração de lados e de transporte (`PacketSideData` e companhia).
            if let (Some(packet), Some(owner)) = (ConfigPacket::decode(body), config_owner(&block)) {
                (owner.apply)(game, pos, packet);
            }
        }
        OP_REMOVE => {
            let Some((index, all)) = uw::decode_remove(body) else {
                return;
            };
            let order = spec_types(spec);
            if let Some(upgrade) = uw::type_at(&order, index).map(str::to_owned)
                && let Some(remove) = remover_of(&block)
            {
                remove(game, pos, &upgrade, all);
            }
        }
        _ => {}
    }
}

/// Quem tem uma janela de configuração aberta (jogador e bloco) e o último estado que o servidor mandou a ele.
static CONFIG_TRACKING: Mutex<Option<HashMap<(u128, [i32; 3]), Vec<u8>>>> = Mutex::new(None);

struct ConfigTracking(std::sync::MutexGuard<'static, Option<HashMap<(u128, [i32; 3]), Vec<u8>>>>);

impl std::ops::Deref for ConfigTracking {
    type Target = HashMap<(u128, [i32; 3]), Vec<u8>>;
    fn deref(&self) -> &Self::Target {
        self.0.as_ref().expect("a tabela nasce em config_tracking")
    }
}

impl std::ops::DerefMut for ConfigTracking {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.as_mut().expect("a tabela nasce em config_tracking")
    }
}

fn config_tracking() -> ConfigTracking {
    let mut guard = CONFIG_TRACKING.lock().unwrap_or_else(|e| e.into_inner());
    guard.get_or_insert_with(HashMap::new);
    ConfigTracking(guard)
}

/// A cada passada do servidor, a quem tem a janela aberta vai o estado da configuração e do ejetor, quando mudou (o `broadcastChanges`
/// dos dados do menu e o pacote de atualização do tile, no original).
fn send_config_snapshots(game: &mut Game<'_>) {
    let tracked: Vec<(u128, [i32; 3])> = config_tracking().keys().copied().collect();
    for (player, pos) in tracked {
        if open_container(game, player) != Some(pos) {
            config_tracking().remove(&(player, pos));
            continue;
        }
        let block = match game.world.block_entities.get(&(pos[0], pos[1], pos[2])) {
            Some(BlockEntity::Mod(entity)) => entity.id.clone(),
            _ => continue,
        };
        let Some(owner) = config_owner(&block) else {
            continue;
        };
        let Some(bytes) = (owner.snapshot)(game, pos) else {
            continue;
        };
        let known = config_tracking().get(&(player, pos)).cloned();
        if known.as_ref() == Some(&bytes) {
            continue;
        }
        let mut body = Vec::with_capacity(12 + bytes.len());
        for axis in pos {
            body.extend(axis.to_be_bytes());
        }
        body.extend(&bytes);
        if let Err(error) = game.native_net_send_to_player(player, CONFIG_CHANNEL, &body) {
            eprintln!("machinery_basis: a configuração não chegou a {player:032x}: {error}");
        }
        config_tracking().insert((player, pos), bytes);
    }
}

/// Manda ao jogador os tipos e as tags de upgrade do servidor, e os recursos (os químicos). Só quem não carregou os dados precisa: o
/// anfitrião os tem.
fn send_types(game: &mut Game<'_>, player: u128) {
    let bytes = upgrade::encode_sync(&upgrade::current_types(), &upgrade::current_tags());
    if let Err(error) = game.native_net_send_to_player(player, TYPES_CHANNEL, &bytes) {
        eprintln!("machinery_basis: tipos de upgrade não chegaram a {player:032x}: {error}");
    }
    let bytes = resource::encode_sync(&resource::current());
    if let Err(error) = game.native_net_send_to_player(player, RESOURCES_CHANNEL, &bytes) {
        eprintln!("machinery_basis: recursos não chegaram a {player:032x}: {error}");
    }
}

/// `native_player_logged_in`: o registro vai para quem entrou.
pub(in crate::game) fn player_logged_in(game: &mut Game<'_>, player: u128) {
    send_types(game, player);
    // `PlayerList.placeNewPlayer`: `sendInitialRecipeBook`, no próximo `book_server_sync`.
    book_sent(|sent| sent.remove(&player));
    game.mb_muffling_player_in(player);
    game.mb_view_player_in(player);
}

/// `native_server_data_loaded`: os dados mudaram (um `/reload`), então todos os que estão no servidor recebem de novo.
pub(in crate::game) fn data_reloaded(game: &mut Game<'_>) {
    // Um mundo que abre e um `/reload` (`PlayerList.reloadResources`): todos recebem o livro inteiro de novo.
    book_sent(HashMap::clear);
    let players: Vec<u128> = game.native_server_players().iter().map(|p| p.uuid).collect();
    for player in players {
        send_types(game, player);
    }
}

/// `native_net_from_server`: o cliente guarda o que o servidor mandou.
pub(in crate::game) fn net_from_server(channel: &str, data: &[u8]) {
    if crate::game::machinery_tile_sound::muffling_from_server(channel, data)
        || crate::game::machinery_tile_sound::name_from_server(channel, data)
        || crate::game::machinery_tile_sound::view_from_server(channel, data)
    {
        return;
    }
    match channel {
        BOOK_CHANNEL => book_from_server(data),
        CONTAINER_CHANNEL => container_from_server(data),
        CONFIG_CHANNEL => {
            if data.len() > 12 {
                let axis = |n: usize| i32::from_be_bytes([data[n * 4], data[n * 4 + 1], data[n * 4 + 2], data[n * 4 + 3]]);
                let pos = [axis(0), axis(1), axis(2)];
                config_mirror(|table| table.insert(pos, data[12..].to_vec()));
            }
        }
        RESOURCES_CHANNEL => match resource::decode_sync(data) {
            Some(registry) => resource::set_source(SYNC_SOURCE, registry),
            None => eprintln!("machinery_basis: o servidor mandou recursos ilegíveis"),
        },
        TYPES_CHANNEL => match upgrade::decode_sync(data) {
            Some((types, tags)) => {
                upgrade::set_types_source(SYNC_SOURCE, types);
                upgrade::set_tags_source(SYNC_SOURCE, tags);
            }
            None => eprintln!("machinery_basis: o servidor mandou tipos de upgrade ilegíveis"),
        },
        _ => {}
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// A porta de entrada: métodos de `Game`, que os arquivos dos hooks e o glue do mod chamam sem precisar do caminho deste módulo.

impl Game<'_> {
    /// O mod registra a janela de um bloco.
    pub(in crate::game) fn mb_window_register(spec: &'static UpgradeWindowSpec, remove: RemoveFn, add: AddFn, source: SourceFn) {
        register(spec, remove, add, source);
    }

    /// O mod diz que o bloco tem as abas de configuração de lados e de transporte.
    pub(in crate::game) fn mb_config_register(block: &'static str, view: ViewFn, apply: ApplyFn, snapshot: SnapshotFn) {
        register_config(block, view, apply, snapshot);
    }

    /// `ItemUpgrade.useOn`.
    pub(in crate::game) fn mb_window_use_item_on(&mut self, stack: &ItemStack, pos: [i32; 3], claimed: &mut bool) {
        use_item_on(self, stack, pos, claimed);
    }

    pub(in crate::game) fn mb_window_frame(&mut self) {
        // A forma de colisão por instância dos transmissores (`machinery_transmitter_collision`) é lida pela varredura de colisão em cada thread: o jogador anda na thread do cliente
        // (`main`), que nunca passa por `Game::enter_level` nem por `publish_arms` e ficava sem nível, com a forma do estado (medido em jogo, 2026-11-14: o jogador sobre o braço `PULL` ficava
        // 0,03 acima do original e oscilava, porque o servidor, que tem o nível, discordava). O quadro diz o nível do jogo desta thread.
        horizon_client::collisions::machinery_transmitter_collision::enter(self.native_dimension());
        crate::game::machinery_transmitter_client::collision_refresh(self);
        frame(self);
        // A tela de máquina fechada limpa o estado do fundo (`container_dim`).
        horizon_client::hud::machinery_container_dim::set(self.mb_screen_owns_dim());
    }

    /// Se a tela de contêiner aberta é de uma máquina nossa, que o original desenha com o degradê do vanilla no fundo (injeção `container_dim`).
    pub(in crate::game) fn mb_screen_owns_dim(&self) -> bool {
        open_spec(self).is_some()
    }

    pub(in crate::game) fn mb_window_key(&self, key: &str, canceled: &mut bool) {
        key_pressed(self, key, canceled);
    }

    pub(in crate::game) fn mb_window_char(&self, ch: char, canceled: &mut bool) {
        char_typed(self, ch, canceled);
    }

    pub(in crate::game) fn mb_window_press(
        &self,
        view: &NativeScreenView<'_>,
        button: i32,
        modifiers: u32,
        canceled: &mut bool,
    ) {
        mouse_pressed(self, view, button, modifiers, canceled);
    }

    pub(in crate::game) fn mb_window_release(&self, canceled: &mut bool) {
        mouse_released(self, canceled);
    }

    pub(in crate::game) fn mb_window_drag(&self, view: &NativeScreenView<'_>, canceled: &mut bool) {
        mouse_dragged(self, view, canceled);
    }

    pub(in crate::game) fn mb_window_scroll(&self, view: &NativeScreenView<'_>, scroll: [f64; 2], canceled: &mut bool) {
        mouse_scrolled(self, view, scroll, canceled);
    }

    pub(in crate::game) fn mb_window_net(&mut self, player: u128, channel: &str, data: &[u8]) {
        net_from_client(self, player, channel, data);
    }

    pub(in crate::game) fn mb_window_player_in(&mut self, player: u128) {
        player_logged_in(self, player);
    }

    pub(in crate::game) fn mb_window_data_reloaded(&mut self) {
        data_reloaded(self);
    }

    pub(in crate::game) fn mb_window_net_server(&self, channel: &str, data: &[u8]) {
        net_from_server(channel, data);
    }

    /// Devolve se tomou o desenho dos slots.
    pub(in crate::game) fn mb_window_draw_slots(&mut self, batch: &mut HudBatch, ox: f32, oy: f32) -> bool {
        draw_slots(self, batch, ox, oy)
    }

    pub(in crate::game) fn mb_window_slot_at(&self, found: Option<usize>, x: f32, y: f32) -> Option<usize> {
        slot_at(self, found, x, y)
    }

    /// A aba, a desenhar junto do resto da tela do bloco.
    pub(in crate::game) fn mb_window_draw_tab(&self, batch: &mut HudBatch, block: &str, origin: [f32; 2]) {
        draw_tab(self, batch, block, origin);
    }

    /// O espelho do menu da tela de mod aberta, seja qual for o bloco.
    pub(in crate::game) fn mb_open_mirror(&self) -> Mirror {
        let block = self.mod_screen.as_ref().map(|screen| screen.block.clone()).unwrap_or_default();
        self.mb_mirror(&block)
    }

    /// O espelho do menu da tela de mod aberta, que o servidor sincroniza (`MekanismContainer.trackedData` no cliente): o estado que as telas, as
    /// abas e as condições de `screen_elements` leem. Antes do primeiro pacote, ou sem tela do bloco `block`, o menu vazio dele.
    pub(in crate::game) fn mb_mirror(&self, block: &str) -> Mirror {
        let pos = self.mb_open_screen_pos();
        mirrors(|table| pos.and_then(|pos| table.get(&pos)).map(|entry| entry.mirror.clone()))
            .unwrap_or_else(|| Mirror::new(&spec_of(block).map(|spec| (spec.trackers)()).unwrap_or_default()))
    }

    /// Se o ponto `at` (relativo ao canto da tela) está sobre a janela de upgrades ou a aba dela: um elemento ativo da tela.
    pub(in crate::game) fn mb_window_over(&self, at: [f32; 2]) -> bool {
        over_element(self, at)
    }

    /// Se a janela de upgrades aberta cobre o ponto `at` (relativo ao canto da tela).
    pub(in crate::game) fn mb_window_covers(&self, at: [f32; 2]) -> bool {
        covers(at)
    }

    /// Se a janela do livro de receitas está aberta (a aba dela fica inativa, `setListenerTab`).
    pub(in crate::game) fn mb_recipe_book_open(&self) -> bool {
        index_of(&client(), Kind::RecipeBook).is_some()
    }

    /// Os tooltips da aba e da janela; devolve se mostrou algum.
    pub(in crate::game) fn mb_window_tooltip(
        &self,
        batch: &mut HudBatch,
        block: &str,
        at: [f32; 2],
        cursor: (f32, f32),
    ) -> bool {
        tooltip(self, batch, block, at, cursor)
    }

    /// O bloco da tela de mod aberta.
    pub(in crate::game) fn mb_open_screen_pos(&self) -> Option<[i32; 3]> {
        self.mod_screen.as_ref().and_then(|screen| screen.pos())
    }

    /// O jogador abriu o menu do bloco `block` em `pos`: sessão nova e todos os dados.
    pub(in crate::game) fn mb_container_open(&mut self, player: u128, pos: [i32; 3], block: &str) {
        container_open(self, player, pos, block);
    }

    /// O `broadcastChanges` de todos os menus abertos.
    pub(in crate::game) fn mb_container_broadcast(&mut self) {
        broadcast(self);
    }

    /// Quem fechou o menu sai das sessões.
    pub(in crate::game) fn mb_container_recount(&self) {
        recount(self);
    }

    /// `playersUsing.size()` do tile em `pos`.
    pub(in crate::game) fn mb_container_players(pos: [i32; 3]) -> usize {
        players_using(pos)
    }

    /// A janela escolhida por quem age no contêiner em `pos`.
    pub(in crate::game) fn mb_window_selected(&self, pos: [i32; 3]) -> Option<SelectedWindow> {
        selected_for(pos, acting(self))
    }
}
