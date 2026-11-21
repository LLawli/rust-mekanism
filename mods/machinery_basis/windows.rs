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
use machinery_basis_core::recipe_book_data::{ChangeSettings, ClientKnown, Displays, SendOrder, ToClient, ToServer, Unlocked, settings_fields};
use machinery_basis_core::recipe_book_view as rbv;
use machinery_basis_core::recipe_place as rp;
use machinery_basis_core::stacked_contents::StackedContents;
use machinery_basis_core::resource;
use machinery_basis_core::scroll_list::ScrollList;
use machinery_basis_core::security_desk_screen as dsk;
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
    /// `MultiPlayerGameMode.handlePlaceRecipe`: o livro manda colocar o display, com o shift (`useMaxItems`).
    PlaceRecipe { id: i32, max: bool },
    /// Ctrl+C (0), Ctrl+V (1) e Ctrl+X (2) na caixa de busca do livro (`EditBox.keyPressed`): mexem na área de transferência do jogo.
    BookShortcut(u8),
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
    /// A caixa de busca do livro (`EditBox`, pelo `TextField` do Horizon, que é o porte dele): o texto, o cursor, a seleção e o primeiro
    /// caractere à mostra. A posição dela é relativa à janela.
    book_field: horizon_client::screen_draw::TextField,
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
    /// O que é do componente do livro da tela e não da janela (`RecipeBookComponent.time`, `lastPlacedRecipe`, `lastRecipe`): passa de
    /// uma abertura da janela para a outra enquanto a tela é a mesma.
    book_carry: Option<rbv::Carried>,
    /// O display cujo fantasma está nos slots da tela (`IGhostRecipeComponent.setGhostData`), até um clique num slot de receita, outra
    /// colocação ou a tela fechar.
    ghost: Option<i32>,
    /// `skipNextRelease`: o clique que já valeu ao apertar não vale de novo ao soltar.
    skip_release: bool,
    /// O botão esquerdo está apertado (o arrasto que a caixa de busca do livro segue).
    drag_left: bool,
    /// `MouseHandler.lastClick` e `lastClickButton`: quando e com que botão foi o último clique na tela (o duplo clique é outro do
    /// mesmo botão em menos de 250 ms).
    last_click: Option<(i64, i32)>,
    /// A tela da Security Desk aberta (`GuiSecurityDesk`), enquanto o bloco da tela é uma mesa.
    desk: Option<DeskUi>,
    /// `GuiMekanism.getTimeOpened`: quando a tela de agora abriu, de onde os textos que rolam contam.
    screen_opened_ms: i64,
}

static CLIENT: Mutex<Client> = Mutex::new(Client {
    screen_block: None,
    screen_pos: None,
    positions: None,
    windows: Vec::new(),
    pending: Vec::new(),
    sent_selection: None,
    click_sound: false,
    book_carry: None,
    ghost: None,
    skip_release: false,
    drag_left: false,
    last_click: None,
    desk: None,
    screen_opened_ms: 0,
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
    // O componente do livro é da tela: o relógio, as últimas receitas, a busca, a aba e a página continuam de onde a janela anterior
    // os deixou (`RecipeBookComponent.initVisuals` de novo).
    let carry = state.book_carry.clone();
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
            book: (kind == Kind::RecipeBook).then(|| new_book_view(spec, &contents, carry.as_ref())).flatten(),
            book_field: new_book_field(game, carry.as_ref().filter(|_| kind == Kind::RecipeBook)),
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
    desk_frame(game);
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
            // `GuiMekanism.removed`: o componente do livro e os fantasmas dos slots somem com a tela.
            guard.book_carry = None;
            guard.ghost = None;
            guard.skip_release = false;
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
            state.screen_opened_ms = now_ms();
            state.windows.clear();
            state.pending.clear();
            state.book_carry = None;
            state.ghost = None;
            state.skip_release = false;
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
        let mut carry = None;
        for open in guard.windows.iter_mut() {
            if let Some(view) = open.book.as_mut() {
                // O `a` do `extractRenderState`: `DeltaTracker.getGameTimeDeltaTicks`, os milissegundos do quadro pelo tick do jogo
                // (`Minecraft.getTickTargetMillis`: o do nível quando ele anda normalmente e é mais lento que 50 ms).
                let ticks = rb::delta_ticks(now - open.book_clock_ms, game.clock.runs_normally(), game.clock.tick_duration().as_secs_f32() * 1000.0);
                if !ctrl {
                    view.time += ticks;
                }
                view.advance_animations(ticks);
                open.book_clock_ms = now;
                if let Some(contents) = &contents {
                    let data = view.data();
                    if dirty {
                        let (known, highlighted) = unlocked(|book| (book.table().clone(), book.highlighted().collect::<Vec<i32>>()));
                        view.update_book(&known, highlighted.into_iter(), &|row| data.can_craft(row.0, contents));
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
                carry = Some(view.carried());
            }
        }
        if carry.is_some() {
            guard.book_carry = carry;
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
            Action::PlaceRecipe { id, max } => send_menu_button(game, rp::place_button(id, max)),
            Action::BookShortcut(which) => {
                let shortcut = match which {
                    0 => horizon_client::clipboard::EditShortcut::Copy,
                    1 => horizon_client::clipboard::EditShortcut::Paste,
                    _ => horizon_client::clipboard::EditShortcut::Cut,
                };
                let m = horizon_client::screen_draw::Measure(&game.gui.atlas.font);
                let mut guard = client();
                if let Some(open) = guard.windows.iter_mut().find(|open| open.kind == Kind::RecipeBook) {
                    open.book_field.shortcut(&m, shortcut, &mut game.clipboard);
                    if let Some(view) = open.book.as_mut() {
                        view.set_search(&open.book_field.value);
                    }
                }
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
    if desk_key(game, key, canceled) {
        return;
    }
    if open_spec(game).is_none() {
        return;
    }
    let shift = game.has_shift_down();
    // `AbstractContainerScreen.keyPressed`: com o cursor vazio sobre um slot, as teclas da barra e a da outra mão trocam o item
    // (`SWAP`), e a de largar joga o que o slot tem (`THROW`); todas passam por `slotClicked`.
    // Um slot com fantasma não é o `hoveredSlot` (o `GuiSlot` ativo o cobre): a tecla não clica nele.
    let ghost = client().ghost;
    let slot_key = !key_is_book_only(game, key)
        && game.carried.is_none()
        && crafting_slot_hovered(game, open_spec(game)).is_some_and(|which| !open_spec(game).is_some_and(|spec| slot_ghosted(spec, ghost, which)));
    let mut guard = client();
    let state = &mut *guard;
    // `GuiMekanism.keyPressed`: as janelas, da do topo para baixo; a do livro passa a tecla antes ao componente.
    for index in 0..state.windows.len() {
        if state.windows[index].kind != Kind::RecipeBook {
            continue;
        }
        let Client { windows, pending, click_sound, ghost, .. } = state;
        if book_key(game, &mut windows[index], key, shift, pending, click_sound, ghost) {
            *canceled = true;
            return;
        }
    }
    if slot_key {
        recipe_slot_clicked(state);
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
    if desk_char(game, ch, canceled) {
        return;
    }
    if open_spec(game).is_none() {
        return;
    }
    let mut guard = client();
    for open in guard.windows.iter_mut() {
        if open.kind == Kind::RecipeBook && book_char(game, open, ch) {
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
    if desk_press(game, view, button, modifiers, canceled) {
        return;
    }
    let Some(spec) = open_spec(game) else {
        return;
    };
    let Some(screen) = screen_of(view, game) else {
        return;
    };
    let mouse = mouse_of(view);
    let sync = read_sync(game);
    let over_recipe_slot = crafting_slot_hovered(game, Some(spec));
    let carrying = game.carried.is_some();
    let config = if client().windows.iter().any(|open| open.kind.is_config()) {
        config_view(game, spec)
    } else {
        None
    };
    let mut guard = client();
    let state = &mut *guard;
    let shift = modifiers & 1 != 0;
    // `MouseHandler.onButton`: o duplo clique é o segundo do mesmo botão em menos de 250 ms.
    let now = now_ms();
    let mut double = state.last_click.is_some_and(|(at, last)| now - at < 250 && last == button);
    state.last_click = Some((now, button));
    state.drag_left = button == 0;
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
        let Client { windows, pending, click_sound, ghost, .. } = state;
        let double = std::mem::take(&mut double);
        if book_click(game, &mut windows[book], rel, button, shift, double, click_sound, pending, ghost, spec) == BookClick::Handled {
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
        // `AbstractContainerScreen.mouseClicked` sobre um slot com o cursor vazio: o clique vale ao apertar (`slotClicked`), e com um
        // item no cursor, ao soltar. `VirtualRecipeBookComponent.slotClicked` de um slot de receita tira os fantasmas.
        if let Some(which) = over_recipe_slot {
            // O slot com fantasma toma o clique esquerdo antes da tela (`GuiSlot` ativo): com o cursor vazio nada acontece e o fantasma
            // fica; com um item, o clique vale ao soltar (`mouseReleased` chama `slotClicked` com o cursor cheio), que é quando o
            // fantasma some. O direito não é do `GuiSlot` e segue para o slot.
            let ghosted = button == 0 && slot_ghosted(spec, state.ghost, which);
            if ghosted && !carrying {
                *canceled = true;
                return;
            }
            if carrying || ghosted {
                state.skip_release = false;
            } else {
                recipe_slot_clicked(state);
                state.skip_release = true;
            }
        }
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
    if desk_release(game) {
        return;
    }
    let Some(spec) = open_spec(game) else {
        return;
    };
    // `AbstractContainerScreen.mouseReleased`: com um item no cursor, o clique no slot sob o mouse vale agora (`slotClicked`), a não
    // ser que já tenha valido ao apertar (`skipNextRelease`).
    let over_recipe_slot = game.carried.is_some() && crafting_slot_hovered(game, Some(spec)).is_some();
    let mut state = client();
    state.drag_left = false;
    if !std::mem::take(&mut state.skip_release) && over_recipe_slot && !state.windows.iter().any(|open| open.window.dragging() || open.list.dragging()) {
        recipe_slot_clicked(&mut state);
    }
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
    if desk_drag(game, view, canceled) {
        return;
    }
    if open_spec(game).is_none() {
        return;
    }
    let Some(screen) = screen_of(view, game) else {
        return;
    };
    let mouse = mouse_of(view);
    let mut state = client();
    // `GuiRecipeBookWindow.mouseDragged` passa antes pelo livro: com a caixa de busca focada, todo arrasto com o botão esquerdo é dela
    // (`AbstractWidget.mouseDragged`, `EditBox.onDrag`: o cursor segue o mouse e a seleção se estende), e a janela não anda.
    if state.drag_left
        && let Some(book) = index_of(&state, Kind::RecipeBook)
        && state.windows[book].book.as_ref().is_some_and(|view| view.focused)
    {
        let rel = relative(&state.windows[book], screen, mouse);
        let m = horizon_client::screen_draw::Measure(&game.gui.atlas.font);
        state.windows[book].book_field.click(&m, rel[0], true);
        *canceled = true;
        return;
    }
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
    if desk_scroll(game, view, scroll, canceled) {
        return;
    }
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
    // Um slot de receita com fantasma tem o `GuiSlot` ativo por cima (`GuiSlot.isActive` com `ghostRecipeData`), e o slot do vanilla
    // debaixo dele não é alvo (`GuiMekanism.isHovering`): sem realce e sem tooltip do slot. Quem pergunta pelo slot de receita sob o
    // cursor ([`crafting_slot_hovered`]) o recebe assim mesmo.
    if !RAW_SLOT.with(std::cell::Cell::get)
        && let Some(id) = found
        && let Some(which) = spec.book_slots.iter().position(|index| id == 300 + index)
        && slot_ghosted(spec, state.ghost, which)
    {
        return None;
    }
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
    let alignment = if center { machinery_basis_core::gui::TextAlignment::Center } else { machinery_basis_core::gui::TextAlignment::Left };
    aligned_scrolling_text(game, batch, text, area, alignment, color, scale, since);
}

/// [`scrolling_text`] com o alinhamento do original (`TextAlignment.getTarget`): à esquerda, centrado ou à direita.
#[allow(clippy::too_many_arguments)]
fn aligned_scrolling_text(
    game: &Game<'_>,
    batch: &mut HudBatch,
    text: &str,
    area: [f32; 4],
    alignment: machinery_basis_core::gui::TextAlignment,
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
    } else {
        alignment.target(min_x, max_x, width)
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

/// `ClientRecipeBook.known` e `highlight`: os displays que o servidor disse que o jogador conhece, por id, os de todas as receitas do
/// jogo (as do vanilla e as de outros livros também), na tabela do `HashMap` do original, que decide a ordem das coleções. O servidor é
/// quem guarda (`ServerRecipeBook`, no NBT do jogador) e manda o que muda.
static BOOK_UNLOCKED: Mutex<Option<ClientKnown>> = Mutex::new(None);

/// O tamanho de um pedaço do livro no canal do mod, abaixo do teto dele.
const BOOK_PACKET_MAX: usize = 24_000;

/// No servidor: as listas `recipes` e `toBeDisplayed` de cada jogador da última vez e os displays que saíram delas.
#[allow(clippy::type_complexity)]
static BOOK_DERIVED: Mutex<Option<HashMap<u128, (std::collections::BTreeSet<String>, std::collections::BTreeSet<String>, Unlocked)>>> = Mutex::new(None);

/// No servidor: os jogadores cujo próximo envio leva o que um `/recipe give` com várias receitas deu, em ordem de id
/// (`RecipeCommand.giveRecipes` com `getRecipes()`); `None` na lista vale para todos (o alvo do comando não é quem age).
static BOOK_ORDERED: Mutex<Vec<Option<u128>>> = Mutex::new(Vec::new());

/// O livro do cliente mudou (um pacote do servidor): a janela aberta refaz as coleções (`RecipeBookComponent.recipesUpdated`).
static BOOK_DIRTY: AtomicBool = AtomicBool::new(false);

/// O que o cliente tem a dizer ao servidor (a configuração que mudou, a receita vista), mandado no quadro.
static BOOK_OUTBOX: Mutex<Vec<ToServer>> = Mutex::new(Vec::new());

/// O que [`book_restore`] já conferiu de cada jogador: quantas receitas os livros registrados têm e o tamanho da lista lida dele.
static BOOK_RESTORED: Mutex<Option<HashMap<u128, (usize, usize)>>> = Mutex::new(None);

/// No servidor: o livro que cada jogador já recebeu e os tipos de livro das configurações mandadas, para mandar só o que muda.
static BOOK_SENT: Mutex<Option<HashMap<u128, (Unlocked, Vec<&'static str>)>>> = Mutex::new(None);

fn unlocked<R>(act: impl FnOnce(&mut ClientKnown) -> R) -> R {
    let mut guard = BOOK_UNLOCKED.lock().unwrap_or_else(|e| e.into_inner());
    act(guard.get_or_insert_with(ClientKnown::default))
}

fn book_sent<R>(act: impl FnOnce(&mut HashMap<u128, (Unlocked, Vec<&'static str>)>) -> R) -> R {
    let mut guard = BOOK_SENT.lock().unwrap_or_else(|e| e.into_inner());
    act(guard.get_or_insert_with(HashMap::new))
}

/// Os livros que os mods registraram até agora (cada um quando o bloco dele aparece), um por tipo de livro, na ordem do registro.
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

/// Todas as receitas do jogo com display (a numeração do `RecipeManager` do original), que o primeiro livro registrado traz: é uma
/// tabela só para o jogo inteiro.
fn book_displays() -> Displays {
    Displays(books().first().map_or(&[], |book| book.displays))
}

/// O `/recipe give` com várias receitas: o que entrar no livro de quem age vai ao cliente em ordem de id no próximo envio.
pub(in crate::game) fn book_ordered_batch(game: &mut Game<'_>) {
    let mut list = BOOK_ORDERED.lock().unwrap_or_else(|e| e.into_inner());
    if !list.contains(&game.lan_acting_uuid) {
        list.push(game.lan_acting_uuid);
    }
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

fn new_book_view(spec: &UpgradeWindowSpec, contents: &StackedContents, carried: Option<&rbv::Carried>) -> Option<rbv::View> {
    let data = spec.book?;
    let (known, highlighted) = unlocked(|book| (book.table().clone(), book.highlighted().collect::<Vec<i32>>()));
    let filtering = book_settings(|settings| settings.is_filtering(data.settings_type));
    Some(rbv::View::reopened(data, &known, highlighted.into_iter(), filtering, &|row| data.can_craft(row.0, contents), carried))
}

/// `RecipeBookComponent.initVisuals`: a caixa de busca nova (`new EditBox(font, xo + 25, yo + 13, 81, 14, ...)`, `setMaxLength(50)`,
/// `setTextColor(-1)`, `setValue(oldEdit)`, `setHint(SEARCH_HINT)`), com o texto que a caixa anterior desta tela tinha e o cursor no fim.
fn new_book_field(game: &Game<'_>, carried: Option<&rbv::Carried>) -> horizon_client::screen_draw::TextField {
    let [xo, yo] = BOOK_ORIGIN;
    let rect = rb::search_rect(xo, yo);
    let mut field = horizon_client::screen_draw::TextField::new(rect[0], rect[1], rect[2], rect[3]);
    field.max_length = rb::SEARCH_MAX_LENGTH;
    field.text_color = -1;
    field.hint = Some(horizon_client::text::Text::styled(
        lang::get("gui.recipebook.search_hint"),
        horizon_client::text::Style { color: Some(0xAAAAAA), italic: Some(true), ..Default::default() },
    ));
    if let Some(carried) = carried {
        field.set_value(&horizon_client::screen_draw::Measure(&game.gui.atlas.font), carried.search());
    }
    field
}

/// `EditBox.selectWord`: a palavra sob o clique (o duplo clique na caixa).
fn book_select_word(field: &mut horizon_client::screen_draw::TextField, m: &horizon_client::screen_draw::Measure<'_>, x: f64) {
    field.click(m, x, false);
    let clicked = field.cursor;
    let (start, end) = (field.word_position(-1, clicked), field.word_position(1, clicked));
    field.move_cursor_to(m, start, false);
    field.move_cursor_to(m, end, true);
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
        Some(ToClient::Settings(bytes)) => match BookSettings::decode_named(&bytes) {
            Some(settings) => {
                book_settings(|mine| {
                    for (book_type, value) in &settings {
                        mine.set(book_type, *value);
                    }
                });
                BOOK_DIRTY.store(true, Ordering::Relaxed);
            }
            None => eprintln!("machinery_basis: o servidor mandou configurações do livro ilegíveis"),
        },
        // `ClientPacketListener.handlePlaceRecipe`: a tela aberta mostra o fantasma (`GuiMekanism.fillGhostRecipe`); sem tela, nada.
        Some(ToClient::PlaceGhost(id)) => {
            let mut state = client();
            if state.screen_block.is_some() {
                state.ghost = Some(id);
            }
        }
        Some(packet) => {
            // O pacote com `replace` só vem na entrada (`sendInitialRecipeBook`), quando o cliente do original tem um
            // `ClientRecipeBook` novo (`ClientPacketListener.handleLogin`): a tabela de `known` recomeça do zero.
            if matches!(packet, ToClient::Add { replace: true, .. }) {
                unlocked(|book| *book = ClientKnown::default());
            }
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

/// O livro do jogador `player` no servidor: o do nível quando é ele quem age (o Horizon põe o estado dele no lugar, `as_guest`), senão o
/// do convidado guardado no hub. Todo jogador é um convidado do hub, o da janela também; o 0 só vale sem hub.
fn with_server_book<R>(
    game: &mut Game<'_>,
    player: u128,
    act: impl FnOnce(&mut horizon_client::recipe_book::Settings, &mut Option<Tag>) -> R,
) -> Option<R> {
    if game.lan_acting_uuid == Some(player) || (player == 0 && game.lan_acting_uuid.is_none() && game.lan.is_none()) {
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
            let Some(recipe) = book_displays().ids().find(|row| row.0 == id).map(|row| row.1) else {
                return;
            };
            with_server_book(game, player, |book, _| book.remove_highlight(recipe));
        }
        None => {}
    }
}

/// Ao abrir o mundo, o Horizon tira do livro do dono as receitas que o `RecipeIndex` dele não conhece (`Game::load_world`:
/// `ServerRecipeBook.loadUntrusted` com o `RecipeIndex`, que só lê `data/minecraft/recipe`), e é desse livro cortado que o jogador da
/// janela nasce no servidor (`lan::playerdata::load_owner`). No original o `RecipeManager` tem as do mod e elas ficam. O composto lido do
/// jogador (`Guest::saved`) ainda tem as duas listas como estavam no arquivo: as receitas dos livros registrados que estão nele voltam
/// ao livro e saem da lista lida (o `player_tag` a reescreve ao gravar), o que faz disto uma vez só por receita, sem depender de quando
/// cada livro é registrado (decisão 267; o H274 tiraria isto). Para um convidado, que o Horizon lê sem cortar, isto não muda nada.
fn book_restore(book: &mut horizon_client::recipe_book::Settings, extra: &mut Option<Tag>, names: &std::collections::HashSet<&str>, seen: &mut (usize, usize)) {
    let Some(Tag::Compound(saved)) = extra.as_mut().and_then(|tag| tag.get_mut("recipeBook")) else {
        return;
    };
    if !matches!(saved.get(&JavaString::from("recipes")), Some(Tag::List(_))) {
        return;
    }
    // As duas listas como nomes; o que não é texto fica onde está.
    let names_of = |saved: &Compound, key: &str| -> Vec<String> {
        match saved.get(&JavaString::from(key)) {
            Some(Tag::List(list)) => list.iter().filter_map(|tag| if let Tag::String(name) = tag { Some(name.to_string_lossy()) } else { None }).collect(),
            _ => Vec::new(),
        }
    };
    let mut recipes = names_of(saved, "recipes");
    let mut display = names_of(saved, "toBeDisplayed");
    let Some((known, highlight)) = machinery_basis_core::recipe_book_data::restore_saved(&mut recipes, &mut display, names, seen) else {
        return;
    };
    // O que voltou sai das listas lidas (o `player_tag` as reescreve ao gravar).
    for key in ["recipes", "toBeDisplayed"] {
        if let Some(Tag::List(list)) = saved.get_mut(&JavaString::from(key)) {
            list.retain(|tag| !matches!(tag, Tag::String(name) if names.contains(name.to_string_lossy().as_str())));
        }
    }
    for name in highlight {
        book.to_be_displayed.insert(name);
    }
    book.recipes.extend(known);
}

/// `ServerRecipeBook.removeRecipes` de receitas de mod (o `/recipe take`): elas saem também das listas lidas do jogador que age, para o
/// [`book_restore`] de um livro registrado depois não as trazer de volta.
pub(in crate::game) fn book_forget_saved(game: &mut Game<'_>, names: &[&str]) {
    let Some(Tag::Compound(saved)) = game.level.player_extra.as_mut().and_then(|tag| tag.get_mut("recipeBook")) else {
        return;
    };
    for key in ["recipes", "toBeDisplayed"] {
        if let Some(Tag::List(list)) = saved.get_mut(&JavaString::from(key)) {
            list.retain(|tag| !matches!(tag, Tag::String(name) if names.contains(&name.to_string_lossy().as_str())));
        }
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
    let displays = book_displays();
    if displays.0.is_empty() {
        return;
    }
    let types = book_types();
    // Todo jogador é um convidado do hub no servidor, o da janela também (`Player[/127.0.0.1:0] logged in`).
    let players: Vec<u128> = match game.lan.as_ref() {
        Some(host) => host.hub.guests.iter().filter(|guest| guest.placed).map(|guest| guest.uuid).collect(),
        None => return,
    };
    book_sent(|sent| sent.retain(|player, _| players.contains(player)));
    if let Some(derived) = BOOK_DERIVED.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        derived.retain(|player, _| players.contains(player));
    }
    let ordered: Vec<Option<u128>> = std::mem::take(&mut *BOOK_ORDERED.lock().unwrap_or_else(|e| e.into_inner()));
    // O que o Horizon corta do livro do dono ao abrir o mundo e volta: toda receita da tabela (as dos livros registrados e as outras do
    // jogo que o `RecipeIndex` dele não numera).
    let names: std::collections::HashSet<&str> = displays.0.iter().map(|row| row.0).collect();
    for player in players {
        let mut seen = BOOK_RESTORED
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_or_insert_with(HashMap::new)
            .get(&player)
            .copied()
            .unwrap_or((usize::MAX, usize::MAX));
        let state = with_server_book(game, player, |book, extra| {
            book_restore(book, extra, &names, &mut seen);
            // `ServerRecipeBook.known`: os displays de toda receita que o jogador conhece, as do vanilla também.
            // A tabela tem todos os displays do jogo: só é percorrida de novo quando as listas do jogador mudam.
            let mut derived = BOOK_DERIVED.lock().unwrap_or_else(|e| e.into_inner());
            let derived = derived.get_or_insert_with(HashMap::new);
            let now = match derived.get(&player) {
                Some((recipes, display, now)) if *recipes == book.recipes && *display == book.to_be_displayed => now.clone(),
                _ => {
                    let now = Unlocked::from_recipes(displays.ids(), |recipe| book.recipes.contains(recipe), |recipe| book.to_be_displayed.contains(recipe));
                    derived.insert(player, (book.recipes.clone(), book.to_be_displayed.clone(), now.clone()));
                    now
                }
            };
            (now, server_book_settings(extra, &types))
        });
        BOOK_RESTORED.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(player, seen);
        let Some((now, settings)) = state else {
            continue;
        };
        let before = book_sent(|sent| sent.get(&player).cloned());
        // A ordem dos displays no pacote é a ordem em que entram no `HashMap` do cliente: na entrada, a do `IdentityHashSet` das chaves
        // das receitas (`sendInitialRecipeBook`), com o hash de identidade de cada chave sorteado por processo como no JVM; depois, a
        // de quem premiou (decisão 269).
        let order = SendOrder {
            displays,
            identity: &|recipe| machinery_basis_core::identity_hash::identity_hash(recipe),
            award: &|recipe| machinery_basis_core::identity_hash::identity_hash(&format!("recipe-award:{recipe}")),
            ascending: ordered.contains(&Some(player)) || ordered.contains(&None),
        };
        // As configurações vão com o livro inteiro e de novo quando um tipo de livro novo é registrado (o bloco dele apareceu).
        let packets = machinery_basis_core::recipe_book_data::server_sync(
            before.as_ref().map(|(sent, sent_types)| (sent, sent_types.as_slice())),
            &now,
            &types,
            || settings.encode_named(&types),
            // O display de um livro registrado vai com a categoria e o grupo; o de outra receita do jogo, só com o id.
            |id| {
                Some(rows.iter().find(|row| row.0 == id).map_or_else(
                    || rb::Entry { id, category: String::new(), group: None },
                    |row| rb::Entry { id: row.0, category: row.4.to_owned(), group: (row.3 >= 0).then_some(row.3) },
                ))
            },
            &order,
        );
        if packets.is_empty() {
            continue;
        }
        book_sent(|sent| sent.insert(player, (now, types.clone())));
        // O canal do mod leva até 30.000 bytes por mensagem (`horizon_protocol::mod_net::MAX_DATA`): o livro de quem conhece tudo vai em pedaços.
        for packet in packets.into_iter().flat_map(|packet| packet.split(BOOK_PACKET_MAX)) {
            if let Err(error) = game.native_net_send_to_player(player, BOOK_CHANNEL, &packet.encode()) {
                eprintln!("machinery_basis: o livro de receitas não chegou a {player:032x}: {error}");
            }
        }
    }
}

/// O recurso de uma pilha para a colocação (`ItemResource`: o item e os componentes): o id, e o que distingue a pilha quando ela tem
/// dano ou outros componentes.
fn place_key(stack: &ItemStack) -> String {
    if stack.damage == 0 && stack.extra.is_none() {
        stack.item.to_string()
    } else {
        format!("{}#{}:{:?}", stack.item, stack.damage, stack.extra)
    }
}

/// As regras dos dois slots de receita (0 a entrada, 1 a saída) com o que o dono do bloco diz da entrada.
struct PlaceRules<'a, 'g> {
    game: &'a Game<'g>,
    stacks: &'a HashMap<String, ItemStack>,
    valid: &'a dyn Fn(&Game<'_>, &ItemStack) -> bool,
    can_insert: &'a dyn Fn(&Game<'_>, &ItemStack) -> bool,
}

impl rp::Rules for PlaceRules<'_, '_> {
    fn max_stack(&self, resource: &str) -> i32 {
        self.stacks.get(resource).map_or(64, |stack| basis::stack_limit(self.game, stack) as i32)
    }

    fn valid(&self, slot: usize, resource: &str) -> bool {
        // `OutputInventorySlot`: o validador aceita tudo; quem barra é o `canInsert`.
        slot != 0 || self.stacks.get(resource).is_some_and(|stack| (self.valid)(self.game, stack))
    }

    fn can_insert(&self, slot: usize, resource: &str) -> bool {
        // `OutputInventorySlot`: `internalOnly`.
        slot == 0 && self.stacks.get(resource).is_some_and(|stack| (self.can_insert)(self.game, stack))
    }

    fn can_extract(&self, _: usize, _: &str) -> bool {
        // `InputInventorySlot` (`notExternal`) e `OutputInventorySlot` (`alwaysTrue`) com `AutomationType.MANUAL`.
        true
    }

    fn hash(&self, resource: &str) -> i32 {
        rp::item_resource_hash(resource)
    }
}

/// `ServerGamePacketListenerImpl.handlePlaceRecipe` com `MekanismContainer.handlePlacement`, no turno de quem clicou (o botão de menu
/// de `rp::place_button`): a receita é a do display no livro da máquina, o jogador tem de conhecê-la, e a colocação mexe no inventário
/// dele e nos slots de entrada e de saída do bloco. `tank` é o tanque de químico da máquina (o químico e a quantidade), e `valid` e
/// `can_insert` o `isValid` e o `canInsert(MANUAL)` do slot de entrada. Devolve se o botão era um pedido de colocação.
pub(in crate::game) fn recipe_place_button(
    game: &mut Game<'_>,
    entity: &mut horizon_client::containers::ModBlockEntity,
    id: i32,
    tank: Option<(String, i64)>,
    valid: &dyn Fn(&Game<'_>, &ItemStack) -> bool,
    can_insert: &dyn Fn(&Game<'_>, &ItemStack) -> bool,
) -> bool {
    let Some((display, use_max)) = rp::place_button_request(id) else {
        return false;
    };
    let Some(book) = spec_of(&entity.id).and_then(|spec| spec.book.map(|book| (book, spec.book_slots))) else {
        return true;
    };
    let (book, [input, output]) = book;
    // `getRecipeFromDisplay` e `player.getRecipeBook().contains(recipe.id())`. Um display de outro tipo de receita (que só um pacote
    // forjado mandaria a esta máquina) é recusado aqui.
    let Some(row) = book.rows.iter().find(|row| row.0 == display && row.2 == book.book_type) else {
        return true;
    };
    if !game.level.recipe_book.recipes.contains(row.1) || entity.items.len() <= input.max(output) {
        return true;
    }
    let mut stacks: HashMap<String, ItemStack> = HashMap::new();
    let mut keyed = |stack: Option<&ItemStack>| -> rp::Stack {
        let stack = stack.filter(|stack| stack.count > 0)?;
        let key = place_key(stack);
        stacks.entry(key.clone()).or_insert_with(|| stack.clone());
        Some((key, stack.count as i32))
    };
    let before = rp::State {
        main: (0..rp::MAIN_SLOTS).map(|slot| keyed(game.inventory.get(slot).and_then(Option::as_ref))).collect(),
        offhand: keyed(game.offhand.as_ref()),
        selected: game.selected,
        slots: vec![keyed(entity.items[input].as_ref()), keyed(entity.items[output].as_ref())],
        dropped: Vec::new(),
    };
    let Some(recipe) = book.placement(display, tank.as_ref().map(|(name, amount)| (name.as_str(), *amount))) else {
        return true;
    };
    let mut now = before.clone();
    // `player.isCreative()`: o `allowDroppingItemsToClear`.
    let creative = game.mode_id() == 1;
    let action = {
        let rules = PlaceRules { game: &*game, stacks: &stacks, valid, can_insert };
        rp::place_recipe(&mut now, &rules, &recipe, rp::Containers { inputs: &[0], outputs: &[1] }, use_max, creative)
    };
    let stack_of = |stack: &rp::Stack| -> Option<ItemStack> {
        let (key, count) = stack.as_ref()?;
        let mut stack = stacks.get(key)?.clone();
        stack.count = *count as u32;
        Some(stack)
    };
    for slot in 0..rp::MAIN_SLOTS {
        if now.main[slot] != before.main[slot] {
            if game.inventory.len() <= slot {
                game.inventory.resize(slot + 1, None);
            }
            game.inventory[slot] = stack_of(&now.main[slot]);
        }
    }
    if now.offhand != before.offhand {
        game.offhand = stack_of(&now.offhand);
    }
    for (at, slot) in [input, output].into_iter().enumerate() {
        if now.slots[at] != before.slots[at] {
            entity.items[slot] = stack_of(&now.slots[at]);
        }
    }
    // `PlayerInventoryWrapper.drop(resource, amount, false, false, ...)`: `player.drop(stack, false, false)`, sem o nome de quem largou.
    for dropped in &now.dropped {
        if let Some(stack) = stack_of(&Some(dropped.clone()))
            && let Some(eye) = game.eye_position()
        {
            game.swing_hand(horizon_client::first_person::Hand::Main, horizon_client::first_person::SwingAnimation::DEFAULT);
            game.throw_from_eyes(stack, eye, false);
        }
    }
    match action {
        rp::Action::Nothing => {}
        // `ClientboundPlaceGhostRecipePacket(containerId, display)`.
        rp::Action::PlaceGhostRecipe => {
            if let Some(player) = game.lan_acting_uuid
                && let Err(error) = game.native_net_send_to_player(player, BOOK_CHANNEL, &ToClient::PlaceGhost(display).encode())
            {
                eprintln!("machinery_basis: o fantasma da receita não chegou a {player:032x}: {error}");
            }
        }
        // No original o `ClassCastException` de `handlePlacement` é registrado e engolido (`ServerPacketListener.onPacketError`: `Failed to
        // handle packet ..., suppressing error`), e nada muda: aqui também só o registro (decisão 268, medido em jogo).
        rp::Action::ClassCast => {
            eprintln!("machinery_basis: Failed to handle the recipe placement of {}, suppressing error: ClassCastException (not a MekanismRecipe)", row.1);
        }
    }
    true
}

/// O fantasma de um slot de receita da tela aberta (`GuiSlot.tryDrawGhost`), com o item em `at`: o fundo avermelhado, o item que o
/// `SlotSelectTime` escolhe entre os que o ingrediente (ou a saída) mostra, o véu branco e a quantidade. `inputs` é quantos componentes
/// de entrada a tela registrou no livro (`ghostInputs`: os slots de entrada e as barras ou medidores de tanque): com outro número de
/// ingredientes no display as entradas ficam sem fantasma (`MekanismRecipeBookComponent.fillGhostRecipe`). Devolve se desenhou.
fn ghost_slot(game: &Game<'_>, batch: &mut HudBatch, at: [f32; 2], inputs: usize, output: bool, index: usize) -> bool {
    let Some((ghost, select)) = shown_ghost(game) else {
        return false;
    };
    let lists = if output { ghost.2 } else { ghost.1 };
    if !output && ghost.1.len() != inputs {
        return false;
    }
    let Some((item, count)) = lists.get(index).and_then(|entries| rbv::ghost_shown(entries, select)) else {
        return false;
    };
    let atlas = &game.gui.atlas;
    batch.fill(atlas, at[0], at[1], 16.0, 16.0, basis::GHOST_RED);
    let stack = ItemStack::new(item, count.max(1) as u32);
    // `fakeItem`: o item sem as decorações.
    game.draw_item_model(batch, &stack, at[0], at[1]);
    batch.fill(atlas, at[0], at[1], 16.0, 16.0, basis::GHOST_WHITE);
    // A saída desenha `itemDecorations` e a entrada só `itemCount`: num item de receita, sem dano, as duas dão a quantidade.
    game.draw_item_decorations(batch, &stack, at[0], at[1], None);
    true
}

/// Se o slot de receita `which` (0 a entrada, 1 a saída) da tela aberta tem fantasma: o `GuiSlot` dele fica ativo e toma o clique
/// esquerdo (`GuiSlot.isActive` com `ghostRecipeData`, `AbstractWidget.mouseClicked`), sem som e sem chegar ao slot.
fn slot_ghosted(spec: &UpgradeWindowSpec, ghost: Option<i32>, which: usize) -> bool {
    let Some(row) = ghost.and_then(|id| spec.book.and_then(|book| book.ghost(id))) else {
        return false;
    };
    let lists = if which == 0 { row.1 } else { row.2 };
    lists.first().is_some_and(|entries| !entries.is_empty())
}

/// O display do fantasma da tela aberta e o índice do `SlotSelectTime` de agora.
fn shown_ghost(game: &Game<'_>) -> Option<(&'static rbv::GhostRow, i32)> {
    let book = open_spec(game)?.book?;
    let (id, time) = {
        let state = client();
        (state.ghost?, state.book_carry.as_ref().map_or(0.0, |carry| carry.time))
    };
    Some((book.ghost(id)?, rb::slot_select_index(time)))
}

/// [`shown_ghost`] com o estado já na mão.
fn shown_ghost_of(spec: &UpgradeWindowSpec, state: &Client) -> Option<(&'static rbv::GhostRow, i32)> {
    let book = spec.book?;
    Some((book.ghost(state.ghost?)?, rb::slot_select_index(state.book_carry.as_ref().map_or(0.0, |carry| carry.time))))
}

/// O fantasma de uma barra de químico registrada como a entrada `index` (`GuiChemicalBar.setGhostData` com
/// `GhostSlot.createWithMaxTank`): o químico que o `SlotSelectTime` escolhe e o nível que a barra desenha (`renderBarGhostContents`).
fn ghost_chemical(game: &Game<'_>, inputs: usize, index: usize) -> Option<(&'static str, f64)> {
    let (ghost, select) = shown_ghost(game)?;
    if ghost.1.len() != inputs {
        return None;
    }
    let (name, amount) = rbv::ghost_shown(ghost.3.get(index)?, select)?;
    Some((name, rbv::ghost_bar_level(amount, rbv::ghost_max_tank(ghost))))
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
        draw_book_contents(game, batch, view, &open.book_field, open.book_focus_ms, wx, wy + rb::Y_OFFSET as f32, hover_ok, mouse);
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
/// `RIGHT` é 3), com `rel` relativo à janela. Na ordem do Java: a página (o overlay das variantes quando aberto, as setas e os botões das
/// receitas), a lupa e a caixa de busca (o foco), o filtro e as abas. O `GuiMekanism` oferece todo clique a todas as janelas, então um
/// clique em qualquer lugar fora da caixa tira o foco dela. O esquerdo numa receita a manda colocar (`tryPlaceRecipe`, com o shift para
/// `useMaxItems`) e tira os fantasmas (`clearGhostValues`); o direito abre o overlay de um botão com mais de uma receita.
#[allow(clippy::too_many_arguments)]
fn book_click(
    game: &Game<'_>,
    open: &mut Open,
    rel: [f64; 2],
    button: i32,
    shift: bool,
    double: bool,
    click_sound: &mut bool,
    pending: &mut Vec<Action>,
    ghost: &mut Option<i32>,
    spec: &UpgradeWindowSpec,
) -> BookClick {
    let [xo, yo] = BOOK_ORIGIN;
    let left = button == 0;
    let tab = book_tab_at(open, rel);
    let Open { book, book_field: field, book_focus_ms, .. } = open;
    let Some(view) = book.as_mut() else {
        return BookClick::Missed;
    };
    let inside = |rect: rb::Rect| rb::contains(rect, rel[0], rel[1]);
    // `RecipeBookPage.mouseClicked`: com o overlay aberto o clique é dele; senão as setas, só com o esquerdo e visíveis, e depois os
    // botões das receitas com o esquerdo e o direito (`isValidClickButton`).
    if view.overlay.is_none() {
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
    }
    let mouse = match button {
        0 => rbv::Mouse::Left,
        1 => rbv::Mouse::Right,
        _ => rbv::Mouse::Other,
    };
    match view.click_page(xo, yo, rel[0], rel[1], mouse) {
        rbv::PageClick::Place(id) => {
            *click_sound = true;
            *ghost = None;
            pending.push(Action::PlaceRecipe { id, max: shift });
            return BookClick::Handled;
        }
        // `tryPlaceRecipe` recusou e `mouseClicked` devolve falso: o clique segue para a janela, sem passar pela caixa de busca.
        rbv::PageClick::Refused => {
            *click_sound = true;
            return BookClick::Missed;
        }
        rbv::PageClick::Handled { sound } => {
            *click_sound |= sound;
            return BookClick::Handled;
        }
        rbv::PageClick::Missed => {}
    }
    // A lupa foca a caixa com qualquer botão (`magnifierIconPlacement.containsPoint`); a caixa, só com o esquerdo, e sem som (`EditBox.playDownSound`).
    let magnifier = inside(rb::magnifier_rect(xo, yo));
    if magnifier || (left && inside(rb::search_rect(xo, yo))) {
        if !magnifier {
            // `EditBox.onClick`: o cursor vai para onde o mouse está (o shift estende a seleção), e o duplo clique escolhe a palavra.
            let m = horizon_client::screen_draw::Measure(&game.gui.atlas.font);
            if double {
                book_select_word(field, &m, rel[0]);
            } else {
                field.click(&m, rel[0], shift);
            }
        }
        // `EditBox.setFocused(true)` reinicia o `focusedTime` (o pisca do cursor) a cada vez.
        *book_focus_ms = now_ms();
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
/// param nela (`capturesInput`); sem foco, a tecla do chat (`T`) foca a caixa e o caractere dela não entra (`ignoreTextInput`), e o Enter,
/// o Enter numérico e o espaço (`isSelection`) tocam o som do botão e mandam colocar de novo a última receita clicada.
fn book_key(game: &Game<'_>, open: &mut Open, key: &str, shift: bool, pending: &mut Vec<Action>, click_sound: &mut bool, ghost: &mut Option<i32>) -> bool {
    let Open { book, book_field: field, .. } = open;
    let Some(view) = book.as_mut() else {
        return false;
    };
    view.ignore_text = false;
    if key == "key.keyboard.escape" {
        return false;
    }
    if view.focused {
        // `EditBox.keyPressed`: Backspace e Delete apagam (a palavra com o Ctrl), Home, End e as setas movem o cursor (com o shift
        // estendem a seleção, com o Ctrl pulam a palavra), Ctrl+A seleciona tudo e Ctrl+C, Ctrl+V e Ctrl+X usam a área de
        // transferência. A caixa focada prende as outras teclas (`capturesInput`).
        use winit::keyboard::KeyCode as K;
        let m = horizon_client::screen_draw::Measure(&game.gui.atlas.font);
        let ctrl = game.has_control_down_with_quirk();
        let code = match key {
            "key.keyboard.backspace" => Some(K::Backspace),
            "key.keyboard.delete" => Some(K::Delete),
            "key.keyboard.home" => Some(K::Home),
            "key.keyboard.end" => Some(K::End),
            "key.keyboard.left" => Some(K::ArrowLeft),
            "key.keyboard.right" => Some(K::ArrowRight),
            "key.keyboard.a" => Some(K::KeyA),
            _ => None,
        };
        let alt = game.modifiers.alt_key();
        if let Some(code) = code.filter(|code| *code != K::KeyA || !alt) {
            field.key(&m, code, shift, ctrl);
            view.set_search(&field.value);
        } else if ctrl && !shift && !alt {
            match key {
                "key.keyboard.c" => pending.push(Action::BookShortcut(0)),
                "key.keyboard.v" => pending.push(Action::BookShortcut(1)),
                "key.keyboard.x" => pending.push(Action::BookShortcut(2)),
                _ => {}
            }
        }
        return true;
    }
    // `minecraft.options.keyChat.matches(event)`: a tecla do chat configurada.
    if key == game.settings.keys.get("key.chat") {
        view.ignore_text = true;
        view.focused = true;
        open.book_focus_ms = now_ms();
        return true;
    }
    if key_is_selection(key)
        && let Some(click) = view.place_last()
    {
        *click_sound = true;
        if let rbv::PageClick::Place(id) = click {
            *ghost = None;
            pending.push(Action::PlaceRecipe { id, max: shift });
            return true;
        }
    }
    false
}

/// `InputWithModifiers.isSelection`: Enter, espaço e o Enter do teclado numérico.
fn key_is_selection(key: &str) -> bool {
    matches!(key, "key.keyboard.enter" | "key.keyboard.space" | "key.keyboard.keypad.enter")
}

/// As teclas que não clicam num slot: só as da barra (`keyHotbarSlots`), a da outra mão (`keySwapOffhand`) e a de largar (`keyDrop`)
/// clicam (`AbstractContainerScreen.keyPressed` e `checkHotbarKeyPressed`), as que o jogador configurou.
fn key_is_book_only(game: &Game<'_>, key: &str) -> bool {
    let keys = &game.settings.keys;
    let hotbar = (1..=9).any(|slot| keys.get(&format!("key.hotbar.{slot}")) == key);
    !(hotbar || keys.get("key.swapOffhand") == key || keys.get("key.drop") == key)
}

/// O slot de receita da máquina sob o cursor (`hoveredSlot` com `isRecipeCraftingSlot`: 0 a entrada e 1 a saída dos
/// `RecipeContainers`). Pergunta ao jogo, que passa pelo filtro das janelas: chame sem segurar a trava do estado.
fn crafting_slot_hovered(game: &Game<'_>, spec: Option<&UpgradeWindowSpec>) -> Option<usize> {
    let spec = spec.filter(|spec| spec.book.is_some())?;
    let renderer = game.renderer.as_ref()?;
    let batch = HudBatch::new([renderer.config.width, renderer.config.height]);
    RAW_SLOT.with(|raw| raw.set(true));
    let slot = game.slot_at(&batch, game.cursor.0, game.cursor.1);
    RAW_SLOT.with(|raw| raw.set(false));
    let slot = slot?;
    spec.book_slots.iter().position(|index| slot == 300 + index)
}

thread_local! {
    /// [`crafting_slot_hovered`] está perguntando: o filtro [`slot_at`] não esconde o slot com fantasma.
    static RAW_SLOT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// `VirtualRecipeBookComponent.slotClicked` de um slot de receita: os fantasmas somem (`clearGhostValues`) e `lastPlacedRecipe` volta
/// a nulo. O `updateStackedContents` que o segue acontece no quadro, quando o inventário muda.
fn recipe_slot_clicked(state: &mut Client) {
    state.ghost = None;
    if let Some(carry) = state.book_carry.as_mut() {
        carry.slot_clicked();
    }
    for open in state.windows.iter_mut() {
        if let Some(view) = open.book.as_mut() {
            view.slot_clicked();
        }
    }
}

/// `MultiPlayerGameMode.handleInventoryButtonClick` e `handlePlaceRecipe`: um botão do menu aberto vai ao servidor
/// (`ServerboundContainerButtonClickPacket`), que o entrega ao dono do bloco (`menu_button`) no turno de quem clicou.
fn send_menu_button(game: &mut Game<'_>, button: i32) {
    let window = game.remote.as_ref().and_then(|remote| remote.menu.as_ref()).map(|(window, _)| *window);
    if let Some(container) = window {
        game.send(horizon_protocol::packets::play::Serverbound::ContainerButtonClick(horizon_protocol::packets::play::ContainerButtonClick { container, button }));
    }
}

/// O caractere com o livro aberto (`RecipeBookComponent.charTyped`): entra na caixa focada.
fn book_char(game: &Game<'_>, open: &mut Open, ch: char) -> bool {
    let Open { book, book_field: field, .. } = open;
    let Some(view) = book.as_mut() else {
        return false;
    };
    if view.ignore_text || !view.focused {
        return false;
    }
    // `EditBox.charTyped`: só um caractere que o chat aceita (`isAllowedChatCharacter`), no lugar da seleção (`insertText`).
    if ch == '\u{a7}' || (ch as u32) < 0x20 || ch == '\u{7f}' {
        return false;
    }
    field.insert_text(&horizon_client::screen_draw::Measure(&game.gui.atlas.font), &ch.to_string());
    view.set_search(&field.value);
    true
}

/// `VirtualRecipeBookComponent.extractRenderState`: o fundo recortado em 7, a caixa de busca, as abas, o botão de filtro e a página
/// (`RecipeBookPage.extractRenderState`: a grade, o `Page n/m` e as setas), na origem (`xo`, `yo`) do livro.
#[allow(clippy::too_many_arguments)]
fn draw_book_contents(
    game: &Game<'_>,
    batch: &mut HudBatch,
    view: &rbv::View,
    field: &horizon_client::screen_draw::TextField,
    focus_ms: i64,
    xo: f32,
    yo: f32,
    hover_ok: bool,
    mouse: (f32, f32),
) {
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
    // a dica `Search...` (cinza e itálico, `SEARCH_HINT_STYLE`). O cursor no meio do texto é a barra de 1 pixel (`insert`), a seleção o
    // realce invertido (`textHighlight`), e o texto que não cabe nos 73 pixels de dentro rola com o cursor (`scrollTo`).
    let mut search = field.clone();
    search.x += xi - BOOK_ORIGIN[0];
    search.y += yi - BOOK_ORIGIN[1];
    search.focused = view.focused;
    let mut draws = Vec::new();
    search.draw(&mut draws, &horizon_client::screen_draw::Measure(&atlas.font), (now_ms() - focus_ms).max(0) as u64, None);
    horizon_client::screen_draw::paint(batch, atlas, &draws);
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
    // `graphics.nextStratum(); overlay.extractRenderState(...)`: o overlay das variantes por cima de tudo da página. As coordenadas dele
    // são as do clique, relativas à janela (a origem do livro fica em `BOOK_ORIGIN`).
    if let Some(overlay) = &view.overlay {
        let (wx, wy) = (xi - BOOK_ORIGIN[0], yi - BOOK_ORIGIN[1]);
        let [x, y, w, h] = overlay.background();
        horizon_client::ui::nine_slice(batch, atlas, "gui/sprites/recipe_book/overlay_recipe", (wx + x) as f32, (wy + y) as f32, w as f32, h as f32, u32::MAX);
        let select = rb::slot_select_index(view.time);
        for button in &overlay.buttons {
            let rect = [wx + button.rect[0], wy + button.rect[1], button.rect[2], button.rect[3]];
            draw_overlay_button(game, batch, view.data(), button, rect, over(rect), select);
        }
    }
}

/// `MekanismOverlayRecipeButton`: o sprite pelo número de entradas da máquina (uma, o do forno; mais, o da bancada, que é o que o
/// original usa enquanto não tem os dele), com o estado de fazível e o de sob o mouse, e cada ingrediente a 0,375 do tamanho na grade
/// do botão: o item com a quantidade, ou o químico em mosaico com a quantidade quando não é 1.
fn draw_overlay_button(game: &Game<'_>, batch: &mut HudBatch, data: &rbv::BookData, button: &rbv::OverlayButton, rect: rb::Rect, hovered: bool, select: i32) {
    let atlas = &game.gui.atlas;
    let Some(ghost) = data.ghost(button.id) else {
        return;
    };
    // `inputs.size()` dos `RecipeContainers`: nas máquinas portadas é o número de ingredientes do display.
    let inputs = ghost.1.len();
    let base = if inputs == 1 { "furnace_overlay" } else { "crafting_overlay" };
    let name = match (button.craftable, hovered) {
        (true, false) => base.to_owned(),
        (false, false) => format!("{base}_disabled"),
        (true, true) => format!("{base}_highlighted"),
        (false, true) => format!("{base}_disabled_highlighted"),
    };
    batch.sprite(atlas, &format!("gui/sprites/recipe_book/{name}"), rect[0] as f32, rect[1] as f32);
    const SCALE: f32 = 0.375;
    for index in 0..inputs {
        let (grid_x, grid_y) = match inputs {
            1 => (1, 1),
            2 => (if index == 0 { 0 } else { 2 }, 1),
            3 => (index as i32, if index == 1 { 0 } else { 2 }),
            _ => ((index % 3) as i32, (index / 3) as i32),
        };
        // `translate(gridPosX + pos.x, gridPosY + pos.y); scale(0.375); translate(-8, -8)`.
        let pivot = [(rect[0] + 2 + 3 + grid_x * 7) as f32, (rect[1] + 2 + 3 + grid_y * 7) as f32];
        let at = [pivot[0] - 8.0, pivot[1] - 8.0];
        let start = batch.vertices.len();
        if let Some((item, count)) = rbv::ghost_shown(ghost.1[index], select) {
            let stack = ItemStack::new(item, count.max(1) as u32);
            // `graphics.item(stack, 0, 0)` e `graphics.itemCount(font, stack, 0, 0, null)`.
            game.draw_item_model(batch, &stack, at[0], at[1]);
            game.draw_item_decorations(batch, &stack, at[0], at[1], None);
        } else if let Some((chemical, amount)) = ghost.3.get(index).and_then(|entries| rbv::ghost_shown(entries, select)) {
            let registry = resource::current();
            if let Some(found) = registry.get(chemical)
                && let Some(icon) = found.icon.as_deref()
            {
                basis::tank_bar_contents(game, batch, icon, [16.0, 16.0], [at[0], at[1], 16.0, 16.0], found.tint);
            }
            if amount != 1 {
                let text = amount.to_string();
                game.native_text(batch, &text, [at[0] + 17.0 - game.native_text_width(&text), at[1] + 9.0], WHITE, true);
            }
        }
        batch.scale_since(start, pivot[0], pivot[1], SCALE, SCALE);
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
        // `GuiRecipeBookWindow.isMouseOver`: a janela ou uma aba do livro, que fica fora dela (`isMouseOverTab`).
        (rel[0] >= 0.0 && rel[1] >= 0.0 && rel[0] < open.window.width as f32 && rel[1] < open.window.height as f32)
            || (open.kind == Kind::RecipeBook && book_tab_at(open, [f64::from(rel[0]), f64::from(rel[1])]).is_some())
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
    // O slot de receita sob o mouse, perguntado ao jogo antes de segurar o estado (a pergunta passa pelo filtro das janelas).
    let ghost_slot_hovered = crafting_slot_hovered(game, Some(spec));
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
            } else if let Some(which) = ghost_slot_hovered
                && let Some((ghost, select)) = shown_ghost_of(spec, &state)
                && let Some((item, count)) = (if which == 0 { ghost.1 } else { ghost.2 }).first().and_then(|entries| rbv::ghost_shown(entries, select))
            {
                // `GuiSlot.renderToolTip` de um slot com fantasma: o tooltip do item que o fantasma mostra agora, no mouse.
                drop(state);
                let item_lines = game.stack_tooltip(&ItemStack::new(item, count.max(1) as u32));
                basis::mouse_tooltip_flat(game, batch, &item_lines, cursor);
                return true;
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
            } else if let Some(view) = open.book.as_ref() {
                let [xo, yo] = BOOK_ORIGIN;
                let inside = |r: rb::Rect| rb::contains(r, rel[0], rel[1]);
                // `VirtualRecipeBookComponent.extractTooltip`: o botão de receita sob o mouse, sem o overlay das variantes, mostra o
                // tooltip do item que ele mostra agora (`Screen.getTooltipFromItem`) e, com mais de uma receita, `Right Click for
                // More` (`RecipeButton.getTooltipText`), no mouse.
                let hovered = view.overlay.is_none().then(|| view.buttons().into_iter().enumerate().find(|(index, _)| inside(rb::button_rect(xo, yo, *index)))).flatten();
                if let Some((_, button)) = hovered {
                    // O estado solto antes de pedir o tooltip do item ao jogo.
                    drop(state);
                    let mut item_lines = game.stack_tooltip(&ItemStack::new(button.item, button.count.max(1) as u32));
                    if button.many {
                        item_lines.push((horizon_client::text::Text::literal(lang::get("gui.recipebook.moreRecipes")), u32::MAX));
                    }
                    basis::mouse_tooltip_flat(game, batch, &item_lines, cursor);
                    return true;
                }
                // Os botões do vanilla dentro do livro têm `Tooltip.create`: o filtro (o nome do filtro do livro quando ligado,
                // `Showing All` desligado) e as setas (`Next Page`, `Previous Page`). As abas não têm tooltip.
                let widget = if inside(rb::filter_rect(xo, yo)) {
                    let key = if view.filtering { view.data().filter_name } else { "gui.recipebook.toggleRecipes.all" };
                    Some((key, rb::filter_rect(xo, yo)))
                } else if view.page.forward_visible() && inside(rb::forward_rect(xo, yo)) {
                    Some(("gui.recipebook.next_page", rb::forward_rect(xo, yo)))
                } else if view.page.back_visible() && inside(rb::back_rect(xo, yo)) {
                    Some(("gui.recipebook.previous_page", rb::back_rect(xo, yo)))
                } else {
                    None
                };
                let Some((key, area)) = widget else {
                    return false;
                };
                lines.push((lang::get(key), None));
                rect = win(area);
                flat = true;
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

/// `TileEntitySecurityDesk.validateAccess`, `closeIfNoAccess` e `BlockSecurityUtils.securityChanged`: quem tem a tela de um tile aberta e deixou de ter acesso a ele
/// é posto para fora (`player.closeContainer()`). O Java fecha na hora da mudança; aqui os pedidos do tick são atendidos no começo da passada seguinte do servidor,
/// antes de as sessões serem recontadas. Só as telas do nível em que o jogo está são conferidas.
pub(in crate::game) fn security_revalidate(game: &mut Game<'_>) {
    let requests = crate::game::machinery_basis::security_take_revalidations();
    if requests.is_empty() {
        return;
    }
    let level = game.native_dimension();
    let open: Vec<([i32; 3], u128)> = {
        let guard = sessions();
        guard.as_ref().map_or_else(Vec::new, |table| {
            table.containers().into_iter().filter(|(_, at)| at == level).flat_map(|(pos, _)| table.players_of(pos).into_iter().map(move |player| (pos, player))).collect()
        })
    };
    for (pos, player) in open {
        // O anfitrião é o 0 das sessões.
        let uuid = if player == 0 { crate::game::machinery_basis::host_uuid(game) } else { Some(player) };
        let Some(uuid) = uuid else {
            continue;
        };
        if !requests.iter().any(|request| request.is_none_or(|only| only == uuid)) {
            continue;
        }
        if crate::game::machinery_basis::security_can_access(game, pos, &|_| true, uuid) {
            continue;
        }
        if player == 0 {
            game.close_container();
        } else if let Some(host) = game.lan.as_mut()
            && let Some(conn) = host.hub.guests.iter().find(|guest| guest.uuid == player).map(|guest| guest.conn)
        {
            host.hub.close_container(conn);
        }
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
    BOOK_RESTORED.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).remove(&player);
    game.mb_muffling_player_in(player);
    game.mb_view_player_in(player);
    game.mb_tank_view_player_in(player);
}

/// `native_server_data_loaded`: os dados mudaram (um `/reload`), então todos os que estão no servidor recebem de novo.
pub(in crate::game) fn data_reloaded(game: &mut Game<'_>) {
    // Um mundo que abre e um `/reload` (`PlayerList.reloadResources`): todos recebem o livro inteiro de novo.
    book_sent(HashMap::clear);
    *BOOK_RESTORED.lock().unwrap_or_else(|e| e.into_inner()) = None;
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
        || crate::game::machinery_tile_sound::tank_view_from_server(channel, data)
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

// ---------------------------------------------------------------------------------------------------------------------------
// A tela da Security Desk (`GuiSecurityDesk`): os elementos, o mouse, o teclado e os pacotes. A lógica é da lib (`security_desk_screen`); aqui ficam o `EditBox` da
// caixa de nome (o `TextField` do Horizon), o desenho e a ligação com o jogo. A tela é de quem registra o bloco (`mb_desk_register`).

static DESKS: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

fn is_desk_screen(block: &str) -> bool {
    DESKS.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|desk| *desk == block)
}

/// O que a tela pede e só o quadro pode fazer (o `&mut Game`).
enum DeskPending {
    Menu(i32),
    AddTrusted(String),
    /// Ctrl+C (0), Ctrl+V (1) e Ctrl+X (2) na caixa de nome.
    Shortcut(u8),
}

struct DeskUi {
    pos: Option<[i32; 3]>,
    screen: dsk::DeskScreen,
    /// O `EditBox` de dentro do `GuiTextField`: sem borda, até 16 caracteres (`SharedConstants.MAX_PLAYER_NAME_LENGTH`).
    field: horizon_client::screen_draw::TextField,
    /// `EditBox.focusedTime`.
    focus_ms: i64,
    /// `GuiMekanism.lastMSInitialized`: de quando os textos que não cabem contam a rolagem.
    opened_ms: i64,
    /// O último `containerTick` (a cada 50 ms).
    tick_ms: i64,
    pending: Vec<DeskPending>,
    left_down: bool,
}

/// `MekanismLang.SECURITY_OVERRIDE`, `OnOff`, e os sprites.
const DESK_ON_KEY: &str = "boolean.mekanism.on";
const DESK_OFF_KEY: &str = "boolean.mekanism.off";
const DESK_TITLE_KEY_PREFIX: &str = "container.";
/// `CommonColors.LIGHT_GRAY`, o texto de um botão desligado (`TEXT_INACTIVE_BUTTON`).
const INACTIVE_BUTTON_TEXT: u32 = 0xFF_A0_A0_A0;
/// `Tooltip.create`: a largura em que o texto de um tooltip quebra.
const DESK_TOOLTIP_WRAP: i32 = 170;
/// O `containerTick` do cliente: 20 por segundo.
const DESK_TICK_MS: i64 = 50;

/// O que a tela lê do tile do cliente: o dono e a frequência que o menu trouxe ao abrir (posição 0 de `TileTrackers { security }`), e quem olha.
struct DeskData {
    owner: Option<(u128, String)>,
    frequency: Option<machinery_basis_core::security_frequency::SecurityFrequency>,
    viewer: Option<u128>,
}

impl DeskData {
    fn read(game: &Game<'_>, block: &str) -> Self {
        let mirror = game.mb_mirror(block);
        let frequency = basis::security_menu_frequency(&mirror, 0);
        let viewer = basis::viewer_uuid(game);
        // `tile.getOwnerName()` no cliente: o nome que o cliente sabe do dono (`OwnerDisplay.getOwnerName`), como na aba de segurança.
        let owner = machinery_basis_core::security::SecurityView::read(&mirror, 0, 1).owner.map(|(owner, _)| (owner, basis::owner_name(game, owner, viewer)));
        Self { owner, frequency, viewer }
    }

    fn view(&self) -> machinery_basis_core::security_desk::DeskView<'_> {
        machinery_basis_core::security_desk::DeskView { owner: self.owner.as_ref().map(|(owner, _)| *owner), frequency: self.frequency.as_ref(), viewer: self.viewer }
    }
}

fn new_desk_field() -> horizon_client::screen_draw::TextField {
    let [x, y, width, height] = dsk::EDIT_BOX;
    let mut field = horizon_client::screen_draw::TextField::new(x, y, width, height);
    field.bordered = false;
    field.max_length = machinery_basis_core::security_desk::MAX_NAME_LENGTH;
    field
}

/// O bloco da tela aberta, se é uma mesa.
fn open_desk(game: &Game<'_>) -> Option<&'static str> {
    let spec = open_spec(game)?;
    is_desk_screen(spec.block).then_some(spec.block)
}

fn desk_apply(ui: &mut DeskUi, game: &Game<'_>, outputs: Vec<dsk::DeskOutput>, click_sound: &mut bool, shift: bool) {
    let m = horizon_client::screen_draw::Measure(&game.gui.atlas.font);
    for output in outputs {
        match output {
            dsk::DeskOutput::Menu(button) => ui.pending.push(DeskPending::Menu(button)),
            dsk::DeskOutput::AddTrusted(name) => ui.pending.push(DeskPending::AddTrusted(name)),
            dsk::DeskOutput::ClearField => ui.field.set_value(&m, ""),
            dsk::DeskOutput::ClickSound => *click_sound = true,
            dsk::DeskOutput::FieldCursor(x) => ui.field.click(&m, x, shift),
            dsk::DeskOutput::FieldFocused => ui.focus_ms = now_ms(),
        }
    }
}

/// O quadro: a tela nasce com o menu, o `containerTick` roda o `updateButtons`, e o que o clique pediu vai ao servidor.
fn desk_frame(game: &mut Game<'_>) {
    let Some(block) = open_desk(game) else {
        client().desk = None;
        return;
    };
    let pos = game.mb_open_screen_pos();
    let data = DeskData::read(game, block);
    let now = now_ms();
    let pending = {
        let mut guard = client();
        if guard.desk.as_ref().is_none_or(|ui| ui.pos != pos) {
            guard.desk = Some(DeskUi {
                pos,
                screen: dsk::DeskScreen::new(&data.view()),
                field: new_desk_field(),
                focus_ms: now,
                opened_ms: now,
                tick_ms: now,
                pending: Vec::new(),
                left_down: false,
            });
        }
        let Some(ui) = guard.desk.as_mut() else {
            return;
        };
        if now - ui.tick_ms >= DESK_TICK_MS {
            ui.tick_ms = now;
            ui.screen.update_buttons(&data.view());
        }
        std::mem::take(&mut ui.pending)
    };
    for action in pending {
        match action {
            DeskPending::Menu(button) => send_menu_button(game, button),
            DeskPending::AddTrusted(name) => {
                if let Some(pos) = pos {
                    basis::security_send_trusted(game, pos, &name);
                }
            }
            DeskPending::Shortcut(which) => {
                use horizon_client::clipboard::Clipboard as _;
                let m = horizon_client::screen_draw::Measure(&game.gui.atlas.font);
                let mut guard = client();
                let Some(ui) = guard.desk.as_mut() else {
                    continue;
                };
                match which {
                    0 => {
                        ui.field.shortcut(&m, horizon_client::clipboard::EditShortcut::Copy, &mut game.clipboard);
                    }
                    1 => {
                        // `GuiTextField.keyPressed` com `isPaste`: um caractere que o `InputValidator` recusa barra o texto inteiro.
                        let text = game.clipboard.get();
                        if dsk::DeskScreen::paste_accepted(&text) {
                            ui.field.insert_text(&m, &text);
                        }
                    }
                    _ => {
                        ui.field.shortcut(&m, horizon_client::clipboard::EditShortcut::Cut, &mut game.clipboard);
                    }
                }
                let DeskUi { screen, field, .. } = ui;
                screen.text_changed(&field.value);
            }
        }
    }
}

/// O clique: devolve se a tela da mesa está aberta (o clique não segue para as janelas) e põe `canceled` quando um elemento ficou com ele.
fn desk_press(game: &Game<'_>, view: &NativeScreenView<'_>, button: i32, modifiers: u32, canceled: &mut bool) -> bool {
    let Some(block) = open_desk(game) else {
        return false;
    };
    let data = DeskData::read(game, block);
    let mouse = mouse_of(view);
    let rel = [mouse[0] - f64::from(view.rect[0]), mouse[1] - f64::from(view.rect[1])];
    let mut guard = client();
    let state = &mut *guard;
    let Some(ui) = state.desk.as_mut() else {
        return true;
    };
    // `MouseHandler.onButton`: o duplo clique é o segundo do mesmo botão em menos de 250 ms.
    let now = now_ms();
    let double = state.last_click.is_some_and(|(at, last)| now - at < 250 && last == button);
    state.last_click = Some((now, button));
    ui.left_down = button == 0;
    let text = ui.field.value.clone();
    let (taken, outputs) = ui.screen.click(&data.view(), rel, button, &text);
    let on_box = outputs.iter().any(|output| matches!(output, dsk::DeskOutput::FieldCursor(_)));
    desk_apply(ui, game, outputs, &mut state.click_sound, modifiers & 1 != 0);
    if on_box && double {
        // `EditBox.onClick` com o duplo clique: a palavra sob o mouse.
        let m = horizon_client::screen_draw::Measure(&game.gui.atlas.font);
        book_select_word(&mut ui.field, &m, rel[0]);
    }
    let DeskUi { screen, field, .. } = ui;
    screen.text_changed(&field.value);
    if taken {
        *canceled = true;
    }
    true
}

fn desk_release(game: &Game<'_>) -> bool {
    if open_desk(game).is_none() {
        return false;
    }
    if let Some(ui) = client().desk.as_mut() {
        ui.left_down = false;
        ui.screen.release();
    }
    true
}

fn desk_drag(game: &Game<'_>, view: &NativeScreenView<'_>, canceled: &mut bool) -> bool {
    if open_desk(game).is_none() {
        return false;
    }
    let mouse = mouse_of(view);
    let rel = [mouse[0] - f64::from(view.rect[0]), mouse[1] - f64::from(view.rect[1])];
    if let Some(ui) = client().desk.as_mut()
        && ui.left_down
        && ui.screen.list.dragging()
    {
        ui.screen.drag(rel);
        *canceled = true;
    }
    true
}

fn desk_scroll(game: &Game<'_>, view: &NativeScreenView<'_>, scroll: [f64; 2], canceled: &mut bool) -> bool {
    if open_desk(game).is_none() {
        return false;
    }
    let mouse = mouse_of(view);
    let rel = [mouse[0] - f64::from(view.rect[0]), mouse[1] - f64::from(view.rect[1])];
    if let Some(ui) = client().desk.as_mut()
        && ui.screen.scroll(rel, scroll[1])
    {
        *canceled = true;
    }
    true
}

fn desk_key(game: &Game<'_>, key: &str, canceled: &mut bool) -> bool {
    let Some(block) = open_desk(game) else {
        return false;
    };
    let data = DeskData::read(game, block);
    let mut guard = client();
    let state = &mut *guard;
    let Some(ui) = state.desk.as_mut() else {
        return true;
    };
    let desk_key = match key {
        "key.keyboard.enter" | "key.keyboard.keypad.enter" => dsk::DeskKey::Enter,
        "key.keyboard.space" => dsk::DeskKey::Space,
        "key.keyboard.escape" => dsk::DeskKey::Escape,
        "key.keyboard.tab" => dsk::DeskKey::Tab,
        _ => dsk::DeskKey::Other,
    };
    let text = ui.field.value.clone();
    let (result, outputs) = ui.screen.key(&data.view(), desk_key, &text);
    desk_apply(ui, game, outputs, &mut state.click_sound, false);
    if result == dsk::KeyResult::Field {
        // `EditBox.keyPressed`, como a caixa de busca do livro: apagar, mover o cursor, selecionar tudo e a área de transferência.
        use winit::keyboard::KeyCode as K;
        let m = horizon_client::screen_draw::Measure(&game.gui.atlas.font);
        let shift = game.has_shift_down();
        let ctrl = game.has_control_down_with_quirk();
        let alt = game.modifiers.alt_key();
        let code = match key {
            "key.keyboard.backspace" => Some(K::Backspace),
            "key.keyboard.delete" => Some(K::Delete),
            "key.keyboard.home" => Some(K::Home),
            "key.keyboard.end" => Some(K::End),
            "key.keyboard.left" => Some(K::ArrowLeft),
            "key.keyboard.right" => Some(K::ArrowRight),
            "key.keyboard.a" => Some(K::KeyA),
            _ => None,
        };
        if let Some(code) = code.filter(|code| *code != K::KeyA || !alt) {
            ui.field.key(&m, code, shift, ctrl);
        } else if ctrl && !shift && !alt {
            match key {
                "key.keyboard.c" => ui.pending.push(DeskPending::Shortcut(0)),
                "key.keyboard.v" => ui.pending.push(DeskPending::Shortcut(1)),
                "key.keyboard.x" => ui.pending.push(DeskPending::Shortcut(2)),
                _ => {}
            }
        }
    }
    let DeskUi { screen, field, .. } = ui;
    screen.text_changed(&field.value);
    if result != dsk::KeyResult::Pass {
        *canceled = true;
    }
    true
}

fn desk_char(game: &Game<'_>, ch: char, canceled: &mut bool) -> bool {
    if open_desk(game).is_none() {
        return false;
    }
    let mut guard = client();
    let Some(ui) = guard.desk.as_mut() else {
        return true;
    };
    if ui.screen.field_focused() {
        // `GuiTextField.charTyped`: a caixa focada fica com o caractere se o validador o aceita; o `EditBox` ainda recusa o que o chat não aceita.
        if ui.screen.char_typed(ch) {
            let m = horizon_client::screen_draw::Measure(&game.gui.atlas.font);
            ui.field.insert_text(&m, &ch.to_string());
            let DeskUi { screen, field, .. } = ui;
            screen.text_changed(&field.value);
        }
        *canceled = true;
    }
    true
}

#[derive(Clone, Copy, PartialEq)]
enum Align {
    Left,
    Center,
    Right,
}

/// `IFancyFontRenderer.drawScrollingString` de um componente: o texto na área `[min_x, min_y, max_x, max_y]`, alinhado, ou rolando se não cabe.
#[allow(clippy::too_many_arguments)]
fn desk_text(game: &Game<'_>, batch: &mut HudBatch, text: &horizon_client::text::Text, area: [f32; 4], align: Align, argb: u32, shadow: bool, since: i64) {
    let [min_x, min_y, max_x, max_y] = area;
    let atlas = &game.gui.atlas;
    let width = atlas.font.width_text(text);
    let area_width = max_x - min_x;
    let scrolls = width > area_width;
    let y = (min_y + max_y - LINE_HEIGHT) / 2.0;
    let x = if scrolls {
        min_x - uw::scroll_overflow(f64::from(width - area_width), now_ms() - since) as f32
    } else {
        match align {
            Align::Left => min_x,
            Align::Center => min_x + (area_width - width) / 2.0,
            Align::Right => max_x - width,
        }
    };
    let start = batch.vertices.len();
    batch.text_styled(atlas, text, x, y, basis::hud_color(argb), shadow);
    if scrolls {
        batch.clip_since(start, min_x, min_y, max_x, max_y);
    }
}

/// A área de um `drawScrollingString(text, x, y, alinhamento, cor, width, maxLengthPad, ...)` com a altura de uma linha.
fn desk_line(origin: [f32; 2], [x, y, width, pad]: [i32; 4]) -> [f32; 4] {
    [origin[0] + (x + pad) as f32, origin[1] + y as f32, origin[0] + (x + width - pad) as f32, origin[1] + y as f32 + LINE_HEIGHT]
}

fn desk_literal(text: &str) -> horizon_client::text::Text {
    horizon_client::text::Text::literal(text)
}

/// `GuiSecurityDesk`: o fundo de cada elemento na ordem dos `addRenderableWidget`, e depois os textos (`renderForeground` e `drawForegroundText`).
pub(in crate::game) fn desk_draw(game: &Game<'_>, batch: &mut HudBatch, block: &str, origin: [f32; 2], panel: &basis::PanelStyle) {
    use horizon_client::text::{Style, Text};
    let Some(spec) = spec_of(block) else {
        return;
    };
    let data = DeskData::read(game, block);
    let view = data.view();
    let guard = client();
    let fresh;
    let (screen, field, focus_ms, opened_ms) = match guard.desk.as_ref() {
        Some(ui) => (&ui.screen, ui.field.clone(), ui.focus_ms, ui.opened_ms),
        // O primeiro quadro, antes de o estado nascer: a tela como `addGuiElements` a monta.
        None => {
            fresh = dsk::DeskScreen::new(&view);
            (&fresh, new_desk_field(), now_ms(), now_ms())
        }
    };
    let [ox, oy] = origin;
    let at = |rect: [i32; 4]| shifted(rect, ox, oy);
    let cursor = game.native_cursor();
    let mouse = [f64::from(cursor[0] - ox), f64::from(cursor[1] - oy)];
    let hovered = screen.hovered(mouse);
    let active = screen.active();
    // O painel e os três suportes.
    basis::nine_slice(game, batch, panel.base, panel.base_slice, [ox, oy, dsk::IMAGE_WIDTH as f32, dsk::IMAGE_HEIGHT as f32]);
    for holder in dsk::HOLDERS {
        nine(game, batch, spec, "element_holder", uw::ELEMENT_HOLDER_SLICE, at(holder), WHITE);
    }
    // `addSlots` (`dynamicSlots`): os dois slots do tile e os 36 do jogador, e por cima dos do tile os dois `INNER_HOLDER_SLOT`.
    let slot = machinery_basis_core::gui::SLOT_SIZE;
    for [x, y] in dsk::INNER_SLOTS {
        game.native_sprite(batch, panel.slot, [0.0, 0.0, slot, slot], [ox + x as f32, oy + y as f32, slot, slot]);
    }
    for row in 0..4 {
        let y = dsk::INVENTORY_Y - 1 + row * 18 + if row == 3 { 4 } else { 0 };
        for column in 0..9 {
            game.native_sprite(batch, panel.slot, [0.0, 0.0, slot, slot], [ox + (7 + column * 18) as f32, oy + y as f32, slot, slot]);
        }
    }
    for [x, y] in dsk::INNER_SLOTS {
        plain(game, batch, spec, "slot/inner_holder_slot", [18.0, 18.0], [ox + x as f32, oy + y as f32, 18.0, 18.0]);
    }
    // `GuiSecurityLight`: a tela interna de 8 por 8 e a luz de 6 por 6.
    nine(game, batch, spec, "inner_screen", uw::INNER_SCREEN_SLICE, at(dsk::LIGHT), WHITE);
    let light = match view.light() {
        machinery_basis_core::security_desk::SecurityLight::Enabled => "light/security/enabled",
        machinery_basis_core::security_desk::SecurityLight::Disabled => "light/security/disabled",
        machinery_basis_core::security_desk::SecurityLight::NotApplicable => "light/security/not_applicable",
    };
    let [lx, ly, lw, lh] = dsk::LIGHT;
    plain(game, batch, spec, light, [6.0, 6.0], at([lx + 1, ly + 1, lw - 2, lh - 2]));
    plain(game, batch, spec, "security/public", [18.0, 18.0], at(dsk::PUBLIC_ICON));
    plain(game, batch, spec, "security/private", [18.0, 18.0], at(dsk::PRIVATE_ICON));
    // `GuiTextScrollList`: a tela interna, a moldura da barra, o polegar e a linha escolhida.
    let list = &screen.list;
    nine(game, batch, spec, "inner_screen", uw::INNER_SCREEN_SLICE, at(dsk::LIST), WHITE);
    let bar_x = list.x + list.bar_x_shift();
    nine(game, batch, spec, "scroll/list_border", uw::LIST_BORDER_SLICE, at([bar_x - 1, list.y + 1, 6, list.height - 2]), WHITE);
    plain(game, batch, spec, "scroll/list_bar", [4.0, 4.0], at(list.thumb_rect()));
    if let Some(row) = screen.selected_row() {
        let rect = [list.x + 1, list.y + 1 + row as i32 * list.element_height(), list.bar_x_shift() - 2, list.element_height()];
        plain(game, batch, spec, "scroll/list_selection", [2.0, 2.0], at(rect));
    }
    // O botão de tirar.
    button_background(game, batch, spec, at(dsk::REMOVE), active.remove, hovered == Some(dsk::DeskWidget::Remove));
    // A caixa de nome: a tela interna um pixel maior, o `EditBox` e o botão de confirmar.
    let [fx, fy, fw, fh] = dsk::FIELD;
    nine(game, batch, spec, "inner_screen", uw::INNER_SCREEN_SLICE, at([fx - 1, fy - 1, fw + 2, fh + 2]), WHITE);
    let mut drawn = field;
    drawn.x += ox as i32;
    drawn.y += oy as i32;
    drawn.focused = screen.field_focused();
    let mut draws = Vec::new();
    drawn.draw(&mut draws, &horizon_client::screen_draw::Measure(&game.gui.atlas.font), (now_ms() - focus_ms).max(0) as u64, None);
    horizon_client::screen_draw::paint(batch, &game.gui.atlas, &draws);
    button_background(game, batch, spec, at(dsk::CHECKMARK), active.checkmark, screen.checkmark_hovered(mouse));
    plain(game, batch, spec, "button/checkmark", [12.0, 12.0], at(dsk::CHECKMARK));
    // Os três botões de modo e o de sobreposição.
    for (index, name) in ["security/button_public", "security/button_private", "security/button_trusted"].into_iter().enumerate() {
        let mode = machinery_basis_core::security::SecurityMode::by_id(index as i32);
        button_background(game, batch, spec, at(dsk::MODE_BUTTONS[index]), active.modes[index], hovered == Some(dsk::DeskWidget::Mode(mode)));
        plain(game, batch, spec, name, [40.0, 16.0], at(dsk::MODE_BUTTONS[index]));
    }
    button_background(game, batch, spec, at(dsk::OVERRIDE), active.override_button, hovered == Some(dsk::DeskWidget::Override));
    plain(game, batch, spec, "button/exclamation", [16.0, 16.0], at(dsk::OVERRIDE));
    // Os textos dos elementos: os nomes da lista (0,8 da altura, na cor da tela) e o do botão de tirar (com sombra).
    for (row, _, name) in screen.visible_rows() {
        let y = oy + (list.y + 2 + list.element_height() * row as i32) as f32;
        let area = [ox + (list.x + 2) as f32, y, ox + (list.x + list.bar_x_shift() - 2) as f32, y + LINE_HEIGHT];
        scrolling_text(game, batch, name, area, false, SCREEN_COLOR, 0.8, opened_ms);
    }
    let [rx, ry, rw, rh] = dsk::REMOVE;
    let remove_area = [ox + (rx + 2) as f32, oy + ry as f32, ox + (rx + rw - 2) as f32, oy + (ry + rh + 1) as f32];
    let remove_color = if active.remove { WHITE } else { INACTIVE_BUTTON_TEXT };
    desk_text(game, batch, &desk_literal(&lang::get(machinery_basis_core::security_desk::BUTTON_REMOVE_KEY)), remove_area, Align::Center, remove_color, true, opened_ms);
    // `drawForegroundText`: o título, "Inventory" e o dono, "Trusted Players", o modo e "Add:".
    let title = basis::screen_title(game, &format!("{DESK_TITLE_KEY_PREFIX}{}", block.replace(':', ".")));
    desk_text(game, batch, &title, desk_line(origin, [0, dsk::TITLE_LABEL_Y, dsk::IMAGE_WIDTH, 4]), Align::Center, TITLE_COLOR, false, opened_ms);
    let inventory_y = dsk::IMAGE_HEIGHT - 94;
    desk_text(game, batch, &desk_literal(&lang::get("container.inventory")), desk_line(origin, [8, inventory_y, dsk::INVENTORY_TEXT_WIDTH, 0]), Align::Left, TITLE_COLOR, false, opened_ms);
    // `OwnerDisplay.of(uuid, nome).getTextComponent()`: sem jogador, o texto inteiro em `DARK_GRAY`; sem dono, `No Owner` em vermelho.
    let owner = match &data.owner {
        Some((_, name)) => Text::translate("owner.mekanism.format", &[desk_literal(name)]).with_style(&Style::color(machinery_basis_core::security::COLOR_DARK_GRAY)),
        None => Text::translate("owner.mekanism.none", &[]).with_style(&Style::color(machinery_basis_core::security::COLOR_RED)),
    };
    let owner_line = [dsk::OWNER_TEXT_START, inventory_y, dsk::IMAGE_WIDTH - dsk::OWNER_TEXT_START, dsk::OWNER_TEXT_PAD];
    desk_text(game, batch, &owner, desk_line(origin, owner_line), Align::Right, TITLE_COLOR, false, opened_ms);
    let trusted = desk_literal(&lang::get(machinery_basis_core::security_desk::TRUSTED_PLAYERS_KEY));
    desk_text(game, batch, &trusted, desk_line(origin, dsk::TRUSTED_PLAYERS_TEXT), Align::Center, SUBHEADING_COLOR, false, opened_ms);
    let mode = match view.mode_text() {
        Some(mode) => {
            let line = machinery_basis_core::security::security_line(mode);
            let arg = match &line.arg {
                machinery_basis_core::security::Arg::Literal(text) => Text::literal(text.as_str()),
                machinery_basis_core::security::Arg::Translate(key) => Text::translate(key, &[]),
            }
            .with_style(&Style::color(line.arg_color));
            Text::translate(line.key, &[arg])
        }
        None => Text::translate(machinery_basis_core::security_desk::SECURITY_OFFLINE_KEY, &[]).with_style(&Style::color(machinery_basis_core::security::COLOR_RED)),
    };
    desk_text(game, batch, &mode, desk_line(origin, dsk::MODE_TEXT), Align::Left, TITLE_COLOR, false, opened_ms);
    let add = desk_literal(&lang::get(machinery_basis_core::security_desk::SECURITY_ADD_KEY));
    desk_text(game, batch, &add, desk_line(origin, dsk::ADD_TEXT), Align::Right, TITLE_COLOR, false, opened_ms);
}

/// Os tooltips da tela: os três botões de modo (`setTooltip`) e o de sobreposição (`TooltipToggleButton`), só ligados (`findElementOver` pede `isMouseOver`), sobre
/// o botão (`MenuTooltipPositioner`). Devolve se desenhou.
pub(in crate::game) fn desk_tooltip(game: &Game<'_>, batch: &mut HudBatch, block: &str, at: [f32; 2], cursor: (f32, f32)) -> bool {
    if !is_desk_screen(block) {
        return false;
    }
    let data = DeskData::read(game, block);
    let guard = client();
    let Some(ui) = guard.desk.as_ref() else {
        return false;
    };
    let (text, rect) = match ui.screen.hovered([f64::from(at[0]), f64::from(at[1])]) {
        Some(dsk::DeskWidget::Mode(mode)) => (lang::get(machinery_basis_core::security_desk::MODE_TOOLTIP_KEYS[mode.ordinal()]), dsk::MODE_BUTTONS[mode.ordinal()]),
        Some(dsk::DeskWidget::Override) => {
            let state = lang::get(if data.view().override_on() { DESK_ON_KEY } else { DESK_OFF_KEY });
            (lang::get(machinery_basis_core::security_desk::SECURITY_OVERRIDE_KEY).replace("%1$s", &state), dsk::OVERRIDE)
        }
        _ => return false,
    };
    let lines: Vec<(String, Option<u32>)> =
        uw::wrap_lines(&text, DESK_TOOLTIP_WRAP, &|piece: &str| game.native_text_width(piece)).into_iter().map(|line| (line, None)).collect();
    let widget = [cursor.0 - at[0] + rect[0] as f32, cursor.1 - at[1] + rect[1] as f32, rect[2] as f32, rect[3] as f32];
    basis::menu_tooltip(game, batch, &lines, cursor, widget);
    true
}

impl Game<'_> {
    /// O mod diz que a tela do bloco é a da Security Desk (`GuiSecurityDesk`); o bloco também registra a janela dele (`mb_window_register`), que dá o menu.
    pub(in crate::game) fn mb_desk_register(block: &'static str) {
        let mut desks = DESKS.lock().unwrap_or_else(|e| e.into_inner());
        if !desks.contains(&block) {
            desks.push(block);
        }
    }

    /// A tela da mesa, desenhada inteira (o painel, os elementos e os textos).
    pub(in crate::game) fn mb_desk_draw(&self, batch: &mut HudBatch, block: &str, origin: [f32; 2], panel: &basis::PanelStyle) {
        desk_draw(self, batch, block, origin, panel);
    }

    /// Os tooltips dos botões da mesa; devolve se mostrou algum.
    pub(in crate::game) fn mb_desk_tooltip(&self, batch: &mut HudBatch, block: &str, at: [f32; 2], cursor: (f32, f32)) -> bool {
        desk_tooltip(self, batch, block, at, cursor)
    }

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

    /// O `/recipe take` de receitas de mod: saem também das listas lidas do jogador que age.
    pub(in crate::game) fn mb_book_forget_saved(&mut self, names: &[&str]) {
        book_forget_saved(self, names);
    }

    /// O `/recipe give` com várias receitas: o que entrar vai ao cliente de quem age em ordem de id.
    pub(in crate::game) fn mb_book_ordered_batch(&mut self) {
        book_ordered_batch(self);
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

    /// O fantasma do slot de receita com o item em `at` (veja [`ghost_slot`]).
    pub(in crate::game) fn mb_recipe_ghost_slot(&self, batch: &mut HudBatch, at: [f32; 2], inputs: usize, output: bool, index: usize) -> bool {
        ghost_slot(self, batch, at, inputs, output, index)
    }

    /// O fantasma da barra de químico que é a entrada `index` da tela (veja [`ghost_chemical`]).
    pub(in crate::game) fn mb_recipe_ghost_chemical(&self, inputs: usize, index: usize) -> Option<(&'static str, f64)> {
        ghost_chemical(self, inputs, index)
    }

    /// O botão de menu da colocação de uma receita pelo livro (veja [`recipe_place_button`]).
    pub(in crate::game) fn mb_recipe_place_button(
        &mut self,
        entity: &mut horizon_client::containers::ModBlockEntity,
        id: i32,
        tank: Option<(String, i64)>,
        valid: &dyn Fn(&Game<'_>, &ItemStack) -> bool,
        can_insert: &dyn Fn(&Game<'_>, &ItemStack) -> bool,
    ) -> bool {
        recipe_place_button(self, entity, id, tank, valid, can_insert)
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

    /// Um texto de um elemento da tela do bloco (`GuiElement.drawScaledScrollingString` com `getTimeOpened()`): alinhado na área `[min_x, min_y, max_x, max_y]`, em
    /// pixels de GUI da janela, ou rolando desde que a tela abriu se não cabe.
    pub(in crate::game) fn mb_screen_text(&self, batch: &mut HudBatch, text: &str, area: [f32; 4], alignment: machinery_basis_core::gui::TextAlignment, color: u32, scale: f32) {
        let since = client().screen_opened_ms;
        aligned_scrolling_text(self, batch, text, area, alignment, color, scale, since);
    }

    /// O título de uma tela de bloco (`GuiMekanism.renderTitleText`, `IFancyFontRenderer.drawTitleText`): o componente inteiro, com o estilo dele, centrado na
    /// área do título ou, se não cabe, rolando desde que a tela abriu e cortado nela. `origin` é o canto da tela e `color` a cor de quem não tem estilo.
    pub(in crate::game) fn mb_screen_title(&self, batch: &mut HudBatch, title: &horizon_client::text::Text, origin: [f32; 2], image_width: f32, color: u32) {
        let [min_x, min_y, max_x, max_y] = machinery_basis_core::gui::title_area(image_width);
        let atlas = &self.gui.atlas;
        let since = client().screen_opened_ms;
        let (x, scrolls) =
            machinery_basis_core::gui::scrolling_text_x(min_x, max_x, atlas.font.width_text(title), machinery_basis_core::gui::TextAlignment::Center, now_ms() - since);
        let start = batch.vertices.len();
        batch.text_styled(atlas, title, origin[0] + x, origin[1] + (min_y + max_y - LINE_HEIGHT) / 2.0, color, false);
        if scrolls {
            batch.clip_since(start, origin[0] + min_x, origin[1] + min_y, origin[0] + max_x, origin[1] + max_y);
        }
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
    pub(in crate::game) fn mb_security_revalidate(&mut self) {
        security_revalidate(self);
    }

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
