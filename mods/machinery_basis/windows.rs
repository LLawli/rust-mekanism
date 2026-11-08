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
use horizon_world::nbt::{JavaString, Tag};
use machinery_basis_core::container_sync::{Mirror, Property, TrackedSource, wire};
use machinery_basis_core::gui::NineSlice;
use machinery_basis_core::machine_sync::read_upgrade_map;
use machinery_basis_core::resource;
use machinery_basis_core::scroll_list::ScrollList;
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
        .find(|(spec, _)| spec.block == block && spec.item == item)
        .map(|(spec, owner)| (*spec, owner.add))
}

/// Se `item` é o item de upgrade de algum bloco registrado: a pergunta barata, antes de olhar o bloco.
fn is_upgrade_item(item: &str) -> bool {
    let specs = SPECS.lock().unwrap_or_else(|e| e.into_inner());
    specs.iter().any(|(spec, _)| spec.item == item)
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
const SCREEN_COLOR: u32 = 0xFF_3C_FE_9A;
const FADE: u32 = 0x80_78_78_78;
const SLOT_HOVER: u32 = 0x80_FF_FF_FF;
const YELLOW: u32 = 0xFF_DD_4F;
const RED: u32 = 0xFF_38_3C;
const SHADOW_TINT: u32 = 0xBF_FF_FF_FF;
const DIGITAL_HOVER: u32 = 0xFF_3C_FE_9A;
const DIGITAL_ACTIVE: u32 = 0xFF_30_CB_7B;
const DIGITAL_INACTIVE: u32 = 0xFF_24_98_5C;
const LINE_HEIGHT: f32 = 9.0;

// ---------------------------------------------------------------------------------------------------------------------------
// Estado do cliente. É um `Mutex` e não um campo do jogo porque os hooks de entrada só têm `&Game`.

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Open,
    Close,
    Remove { index: u16, all: bool },
}

struct Open {
    window: Window,
    list: ScrollList,
    /// O tipo escolhido na lista, por índice do registro.
    selected: Option<usize>,
    opened_ms: i64,
    selected_ms: i64,
}

struct Client {
    /// O bloco da tela a que o estado pertence; trocar de tela começa de novo.
    screen_block: Option<String>,
    /// Onde esse bloco fica: o espelho do menu dele mora ali.
    screen_pos: Option<[i32; 3]>,
    positions: Option<WindowPositions>,
    window: Option<Open>,
    pending: Vec<Action>,
    /// A seleção que o servidor já sabe, para só mandar a mudança.
    sent_selected: bool,
}

static CLIENT: Mutex<Client> = Mutex::new(Client {
    screen_block: None,
    screen_pos: None,
    positions: None,
    window: None,
    pending: Vec::new(),
    sent_selected: false,
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

/// A posição guardada no cliente, como o `client.toml` do Mekanism: `config/<ns>/client.toml`.
fn positions_path(spec: &UpgradeWindowSpec) -> Option<std::path::PathBuf> {
    Some(Game::native_config_dir().ok()?.join(spec.ns).join("client.toml"))
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
        let _ = std::fs::write(path, positions.to_toml());
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

fn selection(spec: &UpgradeWindowSpec) -> SelectedWindow {
    SelectedWindow::new(spec.window_type, 0)
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

/// `GuiWindow.close`: tira a janela, grava onde ela estava e avisa o servidor.
fn close_window(game: &mut Game<'_>, spec: &UpgradeWindowSpec, tell_server: bool) {
    let pos = {
        let mut guard = client();
        let state = &mut *guard;
        let Some(open) = state.window.take() else {
            return;
        };
        let positions = state.positions.get_or_insert_with(|| load_positions(spec));
        if open.window.close(positions) {
            save_positions(spec, positions);
        }
        state.sent_selected = false;
        state.screen_pos
    };
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
        let mut body = vec![OP_SELECT];
        body.extend(window::encode_selection(None));
        send(game, body);
    }
}

/// `GuiWindowCreatorTab.openWindow`.
fn open_window(game: &mut Game<'_>, spec: &UpgradeWindowSpec) {
    let Some(size) = gui_size(game) else {
        return;
    };
    let Some(rect) = game.mod_screen.as_ref().map(|screen| screen.screen.width) else {
        return;
    };
    let types = spec_types(spec);
    let layout = SupportedLayout::new(game.native_text_width(&lang::get("gui.mekanism.upgrades.supported")) as i32);
    let rows = layout.needed_rows(types.len());
    let height = uw::window_height(rows);
    let screen = {
        let origin = game_origin(game);
        Screen {
            left: origin[0],
            top: origin[1],
            scaled_width: size[0],
            scaled_height: size[1],
        }
    };
    let mut guard = client();
    let state = &mut *guard;
    let positions = state.positions.get_or_insert_with(|| load_positions(spec));
    let window = Window::new(
        selection(spec),
        positions,
        screen,
        uw::default_position(rect as i32),
        [uw::WINDOW_WIDTH, height],
    );
    let list = ScrollList::new(LIST[0], LIST[1], LIST[2], LIST[3]);
    let now = now_ms();
    state.window = Some(Open {
        window,
        list,
        selected: None,
        opened_ms: now,
        selected_ms: now,
    });
    drop(guard);
    // `startTracking`: a janela acrescenta os dados dela ao menu, no cliente e (pelo pedido) no servidor, nas mesmas posições.
    if let Some(pos) = client().screen_pos {
        mirrors(|table| {
            if let Some(entry) = table.get_mut(&pos) {
                let positions = entry.mirror.start_tracking(UPGRADE_WINDOW_KEY, &(spec.window_trackers)());
                entry.window = positions.try_into().ok();
            }
        });
    }
    // O `PacketGuiInteract` do `startTracking`, e `onFocused` que escolhe a janela.
    send(game, vec![OP_TRACK]);
    let mut body = vec![OP_SELECT];
    body.extend(window::encode_selection(Some(selection(spec))));
    send(game, body);
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

/// O hook `frame`: cuida do que a entrada pediu, põe os slots virtuais onde a janela está e esconde-os quando ela fecha.
pub(in crate::game) fn frame(game: &mut Game<'_>) {
    let Some(spec) = open_spec(game) else {
        // A tela fechou: `GuiMekanism.removed` fecha as janelas e grava a posição, sem precisar avisar o servidor, que larga o menu.
        let had_window = {
            let state = client();
            state.window.is_some() || state.screen_block.is_some()
        };
        if had_window {
            let block = client().screen_block.clone();
            if let Some(spec) = block.and_then(|block| spec_of(&block)) {
                close_window(game, spec, false);
            }
            let mut guard = client();
            guard.screen_block = None;
            guard.pending.clear();
            guard.sent_selected = false;
            // `MekanismContainer.removed`: o menu some com a tela.
            if let Some(pos) = guard.screen_pos.take() {
                mirrors(|table| table.remove(&pos));
            }
        }
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
            state.window = None;
            state.pending.clear();
            let positions = state.positions.get_or_insert_with(|| load_positions(spec));
            if positions.was_pinned(selection(spec)) {
                state.pending.push(Action::Open);
            }
        }
    }
    let actions: Vec<Action> = std::mem::take(&mut client().pending);
    for action in actions {
        match action {
            Action::Open => {
                let closed = client().window.is_none();
                if closed {
                    open_window(game, spec);
                }
            }
            Action::Close => close_window(game, spec, true),
            Action::Remove { index, all } => {
                send(game, [&[OP_REMOVE][..], &uw::encode_remove(index, all)].concat());
            }
        }
    }
    place_virtual_slots(game, spec);
}

/// Os slots virtuais ficam na posição que a janela dá a eles, ou somem (`None`) sem janela.
fn place_virtual_slots(game: &mut Game<'_>, spec: &UpgradeWindowSpec) {
    let wanted: [Option<[f32; 2]>; 2] = {
        let state = client();
        match &state.window {
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

/// `screen_key_pressed_pre`: Esc fecha a janela que não está fixa e consome a tecla; fixa, a tecla segue e a tela fecha.
pub(in crate::game) fn key_pressed(game: &Game<'_>, key: &str, canceled: &mut bool) {
    if open_spec(game).is_none() || key != "key.keyboard.escape" {
        return;
    }
    let mut guard = client();
    let state = &mut *guard;
    if let Some(open) = &state.window
        && open.window.escape()
    {
        state.pending.push(Action::Close);
        *canceled = true;
    }
}

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
    let mut guard = client();
    let state = &mut *guard;
    let Some(open) = state.window.as_mut() else {
        // `GuiInsetElement.isMouseOver`: só com a aba ativa e sobre o botão de dentro, e só o botão esquerdo.
        let rel = [mouse[0] - f64::from(screen.left), mouse[1] - f64::from(screen.top)];
        if button == 0 && in_rect(TAB_BUTTON, rel) {
            state.pending.push(Action::Open);
            *canceled = true;
        }
        return;
    };
    match open.window.press(screen, mouse) {
        Click::Outside => {}
        Click::Inside { .. } => {
            let rel = relative(open, screen, mouse);
            let shift = modifiers & 1 != 0;
            // Os filhos, do último ao primeiro: os slots virtuais ficam com o clique do jogo (`slotClicked`).
            let on_slot = SLOT_ITEMS
                .iter()
                .any(|item| in_rect([item[0] - 1, item[1] - 1, 18, 18], rel));
            if on_slot {
                return;
            }
            *canceled = true;
            let left_or_right = button == 0 || button == 1;
            if left_or_right && open.selected.is_some() && in_rect(UNINSTALL, rel) {
                if let Some(index) = open.selected.and_then(|n| u16::try_from(n).ok()) {
                    state.pending.push(Action::Remove { index, all: shift });
                }
            } else if left_or_right && in_rect(PIN, rel) {
                open.window.toggle_pinned();
            } else if left_or_right && in_rect(CLOSE, rel) {
                state.pending.push(Action::Close);
            } else if button == 0 {
                let installed = sync.installed();
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
    }
}

/// `screen_mouse_button_released_pre`: solta o arrasto da janela e o da barra.
pub(in crate::game) fn mouse_released(game: &Game<'_>, canceled: &mut bool) {
    if open_spec(game).is_none() {
        return;
    }
    let mut state = client();
    if let Some(open) = state.window.as_mut() {
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
    let Some(open) = state.window.as_mut() else {
        return;
    };
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

/// `screen_mouse_scrolled_pre`: a roda sobre a lista a rola; sem barra o gesto segue para a tela (`InteractionStrategy.ALL`).
pub(in crate::game) fn mouse_scrolled(
    game: &Game<'_>,
    view: &NativeScreenView<'_>,
    scroll: [f64; 2],
    canceled: &mut bool,
) {
    let Some(spec) = open_spec(game) else {
        return;
    };
    let Some(screen) = screen_of(view, game) else {
        return;
    };
    let mouse = mouse_of(view);
    let total = read_sync(game).installed().len();
    let mut state = client();
    let Some(open) = state.window.as_mut() else {
        return;
    };
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
    let Some(open) = state.window.as_ref() else {
        return found;
    };
    let Some(renderer) = game.renderer.as_ref() else {
        return None;
    };
    let batch = HudBatch::new([renderer.config.width, renderer.config.height]);
    let (ox, oy) = game.inventory_origin(&batch);
    let rel = [x - ox, y - oy];
    for (id, item) in virtual_ids(spec).into_iter().zip(SLOT_ITEMS) {
        let frame = [
            (open.window.relative_x + item[0] - 1) as f32,
            (open.window.relative_y + item[1] - 1) as f32,
        ];
        if rel[0] >= frame[0] && rel[0] < frame[0] + 18.0 && rel[1] >= frame[1] && rel[1] < frame[1] + 18.0 {
            return Some(id);
        }
    }
    let [wx, wy] = [open.window.relative_x as f32, open.window.relative_y as f32];
    let inside = rel[0] >= wx
        && rel[0] < wx + open.window.width as f32
        && rel[1] >= wy
        && rel[1] < wy + open.window.height as f32;
    if inside { None } else { found }
}

/// `Game::draw_inventory_slots`, no lugar do Horizon enquanto há janela: os itens dos slots normais, a janela por cima, e os
/// itens dos slots virtuais. O tooltip e o item no cursor o Horizon desenha depois, por cima de tudo. Devolve se tomou o desenho.
pub(in crate::game) fn draw_slots(game: &mut Game<'_>, batch: &mut HudBatch, ox: f32, oy: f32) -> bool {
    let Some(spec) = open_spec(game) else {
        return false;
    };
    if client().window.is_none() {
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
    draw_window(game, spec, batch, ox, oy, &virtual_ids);
    true
}

/// `GuiMekanism.findElementOver` para o que a lib desenha: a janela de upgrades aberta, ou o botão da aba dela quando não há janela
/// (`GuiWindow` é ativo, e a aba fica inativa com a janela aberta). O ponto `at` é relativo ao canto da tela.
pub(in crate::game) fn over_element(game: &Game<'_>, at: [f32; 2]) -> bool {
    if open_spec(game).is_none() {
        return false;
    }
    covers(at) || (client().window.is_none() && hovering(shifted(TAB_BUTTON, 0.0, 0.0), (at[0], at[1])))
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

fn draw_window(
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
    let state = client();
    let Some(open) = state.window.as_ref() else {
        return;
    };
    let data = (spec.data)();
    let order = data.type_ids();
    let types = upgrade::current_types();
    let sync = read_sync(game);
    let installed = sync.installed();
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
    if let Some(effect) = uw::effect_text(data.supports(id), amount, max, spec.max_multiplier) {
        text_y += 6.0;
        line(&uw::translate(&lang::get("gui.mekanism.upgrades.effect"), &[effect]), text_y);
    }
}

/// A aba na lateral direita, a cada quadro, na ordem de `GuiSideHolder` e `GuiInsetElement`: o fundo do botão, a aba tingida por
/// cima dele, o fundo outra vez e o ícone. `origin` é o canto da tela.
pub(in crate::game) fn draw_tab(game: &Game<'_>, batch: &mut HudBatch, block: &str, origin: [f32; 2]) {
    let Some(spec) = spec_of(block) else {
        return;
    };
    let active = client().window.is_none();
    let button = shifted(TAB_BUTTON, origin[0], origin[1]);
    let cursor = game.native_cursor();
    let hovered = active && hovering(button, (cursor[0], cursor[1]));
    button_background(game, batch, spec, button, active, hovered);
    nine(game, batch, spec, "holder_right", uw::HOLDER_SLICE, shifted(TAB, origin[0], origin[1]), WHITE);
    button_background(game, batch, spec, button, active, hovered);
    plain(game, batch, spec, "button/upgrade", [18.0, 18.0], button);
}

/// `GuiElement.isMouseOverCheckWindows`: o ponto `at` (relativo ao canto da tela) está sobre a janela aberta, e então o elemento de baixo
/// não o recebe.
pub(in crate::game) fn covers(at: [f32; 2]) -> bool {
    client().window.as_ref().is_some_and(|open| {
        let rel = [at[0] - open.window.relative_x as f32, at[1] - open.window.relative_y as f32];
        rel[0] >= 0.0 && rel[1] >= 0.0 && rel[0] < open.window.width as f32 && rel[1] < open.window.height as f32
    })
}

/// Os tooltips da aba e da janela, no mesmo estilo do `WidgetTooltipHolder`. `at` é o mouse relativo ao canto da tela.
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
    let state = client();
    let origin = [cursor.0 - at[0], cursor.1 - at[1]];
    let mut lines: Vec<(String, Option<u32>)> = Vec::new();
    let mut flat = false;
    let rect: [f32; 4];
    match &state.window {
        None => {
            if !hovering(shifted(TAB_BUTTON, 0.0, 0.0), (at[0], at[1])) {
                return false;
            }
            lines.push((lang::get("gui.mekanism.upgrades"), None));
            rect = shifted(TAB_BUTTON, origin[0], origin[1]);
        }
        Some(open) => {
            let [wx, wy] = [open.window.relative_x as f32, open.window.relative_y as f32];
            let rel = [at[0] - wx, at[1] - wy];
            let rel_i = [f64::from(rel[0]), f64::from(rel[1])];
            if rel[0] < 0.0 || rel[1] < 0.0 || rel[0] >= open.window.width as f32 || rel[1] >= open.window.height as f32 {
                return false;
            }
            let win = |r: [i32; 4]| shifted(r, origin[0] + wx, origin[1] + wy);
            let order = spec_types(spec);
            let sync = read_sync(game);
            let installed = sync.installed();
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
    adopt_open_menus(game);
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
            let shown = selected == Some(selection(spec));
            if let Some(host) = game.lan.as_mut()
                && let Some(conn) = host.hub.guests.iter().find(|g| g.uuid == player).map(|g| g.conn)
            {
                let hidden = if shown { Vec::new() } else { spec.virtual_slots.to_vec() };
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
    game.mb_muffling_player_in(player);
}

/// `native_server_data_loaded`: os dados mudaram (um `/reload`), então todos os que estão no servidor recebem de novo.
pub(in crate::game) fn data_reloaded(game: &mut Game<'_>) {
    let players: Vec<u128> = game.native_server_players().iter().map(|p| p.uuid).collect();
    for player in players {
        send_types(game, player);
    }
}

/// `native_net_from_server`: o cliente guarda o que o servidor mandou.
pub(in crate::game) fn net_from_server(channel: &str, data: &[u8]) {
    if crate::game::machinery_tile_sound::muffling_from_server(channel, data)
        || crate::game::machinery_tile_sound::name_from_server(channel, data)
    {
        return;
    }
    match channel {
        CONTAINER_CHANNEL => container_from_server(data),
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

    /// `ItemUpgrade.useOn`.
    pub(in crate::game) fn mb_window_use_item_on(&mut self, stack: &ItemStack, pos: [i32; 3], claimed: &mut bool) {
        use_item_on(self, stack, pos, claimed);
    }

    pub(in crate::game) fn mb_window_frame(&mut self) {
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
