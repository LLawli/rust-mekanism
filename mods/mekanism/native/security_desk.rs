// Entra em `crates/horizon-client/src/game.rs` como `mod mekanism_desk`. A Security Desk (`TileEntitySecurityDesk`, `BlockTypeTile SECURITY_DESK`): o tile com o dono e
// os dois slots (o de cima destrava o item do dono, o de baixo o trava com o dono e o modo da frequência), o bloco de apoio em cima (`AttributeHasBounding.ABOVE_ONLY`) e
// os pacotes da tela (`PacketGuiInteract` da mesa e `PacketAddTrusted`). A frequência de segurança, a lista de confiança e a sobreposição são da `machinery_basis`
// (`security_frequency`, `security_desk`); aqui fica só o que é deste bloco.
//
// A tela (`GuiSecurityDesk`) é da `machinery_basis` (`security_desk_screen` e a seção da mesa em `windows.rs`): aqui o bloco diz o contêiner dos dois slots, o tamanho
// da tela, o que o menu sincroniza (a frequência do dono, que só chega ao abrir) e as regras dos slots.
use crate::game::mekanism_glue as glue;
use crate::game::mekanism_rules as rules;
use crate::game::{Game, machinery_basis as basis};
use horizon_client::containers::{BlockEntity, ModBlockEntity, ModContainer};
use horizon_client::hud::HudBatch;
use horizon_client::mod_api::NativeScreen;
use machinery_basis_core::container_sync::{Tracked, TrackedSource, Value};
use machinery_basis_core::quick_move::{self, QuickMenu, Region};
use machinery_basis_core::security_desk_screen as layout;
use machinery_basis_core::tile_trackers::{TileTrackers, source};
use machinery_basis_core::upgrade_window::UpgradeWindowSpec;
use horizon_client::items::ItemStack;
use horizon_world::nbt::Compound;

/// `MekanismBlocks.SECURITY_DESK` e `BOUNDING_BLOCK`.
pub(in crate::game) const DESK: &str = "mekanism:security_desk";
pub(in crate::game) const BOUNDING: &str = "mekanism:bounding_block";
/// O nome deste mod nos registros da `machinery_basis`.
const OWNER: &str = "mekanism";
/// `AttributeHasBounding.ABOVE_ONLY`: um apoio, em cima.
const ABOVE_ONLY: &[[i32; 3]] = &[[0, 1, 0]];

pub(in crate::game) fn is_desk(block: &str) -> bool {
    block == DESK
}

/// Os blocos desta parte que têm block entity: a mesa e o bloco de apoio (`TileEntityBoundingBlock`, que guarda o deslocamento até o principal).
pub(in crate::game) fn block_entity(block: &str, entity: &mut Option<(String, Compound)>) {
    if is_desk(block) {
        // A mesa só tem dono (`ownerCapability`, sem `securityCapability`): só ele a abre, a quebra e a configura.
        basis::security_owner_only_register(DESK);
        // `ItemBlockTooltip.attachCapabilities`: os itens dos blocos com `AttributeSecurity` deste mod têm dono (as máquinas, os cubos e a própria mesa).
        basis::security_ownable_register(OWNER, glue::item_has_owner);
        basis::bounding_register(DESK, BOUNDING, ABOVE_ONLY);
        Game::mb_window_register(&WINDOW_SPEC, remove_upgrade, add_upgrades, window_source);
        Game::mb_desk_register(DESK);
    }
    if (is_desk(block) || block == BOUNDING) && entity.is_none() {
        *entity = Some((block.to_owned(), Compound::default()));
    }
}

/// `serverTicker(TileEntityMekanism::tickServer)`.
pub(in crate::game) fn ticks(block: &str, ticks: &mut bool) {
    if is_desk(block) {
        *ticks = true;
    }
}

/// `TileEntitySecurityDesk.onUpdateServer`: a frequência do tile é validada (`TileComponentFrequency.tickServer`) e os dois slots agem. `ownable` diz que itens têm
/// dono (`SecurityInventorySlot.VALIDATOR`).
pub(in crate::game) fn tick(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, ownable: &dyn Fn(&str) -> bool) {
    basis::security_apply_pending(entity, pos);
    with_content(|spec| basis::content_apply_pending(entity, pos, spec));
    basis::security_tile_tick(game, pos, entity);
    if basis::security_desk_tick(game, entity, ownable) {
        game.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
    }
}

/// `BlockMekanism.setPlacedBy`: o dono de quem pôs (ou o do item) e o bloco de apoio em cima.
pub(in crate::game) fn place_block(game: &mut Game<'_>, stack: &ItemStack, pos: [i32; 3]) {
    if !is_desk(&game.native_block(pos)) {
        return;
    }
    basis::name_placed(game, stack, pos, &is_desk);
    basis::security_placed(game, stack, pos, &is_desk);
    // `applyImplicitComponents`: os dois slots que o item trouxe (`mekanism:items`) voltam ao tile.
    with_content(|spec| basis::content_placed(game, stack, pos, spec));
    basis::bounding_placed(game, pos);
}

/// O que a mesa guarda no item (`loot_table/blocks/security_desk.json`: `copy_components` de `mekanism:owner`, `mekanism:security` e `mekanism:items`): os dois
/// slots, pelo mesmo mecanismo das máquinas (`machinery_basis::ContentSpec`). O dono e o modo são de `security_loot`.
fn with_content<R>(run: impl FnOnce(&basis::ContentSpec<'_>) -> R) -> R {
    let spec = basis::ContentSpec { is_ours: &is_desk, item_slots: SLOTS, collect: &|_, _, _| Vec::new(), apply: &|_, _| {} };
    run(&spec)
}

/// A mesa vai ser quebrada: os dois slots vão para o item e saem do tile (o original não os solta no chão). Quem chama já conferiu o acesso.
pub(in crate::game) fn destroy_content(game: &mut Game<'_>, pos: [i32; 3]) {
    if is_desk(&game.native_block(pos)) {
        with_content(|spec| basis::content_destroying(game, pos, spec));
    }
}

/// O jogador quebra a mesa ou o apoio dela. Devolve se o bloco era desta parte.
///
/// A mesa: quem não é o dono não a quebra (`BlockMekanism.getDestroyProgress` e o `BreakBlockEvent`); o dono e o nome ficam guardados para o item solto. O apoio
/// (`BlockBounding.playerDestroy` e `onDestroyedByPlayer`): quebrá-lo é quebrar o principal como o mesmo jogador (os itens, o som e as partículas são os do
/// principal); se o principal recusa, o apoio fica.
pub(in crate::game) fn destroy_block(game: &mut Game<'_>, pos: [i32; 3], canceled: &mut bool) -> bool {
    let block = game.native_block(pos);
    if block == BOUNDING {
        if *canceled {
            return true;
        }
        let Some(main) = basis::bounding_main(game, pos).filter(|main| is_desk(&game.native_block(*main))) else {
            return true;
        };
        if let Err(error) = game.break_block_as_player(main) {
            eprintln!("mekanism: o bloco principal em {main:?} não foi quebrado pelo apoio: {error}");
        }
        // O principal saiu e levou os apoios; se ficou (sem acesso), o apoio também fica. De todo jeito a quebra do apoio em si não segue.
        basis::bounding_flush(game);
        *canceled = true;
        return true;
    }
    if !is_desk(&block) {
        return false;
    }
    if !*canceled && basis::security_break(game, pos, &is_desk) {
        *canceled = true;
        return true;
    }
    if !*canceled {
        basis::security_destroying(game, pos, &is_desk);
        basis::name_destroying(game, pos, &is_desk);
    }
    true
}

/// `copy_name` e `copy_components` de `owner`, `security` e `items` do loot da mesa (`loot_table/blocks/security_desk.json`).
pub(in crate::game) fn loot_drops(game: &Game<'_>, context: &horizon_client::mod_api::NativeLootDrops<'_>, drops: &mut [ItemStack]) {
    basis::name_loot(game, context, drops, &is_desk);
    basis::security_loot(game, context, drops, &is_desk);
    with_content(|spec| basis::content_loot(game, context, drops, spec));
}

/// `TileEntityMekanism.openGui`: só o dono abre a mesa; os outros recebem a mensagem. Devolve se a abertura deve ser cancelada.
pub(in crate::game) fn container_opened(game: &mut Game<'_>, player: u128, pos: [i32; 3]) -> bool {
    is_desk(&game.native_block(pos)) && basis::security_open(game, player, pos, &is_desk)
}

/// Os três `GuiInteraction` da mesa (`SECURITY_DESK_MODE`, `OVERRIDE_BUTTON` e `REMOVE_TRUSTED`). Devolve se o tile é a mesa.
pub(in crate::game) fn menu_button(game: &mut Game<'_>, pos: [i32; 3], entity: &ModBlockEntity, id: i32) -> bool {
    if !is_desk(&entity.id) {
        return false;
    }
    basis::security_desk_button(game, pos, entity, id);
    true
}

// ---------------------------------------------------------------------------------------------------------------------------
// A tela (`GuiSecurityDesk`) e o menu (`MekanismTileContainer<TileEntitySecurityDesk>`).

/// Os dois slots do tile (`SecurityInventorySlot.unlock(..., 146, 18)` e `lock(..., 146, 97)`).
pub(in crate::game) const SLOTS: usize = 2;

/// O contêiner do tipo de bloco: os dois slots e o menu. Nenhuma face: a mesa não tem configuração de lados, e o original não a expõe a funil nem a cano por aqui.
pub(in crate::game) fn container(block: &str, container: &mut Option<ModContainer>) {
    if is_desk(block) {
        *container = Some(ModContainer { slots: SLOTS, menu: Some(block.to_owned()), ..Default::default() });
    }
}

/// `GuiSecurityDesk`: 176 por 230 (`DEFAULT_IMAGE_HEIGHT + 64`), os dois slots do tile e o inventário do jogador 64 abaixo do lugar comum.
pub(in crate::game) fn screen(block: &str, screen: &mut Option<NativeScreen>) -> bool {
    if !is_desk(block) {
        return false;
    }
    *screen = Some(NativeScreen {
        width: layout::IMAGE_WIDTH as f32,
        height: layout::IMAGE_HEIGHT as f32,
        slots: machinery_basis_core::security_desk::SLOT_POSITIONS.iter().map(|(x, y)| Some([*x as f32, *y as f32])).collect(),
        inventory_y: layout::INVENTORY_Y as f32,
        inventory_x: 8.0,
        own_labels: true,
        own_background: true,
    });
    true
}

/// `TileEntityMekanism.addContainerTrackers` da mesa: os componentes de frequência e de segurança (a frequência do dono e o modo do tile, que a mesa não usa).
fn trackers() -> Vec<Tracked> {
    TileTrackers { security: true, ..TileTrackers::default() }.trackers()
}

/// A janela do bloco na `machinery_basis`: só o menu (os dados que ele sincroniza). A mesa não tem upgrades, livro nem abas.
static WINDOW_SPEC: UpgradeWindowSpec = UpgradeWindowSpec {
    block: DESK,
    upgrades: false,
    recipe_book: false,
    book: None,
    book_slots: [0, 0],
    virtual_slots: [SLOTS, SLOTS + 1],
    trackers,
    window_trackers: rules::upgrade_window_data,
    ns: "mekanism",
    window_type: rules::UPGRADE_WINDOW,
    window_types: &rules::WINDOW_TYPES,
    base_slice: rules::PANEL_SLICE,
    max_multiplier: machinery_basis_core::settings::max_upgrade_multiplier,
    client_config: rules::CLIENT_CONFIG_FILE,
    data: glue::window_upgrade_data,
    color: rules::enum_color,
    item: rules::UPGRADE_ITEM,
    item_component: rules::UPGRADE_TYPE_COMPONENT,
};

fn remove_upgrade(_: &mut Game<'_>, _: [i32; 3], _: &str, _: bool) {}

fn add_upgrades(_: &mut Game<'_>, _: [i32; 3], _: &str, _: i32) -> i32 {
    0
}

/// Os dados do menu da mesa, lidos do tile e da tabela das frequências.
struct DeskSync {
    security: machinery_basis_core::security::SecuritySync,
}

impl TrackedSource for DeskSync {
    fn value(&self, id: u16) -> Value {
        match id {
            source::SECURITY_FREQUENCY => {
                let (hash, bytes) = self.security.frequency();
                Value::Codec { hash, bytes }
            }
            source::SECURITY_MODE => Value::Enum(self.security.mode()),
            other => panic!("fonte {other} desconhecida"),
        }
    }
}

/// O valor vivo dos dados do menu da mesa em `pos` (`MekanismContainer.broadcastChanges`).
fn window_source(game: &mut Game<'_>, pos: [i32; 3], act: &mut dyn FnMut(&dyn TrackedSource)) -> bool {
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(pos[0], pos[1], pos[2])) else {
        return false;
    };
    if !is_desk(&entity.id) {
        return false;
    }
    let security = basis::security_sync(game, entity);
    act(&DeskSync { security });
    true
}

/// `InventoryContainerSlot.mayPlace` do clique que vai rodar nos dois slots da mesa aberta. Devolve se o bloco é a mesa.
pub(in crate::game) fn publish_slot_rules(game: &Game<'_>, pos: [i32; 3], owner: &'static str, ownable: fn(&str) -> bool) -> bool {
    if !is_desk(&game.native_block(pos)) {
        return false;
    }
    basis::security_desk_slot_rules(game, pos, owner, ownable);
    true
}

/// O menu da mesa visto pelo `quickMoveStack` do Mekanism: os dois slots do tile e o inventário de quem clica. Um slot do tile tem o índice dele; o do jogador,
/// `items.len() + índice no inventário` (a barra de atalhos é 0 a 8, o inventário 9 a 35).
struct DeskMenu<'a, 'g> {
    game: &'a Game<'g>,
    entity: &'a mut ModBlockEntity,
    inventory: &'a mut [Option<ItemStack>],
    desk_owner: Option<u128>,
}

impl DeskMenu<'_, '_> {
    fn first_player(&self) -> usize {
        self.entity.items.len()
    }

    fn cell(&mut self, slot: usize) -> &mut Option<ItemStack> {
        let first = self.first_player();
        if slot < first { &mut self.entity.items[slot] } else { &mut self.inventory[slot - first] }
    }
}

impl QuickMenu for DeskMenu<'_, '_> {
    type Item = ItemStack;

    fn slots_of(&self, region: Region) -> Vec<usize> {
        let first = self.first_player();
        match region {
            // `getInitialInventory`: o de destravar e depois o de travar.
            Region::Container => (0..SLOTS).collect(),
            Region::Hotbar => (first..first + 9).collect(),
            Region::Main => (first + 9..first + 36).collect(),
            // A armadura só aceita o que se veste, e nenhum item com dono daqui se veste.
            Region::Armor | Region::Offhand => Vec::new(),
        }
    }

    fn region(&self, slot: usize) -> Region {
        let first = self.first_player();
        if slot < first {
            Region::Container
        } else if slot - first < 9 {
            Region::Hotbar
        } else {
            Region::Main
        }
    }

    fn exists(&self, slot: usize) -> bool {
        // Os dois virtuais que a janela da `machinery_basis` põe no fim do tile não são da mesa.
        slot < SLOTS || slot >= self.first_player()
    }

    fn stack(&self, slot: usize) -> Option<(ItemStack, u32)> {
        let first = self.first_player();
        let held = if slot < first { self.entity.items[slot].as_ref() } else { self.inventory[slot - first].as_ref() };
        held.filter(|stack| stack.count > 0).map(|stack| (stack.clone(), stack.count))
    }

    fn insert(&mut self, slot: usize, item: &ItemStack, amount: u32) -> u32 {
        if slot < self.first_player() {
            // `SecurityInventorySlot.canInsert` com `MANUAL`.
            let seen = basis::security_item(self.game, item, basis::security_ownable(&item.item));
            if !machinery_basis_core::security_desk::manual_accepts(slot, &seen, self.desk_owner) {
                return 0;
            }
        }
        basis::insert_into_cell(self.game, self.cell(slot), item, amount)
    }

    fn remove(&mut self, slot: usize, amount: u32) {
        basis::remove_from_cell(self.cell(slot), amount);
    }
}

/// Shift-clique no menu da mesa (`MekanismContainer.quickMoveStack`, `:274-328`): do inventário, o item do dono da mesa vai para o slot de destravar e o item sem
/// dono para o de travar, e o que os dois recusam vai para a outra região do inventário; de um slot da mesa, para a barra de atalhos e depois o inventário.
/// Devolve se o tile é a mesa.
pub(in crate::game) fn quick_move(game: &mut Game<'_>, entity: &mut ModBlockEntity, container_slots: usize, slot: usize, stack: &mut ItemStack, handled: &mut bool) -> bool {
    if !is_desk(&entity.id) {
        return false;
    }
    if entity.items.len() < SLOTS {
        entity.items.resize(SLOTS, None);
    }
    let desk_owner = basis::security_read(entity).owner;
    let first = entity.items.len();
    // O menu numera o inventário do jogador depois dos slots do bloco: o principal (9 a 35), depois a barra de atalhos (0 a 8).
    let from = if slot < container_slots {
        slot
    } else {
        let n = slot - container_slots;
        first + if n < 27 { 9 + n } else { n - 27 }
    };
    let mut inventory = std::mem::take(&mut game.inventory);
    let left = {
        let mut menu = DeskMenu { game: &*game, entity, inventory: &mut inventory, desk_owner };
        let _ = quick_move::quick_move(&mut menu, from);
        menu.stack(from)
    };
    game.inventory = inventory;
    // O slot clicado guarda o que sobrou; `handled` impede o roteamento do jogo, que levaria o item a outros slots.
    match left {
        Some((remaining, count)) => {
            *stack = remaining;
            stack.count = count;
        }
        None => stack.count = 0,
    }
    *handled = true;
    true
}

/// A tela inteira: o painel `base` do Mekanism e os elementos de `GuiSecurityDesk`.
pub(in crate::game) fn screen_draw(game: &Game<'_>, block: &str, batch: &mut HudBatch, origin: [f32; 2], panel: &basis::PanelStyle) -> bool {
    if !is_desk(block) {
        return false;
    }
    game.mb_desk_draw(batch, block, origin, panel);
    true
}

/// Os tooltips dos botões de modo e de sobreposição. Devolve se o bloco é a mesa e se desenhou.
pub(in crate::game) fn widget_tooltip(game: &Game<'_>, batch: &mut HudBatch, block: &str, at: [f32; 2], cursor: (f32, f32)) -> Option<bool> {
    is_desk(block).then(|| game.mb_desk_tooltip(batch, block, at, cursor))
}
