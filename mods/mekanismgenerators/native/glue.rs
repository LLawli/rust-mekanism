// O Heat Generator no jogo. As regras ficam em mekanismgenerators_rules, e o que é genérico em
// machinery_basis.
use crate::game::mekanismgenerators_rules as rules;
use crate::game::machinery_armor_bar::declare as declare_armor_bar;
use crate::game::{Game, GameLevel, NativeConfig, NativeConfigFile, NativeConfigKind, machinery_basis as basis};
use horizon_client::containers::{BlockEntity, ModBlockEntity, ModContainer, ModEnergyBuffer};
use horizon_client::hud::HudBatch;
use horizon_client::items::ItemStack;
use horizon_client::lang;
use horizon_client::mod_api::{NativeScreen, NativeScreenView};
use horizon_world::nbt::{Compound, JavaString, Tag};
use horizon_world::registry::BlockStateId;
use machinery_basis_core::armor_bar;
use machinery_basis_core::armor_slots::{self, EquipmentSlot, SlotKind};
use machinery_basis_core::gui;
use machinery_basis_core::fluid_logged::{EMPTY_BUCKET, LAVA_BUCKET};
use machinery_basis_core::heat::{AmbientCache, Capacitor};
use machinery_basis_core::transmission::heat_owner::HostCapacitor;
use machinery_basis_core::menu_source;
use machinery_basis_core::quick_move::{self, QuickMenu, Region};
use machinery_basis_core::slot_rules;
use machinery_basis_core::side_tab;
use machinery_basis_core::sides::Face;
use machinery_basis_core::slot_view::{SlotOverlay, SlotType, slot_type};
use machinery_basis_core::tank::{Action, Tank};
use machinery_basis_core::containers::InventorySlot;
use machinery_basis_core::energy_conversion::EnergyConversion;
use machinery_basis_core::energy_cube::ItemEnergy;
use machinery_basis_core::tile::{RedstoneControl, SavedState, TileBase};
use machinery_basis_core::upgrade::{self, UpgradeComponent, UpgradeTransient};
use machinery_basis_core::container_sync::TrackedSource;
use machinery_basis_core::upgrade_window::{UpgradeSync, UpgradeWindowSpec};
use machinery_basis_core::window::SelectedWindow;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

const FUEL_SLOT: usize = rules::FUEL_SLOT;
const ENERGY_SLOT: usize = rules::ENERGY_SLOT;

/// `MekanismGeneratorsConfig.generators` e `.storageConfig` (os dois `Type.SYNCED`, `Mekanism/generators.toml` e `Mekanism/generator-storage.toml`), como o
/// último arquivo lido os deixou. O processo tem a janela e a thread do servidor integrado, então a tabela é do processo; um convidado de LAN não a recebe do
/// anfitrião (o original sincroniza).
static HEAT_CONFIG: std::sync::RwLock<rules::HeatConfig> = std::sync::RwLock::new(rules::HeatConfig::DEFAULT);
/// O que cada arquivo deu na última leitura: o validador de `heatGenerationFluidRate` vê o `tankCapacity` de antes (`CachedLongValue.getOrDefault`).
static LOADED: std::sync::Mutex<(Option<machinery_basis_core::client_config::Loaded>, Option<machinery_basis_core::client_config::Loaded>)> =
    std::sync::Mutex::new((None, None));

fn heat_config() -> rules::HeatConfig {
    *HEAT_CONFIG.read().unwrap_or_else(|error| error.into_inner())
}

/// `VariableCapacityFluidTank`: a capacidade do tanque de lava, lida da configuração a cada uso.
fn lava_tank() -> [u64; 1] {
    [heat_config().tank()]
}

/// `ModContainer.registerConfig(Type.SYNCED, ..)` dos dois arquivos do Generators (`MekanismGeneratorsConfig.registerConfigs`), lidos pelo Horizon como `Server`
/// (decisão 15).
pub(in crate::game) fn register_configs(configs: &mut Vec<NativeConfig>) {
    for (file, spec) in [
        (rules::GENERATORS_CONFIG_FILE, &rules::GENERATORS_CONFIG),
        (rules::GENERATOR_STORAGE_CONFIG_FILE, &rules::GENERATOR_STORAGE_CONFIG),
    ] {
        configs.push(NativeConfig::new("mekanismgenerators", NativeConfigKind::Server, &spec.default_text()).file(file));
    }
}

/// `ModConfigEvent.Loading` e `Reloading`: o arquivo lido vira a configuração (um valor errado é corrigido e o arquivo escrito de novo, como no Mekanism).
pub(in crate::game) fn config_loaded(config: &NativeConfigFile) {
    if config.mod_id != "mekanismgenerators" {
        return;
    }
    let mut loaded = LOADED.lock().unwrap_or_else(|error| error.into_inner());
    match config.file.as_str() {
        rules::GENERATORS_CONFIG_FILE => {
            let next = basis::read_config(config, &rules::GENERATORS_CONFIG, loaded.0.as_ref());
            loaded.0 = Some(next);
        }
        rules::GENERATOR_STORAGE_CONFIG_FILE => {
            let next = basis::read_config(config, &rules::GENERATOR_STORAGE_CONFIG, None);
            loaded.1 = Some(next);
        }
        _ => return,
    }
    let defaults = (rules::GENERATORS_CONFIG.defaults(), rules::GENERATOR_STORAGE_CONFIG.defaults());
    let (generators, storage) = (loaded.0.as_ref().unwrap_or(&defaults.0), loaded.1.as_ref().unwrap_or(&defaults.1));
    *HEAT_CONFIG.write().unwrap_or_else(|error| error.into_inner()) = rules::HeatConfig::of(generators, storage);
}
/// A marca que o tick do gerador deixa em `ModBlockEntity.data` (que o Horizon não salva): o tile é o mesmo de antes. Um tile novo, ou lido do disco, nasce sem ela.
const TILE_TICKED: i32 = 1;
const NETHER: &str = "minecraft:the_nether";
const LAVA_BLOCK: &str = "minecraft:lava";
const HEAT_KEY: &str = "heat";
/// `SerializationConstants.ACTIVE_STATE` e `UPDATE_DELAY`: o `currentActive` e o `updateDelay` que `TileEntityMekanism` salva.
const ACTIVE_STATE: &str = "active_state";
/// `SerializationConstants.REDSTONE` e `CONTROL_TYPE`: o `redstone` que o último aviso dos vizinhos deixou (`TileEntityMekanism.java:775`) e o
/// modo de controle (`writeSustainedData`).
const REDSTONE: &str = "redstone";
const CONTROL_TYPE: &str = "control_type";
/// O nome com que o mod diz à `machinery_basis` quais blocos têm `redstone`.
const REDSTONE_OWNER: &str = "mekanismgenerators";
const UPDATE_DELAY: &str = "update_delay";
/// `SerializationConstants.CURRENT_REDSTONE`: o nível que o comparador lê, salvo porque o Heat Generator tem `AttributeComparator`.
const CURRENT_REDSTONE: &str = "current_redstone";
const LIT: &str = "lit";

// A tela, nas posições de GuiHeatGenerator. O Mekanism Generators usa os sprites do Mekanism, como o original.
const WIDTH: f32 = 176.0;
const HEIGHT: f32 = 166.0;
const SLOT: &str = "mekanism:gui/sprites/slot/normal";
const PANEL: basis::PanelStyle = basis::PanelStyle {
    base: "mekanism:gui/sprites/base",
    base_slice: rules::PANEL_SLICE,
    slot: SLOT,
};
const BAR_STYLE: basis::BarStyle = basis::BarStyle {
    frame: "mekanism:gui/sprites/bar/base",
    frame_slice: rules::BAR_FRAME_SLICE,
};
const POWER_BAR_SPRITE: &str = "mekanism:gui/sprites/bar/vertical_power";
/// `GaugeType.WIDE` com `GaugeInfo.STANDARD`: a moldura `gauge/normal` e o overlay `gauge/overlay/wide`.
const GAUGE_STYLE: basis::GaugeStyle = basis::GaugeStyle {
    frame: "mekanism:gui/sprites/gauge/normal",
    frame_slice: rules::GAUGE_FRAME_SLICE,
    overlay: "mekanism:gui/sprites/gauge/overlay/wide",
};
/// Os elementos do tamanho do `GuiElement`: o medidor largo é 64 por 48 mais a moldura, e a barra soma dois ao que o Java passa.
const LAVA_GAUGE: [f32; 4] = [55.0, 18.0, 66.0, 50.0];
/// `Tooltip.MAX_WIDTH`: onde `Tooltip.create` quebra o texto.
const TOOLTIP_WRAP: i32 = 170;
const ENERGY_BAR: [f32; 4] = [164.0, 15.0, 6.0, 54.0];
/// `MekanismRenderer.color(lava)`: a lava não tem cor de fluido, e o sprite sai como é.
const LAVA_TINT: u32 = 0xFF_FF_FF_FF;

/// O sprite da moldura de um slot (`SlotType.texture`). Só os tipos que esta tela usa têm arquivo.
fn slot_sprite(slot: SlotType) -> &'static str {
    match slot {
        SlotType::Power => "mekanism:gui/sprites/slot/power",
        _ => SLOT,
    }
}

/// O sprite que vai por cima da moldura (`SlotOverlay.texture`). Só o do slot de energia tem arquivo.
fn overlay_sprite(overlay: SlotOverlay) -> Option<&'static str> {
    (overlay == SlotOverlay::Power).then_some("mekanism:gui/sprites/slot/overlay/power")
}

fn is_generator(block: &str) -> bool {
    block == rules::HEAT_GENERATOR
}

/// O nome do tile é o da `machinery_basis` (`basis::name_*`): o componente inteiro, só para o bloco daqui.
fn is_generator_id(id: &str) -> bool {
    is_generator(id)
}

/// `BlockTile.animateTick` do Heat Generator (`GeneratorsBlockTypes.java:69`), pela injeção `animate_tick` da `machinery_basis`.
fn heat_generator_animator(
    engine: &mut horizon_client::particles::ParticleEngine,
    ctx: &horizon_client::animate_tick::AnimateContext<'_>,
    state: BlockStateId,
    pos: [i32; 3],
) {
    basis::animate_active_block(engine, ctx, state, pos, &|block| (block == rules::HEAT_GENERATOR).then_some(&rules::HEAT_GENERATOR_FX[..]));
}

/// O `tick` do cliente: registra o animador do Heat Generator (registrar de novo não faz nada) e toca o som em laço dos geradores acesos
/// (`TileEntityMekanism.tickClient` -> `updateSound`).
pub(in crate::game) fn client_tick(game: &mut Game<'_>) {
    horizon_client::animate_tick::machinery_animate::register("mekanismgenerators", heat_generator_animator);
    if !game.native_window() {
        return;
    }
    let dimension = game.native_dimension();
    let (types, data) = (upgrade::current_types(), upgrade_data(game));
    let muffling_max = types.get(rules::MUFFLING_UPGRADE).map(|kind| kind.max);
    let supports = data.supports(rules::MUFFLING_UPGRADE);
    let tiles: Vec<crate::game::machinery_tile_sound::SoundTile> = game
        .world
        .block_entities
        .iter()
        .filter_map(|(&(x, y, z), entity)| {
            let BlockEntity::Mod(entity) = entity else {
                return None;
            };
            if !is_generator(&entity.id) {
                return None;
            }
            let pos = [x, y, z];
            let muffling = muffling_max.filter(|_| supports).map(|max| (crate::game::machinery_tile_sound::muffling_count(dimension, pos), max));
            Some(crate::game::machinery_tile_sound::SoundTile {
                key: (dimension, pos),
                // `canPlaySound()` é `getActive()`, que no cliente é o estado `lit` do bloco.
                lit: game.native_block_property(pos, LIT).as_deref() == Some("true"),
                fully_muffled: machinery_basis_core::sound::fully_muffled(muffling),
                volume_factor: machinery_basis_core::sound::volume_factor(muffling),
                // `HolidayManager.filterSound`: o som do Heat Generator não está entre os que o feriado troca, então vale o próprio.
                sound: rules::HEAT_GENERATOR_SOUND.to_owned(),
            })
        })
        .collect();
    game.mb_tile_sounds(RULES_OWNER, dimension, &tiles, crate::game::machinery_tile_sound::published_sound_config());
}

pub(in crate::game) fn place_block(game: &mut Game<'_>, stack: &ItemStack, pos: [i32; 3]) {
    basis::name_placed(game, stack, pos, &is_generator_id);
    basis::security_placed(game, stack, pos, &is_generator_id);
    if is_generator(&game.native_block(pos)) {
        // `BlockMekanism.setPlacedBy` -> `onAdded` -> `updatePower`: só o bloco posto com um item lê o sinal ao nascer.
        basis::on_added(game, pos);
        // `applyImplicitComponents` -> `copyToTile`: o item traz os upgrades, os slots, a lava e o calor.
        basis::content_placed(game, stack, pos, &content_spec());
        // O item traz a energia, limitada à capacidade do gerador.
        basis::energy_placed(game, stack, pos, rules::ENERGY_COMPONENT, heat_config().energy_capacity);
        // O tile novo guarda `currentRedstoneLevel = 0`: um comparador que olhe agora não lê o nível de um gerador que esteve ali antes.
        COMPARATOR_LEVELS.with_borrow_mut(|levels| levels.insert((game.native_dimension(), pos), 0));
    }
}

pub(in crate::game) fn loot_drops(game: &Game<'_>, context: &horizon_client::mod_api::NativeLootDrops<'_>, drops: &mut [ItemStack]) {
    basis::name_loot(game, context, drops, &is_generator_id);
    basis::security_loot(game, context, drops, &is_generator_id);
    basis::energy_loot(game, context, drops, rules::ENERGY_COMPONENT, &is_generator_id);
    basis::content_loot(game, context, drops, &content_spec());
}

/// A linha `STORED_ENERGY` do tooltip do Heat Generator (`ContainerType.ENERGY`, a capacidade de `generator-storage.heatGenerator`).
pub(in crate::game) fn tooltip(game: &Game<'_>, stack: &ItemStack, lines: &mut Vec<(horizon_client::mod_api::Text, u32)>) {
    if !rules::has_details(&stack.item) {
        return;
    }
    // `ItemTooltipUtils.shouldDisplayDetails`: os detalhes só aparecem com a tecla de detalhes (Left Shift) apertada, e a dica da descrição vem depois deles.
    let details = game.has_shift_down();
    if details && &*stack.item == rules::HEAT_GENERATOR {
        // `OWNER` e `SECURITY`: logo depois do nome, antes dos upgrades.
        let at = lines.len().min(1);
        lines.splice(at..at, basis::security_tooltip_lines(game, stack));
        // `registerComponentAppenderAfter`: os upgrades antes da energia, o fluido e `Inventory` depois.
        lines.extend(basis::content_upgrade_lines(game, stack, rules::enum_color));
        lines.push(basis::stored_energy_line(
            game,
            stack,
            heat_config().energy_capacity,
            &basis::StoredEnergyLine {
                component: rules::ENERGY_COMPONENT,
                label: rules::STORED_ENERGY_LANG,
                label_color: rules::STORED_ENERGY_COLOR,
                arg_color: rules::STORED_ENERGY_ARG_COLOR,
                unit: rules::ENERGY_UNIT_LANG,
                fraction: rules::FRACTION_LANG,
                infinite: rules::INFINITE_LANG,
            },
        ));
        lines.extend(basis::content_stored_lines(game, stack, false, true));
    }
    lines.extend(basis::content_hold_lines(game, details, &format!("description.{}", stack.item.replace([':', '/'], "."))));
}

pub(in crate::game) fn container_opened(game: &mut Game<'_>, player: u128, pos: Option<[i32; 3]>, cancel: &mut bool) {
    if !*cancel
        && let Some(at) = pos
        && is_generator(&game.native_block(at))
        && basis::security_open(game, player, at, &is_generator_id)
    {
        *cancel = true;
        return;
    }
    basis::name_container_opened(game, player, pos, &is_generator_id);
    // `MekanismTileContainer.openInventory`: o jogador recebe os dados do menu.
    if let Some(pos) = pos
        && is_generator(&game.native_block(pos))
    {
        game.mb_container_open(player, pos, rules::HEAT_GENERATOR);
    }
}

pub(in crate::game) fn destroy_block(game: &mut Game<'_>, pos: [i32; 3], canceled: &mut bool) {
    if !*canceled && is_generator(&game.native_block(pos)) && basis::security_break(game, pos, &is_generator_id) {
        *canceled = true;
        return;
    }
    let canceled = *canceled;
    if !canceled && is_generator(&game.native_block(pos)) {
        basis::security_destroying(game, pos, &is_generator_id);
        basis::name_destroying(game, pos, &is_generator_id);
        basis::energy_destroying(game, pos, &is_generator_id);
        basis::content_destroying(game, pos, &content_spec());
    }
}

/// O que o Heat Generator guarda no item (`loot_table/blocks/heat_generator.json`: `mekanism:items`, `fluids`, `heat`, `upgrades`, `redstone_control` e a energia): os dois slots, a lava e o calor.
fn content_spec() -> basis::ContentSpec<'static> {
    basis::ContentSpec { is_ours: &is_generator_id, item_slots: rules::SLOTS_SPEC.len(), collect: &content_collect, apply: &content_apply }
}

/// A lava (`ContainerType.FLUID`, o componente `mekanism:fluids`) e o calor (`ContainerType.HEAT`, `mekanism:heat`, só quando não está na temperatura ambiente, `Mth.equal`).
fn content_collect(game: &Game<'_>, entity: &ModBlockEntity, pos: [i32; 3]) -> Vec<(&'static str, Tag)> {
    use machinery_basis_core::item_content as content;
    let mut out = Vec::new();
    let tank = &basis::load_tanks(entity, &lava_tank())[0];
    let stored = match tank.resource() {
        Some(id) if tank.amount() > 0 => serde_json::json!({"type": {"id": id}, "amount": tank.amount()}),
        _ => serde_json::json!({}),
    };
    if let Some(value) = content::containers_collect(&[stored]) {
        out.push((basis::CONTENT_FLUIDS, basis::value_tag(&value, "")));
    }
    if let (Some(Tag::Double(bits)), Some(ambient)) = (entity.nbt.get(&JavaString::from(HEAT_KEY)), basis::ambient_temperature(game, pos)) {
        let heat = f64::from_bits(*bits);
        let at_ambient = (heat / rules::HEAT_CAPACITY - ambient).abs() < 1.0e-5;
        if let Some(value) = content::heat_collect(heat, rules::HEAT_CAPACITY, at_ambient) {
            out.push((basis::CONTENT_HEAT, basis::value_tag(&value, "")));
        }
    }
    out
}

/// O contrário: a lava e o calor do item entram no tile (`copyToContainer`).
fn content_apply(entity: &mut ModBlockEntity, of: &dyn Fn(&str) -> Option<Tag>) {
    use machinery_basis_core::item_content as content;
    if let Some(fluids) = of(basis::CONTENT_FLUIDS)
        && let Some(list) = content::containers_apply(&basis::tag_value(&fluids), 1)
    {
        let capacity = lava_tank()[0];
        let stored = &list[0];
        let id = stored["type"]["id"].as_str().map(str::to_owned);
        let amount = stored["amount"].as_u64().unwrap_or(0);
        let tank = machinery_basis_core::tank::Tank::restored(id.filter(|_| amount > 0), amount, capacity);
        basis::save_tanks(entity, &[tank]);
    }
    if let Some(heat) = of(basis::CONTENT_HEAT)
        && let (Some(heat), _) = content::heat_apply(&basis::tag_value(&heat))
    {
        entity.nbt.insert(JavaString::from(HEAT_KEY), Tag::Double(heat.to_bits()));
    }
}

pub(in crate::game) fn block_entity(block: &str, entity: &mut Option<(String, Compound)>) {
    if is_generator(block) {
        // Os blocos do `TileEntityMekanism` com `supportsRedstone()`: o jogo os avisa quando um vizinho muda (`basis::neighbor_changed`).
        basis::watch_redstone(REDSTONE_OWNER, is_generator);
        // A janela de upgrades vale desde que o bloco existe (`ItemUpgrade.useOn`), e não só depois de a tela abrir.
        Game::mb_window_register(&WINDOW_SPEC, remove_upgrade, add_upgrades, window_source);
    }
    if is_generator(block) && entity.is_none() {
        *entity = Some((block.to_owned(), Compound::default()));
    }
}

pub(in crate::game) fn ticks(block: &str, ticks: &mut bool) {
    if is_generator(block) {
        *ticks = true;
        // O `lavaTank` é uma capability de fluido que os canos mecânicos alcançam (`TileEntityHeatGenerator.java:98-103`).
        crate::game::machinery_resources::fluid_neighbor_register(rules::HEAT_GENERATOR, lava_tank_probe, lava_tank_write);
        // O capacitor de calor é a capability `HEAT` nos seis lados (`OverridingSingleHolder`), que os condutores termodinâmicos alcançam.
        crate::game::machinery_heat::heat_neighbor_register(rules::HEAT_GENERATOR, heat_probe, heat_write);
    }
}

/// O `lavaTank` como a rede de fluido o vê: só lava (`fluidStack.is(FluidTags.LAVA)`), entra de fora e ninguém de fora o esvazia (`VariableCapacityFluidTank.input`).
/// `None` com o tile fora do mundo (está no tick).
fn lava_tank_probe(
    game: &Game<'_>,
    _dimension: &'static str,
    pos: [i32; 3],
) -> Option<machinery_basis_core::transmission::host_resource::ResourceSnapshot<machinery_basis_core::fluid::FluidResource>> {
    use machinery_basis_core::fluid::FluidResource;
    use machinery_basis_core::resource_tank::StackContents;
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(pos[0], pos[1], pos[2])) else {
        return None;
    };
    if !is_generator(&entity.id) {
        return None;
    }
    let capacity = lava_tank();
    let tanks = basis::load_tanks(entity, &capacity);
    let tank = &tanks[0];
    Some(machinery_basis_core::transmission::host_resource::ResourceSnapshot {
        contents: StackContents::new(tank.resource().map(FluidResource::new), tank.amount()),
        capacity: capacity[0],
        accepts: Rc::new(|fluid: &FluidResource| &*fluid.id == rules::LAVA),
        can_insert: true,
        can_extract: false,
        creative: false,
        increment: 1,
    })
}

/// O capacitor de calor como a rede de calor o vê (`heatCapacitor`: capacidade 10, condução 5, isolação 100; visto de baixo é o `DefaultInsulationCapacitor`, sem isolação).
/// `None` com o tile fora do mundo (está no tick).
fn heat_probe(game: &Game<'_>, _dimension: &'static str, pos: [i32; 3], side: Face) -> Option<HostCapacitor> {
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(pos[0], pos[1], pos[2])) else {
        return None;
    };
    if !is_generator(&entity.id) {
        return None;
    }
    // `BasicHeatCapacitor.getHeat`: um calor que nunca foi salvo parte do ar do bloco.
    let ambient = AmbientCache::default().get(AmbientCache::SELF_INDEX, || basis::ambient_temperature(game, pos));
    let capacitor = load_heat(entity, ambient);
    Some(HostCapacitor {
        temperature: capacitor.temperature(),
        heat_capacity: capacitor.capacity,
        inverse_conduction: capacitor.inverse_conduction,
        inverse_insulation: if side == Face::Down { rules::INSULATION_BY_FACE[Face::Down as usize] } else { capacitor.inverse_insulation },
    })
}

/// `handleHeat(transfer, transaction)` que a rede de calor fez no capacitor do gerador.
fn heat_write(game: &mut Game<'_>, _dimension: &'static str, pos: [i32; 3], transfer: f64) {
    let ambient = AmbientCache::default().get(AmbientCache::SELF_INDEX, || basis::ambient_temperature(game, pos));
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&(pos[0], pos[1], pos[2])) else {
        return;
    };
    if !is_generator(&entity.id) {
        return;
    }
    let mut capacitor = load_heat(entity, ambient);
    capacitor.handle(transfer);
    entity.nbt.insert(JavaString::from(HEAT_KEY), Tag::Double(capacitor.heat.to_bits()));
    game.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
}

/// O que a rede mudou no `lavaTank`.
fn lava_tank_write(
    game: &mut Game<'_>,
    _dimension: &'static str,
    pos: [i32; 3],
    contents: machinery_basis_core::resource_tank::StackContents<machinery_basis_core::fluid::FluidResource>,
) {
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&(pos[0], pos[1], pos[2])) else {
        return;
    };
    let capacity = lava_tank();
    let mut tanks = basis::load_tanks(entity, &capacity);
    tanks[0] = Tank::restored(contents.resource.map(|fluid| fluid.id.to_string()), contents.amount, capacity[0]);
    basis::save_tanks(entity, &tanks);
    game.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
}

pub(in crate::game) fn energy(block: &str, energy: &mut Option<ModEnergyBuffer>) {
    if is_generator(block) {
        // `withEnergyStorage(generator-storage.heatGenerator)`: o `ModEnergyBuffer` do Horizon é `i32`.
        let energy_capacity = basis::energy_as_int(heat_config().energy_capacity, "a capacidade do gerador");
        *energy = Some(ModEnergyBuffer {
            capacity: energy_capacity,
            max_extract: energy_capacity,
            extract_faces: [true; 6],
            ..Default::default()
        });
    }
}

pub(in crate::game) fn container(block: &str, container: &mut Option<ModContainer>) {
    if is_generator(block) {
        // Todo caminho que abre o menu (o servidor que monta o menu com os cinco slots a mais, o cliente que os põe na tela) passa por aqui antes.
        declare_armor_bar(rules::MENU, rules::armor_bar_declaration);
        machinery_basis_core::screen_elements::declare_once(rules::MENU, rules::screen_elements);
        // As faces, as entradas e as saídas são da instância (H2, decisão 94): quem responde por elas é `ITEM_INSTANCES`.
        basis::register_item_instances(RULES_OWNER, ITEM_INSTANCES);
        basis::register_item_energy(RULES_OWNER, ENERGY_ITEMS);
        *container = Some(ModContainer {
            slots: rules::MENU_SLOTS,
            menu: Some(rules::MENU.to_owned()),
            ..Default::default()
        });
    }
}

pub(in crate::game) fn screen(block: &str, screen: &mut Option<NativeScreen>) {
    if is_generator(block) {
        declare_armor_bar(rules::MENU, rules::armor_bar_declaration);
        machinery_basis_core::screen_elements::declare_once(rules::MENU, rules::screen_elements);
        // Os slots virtuais da janela de upgrades vêm depois dos do gerador e ficam escondidos até a janela abrir.
        let mut slots: Vec<Option<[f32; 2]>> = rules::SLOTS_SPEC.iter().map(|slot| Some(slot.item)).collect();
        slots.extend([None, None]);
        *screen = Some(NativeScreen {
            width: WIDTH,
            height: HEIGHT,
            slots,
            inventory_y: 84.0,
            inventory_x: 8.0,
            own_labels: true,
            own_background: true,
        });
    }
}

fn burn_time(game: &Game<'_>, stack: &ItemStack) -> i32 {
    game.res.providers().burn_time(world_items!(game), stack, false)
}

/// `itemType.has(DataComponents.COOKING_FUEL)` (`TileEntityHeatGenerator.java:112`): o catálogo de itens do jogo guarda o componente em `fuel`.
fn is_fuel(game: &Game<'_>, stack: &ItemStack) -> bool {
    game.items().get(&stack.item).is_some_and(|info| info.fuel.is_some())
}

/// `FluidInventorySlot.canFill(lavaTank, item)`: o item tem um fluido que o tanque (só lava) aceita. O único item portado assim é o balde de lava.
fn fills_tank(stack: &ItemStack) -> bool {
    &*stack.item == LAVA_BUCKET
}

fn lava_bucket_burn_time(game: &Game<'_>) -> i32 {
    let bucket = ItemStack {
        item: LAVA_BUCKET.into(),
        count: 1,
        damage: 0,
        extra: None,
    };
    burn_time(game, &bucket)
}

/// `FluidFuelInventorySlot.fillOrBurn`: um balde enche o tanque, e o resto queima em lava.
fn fill_or_burn(game: &Game<'_>, entity: &mut ModBlockEntity, tank: &mut Tank) {
    let Some(stack) = entity.items[FUEL_SLOT].clone() else {
        return;
    };
    if &*stack.item == LAVA_BUCKET {
        if let Some(lava) = rules::fuel_to_tank(rules::FuelItem::LavaBucket, tank.room()) {
            tank.insert(rules::LAVA, lava, Action::Execute);
            let mut empty = stack;
            empty.item = EMPTY_BUCKET.into();
            entity.items[FUEL_SLOT] = Some(empty);
        }
        return;
    }
    let solid = rules::FuelItem::Solid {
        burn_time: burn_time(game, &stack),
        lava_bucket_burn_time: lava_bucket_burn_time(game),
    };
    if let Some(lava) = rules::fuel_to_tank(solid, tank.room()) {
        tank.insert(rules::LAVA, lava, Action::Execute);
        let mut left = stack;
        left.count -= 1;
        entity.items[FUEL_SLOT] = (left.count > 0).then_some(left);
    }
}

fn lava_faces(game: &Game<'_>, pos: [i32; 3]) -> u32 {
    Face::ALL
        .iter()
        .filter(|face| {
            let [x, y, z] = face.offset();
            game.native_block([pos[0] + x, pos[1] + y, pos[2] + z]) == LAVA_BLOCK
        })
        .count() as u32
}

/// O cliente só tem a block entity, e o que ela desenha, se o servidor mandar uma cópia.
pub(in crate::game) fn update_tag(entity: &ModBlockEntity, tag: &mut Option<Compound>) {
    if is_generator(&entity.id) {
        *tag = Some(Compound::default());
    }
}

fn load_heat(entity: &ModBlockEntity, ambient: f64) -> Capacitor {
    let mut capacitor = rules::new_capacitor(ambient);
    if let Some(Tag::Double(bits)) = entity.nbt.get(&JavaString::from(HEAT_KEY)) {
        capacitor.heat = f64::from_bits(*bits);
    }
    capacitor
}

/// O que `TileEntityMekanism.loadAdditional` lê (`redstone`, o modo de controle, `currentActive` e `updateDelay`), com o estado `lit` que o
/// bloco tem no mundo, e o `redstoneLastTick` que o tile tinha (o Java o guarda no objeto, e não o salva).
fn load_tile(entity: &ModBlockEntity, block_active: bool, was_powered: bool) -> TileBase {
    let redstone = matches!(entity.nbt.get(&JavaString::from(REDSTONE)), Some(Tag::Byte(value)) if *value != 0);
    let control_type = match entity.nbt.get(&JavaString::from(CONTROL_TYPE)) {
        Some(Tag::String(name)) => RedstoneControl::from_serialized_name(&name.to_string_lossy()),
        _ => RedstoneControl::Disabled,
    };
    let active = matches!(entity.nbt.get(&JavaString::from(ACTIVE_STATE)), Some(Tag::Byte(value)) if *value != 0);
    let update_delay = match entity.nbt.get(&JavaString::from(UPDATE_DELAY)) {
        Some(Tag::Int(value)) => *value,
        _ => 0,
    };
    let current_redstone = match entity.nbt.get(&JavaString::from(CURRENT_REDSTONE)) {
        Some(Tag::Int(value)) => *value,
        _ => 0,
    };
    let mut tile = TileBase::new(machinery_basis_core::settings::ConfigValue::Live(machinery_basis_core::settings::block_deactivation_delay));
    let saved = SavedState {
        redstone,
        control_type,
        active,
        update_delay,
        current_redstone,
        ..Default::default()
    };
    tile.restore(&saved, block_active);
    tile.set_redstone_last_tick(was_powered);
    tile
}

/// `saveAdditional` do `TileEntityMekanism`: `redstone`, o modo de controle, `currentActive` e `updateDelay`.
fn save_tile(entity: &mut ModBlockEntity, tile: &TileBase) {
    entity.nbt.insert(JavaString::from(REDSTONE), Tag::Byte(i8::from(tile.is_powered())));
    entity.nbt.insert(JavaString::from(CONTROL_TYPE), Tag::String(JavaString::from(tile.control_type().serialized_name())));
    entity.nbt.insert(JavaString::from(ACTIVE_STATE), Tag::Byte(i8::from(tile.active())));
    entity.nbt.insert(JavaString::from(UPDATE_DELAY), Tag::Int(tile.update_delay()));
    entity.nbt.insert(JavaString::from(CURRENT_REDSTONE), Tag::Int(tile.current_redstone_level()));
}

// ---------------------------------------------------------------------------------------------------------------------------
// Os upgrades (`TileComponentUpgrade`). O gerador não tem objeto vivo: o componente é refeito do NBT (`component_upgrade`) e dos dois
// slots virtuais do menu a cada tick, avança, e volta para os dois. O que o Java guarda só na memória (`upgradeTicks` e
// `canCheckUpgrades`) fica numa tabela por thread, uma entrada por gerador.

/// Quando o item não é conhecido, a pilha dele.
const UNKNOWN_STACK: u32 = 64;
/// De quanto em quanto tempo a tabela é varrida, e quanto sem andar uma entrada vive.
const SWEEP_TICKS: i64 = 200;
const STALE_TICKS: i64 = 100;

/// A janela de upgrades do gerador: só dados. A mecânica (aba, janela, slots virtuais, o item de upgrade na mão) é da machinery_basis.
static WINDOW_SPEC: UpgradeWindowSpec = UpgradeWindowSpec {
    block: rules::HEAT_GENERATOR,
    upgrades: true,
    recipe_book: false,
    book: None,
    book_slots: [0, 0],
    virtual_slots: [rules::UPGRADE_SLOT, rules::UPGRADE_OUTPUT_SLOT],
    trackers: rules::heat_generator_trackers,
    window_trackers: rules::upgrade_window_data,
    ns: "mekanism",
    window_type: rules::UPGRADE_WINDOW,
    window_types: &rules::WINDOW_TYPES,
    base_slice: rules::PANEL_SLICE,
    max_multiplier: machinery_basis_core::settings::max_upgrade_multiplier,
    client_config: "Mekanism/client.toml",
    data: window_upgrade_data,
    color: rules::enum_color,
    item: rules::UPGRADE_ITEM,
    item_component: rules::UPGRADE_TYPE_COMPONENT,
};

fn window_upgrade_data() -> Rc<upgrade::UpgradeData> {
    rules::upgrade_data(&upgrade::current_types(), &upgrade::current_tags(), UNKNOWN_STACK)
}

/// Os dados de upgrade desta carga: os tipos, as tags e o limite de pilha do item.
fn upgrade_data(game: &Game<'_>) -> Rc<upgrade::UpgradeData> {
    let stack = game.native_max_stack_size(rules::UPGRADE_ITEM).unwrap_or(UNKNOWN_STACK);
    rules::upgrade_data(&upgrade::current_types(), &upgrade::current_tags(), stack)
}

/// O Heat Generator como o `MekanismContainer` o lê: o estado do fim do tick (o objeto do Java está vivo, o port o refaz a cada tick) e o que
/// o tick produziu.
struct View {
    state: rules::State,
    last: rules::Produced,
    upgrades: UpgradeSync,
}

/// O que o gerador guarda do componente só na memória.
struct UpgradeMemory {
    transient: UpgradeTransient,
    /// O último tick em que o gerador andou: um tick que não é o seguinte é um objeto novo no Java (o chunk saiu, o mundo foi reaberto).
    last_seen: i64,
    /// `redstoneLastTick`: o tile do Java o guarda no objeto (`TileEntityMekanism.java:653-655`), e aqui o tile é refeito do NBT a cada tick.
    was_powered: bool,
    /// `TileEntityMekanism.ambientTemperature` (`CachedAmbientTemperature`): o ar de cada face e do bloco, lido do mundo na primeira vez e
    /// guardado enquanto o objeto vive. Um tile novo (o mesmo `last_seen` de cima) lê de novo; o bioma que muda por baixo não o muda.
    ambient: AmbientCache,
    /// O que o último tick deixou para o `broadcastChanges` ler (o objeto vivo do Java): o menu mostra o estado do fim do tick.
    view: Option<View>,
    /// Quantos upgrades de silêncio o cliente já sabe que o tile tem (`MUFFLING_COUNT` do pacote de atualização).
    /// O que o servidor sabe da contagem de upgrades de silêncio e o que o cliente sabe: só a subida avisa o cliente (`MufflingSync`).
    muffling: machinery_basis_core::sound::MufflingSync,
    /// A lava que o tanque tinha no fim do último tick: o `makesComparatorDirty` do Heat Generator é o tanque de fluido (`TileEntityHeatGenerator.java:236-238`),
    /// então o comparador só relê quando a lava mudou desde a última leitura (`TileEntityMekanism.java:645`). `None` num objeto novo: o Java
    /// também nasce sem a marca (`updateComparators = false`) e só uma mudança dentro do primeiro tick a acende.
    lava_seen: Option<u64>,
}

impl UpgradeMemory {
    fn new() -> Self {
        Self {
            transient: UpgradeTransient::default(),
            last_seen: i64::MIN,
            was_powered: false,
            ambient: AmbientCache::default(),
            view: None,
            // Um tile novo avisa o cliente na primeira vez, mesmo com zero: o `getUpdateTag` do Java sempre leva a contagem.
            muffling: Default::default(),
            lava_seen: None,
        }
    }
}

thread_local! {
    static UPGRADES: RefCell<HashMap<(&'static str, [i32; 3]), UpgradeMemory>> = RefCell::new(HashMap::new());
    /// `currentRedstoneLevel` de cada gerador que já andou, o que `getAnalogOutputSignal` devolve. O tile está fora do mundo durante o próprio tick, e
    /// um comparador pode perguntar nessa hora.
    static COMPARATOR_LEVELS: RefCell<HashMap<(&'static str, [i32; 3]), i32>> = RefCell::new(HashMap::new());
    static LAST_SWEEP: Cell<i64> = const { Cell::new(i64::MIN) };
}

/// Um mundo antigo tem menos slots salvos que o menu de agora (os dois virtuais vêm depois dos do gerador).
fn ensure_slots(entity: &mut ModBlockEntity) {
    if entity.items.len() < rules::MENU_SLOTS {
        entity.items.resize(rules::MENU_SLOTS, None);
    }
}

/// O valor vivo dos dados do menu do Heat Generator em `pos` (`MekanismContainer.broadcastChanges` e `sendAllDataToRemote`): o estado do fim do
/// último tick do gerador (o objeto do Java está sempre vivo, o port o refaz a cada tick), com a energia que o buffer guarda agora, porque
/// uma extração de fora do tick a muda, e o `redstone` que um aviso deixou depois do tick (`updatePower` troca o campo na hora: o valor vai
/// também para o NBT, de onde o tick seguinte refaz o tile). Até o primeiro tick do tile (a marca em `entity.data`, que um tile novo ou
/// lido do disco não tem) o Java já tem o tile carregado do NBT (`loadAdditional`): o menu lê o estado que o NBT dá, sem produção nem
/// perdas (`menu_source`). Devolve se havia estado para ler.
fn window_source(game: &mut Game<'_>, pos: [i32; 3], act: &mut dyn FnMut(&dyn TrackedSource)) -> bool {
    let at = (pos[0], pos[1], pos[2]);
    let ticked = match game.world.block_entities.get(&at) {
        Some(BlockEntity::Mod(entity)) if is_generator(&entity.id) => !entity.data.is_empty(),
        _ => return false,
    };
    let key = (game.native_dimension(), pos);
    let power = basis::take_power(game, pos);
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&at) else {
        return false;
    };
    if let Some(powered) = power {
        entity.nbt.insert(JavaString::from(REDSTONE), Tag::Byte(i8::from(powered)));
    }
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&at) else {
        return false;
    };
    let energy = basis::stored_energy(entity);
    let security = basis::security_sync(game, entity);
    let mut loaded = None;
    UPGRADES.with_borrow_mut(|tiles| {
        let live = tiles.get_mut(&key).and_then(|memory| memory.view.as_mut());
        let Some(view) = menu_source::object(ticked, live, &mut loaded, || loaded_view(game, pos, entity)) else {
            return false;
        };
        if ticked && let Some(powered) = power {
            view.state.tile.update_power(powered);
        }
        let mut state = view.state.clone();
        state.energy = energy;
        act(&rules::HeatGeneratorSync { state: &state, last: &view.last, upgrades: &view.upgrades, security });
        true
    })
}

/// O gerador como o `loadAdditional` o deixa, antes do primeiro tick: o calor, a lava, a energia, o `TileEntityMekanism` e os upgrades do
/// NBT; o que o tick produz (`productionRate`, as duas perdas de calor) não é salvo e vale zero, e o `redstoneLastTick` começa falso. O
/// `loadAdditional` não lê o ar: o calor salvo entra com `setHeat`, e só um calor que nunca foi salvo parte do ar do bloco
/// (`BasicHeatCapacitor.getHeat`, inicializado preguiçosamente), que é a mesma leitura do tick (`AmbientCache`, `SELF_INDEX`). O objeto da tela
/// lê num cache descartável: não preenche nem grava o `ambient` do objeto vivo (a tela não chama `getAmbientTemperature`).
fn loaded_view(game: &Game<'_>, pos: [i32; 3], entity: &ModBlockEntity) -> View {
    let ambient = AmbientCache::default().get(AmbientCache::SELF_INDEX, || basis::ambient_temperature(game, pos));
    let block_active = game.native_block_property(pos, LIT).as_deref() == Some("true");
    let tanks = basis::load_tanks(entity, &lava_tank());
    let state = rules::State {
        heat: load_heat(entity, ambient),
        lava: tanks[0].amount(),
        energy: basis::stored_energy(entity),
        tile: load_tile(entity, block_active, false),
        config: heat_config(),
    };
    let component = upgrade::rebuild(upgrade_data(game), &basis::read_upgrades(game, entity), None, UpgradeTransient::default());
    View { state, last: rules::Produced::default(), upgrades: UpgradeSync::of(&component) }
}

/// Os dois slots virtuais como o menu os tem, a entrada e a saída.
fn menu_contents(game: &Game<'_>, entity: &ModBlockEntity) -> [machinery_basis_core::containers::SlotContents; 2] {
    [rules::UPGRADE_SLOT, rules::UPGRADE_OUTPUT_SLOT].map(|index| basis::slot_contents(game, entity.items.get(index).and_then(Option::as_ref)))
}

/// `TileComponentUpgrade.setUpgrades` → `sendUpdatePacket`: o cliente só quer saber quantos upgrades de silêncio há, e só é avisado quando um upgrade ENTRA. `removeUpgrade`
/// não chama `sendUpdatePacket` (`TileComponentUpgrade.java:167-189`): o cliente segue com a contagem velha até um pacote do bloco (o `lit` que muda, em `tick`). A contagem
/// verdadeira vai à tabela do servidor, que quem entra depois recebe.
fn sync_muffling(game: &mut Game<'_>, dimension: &'static str, pos: [i32; 3], memory: &mut UpgradeMemory, component: &UpgradeComponent) {
    let step = memory.muffling.tick(component.get_upgrades(rules::MUFFLING_UPGRADE));
    if let Some(server) = step.server {
        game.mb_muffling_server(dimension, pos, server);
    }
    if let Some(send) = step.send {
        game.mb_muffling_changed(dimension, pos, send);
    }
}

/// O primeiro passo do tick do tile (`TileEntityMekanism.tickServer`: o componente anda antes de tudo). Devolve o que a janela de upgrades
/// mostra depois dele, o `redstoneLastTick` que o tile tinha e o ar que o objeto já leu do mundo.
fn tick_upgrades(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) -> (UpgradeSync, bool, AmbientCache) {
    ensure_slots(entity);
    let now = game.world.game_time;
    let dimension = game.native_dimension();
    let key = (dimension, pos);
    // Um bloco novo na mesma posição não herda nada: sem a chave `active_state` (o Java a grava sempre), nasce outra entrada.
    let saved_before = entity.nbt.contains_key(&JavaString::from(ACTIVE_STATE));
    let remembered = UPGRADES
        .with_borrow_mut(|tiles| tiles.remove(&key))
        .filter(|memory| memory.last_seen + 1 == now && saved_before);
    // No tick em que o objeto nasce o NBT do componente manda; depois, o que o menu tem nos dois slots virtuais.
    let fresh = remembered.is_none();
    let mut memory = remembered.unwrap_or_else(UpgradeMemory::new);
    let nbt = basis::read_upgrades(game, entity);
    let menu = (!fresh).then(|| menu_contents(game, entity));
    let component = upgrade::rebuild(upgrade_data(game), &nbt, menu.as_ref(), memory.transient.clone());
    component.tick_server();
    let saved = component.saved();
    if saved != nbt {
        basis::write_upgrades(entity, &saved);
    }
    basis::upgrade_to_cell(game, rules::UPGRADE_TYPE_COMPONENT, &mut entity.items[rules::UPGRADE_SLOT], &saved.input);
    basis::upgrade_to_cell(game, rules::UPGRADE_TYPE_COMPONENT, &mut entity.items[rules::UPGRADE_OUTPUT_SLOT], &saved.output);
    memory.transient = component.transient();
    sync_muffling(game, dimension, pos, &mut memory, &component);
    let sync = UpgradeSync::of(&component);
    let was_powered = memory.was_powered;
    let ambient = memory.ambient;
    memory.last_seen = now;
    UPGRADES.with_borrow_mut(|tiles| {
        tiles.insert(key, memory);
        if now.saturating_sub(LAST_SWEEP.get()) >= SWEEP_TICKS {
            LAST_SWEEP.set(now);
            tiles.retain(|_, memory| now - memory.last_seen <= STALE_TICKS);
            COMPARATOR_LEVELS.with_borrow_mut(|levels| levels.retain(|key, _| tiles.contains_key(key)));
        }
    });
    (sync, was_powered, ambient)
}

/// Faz `act` no componente de upgrades do gerador em `pos` fora do tick (o pacote de remoção, o upgrade na mão), e guarda o que ele
/// deixou no NBT, nos dois slots do menu e na memória. O menu manda sobre o NBT: o tick seguinte o lê primeiro.
fn act_upgrades<R>(game: &mut Game<'_>, pos: [i32; 3], act: impl FnOnce(&UpgradeComponent) -> R) -> Option<R> {
    let dimension = game.native_dimension();
    let key = (dimension, pos);
    let at = (pos[0], pos[1], pos[2]);
    let (nbt, menu) = match game.world.block_entities.get(&at) {
        Some(BlockEntity::Mod(entity)) if is_generator(&entity.id) => (basis::read_upgrades(game, entity), menu_contents(game, entity)),
        _ => return None,
    };
    let mut memory = UPGRADES.with_borrow_mut(|tiles| tiles.remove(&key));
    let transient = memory.as_ref().map_or_else(UpgradeTransient::default, |memory| memory.transient.clone());
    let component = upgrade::rebuild(upgrade_data(game), &nbt, Some(&menu), transient);
    let result = act(&component);
    let saved = component.saved();
    // As células novas dos dois slots, calculadas com o jogo ainda emprestado só para leitura.
    let cells = match game.world.block_entities.get(&at) {
        Some(BlockEntity::Mod(entity)) => [(rules::UPGRADE_SLOT, &saved.input), (rules::UPGRADE_OUTPUT_SLOT, &saved.output)].map(|(index, contents)| {
            let cell = entity.items.get(index).and_then(Option::as_ref);
            basis::upgrade_cell_after(game, rules::UPGRADE_TYPE_COMPONENT, cell, contents)
        }),
        _ => [None, None],
    };
    if let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&at) {
        ensure_slots(entity);
        if saved != nbt {
            basis::write_upgrades(entity, &saved);
        }
        for (index, cell) in [rules::UPGRADE_SLOT, rules::UPGRADE_OUTPUT_SLOT].into_iter().zip(cells) {
            if let Some(cell) = cell {
                entity.items[index] = cell;
            }
        }
    }
    game.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
    if let Some(memory) = memory.as_mut() {
        memory.transient = component.transient();
        sync_muffling(game, dimension, pos, memory, &component);
        // O objeto do Java é um só: o `broadcastChanges` vê o mapa novo no tick seguinte, não depois do próximo tick do tile.
        if let Some(view) = memory.view.as_mut() {
            view.upgrades = UpgradeSync::of(&component);
        }
    }
    if let Some(memory) = memory {
        UPGRADES.with_borrow_mut(|tiles| tiles.insert(key, memory));
    }
    Some(result)
}

/// `PacketRemoveUpgrade.handle`: o servidor tira um ou todos os upgrades do tipo do gerador em `pos`.
fn remove_upgrade(game: &mut Game<'_>, pos: [i32; 3], upgrade: &str, all: bool) {
    act_upgrades(game, pos, |component| component.remove_upgrade(upgrade, all));
}

/// `TileComponentUpgrade.addUpgrades`, que `ItemUpgrade.useOn` (na `machinery_basis`) chama depois de conferir o suporte e o shift:
/// quantos upgrades do tipo entraram no gerador em `pos`, de até `available`.
fn add_upgrades(game: &mut Game<'_>, pos: [i32; 3], upgrade: &str, available: i32) -> i32 {
    act_upgrades(game, pos, |component| component.add_upgrades(upgrade, available)).unwrap_or(0)
}

/// `notifyComparatorChange` (`Level.updateNeighbourForOutputSignal`) quando `tickServer` mudou o nível, mais o nível que `getAnalogOutputSignal`
/// devolve. O aviso sai dentro do tick do gerador, como no Java: uma máquina vizinha que tica depois já o vê.
fn publish_comparator(game: &mut Game<'_>, dimension: &'static str, pos: [i32; 3], level: i32, changed: bool) {
    COMPARATOR_LEVELS.with_borrow_mut(|levels| levels.insert((dimension, pos), level));
    if changed {
        game.native_update_output_neighbors(dimension, pos);
        basis::on_neighbor_change(game, dimension, pos);
    }
}

/// `hasAnalogOutputSignal`: o Heat Generator tem `AttributeComparator` (`Generator.java:15`).
pub(in crate::game) fn has_analog_output(level: &GameLevel<'_>, state: BlockStateId, output: &mut bool) {
    if level.native_state_block(state).is_some_and(is_generator) {
        *output = true;
    }
}

/// `getAnalogOutputSignal`: o `currentRedstoneLevel` do tile.
pub(in crate::game) fn analog_output(dimension: &'static str, pos: [i32; 3], output: &mut i32) {
    if let Some(level) = COMPARATOR_LEVELS.with_borrow(|levels| levels.get(&(dimension, pos)).copied()) {
        *output = level;
    }
}

pub(in crate::game) fn tick(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    if !is_generator(&entity.id) {
        return;
    }
    basis::name_apply_pending(entity, pos);
    basis::security_apply_pending(entity, pos);
    // `TileComponentFrequency.tickServer`: a frequência de segurança que o tile leu do NBT é validada pela tabela.
    basis::security_tile_tick(game, pos, entity);
    // A capability de fluido do `lavaTank`: todo lado menos a frente (`RelativeSide.LEFT, RIGHT, BACK, TOP, BOTTOM`).
    let front = basis::front(game, pos);
    crate::game::machinery_resources::fluid_faces_set(game.native_dimension(), pos, &entity.id, Face::ALL.map(|face| face != front));
    basis::content_apply_pending(entity, pos, &content_spec());
    basis::energy_apply_pending(entity, pos, heat_config().energy_capacity);
    // `TileEntityMekanism.tickServer`: o componente de upgrades anda antes do atraso do estado ativo e do `onUpdateServer`.
    let (upgrade_sync, was_powered, mut ambient_cache) = tick_upgrades(game, pos, entity);
    let mut tanks = basis::load_tanks(entity, &lava_tank());
    let tank = &mut tanks[0];
    let lava_before = tank.amount();
    fill_or_burn(game, entity, tank);

    // `CachedAmbientTemperature.getTemperature(side)`: o ar do próprio bloco (o que a eficiência de Carnot e o calor inicial usam) e o do
    // bloco vizinho de cada face (a perda para o ambiente), lidos do mundo na primeira vez e guardados pelo resto da vida do objeto.
    let ambient = ambient_cache.get(AmbientCache::SELF_INDEX, || basis::ambient_temperature(game, pos));
    let mut face_ambient = [0.0; 6];
    for (index, face) in Face::ALL.into_iter().enumerate() {
        let [x, y, z] = face.offset();
        face_ambient[index] = ambient_cache.get(index, || basis::ambient_temperature(game, [pos[0] + x, pos[1] + y, pos[2] + z]));
    }
    let around = rules::Surroundings {
        lava_faces: lava_faces(game, pos) + u32::from(basis::has_logged_lava(game, pos)),
        in_nether: game.native_dimension() == NETHER,
        ambient,
        face_ambient,
    };
    let block_active = game.native_block_property(pos, LIT).as_deref() == Some("true");
    let mut state = rules::State {
        heat: load_heat(entity, ambient),
        lava: tank.amount(),
        energy: basis::stored_energy(entity),
        tile: load_tile(entity, block_active, was_powered),
        config: heat_config(),
    };
    // `updatePower` só roda quando o jogo avisa o bloco (`neighborChanged`, `onNeighborChange`, `onAdded`): o `redstone` não é lido a cada
    // tick. O que o aviso deixou chega aqui, antes de `canFunction()`.
    if let Some(powered) = basis::take_power(game, pos) {
        state.tile.update_power(powered);
    }
    // `TileEntityGenerator.onUpdateServer` (o `super` de `TileEntityHeatGenerator.onUpdateServer`): `EnergyUtils.emit` para o vizinho da frente, só com `canFunction()`, antes de
    // queimar a lava e produzir: sai a energia que o buffer guardava no fim do tick anterior.
    let dimension = game.native_dimension();
    let front = basis::front(game, pos);
    rules::emit_energy(&mut state, &basis::GameEnergy::new(game, dimension, pos), front);
    // `TileEntityHeatGenerator.onUpdateServer`: o slot de energia entrega ao item antes de o combustível entrar, e o `prev` de `producingEnergy` já é o do buffer depois disso.
    drain_energy_slot(game, entity, &mut state);
    // `simulateAdjacent` só pelo lado de baixo (`getAdjacent`): o condutor termodinâmico embaixo recebe o calor, e o capacitor dele confirma no fim do `onUpdateServer`.
    let produced = rules::step_with(&mut state, &around, &mut |capacitor| crate::game::machinery_heat::heat_adjacent(game, pos, Face::Down, capacitor));
    if basis::MEDIR {
        let fuel = entity.items[FUEL_SLOT].as_ref().map_or(("", 0), |stack| {
            (stack.item.split_once(':').map_or(&*stack.item, |(_, path)| path), stack.count.max(0) as u64)
        });
        basis::trace(&rules::trace_line(game.world.game_time, &state, &around, &produced, fuel));
    }
    tank.extract(produced.lava_used, Action::Execute);
    basis::set_stored_energy(entity, state.energy);
    let heat = state.heat;
    entity.nbt.insert(JavaString::from(HEAT_KEY), Tag::Double(heat.heat.to_bits()));
    save_tile(entity, &state.tile);

    // O bloco só muda quando o tile manda: o atraso de `blockDeactivationDelay` o segura aceso depois que o gerador para.
    let lit = state.tile.block_active();
    if lit != block_active {
        game.native_set_block_property(pos, LIT, if lit { "true" } else { "false" });
        // O pacote de dados do bloco que acompanha a mudança de estado (`ChunkHolder.broadcastBlockEntityIfNeeded` chama `getUpdatePacket`, que leva o `MUFFLING_COUNT`) leva ao
        // cliente a contagem verdadeira.
        let dimension = game.native_dimension();
        let count = UPGRADES.with_borrow_mut(|tiles| tiles.get_mut(&(dimension, pos)).and_then(|memory| memory.muffling.block_update()));
        if let Some(count) = count {
            game.mb_muffling_changed(dimension, pos, count);
        }
    }

    // `TileEntityMekanism.tickServer`, o fim (`:645-655`): o comparador relê o tanque de lava se ele mudou desde a última leitura
    // (`makesComparatorDirty`, só o fluido), e avisa os vizinhos dentro deste tick; depois `ticker++` e `redstoneLastTick = redstone`.
    let key = (game.native_dimension(), pos);
    let mut lava_seen = UPGRADES.with_borrow(|tiles| tiles.get(&key).and_then(|memory| memory.lava_seen));
    let comparator_changed = rules::finish_tick(&mut state, lava_before, &mut lava_seen);
    entity.nbt.insert(JavaString::from(CURRENT_REDSTONE), Tag::Int(state.tile.current_redstone_level()));
    publish_comparator(game, key.0, pos, state.tile.current_redstone_level(), comparator_changed);
    // O que o `broadcastChanges` do tick seguinte lê: o estado do fim deste tick, com a energia que o buffer guarda agora.
    let mut view_state = state;
    view_state.lava = tank.amount();
    view_state.energy = basis::stored_energy(entity);
    // O Horizon só guarda o `data` do tile enquanto ele é o mesmo: a marca diz ao menu que o estado de `view` é deste tile (`window_source`).
    if entity.data.is_empty() {
        entity.data = vec![TILE_TICKED];
    }
    UPGRADES.with_borrow_mut(|tiles| {
        if let Some(memory) = tiles.get_mut(&key) {
            memory.lava_seen = lava_seen;
            memory.was_powered = view_state.tile.redstone_sync().was_powered;
            memory.ambient = ambient_cache;
            memory.view = Some(View { state: view_state, last: produced, upgrades: upgrade_sync });
        }
    });
    basis::save_tanks(entity, &tanks);
}

pub(in crate::game) fn screen_draw(
    game: &Game<'_>,
    block: &str,
    batch: &mut HudBatch,
    origin: [f32; 2],
) {
    if !is_generator(block) {
        return;
    }
    let [x, y] = origin;
    let at = |rect: [f32; 4]| [x + rect[0], y + rect[1], rect[2], rect[3]];
    let read = rules::ScreenData::read(&game.mb_mirror(block));

    let title = basis::screen_title(game, "container.mekanismgenerators.heat_generator");
    basis::machine_panel(game, batch, &PANEL, [x, y, WIDTH, HEIGHT], &title);
    draw_armor_bar(game, batch, origin);
    for (index, spec) in rules::SLOTS_SPEC.iter().enumerate() {
        let frame = slot_type(None, index, spec.container).map(slot_sprite);
        basis::slot(game, batch, origin, spec.item, frame, spec.overlay.and_then(overlay_sprite));
    }
    // `GuiMekanismTile.addGenericTabs`: a aba de upgrades vem de `super.addGuiElements()`, antes do medidor e da barra.
    game.mb_window_draw_tab(batch, block, origin);
    // `GuiMekanismTile.addGenericTabs`: a aba de upgrades, a de redstone e a de segurança, nesta ordem.
    let redstone = rules::redstone(&game.mb_mirror(block));
    basis::redstone_draw(game, batch, origin, redstone, WIDTH as i32);
    // `GuiHeatGenerator.addWarningTab`: a aba de avisos à direita, com o aviso de redstone que a aba acima tem.
    basis::redstone_warning_tab_draw(game, batch, origin, redstone, WIDTH as i32, false);
    basis::security_draw(game, batch, origin, &rules::security(&game.mb_mirror(block)), WIDTH as i32, machinery_basis_core::side_tab::SECURITY_TAB_Y);
    // A ordem dos `addRenderableWidget` de `GuiHeatGenerator`: o medidor de lava e depois a barra de energia.
    let lava = read.lava;
    let scaled = gui::gauge_level(lava, heat_config().tank(), LAVA_GAUGE[3] as i32);
    let fill = (lava > 0).then_some((rules::LAVA_SPRITE, rules::LAVA_FRAME, LAVA_TINT));
    basis::gauge(game, batch, &GAUGE_STYLE, at(LAVA_GAUGE), scaled, fill);
    let level = gui::divide_to_level(f64::from(read.energy), f64::from(heat_config().energy_capacity_int()));
    basis::vertical_bar(game, batch, &BAR_STYLE, at(ENERGY_BAR), level, None, &|game, batch, dst| {
        basis::power_bar_contents(game, batch, POWER_BAR_SPRITE, rules::POWER_BAR_TILE, dst);
    });
    // `GuiEnergyTab`, antes do medidor e da barra na ordem do Java; não se sobrepõe a nada, então a ordem não aparece.
    let [tab_x, tab_y, tab_w, tab_h] = rules::ENERGY_TAB.rect;
    game.native_sprite(batch, rules::ENERGY_TAB_SPRITE, [0.0, 0.0, tab_w, tab_h], [x + tab_x, y + tab_y, tab_w, tab_h]);
    // `GuiHeatTab`, o último `addRenderableWidget`: o sprite muda com a unidade de temperatura do cliente (`getResource`).
    let sprite = rules::heat_tab_sprite(basis::temperature_unit(rules::LOCAL_CONFIG_NAMESPACE));
    let [tab_x, tab_y, tab_w, tab_h] = rules::HEAT_TAB.rect;
    game.native_sprite(batch, &sprite, [0.0, 0.0, tab_w, tab_h], [x + tab_x, y + tab_y, tab_w, tab_h]);
}

/// `GuiHeatGenerator.addGuiElements`: o suporte (`GuiSideHolder`) vem antes de `super.addGuiElements()`, que põe os `GuiSlot` de todos os slots do
/// menu, os cinco da armadura e da mão entre eles (`SlotType.NORMAL`, um pixel antes do item). Os itens e os fundos vazios o jogo desenha depois.
fn draw_armor_bar(game: &Game<'_>, batch: &mut HudBatch, origin: [f32; 2]) {
    let [x, y] = origin;
    let [holder_x, holder_y, width, height] = rules::ARMOR_HOLDER;
    for cell in gui::nine_slice_cells(side_tab::HOLDER_SLICE, [x + holder_x, y + holder_y, width, height]) {
        basis::sprite_tinted(game, batch, rules::ARMOR_HOLDER_SPRITE, cell.src, cell.dst, rules::ARMOR_HOLDER_TINT);
    }
    for slot in armor_bar::slots(rules::MENU) {
        basis::slot(game, batch, origin, [slot.x as f32, slot.y as f32], Some(SLOT), None);
    }
}

/// `GuiSecurityTab.onClick` e `GuiRedstoneControlTab.onClick`: os dois botões do mouse nas abas de segurança e de redstone do Heat Generator viram o botão de menu do servidor.
pub(in crate::game) fn screen_click(game: &Game<'_>, block: &str, at: [f32; 2], right: bool, button: &mut Option<i32>) {
    if !is_generator(block) {
        return;
    }
    if let Some(id) = basis::redstone_screen_click(game, WIDTH as i32, at, right) {
        *button = Some(id);
    } else if let Some(id) = basis::security_screen_click(game, WIDTH as i32, machinery_basis_core::side_tab::SECURITY_TAB_Y, at, right) {
        *button = Some(id);
    }
}

/// `PacketGuiInteract.NEXT_SECURITY_MODE` e `PREVIOUS_SECURITY_MODE` (o dono do Heat Generator troca o modo) e `NEXT_REDSTONE_CONTROL` e `PREVIOUS_REDSTONE_CONTROL` (quem tem acesso
/// troca o modo de redstone) no servidor.
pub(in crate::game) fn menu_button(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, id: i32) {
    if !is_generator_id(&entity.id) {
        return;
    }
    if let Some(change) = basis::redstone_menu_button(game, entity, id) {
        if let Some(forward) = change {
            step_control(game, pos, entity, forward);
        }
        return;
    }
    basis::security_menu_button(game, entity, id);
}

/// `setControlType(getControlType().getNext(supportsMode))` (ou `getPrevious`): o modo no NBT, de onde o tick seguinte refaz o tile, e na vista que a tela sincroniza, que o Java
/// já tem mudada na hora.
fn step_control(game: &Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, forward: bool) {
    let key = (game.native_dimension(), pos);
    let next = machinery_basis_core::redstone_tab::cycle(basis::redstone_read(entity), forward);
    basis::redstone_write(entity, next);
    UPGRADES.with_borrow_mut(|tiles| {
        if let Some(view) = tiles.get_mut(&key).and_then(|memory| memory.view.as_mut()) {
            view.state.tile.set_control_type(next);
        }
    });
}

/// O clique na aba de calor (`GuiHeatTab.onClick`), antes de a tela fazer qualquer coisa com ele (hook
/// `screen_mouse_button_pressed_pre`): o esquerdo avança a unidade de temperatura do cliente e o direito volta. A aba toma o clique
/// (`canceled`): não vai ao slot, ao item no cursor nem ao botão, e não toca som nem manda nada ao servidor, porque `GuiHeatTab` não
/// tem `clickSound` (só `GuiInsetElement` tem) e a unidade é do cliente. Uma janela por cima do ponto fica com o clique.
pub(in crate::game) fn heat_tab_press(game: &Game<'_>, screen: &NativeScreenView<'_>, button: i32, canceled: &mut bool) {
    if screen.menu != Some(rules::MENU) {
        return;
    }
    let at = [screen.mouse[0] - screen.rect[0], screen.mouse[1] - screen.rect[1]];
    let unit = basis::temperature_unit(rules::LOCAL_CONFIG_NAMESPACE);
    if let Some(updated) = rules::heat_tab_click(unit, at, button, game.mb_window_covers(at)) {
        basis::set_temperature_unit(rules::LOCAL_CONFIG_NAMESPACE, updated);
        *canceled = true;
    }
}

/// `Tooltip.create` de uma lista de linhas: cada uma quebrada em 170 pixels, sem cor própria.
fn wrapped(game: &Game<'_>, lines: &[String]) -> Vec<(String, Option<u32>)> {
    lines
        .iter()
        .flat_map(|line| machinery_basis_core::upgrade_window::wrap_lines(line, TOOLTIP_WRAP, &|piece: &str| game.native_text_width(piece)))
        .map(|line| (line, None))
        .collect()
}

/// Os tooltips dos widgets da tela (`GuiElement.renderToolTip`): a barra de energia (`GuiVerticalPowerBar`) e o medidor de lava
/// (`GuiFluidGauge`). Um tooltip de widget fica por cima ou por baixo dele, não sobre o cursor como o de `native_screen_tooltip`, então
/// sai daqui, pela injeção em `draw_native_screen_tooltip`. Devolve se desenhou. `at` é o cursor relativo ao canto da tela, e `cursor`
/// o mesmo em pixels de GUI da janela.
pub(in crate::game) fn widget_tooltip(
    game: &Game<'_>,
    batch: &mut HudBatch,
    block: &str,
    at: [f32; 2],
    cursor: (f32, f32),
) -> bool {
    if !is_generator(block) {
        return false;
    }
    // A janela de upgrades, quando aberta, e a aba ficam por cima do resto da tela.
    if game.mb_window_tooltip(batch, block, at, cursor) {
        return true;
    }
    let mirror = game.mb_mirror(block);
    let read = rules::ScreenData::read(&mirror);
    let translate = |key: &str| lang::get(key);
    let redstone = rules::redstone(&mirror);
    if basis::redstone_warning_tooltip(game, batch, redstone, WIDTH as i32, false, &translate, at, cursor) {
        return true;
    }
    if basis::redstone_widget_tooltip(game, batch, &translate(machinery_basis_core::redstone_tab::tooltip_key(redstone.control_type)), WIDTH as i32, at, cursor) {
        return true;
    }
    if basis::security_widget_tooltip(game, batch, &rules::security(&mirror), WIDTH as i32, machinery_basis_core::side_tab::SECURITY_TAB_Y, at, cursor) {
        return true;
    }
    // `GuiEnergyTab.updateTooltip`: `Tooltip.create` de uma lista de linhas, que quebra cada uma em 170 pixels.
    if rules::ENERGY_TAB.is_mouse_over(at, false) {
        let production = u64::from(read.production);
        let lines = wrapped(game, &rules::energy_tab_tooltip(production, &translate));
        let [x, y, w, h] = rules::ENERGY_TAB.rect;
        basis::menu_tooltip_flat(game, batch, &lines, cursor, [cursor.0 - at[0] + x, cursor.1 - at[1] + y, w, h]);
        return true;
    }
    // `GuiHeatTab.updateTooltip`: as três linhas do `GuiHeatGenerator` na unidade do cliente, e a da unidade, quebradas do mesmo jeito.
    // Uma janela por cima do ponto esconde a aba (`isMouseOverCheckWindows`).
    if rules::HEAT_TAB.is_mouse_over(at, game.mb_window_covers(at)) {
        let unit = basis::temperature_unit(rules::LOCAL_CONFIG_NAMESPACE);
        let info = rules::heat_tab_tooltip(read.heat, read.transfer_loss, read.environment_loss, unit, &translate);
        let lines = wrapped(game, &info);
        let [x, y, w, h] = rules::HEAT_TAB.rect;
        basis::menu_tooltip_flat(game, batch, &lines, cursor, [cursor.0 - at[0] + x, cursor.1 - at[1] + y, w, h]);
        return true;
    }
    let (widget, text) = if gui::contains(ENERGY_BAR, at) {
        let capacity = heat_config().energy_capacity_int() as u64;
        (ENERGY_BAR, rules::energy_tooltip(u64::from(read.energy), capacity, &translate))
    } else if gui::contains(LAVA_GAUGE, at) {
        (LAVA_GAUGE, rules::lava_tooltip(read.lava, &translate))
    } else {
        return false;
    };
    // O retângulo do widget na janela: o canto da tela é o cursor menos a posição relativa dele.
    let [x, y, w, h] = widget;
    let rect = [cursor.0 - at[0] + x, cursor.1 - at[1] + y, w, h];
    basis::menu_tooltip(game, batch, &[(text, None)], cursor, rect);
    true
}

/// O dono das regras de slot que este mod deixa na `slot_rules` do núcleo.
const RULES_OWNER: &str = "mekanismgenerators";

/// Quem responde pelas faces de item do Heat Generator (H2): lados fixos pela frente do bloco, e a regra de cada slot.
const ITEM_INSTANCES: basis::ItemInstances = basis::ItemInstances { faces: item_faces, extractable: item_extractable, accepts: item_accepts };

fn item_faces(block: &str, _entity: &ModBlockEntity, front: Face) -> Option<machinery_basis_core::item_instance::InstanceItems> {
    (block == rules::HEAT_GENERATOR).then(|| rules::item_instance(front))
}

/// `isValidForExtraction(item, EXTERNAL)`: o slot de combustível (`FluidFuelInventorySlot.forFuel`) e o de energia (`EnergyInventorySlot.drain`: `!External || !canDrain`).
fn item_extractable(block: &str, entity: &ModBlockEntity, slot: usize, stack: &ItemStack, facts: basis::ItemFacts) -> bool {
    if block != rules::HEAT_GENERATOR {
        return false;
    }
    match slot {
        FUEL_SLOT => rules::fuel_slot_extractable(facts.cooking_fuel, fills_tank(stack)),
        ENERGY_SLOT => !energy_can_drain(stored_energy(entity), stack),
        _ => false,
    }
}

/// `isValidForInsertion(item, EXTERNAL)` pela face: a face alcança o slot e o slot aceita o item (o mesmo que o menu pergunta, `GeneratorMenu::accepts`).
fn item_accepts(game: &Game<'_>, pos: [i32; 3], entity: &ModBlockEntity, slot: usize, stack: &ItemStack, face: Option<Face>) -> Option<bool> {
    if entity.id != rules::HEAT_GENERATOR {
        return None;
    }
    let Some(face) = face else {
        return Some(false);
    };
    if !rules::item_instance(basis::front(game, pos)).accepts_through(face, slot) {
        return Some(false);
    }
    Some(match slot {
        FUEL_SLOT => rules::fuel_slot_insertable(is_fuel(game, stack), fills_tank(stack)),
        ENERGY_SLOT => energy_can_drain(stored_energy(entity), stack),
        _ => false,
    })
}

/// Antes de um clique do jogador no menu de uma máquina (`lan_machine_click`, que o servidor roda para o anfitrião e para os
/// convidados): deixa a regra de `InventoryContainerSlot.mayPlace` do Heat Generator aberto, que o hook `menu_slot_rules` do Horizon
/// consulta dentro do menu, onde não há o jogo. A regra do slot de combustível pergunta o tempo de queima, que só o jogo sabe: ela
/// responde para cada item que o clique pode perguntar, que são os que o menu tem (o inventário de quem clica, o cursor e os slots do
/// bloco).
pub(in crate::game) fn menu_click_before(game: &Game<'_>, host: &crate::game::lan::LanHost, conn: u32) {
    slot_rules::clear(RULES_OWNER);
    let Some(guest) = host.hub.guests.iter().find(|guest| guest.conn == conn) else {
        return;
    };
    let Some(horizon_client::lan::menu::Slots::Block((x, y, z))) = guest.menu.as_ref().map(|menu| menu.slots) else {
        return;
    };
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(x, y, z)) else {
        return;
    };
    if entity.id != rules::HEAT_GENERATOR {
        return;
    }
    let stored = stored_energy(entity);
    let mut fuel: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
    let stacks = guest.inventory.iter().flatten().chain(guest.carried.as_ref()).chain(entity.items.iter().flatten());
    for stack in stacks {
        fuel.entry(stack.item.to_string()).or_insert_with(|| rules::fuel_slot_insertable(is_fuel(game, stack), fills_tank(stack)));
    }
    let upgrades = window_upgrade_data();
    slot_rules::set(
        RULES_OWNER,
        rules::HEAT_GENERATOR,
        std::rc::Rc::new(move |slot, item, stack| match slot {
            // O slot virtual de entrada de upgrades (`UpgradeInventorySlot.input`) aceita o upgrade da tag do gerador, que o tipo, um componente
            // da pilha, diz; o de saída só deixa o jogador tirar.
            rules::UPGRADE_SLOT => stack
                .downcast_ref::<ItemStack>()
                .is_some_and(|stack| rules::accepts_upgrade(&upgrades, item, upgrade_type_of(stack).as_deref())),
            FUEL_SLOT => fuel.get(item).copied().unwrap_or(false),
            // `EnergyInventorySlot.drain`: o handler de energia é o da pilha (a energia que ela guarda decide se está cheia).
            ENERGY_SLOT => stack
                .downcast_ref::<ItemStack>()
                .is_some_and(|stack| energy_can_drain(stored, stack)),
            _ => false,
        }),
    );
}

/// O tipo de upgrade que o item carrega (`MekanismDataComponents.UPGRADE_TYPE`), lido da pilha que o hook recebeu: o hook roda dentro do
/// menu, sem o jogo, então lê o componente guardado na própria pilha, como `native_component` faz antes do padrão do item.
fn upgrade_type_of(stack: &ItemStack) -> Option<String> {
    match stack.data()?.other(rules::UPGRADE_TYPE_COMPONENT)? {
        Tag::String(kind) => Some(kind.to_string_lossy()),
        _ => None,
    }
}

/// O menu do Heat Generator visto pelo `quickMoveStack` do Mekanism: os slots do bloco (`entity.items`: combustível e energia, na ordem
/// do Java, e os dois slots virtuais da janela de upgrades no fim), o inventário do jogador e a barra de armadura. Um slot do bloco tem o
/// índice dele; o do jogador, `items.len() + índice no inventário` (a barra de atalhos é 0 a 8, o inventário 9 a 35); e os da barra
/// `items.len() + 36 + k` (a armadura da cabeça aos pés e a mão), a ordem do menu do Java e dos índices que o cliente manda.
struct GeneratorMenu<'a, 'g> {
    game: &'a Game<'g>,
    entity: &'a mut ModBlockEntity,
    inventory: &'a mut [Option<ItemStack>],
    /// `Inventory.armor`, da cabeça aos pés, e `Inventory.offhand`: os slots 40 a 44 do menu de quem clica.
    armor: &'a mut [Option<ItemStack>; 4],
    offhand: &'a mut Option<ItemStack>,
    /// Os slots da barra que o tipo de menu declarou, na ordem do menu.
    bar: Vec<armor_slots::PlacedSlot>,
    /// A janela que o jogador que clica tem escolhida (`getSelectedWindow(player)`).
    window: Option<SelectedWindow>,
    upgrades: Rc<upgrade::UpgradeData>,
}

/// `EquipmentSlot.getName()`, como o catálogo de itens do jogo o escreve em `equip_slot`.
fn equipment_name(slot: EquipmentSlot) -> &'static str {
    match slot {
        EquipmentSlot::Feet => "feet",
        EquipmentSlot::Legs => "legs",
        EquipmentSlot::Chest => "chest",
        EquipmentSlot::Head => "head",
    }
}

/// `ItemStack.canEquip(slot, player)`: o item se veste nesse slot e o jogador está entre os que o `equippable` dele permite. É a conta
/// que o motor de menus do Horizon faz para o `ArmorSlot` (`Menu::may_place`).
fn can_equip(game: &Game<'_>, stack: &ItemStack, slot: EquipmentSlot) -> bool {
    game.items().get(&stack.item).is_some_and(|info| {
        info.equip_slot.as_deref() == Some(equipment_name(slot))
            && info.equip_allowed.as_ref().is_none_or(|allowed| allowed.contains("minecraft:player"))
    })
}

impl GeneratorMenu<'_, '_> {
    fn first_player(&self) -> usize {
        self.entity.items.len()
    }

    /// O slot da barra (`0` a `bar.len() - 1`) que um índice do menu é.
    fn bar_index(&self, slot: usize) -> Option<usize> {
        let k = slot.checked_sub(self.first_player() + armor_bar::PLAYER_SLOTS)?;
        (k < self.bar.len()).then_some(k)
    }

    /// `isValidForInsertion(.., MANUAL)` do slot: o de combustível (`FluidFuelInventorySlot.forFuel`) aceita o que queima ou o que enche
    /// o tanque de lava (o balde, que também queima), o de energia (`EnergyInventorySlot.drain`) o que `canDrain`: nenhum item portado
    /// tem handler de energia, então nada; e o virtual de entrada de upgrades (`UpgradeInventorySlot.input`) o upgrade da tag do gerador.
    /// A saída de upgrades não aceita nada de quem clica.
    fn accepts(&self, slot: usize, stack: &ItemStack) -> bool {
        match slot {
            FUEL_SLOT => rules::fuel_slot_insertable(is_fuel(self.game, stack), fills_tank(stack)),
            ENERGY_SLOT => {
                // A capacidade que os upgrades de energia do item dão, vista pelo jogo, entra na conta do handler.
                basis::energy_slot_resource(self.game, stack);
                energy_can_drain(stored_energy(self.entity), stack)
            }
            rules::UPGRADE_SLOT => rules::accepts_upgrade(&self.upgrades, &stack.item, self.upgrade_type(stack).as_deref()),
            _ => false,
        }
    }

    fn upgrade_type(&self, stack: &ItemStack) -> Option<String> {
        match self.game.native_component(stack, rules::UPGRADE_TYPE_COMPONENT) {
            Ok(Some(Tag::String(kind))) => Some(kind.to_string_lossy()),
            _ => None,
        }
    }

    fn cell(&mut self, slot: usize) -> &mut Option<ItemStack> {
        let first = self.first_player();
        if slot < first {
            &mut self.entity.items[slot]
        } else if let Some(k) = self.bar_index(slot) {
            if k < self.armor.len() { &mut self.armor[k] } else { &mut *self.offhand }
        } else {
            &mut self.inventory[slot - first]
        }
    }

    /// `TransactionalSlot.insert` do `ArmorSlot` e do `OffhandSlot` da barra: a regra é de `armor_slots::insert`, o jogo só diz o que sabe
    /// do item (em que slot se veste, o tamanho da pilha).
    fn insert_into_bar(&mut self, k: usize, item: &ItemStack, amount: u32) -> u32 {
        let kind = self.bar[k].kind;
        let item_max = basis::stack_limit(self.game, item);
        let game = self.game;
        let cell = self.cell_of_bar(k);
        let held = cell.as_ref().map_or(0, |held| held.count);
        let same = cell.as_ref().is_some_and(|held| held.same_item_same_components(item));
        let moved = armor_slots::insert(kind, held, same, amount, item_max, |slot| can_equip(game, item, slot));
        if moved == 0 {
            return 0;
        }
        match cell {
            Some(held) => held.count += moved,
            empty => {
                let mut placed = item.clone();
                placed.count = moved;
                *empty = Some(placed);
            }
        }
        moved
    }

    fn cell_of_bar(&mut self, k: usize) -> &mut Option<ItemStack> {
        if k < self.armor.len() { &mut self.armor[k] } else { &mut *self.offhand }
    }
}

/// O handler de energia dos itens (`Capabilities.ENERGY`): quem sabe quais itens têm handler é o mod dono do item (`machinery_basis::register_item_energy`), e o Heat Generator
/// responde pelo próprio item (`ENERGY_ITEMS`). O slot de energia do gerador não converte item em energia, então o índice de conversões é vazio.
fn items() -> ItemEnergy {
    basis::item_energy_handler(EnergyConversion::cache_of(Vec::new()))
}

/// `EnergyInventorySlot.canDrain(storage, energyHandler)` do slot de energia (`EnergyInventorySlot.drain`), com o buffer do gerador (`BasicEnergyContainer.output`) com `stored`.
fn energy_can_drain(stored: i64, stack: &ItemStack) -> bool {
    let stored = i32::try_from(stored).unwrap_or(i32::MAX);
    rules::slot_can_drain(&items(), stored, heat_config().energy_capacity, &basis::item_resource_with_limit(stack, 1))
}

/// Os itens com handler de energia daqui: o próprio Heat Generator (`ItemBlockTooltip.getDefaultEnergyContainer`).
const ENERGY_ITEMS: basis::ItemEnergyProvider = basis::ItemEnergyProvider { component: rules::ENERGY_COMPONENT, spec: energy_item_spec, upgraded: None };

fn energy_item_spec(item: &str, energy: i64, _capacity: Option<i64>) -> Option<machinery_basis_core::energy_cube::ItemEnergySpec> {
    rules::item_energy_spec(item, energy, &heat_config())
}

/// `energySlot.drainContainerIntoSlot(null)`, o primeiro passo do Heat Generator depois do `super.onUpdateServer`: o buffer entrega ao item do slot de energia o que a taxa de carga
/// dele deixa. O slot é o do tile (`entity.items`), e o item que ele guarda ganha a energia nova.
fn drain_energy_slot(game: &Game<'_>, entity: &mut ModBlockEntity, state: &mut rules::State) {
    let Some(stack) = entity.items.get(ENERGY_SLOT).cloned().flatten() else {
        return;
    };
    let slot = InventorySlot::new(None);
    let resource = basis::energy_slot_resource(game, &stack);
    slot.set_contents(Some(resource.clone()), stack.count, None);
    rules::drain_into_slot(state, &items(), &slot);
    let contents = slot.contents();
    if let Some(after) = contents.resource.filter(|after| *after != resource) {
        entity.items[ENERGY_SLOT] = Some(basis::energy_slot_stack(game, Some(&stack), &after, contents.amount));
    }
}

fn stored_energy(entity: &ModBlockEntity) -> i64 {
    i64::from(basis::stored_energy(entity))
}

impl QuickMenu for GeneratorMenu<'_, '_> {
    type Item = ItemStack;

    fn slots_of(&self, region: Region) -> Vec<usize> {
        let first = self.first_player();
        let bar = |wanted: fn(SlotKind) -> bool| -> Vec<usize> {
            let base = first + armor_bar::PLAYER_SLOTS;
            self.bar.iter().enumerate().filter(|(_, slot)| wanted(slot.kind)).map(|(k, _)| base + k).collect()
        };
        match region {
            // A ordem do Java: os dois virtuais primeiro, depois os do gerador em `getInitialInventory`.
            Region::Container => rules::CONTAINER_ORDER.to_vec(),
            Region::Hotbar => (first..first + 9).collect(),
            Region::Main => (first + 9..first + 36).collect(),
            Region::Armor => bar(|kind| matches!(kind, SlotKind::Armor(_))),
            Region::Offhand => bar(|kind| kind == SlotKind::Offhand),
        }
    }

    fn region(&self, slot: usize) -> Region {
        let first = self.first_player();
        if slot < first {
            Region::Container
        } else if let Some(k) = self.bar_index(slot) {
            match self.bar[k].kind {
                SlotKind::Armor(_) => Region::Armor,
                SlotKind::Offhand => Region::Offhand,
            }
        } else if slot - first < 9 {
            Region::Hotbar
        } else {
            Region::Main
        }
    }

    /// `ITransactionalSlot.exists(selectedWindow)`: um slot virtual só existe com a janela dele escolhida por quem clica.
    fn exists(&self, slot: usize) -> bool {
        rules::slot_exists(slot, self.window)
    }

    fn stack(&self, slot: usize) -> Option<(ItemStack, u32)> {
        let first = self.first_player();
        let held = if slot < first {
            self.entity.items[slot].as_ref()
        } else if let Some(k) = self.bar_index(slot) {
            if k < self.armor.len() { self.armor[k].as_ref() } else { self.offhand.as_ref() }
        } else {
            self.inventory[slot - first].as_ref()
        };
        held.filter(|stack| stack.count > 0).map(|stack| (stack.clone(), stack.count))
    }

    fn insert(&mut self, slot: usize, item: &ItemStack, amount: u32) -> u32 {
        if let Some(k) = self.bar_index(slot) {
            return self.insert_into_bar(k, item, amount);
        }
        if slot < self.first_player() && !self.accepts(slot, item) {
            return 0;
        }
        basis::insert_into_cell(self.game, self.cell(slot), item, amount)
    }

    fn remove(&mut self, slot: usize, amount: u32) {
        basis::remove_from_cell(self.cell(slot), amount);
    }
}

/// Shift-click do menu do Heat Generator: `MekanismContainer.quickMoveStack`, como nas quatro máquinas. Do inventário vai para o slot do
/// gerador que aceita o item (o combustível, e o de upgrades quando a janela do jogador que clica está aberta) e, não cabendo, para a
/// outra região do inventário; do gerador vai para a barra de atalhos e depois o inventário.
pub(in crate::game) fn quick_move(
    game: &mut Game<'_>,
    pos: [i32; 3],
    entity: &mut ModBlockEntity,
    container_slots: usize,
    slot: usize,
    stack: &mut ItemStack,
    handled: &mut bool,
) {
    if !is_generator(&entity.id) {
        return;
    }
    ensure_slots(entity);
    let window = game.mb_window_selected(pos);
    let upgrades = window_upgrade_data();
    let first = entity.items.len();
    // O menu numera o inventário do jogador depois dos slots do bloco: o principal (9 a 35), depois a barra de atalhos (0 a 8), e então a
    // barra de armadura (a armadura da cabeça aos pés e a mão).
    let from = if slot < container_slots {
        slot
    } else {
        let n = slot - container_slots;
        first + match n {
            0..27 => 9 + n,
            27..36 => n - 27,
            _ => n,
        }
    };
    let mut inventory = std::mem::take(&mut game.inventory);
    let mut armor = std::mem::take(&mut game.armor);
    let mut offhand = game.offhand.take();
    let left = {
        let bar = armor_bar::slots(rules::MENU);
        let mut menu = GeneratorMenu {
            game: &*game,
            entity,
            inventory: &mut inventory,
            armor: &mut armor,
            offhand: &mut offhand,
            bar,
            window,
            upgrades,
        };
        quick_move::quick_move(&mut menu, from);
        menu.stack(from)
    };
    game.inventory = inventory;
    game.armor = armor;
    game.offhand = offhand;
    // O slot clicado guarda o que sobrou; `handled` impede o roteamento do jogo, que levaria o item a outros slots.
    match left {
        Some((remaining, count)) => {
            *stack = remaining;
            stack.count = count;
        }
        None => stack.count = 0,
    }
    *handled = true;
}
