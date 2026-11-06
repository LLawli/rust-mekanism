//! Janelas de GUI: porte da parte que não desenha de `GuiWindow`, `GuiWindowCreatorTab`, `SelectedWindowData` e do pedaço de
//! `GuiMekanism` que cuida delas (`addWindow`, `removeWindow`, `keyPressed`, `mouseClicked`). A lib não conhece nenhuma janela:
//! o mod entrega a tabela de [`WindowType`] (o enum `WindowType` do Java), e tudo aqui é conta sobre inteiros de GUI.
//!
//! Coordenadas: o `relative_x` e o `relative_y` de uma janela são relativos ao canto da tela (`leftPos`, `topPos`), como no Java. O
//! mouse chega em pixels de GUI absolutos.
use std::collections::BTreeMap;

/// O valor de uma posição que nunca foi gravada (`Integer.MAX_VALUE` no `ClientConfig`).
pub const UNSET_POSITION: i32 = i32::MAX;

/// A faixa de cima da janela por onde ela se arrasta (`event.y() < getY() + 18`).
pub const DRAG_STRIP: i32 = 18;

/// `WindowType`: o que o tipo da janela diz sobre como ela guarda a posição.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WindowType {
    /// O `ordinal()` do enum, que é o que vai no `PacketWindowSelect`.
    pub ordinal: u8,
    /// `saveName`: minúsculo; `None` nas janelas que não guardam posição.
    pub save_name: Option<&'static str>,
    /// `canPin`.
    pub can_pin: bool,
    /// `maxData`: quantas janelas desse tipo podem existir (o `extraData` vai de 0 a `max_data - 1`).
    pub max_data: u8,
}

impl WindowType {
    /// `isValid(extraData)`.
    pub fn is_valid(&self, extra: u8) -> bool {
        extra < self.max_data
    }

    /// `getSaveName(extraData)`: com mais de uma janela do tipo, o `extraData` entra no fim do nome.
    pub fn save_name_for(&self, extra: u8) -> Option<String> {
        let name = self.save_name?;
        Some(if self.max_data == 1 {
            name.to_owned()
        } else {
            format!("{name}{extra}")
        })
    }
}

/// `SelectedWindowData`: o tipo da janela e o dado extra. É a chave de `exists` dos slots virtuais.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SelectedWindow {
    pub ty: WindowType,
    pub extra: u8,
}

impl SelectedWindow {
    /// O construtor canônico: um `extraData` inválido vira zero.
    pub fn new(ty: WindowType, extra: u8) -> Self {
        Self {
            ty,
            extra: if ty.is_valid(extra) { extra } else { 0 },
        }
    }

    /// `VirtualInventoryContainerSlot.exists(windowData)`: o slot existe quando a janela escolhida é a dele.
    pub fn exists(own: SelectedWindow, selected: Option<SelectedWindow>) -> bool {
        selected == Some(own)
    }
}

/// Os bytes do `PacketWindowSelect`: o `extraData` (ou -1 sem janela) e, havendo janela, o `ordinal` do tipo.
pub fn encode_selection(selected: Option<SelectedWindow>) -> Vec<u8> {
    match selected {
        None => vec![0xFF],
        Some(window) => vec![window.extra, window.ty.ordinal],
    }
}

/// O inverso de [`encode_selection`]. `types` é o enum inteiro, na ordem do `ordinal`: o `ByIdMap.continuous` do Java dá a volta
/// quando o id passa do fim. `None` é uma mensagem malformada (o servidor a ignora); `Some(None)` é "sem janela".
pub fn decode_selection(bytes: &[u8], types: &[WindowType]) -> Option<Option<SelectedWindow>> {
    match bytes {
        [0xFF] => Some(None),
        [extra, ordinal] if !types.is_empty() && *extra != 0xFF => {
            let ty = types[usize::from(*ordinal) % types.len()];
            Some(Some(SelectedWindow::new(ty, *extra)))
        }
        _ => None,
    }
}

/// `WindowPosition`: onde a janela fechou da última vez.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowPosition {
    pub x: i32,
    pub y: i32,
    pub pinned: bool,
}

impl WindowPosition {
    /// O que `getLastPosition` devolve quando nada foi gravado.
    pub const UNSET: Self = Self {
        x: UNSET_POSITION,
        y: UNSET_POSITION,
        pinned: false,
    };
}

/// `ClientConfig.lastWindowPositions`: uma entrada por nome de gravação, com o x, o y e (nas janelas que fixam) o `pinned`. Mora no
/// cliente e persiste num arquivo, como o `client.toml` do Mekanism.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WindowPositions {
    entries: BTreeMap<String, WindowPosition>,
}

impl WindowPositions {
    /// `SelectedWindowData.getLastPosition`.
    pub fn last_position(&self, window: SelectedWindow) -> WindowPosition {
        window
            .ty
            .save_name_for(window.extra)
            .and_then(|name| self.entries.get(&name).copied())
            .unwrap_or(WindowPosition::UNSET)
    }

    /// `SelectedWindowData.wasPinned`.
    pub fn was_pinned(&self, window: SelectedWindow) -> bool {
        self.last_position(window).pinned
    }

    /// `updateLastPosition`: grava o x e o y, e o `pinned` só quando o tipo fixa. Devolve se algo mudou (o Java só salva então).
    pub fn update_last_position(
        &mut self,
        window: SelectedWindow,
        x: i32,
        y: i32,
        pinned: bool,
    ) -> bool {
        let Some(name) = window.ty.save_name_for(window.extra) else {
            return false;
        };
        let entry = self.entries.entry(name).or_insert(WindowPosition::UNSET);
        let before = *entry;
        entry.x = x;
        entry.y = y;
        if window.ty.can_pin {
            entry.pinned = pinned;
        }
        *entry != before
    }

    /// O arquivo: uma tabela `[window.<nome>]` por entrada, com as chaves `x`, `y` e `pinned` do `ClientConfig`.
    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        for (name, position) in &self.entries {
            out.push_str(&format!(
                "[window.{name}]\nx = {}\ny = {}\npinned = {}\n\n",
                position.x, position.y, position.pinned
            ));
        }
        out
    }

    /// Lê o que [`to_toml`](Self::to_toml) escreve. Linha que não entende é ignorada, e valor fora do intervalo de um inteiro vale
    /// o padrão (o `ForgeConfigSpec` troca valor inválido pelo padrão).
    pub fn from_toml(text: &str) -> Self {
        let mut positions = Self::default();
        let mut current: Option<String> = None;
        for line in text.lines().map(str::trim) {
            if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                current = header.strip_prefix("window.").map(str::to_owned);
                if let Some(name) = &current {
                    positions
                        .entries
                        .entry(name.clone())
                        .or_insert(WindowPosition::UNSET);
                }
                continue;
            }
            let (Some(name), Some((key, value))) = (&current, line.split_once('=')) else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            let Some(entry) = positions.entries.get_mut(name) else {
                continue;
            };
            match key {
                "x" => entry.x = value.parse().unwrap_or(UNSET_POSITION),
                "y" => entry.y = value.parse().unwrap_or(UNSET_POSITION),
                "pinned" => entry.pinned = value == "true",
                _ => {}
            }
        }
        positions
    }
}

/// `GuiWindow.calculateTarget`: a posição inicial de um eixo. Sem posição gravada vale o padrão; uma gravada que sairia da tela
/// encosta na borda mais próxima.
pub fn calculate_target(
    last_position: i32,
    start: i32,
    size: i32,
    scaled_size: i32,
    default_value: i32,
) -> i32 {
    if last_position == UNSET_POSITION {
        default_value
    } else if start + last_position < 0 {
        -start
    } else if start + last_position + size > scaled_size {
        scaled_size - start - size
    } else {
        last_position
    }
}

/// Onde a tela está: o canto (`leftPos`, `topPos`) e o tamanho da janela do jogo em pixels de GUI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Screen {
    pub left: i32,
    pub top: i32,
    pub scaled_width: i32,
    pub scaled_height: i32,
}

/// O arrasto em curso: onde o mouse estava ao apertar e quanto já foi aplicado.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Drag {
    from: [f64; 2],
    applied: [i32; 2],
}

/// O que `GuiWindow` guarda: posição relativa, tamanho, se está fixa e o arrasto.
#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    pub selection: SelectedWindow,
    pub relative_x: i32,
    pub relative_y: i32,
    pub width: i32,
    pub height: i32,
    pub pinned: bool,
    drag: Option<Drag>,
}

/// O que um clique fez na janela (`GuiWindow.mouseClicked` com `InteractionStrategy.ALL`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Click {
    /// Dentro da janela: o `AbstractWidget.mouseClicked` dela responde ao clique em qualquer ponto de dentro, então ele não passa
    /// à tela. Quem trata o que há sob o mouse (botão, lista, slot virtual) é o chamador.
    Inside { drag_started: bool },
    /// Fora da janela: não é consumido, e a tela atrás o recebe como se a janela não existisse.
    Outside,
}

impl Window {
    /// O construtor de `GuiWindow`: a posição é a gravada (corrigida por [`calculate_target`]) ou o padrão `(x, y)`.
    pub fn new(
        selection: SelectedWindow,
        positions: &WindowPositions,
        screen: Screen,
        default: [i32; 2],
        size: [i32; 2],
    ) -> Self {
        let last = positions.last_position(selection);
        Self {
            selection,
            relative_x: calculate_target(
                last.x,
                screen.left,
                size[0],
                screen.scaled_width,
                default[0],
            ),
            relative_y: calculate_target(
                last.y,
                screen.top,
                size[1],
                screen.scaled_height,
                default[1],
            ),
            width: size[0],
            height: size[1],
            pinned: last.pinned,
            drag: None,
        }
    }

    /// `getX()`, `getY()`, `getRight()`, `getBottom()`: o retângulo na tela.
    pub fn bounds(&self, screen: Screen) -> [i32; 4] {
        let x = screen.left + self.relative_x;
        let y = screen.top + self.relative_y;
        [x, y, x + self.width, y + self.height]
    }

    /// `isMouseOver`.
    pub fn mouse_over(&self, screen: Screen, mouse: [f64; 2]) -> bool {
        let [x, y, right, bottom] = self.bounds(screen);
        mouse[0] >= f64::from(x)
            && mouse[1] >= f64::from(y)
            && mouse[0] < f64::from(right)
            && mouse[1] < f64::from(bottom)
    }

    /// `mouseClicked`: dentro da janela o clique nunca passa à tela, e na faixa de cima começa o arrasto, mesmo que um botão o tenha
    /// tratado (`ret` não entra nessa conta). Fora dela o clique é da tela.
    pub fn press(&mut self, screen: Screen, mouse: [f64; 2]) -> Click {
        if !self.mouse_over(screen, mouse) {
            return Click::Outside;
        }
        let drag_started = mouse[1] < f64::from(screen.top + self.relative_y + DRAG_STRIP);
        if drag_started {
            self.drag = Some(Drag {
                from: mouse,
                applied: [0, 0],
            });
        }
        Click::Inside { drag_started }
    }

    /// `isDragging()`.
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// `onDrag`: `Math.round` da distância do ponto de partida, menos o que já valeu, preso para a janela não sair da tela. Note
    /// que `prevDX` recebe o `newDX` inteiro mesmo quando o `clamp` cortou: o que foi cortado se perde, como no Java.
    pub fn drag_to(&mut self, screen: Screen, mouse: [f64; 2]) {
        let Some(mut drag) = self.drag else {
            return;
        };
        let new = [
            java_round(mouse[0] - drag.from[0]),
            java_round(mouse[1] - drag.from[1]),
        ];
        let [x, y, right, bottom] = self.bounds(screen);
        let change = [
            clamp(new[0] - drag.applied[0], -x, screen.scaled_width - right),
            clamp(new[1] - drag.applied[1], -y, screen.scaled_height - bottom),
        ];
        drag.applied = new;
        self.drag = Some(drag);
        self.relative_x += change[0];
        self.relative_y += change[1];
    }

    /// `onRelease`: `setDragging(false)`.
    pub fn release(&mut self) {
        self.drag = None;
    }

    /// `togglePinned`.
    pub fn toggle_pinned(&mut self) {
        self.pinned = !self.pinned;
    }

    /// `keyPressed` com Esc: uma janela fixa não fecha. Devolve se a tecla foi consumida (e a janela deve fechar).
    pub fn escape(&self) -> bool {
        !self.pinned
    }

    /// `close()`: o que se grava da posição. Devolve se mudou algo para salvar.
    pub fn close(&self, positions: &mut WindowPositions) -> bool {
        positions.update_last_position(
            self.selection,
            self.relative_x,
            self.relative_y,
            self.pinned,
        )
    }
}

/// `Math.round(double)`: o piso de `x + 0.5`.
pub fn java_round(value: f64) -> i32 {
    (value + 0.5).floor() as i32
}

/// `Math.clamp(long, int, int)` sem a exceção de `min > max`: com a janela maior que a tela, vale o mínimo.
fn clamp(value: i32, min: i32, max: i32) -> i32 {
    value.min(max).max(min)
}

/// A pilha de janelas abertas de uma tela (`GuiMekanism.windows`, uma `LRU`): a primeira é a do topo. Guarda só a ordem; o
/// chamador guarda as janelas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowStack<T> {
    windows: Vec<T>,
}

impl<T> Default for WindowStack<T> {
    fn default() -> Self {
        Self {
            windows: Vec::new(),
        }
    }
}

impl<T> WindowStack<T> {
    /// `windows.add` põe a janela nova no topo.
    pub fn push(&mut self, window: T) {
        self.windows.insert(0, window);
    }

    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    pub fn len(&self) -> usize {
        self.windows.len()
    }

    /// Do topo para o fundo, a ordem de `for (GuiWindow w : windows)`.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.windows.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.windows.iter_mut()
    }

    /// Do fundo para o topo, a ordem do desenho (`getWindowsDescendingIterator`).
    pub fn bottom_to_top(&self) -> impl Iterator<Item = &T> {
        self.windows.iter().rev()
    }

    pub fn top(&self) -> Option<&T> {
        self.windows.first()
    }

    pub fn remove(&mut self, index: usize) -> T {
        self.windows.remove(index)
    }

    /// `windows.moveUp`: a janela clicada vai para o topo.
    pub fn move_to_top(&mut self, index: usize) {
        let window = self.windows.remove(index);
        self.windows.insert(0, window);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UPGRADE: WindowType = WindowType {
        ordinal: 9,
        save_name: Some("upgrade"),
        can_pin: true,
        max_data: 1,
    };
    const CRAFTING: WindowType = WindowType {
        ordinal: 2,
        save_name: Some("crafting"),
        can_pin: true,
        max_data: 3,
    };
    const COLOR: WindowType = WindowType {
        ordinal: 0,
        save_name: Some("color"),
        can_pin: false,
        max_data: 1,
    };
    const SCREEN: Screen = Screen {
        left: 100,
        top: 50,
        scaled_width: 400,
        scaled_height: 300,
    };

    fn upgrade() -> SelectedWindow {
        SelectedWindow::new(UPGRADE, 0)
    }

    fn window() -> Window {
        Window::new(
            upgrade(),
            &WindowPositions::default(),
            SCREEN,
            [-11, 15],
            [198, 100],
        )
    }

    #[test]
    fn an_invalid_extra_becomes_zero_and_the_save_name_follows_max_data() {
        assert_eq!(SelectedWindow::new(UPGRADE, 5).extra, 0);
        assert_eq!(SelectedWindow::new(CRAFTING, 2).extra, 2);
        assert_eq!(SelectedWindow::new(CRAFTING, 3).extra, 0);
        assert_eq!(UPGRADE.save_name_for(0).as_deref(), Some("upgrade"));
        assert_eq!(CRAFTING.save_name_for(2).as_deref(), Some("crafting2"));
    }

    #[test]
    fn exists_is_equality_with_the_selected_window() {
        assert!(SelectedWindow::exists(upgrade(), Some(upgrade())));
        assert!(!SelectedWindow::exists(upgrade(), None));
        assert!(!SelectedWindow::exists(
            upgrade(),
            Some(SelectedWindow::new(CRAFTING, 0))
        ));
    }

    #[test]
    fn the_selection_packet_round_trips_and_wraps_the_ordinal() {
        // A tabela está na ordem do `ordinal`, como o enum.
        let types = [
            WindowType {
                ordinal: 0,
                ..COLOR
            },
            WindowType {
                ordinal: 1,
                ..CRAFTING
            },
            WindowType {
                ordinal: 2,
                ..UPGRADE
            },
        ];
        let crafting = SelectedWindow::new(types[1], 2);
        assert_eq!(encode_selection(None), [0xFF]);
        assert_eq!(decode_selection(&[0xFF], &types), Some(None));
        let wire = encode_selection(Some(crafting));
        assert_eq!(wire, [2, 1]);
        assert_eq!(decode_selection(&wire, &types), Some(Some(crafting)));
        // `ByIdMap.continuous(..., WRAP)`: o ordinal 4 de 3 tipos é o 1.
        assert_eq!(
            decode_selection(&[0, 4], &types),
            Some(Some(SelectedWindow::new(types[1], 0)))
        );
        assert_eq!(decode_selection(&[], &types), None);
        assert_eq!(decode_selection(&[1, 2, 3], &types), None);
    }

    #[test]
    fn calculate_target_follows_the_java_branches() {
        // Sem posição gravada: o padrão.
        assert_eq!(calculate_target(UNSET_POSITION, 100, 198, 400, -11), -11);
        // Sairia pela esquerda: encosta em x = 0 da tela.
        assert_eq!(calculate_target(-150, 100, 198, 400, 0), -100);
        // Sairia pela direita: o fim da janela encosta no fim da tela.
        assert_eq!(calculate_target(250, 100, 198, 400, 0), 400 - 100 - 198);
        assert_eq!(calculate_target(40, 100, 198, 400, 0), 40);
        // A borda exata não corta: `start + last + size == scaled`.
        assert_eq!(calculate_target(102, 100, 198, 400, 0), 102);
        assert_eq!(calculate_target(-100, 100, 198, 400, 0), -100);
        // Um pixel para fora da esquerda já encosta, e um pixel para fora da direita também.
        assert_eq!(calculate_target(-101, 100, 198, 400, 0), -100);
        assert_eq!(calculate_target(103, 100, 198, 400, 0), 400 - 100 - 198);
    }

    #[test]
    fn a_new_window_starts_where_it_closed_clamped_to_the_screen() {
        let mut positions = WindowPositions::default();
        assert!(positions.update_last_position(upgrade(), 300, 20, true));
        let window = Window::new(upgrade(), &positions, SCREEN, [-11, 15], [198, 100]);
        assert_eq!((window.relative_x, window.relative_y), (102, 20));
        assert!(window.pinned);
        let fresh = window_default();
        assert_eq!((fresh.relative_x, fresh.relative_y), (-11, 15));
        assert!(!fresh.pinned);
    }

    fn window_default() -> Window {
        window()
    }

    #[test]
    fn the_position_is_saved_on_close_with_pinned_only_for_pinnable_types() {
        let mut positions = WindowPositions::default();
        let mut w = window();
        w.relative_x = 7;
        w.relative_y = 9;
        w.toggle_pinned();
        assert!(w.close(&mut positions));
        assert_eq!(
            positions.last_position(upgrade()),
            WindowPosition {
                x: 7,
                y: 9,
                pinned: true
            }
        );
        assert!(positions.was_pinned(upgrade()));
        // Nada mudou: o Java não salva de novo.
        assert!(!w.close(&mut positions));
        // Tipo que não fixa nunca guarda `pinned`.
        let color = SelectedWindow::new(COLOR, 0);
        assert!(positions.update_last_position(color, 1, 2, true));
        assert!(!positions.was_pinned(color));
        // Sem nome de gravação, nada é gravado.
        let unspecified = SelectedWindow::new(
            WindowType {
                ordinal: 10,
                save_name: None,
                can_pin: false,
                max_data: 1,
            },
            0,
        );
        assert!(!positions.update_last_position(unspecified, 1, 2, false));
    }

    #[test]
    fn the_positions_file_round_trips() {
        let mut positions = WindowPositions::default();
        positions.update_last_position(upgrade(), -4, 33, true);
        positions.update_last_position(SelectedWindow::new(CRAFTING, 1), 5, 6, false);
        let text = positions.to_toml();
        assert_eq!(WindowPositions::from_toml(&text), positions);
        assert!(text.contains("[window.upgrade]\nx = -4\ny = 33\npinned = true"));
        assert!(text.contains("[window.crafting1]"));
        // Valor inválido volta ao padrão, e uma linha solta não derruba a leitura.
        let broken = WindowPositions::from_toml("lixo\n[window.upgrade]\nx = abc\ny = 2\n");
        assert_eq!(
            broken.last_position(upgrade()),
            WindowPosition {
                x: UNSET_POSITION,
                y: 2,
                pinned: false
            }
        );
    }

    #[test]
    fn a_click_inside_is_consumed_and_one_outside_goes_to_the_screen() {
        let mut w = window();
        // A janela está em (100 - 11, 50 + 15) = (89, 65), 198 por 100.
        assert_eq!(w.bounds(SCREEN), [89, 65, 287, 165]);
        assert_eq!(
            w.press(SCREEN, [120.0, 100.0]),
            Click::Inside {
                drag_started: false
            }
        );
        assert!(!w.dragging());
        assert_eq!(w.press(SCREEN, [88.0, 100.0]), Click::Outside);
        assert_eq!(w.press(SCREEN, [287.0, 100.0]), Click::Outside);
        assert_eq!(w.press(SCREEN, [120.0, 165.0]), Click::Outside);
    }

    #[test]
    fn the_strip_starts_a_drag_and_the_move_follows_the_mouse() {
        let mut w = window();
        // `y < getY() + 18`: 82 ainda é a faixa, 83 já não é.
        assert_eq!(
            w.press(SCREEN, [120.0, 82.9]),
            Click::Inside { drag_started: true }
        );
        w.release();
        assert_eq!(
            w.press(SCREEN, [120.0, 83.0]),
            Click::Inside {
                drag_started: false
            }
        );
        w.press(SCREEN, [120.0, 70.0]);
        w.drag_to(SCREEN, [130.0, 75.0]);
        assert_eq!((w.relative_x, w.relative_y), (-1, 20));
        // O segundo evento aplica só a diferença.
        w.drag_to(SCREEN, [135.0, 75.0]);
        assert_eq!((w.relative_x, w.relative_y), (4, 20));
        w.release();
        w.drag_to(SCREEN, [300.0, 300.0]);
        assert_eq!((w.relative_x, w.relative_y), (4, 20));
    }

    #[test]
    fn the_drag_is_clamped_and_what_the_clamp_cut_is_lost() {
        let mut w = window();
        w.press(SCREEN, [120.0, 70.0]);
        // Para a esquerda: x = 89, então -89 é o máximo que vale.
        w.drag_to(SCREEN, [120.0 - 200.0, 70.0]);
        assert_eq!(w.relative_x + SCREEN.left, 0);
        // `prevDX` ficou em -200: voltar 150 dá 50 de diferença, não 150.
        w.drag_to(SCREEN, [120.0 - 50.0, 70.0]);
        assert_eq!(w.relative_x + SCREEN.left, 150);
        // Para a direita: o fim encosta em 400.
        w.drag_to(SCREEN, [120.0 + 900.0, 70.0]);
        assert_eq!(w.bounds(SCREEN)[2], 400);
    }

    #[test]
    fn the_drag_rounds_like_math_round() {
        let mut w = window();
        w.press(SCREEN, [120.0, 70.0]);
        w.drag_to(SCREEN, [122.5, 70.0]);
        assert_eq!(w.relative_x, -11 + 3);
        let mut w = window();
        w.press(SCREEN, [120.0, 70.0]);
        w.drag_to(SCREEN, [117.5, 70.0]);
        assert_eq!(w.relative_x, -11 - 2);
    }

    #[test]
    fn escape_closes_only_an_unpinned_window() {
        let mut w = window();
        assert!(w.escape());
        w.toggle_pinned();
        assert!(!w.escape());
    }

    #[test]
    fn the_stack_keeps_the_newest_on_top_and_moves_a_clicked_one_up() {
        let mut stack = WindowStack::default();
        stack.push("a");
        stack.push("b");
        stack.push("c");
        assert_eq!(stack.top(), Some(&"c"));
        assert_eq!(stack.iter().copied().collect::<Vec<_>>(), ["c", "b", "a"]);
        assert_eq!(
            stack.bottom_to_top().copied().collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
        stack.move_to_top(2);
        assert_eq!(stack.iter().copied().collect::<Vec<_>>(), ["a", "c", "b"]);
        assert_eq!(stack.remove(0), "a");
        assert_eq!(stack.len(), 2);
    }
}
