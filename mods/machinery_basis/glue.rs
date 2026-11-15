// Tudo da lib que fala com o jogo. As regras ficam em machinery_basis_core; este arquivo só as
// liga ao Horizon. Outros mods chamam daqui como crate::game::machinery_basis.
use crate::game::{Game, GameLevel, LevelDimension, NativeReloadListener};
use horizon_client::containers::ModBlockEntity;
use horizon_client::crafting::{Cooking, Ingredient, Shape};
use horizon_client::hud::HudBatch;
use horizon_client::items::ItemStack;
use horizon_core::geometry::Direction;
use horizon_core::position::BlockPos;
use horizon_core::random::RandomSource;
use horizon_world::fluid::{FluidKind, FluidState};
use horizon_world::level::{PlacementContext, Ticks};
use horizon_world::nbt::{Compound, JavaString, Tag};
use horizon_world::registry::BlockStateId;
use horizon_client::item_data::{StackData, canon_tag, decode_components, encode_components};
use machinery_basis_core::components::Components;
use machinery_basis_core::components_ingredient::{self, ComponentsIngredient, Leaf, Tree};
use machinery_basis_core::containers::{ItemResource, SlotContents, room_for};
use machinery_basis_core::biome_temperature;
use machinery_basis_core::fluid_logged;
use machinery_basis_core::gui::{self, NineSlice, TilingDirection};
use machinery_basis_core::tooltip;
use machinery_basis_core::heat;
use machinery_basis_core::liquid_data;
use machinery_basis_core::neighbors::{OUTPUT_SIGNAL_ORDER, PendingPower};
use machinery_basis_core::network::{self, EnergyBlocks};
use machinery_basis_core::placement::{self, Rules as PlacementRules};
use machinery_basis_core::recipe::{
    Conversion, ConversionFields, EnergyConversionData, MachineRecipeData, RecipeFields, ResourceInput,
};
use machinery_basis_core::resource::{self, Registry, Tags};
use machinery_basis_core::sides::Face;
use machinery_basis_core::tank::{FORMAT, Tank};
use machinery_basis_core::temperature;
use machinery_basis_core::upgrade::{self, UpgradeSaved, UpgradeTags, UpgradeTypes};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Instant;

const LISTENER: &str = "machinery_basis:resources";
const TAGS_LISTENER: &str = "machinery_basis:resource_tags";
const ENERGY_LISTENER: &str = "machinery_basis:energy";
const PLACEMENT_LISTENER: &str = "machinery_basis:placement";
const UPGRADES_LISTENER: &str = "machinery_basis:upgrades";
const UPGRADE_TAGS_LISTENER: &str = "machinery_basis:upgrade_tags";
const LIQUIDS_LISTENER: &str = "machinery_basis:liquids";
const SAVED: &str = "machinery_basis";
const ENERGY: &str = "energy";
const DEFAULT_STACK: u32 = 64;
const WATER: &str = "minecraft:water";
const LAVA: &str = "minecraft:lava";
/// `WaterFluid.getTickDelay` e `LavaFluid.getTickDelay` (fora e dentro do Nether).
const WATER_TICK_DELAY: i32 = 5;
const LAVA_TICK_DELAY: i32 = 30;
const LAVA_TICK_DELAY_NETHER: i32 = 10;
const NETHER: &str = "minecraft:the_nether";
const CREATIVE: i32 = 1;
/// `Block.UPDATE_ALL`, as flags do `setBlock` do balde.
const BUCKET_SOUND_FILL: &str = "minecraft:item.bucket.fill";
const BUCKET_SOUND_FILL_LAVA: &str = "minecraft:item.bucket.fill_lava";
const BUCKET_SOUND_EMPTY: &str = "minecraft:item.bucket.empty";
const BUCKET_SOUND_EMPTY_LAVA: &str = "minecraft:item.bucket.empty_lava";
const FIRE_EXTINGUISH: &str = "minecraft:block.fire.extinguish";
/// Os eventos de nível do dispenser: o som padrão (1000) e a animação (2000, com a direção).
const DISPENSER_CLICK: i32 = 1000;
const DISPENSER_ANIMATION: i32 = 2000;
// Os índices de receitas por tipo. Só valem até a próxima carga de dados.
static MACHINE_RECIPES: Mutex<BTreeMap<String, Arc<Vec<MachineRecipe>>>> = Mutex::new(BTreeMap::new());
static CONVERSIONS: Mutex<BTreeMap<String, Arc<Vec<ItemConversion>>>> = Mutex::new(BTreeMap::new());
static ENERGY_CONVERSIONS: Mutex<BTreeMap<String, Arc<Vec<ItemEnergyConversion>>>> = Mutex::new(BTreeMap::new());
/// As receitas que `report` achou incompletas (um ingrediente sem membro: `placementInfo().isImpossibleToPlace()`) nesta carga de dados.
/// Se esvazia a cada carga, junto dos índices.
static INCOMPLETE: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
/// Sobe a cada carga de dados (`data_loaded`). Os índices de receitas de cada thread (`MekanismRecipeType.getInputCache`) guardam a
/// carga de onde saíram e se limpam (`clear`) quando ela muda.
static DATA_GENERATION: AtomicU64 = AtomicU64::new(0);
// A parte da tela que toda máquina tem, nas posições do inventário vanilla.
const INVENTORY_LEFT: f32 = 7.0;
const INVENTORY_TOP: f32 = 83.0;
const HOTBAR_GAP: f32 = 4.0;
const INVENTORY_COLUMNS: usize = 9;
const INVENTORY_ROWS: usize = 3;

/// Os sprites que dão a cara de um mod às telas dele.
pub(in crate::game) struct PanelStyle {
    /// O painel de fundo (`GuiMekanism.BASE_BACKGROUND_SLICE`) e o `gui.scaling` do `.mcmeta` dele.
    pub base: &'static str,
    pub base_slice: NineSlice,
    /// A moldura dos slots do inventário do jogador (`SlotType.NORMAL`).
    pub slot: &'static str,
}

/// Uma receita de máquina, com o que o jogo resolveu.
pub(in crate::game) struct MachineRecipe {
    pub id: String,
    pub item: Ingredient,
    /// Os componentes do ingrediente, quando ele é um `neoforge:components`.
    pub components: Option<Arc<Tree>>,
    pub count: u32,
    pub resource: Option<ResourceInput>,
    pub output: ItemStack,
    /// `per_tick_usage`: a receita gasta o recurso a cada tick, e não de uma vez no fim.
    pub per_tick_usage: bool,
    /// `MekanismRecipe.experience()` ou `SmeltingRecipe.experience()`: a experiência de cada operação.
    pub experience: f32,
}

/// Um item que vira recurso, com o que o jogo resolveu.
pub(in crate::game) struct ItemConversion {
    pub id: String,
    pub item: Ingredient,
    pub components: Option<Arc<Tree>>,
    pub count: u32,
    pub resource: String,
    pub amount: u64,
}

/// Um item que vira energia, com o que o jogo resolveu.
pub(in crate::game) struct ItemEnergyConversion {
    pub id: String,
    pub item: Ingredient,
    pub components: Option<Arc<Tree>>,
    pub count: u32,
    pub amount: i32,
}

/// De onde vêm as receitas de uma máquina.
#[derive(Clone, Copy)]
pub(in crate::game) enum RecipeSource {
    /// Arquivos de um tipo próprio.
    Custom(&'static str, RecipeFields),
    /// As receitas de cozimento do jogo, de um tipo como `smelting`.
    Cooking(&'static str),
}

impl Game<'_> {
    /// Roda `act` em cada nível que o servidor ticou e que `wanted` aceita (pelo id da dimensão), com o nível no lugar de `self.world`.
    /// Serve ao `ServerTickEnd`, onde só o último nível está no lugar: entra em cada um como `close_idle_levels` e deixa no lugar o
    /// que estava, para o resto do fim do tick não ver outro nível.
    pub(in crate::game) fn mb_in_levels(
        &mut self,
        wanted: impl Fn(&str) -> bool,
        mut act: impl FnMut(&mut Self, &'static str),
    ) {
        let back = self.dim;
        let mut dims: Vec<crate::game::Dim> =
            self.levels_to_tick().into_iter().filter(|dim| wanted(dim.id())).collect();
        // O nível que já está no lugar fica por último: o caso comum, uma dimensão só, não troca nada.
        dims.sort_by_key(|dim| *dim == back);
        for dim in dims {
            if dim != self.dim && self.enter_level(None, dim).is_err() {
                continue;
            }
            act(self, dim.id());
        }
        if self.dim != back {
            let _ = self.enter_level(None, back);
        }
    }

    /// `MinecraftServer.createLevels` abre os três níveis ao começar, e um chunk forçado (`/forceload`) do Nether ou do End anda desde o primeiro tick, sem jogador nenhum lá. O Horizon só
    /// abre o nível de uma dimensão quando alguém entra nela: o que o mundo guardou de forçado fora da dimensão do jogador ficava parado até alguém visitar a dimensão (medido: três cabos
    /// num chunk forçado do Nether, reabertos, só voltavam à rede depois de um comando no Nether). Abre aqui, antes do laço do servidor (`WorldOpenReady`), cada nível que o mundo já visitou
    /// (a pasta da dimensão existe); o que não tem chunk forçado fecha sozinho pela ociosidade (`close_idle_levels`).
    pub(in crate::game) fn mb_open_visited_levels(&mut self) {
        if self.remote.is_some() {
            return;
        }
        let back = self.dim;
        for dim in [crate::game::Dim::Nether, crate::game::Dim::End] {
            let visited = self.world_root.join("dimensions").join(dim.id().replace(':', "/")).is_dir();
            if dim != back && visited && self.enter_level(None, dim).is_err() {
                continue;
            }
        }
        if self.dim != back {
            let _ = self.enter_level(None, back);
        }
    }

    /// O chunk do dono do mundo que ainda não entrou (decisão 195): `Owner.saved` fica cheio até o dono entrar (`lan_guest_joined` o tira), e a posição dele é a do arquivo do jogador
    /// (o chunk em que ele estava ao sair) ou, num jogador novo, a do spawn. Só vale para o nível em que ele estava: o de outra dimensão não o tem como jogador. O segundo valor é o modo
    /// espectador (a regra `spectators_generate_chunks` o tira dos tickets, como o de qualquer jogador).
    fn mb_pending_owner(&self) -> Option<((i32, i32), bool)> {
        let saved = self.lan.as_ref()?.owner.as_ref()?.saved.as_ref()?;
        if !saved.dimension.is_empty() && saved.dimension != self.dim.id() {
            return None;
        }
        let (x, z) = match saved.position {
            Some(position) => (position[0], position[2]),
            None => self
                .spawn
                .map(|s| (s.x, s.z))
                .or_else(|| self.level.spawn.map(|s| (s[0], s[2])))?,
        };
        Some((((x.floor() as i32) >> 4, (z.floor() as i32) >> 4), saved.mode() == 3))
    }

    /// O nível em que o dono do mundo que ainda não entrou está (a dimensão da posição guardada no arquivo do jogador), que o servidor anda no primeiro tick como o original: o dono já está
    /// lá e o nível anda por causa dele. `levels_to_tick` só conhece os convidados e os chunks forçados (um mundo reaberto com um chunk forçado no Nether andaria só o Nether na primeira
    /// passada, e o Overworld do dono ficaria parado). Acrescenta o nível do dono, na ordem do servidor (Overworld, Nether, End), a quem já anda; sem dono à espera devolve a lista como veio.
    pub(in crate::game) fn mb_owner_level(&self, dims: Vec<crate::game::Dim>) -> Vec<crate::game::Dim> {
        use crate::game::Dim;
        if mb_first_pass_off() {
            return dims;
        }
        let Some(saved) = self.lan.as_ref().and_then(|h| h.owner.as_ref()).and_then(|o| o.saved.as_ref()) else {
            return dims;
        };
        let owner = Dim::from_id(&saved.dimension);
        if dims.contains(&owner) {
            return dims;
        }
        if TRANSMITTER_PROBE {
            eprintln!("MBT {{\"t\":{},\"e\":\"owner_level\",\"dim\":\"{}\"}}", TRANSMITTER_PASS.load(Ordering::Relaxed), owner.id());
        }
        [Dim::Overworld, Dim::Nether, Dim::End].into_iter().filter(|dim| *dim == owner || dims.contains(dim)).collect()
    }

    /// O jogador já está no servidor no primeiro tick do original (decisão 195, a causa da rede na segunda passada): `Game::update_chunk_tickets` só conhece os convidados e o `self.player`,
    /// e o dono do mundo entra no fim da primeira passada. Enquanto ele não entrou, esta função faz o que a original faz (a lista de jogadores, os chunks forçados, o portal, a arena do
    /// dragão e as distâncias) com o chunk dele como mais um jogador, e devolve `true` para a função original não rodar; sem dono à espera devolve `false` e a função original roda, intacta. O corpo é
    /// o do Horizon da base `f319701`, linha a linha, com a única diferença do `players.push` do dono.
    pub(in crate::game) fn mb_owner_tickets(&mut self) -> bool {
        if mb_first_pass_off() {
            return false;
        }
        let Some((chunk, spectator)) = self.mb_pending_owner() else {
            return false;
        };
        if TRANSMITTER_PROBE {
            eprintln!("MBT {{\"t\":{},\"e\":\"owner_tickets\",\"chunk\":[{},{}]}}", TRANSMITTER_PASS.load(Ordering::Relaxed), chunk.0, chunk.1);
        }
        let rule = self.ui.rules.bool("spectators_generate_chunks");
        let mut players: Vec<((i32, i32), bool)> = self.lan.as_ref().map_or_else(Vec::new, |h| {
            h.hub
                .guests
                .iter()
                .filter(|g| h.hub.here(g))
                .map(|g| (g.chunk(), g.spectator()))
                .collect()
        });
        if let Some(p) = &self.player {
            players.push((
                ((p.position.x.floor() as i32) >> 4, (p.position.z.floor() as i32) >> 4),
                self.spectator(),
            ));
        }
        players.push((chunk, spectator));
        let sources = horizon_client::ticket_levels::Sources {
            players: players
                .into_iter()
                .filter(|&(_, spectator)| crate::game::game_mode::loads_chunks(rule, spectator))
                .map(|(chunk, _)| chunk)
                .collect(),
            forced: self
                .streaming
                .forced
                .iter()
                .chain(self.streaming.mod_forced.iter().filter(|c| !self.streaming.forced.contains(c)))
                .copied()
                .collect(),
            portal: Vec::new(),
            dragon: (self.dim == crate::game::Dim::End && self.fight.arena_watched()).then_some((0, 0)),
            view_distance: self.settings.render_distance.clamp(2, 32),
            simulation_distance: self.settings.simulation_distance.max(2),
        };
        self.tickets.update(Some(sources));
        true
    }

    /// Sonda de `tp-dimensao`: por que cada nível segurado está segurado (as cláusulas de `levels_to_tick`), escrito só quando o motivo muda.
    fn mb_probe_hold(&self, held: &[crate::game::Dim]) {
        use crate::game::Dim;
        static LAST: Mutex<String> = Mutex::new(String::new());
        let mut line = String::new();
        for dim in [Dim::Overworld, Dim::Nether, Dim::End] {
            let streaming = (dim == self.dim).then_some(&self.streaming);
            let guests = self.lan.as_ref().map_or(0, |h| h.hub.guests.iter().filter(|g| g.dimension == dim.id()).count());
            line.push_str(&format!(
                " {}:{{held:{},guests:{guests},viewers:{},bed:{},forced:{},mod_forced:{},tests:{},pregen:{},arrivals:{},portal:{},vehicle:{},trips:{}}}",
                dim.id(),
                held.contains(&dim),
                self.mod_container_viewers.keys().any(|(k, _)| *k == dim.id()),
                self.player_bed_levels().any(|b| b == dim),
                streaming.map_or(0, |s| s.forced.len()),
                streaming.map_or(0, |s| s.mod_forced.len()),
                self.game_tests.has_tests_in(dim),
                self.pregen_level() == Some(dim),
                self.arrival_chunks(dim).len(),
                self.portal_ticket_chunks(dim).len(),
                self.vehicle_waits_in(dim),
                self.server_trips.len(),
            ));
        }
        let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
        if *last != line {
            eprintln!("MBT {{\"t\":{},\"e\":\"hold\",\"dim\":\"{}\",\"razoes\":\"{}\"}}", TRANSMITTER_PASS.load(Ordering::Relaxed), self.dim.id(), line.trim());
            *last = line;
        }
    }

    /// O nível está aberto: o que está no lugar ou um dos parados.
    fn mb_level_open(&self, dim: crate::game::Dim) -> bool {
        dim == self.dim || self.lod_source_in(dim).is_some()
    }

    /// O fim de um tick do servidor para os níveis que ninguém segura (decisão 193): o original anda todos os níveis a cada tick, e o `ServerChunkCache` do que o jogador deixou, sem chunk forçado, tira os
    /// tickets e o `ChunkMap.processUnloads` o descarrega no segundo tick (`machinery_basis_core::level_idle`, medido). O Horizon só anda `levels_to_tick` e nunca descarregava o Overworld parado.
    /// Aqui o nível parado que a lib manda descarregar entra no lugar, o `update_world` roda sem área nenhuma (o que um servidor sem jogador faz, `radius < 0`: só os chunks forçados ficam) e solta
    /// os chunks pelo caminho do jogo (o `chunk_unload` dos mods antes de os block entities saírem, a gravação do que mudou), e o nível volta ao lugar. Devolve os níveis em que o `ServerTickPost`
    /// deste tick roda: os que o servidor andou e os parados que ainda estão carregados.
    pub(in crate::game) fn mb_idle_levels_end_of_tick(&mut self) -> Vec<crate::game::Dim> {
        use crate::game::Dim;
        const ALL: [Dim; 3] = [Dim::Overworld, Dim::Nether, Dim::End];
        let mut held = self.levels_to_tick();
        // Um veículo à espera do passageiro em outro nível segura o nível dele (`close_idle_levels`).
        for dim in ALL {
            if !held.contains(&dim) && self.vehicle_waits_in(dim) {
                held.push(dim);
            }
        }
        let held_ids: Vec<&str> = held.iter().map(|dim| dim.id()).collect();
        if TRANSMITTER_PROBE {
            self.mb_probe_hold(&held);
        }
        let step = IDLE_LEVELS.with_borrow_mut(|levels| levels.end_of_tick(&held_ids));
        let back = self.dim;
        if TRANSMITTER_PROBE && !step.waiting.is_empty() {
            eprintln!("MBT {{\"t\":{},\"e\":\"idle_step\",\"waiting\":{:?},\"unload\":{:?},\"held\":{:?}}}", TRANSMITTER_PASS.load(Ordering::Relaxed), step.waiting, step.unload, held_ids);
        }
        for id in &step.unload {
            let dim = Dim::from_id(id);
            if !self.mb_level_open(dim) || (dim != self.dim && self.enter_level(None, dim).is_err()) {
                continue;
            }
            // Sem área nenhuma: a do jogador que saiu (`lan_update_streaming`, o `extra` do último tick em que o nível esteve no lugar) e o raio de visão saem; ficam os chunks forçados.
            let radius = self.streaming.radius;
            let extra = std::mem::take(&mut self.streaming.extra);
            let before = self.world.chunks.len();
            self.streaming.radius = -1;
            // O nível que ninguém segura solta tudo: nada fica segurado para o original (decisão 199), nem o que o Horizon já tinha soltado antes.
            RELEASE_ALL.store(true, Ordering::Relaxed);
            self.update_world();
            transmitters_release_held(self, dim.id(), true);
            RELEASE_ALL.store(false, Ordering::Relaxed);
            self.streaming.radius = radius;
            self.streaming.extra = extra;
            if TRANSMITTER_PROBE {
                eprintln!(
                    "MBT {{\"t\":{},\"e\":\"idle_unload\",\"dim\":\"{}\",\"antes\":{before},\"depois\":{},\"forcados\":{}}}",
                    TRANSMITTER_PASS.load(Ordering::Relaxed),
                    dim.id(),
                    self.world.chunks.len(),
                    self.streaming.forced.len() + self.streaming.mod_forced.len()
                );
            }
        }
        // Os chunks da volta de um nível descarregado que esperavam (`ARRIVAL_DELAY_TICKS`) entram no registro no começo do tick seguinte (`mb_deliver_arrival_loads`).
        ARRIVAL_LOADS.with_borrow_mut(|loads| loads.extend(step.loads.iter().cloned()));
        if self.dim != back {
            let _ = self.enter_level(None, back);
        }
        machinery_basis_core::level_idle::IdleLevels::post_levels(&held_ids, &step).iter().map(|id| Dim::from_id(id)).collect()
    }

    /// O começo do tick seguinte ao fim em que a espera da volta venceu: o `ChunkEvent.Load` dos chunks que o nível carregou ao voltar chega ao registro, e o `Post` deste tick refaz as redes (dois
    /// ticks depois do `tp`, como o original, `ARRIVAL_DELAY_TICKS`).
    pub(in crate::game) fn mb_deliver_arrival_loads(&mut self) {
        use crate::game::Dim;
        let due = ARRIVAL_LOADS.with_borrow_mut(std::mem::take);
        if due.is_empty() || self.remote.is_some() {
            return;
        }
        let back = self.dim;
        for (id, chunk) in due {
            let dim = Dim::from_id(&id);
            if self.mb_level_open(dim) && (dim == self.dim || self.enter_level(None, dim).is_ok()) && self.world.chunks.contains_key(&(chunk[0], chunk[1])) {
                if TRANSMITTER_PROBE {
                    eprintln!("MBT {{\"t\":{},\"e\":\"arrival_load\",\"dim\":\"{id}\",\"cx\":{},\"cz\":{}}}", TRANSMITTER_PASS.load(Ordering::Relaxed), chunk[0], chunk[1]);
                }
                transmitters_chunk_loaded_now(self, &id, chunk);
            }
        }
        if self.dim != back {
            let _ = self.enter_level(None, back);
        }
    }

    /// O servidor fecha (`MinecraftServer.stopServer`: `deactivateTicketsOnClosing`, os ticks até `hasWork` acabar): cada nível solta os chunks pelo caminho de sempre, o `ChunkEvent.Unload` e o
    /// `onChunkUnloaded` e `setRemoved` de cada transmissor, antes do `ServerStoppedEvent` (`WorldCloseFinal`). Depois do salvamento final, com os block entities ainda no lugar; os níveis na ordem
    /// em que o original os solta (`closing_order`: o de menos chunks primeiro).
    pub(in crate::game) fn mb_close_levels(&mut self) {
        use crate::game::Dim;
        if self.remote.is_some() {
            return;
        }
        let back = self.dim;
        let mut counts = Vec::new();
        for dim in [Dim::Overworld, Dim::Nether, Dim::End] {
            if !self.mb_level_open(dim) || (dim != self.dim && self.enter_level(None, dim).is_err()) {
                continue;
            }
            counts.push((dim.id().to_owned(), self.world.chunks.len()));
        }
        for id in machinery_basis_core::level_idle::closing_order(&counts) {
            let dim = Dim::from_id(&id);
            if dim != self.dim && self.enter_level(None, dim).is_err() {
                continue;
            }
            let chunks: Vec<_> = self.world.chunks.values().cloned().collect();
            RELEASE_ALL.store(true, Ordering::Relaxed);
            self.native_chunks_changed(&chunks);
            transmitters_release_held(self, dim.id(), true);
            RELEASE_ALL.store(false, Ordering::Relaxed);
        }
        if self.dim != back {
            let _ = self.enter_level(None, back);
        }
    }
}

impl RecipeSource {
    /// O tipo de receita de onde as da máquina saem: um índice de receitas por tipo.
    pub(in crate::game) fn key(&self) -> &'static str {
        match self {
            Self::Custom(recipe_type, _) | Self::Cooking(recipe_type) => recipe_type,
        }
    }
}

/// O valor de `fluid_logged` do bloco em `pos`, se ele tem a propriedade.
fn logged_value(game: &Game<'_>, pos: [i32; 3]) -> Option<String> {
    game.native_block_property(pos, fluid_logged::PROPERTY)
}

/// Se o bloco em `pos` guarda lava no estado `fluid_logged`.
pub(in crate::game) fn has_logged_lava(game: &Game<'_>, pos: [i32; 3]) -> bool {
    logged_value(game, pos).as_deref() == Some(fluid_logged::LAVA)
}

/// `Fluid.getTickDelay` de um fluido guardado: a água espera 5 ticks, a lava 30 (10 no Nether).
fn tick_delay(fluid: &str, nether: bool) -> i32 {
    match (fluid, nether) {
        (WATER, _) => WATER_TICK_DELAY,
        (_, true) => LAVA_TICK_DELAY_NETHER,
        _ => LAVA_TICK_DELAY,
    }
}

/// O fluido do vanilla que o valor de `fluid_logged` guarda (`FluidLogType.getFluid`).
fn fluid_of(value: &str) -> Option<&'static str> {
    match value {
        fluid_logged::WATER => Some(WATER),
        fluid_logged::LAVA => Some(LAVA),
        _ => None,
    }
}

/// `IStateFluidLoggable.isValidFluid` junto de `canPlaceLiquid`: o bloco está vazio e declara o valor (o Heat Generator só tem `empty` e `lava`).
fn can_place_liquid(game: &mut Game<'_>, pos: [i32; 3], value: &str) -> bool {
    logged_value(game, pos).as_deref() == Some(fluid_logged::EMPTY) && declares_value(game, pos, value)
}

/// Se o bloco em `pos` declara o valor de `fluid_logged` (`IStateFluidLoggable.isValidFluid`: o Heat Generator só tem `empty` e `lava`).
fn declares_value(game: &mut Game<'_>, pos: [i32; 3], value: &str) -> bool {
    let state = game.world.block(pos[0], pos[1], pos[2]);
    game.with_level(|_, level, _| level.native_state_with_property(state, fluid_logged::PROPERTY, value).is_some())
}

/// Se o `Fluid.getTickDelay` do fluido guardado vale a do Nether.
fn in_nether(game: &Game<'_>) -> bool {
    game.dim.id() == NETHER
}

fn is_creative(game: &Game<'_>) -> bool {
    game.native_acting_player().is_some_and(|player| player.game_mode == CREATIVE)
}

/// `LiquidBlockContainer.placeLiquid` do `IStateFluidLoggable`: o valor entra no estado e o fluido agenda o próprio tick.
fn place_logged(game: &mut Game<'_>, pos: [i32; 3], value: &str) {
    game.native_set_block_property(pos, fluid_logged::PROPERTY, value);
    if let Some(fluid) = fluid_of(value) {
        let delay = tick_delay(fluid, in_nether(game));
        game.with_level(|_, level, ticks| level.native_schedule_fluid_tick(ticks, pos, fluid, delay));
    }
}

/// O centro do bloco, onde o som do balde toca.
fn center(pos: [i32; 3]) -> [f64; 3] {
    pos.map(|v| f64::from(v) + 0.5)
}

/// `BucketItem.emptyContents` sobre um bloco com `fluid_logged`: a água do Nether evapora (`WATER_EVAPORATES` vem antes do contêiner, com a fumaça de `sendParticles`), e o resto entra
/// no estado. `who` é quem esvazia o balde (o jogador no `use`; ninguém no dispenser, `user` nulo). Devolve se o balde esvaziou.
fn empty_into(game: &mut Game<'_>, pos: [i32; 3], value: &str, who: Option<horizon_client::mobs::Who>) -> bool {
    if value == fluid_logged::WATER && in_nether(game) {
        let random = &mut game.world.random;
        let pitch = 2.6 + (random.next_float() - random.next_float()) * 0.8;
        game.audio.play(FIRE_EXTINGUISH, Some(center(pos)), 0.5, pitch);
        game.evaporation_smoke(pos);
        return true;
    }
    place_logged(game, pos, value);
    let sound = if value == fluid_logged::LAVA { BUCKET_SOUND_EMPTY_LAVA } else { BUCKET_SOUND_EMPTY };
    game.audio.play(sound, Some(center(pos)), 1.0, 1.0);
    game.post_block_game_event("minecraft:fluid_place", pos, who, None);
    true
}

/// `BucketItem.use` de um balde cheio de água ou de lava sobre um bloco com `fluid_logged` (o raio sem fluido, `canBlockContainFluid`): sem agachar, o fluido entra no bloco;
/// agachado, `canPlaceFluidInsideBlock` falha e o balde vai para o bloco ao lado, que o jogo trata como o de sempre.
fn use_filled(game: &mut Game<'_>, stack: &ItemStack, value: &'static str, claimed: &mut bool) {
    if game.native_sneaking() || !game.may_build() {
        return;
    }
    let Some(hit) = game.pick() else {
        return;
    };
    if !can_place_liquid(game, hit.pos, value) {
        return;
    }
    let mut canceled = false;
    game.native_fill_bucket(stack, hit.pos, &mut canceled);
    if canceled || !empty_into(game, hit.pos, value, Some(horizon_client::mobs::Who::Player)) {
        return;
    }
    game.trigger_block("minecraft:placed_block", hit.pos, stack);
    game.award_stat("used", &stack.item, 1);
    if !is_creative(game) && game.native_take_items(&stack.item, 1) == 1 {
        game.native_give_items(fluid_logged::EMPTY_BUCKET, 1);
    }
    *claimed = true;
}

/// `BucketItem.use` do balde vazio sobre um bloco com fluido guardado (`IStateFluidLoggable.pickupBlock`): o fluido sai do estado e o balde cheio vem no lugar do vazio.
fn use_empty(game: &mut Game<'_>, stack: &ItemStack, claimed: &mut bool) {
    if !game.may_build() {
        return;
    }
    let Some(hit) = game.bucket_hit() else {
        return;
    };
    let Some(value) = logged_value(game, hit.pos) else {
        return;
    };
    let Some(bucket) = fluid_logged::bucket_of_value(&value) else {
        return;
    };
    let mut canceled = false;
    game.native_fill_bucket(stack, hit.pos, &mut canceled);
    if canceled || !game.native_set_block_property(hit.pos, fluid_logged::PROPERTY, fluid_logged::EMPTY) {
        return;
    }
    game.award_stat("used", &stack.item, 1);
    let sound = if value == fluid_logged::LAVA { BUCKET_SOUND_FILL_LAVA } else { BUCKET_SOUND_FILL };
    game.audio.play(sound, Some(center(hit.pos)), 1.0, 1.0);
    game.post_block_game_event("minecraft:fluid_pickup", hit.pos, Some(horizon_client::mobs::Who::Player), None);
    if is_creative(game) {
        // `ItemUtils.createFilledResult`: o criativo ganha o balde cheio se ainda não o tem, e o vazio fica.
        if game.native_count_items(bucket) == 0 {
            game.native_give_items(bucket, 1);
        }
    } else if game.native_take_items(&stack.item, 1) == 1 {
        game.native_give_items(bucket, 1);
    }
    *claimed = true;
}

/// `DispenserBlock.getDispenseMethod` dos baldes contra um bloco com `fluid_logged` (`IStateFluidLoggable`, que é `LiquidBlockContainer` e `BucketPickup`): o balde cheio de água ou de
/// lava enche o bloco vazio (`emptyContents` com `user` e `hitResult` nulos, sem o bloco ao lado), o vazio tira o fluido (`pickupBlock`), e o que não dá solta o item (o
/// `DefaultDispenseItemBehavior`). Um bloco sem a propriedade fica para o dispenser do Horizon. A decisão é de `fluid_logged::dispense_bucket`.
pub(in crate::game) fn dispense_item(
    game: &mut Game<'_>,
    dimension: &str,
    pos: [i32; 3],
    facing: Direction,
    stack: &ItemStack,
    answer: &mut Option<horizon_client::mod_api::NativeDispensed>,
) {
    let offset = face_of(facing).offset();
    let target = [pos[0] + offset[0], pos[1] + offset[1], pos[2] + offset[2]];
    let current = logged_value(game, target);
    let outcome = fluid_logged::dispense_bucket(&stack.item, current.as_deref(), |value| declares_value(game, target, value));
    match outcome {
        fluid_logged::Dispensed::Pass => {}
        fluid_logged::Dispensed::Drop => {
            *answer = Some((game.native_dispense_default(dimension, pos, facing, stack.clone()), DISPENSER_CLICK));
        }
        fluid_logged::Dispensed::Fill { value } => {
            empty_into(game, target, value, None);
            spawn_bucket_mob(game, target, stack);
            let empty = ItemStack::new(fluid_logged::EMPTY_BUCKET, 1);
            *answer = Some((consume_with_remainder(game, dimension, pos, facing, stack.clone(), empty), DISPENSER_CLICK));
        }
        fluid_logged::Dispensed::Pickup { value, bucket } => {
            game.native_set_block_property(target, fluid_logged::PROPERTY, value);
            game.post_block_game_event("minecraft:fluid_pickup", target, None, None);
            let filled = ItemStack::new(bucket, 1);
            *answer = Some((consume_with_remainder(game, dimension, pos, facing, stack.clone(), filled), DISPENSER_CLICK));
        }
    }
}

/// `MobBucketItem.checkExtraContent`: o bicho do balde de peixe, solto no bloco que recebeu a água (com o `ENTITY_PLACE` do jogo).
fn spawn_bucket_mob(game: &mut Game<'_>, target: [i32; 3], stack: &ItemStack) {
    let Some((_, _, Some(kind))) = horizon_client::items::bucket_contents(&stack.item) else {
        return;
    };
    if let Some(id) = game.spawn_mob_at(kind, (target[0], target[1], target[2]), true) {
        if let Some(mob) = game.mobs.iter_mut().find(|m| m.id == id) {
            horizon_client::entity_nbt::apply_item_components(mob, stack);
        }
        let _ = game.mob_placed(id);
    }
    game.post_block_game_event("minecraft:entity_place", target, None, None);
}

/// `DefaultDispenseItemBehavior.consumeWithRemainder`: um item sai do monte e o resto (`remainder`) vai para o primeiro slot livre do dispenser (`insertItem`: os baldes cheios têm
/// pilha de 1, então nada se junta) ou, sem lugar, sai como item solto com o som e a animação do dispenser.
fn consume_with_remainder(
    game: &mut Game<'_>,
    dimension: &str,
    pos: [i32; 3],
    facing: Direction,
    mut stack: ItemStack,
    remainder: ItemStack,
) -> Option<ItemStack> {
    stack.count -= 1;
    if stack.count == 0 {
        return Some(remainder);
    }
    let mut rest = Some(remainder);
    if let Some(entity) = game.world.block_entities.get_mut(&(pos[0], pos[1], pos[2])) {
        if let Some(cell) = entity.slots_mut().iter_mut().find(|cell| cell.is_none()) {
            *cell = rest.take();
        }
    }
    if let Some(rest) = rest {
        game.native_dispense_default(dimension, pos, facing, rest);
        game.dispenser_event(DISPENSER_CLICK, (pos[0], pos[1], pos[2]), 0);
        game.dispenser_event(DISPENSER_ANIMATION, (pos[0], pos[1], pos[2]), facing_data(facing));
    }
    Some(stack)
}

/// `Direction.get3DDataValue`, o dado do evento de animação do dispenser.
fn facing_data(facing: Direction) -> i32 {
    match facing {
        Direction::Down => 0,
        Direction::Up => 1,
        Direction::North => 2,
        Direction::South => 3,
        Direction::West => 4,
        Direction::East => 5,
    }
}

/// O item de upgrade com shift entra num bloco com janela de upgrades. Um bloco com tela só chega aqui com o jogador agachado: em pé, o clique abre a tela.
pub(in crate::game) fn use_item_on(
    game: &mut Game<'_>,
    stack: &ItemStack,
    pos: [i32; 3],
    claimed: &mut bool,
) {
    // `ItemUpgrade.useOn`: o upgrade na mão, com shift, entra no bloco que o aceita.
    game.mb_window_use_item_on(stack, pos, claimed);
}

/// O uso do item na mão. O balde de um fluido de mod esvazia, e o balde vazio pega a fonte de um (`machinery_liquid`). O balde de água ou de lava enche um bloco com
/// `fluid_logged`, e o vazio o esvazia, como a água de uma escada.
pub(in crate::game) fn use_item(game: &mut Game<'_>, stack: &ItemStack, claimed: &mut bool) {
    crate::game::machinery_liquid::use_item(game, stack, claimed);
    if *claimed {
        return;
    }
    if let Some(value) = fluid_logged::value_of_bucket(&stack.item) {
        use_filled(game, stack, value, claimed);
    } else if &*stack.item == fluid_logged::EMPTY_BUCKET {
        use_empty(game, stack, claimed);
    }
}

pub(in crate::game) fn face_of(direction: Direction) -> Face {
    match direction {
        Direction::Down => Face::Down,
        Direction::Up => Face::Up,
        Direction::North => Face::North,
        Direction::South => Face::South,
        Direction::West => Face::West,
        Direction::East => Face::East,
    }
}

fn with_side(level: &GameLevel<'_>, state: BlockStateId, side: Face, joined: bool) -> BlockStateId {
    let value = if joined { "true" } else { "false" };
    level.native_state_with_property(state, side.name(), value).unwrap_or(state)
}

fn is_cable_state(level: &GameLevel<'_>, state: BlockStateId) -> bool {
    level.native_state_block(state).is_some_and(|block| cable_capacity(block).is_some())
}

/// O bloco sendo colocado. Um transmissor liga cada lado só ao que troca energia por aquela face. Um bloco com regra de colocação (`machinery_basis/placement`) escolhe
/// o `facing` pelo jogador ou pela face clicada e o `fluid_logged` pelo fluido que estava no lugar (`AttributeStateFacing.getStateForPlacement`, `BlockStateHelper.
/// getStateForPlacement`).
pub(in crate::game) fn state_for_placement(
    level: &GameLevel<'_>,
    state: BlockStateId,
    context: &PlacementContext,
    fluid: FluidState,
    answer: &mut Option<Option<BlockStateId>>,
) {
    let pos = context.pos;
    let rules = placement::current();
    let rule = level.native_state_block(state).and_then(|block| rules.get(block));
    if is_cable_state(level, state) {
        // `BlockTransmitter.getStateForPlacement`: só o fluido. As conexões vêm do `Transmitter` (a block entity), e o dono as publica nas propriedades de lado.
        let placed = Face::ALL.iter().fold(state, |placed, &side| with_side(level, placed, side, false));
        let placed = rule.map_or(placed, |rule| with_fluid(level, placed, rule, fluid));
        *answer = Some(Some(placed));
        return;
    }
    let Some(rule) = rule else {
        return;
    };
    let mut placed = state;
    if let Some(facing) = &rule.facing
        && let Some(direction) = facing.direction(context.yaw, context.pitch, face_of(context.face).name())
    {
        placed = level.native_state_with_property(placed, &facing.property, direction).unwrap_or(placed);
    }
    *answer = Some(Some(with_fluid(level, placed, rule, fluid)));
}

/// O `fluid_logged` do estado pelo fluido do lugar (a fonte de água dá `water`, a de lava `lava`, o resto `empty`), se a regra tem a propriedade.
fn with_fluid(level: &GameLevel<'_>, state: BlockStateId, rule: &placement::Rule, fluid: FluidState) -> BlockStateId {
    let Some(property) = &rule.fluid_logged else {
        return state;
    };
    let value = placement::fluid_logged_value(match fluid.kind() {
        FluidKind::Water => "water",
        FluidKind::Lava => "lava",
        _ => "empty",
    });
    level.native_state_with_property(state, property, value).unwrap_or(state)
}

/// Um vizinho mudou: o transmissor refaz a ligação daquele lado, e a lava do bloco olha de novo
/// para onde pode correr.
pub(in crate::game) fn update_shape(
    level: &GameLevel<'_>,
    ticks: &mut Ticks,
    pos: BlockPos,
    state: BlockStateId,
    direction: Direction,
    around: BlockStateId,
    new: &mut Option<BlockStateId>,
) {
    crate::game::machinery_liquid::update_shape(level, ticks, pos, state, direction, around, new);
    let transmitter = level.native_state_block(state).is_some_and(transmitters::keeps_state_on_neighbor_shape);
    if SWAP_PROBE && transmitter {
        let props = level.registry.state(state).map(|s| s.properties.clone());
        eprintln!("MBS {{\"e\":\"update_shape\",\"p\":[{},{},{}],\"dir\":\"{direction:?}\",\"around\":{:?},\"liquid_new\":{},\"props\":{:?}}}", pos.x, pos.y, pos.z, level.native_state_block(around), new.is_some(), props);
    }
    if new.is_some() {
        return;
    }
    // `IStateFluidLoggable.updateFluids`: o fluido guardado agenda o próprio tick a cada atualização de forma (o transmissor também, `BlockTransmitter.updateShape`).
    let fluid = level
        .registry
        .state(state)
        .and_then(|s| s.properties.get(fluid_logged::PROPERTY))
        .and_then(|value| fluid_of(value));
    if let Some(fluid) = fluid {
        let delay = tick_delay(fluid, matches!(level.dimension, LevelDimension::Nether));
        level.native_schedule_fluid_tick(ticks, [pos.x, pos.y, pos.z], fluid, delay);
    }
    if transmitter {
        // As conexões do transmissor são do `Transmitter`, e não da forma: um vizinho que muda não troca o braço (o dono o faz, `sendUpdatePacket`).
        let _ = (direction, around);
        *new = Some(state);
    }
}

/// O nome das pastas da própria lib, entre as fontes de dados.
const BASIS_SOURCE: &str = "machinery_basis";

pub(in crate::game) fn reload_listeners(listeners: &mut Vec<NativeReloadListener>) {
    listeners.push(NativeReloadListener::json(LISTENER, resource::DATA_FOLDER));
    listeners.push(NativeReloadListener::json(TAGS_LISTENER, resource::TAGS_FOLDER));
    listeners.push(NativeReloadListener::json(ENERGY_LISTENER, network::ENERGY_FOLDER));
    listeners.push(NativeReloadListener::json(PLACEMENT_LISTENER, placement::FOLDER));
    listeners.push(NativeReloadListener::json(UPGRADES_LISTENER, upgrade::TYPES_FOLDER));
    listeners.push(NativeReloadListener::json(UPGRADE_TAGS_LISTENER, upgrade::TAGS_FOLDER));
    listeners.push(NativeReloadListener::json(LIQUIDS_LISTENER, liquid_data::FOLDER));
}

pub(in crate::game) fn reload_apply(
    listener: &NativeReloadListener,
    files: &[(String, serde_json::Value)],
) {
    if listener.id == TAGS_LISTENER {
        resource::set_tags_source(BASIS_SOURCE, Tags::load(files));
        return;
    }
    if listener.id == ENERGY_LISTENER {
        network::set_current(EnergyBlocks::load(files));
        return;
    }
    if listener.id == LIQUIDS_LISTENER {
        let (liquids, errors) = liquid_data::Liquids::load(files);
        for (id, error) in errors {
            eprintln!("machinery_basis: fluido {id} recusado: {error}");
        }
        eprintln!("machinery_basis: {} fluidos", liquids.len());
        liquid_data::set_source(BASIS_SOURCE, liquids);
        return;
    }
    if listener.id == PLACEMENT_LISTENER {
        placement::set_current(PlacementRules::load(files));
        return;
    }
    if listener.id == UPGRADE_TAGS_LISTENER {
        upgrade::set_tags_source(BASIS_SOURCE, UpgradeTags::load(files));
        return;
    }
    if listener.id == UPGRADES_LISTENER {
        let (types, errors) = UpgradeTypes::load(files);
        for (id, error) in errors {
            eprintln!("machinery_basis: upgrade {id} recusado: {error}");
        }
        eprintln!("machinery_basis: {} upgrades", types.len());
        upgrade::set_types_source(BASIS_SOURCE, types);
        return;
    }
    if listener.id != LISTENER {
        return;
    }
    let (registry, errors) = Registry::load(files);
    for (id, error) in errors {
        eprintln!("machinery_basis: recurso {id} recusado: {error}");
    }
    eprintln!("machinery_basis: {} recursos", registry.len());
    resource::set_source(BASIS_SOURCE, registry);
}

/// A carga de dados em uso: muda a cada `data_loaded`.
pub(in crate::game) fn data_generation() -> u64 {
    DATA_GENERATION.load(Ordering::Acquire)
}

/// Os dados mudaram (`MekanismRecipeType.clearCache`): os índices de receitas se limpam e se refazem no próximo uso.
pub(in crate::game) fn data_loaded() {
    DATA_GENERATION.fetch_add(1, Ordering::AcqRel);
    MACHINE_RECIPES.lock().unwrap_or_else(|e| e.into_inner()).clear();
    CONVERSIONS.lock().unwrap_or_else(|e| e.into_inner()).clear();
    ENERGY_CONVERSIONS.lock().unwrap_or_else(|e| e.into_inner()).clear();
    INCOMPLETE.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

/// A tabela dos estados que levam o sombreamento do NeoForge (`EnhancedBlockModelLighter`, `enhanced_lighting.rs`): os de bloco de mod. O NeoForge o dá a todo bloco; o mesher do Horizon o dá só aos
/// de mod, e o dos blocos do vanilla segue o `BlockModelLighter` do vanilla. Refeita a cada carga de dados.
pub(in crate::game) fn index_lighting_states(game: &Game<'_>) {
    let registry = game.pack.assets.registry;
    let table: Vec<bool> = (0..registry.len())
        .map(|index| registry.state(BlockStateId(index as u32)).is_some_and(|state| !state.name.starts_with("minecraft:")))
        .collect();
    horizon_client::mesher::machinery_enhanced::set(table);
}

/// Se a entrada de chave de componentes `patch_key` passa no ingrediente `tree`: o patch dela é o que `PATCHES` guarda para a chave.
pub(in crate::game) fn tree_accepts(tree: &Tree, item: &str, patch_key: Option<&str>) -> bool {
    let patch = patch_key
        .and_then(|key| PATCHES.lock().unwrap_or_else(|e| e.into_inner()).get(key).cloned())
        .unwrap_or_default();
    tree.accepts(item, &patch)
}

/// A árvore de um ingrediente do Horizon, e se há componentes nela.
fn build_node(ingredient: &Ingredient, defaults: &horizon_world::item_defaults::Defaults) -> Result<(Tree, bool), Problem> {
    if let Some((kind, data)) = &ingredient.custom {
        if kind != components_ingredient::KIND {
            let other = ingredient.clone();
            let test = move |item: &str| {
                other.test(&ItemStack { item: Arc::from(item), count: 1, damage: 0, extra: None })
            };
            return Ok((Tree::Opaque(Box::new(test)), false));
        }
        let parsed = ComponentsIngredient::parse(data, |value| canon_tag(&json_tag(value)))
            .ok_or_else(|| Problem::Parse("`components` ou `strict` inválido".into()))?;
        let items = ingredient.order.clone();
        let table = items.iter().map(|item| (item.clone(), default_texts(defaults, item))).collect();
        return Ok((Tree::Components(Leaf { ingredient: parsed, items, defaults: table }), true));
    }
    let Some((shape, kids)) = &ingredient.children else {
        return Ok((Tree::Plain(ingredient.order.clone()), false));
    };
    let mut nodes = Vec::with_capacity(kids.len());
    let mut any = false;
    for kid in kids {
        let (node, has) = build_node(kid, defaults)?;
        any |= has;
        nodes.push(node);
    }
    if !any {
        return Ok((Tree::Plain(ingredient.order.clone()), false));
    }
    let node = match shape {
        Shape::Compound => Tree::Compound(nodes),
        Shape::Intersection => Tree::Intersection(nodes),
        Shape::Difference => {
            let mut nodes = nodes.into_iter();
            match (nodes.next(), nodes.next()) {
                (Some(base), Some(subtracted)) => Tree::Difference(Box::new(base), Box::new(subtracted)),
                _ => return Err(Problem::Parse("`difference` sem `base` e `subtracted`".into())),
            }
        }
    };
    Ok((node, true))
}

/// A árvore de um ingrediente com `neoforge:components` em algum ramo; `None` sem nenhum (o jogo o testa sozinho).
fn node_of(game: &Game<'_>, ingredient: &Ingredient) -> Result<Option<Arc<Tree>>, Problem> {
    let defaults = horizon_world::item_defaults::shared(&game.options.resources)
        .map_err(|error| Problem::Parse(format!("componentes padrão: {error}")))?;
    let (node, has) = build_node(ingredient, &defaults)?;
    Ok(has.then(|| Arc::new(node)))
}

/// O valor JSON de um componente como `Tag`, para o texto canônico ser o mesmo do patch de uma pilha do jogo.
fn json_tag(value: &serde_json::Value) -> Tag {
    match value {
        serde_json::Value::Null => Tag::Compound(Compound::default()),
        serde_json::Value::Bool(flag) => Tag::Byte(i8::from(*flag)),
        serde_json::Value::Number(number) => match number.as_i64() {
            Some(whole) => i32::try_from(whole).map_or(Tag::Long(whole), Tag::Int),
            None => Tag::Double(number.as_f64().unwrap_or(0.0).to_bits()),
        },
        serde_json::Value::String(text) => Tag::String(JavaString::from(text.as_str())),
        serde_json::Value::Array(list) => Tag::List(list.iter().map(json_tag).collect()),
        serde_json::Value::Object(map) => {
            Tag::Compound(map.iter().map(|(key, value)| (JavaString::from(key.as_str()), json_tag(value))).collect())
        }
    }
}

/// Os componentes padrão de um item em texto canônico.
fn default_texts(defaults: &horizon_world::item_defaults::Defaults, item: &str) -> BTreeMap<String, String> {
    defaults
        .get(item)
        .and_then(serde_json::Value::as_object)
        .map(|components| components.iter().map(|(kind, value)| (kind.clone(), canon_tag(&json_tag(value)))).collect())
        .unwrap_or_default()
}

/// Por que um ingrediente não virou nada: o arquivo está errado (o codec o recusaria: `Couldn't parse data file`), ou o ingrediente
/// não tem item nenhum (uma tag vazia: o Mekanism guarda a receita, que não casa com nada, e avisa em `Incomplete recipe detected`).
enum Problem {
    Parse(String),
    Empty(Vec<String>),
}

/// O texto de `Problem` para a receita `id`, no log, como o original: `Couldn't parse data file` do `SimpleJsonResourceReloadListener`, e
/// `Incomplete recipe detected` com `Empty tag` (`MekanismRecipeType.checkMyIncompleteRecipes`, `ItemStackIngredient.logMissingTags`).
fn report(id: &str, problem: &Problem) {
    match problem {
        Problem::Parse(why) => eprintln!("machinery_basis: Couldn't parse data file '{id}': {why}"),
        Problem::Empty(tags) => {
            INCOMPLETE.lock().unwrap_or_else(|e| e.into_inner()).insert(id.to_owned());
            eprintln!("machinery_basis: Incomplete recipe detected: {id}");
            for tag in tags {
                eprintln!("machinery_basis: Empty tag: {tag}");
            }
        }
    }
}

/// O que `native_recipe_ingredient` não achou: um item que o jogo não conhece é erro do arquivo, e uma tag sem membros deixa a receita
/// incompleta.
fn missing_items(json: &serde_json::Value) -> Problem {
    let names: Vec<&str> = match json {
        serde_json::Value::String(name) => vec![name.as_str()],
        serde_json::Value::Array(list) => list.iter().filter_map(serde_json::Value::as_str).collect(),
        other => return Problem::Parse(format!("ingrediente inválido: {other}")),
    };
    let tags: Vec<String> = names.iter().filter_map(|name| name.strip_prefix('#')).map(str::to_owned).collect();
    if !tags.is_empty() && tags.len() == names.len() {
        return Problem::Empty(tags);
    }
    Problem::Parse(format!("item desconhecido em {json}"))
}

/// O ingrediente de item de uma receita de máquina. Um item, uma tag ou uma lista saem pelo jogo (`native_recipe_ingredient`, que também
/// tira os itens que ele não conhece). Um objeto (`neoforge:components`, `compound`, `intersection`, `difference`) o jogo não entrega à
/// mod, então o arquivo passa pelo `Ingredient::parse` do Horizon (com a injeção `components-ingredient`, que lê o
/// `neoforge:components` com as tags resolvidas) e a árvore sai daqui.
fn machine_ingredient(game: &Game<'_>, json: &serde_json::Value) -> Result<(Ingredient, Option<Arc<Tree>>), Problem> {
    if !json.is_object() {
        return game.native_recipe_ingredient(json).map(|ingredient| (ingredient, None)).ok_or_else(|| missing_items(json));
    }
    let ingredient = Ingredient::parse(json, game.items())
        .ok_or_else(|| Problem::Parse(format!("o Horizon não leu o ingrediente {json}")))?;
    let node = node_of(game, &ingredient)?;
    Ok((ingredient, node))
}

fn custom_recipes(game: &Game<'_>, recipe_type: &str, fields: &RecipeFields) -> Vec<MachineRecipe> {
    let mut recipes = Vec::new();
    for recipe in game.native_recipes_of_type(recipe_type) {
        let Some(parsed) = MachineRecipeData::parse(&recipe.json, fields) else {
            report(&recipe.id, &Problem::Parse(MachineRecipeData::why_not(&recipe.json, fields)));
            continue;
        };
        let (item, components) = match machine_ingredient(game, &parsed.item.ingredient) {
            Ok(found) => found,
            Err(problem) => {
                report(&recipe.id, &problem);
                continue;
            }
        };
        let Some(output) = game.native_recipe_result(&parsed.output) else {
            report(&recipe.id, &Problem::Parse(format!("resultado inválido ou com item desconhecido: {}", parsed.output)));
            continue;
        };
        // Uma tag de recurso sem membro (`ChemicalStackIngredient.hasNoMatchingInstances`) deixa a receita incompleta, como a de item. O
        // original a guarda, e ela não casa com nada: aqui a receita fica, com o mesmo efeito, e só o aviso sai.
        if let Some(tag) = parsed.resource.as_ref().and_then(|input| input.ingredient.strip_prefix(resource::TAG_PREFIX))
            && resource::current_tags().members(tag).next().is_none()
        {
            report(&recipe.id, &Problem::Empty(vec![tag.to_owned()]));
        }
        recipes.push(MachineRecipe {
            id: recipe.id.clone(),
            item,
            components,
            count: parsed.item.count,
            resource: parsed.resource,
            output,
            per_tick_usage: parsed.per_tick_usage,
            experience: recipe.json.get("experience").and_then(serde_json::Value::as_f64).unwrap_or(0.0) as f32,
        });
    }
    recipes
}

/// A árvore do ingrediente de uma receita de cozimento do jogo, se tem `neoforge:components` nela. Se a árvore não se monta, a receita fica com
/// o teste do Horizon e o aviso no log.
fn cooking_node(game: &Game<'_>, recipe: &Cooking) -> Option<Arc<Tree>> {
    match node_of(game, &recipe.ingredient) {
        Ok(node) => node,
        Err(problem) => {
            report(&recipe.id, &problem);
            None
        }
    }
}

fn cooking_recipes(game: &Game<'_>, kind: &str) -> Vec<MachineRecipe> {
    let of_kind = |recipe: &&Cooking| recipe.kind.strip_prefix("minecraft:").unwrap_or(&recipe.kind) == kind;
    let cooking = game.recipes().cooking.iter().filter(of_kind);
    cooking
        .map(|recipe| MachineRecipe {
            id: recipe.id.clone(),
            item: recipe.ingredient.clone(),
            components: cooking_node(game, recipe),
            count: 1,
            resource: None,
            output: recipe.result.clone(),
            per_tick_usage: false,
            experience: recipe.experience,
        })
        .collect()
}

/// As receitas de uma máquina. Fica de fora a que cita um item que o jogo não conhece.
pub(in crate::game) fn machine_recipes(
    game: &Game<'_>,
    source: &RecipeSource,
) -> Arc<Vec<MachineRecipe>> {
    let mut index = MACHINE_RECIPES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(recipes) = index.get(source.key()) {
        return recipes.clone();
    }
    let recipes = Arc::new(match source {
        RecipeSource::Custom(recipe_type, fields) => custom_recipes(game, recipe_type, fields),
        RecipeSource::Cooking(kind) => cooking_recipes(game, kind),
    });
    index.insert(source.key().to_owned(), recipes.clone());
    recipes
}

/// As conversões de um tipo. Fica de fora a que dá um recurso que ninguém declarou.
pub(in crate::game) fn conversions(
    game: &Game<'_>,
    recipe_type: &str,
    fields: &ConversionFields,
) -> Arc<Vec<ItemConversion>> {
    let mut index = CONVERSIONS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(conversions) = index.get(recipe_type) {
        return conversions.clone();
    }
    let registry = resource::current();
    let mut loaded = Vec::new();
    for recipe in game.native_recipes_of_type(recipe_type) {
        let Some(parsed) = Conversion::parse(&recipe.json, fields) else {
            report(&recipe.id, &Problem::Parse(Conversion::why_not(&recipe.json, fields)));
            continue;
        };
        if registry.get(&parsed.resource).is_none() {
            report(&recipe.id, &Problem::Parse(format!("o recurso {} não existe", parsed.resource)));
            continue;
        }
        let (item, components) = match machine_ingredient(game, &parsed.item.ingredient) {
            Ok(found) => found,
            Err(problem) => {
                report(&recipe.id, &problem);
                continue;
            }
        };
        loaded.push(ItemConversion {
            id: recipe.id.clone(),
            item,
            components,
            count: parsed.item.count,
            resource: parsed.resource,
            amount: parsed.amount,
        });
    }
    let conversions = loaded;
    let conversions = Arc::new(conversions);
    index.insert(recipe_type.to_owned(), conversions.clone());
    conversions
}

/// As conversões de item em energia de um tipo (`energy_conversion`), na ordem em que o jogo as entrega. Fica de fora a que cita
/// um item ou uma tag que o jogo não conhece (`native_recipe_ingredient` devolve `None`), e a que não tem uma energia inteira e
/// positiva.
pub(in crate::game) fn energy_conversions(
    game: &Game<'_>,
    recipe_type: &str,
    fields: &ConversionFields,
) -> Arc<Vec<ItemEnergyConversion>> {
    let mut index = ENERGY_CONVERSIONS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(conversions) = index.get(recipe_type) {
        return conversions.clone();
    }
    let mut loaded = Vec::new();
    for recipe in game.native_recipes_of_type(recipe_type) {
        let Some(parsed) = EnergyConversionData::parse(&recipe.json, fields) else {
            report(&recipe.id, &Problem::Parse(EnergyConversionData::why_not(&recipe.json, fields)));
            continue;
        };
        let (item, components) = match machine_ingredient(game, &parsed.item.ingredient) {
            Ok(found) => found,
            Err(problem) => {
                report(&recipe.id, &problem);
                continue;
            }
        };
        loaded.push(ItemEnergyConversion {
            id: recipe.id.clone(),
            item,
            components,
            count: parsed.item.count,
            amount: parsed.amount,
        });
    }
    let conversions = loaded;
    let conversions = Arc::new(conversions);
    index.insert(recipe_type.to_owned(), conversions.clone());
    conversions
}

/// `MekanismRecipeType.checkIncompleteRecipes`: lê agora as receitas dos tipos que o mod usa (o que o primeiro uso faria depois da carga de
/// dados) e devolve os ids das que ficaram incompletas, em ordem. Cada uma já foi ao log (`Incomplete recipe detected: <id>`, com
/// `Empty tag: <tag>`), como `checkMyIncompleteRecipes` e `logMissingTags`.
pub(in crate::game) fn incomplete_recipes(
    game: &Game<'_>,
    machines: &[RecipeSource],
    conversions: &[(&str, &ConversionFields)],
    energy: &[(&str, &ConversionFields)],
) -> Vec<String> {
    for source in machines {
        machine_recipes(game, source);
    }
    for (recipe_type, fields) in conversions {
        self::conversions(game, recipe_type, fields);
    }
    for (recipe_type, fields) in energy {
        energy_conversions(game, recipe_type, fields);
    }
    INCOMPLETE.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect()
}

/// `Item.getMaxStackSize` de uma pilha: o componente `minecraft:max_stack_size` dela, se tem, e senão o do item
/// (`ItemResource.getMaxStackSize` lê os componentes do recurso).
pub(in crate::game) fn stack_limit(game: &Game<'_>, stack: &ItemStack) -> u32 {
    let patched = stack
        .data()
        .and_then(|data| data.other(MAX_STACK_SIZE_COMPONENT))
        .and_then(|tag| match tag {
            Tag::Int(limit) => u32::try_from(*limit).ok(),
            _ => None,
        });
    patched.unwrap_or_else(|| game.native_max_stack_size(&stack.item).unwrap_or(DEFAULT_STACK))
}

/// O que um `ItemStack` do jogo tem além do id e da quantidade, guardado por chave de componentes: o dano e os componentes que o
/// distinguem. A lib compara o `ItemResource` por esses componentes em texto canônico, e o jogo precisa do que o texto veio para
/// refazer a pilha quando o item sai de um slot da lib. Cresce com os conjuntos de componentes distintos que já passaram por uma
/// máquina.
type Extras = (u32, Option<Box<StackData>>);
static EXTRAS: Mutex<BTreeMap<String, Extras>> = Mutex::new(BTreeMap::new());
/// Os componentes em texto canônico de cada chave de `EXTRAS`: o teste de um ingrediente de componentes (`neoforge:components`) só recebe a
/// chave da entrada e refaz a pilha daqui.
static PATCHES: Mutex<BTreeMap<String, Vec<(String, String)>>> = Mutex::new(BTreeMap::new());
const MAX_STACK_SIZE_COMPONENT: &str = "minecraft:max_stack_size";

/// O `DataComponentPatch` de uma pilha como a lib o compara: cada componente com o valor em texto canônico, o dano entre eles
/// (`minecraft:damage`). Registra o que a pilha tem para refazê-la.
fn item_components(stack: &ItemStack) -> Components {
    let Some(Tag::Compound(entries)) = encode_components(stack) else {
        return Components::none();
    };
    let texts: Vec<(String, String)> = entries
        .iter()
        .map(|(id, value)| (id.to_string_lossy(), canon_tag(value)))
        .collect();
    let components = Components::new(texts.iter().map(|(id, value)| (id.as_str(), value.as_str())));
    let mut extras = EXTRAS.lock().unwrap_or_else(|e| e.into_inner());
    extras
        .entry(components.key().to_owned())
        .or_insert_with(|| (stack.damage, stack.extra.clone()));
    PATCHES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entry(components.key().to_owned())
        .or_insert_with(|| components.iter().map(|(kind, value)| (kind.to_owned(), value.to_owned())).collect());
    components
}

/// `ItemResource.of(stack)`: o item, a pilha máxima dele e os componentes que o distinguem.
pub(in crate::game) fn item_resource(game: &Game<'_>, stack: &ItemStack) -> ItemResource {
    item_resource_with_limit(stack, stack_limit(game, stack))
}

/// O mesmo com o limite de pilha já sabido: para quem não tem o jogo na mão (o `can_take` do Horizon).
pub(in crate::game) fn item_resource_with_limit(stack: &ItemStack, limit: u32) -> ItemResource {
    ItemResource::with_components(&stack.item, limit, item_components(stack))
}

/// O que um slot da lib guarda, de uma pilha do jogo.
pub(in crate::game) fn slot_contents(game: &Game<'_>, stack: Option<&ItemStack>) -> SlotContents {
    match stack {
        Some(stack) => SlotContents::new(Some(item_resource(game, stack)), stack.count),
        None => SlotContents::default(),
    }
}

/// `ItemResource.toStack(amount)`: a pilha de um recurso, com o dano e os componentes que ele tem. `None` quando o recurso tem
/// componentes que nunca passaram por uma pilha do jogo (quem o fez na lib os monta à mão).
pub(in crate::game) fn stack_of_resource(resource: &ItemResource, count: u32) -> Option<ItemStack> {
    let (damage, extra) = if resource.components.is_empty() {
        (0, None)
    } else {
        EXTRAS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(resource.components.key())
            .cloned()?
    };
    Some(ItemStack {
        item: Arc::from(&*resource.id),
        count,
        damage,
        extra,
    })
}

/// Os componentes de um recurso como o `components` do `ItemResource.CODEC` os grava, para o NBT: o dano e cada componente na forma
/// que a pilha tem. `None` sem componentes ou quando o recurso não veio de uma pilha do jogo.
pub(in crate::game) fn components_tag(resource: &ItemResource) -> Option<Tag> {
    encode_components(&stack_of_resource(resource, 1)?)
}

/// O recurso do `ItemResource.CODEC` lido do NBT: o id e o `components` (dano e componentes), com a pilha máxima do item.
pub(in crate::game) fn resource_from_tag(game: &Game<'_>, id: &str, components: Option<&Tag>) -> ItemResource {
    let (damage, extra) = decode_components(components);
    let stack = ItemStack {
        item: Arc::from(id),
        count: 1,
        damage,
        extra,
    };
    item_resource(game, &stack)
}

// ---------------------------------------------------------------------------------------------------------------------------
// O componente de upgrades (`TileComponentUpgrade`) no NBT do tile e nos slots do menu. Vale para quem tem o objeto vivo (as máquinas do
// Mekanism) e para quem se refaz do NBT a cada tick (o Heat Generator).

/// `SerializationConstants.COMPONENT_UPGRADE`: o `TileComponentUpgrade` grava num filho com este nome.
const COMPONENT_UPGRADE: &str = "component_upgrade";
/// `SerializationConstants.UPGRADES` e `ITEMS`: o mapa de upgrades instalados e a lista dos dois slots.
const UPGRADES_KEY: &str = "upgrades";
/// A ordem de escrita das chaves de `upgrades` (decisão 233, H259), guardada no compound da lib (`SAVED`) e não no do original.
const UPGRADE_ORDER_KEY: &str = "upgrade_order";
const ITEMS_KEY: &str = "items";
/// `ContainerType.ITEM.saveTo` e `LargeResourceStack.codec`: `{container: byte, stored: {type: {id, components}, amount}}`.
const CONTAINER_KEY: &str = "container";
const STORED_KEY: &str = "stored";
const TYPE_KEY: &str = "type";
const AMOUNT_KEY: &str = "amount";
/// O `ItemResource.CODEC`: o id do item e os componentes que o distinguem.
const ITEM_ID_KEY: &str = "id";
const COMPONENTS_KEY: &str = "components";

/// `LargeResourceStack.codec` de um `ItemResource`: `type` (`id` e `components`) e `amount`. Os componentes são o `DataComponentPatch`
/// inteiro (o dano e cada componente, o tipo do upgrade entre eles). Um slot que se lê mal vale vazio, como o `readOrEmpty`.
fn upgrade_slot_contents(game: &Game<'_>, stored: &Compound) -> SlotContents {
    let Some(Tag::Compound(kind)) = stored.get(&JavaString::from(TYPE_KEY)) else {
        return SlotContents::default();
    };
    let Some(Tag::String(id)) = kind.get(&JavaString::from(ITEM_ID_KEY)) else {
        return SlotContents::default();
    };
    let id = id.to_string_lossy();
    let amount = match stored.get(&JavaString::from(AMOUNT_KEY)) {
        Some(Tag::Long(amount)) => u32::try_from(*amount).ok(),
        Some(Tag::Int(amount)) => u32::try_from(*amount).ok(),
        _ => None,
    };
    let resource = resource_from_tag(game, &id, kind.get(&JavaString::from(COMPONENTS_KEY)));
    SlotContents::new(Some(resource), amount.unwrap_or(0))
}

/// `TileComponentUpgrade.deserialize`: `upgrades` (o mapa de tipo para total) e os dois slots, na lista `items`.
pub(in crate::game) fn read_upgrades(game: &Game<'_>, entity: &ModBlockEntity) -> UpgradeSaved {
    // Antes de o local `saved` sombrear a função `saved`.
    let order_tag = saved(entity).and_then(|lib| lib.get(&JavaString::from(UPGRADE_ORDER_KEY)));
    let mut saved = UpgradeSaved::default();
    let Some(Tag::Compound(component)) = entity.nbt.get(&JavaString::from(COMPONENT_UPGRADE)) else {
        return saved;
    };
    if let Some(Tag::Compound(upgrades)) = component.get(&JavaString::from(UPGRADES_KEY)) {
        for (id, total) in upgrades {
            if let Tag::Int(total) = total {
                saved.upgrades.insert(id.to_string_lossy(), *total);
            }
        }
    }
    if let Some(Tag::List(order)) = order_tag {
        saved.file_order = order
            .iter()
            .filter_map(|id| match id {
                Tag::String(id) => Some(id.to_string_lossy()),
                _ => None,
            })
            .collect();
    }
    if let Some(Tag::List(items)) = component.get(&JavaString::from(ITEMS_KEY)) {
        for entry in items {
            let Tag::Compound(entry) = entry else {
                continue;
            };
            let slot = match entry.get(&JavaString::from(CONTAINER_KEY)) {
                Some(Tag::Byte(slot)) => *slot,
                _ => -1,
            };
            let contents = match entry.get(&JavaString::from(STORED_KEY)) {
                Some(Tag::Compound(stored)) => upgrade_slot_contents(game, stored),
                _ => SlotContents::default(),
            };
            match slot {
                0 => saved.input = contents,
                1 => saved.output = contents,
                _ => {}
            }
        }
    }
    saved
}

/// O `components` do `ItemResource.CODEC` de um recurso, ou nada sem componentes. Um recurso que veio de uma pilha do jogo tem o
/// patch inteiro. O que a lib faz sozinha (`UpgradeData.resource_of`, o item com o tipo e mais nada) nunca passou por uma pilha:
/// o tipo é o único componente e é um texto.
fn resource_components(resource: &ItemResource) -> Option<Tag> {
    if resource.components.is_empty() {
        return None;
    }
    if let Some(components) = components_tag(resource) {
        return Some(components);
    }
    let mut components = Compound::default();
    for (id, value) in resource.components.iter() {
        let Some(text) = Components::parse_text_value(value) else {
            eprintln!("machinery_basis: o componente {id} do upgrade não é um texto e não se grava");
            continue;
        };
        components.insert(JavaString::from(id), Tag::String(JavaString::from(text.as_str())));
    }
    Some(Tag::Compound(components))
}

/// `TileComponentUpgrade.serialize`: o filho some quando não tem nada.
pub(in crate::game) fn write_upgrades(entity: &mut ModBlockEntity, saved: &UpgradeSaved) {
    let mut component = Compound::default();
    if !saved.upgrades.is_empty() {
        let mut upgrades = Compound::default();
        for (id, total) in &saved.upgrades {
            upgrades.insert(JavaString::from(id.as_str()), Tag::Int(*total));
        }
        component.insert(JavaString::from(UPGRADES_KEY), Tag::Compound(upgrades));
    }
    let mut items = Vec::new();
    for (index, contents) in [&saved.input, &saved.output].into_iter().enumerate() {
        let Some(resource) = &contents.resource else {
            continue;
        };
        let mut kind = Compound::default();
        kind.insert(JavaString::from(ITEM_ID_KEY), Tag::String(JavaString::from(&*resource.id)));
        if let Some(components) = resource_components(resource) {
            kind.insert(JavaString::from(COMPONENTS_KEY), components);
        }
        let mut stored = Compound::default();
        stored.insert(JavaString::from(TYPE_KEY), Tag::Compound(kind));
        stored.insert(JavaString::from(AMOUNT_KEY), Tag::Long(i64::from(contents.amount)));
        let mut entry = Compound::default();
        entry.insert(JavaString::from(CONTAINER_KEY), Tag::Byte(index as i8));
        entry.insert(JavaString::from(STORED_KEY), Tag::Compound(stored));
        items.push(Tag::Compound(entry));
    }
    if !items.is_empty() {
        component.insert(JavaString::from(ITEMS_KEY), Tag::List(items));
    }
    let key = JavaString::from(COMPONENT_UPGRADE);
    if component.is_empty() {
        entity.nbt.remove(&key);
    } else {
        entity.nbt.insert(key, Tag::Compound(component));
    }
    write_upgrade_order(entity, &saved.file_order);
}

/// A ordem em que o original escreveria as chaves de `upgrades`: o NBT do Horizon é ordenado, então ela vai numa lista do compound da lib. Sem ordem (sem upgrades, ou um
/// `saved` de item) a chave some, e o compound da lib some junto se ficou vazio.
fn write_upgrade_order(entity: &mut ModBlockEntity, order: &[String]) {
    let key = JavaString::from(SAVED);
    let mut lib = saved(entity).cloned().unwrap_or_default();
    if order.is_empty() {
        if lib.remove(&JavaString::from(UPGRADE_ORDER_KEY)).is_none() {
            return;
        }
    } else {
        let list = order.iter().map(|id| Tag::String(JavaString::from(id.as_str()))).collect();
        lib.insert(JavaString::from(UPGRADE_ORDER_KEY), Tag::List(list));
    }
    if lib.is_empty() {
        entity.nbt.remove(&key);
    } else {
        entity.nbt.insert(key, Tag::Compound(lib));
    }
}

/// A pilha de um recurso de upgrade. O que a lib faz (`UpgradeData.resource_of`, ao remover um upgrade) é o item com o tipo no
/// componente `component` e mais nada, e nunca passou por uma pilha do jogo: monta-se aqui, e a pilha registrada é a do recurso a
/// partir de então.
pub(in crate::game) fn upgrade_stack(game: &Game<'_>, component: &str, resource: &ItemResource, count: u32) -> ItemStack {
    if let Some(stack) = stack_of_resource(resource, count) {
        return stack;
    }
    let mut stack = ItemStack::new(&resource.id, count);
    if let Some(kind) = resource.components.get(component).and_then(Components::parse_text_value) {
        let _ = game.native_set_component(&mut stack, component, Tag::String(JavaString::from(kind.as_str())));
    }
    let registered = item_resource(game, &stack);
    if registered != *resource {
        eprintln!("machinery_basis: o upgrade {:?} saiu com outros componentes ({:?})", resource.components.key(), registered.components.key());
    }
    stack
}

/// O item que um slot de upgrade guarda, com o tipo no componente `component`.
pub(in crate::game) fn upgrade_slot_stack(game: &Game<'_>, component: &str, contents: &SlotContents) -> Option<ItemStack> {
    let resource = contents.resource.as_ref()?;
    Some(upgrade_stack(game, component, resource, contents.amount))
}

/// O que o componente deixou num slot de upgrade (instalar tira da entrada, remover enche a saída), se a célula do menu não é mais
/// isso: a célula nova (`None` por dentro quando o slot ficou vazio). `None` por fora é a célula igual, que não se mexe.
pub(in crate::game) fn upgrade_cell_after(
    game: &Game<'_>,
    component: &str,
    cell: Option<&ItemStack>,
    contents: &SlotContents,
) -> Option<Option<ItemStack>> {
    let same = match (&contents.resource, cell) {
        (None, None) => true,
        (Some(resource), Some(stack)) => item_resource(game, stack) == *resource && contents.amount == stack.count,
        _ => false,
    };
    (!same).then(|| upgrade_slot_stack(game, component, contents))
}

/// Leva para a célula do menu o que o componente deixou no slot de upgrade, só quando mudou.
pub(in crate::game) fn upgrade_to_cell(game: &Game<'_>, component: &str, cell: &mut Option<ItemStack>, contents: &SlotContents) {
    if let Some(new) = upgrade_cell_after(game, component, cell.as_ref(), contents) {
        *cell = new;
    }
}

/// Insere até `amount` de `item` numa célula de item do jogo (um slot do bloco ou do inventário): só junta o mesmo recurso, o item e os
/// componentes, até a capacidade (`BasicResourceContainer.insert`), e uma célula vazia recebe o item. Quantos entraram. A regra de
/// aceitar é de quem chama.
pub(in crate::game) fn insert_into_cell(
    game: &Game<'_>,
    cell: &mut Option<ItemStack>,
    item: &ItemStack,
    amount: u32,
) -> u32 {
    let resource = item_resource(game, item);
    let held = slot_contents(game, cell.as_ref());
    let moved = amount.min(room_for(&held, &resource, resource.slot_capacity()));
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

/// Tira `amount` de uma célula de item (`Slot.remove`): ela fica vazia ao chegar a zero.
pub(in crate::game) fn remove_from_cell(cell: &mut Option<ItemStack>, amount: u32) {
    let emptied = match cell {
        Some(held) => {
            held.count = held.count.saturating_sub(amount);
            held.count == 0
        }
        None => false,
    };
    if emptied {
        *cell = None;
    }
}

/// Leva o que couber de `stack` para o slot da máquina: o resto fica em `stack`. O slot só junta o que é o mesmo recurso, o item e
/// os componentes (`ItemStack.isSameItemSameComponents`).
pub(in crate::game) fn move_into_slot(
    game: &Game<'_>,
    entity: &mut ModBlockEntity,
    slot: usize,
    stack: &mut ItemStack,
) {
    let resource = item_resource(game, stack);
    let held = slot_contents(game, entity.items[slot].as_ref());
    let moved = stack.count.min(room_for(&held, &resource, resource.slot_capacity()));
    match &mut entity.items[slot] {
        Some(held) => held.count += moved,
        empty => {
            let mut placed = stack.clone();
            placed.count = moved;
            if moved > 0 {
                *empty = Some(placed);
            }
        }
    }
    stack.count -= moved;
}

/// A face da frente de uma máquina, pela propriedade `facing`.
pub(in crate::game) fn front(game: &Game<'_>, pos: [i32; 3]) -> Face {
    game.native_block_property(pos, "facing")
        .and_then(|facing| Face::from_property(&facing))
        .unwrap_or(Face::North)
}

pub(in crate::game) fn stored_energy(entity: &ModBlockEntity) -> i32 {
    match entity.nbt.get(&JavaString::from(ENERGY)) {
        Some(Tag::Int(stored)) => *stored,
        _ => 0,
    }
}

pub(in crate::game) fn set_stored_energy(entity: &mut ModBlockEntity, stored: i32) {
    entity.nbt.insert(JavaString::from(ENERGY), Tag::Int(stored));
}

/// A chave da energia exata de um tile que passa do `i32` do Horizon (o Energy Cube criativo, `Long.MAX_VALUE`): o `energy` inteiro fica no teto para o Horizon e esta guarda o valor.
pub(in crate::game) const ENERGY_AMOUNT: &str = "energy_amount";

/// A energia do tile em `i64`: a exata, se o tile guarda uma, ou a do `energy` inteiro.
pub(in crate::game) fn stored_energy_long(entity: &ModBlockEntity) -> i64 {
    match entity.nbt.get(&JavaString::from(ENERGY_AMOUNT)) {
        Some(Tag::Long(stored)) => *stored,
        _ => i64::from(stored_energy(entity)),
    }
}

/// Grava a energia em `i64`: o `energy` inteiro satura sem aviso (o valor exato fica em [`ENERGY_AMOUNT`] quando passa do `i32`).
pub(in crate::game) fn set_stored_energy_long(entity: &mut ModBlockEntity, stored: i64) {
    set_stored_energy(entity, i32::try_from(stored).unwrap_or(i32::MAX));
    if stored > i64::from(i32::MAX) {
        entity.nbt.insert(JavaString::from(ENERGY_AMOUNT), Tag::Long(stored));
    } else {
        entity.nbt.remove(&JavaString::from(ENERGY_AMOUNT));
    }
}

/// A capacidade de energia de cada máquina em jogo, por instância: o `ModEnergyBuffer` do bloco é um só para todos, e a
/// capacidade de uma máquina muda com os upgrades (`MachineEnergyContainer.setMaxEnergy`). O valor guarda o id do bloco, para
/// uma capacidade velha não valer para outro bloco que tome o lugar.
static ENERGY_CAPACITIES: Mutex<BTreeMap<(&'static str, [i32; 3]), InstanceEnergy>> =
    Mutex::new(BTreeMap::new());

/// O que a energia de uma máquina tem de próprio: o id do bloco (para uma entrada velha não valer para outro bloco que tome o
/// lugar), a capacidade e as faces por onde ela deixa entrar e sair (`ConfigHolder`: o `DataType` do lado, pela frente de agora).
#[derive(Clone, Debug, PartialEq, Eq)]
struct InstanceEnergy {
    block: String,
    capacity: i32,
    insert_faces: [bool; 6],
    extract_faces: [bool; 6],
}

/// O arquivo de configuração que o jogo leu, como o `ModConfigSpec` o lê (`acceptConfig`): um valor errado vai para o limite ou para o padrão, e o arquivo é
/// escrito de novo, no lugar de onde foi lido (o do mundo, se um do mundo trocou o de `config/`), com as chaves que a especificação não tem como estavam. O
/// Horizon avisa da mudança no segundo seguinte, e o texto novo já está certo. `previous` é o que a leitura anterior do mesmo arquivo deu: um validador que
/// olha outra chave a vê pelo valor em cache (`Spec::load_after`).
pub(in crate::game) fn read_config(
    config: &crate::game::NativeConfigFile,
    spec: &machinery_basis_core::client_config::Spec,
    previous: Option<&machinery_basis_core::client_config::Loaded>,
) -> machinery_basis_core::client_config::Loaded {
    let loaded = spec.load_after(&config.text, previous);
    if loaded.corrected {
        // `ConfigFileTypeHandler`: o arquivo de antes fica em `<nome>-1.toml.bak` (até cinco), e só então o corrigido o substitui.
        let saved = machinery_basis_core::client_config::back_up(&config.path, machinery_basis_core::client_config::MAX_BACKUPS)
            .and_then(|()| std::fs::write(&config.path, spec.rewrite(&config.text, &loaded)));
        if let Err(error) = saved {
            eprintln!("machinery_basis: não consegui corrigir {}: {error}", config.path.display());
        }
    }
    loaded
}

/// Uma capacidade em `i64` como o `ModEnergyBuffer` a guarda, em `i32`. O que passa de `i32::MAX` fica em `i32::MAX`, e isso é
/// dito no log: o tipo do Horizon é o limite, e nada o corta sem aviso.
pub(in crate::game) fn energy_as_int(value: i64, what: &str) -> i32 {
    match i32::try_from(value) {
        Ok(value) => value,
        Err(_) => {
            eprintln!("machinery_basis: {what} {value} passa do `i32` do Horizon e fica em {}", i32::MAX);
            i32::MAX
        }
    }
}

/// Dá à máquina em `pos` a capacidade e as faces de energia que ela tem agora (H1, por instância). Quem faz a máquina andar chama
/// quando uma delas muda: os upgrades, a configuração de lados ou a frente do bloco.
pub(in crate::game) fn set_energy_capacity(
    dimension: &'static str,
    pos: [i32; 3],
    block: &str,
    capacity: i32,
    insert_faces: [bool; 6],
    extract_faces: [bool; 6],
) {
    let mut capacities = ENERGY_CAPACITIES.lock().unwrap_or_else(|e| e.into_inner());
    let old = capacities.insert(
        (dimension, pos),
        InstanceEnergy { block: block.to_owned(), capacity, insert_faces, extract_faces },
    );
    // Uma capability que apareceu ou mudou de lado (a configuração de lados, a frente do bloco): `BlockEntity.invalidateCapabilities`, que o cabo vizinho ouve
    // (`RefreshListener`). O Horizon só põe as faces da instância no primeiro tick da máquina, e o cabo que a olhou antes a achou sem capability.
    let before = old.as_ref().map(|old| (old.block.as_str(), old.insert_faces, old.extract_faces));
    if capability_changed(before, (block, insert_faces, extract_faces)) {
        CAPABILITY_CHANGED.lock().unwrap_or_else(|e| e.into_inner()).push((dimension, pos));
    }
}

/// As posições cuja capability de energia mudou desde o último tick do servidor, para os cabos vizinhos ouvirem.
static CAPABILITY_CHANGED: Mutex<Vec<(&'static str, [i32; 3])>> = Mutex::new(Vec::new());

/// Esquece as capacidades das máquinas que já não andam: a tabela não cresce com os blocos quebrados.
pub(in crate::game) fn retain_energy_capacities(keep: impl Fn(&'static str, [i32; 3]) -> bool) {
    let mut capacities = ENERGY_CAPACITIES.lock().unwrap_or_else(|e| e.into_inner());
    capacities.retain(|(dimension, pos), _| keep(*dimension, *pos));
}

/// As faces de energia que a instância em `pos` declarou (entrar e sair), ou `None` se ela não declarou: o que o ejetor de um vizinho precisa para saber se há
/// capability do outro lado (`BlockCapabilityCache.getCapability` não nulo).
pub(in crate::game) fn instance_energy_faces(dimension: &'static str, pos: [i32; 3]) -> Option<([bool; 6], [bool; 6])> {
    if let Some(faces) = cable_faces(dimension, pos) {
        return Some(faces);
    }
    ENERGY_CAPACITIES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&(dimension, pos))
        .map(|energy| (energy.insert_faces, energy.extract_faces))
}

/// Capacidade por instância: a máquina com uma capacidade guardada responde com ela no lugar da do bloco. Sem uma, ou com outro
/// bloco no lugar, segue o caminho normal do Horizon.
pub(in crate::game) fn energy_capacity(
    game: &Game<'_>,
    pos: [i32; 3],
) -> std::ops::ControlFlow<Option<(Arc<horizon_client::containers::ModEnergyBuffer>, i32)>> {
    use horizon_client::containers::{BlockEntity, ModEnergyBuffer};
    use std::ops::ControlFlow::{Break, Continue};
    if let Break(answer) = cable_energy(game, pos) {
        return Break(answer);
    }
    let key = (pos[0], pos[1], pos[2]);
    let Some(InstanceEnergy { block, capacity, insert_faces, extract_faces }) = ENERGY_CAPACITIES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&(game.native_dimension(), pos))
        .cloned()
    else {
        return Continue(());
    };
    let state = game.world.block(pos[0], pos[1], pos[2]);
    let Some(buffer) = game.world.block_entities.mod_energy(state.0) else {
        return Continue(());
    };
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&key) else {
        return Continue(());
    };
    if entity.id.is_empty() || entity.id != block {
        return Continue(());
    }
    let resized = ModEnergyBuffer {
        capacity,
        max_insert: capacity,
        insert_faces,
        extract_faces,
        ..**buffer
    };
    Break(Some((Arc::new(resized), stored_energy(entity))))
}

/// Quem diz, por instância, o que os slots de itens de um bloco de mod deixam fazer (H2): as faces que a configuração de lados dá, e a
/// regra do slot para quem tira de fora. O Horizon sabe o `ModContainer` do tipo de bloco; as duas injeções (`container_instance`, no
/// fim de `Containers::view`, e `container_take`, no começo de `Containers::can_take`) perguntam a quem registrou aqui.
#[derive(Clone, Copy)]
pub struct ItemInstances {
    /// As faces de item da instância: o bloco, o que o tile guarda (o NBT e os itens) e a frente do bloco. `None` se o bloco não é seu.
    pub faces: fn(&str, &ModBlockEntity, Face) -> Option<machinery_basis_core::item_instance::InstanceItems>,
    /// `isValidForExtraction(item, EXTERNAL)` do slot: o bloco, o tile, o slot, a pilha dele e o que o catálogo do jogo sabe do item.
    pub extractable: fn(&str, &ModBlockEntity, usize, &ItemStack, ItemFacts) -> bool,
    /// `isValidForInsertion(item, EXTERNAL)` do slot, pela face (`None` é sem face: sem capability): o jogo, a posição, o tile, o slot, a pilha e a face. `None` se o bloco não é seu.
    pub accepts: fn(&Game<'_>, [i32; 3], &ModBlockEntity, usize, &ItemStack, Option<Face>) -> Option<bool>,
}

/// O que o catálogo de itens do jogo sabe de um item e que o `can_take` do Horizon, que não tem o jogo na mão, entrega a quem responde: o limite de pilha e se o item tem o
/// componente `minecraft:cooking_fuel` (`DataComponents.COOKING_FUEL`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemFacts {
    pub max_stack: u32,
    pub cooking_fuel: bool,
}

/// Quem diz, por item, qual é o handler de energia dele (`Capabilities.ENERGY` do item): o componente que guarda a energia e a conta do mod. Os blocos que carregam itens de
/// energia (o cubo, o Heat Generator) perguntam aqui, e o mod dono do item responde.
#[derive(Clone, Copy)]
pub struct ItemEnergyProvider {
    pub component: &'static str,
    /// O item (id) e a energia que a pilha guarda: o handler dele, ou `None` se não tem.
    pub spec: fn(&str, i64) -> Option<machinery_basis_core::energy_cube::ItemEnergySpec>,
}

static ITEM_ENERGY: Mutex<Vec<(&'static str, ItemEnergyProvider)>> = Mutex::new(Vec::new());

/// Registra (ou troca) quem responde pelos itens de energia de `owner`.
pub(in crate::game) fn register_item_energy(owner: &'static str, provider: ItemEnergyProvider) {
    let mut registered = ITEM_ENERGY.lock().unwrap_or_else(|e| e.into_inner());
    match registered.iter_mut().find(|(name, _)| *name == owner) {
        Some(entry) => entry.1 = provider,
        None => registered.push((owner, provider)),
    }
}

/// O handler de energia de uma pilha (um item só), sem o jogo: a energia vem do texto do componente, o mesmo que o slot do cubo lê.
pub(in crate::game) fn item_energy_spec_of(stack: &ItemStack) -> Option<machinery_basis_core::energy_cube::ItemEnergySpec> {
    let providers: Vec<ItemEnergyProvider> = ITEM_ENERGY.lock().unwrap_or_else(|e| e.into_inner()).iter().map(|(_, p)| *p).collect();
    let resource = item_resource_with_limit(stack, 1);
    providers.iter().find_map(|provider| {
        let energy = resource
            .components
            .get(provider.component)
            .and_then(|text| text.trim_end_matches(['L', 'l']).parse::<i64>().ok())
            .map_or(0, |energy| energy.max(0));
        (provider.spec)(&stack.item, energy)
    })
}

static ITEM_INSTANCES: Mutex<Vec<(&'static str, ItemInstances)>> = Mutex::new(Vec::new());

/// Registra (ou troca) quem responde pelos blocos de `owner`.
pub(in crate::game) fn register_item_instances(owner: &'static str, instances: ItemInstances) {
    let mut registered = ITEM_INSTANCES.lock().unwrap_or_else(|e| e.into_inner());
    match registered.iter_mut().find(|(name, _)| *name == owner) {
        Some(entry) => entry.1 = instances,
        None => registered.push((owner, instances)),
    }
}

thread_local! {
    /// `Containers::view` está rodando por dentro da injeção `container_instance`: a passagem seguinte segue para o corpo original.
    static IN_VIEW: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Marca a entrada em `view`; falso se já estava marcada (a segunda passagem).
pub(in crate::game) fn view_enter() -> bool {
    !IN_VIEW.replace(true)
}

pub(in crate::game) fn view_leave() {
    IN_VIEW.set(false);
}

fn front_of(property: Option<&str>) -> Face {
    property.and_then(Face::from_property).unwrap_or(Face::North)
}

/// O fim de `Containers::view` (H2): as faces, as entradas e as saídas da instância no lugar das do bloco. `base` é o contêiner do bloco,
/// com a contagem de slots e o menu; sem quem responda pelo bloco, ele fica como está.
pub(in crate::game) fn container_instance(
    block: &str,
    entity: &ModBlockEntity,
    front: Option<&str>,
    base: Option<&horizon_client::containers::ModContainer>,
) -> Option<horizon_client::containers::ModContainer> {
    let front = front_of(front);
    let providers: Vec<ItemInstances> = ITEM_INSTANCES.lock().unwrap_or_else(|e| e.into_inner()).iter().map(|(_, p)| *p).collect();
    let instance = providers.iter().find_map(|p| (p.faces)(block, entity, front))?;
    let base = base.cloned().unwrap_or_default();
    Some(horizon_client::containers::ModContainer {
        faces: instance.face_slots(),
        inputs: instance.inputs(),
        outputs: instance.outputs(),
        ..base
    })
}

/// O começo de `Containers::can_take` (H2): se `stack` pode sair de `slot` pela face `face` (o índice do `Direction`): a face da instância
/// deixa sair e a regra do slot, para quem está de fora, deixa tirar. `None` se ninguém responde pelo bloco.
pub(in crate::game) fn container_take(
    block: &str,
    entity: &ModBlockEntity,
    front: Option<&str>,
    face: usize,
    slot: usize,
    stack: &ItemStack,
    facts: ItemFacts,
) -> Option<bool> {
    let front = front_of(front);
    let face = *Face::ALL.get(face)?;
    let providers: Vec<ItemInstances> = ITEM_INSTANCES.lock().unwrap_or_else(|e| e.into_inner()).iter().map(|(_, p)| *p).collect();
    providers.iter().find_map(|p| {
        let instance = (p.faces)(block, entity, front)?;
        Some(instance.releases_through(face, slot) && (p.extractable)(block, entity, slot, stack, facts))
    })
}

/// O começo de `native_container_accepts` (`IItemHandler.isItemValid` da capability da face, H2): o Horizon pergunta antes de um hopper, um dropper ou `native_insert_item` pôr um
/// item num slot que o `ModContainer` deixa encher. Quem registrou em [`register_item_instances`] responde pelo bloco dele: a face da instância tem de deixar entrar e o slot
/// tem de aceitar o item de quem está de fora; recusa cancela a entrada. Um bloco que ninguém reclama passa como o Horizon o deu.
pub(in crate::game) fn container_accepts(game: &Game<'_>, pos: [i32; 3], entity: &ModBlockEntity, slot: usize, stack: &ItemStack, face: Option<Direction>, cancel: &mut bool) {
    let face = face.map(face_of);
    let providers: Vec<ItemInstances> = ITEM_INSTANCES.lock().unwrap_or_else(|e| e.into_inner()).iter().map(|(_, p)| *p).collect();
    if let Some(accepted) = providers.iter().find_map(|p| (p.accepts)(game, pos, entity, slot, stack, face))
        && !accepted
    {
        *cancel = true;
    }
}

pub(in crate::game) fn direction(face: Face) -> Direction {
    match face {
        Face::Down => Direction::Down,
        Face::Up => Direction::Up,
        Face::North => Direction::North,
        Face::South => Direction::South,
        Face::West => Direction::West,
        Face::East => Direction::East,
    }
}

/// Os vizinhos de energia de um bloco em tick (`BlockCapabilityCache.getCapability` e `EnergyHandler.insert` de cada face), para quem entrega energia pelo `EnergyUtils.emit`:
/// o cubo de energia e o Heat Generator. O bloco em tick está fora do mundo, então só os vizinhos passam por aqui.
pub(in crate::game) struct GameEnergy<'a, 'g> {
    game: RefCell<&'a mut Game<'g>>,
    dimension: &'static str,
    pos: [i32; 3],
}

impl<'a, 'g> GameEnergy<'a, 'g> {
    pub(in crate::game) fn new(game: &'a mut Game<'g>, dimension: &'static str, pos: [i32; 3]) -> Self {
        Self { game: RefCell::new(game), dimension, pos }
    }

    fn at(&self, face: Face) -> [i32; 3] {
        neighbour(self.pos, face)
    }
}

impl EnergyNeighbours for GameEnergy<'_, '_> {
    /// `BlockCapabilityCache.getCapability` não nulo: um vizinho com configuração de lados tem capability onde a face deixa entrar ou sair; os outros blocos de energia,
    /// onde a regra deles manda.
    fn has_handler(&self, face: Face) -> bool {
        let at = self.at(face);
        let reached = face.opposite();
        if let Some((insert, extract)) = instance_energy_faces(self.dimension, at) {
            return insert[reached as usize] || extract[reached as usize];
        }
        let game = self.game.borrow();
        let state = game.world.block(at[0], at[1], at[2]);
        if game.world.block_entities.mod_energy(state.0).is_none() {
            return false;
        }
        let block = game.native_block(at);
        network::current().receives(&block, front(&game, at), reached)
    }

    fn insert(&self, face: Face, amount: i32, simulate: bool) -> i32 {
        let at = self.at(face);
        let mut game = self.game.borrow_mut();
        game.native_energy_insert(at, direction(face.opposite()), amount, simulate)
    }
}

fn neighbour(pos: [i32; 3], face: Face) -> [i32; 3] {
    let [x, y, z] = face.offset();
    [pos[0] + x, pos[1] + y, pos[2] + z]
}

fn saved(entity: &ModBlockEntity) -> Option<&Compound> {
    match entity.nbt.get(&JavaString::from(SAVED)) {
        Some(Tag::Compound(compound)) => Some(compound),
        _ => None,
    }
}

/// Os tanques salvos da block entity, um por capacidade. Sem dados, ou num formato que esta
/// versão não conhece, vêm vazios.
pub(in crate::game) fn load_tanks(entity: &ModBlockEntity, capacities: &[u64]) -> Vec<Tank> {
    let list = saved(entity).and_then(|compound| {
        let version = compound.get(&JavaString::from("v"));
        if !matches!(version, Some(Tag::Int(v)) if *v == FORMAT) {
            return None;
        }
        match compound.get(&JavaString::from("tanks")) {
            Some(Tag::List(list)) => Some(list),
            _ => None,
        }
    });
    capacities
        .iter()
        .enumerate()
        .map(|(i, &capacity)| {
            let Some(Tag::Compound(tank)) = list.and_then(|list| list.get(i)) else {
                return Tank::new(capacity);
            };
            let resource = match tank.get(&JavaString::from("resource")) {
                Some(Tag::String(id)) => Some(id.to_string_lossy()),
                _ => None,
            };
            let amount = match tank.get(&JavaString::from("amount")) {
                Some(Tag::Long(amount)) => u64::try_from(*amount).unwrap_or(0),
                _ => 0,
            };
            Tank::restored(resource, amount, capacity)
        })
        .collect()
}

pub(in crate::game) fn save_tanks(entity: &mut ModBlockEntity, tanks: &[Tank]) {
    let list = tanks
        .iter()
        .map(|tank| {
            let mut compound = Compound::default();
            if let Some(resource) = tank.resource() {
                compound.insert(
                    JavaString::from("resource"),
                    Tag::String(JavaString::from(resource)),
                );
            }
            let amount = i64::try_from(tank.amount()).unwrap_or(i64::MAX);
            compound.insert(JavaString::from("amount"), Tag::Long(amount));
            Tag::Compound(compound)
        })
        .collect();
    let mut compound = saved(entity).cloned().unwrap_or_default();
    compound.insert(JavaString::from("v"), Tag::Int(FORMAT));
    compound.insert(JavaString::from("tanks"), Tag::List(list));
    entity.nbt.insert(JavaString::from(SAVED), Tag::Compound(compound));
}

/// Tira os tanques salvos da block entity, para um mod que guarda os seus em outro formato. Os outros dados da lib no mesmo
/// compound ficam.
pub(in crate::game) fn discard_tanks(entity: &mut ModBlockEntity) {
    let has_tanks = saved(entity).is_some_and(|compound| {
        compound.contains_key(&JavaString::from("v")) || compound.contains_key(&JavaString::from("tanks"))
    });
    if !has_tanks {
        return;
    }
    let mut compound = saved(entity).cloned().unwrap_or_default();
    compound.remove(&JavaString::from("v"));
    compound.remove(&JavaString::from("tanks"));
    entity.nbt.insert(JavaString::from(SAVED), Tag::Compound(compound));
}

/// O clima do bioma que `Level.getBiome(pos)` dá em `pos`: o `BiomeManager` não lê o quadrado de 4 por 4 que contém o bloco, e sim o
/// mais próximo de (x - 2, y - 2, z - 2) com um sorteio por quadrado (`BiomeZoom::quart_at_block`, do Horizon), e o quadrado pode
/// estar no chunk vizinho. `None` se o chunk dele não está carregado.
fn biome_climate(game: &Game<'_>, pos: [i32; 3]) -> Option<biome_temperature::Climate> {
    let [qx, qy, qz] = game.streaming.biome_zoom.quart_at_block(BlockPos::new(pos[0], pos[1], pos[2]));
    let [x, y, z] = [qx << 2, qy << 2, qz << 2];
    let chunk = game.world.chunks.get(&(x >> 4, z >> 4))?;
    let biome = &world_biomes!(game).list[chunk.biome(x, y, z) as usize];
    Some(biome_temperature::Climate { temperature: biome.temperature, frozen: biome.frozen })
}

/// `Level.getSeaLevel()` do nível em que o jogo está: o do gerador da dimensão (63, 32 e 0), ou -63 no overworld de um mundo plano
/// (`FlatLevelSource.getSeaLevel`). O Horizon sabe só qual predefinição o mundo tem (`world_flat`).
pub(in crate::game) fn sea_level(game: &Game<'_>) -> i32 {
    biome_temperature::sea_level(game.native_dimension(), game.world_flat)
}

/// A temperatura do ar numa posição (`HeatAPI.getAmbientTemp(level, pos)`): a temperatura do bioma ali com o ajuste por altura de
/// `Biome.getTemperature(pos, seaLevel)`, levada para kelvin. `None` se o chunk do bioma não está carregado (o Java o carregaria): quem
/// guarda o valor (`heat::AmbientCache`) não guarda esse caso.
pub(in crate::game) fn ambient_temperature(game: &Game<'_>, pos: [i32; 3]) -> Option<f64> {
    biome_climate(game, pos).map(|climate| climate.ambient_at(pos, sea_level(game)))
}

/// O que a sonda do ar (`MBA`) mostra além do ar: a temperatura base do bioma que o jogo leu em `pos`, o nível do mar e a temperatura com o ajuste de altura (`Biome.getTemperature(pos, seaLevel)`).
/// Serve para ver o que o jogo leu (o bioma, o nível do mar e a altura), não entra em nenhuma conta.
pub(in crate::game) fn ambient_detail(game: &Game<'_>, pos: [i32; 3]) -> Option<(f32, i32, f32)> {
    let climate = biome_climate(game, pos)?;
    let sea = sea_level(game);
    Some((climate.temperature, sea, climate.temperature_at(pos, sea)))
}

/// As configurações locais do cliente (`MekanismConfig.local`), uma por namespace: a unidade de temperatura que as abas de calor
/// mostram. É estado do cliente e vive num `static`, porque o desenho e o clique só têm `&Game`.
static LOCAL_UNITS: Mutex<Vec<(&'static str, temperature::LocalUnit)>> = Mutex::new(Vec::new());

/// Roda `f` com a unidade local do namespace `ns` (`config/<ns>/local.toml`), criando-a na primeira vez. `None` se o jogo não dá a
/// pasta de configuração.
fn with_local_unit<T>(ns: &'static str, f: impl FnOnce(&mut temperature::LocalUnit) -> T) -> Option<T> {
    let mut units = LOCAL_UNITS.lock().unwrap_or_else(|e| e.into_inner());
    if !units.iter().any(|(known, _)| *known == ns) {
        let path = Game::native_config_dir().ok()?.join(ns).join(temperature::LOCAL_CONFIG_FILE);
        units.push((ns, temperature::LocalUnit::new(path)));
    }
    units.iter_mut().find(|(known, _)| *known == ns).map(|(_, local)| f(local))
}

/// `MekanismConfig.local.tempUnit.get()`: a unidade de temperatura do cliente, relida do arquivo quando ele muda. O desenho e o
/// tooltip da aba de calor a leem a cada quadro.
pub(in crate::game) fn temperature_unit(ns: &'static str) -> temperature::TemperatureUnit {
    with_local_unit(ns, |local| local.get(Instant::now())).unwrap_or_default()
}

/// `MekanismConfig.local.holidays.get()`: se os feriados valem (o som de Natal), relido do arquivo quando ele muda.
pub(in crate::game) fn holidays_enabled(ns: &'static str) -> bool {
    with_local_unit(ns, |local| local.holidays(Instant::now())).unwrap_or(true)
}

/// `MekanismConfig.local.compactEnergyDisplay.get()`: se a energia aparece abreviada (`1.5 kFE`), relido do arquivo quando ele muda.
pub(in crate::game) fn compact_energy_display(ns: &'static str) -> bool {
    with_local_unit(ns, |local| local.compact_energy(Instant::now())).unwrap_or(true)
}

/// `tempUnit.set(unit)` seguido de `save()`: vale no próximo quadro e fica no arquivo.
pub(in crate::game) fn set_temperature_unit(ns: &'static str, unit: temperature::TemperatureUnit) {
    with_local_unit(ns, |local| local.set(unit, Instant::now()));
}

/// O `ARGB` de uma cor do Java na ordem dos vértices do HUD (`hud::rgba`).
fn hud_color(argb: u32) -> u32 {
    let [a, r, g, b] = argb.to_be_bytes();
    horizon_client::hud::rgba(r, g, b, a)
}

/// Avisa o renderer do Horizon de que uma tela de mod desenha este sprite, para o `animate_sprites` dele misturar os quadros
/// interpolados como o original (injeção `sprite_ties`).
fn register_animated(game: &Game<'_>, rect: &horizon_client::hud::Rect) {
    horizon_client::hud::machinery_sprite_ties::register(game.gui.atlas.revision, rect);
}

/// Parte `src` do sprite `name` (em pixels dele) desenhada em `dst` multiplicada por `argb`, como `native_sprite` mas com a cor
/// (`GuiGraphics.blitSprite(..., color)`). Falso se o sprite não existe.
pub(in crate::game) fn sprite_tinted(
    game: &Game<'_>,
    batch: &mut HudBatch,
    name: &str,
    src: [f32; 4],
    dst: [f32; 4],
    argb: u32,
) -> bool {
    let Some(mut r) = game.gui.atlas.sprites.get(name).copied() else {
        return false;
    };
    register_animated(game, &r);
    let (du, dv) = ((r.u1 - r.u0) / r.w as f32, (r.v1 - r.v0) / r.h as f32);
    r.u0 += src[0] * du;
    r.v0 += src[1] * dv;
    r.u1 = r.u0 + src[2] * du;
    r.v1 = r.v0 + src[3] * dv;
    batch.rect_uv(dst[0], dst[1], dst[2], dst[3], r, hud_color(argb));
    true
}

/// Um sprite de nove partes sobre `rect` (`GuiGraphicsExtractor.blitNineSlicedSprite`): os cantos no tamanho do sprite, as bordas
/// e o miolo repetidos, não esticados, a menos que o `.mcmeta` peça. O Horizon só lê o `gui.scaling` dos sprites do vanilla.
pub(in crate::game) fn nine_slice(
    game: &Game<'_>,
    batch: &mut HudBatch,
    sprite: &str,
    slice: NineSlice,
    rect: [f32; 4],
) {
    for cell in gui::nine_slice_cells(slice, rect) {
        game.native_sprite(batch, sprite, cell.src, cell.dst);
    }
}

/// Os sprites de aviso de um elemento (`GuiElement.WARNING_BACKGROUND_TEXTURE` e `WARNING_TEXTURE`). Os dois têm `gui.scaling`
/// do tipo `tile`, que o Horizon não aplica: `tile` é o tamanho do mosaico, o `width` e o `height` do `.mcmeta`.
pub(in crate::game) struct WarningSprites {
    pub background: &'static str,
    pub overlay: &'static str,
    pub tile: [f32; 2],
}

/// Um sprite em mosaico: repete a partir do canto de cima e da esquerda de `dst`, corta o que passa da borda (`clip` é o scissor).
pub(in crate::game) fn tiled_sprite(
    game: &Game<'_>,
    batch: &mut HudBatch,
    sprite: &str,
    tile: [f32; 2],
    dst: [f32; 4],
    clip: Option<[f32; 4]>,
) {
    if let Some(rect) = game.gui.atlas.sprites.get(sprite) {
        register_animated(game, rect);
    }
    for cell in gui::tile_cells(dst, tile, clip) {
        game.native_sprite(batch, sprite, cell.src, cell.dst);
    }
}

/// O que `GuiBar` desenha em volta do conteúdo: a moldura (`bar/base`, nove partes) e o `gui.scaling` dela.
pub(in crate::game) struct BarStyle {
    pub frame: &'static str,
    pub frame_slice: NineSlice,
}

/// `GuiBar.drawBackground` de uma barra vertical: a moldura sobre todo o elemento, o fundo do aviso dentro dela, o conteúdo que
/// `contents` desenha (recebe o retângulo de baixo para cima que `level` pede, só se há nível) e, com a barra quase cheia, o aviso
/// de novo por cima, só na metade de trás. `rect` é o elemento inteiro, com a moldura.
pub(in crate::game) fn vertical_bar(
    game: &Game<'_>,
    batch: &mut HudBatch,
    style: &BarStyle,
    rect: [f32; 4],
    level: f64,
    warning: Option<&WarningSprites>,
    contents: &dyn Fn(&Game<'_>, &mut HudBatch, [f32; 4]),
) {
    nine_slice(game, batch, style.frame, style.frame_slice, rect);
    let warned = warning.map(|sprites| (sprites, gui::bar_warning(rect, level, false)));
    if let Some((sprites, bar)) = &warned {
        tiled_sprite(game, batch, sprites.background, sprites.tile, bar.background, None);
    }
    let Some(dst) = gui::bar_contents(rect, level) else {
        return;
    };
    contents(game, batch, dst);
    if let Some((sprites, bar)) = &warned
        && let Some(overlay) = bar.overlay
    {
        tiled_sprite(game, batch, sprites.overlay, sprites.tile, overlay.dst, Some(overlay.clip));
    }
}

/// `GuiVerticalPowerBar.renderBarContents`: o sprite em mosaico (`tile`, o `gui.scaling` do `.mcmeta`) sobre `dst`, a partir do canto
/// de cima e da esquerda.
pub(in crate::game) fn power_bar_contents(
    game: &Game<'_>,
    batch: &mut HudBatch,
    sprite: &str,
    tile: [f32; 2],
    dst: [f32; 4],
) {
    tiled_sprite(game, batch, sprite, tile, dst, None);
}

/// `GuiTankBar.renderBarContentsDirect` com o sprite de um químico: de cima para baixo e da esquerda para a direita
/// (`TilingDirection.DOWN_RIGHT`), cortado em `dst`, multiplicado pelo `tint` (`ARGB`). `tile` é o tamanho do quadro do sprite. O
/// sprite é o do atlas de blocos, que o HUD do Horizon anima como o jogo.
pub(in crate::game) fn tank_bar_contents(
    game: &Game<'_>,
    batch: &mut HudBatch,
    sprite: &str,
    tile: [f32; 2],
    dst: [f32; 4],
    tint: u32,
) {
    for cell in gui::tiled_fill(dst, tile, TilingDirection::DownRight) {
        sprite_tinted(game, batch, sprite, cell.src, cell.dst, tint);
    }
}

/// `GuiGauge`: o medidor com moldura de nove partes e um overlay do tamanho do miolo.
pub(in crate::game) struct GaugeStyle {
    pub frame: &'static str,
    pub frame_slice: NineSlice,
    pub overlay: &'static str,
}

/// `GuiGauge.drawBackground` e `renderContents`: a moldura, o conteúdo de `scaled` pixels (`GuiTankGauge.getScaledLevel`) em
/// mosaico a partir da base (`TilingDirection.UP_RIGHT`) multiplicado por `tint`, e o overlay por cima de tudo. `fill` é o sprite do
/// que o tanque guarda e o tamanho do quadro dele. `rect` é o elemento inteiro, com a moldura.
pub(in crate::game) fn gauge(
    game: &Game<'_>,
    batch: &mut HudBatch,
    style: &GaugeStyle,
    rect: [f32; 4],
    scaled: i32,
    fill: Option<(&str, [f32; 2], u32)>,
) {
    let [x, y, w, h] = rect;
    nine_slice(game, batch, style.frame, style.frame_slice, rect);
    if let Some((sprite, tile, tint)) = fill
        && scaled > 0
    {
        let scaled = scaled as f32;
        let dst = [x + 1.0, y + 1.0 + (h - 2.0) - scaled, w - 2.0, scaled];
        for cell in gui::tiled_fill(dst, tile, TilingDirection::UpRight) {
            sprite_tinted(game, batch, sprite, cell.src, cell.dst, tint);
        }
    }
    game.native_sprite(batch, style.overlay, [0.0, 0.0, w - 2.0, h - 2.0], [x + 1.0, y + 1.0, w - 2.0, h - 2.0]);
}

/// `GuiSlot.draw`: a moldura do slot de item em `item` (a posição do item, a moldura começa um pixel antes) e o desenho por cima dela.
/// Quem chama escolhe `frame`: a do tipo do slot, ou a do aviso quando a verificação dele diz que sim. `None` num slot sem moldura.
pub(in crate::game) fn slot(
    game: &Game<'_>,
    batch: &mut HudBatch,
    origin: [f32; 2],
    item: [f32; 2],
    frame: Option<&str>,
    overlay: Option<&str>,
) {
    let [x, y, w, h] = gui::slot_frame(item);
    let dst = [origin[0] + x, origin[1] + y, w, h];
    for sprite in frame.into_iter().chain(overlay) {
        game.native_sprite(batch, sprite, [0.0, 0.0, w, h], dst);
    }
}

/// Os três sprites de um tipo de `ProgressType`: o vazio, o cheio e o do aviso, todos do tamanho do elemento.
pub(in crate::game) struct ProgressSprites {
    pub empty: &'static str,
    pub full: &'static str,
    pub warning: &'static str,
}

/// `GuiProgress.drawBackground` de um progresso horizontal que enche da esquerda para a direita: o sprite vazio, e por cima o
/// cheio até onde o progresso chegou (pelo menos um pixel). Com aviso é o sprite do aviso, com o progresso em 1.
/// `inner_offset` é a moldura que o sprite não preenche (`ProgressType.BAR` tem um pixel de cada lado).
pub(in crate::game) fn progress_bar(
    game: &Game<'_>,
    batch: &mut HudBatch,
    sprites: &ProgressSprites,
    rect: [f32; 4],
    fraction: f32,
    inner_offset: f32,
    warning: bool,
) {
    let [x, y, w, h] = rect;
    game.native_sprite(batch, sprites.empty, [0.0, 0.0, w, h], rect);
    if let Some((offset, width)) = gui::progress_fill(w, inner_offset, fraction, warning) {
        let sprite = if warning { sprites.warning } else { sprites.full };
        game.native_sprite(batch, sprite, [offset, 0.0, width, h], [x + offset, y, width, h]);
    }
}

/// `GuiWarningTab`, à esquerda da tela: `sprite` na posição que o Java dá, a partir do canto `origin` da tela.
pub(in crate::game) fn warning_tab(game: &Game<'_>, batch: &mut HudBatch, sprite: &str, origin: [f32; 2]) {
    let [x, y, w, h] = gui::WARNING_TAB_LEFT;
    game.native_sprite(batch, sprite, [0.0, 0.0, w, h], [origin[0] + x, origin[1] + y, w, h]);
}

/// `GuiInsetElement.drawBackground`, a cada quadro: o fundo do botão, o suporte (`holder_left` ou `holder_right`) tingido por `tint` por cima
/// dele, o fundo outra vez e o ícone de 18 por 18 sobre o botão (`GuiSideHolder.drawBackground` e depois a própria). Os sprites do fundo e do
/// suporte são `<ns>:gui/sprites/button`, `button_focus`, `button_inactive` e `holder_*`; `icon` é o nome inteiro do sprite do ícone.
/// `origin` é o canto da tela.
#[allow(clippy::too_many_arguments)]
pub(in crate::game) fn inset_tab(
    game: &Game<'_>,
    batch: &mut HudBatch,
    ns: &str,
    tab: machinery_basis_core::side_tab::InsetTab,
    origin: [f32; 2],
    tint: u32,
    icon: &str,
    look: machinery_basis_core::side_tab::ButtonLook,
) {
    let shift = |rect: gui::Rect| [origin[0] + rect[0], origin[1] + rect[1], rect[2], rect[3]];
    let button = shift(tab.button());
    let holder = shift(tab.holder());
    let background = format!("{ns}:gui/sprites/{}", look.sprite());
    let holder_sprite = format!("{ns}:gui/sprites/holder_{}", if tab.left { "left" } else { "right" });
    let nine = |batch: &mut HudBatch, sprite: &str, slice: NineSlice, rect: gui::Rect, argb: u32| {
        for cell in gui::nine_slice_cells(slice, rect) {
            sprite_tinted(game, batch, sprite, cell.src, cell.dst, argb);
        }
    };
    use machinery_basis_core::upgrade_window::{BUTTON_SLICE, HOLDER_SLICE};
    nine(batch, &background, BUTTON_SLICE, button, 0xFF_FF_FF_FF);
    nine(batch, &holder_sprite, HOLDER_SLICE, holder, tint);
    nine(batch, &background, BUTTON_SLICE, button, 0xFF_FF_FF_FF);
    let [_, _, w, h] = button;
    sprite_tinted(game, batch, icon, [0.0, 0.0, w, h], button, 0xFF_FF_FF_FF);
}

// ---------------------------------------------------------------------------------------------------------------------------
// O nome de um tile (`TileEntityMekanism.customName`): o componente inteiro, do item ao tile, ao NBT, ao título da tela e ao item solto.

/// `SerializationConstants.CUSTOM_NAME`: o NBT do tile.
const CUSTOM_NAME: &str = "CustomName";

thread_local! {
    /// O nome do item que acabou de ser colocado, para o tile que ainda não nasceu: o Horizon cria o tile no primeiro tick.
    static PENDING_NAMES: std::cell::RefCell<HashMap<[i32; 3], Tag>> = std::cell::RefCell::new(HashMap::new());
    /// O nome do tile que o jogador está quebrando, guardado por [`name_destroying`] antes de o Horizon tirá-lo do mundo.
    static BROKEN_NAME: std::cell::RefCell<Option<([i32; 3], Option<Tag>)>> = const { std::cell::RefCell::new(None) };
}

/// O componente de nome que um tag guarda (`ComponentSerialization`): o texto puro (`"Camara"`), um componente com estilo (compound) ou uma
/// lista. O tag é o componente inteiro e passa sem ser lido, com cor, negrito e filhos.
pub(in crate::game) fn name_of_tag(tag: Option<&Tag>) -> Option<Tag> {
    match tag? {
        tag @ Tag::String(_) => Some(tag.clone()),
        tag @ Tag::Compound(compound) if !compound.is_empty() => Some(tag.clone()),
        tag @ Tag::List(list) if !list.is_empty() => Some(tag.clone()),
        _ => None,
    }
}

/// O `CustomName` que o tile em `pos` guarda, se o bloco ali é de `block_ids` (`is_ours`): o componente inteiro.
pub(in crate::game) fn saved_name(game: &Game<'_>, pos: [i32; 3], is_ours: &dyn Fn(&str) -> bool) -> Option<Tag> {
    use horizon_client::containers::BlockEntity;
    match game.world.block_entities.get(&(pos[0], pos[1], pos[2])) {
        Some(BlockEntity::Mod(entity)) if is_ours(&entity.id) => name_of_tag(entity.nbt.get(&JavaString::from(CUSTOM_NAME))),
        _ => None,
    }
}

/// `getDisplayName` na tela: o nome que o servidor mandou ao abrir, ou a descrição do contêiner (`container_description` é a chave de
/// tradução). O componente chega com o estilo.
pub(in crate::game) fn screen_title(game: &Game<'_>, container_description: &str) -> horizon_client::text::Text {
    use horizon_client::text::Text;
    let name = game.mb_open_screen_pos().and_then(|pos| crate::game::machinery_tile_sound::tile_name(game.native_dimension(), pos));
    let tile = {
        let mut tile = machinery_basis_core::name::TileName::<Text>::new(true);
        tile.load(name.as_ref().map(horizon_client::component::name_text));
        tile
    };
    let description = Text::literal(horizon_client::lang::get(container_description));
    tile.display_name(&description, &description).clone()
}

/// `applyImplicitComponents`: o item colocado tinha `custom_name`, e o tile o guarda (`saveAdditional`), o componente inteiro.
pub(in crate::game) fn name_placed(game: &Game<'_>, stack: &ItemStack, pos: [i32; 3], is_ours: &dyn Fn(&str) -> bool) {
    if !is_ours(&game.native_block(pos)) {
        return;
    }
    // `minecraft:custom_name` não é um componente de mod: o componente está nos dados da pilha.
    let item_name = stack.extra.as_ref().and_then(|data| name_of_tag(data.custom_name.as_ref()));
    let mut tile = machinery_basis_core::name::TileName::<Tag>::new(true);
    tile.apply_implicit_components(item_name);
    if let Some(name) = tile.saved() {
        PENDING_NAMES.with_borrow_mut(|names| names.insert(pos, name.clone()));
    } else {
        PENDING_NAMES.with_borrow_mut(|names| names.remove(&pos));
    }
}

/// O tile nasceu: o nome do item que o colocou entra no NBT dele.
pub(in crate::game) fn name_apply_pending(entity: &mut ModBlockEntity, pos: [i32; 3]) {
    if let Some(name) = PENDING_NAMES.with_borrow_mut(|names| names.remove(&pos)) {
        entity.nbt.insert(JavaString::from(CUSTOM_NAME), name);
    }
}

/// O jogador (ou uma explosão que passa pelo hook) vai quebrar o tile em `pos`: o Horizon o tira do mundo antes de rolar a tabela de loot, e
/// o nome que o item solto leva (`copy_name`) é guardado aqui, enquanto o tile ainda existe.
pub(in crate::game) fn name_destroying(game: &Game<'_>, pos: [i32; 3], is_ours: &dyn Fn(&str) -> bool) {
    let name = saved_name(game, pos, is_ours);
    BROKEN_NAME.with_borrow_mut(|slot| *slot = Some((pos, name)));
}

/// `copy_name` da tabela de loot: o item solto leva o componente de nome do tile, inteiro. Um tile que ainda está no mundo dá o nome dele;
/// o que foi quebrado, o que [`name_destroying`] guardou.
pub(in crate::game) fn name_loot(
    game: &Game<'_>,
    context: &horizon_client::mod_api::NativeLootDrops<'_>,
    drops: &mut [ItemStack],
    is_ours: &dyn Fn(&str) -> bool,
) {
    let Some((pos, _)) = context.block else {
        return;
    };
    // Cada mod com nome chama isto: o nome guardado só sai para quem tem o item entre os que caíram (os outros mods o veem depois).
    if !drops.iter().any(|stack| is_ours(&stack.item)) {
        return;
    }
    let kept = BROKEN_NAME.with_borrow_mut(|slot| match slot {
        Some((at, _)) if *at == pos => slot.take().and_then(|(_, name)| name),
        _ => None,
    });
    let Some(name) = saved_name(game, pos, is_ours).or(kept) else {
        return;
    };
    for stack in drops.iter_mut().filter(|stack| is_ours(&stack.item)) {
        stack.extra.get_or_insert_with(Default::default).custom_name = Some(name.clone());
    }
}

/// O jogador abriu o contêiner de um tile: o nome vai com ele (`getDisplayName` no pacote de abertura).
pub(in crate::game) fn name_container_opened(game: &mut Game<'_>, player: u128, pos: Option<[i32; 3]>, is_ours: &dyn Fn(&str) -> bool) {
    let Some(pos) = pos else {
        return;
    };
    if !is_ours(&game.native_block(pos)) {
        return;
    }
    let name = saved_name(game, pos, is_ours);
    let dimension = game.native_dimension();
    game.mb_name_open(player, dimension, pos, name.as_ref());
}

// ---------------------------------------------------------------------------------------------------------------------------
// A segurança de um tile (`TileComponentSecurity`, frente `seguranca`, decisões 250 a 253): o dono e o modo no NBT, vindos do item (`mekanism:owner` e `mekanism:security`) ou de
// quem pôs o bloco (`BlockMekanism.setPlacedBy`), de volta ao item solto, e a recusa a quem não tem acesso. A conta é `machinery_basis_core::security`.

use machinery_basis_core::security as sec;

/// Os componentes do item que a segurança lê e grava (`MekanismDataComponents.OWNER` e `SECURITY`), para o mod registrá-los.
pub const SECURITY_COMPONENTS: [&str; 2] = [sec::OWNER_COMPONENT, sec::SECURITY_COMPONENT];

thread_local! {
    /// O dono e o modo do item que acabou de ser posto, para o tile que o Horizon ainda não criou.
    static PENDING_SECURITY: std::cell::RefCell<HashMap<[i32; 3], sec::SecurityState>> = std::cell::RefCell::new(HashMap::new());
    /// O dono e o modo do tile que o jogador está quebrando, guardados por [`security_destroying`] antes de o Horizon tirá-lo do mundo.
    static BROKEN_SECURITY: std::cell::RefCell<Option<([i32; 3], sec::SecurityState)>> = const { std::cell::RefCell::new(None) };
}

/// O JSON da conta como o NBT: a lista de quatro inteiros do dono é um `IntArray` (`UUIDUtil.CODEC`), o resto segue o formato.
fn security_tag(key: &str, value: &serde_json::Value) -> Tag {
    use serde_json::Value;
    match value {
        Value::Array(list) if key == sec::OWNER_UUID => Tag::IntArray(list.iter().filter_map(|part| part.as_i64().and_then(|n| i32::try_from(n).ok())).collect()),
        Value::Object(map) => Tag::Compound(map.iter().map(|(name, item)| (JavaString::from(name.as_str()), security_tag(name, item))).collect()),
        Value::String(text) => Tag::String(JavaString::from(text.as_str())),
        other => value_tag(other, key),
    }
}

/// O `Tag` da conta como o JSON dela (`IntArray` vira a lista de inteiros).
fn security_json(tag: &Tag) -> serde_json::Value {
    match tag {
        Tag::IntArray(ints) => serde_json::Value::from(ints.clone()),
        Tag::Compound(map) => serde_json::Value::Object(map.iter().map(|(key, value)| (key.to_string_lossy(), security_json(value))).collect()),
        other => tag_value(other),
    }
}

/// O dono e o modo que o tile guarda (`TileComponentSecurity.deserialize`).
pub(in crate::game) fn security_read(entity: &ModBlockEntity) -> sec::SecurityState {
    let component = entity.nbt.get(&JavaString::from(sec::COMPONENT_SECURITY)).map(security_json);
    sec::deserialize(component.as_ref())
}

/// `TileComponentSecurity.serialize` e a frequência de segurança no `component_frequency`.
pub(in crate::game) fn security_write(entity: &mut ModBlockEntity, state: &sec::SecurityState) {
    let (security, frequency) = sec::serialize(state);
    for (key, value) in [(sec::COMPONENT_SECURITY, security), (sec::COMPONENT_FREQUENCY, frequency)] {
        let key = JavaString::from(key);
        match value {
            Some(value) => {
                entity.nbt.insert(key, security_tag("", &value));
            }
            None => {
                entity.nbt.remove(&key);
            }
        }
    }
}

/// O UUID do jogador que age (`placer`, `player`), no servidor.
fn acting_uuid(game: &Game<'_>) -> Option<u128> {
    game.native_acting_player().map(|player| player.uuid)
}

/// O dono e o modo que o item `stack` leva (`applyImplicitComponents`), a partir do estado de um tile novo.
fn security_of_stack(game: &Game<'_>, stack: &ItemStack) -> sec::SecurityState {
    let component = |id: &str| game.native_component(stack, id).ok().flatten().map(security_json);
    sec::apply(sec::SecurityState::default(), component(sec::OWNER_COMPONENT).as_ref(), component(sec::SECURITY_COMPONENT).as_ref())
}

/// `applyImplicitComponents` e `BlockMekanism.setPlacedBy`: o tile novo ganha o dono e o modo do item, e sem dono o de quem o pôs.
pub(in crate::game) fn security_placed(game: &mut Game<'_>, stack: &ItemStack, pos: [i32; 3], is_ours: &dyn Fn(&str) -> bool) {
    use horizon_client::containers::BlockEntity;
    if !is_ours(&game.native_block(pos)) {
        return;
    }
    let mut state = security_of_stack(game, stack);
    state.placed_by(acting_uuid(game));
    if let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&(pos[0], pos[1], pos[2])) {
        security_write(entity, &state);
        game.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
        PENDING_SECURITY.with_borrow_mut(|pending| pending.remove(&pos));
    } else {
        PENDING_SECURITY.with_borrow_mut(|pending| pending.insert(pos, state));
    }
}

/// O tile nasceu: o dono e o modo do item que o colocou (ou de quem o pôs) entram no NBT dele.
pub(in crate::game) fn security_apply_pending(entity: &mut ModBlockEntity, pos: [i32; 3]) {
    if let Some(state) = PENDING_SECURITY.with_borrow_mut(|pending| pending.remove(&pos)) {
        security_write(entity, &state);
    }
}

/// O tile em `pos` vai ser quebrado: o dono e o modo que o item solto leva (`copy_components`) são guardados enquanto o tile existe.
pub(in crate::game) fn security_destroying(game: &Game<'_>, pos: [i32; 3], is_ours: &dyn Fn(&str) -> bool) {
    use horizon_client::containers::BlockEntity;
    if let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(pos[0], pos[1], pos[2]))
        && is_ours(&entity.id)
    {
        BROKEN_SECURITY.with_borrow_mut(|slot| *slot = Some((pos, security_read(entity))));
    }
}

/// `copy_components` de `owner` e `security`: o item solto leva o dono e o modo (o modo só se não é `PUBLIC`, o padrão do item).
pub(in crate::game) fn security_loot(
    game: &Game<'_>,
    context: &horizon_client::mod_api::NativeLootDrops<'_>,
    drops: &mut [ItemStack],
    is_ours: &dyn Fn(&str) -> bool,
) {
    let Some((pos, _)) = context.block else {
        return;
    };
    if !drops.iter().any(|stack| is_ours(&stack.item)) {
        return;
    }
    let kept = BROKEN_SECURITY.with_borrow_mut(|slot| match slot {
        Some((at, _)) if *at == pos => slot.take().map(|(_, state)| state),
        _ => None,
    });
    let live = match game.world.block_entities.get(&(pos[0], pos[1], pos[2])) {
        Some(horizon_client::containers::BlockEntity::Mod(entity)) if is_ours(&entity.id) => Some(security_read(entity)),
        _ => None,
    };
    let Some(state) = live.or(kept) else {
        return;
    };
    let (owner, mode) = sec::collect(&state);
    for stack in drops.iter_mut().filter(|stack| is_ours(&stack.item)) {
        for (id, value) in [(sec::OWNER_COMPONENT, &owner), (sec::SECURITY_COMPONENT, &mode)] {
            if let Some(value) = value
                && let Err(error) = game.native_set_component(stack, id, security_tag(sec::OWNER_UUID, value))
            {
                eprintln!("machinery_basis: o item solto não leva {id}: {error}");
            }
        }
    }
}

thread_local! {
    /// `UsernameCache`: o último nome que o servidor viu de cada UUID (o jogador que age, o anfitrião e os convidados da LAN).
    static USERNAMES: std::cell::RefCell<HashMap<u128, String>> = std::cell::RefCell::new(HashMap::new());
}

/// `MekanismUtils.getLastKnownUsername`: o nome do dono que o servidor sabe, ou `<uuid>`.
fn server_username(game: &Game<'_>, uuid: u128) -> String {
    let mut seen: Vec<(u128, String)> = Vec::new();
    if let Some(player) = game.native_acting_player() {
        seen.push((player.uuid, player.name));
    }
    if let Some(host) = game.lan.as_ref() {
        seen.extend(host.hub.guests.iter().map(|guest| (guest.uuid, guest.name.clone())));
    }
    if game.remote.is_none() && let Some(own) = game.level.player_uuid {
        seen.push((horizon_client::lan::playerdata::ints_uuid(own), game.options.player_name()));
    }
    USERNAMES.with_borrow_mut(|cache| {
        cache.extend(seen);
        cache.get(&uuid).cloned().unwrap_or_else(|| format!("<{}>", uuid_text(uuid)))
    })
}

/// O que o menu de um tile leva da segurança (`TileComponentSecurity.addContainerTrackers`): o dono e o modo do NBT, e o nome do dono.
pub(in crate::game) fn security_sync(game: &Game<'_>, entity: &ModBlockEntity) -> sec::SecuritySync {
    let state = security_read(entity);
    let owner_name = state.owner.map(|owner| server_username(game, owner)).unwrap_or_default();
    sec::SecuritySync { state, owner_name }
}

/// O UUID de quem vê o tooltip: o jogador deste cliente (o perfil da sessão remota, ou o do mundo local).
fn viewer_uuid(game: &Game<'_>) -> Option<u128> {
    game.remote.as_ref().map(|remote| remote.profile()).or_else(|| game.level.player_uuid.map(horizon_client::lan::playerdata::ints_uuid))
}

/// `OwnerDisplay.getOwnerName` no cliente: o nome do jogador deste cliente se ele é o dono, o de um jogador que o cliente conhece, ou `<uuid>`.
fn owner_name(game: &Game<'_>, owner: u128, viewer: Option<u128>) -> String {
    if viewer == Some(owner) {
        return game.remote.as_ref().map_or_else(|| game.options.player_name(), |remote| remote.profile_name.clone());
    }
    let known = game.remote_player_names().into_iter().find(|(id, _)| game.remote_player_uuid(*id) == Some(owner)).map(|(_, name)| name);
    known.unwrap_or_else(|| format!("<{}>", uuid_text(owner)))
}

/// `UUID.toString`: `8-4-4-4-12` em hexadecimal.
fn uuid_text(uuid: u128) -> String {
    let hex = format!("{uuid:032x}");
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

/// `registerComponentAppenderBefore(OWNER, SECURITY, ...)` e `registerComponentAppenderBeforeAll(SECURITY, ...)`: as linhas `Owner:` e `Security:` do item, que vêm logo depois do nome
/// (o rastro `item_content_*.jsonl`). A cor do rótulo é a de `EnumColor` e a do argumento depende do modo e de quem vê.
pub(in crate::game) fn security_tooltip_lines(game: &Game<'_>, stack: &ItemStack) -> Vec<(horizon_client::mod_api::Text, u32)> {
    let state = security_of_stack(game, stack);
    let viewer = viewer_uuid(game);
    let owner = state.owner.map(|owner| (owner, owner_name(game, owner, viewer)));
    sec::tooltip_lines(sec::SecurityConfig::default(), viewer, owner.as_ref().map(|(owner, name)| (*owner, name.as_str())), state.mode)
        .iter()
        .map(|line| (security_line_text(line), u32::MAX))
        .collect()
}

/// `SecurityUtils.canAccess` do jogador que age sobre o tile em `pos`: o servidor não tem a lista de confiança (a Security Desk não existe no port) nem operadores com atalho
/// (`opsBypassRestrictions` é falso por padrão). Um tile sem dono ou sem segurança sempre dá acesso.
pub(in crate::game) fn security_can_access(game: &Game<'_>, pos: [i32; 3], is_ours: &dyn Fn(&str) -> bool, player: u128) -> bool {
    use horizon_client::containers::BlockEntity;
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(pos[0], pos[1], pos[2])) else {
        return true;
    };
    if !is_ours(&entity.id) {
        return true;
    }
    security_read(entity).can_access_player(sec::SecurityConfig::default(), player, false, false, &|_, _| false)
}

/// `SecurityUtils.displayNoAccess`: a mensagem vermelha `[Mekanism] You don't have access.` ao jogador.
pub(in crate::game) fn security_deny(game: &mut Game<'_>, player: u128) {
    use horizon_client::mod_api::Text;
    let line = Text::translate("constants.mekanism.log_format", &[Text::translate("constants.mekanism.mod_name", &[]), Text::translate(sec::NO_ACCESS_KEY, &[])])
        .with_style(&horizon_client::text::Style::color(sec::COLOR_RED));
    game.native_tell(player, line);
}

/// `IBlockSecurityUtils.canAccessOrDisplayError(player, level, pos, tile)` do jogador que age: sem acesso, a mensagem vermelha e `false`. Quem não tem dono, ou não age, passa.
pub(in crate::game) fn security_access_or_error(game: &mut Game<'_>, pos: [i32; 3], is_ours: &dyn Fn(&str) -> bool) -> bool {
    let Some(player) = acting_uuid(game) else {
        return true;
    };
    if security_can_access(game, pos, is_ours, player) {
        return true;
    }
    security_deny(game, player);
    false
}

/// `TileEntityMekanism.openGui`: o menu de um tile com dono recusa quem não tem acesso (`canAccessOrDisplayError`: a mensagem e nada abre). Devolve se a abertura deve ser
/// cancelada.
pub(in crate::game) fn security_open(game: &mut Game<'_>, player: u128, pos: [i32; 3], is_ours: &dyn Fn(&str) -> bool) -> bool {
    if security_can_access(game, pos, is_ours, player) {
        return false;
    }
    security_deny(game, player);
    true
}

/// `BlockMekanism.getDestroyProgress`: quem não acessa o tile não o quebra, e nenhuma mensagem sai (o progresso é zero). Um jogador em criativo quebra sem ler o progresso
/// (`ServerPlayerGameMode.handleBlockBreakAction` destrói direto), então o original deixa o criativo quebrar. Devolve se a quebra deve ser cancelada.
pub(in crate::game) fn security_break(game: &Game<'_>, pos: [i32; 3], is_ours: &dyn Fn(&str) -> bool) -> bool {
    !is_creative(game) && acting_uuid(game).is_some_and(|player| !security_can_access(game, pos, is_ours, player))
}

/// O botão de menu que a aba de segurança manda ao servidor (`PacketGuiInteract(NEXT_SECURITY_MODE)`), e o do clique direito (`PREVIOUS_SECURITY_MODE`). Os ids ficam longe
/// dos botões de cada mod (o despejo e a experiência do Mekanism).
pub const SECURITY_NEXT_BUTTON: i32 = 100;
pub const SECURITY_PREVIOUS_BUTTON: i32 = 101;

/// `GuiSecurityTab.onClick`: o clique esquerdo ou o direito no botão da aba (fora de uma janela por cima) vira o botão de menu; só o dono muda o modo, e o servidor
/// confere isso de novo (`incrementSecurityMode`). `at` é o cursor relativo ao canto da tela.
pub(in crate::game) fn security_screen_click(game: &Game<'_>, image_width: i32, at: [f32; 2], right: bool) -> Option<i32> {
    let tab = machinery_basis_core::side_tab::security_tab(image_width);
    (!game.mb_window_covers(at) && tab.is_mouse_over(true, true, at)).then_some(if right { SECURITY_PREVIOUS_BUTTON } else { SECURITY_NEXT_BUTTON })
}

/// O ícone da aba (`getOverlay`): o sprite do modo no atlas da tela.
fn security_icon(mode: sec::SecurityMode) -> String {
    format!("mekanism:gui/sprites/{}", sec::tab_overlay(mode).trim_start_matches("mekanism:"))
}

/// `GuiSecurityTab` desenhada: o suporte tingido, o botão e o sprite do modo (`getSecurityMode(..., true)`), com o fundo em foco sob o mouse.
pub(in crate::game) fn security_draw(game: &Game<'_>, batch: &mut HudBatch, origin: [f32; 2], view: &sec::SecurityView, image_width: i32) {
    use machinery_basis_core::side_tab::{ButtonLook, SECURITY_TAB_TINT, security_tab};
    let tab = security_tab(image_width);
    let cursor = game.native_cursor();
    let at = [cursor[0] - origin[0], cursor[1] - origin[1]];
    let hovered = !game.mb_window_covers(at) && tab.is_mouse_over(true, true, at);
    let mode = if sec::SecurityConfig::default().allow_protection { view.mode } else { sec::SecurityMode::Public };
    inset_tab(game, batch, "mekanism", tab, origin, SECURITY_TAB_TINT, &security_icon(mode), ButtonLook::of(true, hovered));
}

/// O texto de uma linha de tooltip de segurança: o rótulo na cor dele e o argumento na do argumento.
fn security_line_text(line: &sec::TooltipLine) -> horizon_client::mod_api::Text {
    use horizon_client::mod_api::Text;
    use horizon_client::text::Style;
    let arg = match &line.arg {
        sec::Arg::Literal(text) => Text::literal(text.as_str()),
        sec::Arg::Translate(key) => Text::translate(key, &[]),
    }
    .with_style(&Style::color(line.arg_color));
    Text::translate(line.key, &[arg]).with_style(&Style::color(line.color))
}

/// `GuiSecurityTab.updateTooltip`: `Security: <modo>` e `Owner: <nome>`, sobre o botão. Devolve se desenhou.
pub(in crate::game) fn security_widget_tooltip(
    game: &Game<'_>,
    batch: &mut HudBatch,
    view: &sec::SecurityView,
    image_width: i32,
    at: [f32; 2],
    cursor: (f32, f32),
) -> bool {
    let tab = machinery_basis_core::side_tab::security_tab(image_width);
    if game.mb_window_covers(at) || !tab.is_mouse_over(true, true, at) {
        return false;
    }
    let owner = view.owner.as_ref().map(|(owner, name)| (*owner, name.as_str()));
    let lines: Vec<_> = sec::tab_tooltip_lines(viewer_uuid(game), owner, view.mode).iter().map(security_line_text).collect();
    let [bx, by, bw, bh] = tab.button();
    menu_tooltip_rich(game, batch, &lines, cursor, [cursor.0 - at[0] + bx, cursor.1 - at[1] + by, bw, bh]);
    true
}

/// `PacketGuiInteract.NEXT_SECURITY_MODE` e `PREVIOUS_SECURITY_MODE`, no servidor: `SecurityUtils.incrementSecurityMode(player, security, ...)`. Só o dono do tile
/// troca o modo, e o jogador é o que age. Devolve se o botão era da segurança.
pub(in crate::game) fn security_menu_button(game: &Game<'_>, entity: &mut ModBlockEntity, id: i32) -> bool {
    let forward = match id {
        SECURITY_NEXT_BUTTON => true,
        SECURITY_PREVIOUS_BUTTON => false,
        _ => return false,
    };
    if let Some(player) = acting_uuid(game) {
        let mut state = security_read(entity);
        let before = state.mode;
        state.click(player, forward);
        if state.mode != before {
            security_write(entity, &state);
        }
    }
    true
}

/// O tooltip de um widget (`WidgetTooltipHolder`): `lines`, cada uma com a cor própria (`None` é a do padrão), ao lado do cursor
/// mas por cima ou por baixo do `widget` (`MenuTooltipPositioner`), em vez de sobre o cursor como o tooltip de um item. O hook
/// `native_screen_tooltip` só entrega texto com os códigos `§` do vanilla, que não têm as cores de `EnumColor`, e posiciona sempre
/// no cursor. `mouse` e `widget` (`[x, y, largura, altura]`) estão em pixels de GUI da janela.
pub(in crate::game) fn menu_tooltip(
    game: &Game<'_>,
    batch: &mut HudBatch,
    lines: &[(String, Option<u32>)],
    mouse: (f32, f32),
    widget: [f32; 4],
) {
    menu_tooltip_with(game, batch, lines, mouse, widget, true);
}

/// Como [`menu_tooltip`], para um texto que o jogo quebrou em linhas (`Tooltip.create`): sem o intervalo de dois pixels depois da
/// primeira linha, que só o tooltip com título tem.
pub(in crate::game) fn menu_tooltip_flat(
    game: &Game<'_>,
    batch: &mut HudBatch,
    lines: &[(String, Option<u32>)],
    mouse: (f32, f32),
    widget: [f32; 4],
) {
    menu_tooltip_with(game, batch, lines, mouse, widget, false);
}

fn menu_tooltip_with(
    game: &Game<'_>,
    batch: &mut HudBatch,
    lines: &[(String, Option<u32>)],
    mouse: (f32, f32),
    widget: [f32; 4],
    title_gap: bool,
) {
    let lines: Vec<(horizon_client::text::Text, u32)> = lines
        .iter()
        .map(|(text, color)| {
            let text = match color {
                Some(color) => horizon_client::text::Text::colored(text.as_str(), *color),
                None => horizon_client::text::Text::literal(text.as_str()),
            };
            (text, u32::MAX)
        })
        .collect();
    menu_tooltip_texts(game, batch, &lines, mouse, widget, title_gap);
}

/// Como [`menu_tooltip_flat`], para linhas que já são `Text` (com mais de uma cor na mesma linha, como as da aba de segurança).
pub(in crate::game) fn menu_tooltip_rich(
    game: &Game<'_>,
    batch: &mut HudBatch,
    lines: &[horizon_client::text::Text],
    mouse: (f32, f32),
    widget: [f32; 4],
) {
    let lines: Vec<(horizon_client::text::Text, u32)> = lines.iter().cloned().map(|text| (text, u32::MAX)).collect();
    menu_tooltip_texts(game, batch, &lines, mouse, widget, false);
}

fn menu_tooltip_texts(
    game: &Game<'_>,
    batch: &mut HudBatch,
    lines: &[(horizon_client::text::Text, u32)],
    mouse: (f32, f32),
    widget: [f32; 4],
    title_gap: bool,
) {
    let atlas = &game.gui.atlas;
    // O tamanho não depende de onde o tooltip cai; a posição é a do `MenuTooltipPositioner`.
    let (_, _, mut w, mut h) = Game::tooltip_geometry(atlas, lines, (0.0, 0.0), (batch.width, batch.height), [0.0; 2]);
    if !title_gap {
        // Cada linha medida sozinha: a largura é a maior, e a altura 10 por linha menos 2.
        w = 0.0;
        for line in lines {
            let (_, _, line_w, _) = Game::tooltip_geometry(atlas, std::slice::from_ref(line), (0.0, 0.0), (batch.width, batch.height), [0.0; 2]);
            w = w.max(line_w);
        }
        h = 10.0 * lines.len() as f32 - 2.0;
    }
    let [x, y] = tooltip::menu_position(
        [batch.width as i32, batch.height as i32],
        [mouse.0 as i32, mouse.1 as i32],
        [w as i32, h as i32],
        widget.map(|edge| edge.round() as i32),
    );
    // O 26.x desenha o tooltip com os sprites `tooltip/background` e `tooltip/frame` em nove partes (`TooltipRenderUtil`), de 12 pixels
    // além do texto. `tooltip_box` do Horizon é o desenho de retângulos de versões antigas, de cantos quadrados.
    for sprite in ["gui/sprites/tooltip/background", "gui/sprites/tooltip/frame"] {
        horizon_client::ui::nine_slice(batch, atlas, sprite, x as f32 - 12.0, y as f32 - 12.0, w + 24.0, h + 24.0, u32::MAX);
    }
    if title_gap {
        Game::tooltip_text(batch, atlas, lines, x as f32, y as f32);
    } else {
        for (n, line) in lines.iter().enumerate() {
            Game::tooltip_text(batch, atlas, std::slice::from_ref(line), x as f32, y as f32 + 10.0 * n as f32);
        }
    }
}

/// O fundo de uma tela de máquina: o painel (`GuiMekanism.extractBackground`), os slots do inventário do jogador
/// (`GuiMekanism.addSlots`, um `GuiSlot` normal para cada slot que não é do tile) e os dois rótulos (`renderTitleText` e
/// `renderInventoryText`), o título centrado e "Inventory" à esquerda, os dois em `SpecialColors.TEXT_TITLE` e sem sombra (o título, se
/// tem estilo, com o dele).
pub(in crate::game) fn machine_panel(
    game: &Game<'_>,
    batch: &mut HudBatch,
    style: &PanelStyle,
    rect: [f32; 4],
    title: &horizon_client::text::Text,
) {
    let [x, y, width, height] = rect;
    nine_slice(game, batch, style.base, style.base_slice, rect);
    for row in 0..=INVENTORY_ROWS {
        let gap = if row == INVENTORY_ROWS { HOTBAR_GAP } else { 0.0 };
        let slot_y = y + INVENTORY_TOP + row as f32 * gui::SLOT_SIZE + gap;
        for column in 0..INVENTORY_COLUMNS {
            let slot_x = x + INVENTORY_LEFT + column as f32 * gui::SLOT_SIZE;
            let dst = [slot_x, slot_y, gui::SLOT_SIZE, gui::SLOT_SIZE];
            game.native_sprite(batch, style.slot, [0.0, 0.0, gui::SLOT_SIZE, gui::SLOT_SIZE], dst);
        }
    }
    // O título é o componente inteiro (`drawScrollingString` com `text.getVisualOrderText()`): a cor e o negrito do próprio texto valem
    // sobre `TEXT_TITLE`, e a largura que o centra é a do texto com o estilo.
    let atlas = &game.gui.atlas;
    let title_x = x + gui::title_x(width, atlas.font.width_text(title));
    batch.text_styled(atlas, title, title_x, y + gui::TITLE_LABEL_Y, hud_color(0xFF_00_00_00 | gui::TITLE_TEXT_COLOR), false);
    let inventory = horizon_client::lang::get("container.inventory");
    let label = [x + gui::INVENTORY_LABEL_X, y + gui::inventory_label_y(height)];
    game.native_text(batch, &inventory, label, gui::TITLE_TEXT_COLOR, false);
}

/// Liga a instrumentação de medida da comparação das opções B e C: `MEKTRACE` a cada tick de uma máquina e `MEKBENCH` a
/// cada janela. É o único lugar que a liga e desliga.
pub(in crate::game) const MEDIR: bool = false;
/// A janela do `MEKBENCH`, em ticks do jogo.
const BENCH_WINDOW_TICKS: i64 = 1200;
static BENCH_MACHINE_TICKS: AtomicU64 = AtomicU64::new(0);
static BENCH_NANOS: AtomicU64 = AtomicU64::new(0);
static BENCH_WINDOW: AtomicI64 = AtomicI64::new(i64::MIN);

/// Soma o tempo da lógica de um tick de uma máquina, e a cada [`BENCH_WINDOW_TICKS`] imprime
/// `MEKBENCH ticks=<ticks de máquina na janela> ns=<soma>` e zera.
pub(in crate::game) fn bench_record(game_time: i64, nanos: u64) {
    let window = game_time.div_euclid(BENCH_WINDOW_TICKS);
    let last = BENCH_WINDOW.swap(window, Ordering::Relaxed);
    if last != i64::MIN && last != window {
        let ticks = BENCH_MACHINE_TICKS.swap(0, Ordering::Relaxed);
        let nanos_sum = BENCH_NANOS.swap(0, Ordering::Relaxed);
        eprintln!("MEKBENCH ticks={ticks} ns={nanos_sum}");
    }
    BENCH_MACHINE_TICKS.fetch_add(1, Ordering::Relaxed);
    BENCH_NANOS.fetch_add(nanos, Ordering::Relaxed);
}

/// Uma linha de `MEKTRACE`: o JSON de um tick, com os campos dos rastros.
pub(in crate::game) fn trace(json: &str) {
    eprintln!("MEKTRACE {json}");
}

// ---------------------------------------------------------------------------------------------------------------------------
// O sinal de redstone dos vizinhos (`TileEntityMekanism.updatePower`). Só muda quando o jogo avisa o bloco; veja
// `machinery_basis_core::neighbors`.
// ---------------------------------------------------------------------------------------------------------------------------

type PowerKey = (&'static str, [i32; 3]);

/// Quem o jogo avisou desde a última leitura, na ordem em que avisou. Quem avisa e quem lê podem ser threads diferentes (o servidor de
/// um jogador só), por isso não é `thread_local!`.
static NOTIFIED: Mutex<Vec<PowerKey>> = Mutex::new(Vec::new());
/// O que `flush_neighbor_signals` leu e o dono do tile ainda não pegou (`take_power`).
static PENDING_POWER: LazyLock<Mutex<PendingPower<PowerKey>>> = LazyLock::new(Default::default);
/// Os blocos que têm `redstone` como o `TileEntityMekanism`, por dono (um mod): cada um diz os seus por nome.
static REDSTONE_OWNERS: Mutex<Vec<(&'static str, fn(&str) -> bool)>> = Mutex::new(Vec::new());

/// Um mod diz quais dos seus blocos são `TileEntityMekanism` com `supportsRedstone()`. Pode ser dito mais de uma vez.
pub(in crate::game) fn watch_redstone(owner: &'static str, is_ours: fn(&str) -> bool) {
    let mut owners = REDSTONE_OWNERS.lock().unwrap_or_else(|e| e.into_inner());
    if !owners.iter().any(|(known, _)| *known == owner) {
        owners.push((owner, is_ours));
    }
}

fn takes_redstone(block: &str) -> bool {
    REDSTONE_OWNERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .any(|(_, is_ours)| is_ours(block))
}

/// `BlockTile.neighborChanged`: o jogo avisou o bloco em `pos`. Só anota quem; o sinal é lido no fim da operação em andamento
/// (`flush_neighbor_signals`), porque o gancho não tem como ler o mundo.
pub(in crate::game) fn neighbor_changed(
    level: &GameLevel<'_>,
    dimension: &'static str,
    pos: BlockPos,
    state: BlockStateId,
) {
    crate::game::machinery_liquid::note(level, dimension, pos, state);
    transmitters_neighbor(dimension, pos);
    if level.native_state_block(state).is_some_and(takes_redstone) {
        NOTIFIED
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((dimension, [pos.x, pos.y, pos.z]));
    }
}

/// Lê o sinal dos vizinhos de cada bloco avisado (`level.hasNeighborSignal(pos)`) e guarda para o dono. Roda no fim de cada operação no
/// mundo (`Game::flush_spill`, o último passo de `with_level`) e antes de o dono ler o `redstone`, então o valor é o do mundo no fim da
/// operação que avisou, e não o de um tick depois. Um aviso de outra dimensão espera a vez dela.
pub(in crate::game) fn flush_neighbor_signals(game: &mut Game<'_>) {
    crate::game::machinery_liquid::flush(game);
    transmitters_flush_operation(game);
    let notified = std::mem::take(&mut *NOTIFIED.lock().unwrap_or_else(|e| e.into_inner()));
    if notified.is_empty() {
        return;
    }
    let here = game.native_dimension();
    let mut later = Vec::new();
    for key in notified {
        if key.0 != here {
            later.push(key);
            continue;
        }
        record_power(game, key);
    }
    if !later.is_empty() {
        NOTIFIED.lock().unwrap_or_else(|e| e.into_inner()).extend(later);
    }
}

/// `updatePower`: guarda o sinal de agora se há um tile do mod em `key`. Sem tile o Java não faz nada (`tile != null`).
fn record_power(game: &mut Game<'_>, key: PowerKey) {
    let (dimension, pos) = key;
    let ours = matches!(
        game.world.block_entities.get(&(pos[0], pos[1], pos[2])),
        Some(horizon_client::containers::BlockEntity::Mod(entity)) if takes_redstone(&entity.id)
    );
    if !ours {
        return;
    }
    let power = game.native_neighbor_signal(dimension, pos) > 0;
    PENDING_POWER
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .record(key, power);
}

/// O que `updatePower` deixou para o tile de `pos`: o dono chama antes de ler o `redstone`, uma vez por tick.
pub(in crate::game) fn take_power(game: &mut Game<'_>, pos: [i32; 3]) -> Option<bool> {
    flush_neighbor_signals(game);
    let key = (game.native_dimension(), pos);
    PENDING_POWER.lock().unwrap_or_else(|e| e.into_inner()).take(&key)
}

/// O que `updatePower` deixou para o tile de `pos`, sem tirá-lo da fila: o menu de um tile que ainda não andou o mostra, e o primeiro tick dele o pega.
pub(in crate::game) fn peek_power(game: &mut Game<'_>, pos: [i32; 3]) -> Option<bool> {
    flush_neighbor_signals(game);
    let key = (game.native_dimension(), pos);
    let mut pending = PENDING_POWER.lock().unwrap_or_else(|e| e.into_inner());
    let power = pending.take(&key);
    if let Some(power) = power {
        pending.record(key, power);
    }
    power
}

/// `TileEntityMekanism.onAdded`, de `BlockMekanism.setPlacedBy`: o bloco posto com um item lê o sinal na hora.
pub(in crate::game) fn on_added(game: &mut Game<'_>, pos: [i32; 3]) {
    let key = (game.native_dimension(), pos);
    let power = game.native_neighbor_signal(key.0, pos) > 0;
    PENDING_POWER
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .record(key, power);
}

/// `Level.updateNeighbourForOutputSignal` do NeoForge, para o que o mod mesmo faz (`notifyComparatorChange`): cada vizinho com tile do mod
/// recebe `onNeighborChange`, na ordem de `OUTPUT_SIGNAL_ORDER`, e lê o sinal na hora (`updatePower`). O Horizon só avisa os
/// comparadores do vanilla.
pub(in crate::game) fn on_neighbor_change(game: &mut Game<'_>, dimension: &'static str, pos: [i32; 3]) {
    for face in OUTPUT_SIGNAL_ORDER {
        let [x, y, z] = face.offset();
        let neighbor = [pos[0] + x, pos[1] + y, pos[2] + z];
        record_power(game, (dimension, neighbor));
    }
}

/// `BlockEntity.setChanged` de um block entity do vanilla com contêiner (`Game::container_changed`): o NeoForge avisa os vizinhos pelo
/// `updateNeighbourForOutputSignal` (`BlockEntity.java:216-221`), e as máquinas do Mekanism releem o sinal (`onNeighborChange`). Um tile de mod fica de fora: o
/// `TileEntityMekanism.setChanged` não avisa ninguém (`TileEntityUpdateable.java:89-100`), só o nível do comparador avisa (`notify_comparators`). As duas metades de um
/// baú duplo mudam juntas (`CompoundContainer.setChanged`), cada uma com os vizinhos dela.
pub(in crate::game) fn container_changed(game: &mut Game<'_>, pos: (i32, i32, i32)) {
    let dimension = game.native_dimension();
    let halves: Vec<(i32, i32, i32)> = std::iter::once(pos).chain(game.chest_other_half(pos)).collect();
    for half in halves {
        let vanilla = game
            .world
            .block_entities
            .get(&half)
            .is_some_and(|entity| !matches!(entity, horizon_client::containers::BlockEntity::Mod(_)));
        if vanilla {
            on_neighbor_change(game, dimension, [half.0, half.1, half.2]);
        }
    }
}

/// `MekanismConfig.client.machineEffects` (`ClientConfig.java:92`, `rendering.particle.machineEffects`), como o último arquivo de cliente o deixou. Fica
/// aqui porque quem lê é cada mod com blocos que soltam partículas (as máquinas e o Heat Generator usam a mesma chave do Mekanism).
static MACHINE_EFFECTS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// `ModConfigEvent.Loading` e `Reloading` do arquivo de cliente.
pub(in crate::game) fn set_machine_effects(enabled: bool) {
    MACHINE_EFFECTS.store(enabled, Ordering::Relaxed);
}

/// Escreve no stderr cada chamada que soltou partículas, com o tick do jogo. Só as cópias de teste ligam.
const FX_TRACE: bool = false;

/// `ParticleOptions` de uma partícula de `AttributeParticleFX`.
fn fx_options(particle: machinery_basis_core::active_fx::Particle) -> horizon_client::particles::ParticleOptions {
    use horizon_client::particles::{ParticleOptions, ParticleType};
    use machinery_basis_core::active_fx::Particle;
    match particle {
        Particle::Smoke => ParticleOptions::simple(ParticleType::Smoke),
        Particle::Flame => ParticleOptions::simple(ParticleType::Flame),
        // `DustParticleOptions(color, scale)`: `ARGB.vector3fFromRGB24`.
        Particle::Dust { rgb, scale } => ParticleOptions::dust(
            [((rgb >> 16) & 255) as f32 / 255.0, ((rgb >> 8) & 255) as f32 / 255.0, (rgb & 255) as f32 / 255.0],
            scale,
        ),
    }
}

/// O motor de partículas como o `Level` de `BlockTile.animateTick`: o gerador do sorteio das posições e o `addParticle`.
struct FxLevel<'a>(&'a mut horizon_client::particles::ParticleEngine);

impl machinery_basis_core::active_fx::ParticleLevel for FxLevel<'_> {
    fn next_double(&mut self) -> f64 {
        // `BitRandomSource.nextDouble`: o `next_double` do Horizon faz a conta em `double` (2^-53), e o jogo em `float` (`long * 1.110223E-16F`). Com um expoente de potência de dois,
        // `nextInt(1 << bits)` é `next(bits)` (`BitRandomSource.nextInt(int)`, `(bound * next(31)) >> 31`), um passo do gerador cada.
        use horizon_core::random::RandomSource;
        let random = self.0.random();
        let high = random.next_int_bound(1 << 26).unwrap_or(0);
        let low = random.next_int_bound(1 << 27).unwrap_or(0);
        machinery_basis_core::active_fx::bit_random_double(high, low)
    }

    fn add_particle(&mut self, spawn: machinery_basis_core::active_fx::Spawn) {
        self.0.add(&fx_options(spawn.particle), spawn.pos, [0.0; 3]);
    }
}

/// `BlockTile.animateTick` (`BlockTile.java:80-103`), para o bloco sorteado pelo `ClientLevel.animateTick`: `fx_of` devolve o `AttributeParticleFX` do bloco
/// (ou `None` se ele não tem). Com `machineEffects` ligado e o bloco ativo (`lit`), solta uma partícula por entrada, na posição que ela sorteia, girada pelo
/// `facing` e somada ao centro do bloco. O gerador é o do motor de partículas, o mesmo do sorteio das posições.
pub(in crate::game) fn animate_active_block(
    engine: &mut horizon_client::particles::ParticleEngine,
    ctx: &horizon_client::animate_tick::AnimateContext<'_>,
    state: BlockStateId,
    pos: [i32; 3],
    fx_of: &dyn Fn(&str) -> Option<&'static [machinery_basis_core::active_fx::FxParticle]>,
) {
    use machinery_basis_core::active_fx::{Facing, animate_tick, center_of};
    let Some(row) = ctx.registry.state(state) else {
        return;
    };
    let Some(fx) = fx_of(&row.name) else {
        return;
    };
    let active = row.properties.get("lit").map(String::as_str) == Some("true");
    let facing = Facing::from_property(row.properties.get("facing").map_or("", String::as_str));
    let effects = MACHINE_EFFECTS.load(Ordering::Relaxed);
    animate_tick(fx, facing, center_of(pos), active, effects, &mut FxLevel(engine));
    if FX_TRACE && active && effects {
        eprintln!("MEKFX gt={} block={} pos={pos:?} facing={facing:?} n={}", ctx.world.game_time, row.name, fx.len());
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// A configuração de lados de um tile (`TileComponentConfig` e `TileComponentEjector`): o NBT do tile, os componentes do item e a
// colocação e a quebra do bloco.

/// `SerializationConstants.COMPONENT_CONFIG` e `COMPONENT_EJECTOR`: os filhos do NBT do tile.
/// Liga as linhas `MEKSIDE` do stderr (a colocação e a quebra), para o teste em jogo; o repositório fica com `false`.
const SIDE_TRACE: bool = false;
const COMPONENT_CONFIG: &str = "component_config";
const COMPONENT_EJECTOR: &str = "component_ejector";
/// `EJECT` e `CONFIG`, mais o `ordinal` do tipo de transmissão.
const EJECT_PREFIX: &str = "eject";
const CONFIG_PREFIX: &str = "config";
/// `STRICT_INPUT`, `COLOR` e `INPUT_COLOR` do ejetor.
const STRICT_INPUT: &str = "strict_input";
const EJECTOR_COLOR: &str = "color";
const INPUT_COLOR: &str = "input_color";

/// O que o NBT de um tile guarda da configuração de lados e do ejetor. O ejetor some do NBT quando está no padrão
/// (`ITileComponent.write` descarta o filho vazio).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::game) struct SideNbt {
    pub config: Option<machinery_basis_core::side_config::ConfigData>,
    pub ejector: Option<machinery_basis_core::ejector::EjectorData>,
}

impl SideNbt {
    /// O que os dois componentes gravam: o ejetor no padrão não deixa filho.
    pub fn of(config: machinery_basis_core::side_config::ConfigData, ejector: machinery_basis_core::ejector::EjectorData) -> Self {
        Self { config: Some(config), ejector: (!ejector.is_empty()).then_some(ejector) }
    }
}

fn nbt_flag(tag: Option<&Tag>) -> Option<bool> {
    match tag? {
        Tag::Byte(value) => Some(*value != 0),
        Tag::Int(value) => Some(*value != 0),
        _ => None,
    }
}

fn child<'a>(nbt: &'a Compound, key: &str) -> Option<&'a Compound> {
    match nbt.get(&JavaString::from(key)) {
        Some(Tag::Compound(compound)) => Some(compound),
        _ => None,
    }
}

/// `component_config` e `component_ejector` do tile, como estão agora.
pub(in crate::game) fn read_side_nbt(entity: &ModBlockEntity) -> SideNbt {
    side_nbt_from(&entity.nbt)
}

/// `component_config` e `component_ejector` de um compound: o do tile, ou o que `writeConfigurationData` entrega.
pub(in crate::game) fn side_nbt_from(nbt: &Compound) -> SideNbt {
    use machinery_basis_core::side_config::{ConfigData, ENUM_COLOR_NAMES, TransmissionType};
    let config = child(nbt, COMPONENT_CONFIG).map(|compound| {
        let mut data = ConfigData::default();
        for kind in TransmissionType::VALUES {
            let ordinal = kind.ordinal();
            data.eject[ordinal] = nbt_flag(compound.get(&JavaString::from(format!("{EJECT_PREFIX}{ordinal}").as_str())));
            if let Some(Tag::IntArray(sides)) = compound.get(&JavaString::from(format!("{CONFIG_PREFIX}{ordinal}").as_str())) {
                data.config[ordinal] = Some(sides.clone());
            }
        }
        data
    });
    let ejector = child(nbt, COMPONENT_EJECTOR).map(|compound| {
        let color = match compound.get(&JavaString::from(EJECTOR_COLOR)) {
            Some(Tag::String(name)) => {
                let name = name.to_string_lossy();
                ENUM_COLOR_NAMES.iter().position(|known| *known == name).map(|index| index as u8)
            }
            _ => None,
        };
        machinery_basis_core::ejector::EjectorData {
            strict_input: nbt_flag(compound.get(&JavaString::from(STRICT_INPUT))),
            color,
            input_color: match compound.get(&JavaString::from(INPUT_COLOR)) {
                Some(Tag::IntArray(colors)) => Some(colors.clone()),
                _ => None,
            },
        }
    });
    SideNbt { config, ejector }
}

/// Grava a configuração e o ejetor no NBT do tile, como `TileComponentConfig.write` e `TileComponentEjector.write`: o ejetor vazio
/// apaga o filho.
pub(in crate::game) fn write_side_nbt(entity: &mut ModBlockEntity, side: &SideNbt) {
    side_nbt_into(&mut entity.nbt, side);
}

/// O mesmo, num compound qualquer.
pub(in crate::game) fn side_nbt_into(nbt: &mut Compound, side: &SideNbt) {
    use machinery_basis_core::side_config::{ENUM_COLOR_NAMES, TransmissionType};
    let mut compound = Compound::default();
    if let Some(data) = &side.config {
        for kind in TransmissionType::VALUES {
            let ordinal = kind.ordinal();
            if let Some(ejecting) = data.eject[ordinal] {
                compound.insert(JavaString::from(format!("{EJECT_PREFIX}{ordinal}").as_str()), Tag::Byte(i8::from(ejecting)));
            }
            if let Some(sides) = &data.config[ordinal] {
                compound.insert(JavaString::from(format!("{CONFIG_PREFIX}{ordinal}").as_str()), Tag::IntArray(sides.clone()));
            }
        }
        nbt.insert(JavaString::from(COMPONENT_CONFIG), Tag::Compound(compound));
    }
    match &side.ejector {
        Some(data) if !data.is_empty() => {
            let mut compound = Compound::default();
            if let Some(strict) = data.strict_input {
                compound.insert(JavaString::from(STRICT_INPUT), Tag::Byte(i8::from(strict)));
            }
            if let Some(color) = data.color {
                let name = ENUM_COLOR_NAMES[usize::from(color).min(ENUM_COLOR_NAMES.len() - 1)];
                compound.insert(JavaString::from(EJECTOR_COLOR), Tag::String(JavaString::from(name)));
            }
            if let Some(colors) = &data.input_color {
                compound.insert(JavaString::from(INPUT_COLOR), Tag::IntArray(colors.clone()));
            }
            nbt.insert(JavaString::from(COMPONENT_EJECTOR), Tag::Compound(compound));
        }
        _ => {
            nbt.remove(&JavaString::from(COMPONENT_EJECTOR));
        }
    }
}

/// Os componentes do item que carregam a configuração: `AttachedSideConfig` e `AttachedEjector`.
#[derive(Clone, Copy)]
pub(in crate::game) struct SideComponents {
    pub side_config: &'static str,
    pub ejector: &'static str,
}

/// O que uma máquina tem de próprio nesta configuração: como o componente é montado e o padrão do item de bloco
/// (`ItemBlock...component(SIDE_CONFIG, ...)`).
pub(in crate::game) struct SideSpec {
    pub spec: machinery_basis_core::tile::SideConfigSpec,
    pub default_side_config: machinery_basis_core::side_config::AttachedSideConfig,
}

fn data_type_tag(data_type: machinery_basis_core::side_config::DataType) -> Tag {
    Tag::String(JavaString::from(data_type.serialized_name()))
}

/// `AttachedSideConfig.CODEC` como NBT: `{config: {<tipo>: {side: {<lado>: <datatype>}, eject: <bool>}}}`. O `eject` só vai quando
/// verdadeiro (`optionalFieldOf("eject", false)`).
pub(in crate::game) fn side_config_to_tag(attached: &machinery_basis_core::side_config::AttachedSideConfig) -> Tag {
    let mut types = Compound::default();
    for (kind, light) in &attached.config_info {
        let mut sides = Compound::default();
        for (side, data_type) in &light.side_config {
            sides.insert(JavaString::from(side.serialized_name()), data_type_tag(*data_type));
        }
        let mut info = Compound::default();
        info.insert(JavaString::from("side"), Tag::Compound(sides));
        if light.ejecting {
            info.insert(JavaString::from("eject"), Tag::Byte(1));
        }
        types.insert(JavaString::from(kind.serialized_name()), Tag::Compound(info));
    }
    let mut root = Compound::default();
    root.insert(JavaString::from("config"), Tag::Compound(types));
    Tag::Compound(root)
}

/// O inverso. Tipo ou lado que a lib não conhece é ignorado, e o `DataType` desconhecido vira `NONE`.
pub(in crate::game) fn side_config_from_tag(tag: &Tag) -> Option<machinery_basis_core::side_config::AttachedSideConfig> {
    use machinery_basis_core::side_config::{
        AttachedSideConfig, DataType, LightConfigInfo, RelativeSide, TransmissionType,
    };
    let Tag::Compound(root) = tag else {
        return None;
    };
    let Some(Tag::Compound(types)) = root.get(&JavaString::from("config")) else {
        return None;
    };
    let mut attached = AttachedSideConfig::default();
    for (name, info) in types {
        let (Some(kind), Tag::Compound(info)) = (TransmissionType::from_serialized_name(&name.to_string_lossy()), info) else {
            continue;
        };
        let mut light = LightConfigInfo { ejecting: nbt_flag(info.get(&JavaString::from("eject"))).unwrap_or(false), ..Default::default() };
        if let Some(Tag::Compound(sides)) = info.get(&JavaString::from("side")) {
            for (side, data_type) in sides {
                let Some(side) = RelativeSide::from_serialized_name(&side.to_string_lossy()) else {
                    continue;
                };
                let data_type = match data_type {
                    Tag::String(name) => DataType::from_serialized_name(&name.to_string_lossy()).unwrap_or(DataType::None),
                    _ => DataType::None,
                };
                light.side_config.insert(side, data_type);
            }
        }
        attached.config_info.insert(kind, light);
    }
    Some(attached)
}

/// A chave da cor de saída no componente do item (`AttachedEjector.CODEC` usa `SerializationConstants.TYPES`, e não `COLOR`, que é
/// a do NBT do tile): o original grava assim e o port copia.
const EJECTOR_ITEM_COLOR: &str = "types";

/// `AttachedEjector.CODEC` como NBT: `input_color` (uma entrada por lado: um compound vazio sem cor, ou o nome da cor, que numa lista
/// com compounds o NBT guarda embrulhado num compound de chave vazia, como o arquivo do jogo faz), `strict_input` e `types` (a cor
/// de saída, só quando há).
pub(in crate::game) fn ejector_to_tag(attached: &machinery_basis_core::ejector::AttachedEjector) -> Tag {
    use machinery_basis_core::side_config::ENUM_COLOR_NAMES;
    let colors = attached
        .input_colors
        .iter()
        .map(|color| {
            let mut wrapped = Compound::default();
            if let Some(color) = color {
                wrapped.insert(JavaString::from(""), Tag::String(JavaString::from(ENUM_COLOR_NAMES[usize::from(*color).min(17)])));
            }
            Tag::Compound(wrapped)
        })
        .collect();
    let mut root = Compound::default();
    root.insert(JavaString::from(INPUT_COLOR), Tag::List(colors));
    root.insert(JavaString::from(STRICT_INPUT), Tag::Byte(i8::from(attached.strict_input)));
    if let Some(color) = attached.output_color {
        root.insert(JavaString::from(EJECTOR_ITEM_COLOR), Tag::String(JavaString::from(ENUM_COLOR_NAMES[usize::from(color).min(17)])));
    }
    Tag::Compound(root)
}

pub(in crate::game) fn ejector_from_tag(tag: &Tag) -> Option<machinery_basis_core::ejector::AttachedEjector> {
    use machinery_basis_core::side_config::ENUM_COLOR_NAMES;
    let Tag::Compound(root) = tag else {
        return None;
    };
    let color_of = |name: &JavaString| {
        let name = name.to_string_lossy();
        ENUM_COLOR_NAMES.iter().position(|known| *known == name).map(|index| index as u8)
    };
    // Um nome solto, ou embrulhado num compound de chave vazia.
    let entry = |tag: &Tag| match tag {
        Tag::String(name) => color_of(name),
        Tag::Compound(wrapped) => match wrapped.get(&JavaString::from("")) {
            Some(Tag::String(name)) => color_of(name),
            _ => None,
        },
        _ => None,
    };
    let mut attached = machinery_basis_core::ejector::AttachedEjector::DEFAULT;
    if let Some(Tag::List(colors)) = root.get(&JavaString::from(INPUT_COLOR)) {
        for (slot, color) in attached.input_colors.iter_mut().zip(colors.iter()) {
            *slot = entry(color);
        }
    }
    attached.strict_input = nbt_flag(root.get(&JavaString::from(STRICT_INPUT))).unwrap_or(false);
    attached.output_color = match root.get(&JavaString::from(EJECTOR_ITEM_COLOR)) {
        Some(Tag::String(name)) => color_of(name),
        _ => None,
    };
    Some(attached)
}

thread_local! {
    /// O que o item que acabou de ser colocado traz de configuração, para o tile que o Horizon ainda não criou.
    static PENDING_SIDES: std::cell::RefCell<HashMap<[i32; 3], SideNbt>> = std::cell::RefCell::new(HashMap::new());
    /// A configuração do tile que o jogador está quebrando, guardada antes de o Horizon tirá-lo do mundo.
    static BROKEN_SIDES: std::cell::RefCell<Option<([i32; 3], Option<SideNbt>)>> = const { std::cell::RefCell::new(None) };
}

/// `applyImplicitComponents` da configuração e do ejetor: o item colocado traz `side_config` e `ejector`, e sem eles vale o padrão do
/// item de bloco. A configuração do tile novo nasce daí e vai para o NBT dele no primeiro tick ([`side_apply_pending`]). Quem põe
/// a máquina por `setBlock` não passa por aqui e fica com tudo em `NONE`.
pub(in crate::game) fn side_placed(
    game: &mut Game<'_>,
    stack: &ItemStack,
    pos: [i32; 3],
    components: SideComponents,
    spec_of: &dyn Fn(&str) -> Option<SideSpec>,
) {
    let Some(side) = spec_of(&game.native_block(pos)) else {
        return;
    };
    let of_item = |id: &str| game.native_component(stack, id).ok().flatten();
    let attached = of_item(components.side_config)
        .and_then(side_config_from_tag)
        .unwrap_or(side.default_side_config);
    let ejector = of_item(components.ejector).and_then(ejector_from_tag).unwrap_or_default();
    let mut config = machinery_basis_core::side_config::TileComponentConfig::new(&side.spec.types);
    (side.spec.setup)(&mut config);
    let mut ejecting = machinery_basis_core::ejector::TileComponentEjector::new();
    ejecting.apply_implicit_components(Some(&ejector));
    config.apply_implicit_components(Some(&attached), front(game, pos), false);
    let nbt = SideNbt { config: Some(config.serialize()), ejector: Some(ejecting.serialize()) };
    // O tile já existe quando o jogo avisa a colocação: a configuração entra nele agora, e quem lê a capacidade e as faces de energia
    // desde a colocação (`placed_capacity`) já as vê. Sem tile, ela espera o primeiro tick.
    if let Some(horizon_client::containers::BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&(pos[0], pos[1], pos[2])) {
        write_side_nbt(entity, &nbt);
        game.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
        return;
    }
    PENDING_SIDES.with_borrow_mut(|sides| sides.insert(pos, nbt));
}

/// O tile nasceu: a configuração do item que o colocou entra no NBT dele.
pub(in crate::game) fn side_apply_pending(entity: &mut ModBlockEntity, pos: [i32; 3]) {
    if let Some(side) = PENDING_SIDES.with_borrow_mut(|sides| sides.remove(&pos)) {
        write_side_nbt(entity, &side);
    }
}

/// O que o item solto leva (`collectImplicitComponents`): a configuração e o ejetor do tile em `pos`, lidos do NBT dele. Só o que
/// difere do padrão do item vira componente (`AttachedSideConfig` e `AttachedEjector` iguais ao padrão não fazem retalho).
fn side_components_of(
    nbt: &SideNbt,
    side: &SideSpec,
    facing: Face,
) -> (machinery_basis_core::side_config::AttachedSideConfig, machinery_basis_core::ejector::AttachedEjector) {
    let mut config = machinery_basis_core::side_config::TileComponentConfig::new(&side.spec.types);
    (side.spec.setup)(&mut config);
    let mut ejector = machinery_basis_core::ejector::TileComponentEjector::new();
    config.deserialize(&nbt.config.clone().unwrap_or_default(), facing, false);
    ejector.deserialize(&nbt.ejector.clone().unwrap_or_default());
    (config.collect_implicit_components(), ejector.collect_implicit_components())
}

/// O jogador vai quebrar o tile em `pos`: o Horizon o tira do mundo antes de rolar a tabela de loot, e a configuração que o item
/// solto leva (`copy_components`) é guardada aqui.
pub(in crate::game) fn side_destroying(game: &Game<'_>, pos: [i32; 3], spec_of: &dyn Fn(&str) -> Option<SideSpec>) {
    use horizon_client::containers::BlockEntity;
    let nbt = match game.world.block_entities.get(&(pos[0], pos[1], pos[2])) {
        Some(BlockEntity::Mod(entity)) if spec_of(&entity.id).is_some() => Some(read_side_nbt(entity)),
        _ => None,
    };
    if SIDE_TRACE {
        eprintln!("MEKSIDE destroying pos={pos:?} nbt={}", nbt.is_some());
    }
    BROKEN_SIDES.with_borrow_mut(|slot| *slot = Some((pos, nbt)));
}

/// `copy_components` da tabela de loot: o item solto leva `side_config` e `ejector` do tile, os que não são o padrão. Um tile ainda
/// no mundo dá o que ele tem; o que foi quebrado, o que [`side_destroying`] guardou.
pub(in crate::game) fn side_loot(
    game: &Game<'_>,
    context: &horizon_client::mod_api::NativeLootDrops<'_>,
    drops: &mut [ItemStack],
    components: SideComponents,
    spec_of: &dyn Fn(&str) -> Option<SideSpec>,
) {
    use horizon_client::containers::BlockEntity;
    if SIDE_TRACE {
        eprintln!("MEKSIDE loot block={:?} drops={}", context.block.map(|(pos, _)| pos), drops.len());
    }
    let Some((pos, _)) = context.block else {
        return;
    };
    let kept = BROKEN_SIDES.with_borrow_mut(|slot| match slot {
        Some((at, _)) if *at == pos => slot.take().and_then(|(_, nbt)| nbt),
        _ => None,
    });
    let live = match game.world.block_entities.get(&(pos[0], pos[1], pos[2])) {
        Some(BlockEntity::Mod(entity)) if spec_of(&entity.id).is_some() => Some(read_side_nbt(entity)),
        _ => None,
    };
    let Some(nbt) = live.or(kept) else {
        return;
    };
    let facing = front(game, pos);
    for stack in drops.iter_mut() {
        let Some(side) = spec_of(&stack.item) else {
            continue;
        };
        let (attached, ejector) = side_components_of(&nbt, &side, facing);
        if attached != side.default_side_config
            && let Err(error) = game.native_set_component(stack, components.side_config, side_config_to_tag(&attached))
        {
            eprintln!("machinery_basis: o item solto não leva {}: {error}", components.side_config);
        }
        if ejector != machinery_basis_core::ejector::AttachedEjector::DEFAULT
            && let Err(error) = game.native_set_component(stack, components.ejector, ejector_to_tag(&ejector))
        {
            eprintln!("machinery_basis: o item solto não leva {}: {error}", components.ejector);
        }
    }
}

thread_local! {
    /// A energia que o item que o jogador acabou de pôr traz, para o tile que o Horizon ainda não criou.
    static PENDING_ENERGY: std::cell::RefCell<HashMap<[i32; 3], i64>> = std::cell::RefCell::new(HashMap::new());
    /// A energia do tile que o jogador está quebrando, guardada antes de o Horizon tirá-lo do mundo.
    static BROKEN_ENERGY: std::cell::RefCell<Option<([i32; 3], Option<i64>)>> = const { std::cell::RefCell::new(None) };
}

/// O valor do componente de energia de uma pilha (`ContainerType.ENERGY.getOrZero`): o `long` do componente, ou zero sem ele. O componente do item é um `long`; um valor de outro
/// tipo inteiro (um comando que escreveu `1000` sem o sufixo `L`) vale como o número que é.
pub(in crate::game) fn item_energy_of(game: &Game<'_>, stack: &ItemStack, component: &str) -> i64 {
    match game.native_component(stack, component) {
        Ok(Some(Tag::Long(value))) => (*value).max(0),
        Ok(Some(Tag::Int(value))) => i64::from(*value).max(0),
        Ok(Some(Tag::Short(value))) => i64::from(*value).max(0),
        Ok(Some(Tag::Byte(value))) => i64::from(*value).max(0),
        _ => 0,
    }
}

/// Grava a energia no componente da pilha (`ContainerType.ENERGY.attachCopyToStack`). O zero é o valor do item (`addDefault` põe `0L`), então volta ao padrão e a pilha empilha com a nova.
pub(in crate::game) fn set_item_energy(game: &Game<'_>, stack: &mut ItemStack, component: &str, energy: i64) {
    let result = if energy == 0 {
        game.native_remove_component(stack, component)
    } else {
        game.native_set_component(stack, component, Tag::Long(energy))
    };
    if let Err(error) = result {
        eprintln!("machinery_basis: o item solto não leva {component}: {error}");
    }
}

/// `TileEntityMekanism.applyImplicitComponents` -> `EnergyContainerType.copyToTile`: o item posto traz a energia, e o tile a guarda limitada à capacidade dele
/// (`copyToContainer`). Com o tile já no mundo a energia entra nele agora; sem ele espera o primeiro tick ([`energy_apply_pending`]). Um item sem o componente não muda nada: o tile
/// nasce vazio.
pub(in crate::game) fn energy_placed(game: &mut Game<'_>, stack: &ItemStack, pos: [i32; 3], component: &str, capacity: i64) {
    let carried = item_energy_of(game, stack, component);
    if carried == 0 {
        PENDING_ENERGY.with_borrow_mut(|pending| pending.remove(&pos));
        return;
    }
    if let Some(horizon_client::containers::BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&(pos[0], pos[1], pos[2])) {
        let stored = machinery_basis_core::item_energy::copy_to_container(carried, capacity);
        set_stored_energy_long(entity, stored);
        game.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
        return;
    }
    PENDING_ENERGY.with_borrow_mut(|pending| pending.insert(pos, carried));
}

/// O tile nasceu: a energia do item que o colocou entra nele, limitada à capacidade que a máquina tem agora.
pub(in crate::game) fn energy_apply_pending(entity: &mut ModBlockEntity, pos: [i32; 3], capacity: i64) {
    if let Some(carried) = PENDING_ENERGY.with_borrow_mut(|pending| pending.remove(&pos)) {
        let stored = machinery_basis_core::item_energy::copy_to_container(carried, capacity);
        set_stored_energy_long(entity, stored);
    }
}

/// O jogador (ou uma explosão que passa pelo hook) vai quebrar o tile em `pos`: o Horizon o tira do mundo antes de rolar a tabela de loot, e a energia que o item solto leva
/// (`copy_components` com `mekanism:energy`) é guardada aqui, enquanto o tile ainda existe.
pub(in crate::game) fn energy_destroying(game: &Game<'_>, pos: [i32; 3], is_ours: &dyn Fn(&str) -> bool) {
    use horizon_client::containers::BlockEntity;
    let energy = match game.world.block_entities.get(&(pos[0], pos[1], pos[2])) {
        Some(BlockEntity::Mod(entity)) if is_ours(&entity.id) => Some(stored_energy_long(entity)),
        _ => None,
    };
    BROKEN_ENERGY.with_borrow_mut(|slot| *slot = Some((pos, energy)));
}

/// `copy_components` da tabela de loot: o item solto leva a energia do tile (`collectImplicitComponents` -> `copyFromTile` -> `attachedCopyOf`, sem limitar à capacidade do item). Um tile
/// ainda no mundo dá a que ele tem; o que foi quebrado, a que [`energy_destroying`] guardou.
pub(in crate::game) fn energy_loot(
    game: &Game<'_>,
    context: &horizon_client::mod_api::NativeLootDrops<'_>,
    drops: &mut [ItemStack],
    component: &str,
    is_ours: &dyn Fn(&str) -> bool,
) {
    use horizon_client::containers::BlockEntity;
    let Some((pos, _)) = context.block else {
        return;
    };
    // Cada mod com energia chama isto: o valor guardado só sai para quem tem o item entre os que caíram (os outros mods o veem depois).
    if !drops.iter().any(|stack| is_ours(&stack.item)) {
        return;
    }
    let kept = BROKEN_ENERGY.with_borrow_mut(|slot| match slot {
        Some((at, _)) if *at == pos => slot.take().and_then(|(_, energy)| energy),
        _ => None,
    });
    let live = match game.world.block_entities.get(&(pos[0], pos[1], pos[2])) {
        Some(BlockEntity::Mod(entity)) if is_ours(&entity.id) => Some(stored_energy_long(entity)),
        _ => None,
    };
    let Some(energy) = live.or(kept) else {
        return;
    };
    for stack in drops.iter_mut().filter(|stack| is_ours(&stack.item)) {
        set_item_energy(game, stack, component, energy);
    }
}

/// Os textos e as cores da linha que um item com energia ganha no tooltip (`STORED_ENERGY.translateColored(BRIGHT_GREEN, GRAY, EnergyDisplay.of(handler))`).
pub(in crate::game) struct StoredEnergyLine<'a> {
    pub component: &'a str,
    pub label: &'a str,
    pub label_color: u32,
    pub arg_color: u32,
    pub unit: &'a str,
    pub fraction: &'a str,
    pub infinite: &'a str,
}

/// A linha do tooltip de um item que guarda `capacity` de energia: o rótulo com a energia e a capacidade (`EnergyDisplay.of(handler)`), cada cor no seu pedaço, e a cor do
/// rótulo e do argumento nos pedaços de texto.
pub(in crate::game) fn stored_energy_line(
    game: &Game<'_>,
    stack: &ItemStack,
    capacity: i64,
    line: &StoredEnergyLine<'_>,
) -> (horizon_client::mod_api::Text, u32) {
    use horizon_client::mod_api::Text;
    let stored = item_energy_of(game, stack, line.component);
    let display = machinery_basis_core::item_energy::energy_display(
        stored,
        capacity,
        &horizon_client::lang::get(line.unit).to_string(),
        &horizon_client::lang::get(line.fraction).to_string(),
        &horizon_client::lang::get(line.infinite).to_string(),
    );
    // O rótulo (`BRIGHT_GREEN`) e a energia (`GRAY`) são pedaços de texto com a própria cor, e `u32::MAX` deixa o hook mantê-las (a cor da linha do hook não é `rgba` de `0xRRGGBBAA`).
    let template = horizon_client::lang::get(line.label).to_string();
    let (head, tail) = template.split_once("%1$s").unwrap_or((template.as_str(), ""));
    let mut text = Text::colored(head, line.label_color).append(Text::colored(display, line.arg_color));
    if !tail.is_empty() {
        text = text.append(Text::colored(tail, line.label_color));
    }
    (text, u32::MAX)
}

// ---------------------------------------------------------------------------------------------------------------------------
// O conteúdo guardado no item (frente `item-conteudo`, Fase 5): `TileEntityMekanism.collectImplicitComponents` e `applyImplicitComponents`. A máquina quebrada leva no item
// os upgrades (`mekanism:upgrades`), os slots (`mekanism:items`), os tanques (`mekanism:chemicals`, `mekanism:fluids`), o calor (`mekanism:heat`) e o modo de redstone
// (`mekanism:redstone_control`), e o item posto os devolve ao tile. A conta de cada componente é `machinery_basis_core::item_content`, conferida contra o rastro do original.
// Nada disto cai no chão: o original não solta o conteúdo do tile (`TileEntityMekanism.preRemoveSideEffects` não esvazia nada), então os slots saem do tile ao quebrar.
// ---------------------------------------------------------------------------------------------------------------------------

use machinery_basis_core::item_content as content;

pub const CONTENT_UPGRADES: &str = "mekanism:upgrades";
pub const CONTENT_ITEMS: &str = "mekanism:items";
pub const CONTENT_CHEMICALS: &str = "mekanism:chemicals";
pub const CONTENT_FLUIDS: &str = "mekanism:fluids";
pub const CONTENT_HEAT: &str = "mekanism:heat";
pub const CONTENT_REDSTONE: &str = "mekanism:redstone_control";
/// Os componentes de conteúdo, para o mod registrá-los.
pub const CONTENT_COMPONENTS: [&str; 6] = [CONTENT_UPGRADES, CONTENT_ITEMS, CONTENT_CHEMICALS, CONTENT_FLUIDS, CONTENT_HEAT, CONTENT_REDSTONE];
const CONTROL_TYPE_KEY: &str = "control_type";

/// O que um bloco guarda no item. `collect` e `apply` são os componentes que só o mod sabe (os tanques e o calor), cada um na forma do componente do original.
pub(in crate::game) struct ContentSpec<'a> {
    pub is_ours: &'a dyn Fn(&str) -> bool,
    /// `ContainerType.ITEM`: quantos slots do tile vão para o item (os de `entity.items` antes dos virtuais da janela de upgrades).
    pub item_slots: usize,
    pub collect: &'a dyn Fn(&Game<'_>, &ModBlockEntity, [i32; 3]) -> Vec<(&'static str, Tag)>,
    pub apply: &'a dyn Fn(&mut ModBlockEntity, &dyn Fn(&str) -> Option<Tag>),
}

thread_local! {
    /// Os componentes de conteúdo do item que o jogador acabou de pôr, para o tile que o Horizon ainda não criou.
    static PENDING_CONTENT: std::cell::RefCell<HashMap<[i32; 3], Vec<(String, Tag)>>> = std::cell::RefCell::new(HashMap::new());
    /// O conteúdo do tile que vai ser quebrado, guardado antes de o Horizon tirá-lo do mundo.
    static BROKEN_CONTENT: std::cell::RefCell<HashMap<[i32; 3], Vec<(&'static str, Tag)>>> = std::cell::RefCell::new(HashMap::new());
}

/// O `Tag` como o JSON do rastro: os inteiros viram números, e o formato do componente é o que a conta de `item_content` espera.
pub(in crate::game) fn tag_value(tag: &Tag) -> serde_json::Value {
    use serde_json::{Number, Value};
    match tag {
        Tag::Byte(value) => Value::from(*value),
        Tag::Short(value) => Value::from(*value),
        Tag::Int(value) => Value::from(*value),
        Tag::Long(value) => Value::from(*value),
        Tag::Float(bits) => Number::from_f64(f64::from(f32::from_bits(*bits))).map_or(Value::Null, Value::Number),
        Tag::Double(bits) => Number::from_f64(f64::from_bits(*bits)).map_or(Value::Null, Value::Number),
        Tag::String(text) => Value::String(text.to_string_lossy()),
        Tag::List(list) => Value::Array(list.iter().map(tag_value).collect()),
        Tag::Compound(map) => Value::Object(map.iter().map(|(key, value)| (key.to_string_lossy(), tag_value(value))).collect()),
        Tag::IntArray(ints) => Value::from(ints.clone()),
        _ => Value::Null,
    }
}

/// O contrário, com a largura que cada campo tem no original: `amount` é `long`, `container` é `byte`, o calor é `double`, o resto cabe num `int`.
pub(in crate::game) fn value_tag(value: &serde_json::Value, key: &str) -> Tag {
    use serde_json::Value;
    match value {
        Value::Number(number) => match key {
            "amount" => Tag::Long(number.as_i64().unwrap_or(0)),
            "container" => Tag::Byte(number.as_i64().and_then(|n| i8::try_from(n).ok()).unwrap_or(0)),
            "heat" | "heat_capacity" => Tag::Double(number.as_f64().unwrap_or(0.0).to_bits()),
            _ => match number.as_i64() {
                Some(whole) => i32::try_from(whole).map_or(Tag::Long(whole), Tag::Int),
                None => Tag::Double(number.as_f64().unwrap_or(0.0).to_bits()),
            },
        },
        Value::Array(list) => Tag::List(list.iter().map(|item| value_tag(item, key)).collect()),
        Value::Object(map) => Tag::Compound(map.iter().map(|(name, item)| (JavaString::from(name.as_str()), value_tag(item, name))).collect()),
        Value::String(text) => Tag::String(JavaString::from(text.as_str())),
        Value::Bool(flag) => Tag::Byte(i8::from(*flag)),
        Value::Null => Tag::Compound(Compound::default()),
    }
}

/// `LargeResourceStack.codec` de um `ItemResource`: `{type: {id, components}, amount}`, com os componentes da pilha como o `ItemResource.CODEC` os grava.
fn item_stored(stack: &ItemStack) -> Tag {
    let mut kind = Compound::default();
    kind.insert(JavaString::from(ITEM_ID_KEY), Tag::String(JavaString::from(&*stack.item)));
    if let Some(components) = encode_components(stack)
        && matches!(&components, Tag::Compound(map) if !map.is_empty())
    {
        kind.insert(JavaString::from(COMPONENTS_KEY), components);
    }
    let mut stored = Compound::default();
    stored.insert(JavaString::from(TYPE_KEY), Tag::Compound(kind));
    stored.insert(JavaString::from(AMOUNT_KEY), Tag::Long(i64::from(stack.count)));
    Tag::Compound(stored)
}

/// O contrário: a pilha de um contêiner de item do componente, ou `None` se ele está vazio ou não se lê.
fn stack_of_stored(stored: &Tag) -> Option<ItemStack> {
    let Tag::Compound(stored) = stored else {
        return None;
    };
    let Some(Tag::Compound(kind)) = stored.get(&JavaString::from(TYPE_KEY)) else {
        return None;
    };
    let Some(Tag::String(id)) = kind.get(&JavaString::from(ITEM_ID_KEY)) else {
        return None;
    };
    let amount = match stored.get(&JavaString::from(AMOUNT_KEY)) {
        Some(Tag::Long(amount)) => u32::try_from(*amount).ok()?,
        Some(Tag::Int(amount)) => u32::try_from(*amount).ok()?,
        _ => return None,
    };
    if amount == 0 {
        return None;
    }
    let (damage, extra) = decode_components(kind.get(&JavaString::from(COMPONENTS_KEY)));
    Some(ItemStack { item: Arc::from(id.to_string_lossy().as_str()), count: amount, damage, extra })
}

fn containers_tag(containers: Vec<Tag>) -> Tag {
    let mut component = Compound::default();
    component.insert(JavaString::from("containers"), Tag::List(containers));
    Tag::Compound(component)
}

/// Os componentes de conteúdo que o tile dá ao item (`collectImplicitComponents`), só os que diferem do padrão do item.
fn content_components(game: &Game<'_>, entity: &ModBlockEntity, pos: [i32; 3], spec: &ContentSpec<'_>) -> Vec<(&'static str, Tag)> {
    let mut out = Vec::new();
    let nbt = |key: &str| entity.nbt.get(&JavaString::from(key));
    let upgrades = nbt(COMPONENT_UPGRADE).map(tag_value);
    if let Some(value) = content::upgrades_collect(upgrades.as_ref()) {
        out.push((CONTENT_UPGRADES, value_tag(&value, "")));
    }
    let slots: Vec<Option<&ItemStack>> = (0..spec.item_slots)
        .map(|slot| entity.items.get(slot).and_then(Option::as_ref).filter(|stack| stack.count > 0))
        .collect();
    let judged: Vec<serde_json::Value> =
        slots.iter().map(|stack| if stack.is_some() { serde_json::json!({"type": {}, "amount": 1}) } else { serde_json::json!({}) }).collect();
    if content::containers_collect(&judged).is_some() {
        let stored = slots.iter().map(|stack| stack.map_or_else(|| Tag::Compound(Compound::default()), item_stored)).collect();
        out.push((CONTENT_ITEMS, containers_tag(stored)));
    }
    let control = match nbt(CONTROL_TYPE_KEY) {
        Some(Tag::String(name)) => name.to_string_lossy(),
        _ => content::REDSTONE_DISABLED.to_owned(),
    };
    if let Some(value) = content::redstone_collect(&control) {
        out.push((CONTENT_REDSTONE, value_tag(&value, "")));
    }
    out.extend((spec.collect)(game, entity, pos));
    out
}

/// O tile ganha o conteúdo do item (`applyImplicitComponents`): os upgrades e os slots entram no NBT e em `entity.items`, e o mod põe o que é dele.
fn content_apply(entity: &mut ModBlockEntity, components: &[(String, Tag)], spec: &ContentSpec<'_>) {
    let of = |id: &str| components.iter().find(|(name, _)| name == id).map(|(_, tag)| tag.clone());
    if let Some(upgrades) = of(CONTENT_UPGRADES) {
        let key = JavaString::from(COMPONENT_UPGRADE);
        match content::upgrades_apply(&tag_value(&upgrades)) {
            Some(value) => {
                entity.nbt.insert(key, value_tag(&value, ""));
            }
            None => {
                entity.nbt.remove(&key);
            }
        }
    }
    if let Some(items) = of(CONTENT_ITEMS)
        && content::containers_apply(&tag_value(&items), spec.item_slots).is_some()
        && let Tag::Compound(map) = &items
        && let Some(Tag::List(list)) = map.get(&JavaString::from("containers"))
    {
        if entity.items.len() < spec.item_slots {
            entity.items.resize(spec.item_slots, None);
        }
        for (slot, stored) in list.iter().enumerate() {
            entity.items[slot] = stack_of_stored(stored);
        }
    }
    if let Some(Tag::String(name)) = of(CONTENT_REDSTONE) {
        entity.nbt.insert(JavaString::from(CONTROL_TYPE_KEY), Tag::String(name));
    }
    (spec.apply)(entity, &of);
}

/// O item posto traz o conteúdo: com o tile já no mundo ele entra agora (a capacidade que `placed_capacity` lê já conta os upgrades); sem ele espera o primeiro tick.
pub(in crate::game) fn content_placed(game: &mut Game<'_>, stack: &ItemStack, pos: [i32; 3], spec: &ContentSpec<'_>) {
    let carried: Vec<(String, Tag)> = CONTENT_COMPONENTS
        .iter()
        .filter_map(|id| game.native_component(stack, id).ok().flatten().map(|tag| ((*id).to_owned(), tag.clone())))
        .collect();
    if carried.is_empty() {
        PENDING_CONTENT.with_borrow_mut(|pending| pending.remove(&pos));
        return;
    }
    if let Some(horizon_client::containers::BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&(pos[0], pos[1], pos[2])) {
        content_apply(entity, &carried, spec);
        game.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
        return;
    }
    PENDING_CONTENT.with_borrow_mut(|pending| pending.insert(pos, carried));
}

/// O tile nasceu: o conteúdo do item que o colocou entra nele.
pub(in crate::game) fn content_apply_pending(entity: &mut ModBlockEntity, pos: [i32; 3], spec: &ContentSpec<'_>) {
    if let Some(carried) = PENDING_CONTENT.with_borrow_mut(|pending| pending.remove(&pos)) {
        content_apply(entity, &carried, spec);
    }
}

/// O jogador (ou uma explosão) vai quebrar o tile em `pos`: o conteúdo que o item solto leva é guardado enquanto o tile existe, e os slots saem dele, porque o original não
/// os solta no chão (vão para o item).
pub(in crate::game) fn content_destroying(game: &mut Game<'_>, pos: [i32; 3], spec: &ContentSpec<'_>) {
    use horizon_client::containers::BlockEntity;
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(pos[0], pos[1], pos[2])) else {
        return;
    };
    if !(spec.is_ours)(&entity.id) {
        return;
    }
    let components = content_components(game, entity, pos, spec);
    BROKEN_CONTENT.with_borrow_mut(|broken| broken.insert(pos, components));
    if let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&(pos[0], pos[1], pos[2])) {
        // Todos os slots, os virtuais da janela de upgrades também: o conteúdo deles vai no `mekanism:upgrades`, e o Horizon soltaria o que sobrasse.
        for slot in entity.items.iter_mut() {
            *slot = None;
        }
    }
}

/// `copy_components` da tabela de loot: o item solto leva o conteúdo do tile. O que [`content_destroying`] guardou vale mais que o tile (que já saiu do mundo ou foi esvaziado).
pub(in crate::game) fn content_loot(
    game: &Game<'_>,
    context: &horizon_client::mod_api::NativeLootDrops<'_>,
    drops: &mut [ItemStack],
    spec: &ContentSpec<'_>,
) {
    use horizon_client::containers::BlockEntity;
    let Some((pos, _)) = context.block else {
        return;
    };
    if !drops.iter().any(|stack| (spec.is_ours)(&stack.item)) {
        return;
    }
    let kept = BROKEN_CONTENT.with_borrow_mut(|broken| broken.remove(&pos));
    let components = kept.or_else(|| match game.world.block_entities.get(&(pos[0], pos[1], pos[2])) {
        Some(BlockEntity::Mod(entity)) if (spec.is_ours)(&entity.id) => Some(content_components(game, entity, pos, spec)),
        _ => None,
    });
    let Some(components) = components else {
        return;
    };
    for stack in drops.iter_mut().filter(|stack| (spec.is_ours)(&stack.item)) {
        for (id, tag) in &components {
            if let Err(error) = game.native_set_component(stack, id, tag.clone()) {
                eprintln!("machinery_basis: o item solto não leva {id}: {error}");
            }
        }
    }
}

/// `MekanismDataComponents.registerComponentAppenderAfter(UPGRADES, ...)`: uma linha por upgrade instalado (`UpgradeDisplay`), `- Speed: x3`, ou `- Muffling` quando o tipo só cabe um.
pub(in crate::game) fn content_upgrade_lines(
    game: &Game<'_>,
    stack: &ItemStack,
    enum_color: fn(&str) -> u32,
) -> Vec<(horizon_client::mod_api::Text, u32)> {
    use horizon_client::mod_api::Text;
    let Ok(Some(Tag::Compound(component))) = game.native_component(stack, CONTENT_UPGRADES) else {
        return Vec::new();
    };
    let Some(Tag::Compound(totals)) = component.get(&JavaString::from(UPGRADES_KEY)) else {
        return Vec::new();
    };
    let types = upgrade::current_types();
    let mut installed: Vec<(String, i32)> = totals
        .iter()
        .filter_map(|(id, total)| match total {
            Tag::Int(total) => Some((id.to_string_lossy(), *total)),
            _ => None,
        })
        .collect();
    // `Object2IntOpenHashMap<Holder<Upgrade>>` itera na ordem do hash de identidade do `Holder`, que o HotSpot sorteia a cada execução e mantém nela (decisões 225 e 233): o port
    // sorteia o da `ResourceKey` de cada tipo, uma vez por processo, e refaz as tabelas do fastutil a partir da ordem do `HashMap` do NBT (`upgrade_order::item_tooltip_order`).
    installed.retain(|(id, total)| types.get(id).is_some() && *total > 0);
    let order = machinery_basis_core::upgrade_order::item_tooltip_order(&installed);
    let installed: Vec<(String, i32)> = order
        .into_iter()
        .filter_map(|id| installed.iter().find(|(known, _)| *known == id).cloned())
        .collect();
    installed
        .into_iter()
        .filter_map(|(id, total)| {
            let kind = types.get(&id)?;
            let name = Text::translate(&content_name_key("upgrade", &id), &[]);
            let color = enum_color(&kind.color) & 0xFF_FF_FF;
            let line = if kind.max > 1 && total > 0 {
                Text::translate("upgrade.mekanism.display.level", &[name, Text::colored(total.to_string(), CONTENT_GRAY)])
            } else {
                Text::translate("generic.mekanism.list", &[name])
            };
            Some((line.with_style(&horizon_client::text::Style::color(color)), u32::MAX))
        })
        .collect()
}

/// `MekKeyHandler.isKeyPressed(MekanismKeyHandler.descriptionKey)`: a tecla da descrição é Shift mais N (`KeyModifier.SHIFT`, tecla N). O Horizon não dá o estado de uma tecla qualquer a um
/// mod, mas o `Game` guarda as teclas físicas apertadas (`physical_keys`, das janelas e do socket de controle), e este módulo é dele: a leitura é de estrutura privada (decisão 234).
pub(in crate::game) fn description_key_down(game: &Game<'_>) -> bool {
    game.has_shift_down() && game.physical_keys.contains(&crate::game::KeyCode::KeyN)
}

/// `MekanismDataComponents.registerComponentAppenderAfterAll(DETAILS, ...)` e `registerComponentAppenderAfter(DESCRIPTION, ...)`: sem a tecla de detalhes a dica `Hold Left Shift for details.`
/// (`GRAY` com a tecla em `INDIGO`); depois dela, com Shift mais N a descrição do bloco (`description.<namespace>.<bloco>`, o componente sem estilo) e sem eles a dica
/// `Hold SHIFT + N for a description.` (`GRAY`, a tecla em `AQUA`).
pub(in crate::game) fn content_hold_lines(game: &Game<'_>, details: bool, description_key: &str) -> Vec<(horizon_client::mod_api::Text, u32)> {
    use horizon_client::mod_api::Text;
    use machinery_basis_core::tooltip::{HoldLine, hold_lines};
    let hint = |key: &str, key_name: &str, color: u32| {
        let template = horizon_client::lang::get(key).to_string();
        let (head, tail) = template.split_once("%1$s").unwrap_or((template.as_str(), ""));
        (Text::colored(head, CONTENT_GRAY).append(Text::colored(key_name, color)).append(Text::colored(tail, CONTENT_GRAY)), u32::MAX)
    };
    hold_lines(true, true, details, description_key_down(game))
        .into_iter()
        .flat_map(|line| match line {
            HoldLine::DetailsHint => vec![hint("tooltip.mekanism.hold_for_details", "Left Shift", CONTENT_INDIGO)],
            HoldLine::DescriptionHint => vec![hint("tooltip.mekanism.hold_for_description", "SHIFT + N", CONTENT_AQUA)],
            HoldLine::Description => description_lines(game, description_key),
        })
        .collect()
}

/// A descrição do item (`DESCRIPTION`) cortada como o NeoForge corta o tooltip de um item (`ClientHooks.gatherTooltipComponentsFromElements`): o Horizon desenha as linhas como o
/// mod as entrega, e a descrição é a linha mais larga, então o corte (`tooltip::item_wrap_width`, pelo cursor e pela largura da tela) vale para ela. O cursor de `Game` já está em pixels de GUI.
fn description_lines(game: &Game<'_>, key: &str) -> Vec<(horizon_client::mod_api::Text, u32)> {
    use horizon_client::mod_api::Text;
    let whole = vec![(Text::translate(key, &[]), u32::MAX)];
    let Some(renderer) = game.renderer.as_ref() else { return whole };
    let screen = HudBatch::new([renderer.config.width, renderer.config.height]).width as i32;
    let text = horizon_client::lang::get(key).to_string();
    let width = |piece: &str| game.native_text_width(piece);
    let Some(limit) = tooltip::item_wrap_width(width(&text).ceil() as i32, game.cursor.0 as i32, screen) else { return whole };
    tooltip::split_lines(&text, limit as f32, &width).into_iter().map(|line| (Text::literal(line.as_str()), u32::MAX)).collect()
}

/// A capacidade de energia que o tooltip de uma máquina mostra: `UpgradeAware.getUpgradeCount(energy).getMaxEnergy(def)` (`ItemBlockTooltip`), a padrão `def` vezes o multiplicador máximo
/// elevado à fração de upgrades de energia (`upgrade_id`) que o item leva. Sem o upgrade instalado é a padrão.
pub(in crate::game) fn content_max_energy(game: &Game<'_>, stack: &ItemStack, def: i64, upgrade_id: &str) -> i64 {
    let Ok(Some(Tag::Compound(component))) = game.native_component(stack, CONTENT_UPGRADES) else {
        return def;
    };
    let Some(Tag::Compound(totals)) = component.get(&JavaString::from(UPGRADES_KEY)) else {
        return def;
    };
    let Some(Tag::Int(stored)) = totals.get(&JavaString::from(upgrade_id)) else {
        return def;
    };
    let types = upgrade::current_types();
    let Some(kind) = types.get(upgrade_id) else {
        return def;
    };
    content::upgraded_max_energy(def, *stored, kind.max, machinery_basis_core::settings::max_upgrade_multiplier_f64())
}

/// `FactoryType.addToTooltip` (o `FACTORY_TYPE` do item, `MekanismBlocks.java:285` e vizinhas): `Recipe type: Enriching`, `FACTORY_TYPE.translateColored(INDIGO, GRAY, this)`. O `factory`
/// é o nome do tipo (`enriching`, `crushing`, `smelting`, `infusing`).
pub(in crate::game) fn content_recipe_type_line(factory: &str) -> (horizon_client::mod_api::Text, u32) {
    use horizon_client::mod_api::Text;
    let kind = Text::translate(&format!("factory.mekanism.{factory}"), &[]).with_style(&horizon_client::text::Style::color(CONTENT_GRAY));
    (Text::translate("factory.mekanism.type", &[kind]).with_style(&horizon_client::text::Style::color(CONTENT_INDIGO)), u32::MAX)
}

/// `EnumColor.GRAY`, `ORANGE`, `PINK`, `AQUA` e `DARK_RED`.
const CONTENT_GRAY: u32 = 0xCF_CF_CF;
const CONTENT_ORANGE: u32 = 0xFF_A1_60;
const CONTENT_PINK: u32 = 0xD5_5E_CB;
const CONTENT_AQUA: u32 = 0x30_FF_F9;
const CONTENT_DARK_RED: u32 = 0xC9_07_1F;
const CONTENT_INDIGO: u32 = 0x55_9E_FF;

/// As linhas dos componentes `ATTACHED_CHEMICALS`, `ATTACHED_FLUIDS` e `ATTACHED_ITEMS` do item de uma máquina (`ResourceContainerType.addStoredResource` e `HAS_INVENTORY`), depois da
/// energia: o químico ou o fluido guardado (`Biomass: 150 mB`) ou o aviso de vazio, e `Inventory: yes|no`. `chemicals` e `fluids` dizem se o item guarda tanque desse tipo.
pub(in crate::game) fn content_stored_lines(
    game: &Game<'_>,
    stack: &ItemStack,
    chemicals: bool,
    fluids: bool,
) -> Vec<(horizon_client::mod_api::Text, u32)> {
    use horizon_client::mod_api::Text;
    let stored_of = |id: &str| -> Option<(String, i64)> {
        let Ok(Some(Tag::Compound(component))) = game.native_component(stack, id) else {
            return None;
        };
        let Some(Tag::List(list)) = component.get(&JavaString::from("containers")) else {
            return None;
        };
        list.iter().find_map(|stored| {
            let Tag::Compound(stored) = stored else {
                return None;
            };
            let amount = match stored.get(&JavaString::from(AMOUNT_KEY)) {
                Some(Tag::Long(amount)) => *amount,
                Some(Tag::Int(amount)) => i64::from(*amount),
                _ => return None,
            };
            let name = match stored.get(&JavaString::from(TYPE_KEY)) {
                Some(Tag::String(id)) => id.to_string_lossy(),
                Some(Tag::Compound(kind)) => match kind.get(&JavaString::from(ITEM_ID_KEY)) {
                    Some(Tag::String(id)) => id.to_string_lossy(),
                    _ => return None,
                },
                _ => return None,
            };
            (amount > 0).then_some((name, amount))
        })
    };
    let mut lines = Vec::new();
    let stored_line = |name_key: String, amount: i64, color: u32| {
        let text = Text::translate("generic.mekanism.stored.mb", &[Text::translate(&name_key, &[]), Text::colored(thousands(amount), CONTENT_GRAY)]);
        (text.with_style(&horizon_client::text::Style::color(color)), u32::MAX)
    };
    let empty_line = |key: &str| (Text::translate(key, &[]).with_style(&horizon_client::text::Style::color(CONTENT_DARK_RED)), u32::MAX);
    if chemicals {
        match stored_of(CONTENT_CHEMICALS) {
            Some((id, amount)) => lines.push(stored_line(content_name_key("chemical", &id), amount, CONTENT_ORANGE)),
            None => lines.push(empty_line("tooltip.mekanism.no_chemical")),
        }
    }
    if fluids {
        match stored_of(CONTENT_FLUIDS) {
            Some((id, amount)) => lines.push(stored_line(content_name_key("block", &id), amount, CONTENT_PINK)),
            None => lines.push(empty_line("tooltip.mekanism.no_fluid")),
        }
    }
    let items = game.native_component(stack, CONTENT_ITEMS).ok().flatten().map(tag_value);
    let answer = if content::has_inventory(items.as_ref()) { "boolean.mekanism.yes" } else { "boolean.mekanism.no" };
    let line = Text::translate("tooltip.mekanism.inventory", &[Text::translate(answer, &[]).with_style(&horizon_client::text::Style::color(CONTENT_GRAY))]);
    lines.push((line.with_style(&horizon_client::text::Style::color(CONTENT_AQUA)), u32::MAX));
    lines
}

/// A chave de tradução de um id (`upgrade.mekanism.speed`, `chemical.mekanism.bio`, `block.minecraft.lava`).
fn content_name_key(kind: &str, id: &str) -> String {
    let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", id));
    format!("{kind}.{namespace}.{}", path.replace('/', "."))
}

/// `TextUtils.format(long)`: o número com o separador de milhar (`1,500`).
fn thousands(amount: i64) -> String {
    let digits = amount.abs().to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    if amount < 0 { format!("-{out}") } else { out }
}

// ---------------------------------------------------------------------------------------------------------------------------
// Os eventos do jogo que a rede de transmissores recebe (frente `rede-ganchos`, Fase 3). Um tipo de block entity se registra como transmissor com
// `transmitters_register(id, sink)` e o `sink` recebe, na ordem do original, `clearRemoved`, `onAdded`, `onChunkUnloaded`, `setRemoved`, o tick, o aviso do
// vizinho, a capability invalidada, o ticket do chunk e o `ServerTickEvent.Post`. A lib não conhece nenhum id. Os eventos saem da fila do módulo
// `transmitter_events.rs` (`machinery_transmitters`); veja o cabeçalho dele para o que cada um é e para o que fica diferente do original.
// ---------------------------------------------------------------------------------------------------------------------------

use horizon_client::block_entities::machinery_transmitters as transmitters;

pub use transmitters::{Event as TransmitterEvent, Kind as TransmitterKind, PendingClears};

/// Quem recebe os eventos: roda com o jogo no nível do evento.
pub type TransmitterSink = fn(&mut Game<'_>, &TransmitterEvent);

static TRANSMITTER_SINKS: Mutex<Vec<(&'static str, TransmitterSink)>> = Mutex::new(Vec::new());
/// O rastro dos eventos no `stderr` (`MBT {json}`), para comparar com `transmitter_events_*.jsonl`. O repositório o deixa desligado; `enviar_fontes.sh` o liga na cópia de teste.
const TRANSMITTER_PROBE: bool = false;
/// A sonda da troca de vizinho (frente `troca-vizinho`): a ordem dos avisos `block_changed`, `Neighbor` e `Capability`, e cada vez que um transmissor publica os braços (`MBS {json}`), sem o caldeirão criativo da sonda dos eventos. `enviar_fontes.sh WS PASTA troca` a liga na cópia de teste.
const SWAP_PROBE: bool = false;
/// A sonda leve dos setups reais (decisão 196): só a passada (`pre`), a fila de chunks (`MBC`) e a rede de cada família por tick (`MBE`, `MBP`, `MTN`, `MBQ`), sem uma linha por evento de block entity. A
/// sonda dos eventos (`TRANSMITTER_PROBE`) escreve milhares de linhas por passada e atrasa o servidor a um quarto da velocidade: o carregamento dos chunks corre em tempo real, e a comparação por tick
/// com o original só vale com o servidor andando a 20 por segundo. `enviar_fontes.sh` liga esta (modo `setups`) e deixa a dos eventos desligada.
const SETUPS_PROBE: bool = false;
/// O perfil por etapa do tick dos transmissores (frente `rede-desempenho`, decisão 202): o tempo e a contagem de cada etapa, somados e escritos no `stderr` (`MBF {json}`) a cada `PERF_WINDOW`
/// passadas do servidor. O repositório o deixa desligado (`PERF_PROBE = false` faz de `perf_span` um valor sem relógio); `enviar_fontes.sh WS PASTA perfil` o liga na cópia de teste. Não há
/// `perf` no container (`perf_event_paranoid` 3), então a medida é por marcas de tempo.
const PERF_PROBE: bool = false;
/// De quantas em quantas passadas do servidor o perfil sai.
const PERF_WINDOW: i64 = 40;
/// Os sinks que o perfil separa por dono (a ordem de registro); os de índice maior caem no último.
const PERF_SINKS: usize = 6;
/// As etapas do perfil, na ordem dos índices `PERF_*`.
const PERF_NAMES: [&str; 53] = [
    "tick", "start_flush", "start_client_flush", "upd_all", "upd_transporter_client", "upd_flush", "upd_track", "upd_deliver", "end_idle", "end_flush", "end_post", "end_packets",
    "sink0_upd", "sink1_upd", "sink2_upd", "sink3_upd", "sink4_upd", "sink5_upd",
    "sink0_post", "sink1_post", "sink2_post", "sink3_post", "sink4_post", "sink5_post",
    "sink0_other", "sink1_other", "sink2_other", "sink3_other", "sink4_other", "sink5_other",
    "res_retry", "res_refresh", "res_handle", "res_flush", "res_post_pre", "cab_retry", "cab_refresh", "cab_inboxes", "cab_handle", "cab_flush",
    "heat_retry", "heat_handle", "tra_retry", "tra_refresh", "tra_handle", "tra_flush", "res_post_post", "tra_offered", "tra_mirror", "hook_mek", "hook_gen", "tra_slots", "tra_refuses",
];
pub(in crate::game) const PERF_TICK: usize = 0;
const PERF_START_FLUSH: usize = 1;
const PERF_START_CLIENT_FLUSH: usize = 2;
const PERF_UPD_ALL: usize = 3;
const PERF_UPD_TRANSPORTER_CLIENT: usize = 4;
const PERF_UPD_FLUSH: usize = 5;
const PERF_UPD_TRACK: usize = 6;
const PERF_UPD_DELIVER: usize = 7;
const PERF_END_IDLE: usize = 8;
const PERF_END_FLUSH: usize = 9;
const PERF_END_POST: usize = 10;
const PERF_END_PACKETS: usize = 11;
const PERF_SINK_UPD: usize = 12;
const PERF_SINK_POST: usize = 18;
const PERF_SINK_OTHER: usize = 24;
pub(in crate::game) const PERF_RES_RETRY: usize = 30;
pub(in crate::game) const PERF_RES_REFRESH: usize = 31;
pub(in crate::game) const PERF_RES_HANDLE: usize = 32;
pub(in crate::game) const PERF_RES_FLUSH: usize = 33;
pub(in crate::game) const PERF_RES_POST_PRE: usize = 34;
const PERF_CAB_RETRY: usize = 35;
const PERF_CAB_REFRESH: usize = 36;
const PERF_CAB_INBOXES: usize = 37;
const PERF_CAB_HANDLE: usize = 38;
const PERF_CAB_FLUSH: usize = 39;
pub(in crate::game) const PERF_HEAT_RETRY: usize = 40;
pub(in crate::game) const PERF_HEAT_HANDLE: usize = 41;
pub(in crate::game) const PERF_TRA_RETRY: usize = 42;
pub(in crate::game) const PERF_TRA_REFRESH: usize = 43;
pub(in crate::game) const PERF_TRA_HANDLE: usize = 44;
pub(in crate::game) const PERF_TRA_FLUSH: usize = 45;
pub(in crate::game) const PERF_RES_POST_POST: usize = 46;
pub(in crate::game) const PERF_TRA_OFFERED: usize = 47;
pub(in crate::game) const PERF_TRA_MIRROR: usize = 48;
pub(in crate::game) const PERF_HOOK_MEK: usize = 49;
pub(in crate::game) const PERF_HOOK_GEN: usize = 50;
pub(in crate::game) const PERF_TRA_SLOTS: usize = 51;
pub(in crate::game) const PERF_TRA_REFUSES: usize = 52;
static PERF_NS: [AtomicU64; PERF_NAMES.len()] = [const { AtomicU64::new(0) }; PERF_NAMES.len()];
static PERF_COUNT: [AtomicU64; PERF_NAMES.len()] = [const { AtomicU64::new(0) }; PERF_NAMES.len()];
static PERF_MARK: Mutex<Option<Instant>> = Mutex::new(None);
/// O começo do tick do servidor em andamento (a etapa `tick` vai dele até o fim da passada).
static PERF_TICK_START: Mutex<Option<Instant>> = Mutex::new(None);

/// O fim de uma passada com transmissores: fecha a etapa `tick` e, na janela, escreve o perfil.
fn perf_tick_end() {
    if !PERF_PROBE {
        return;
    }
    if let Some(start) = PERF_TICK_START.lock().unwrap_or_else(|e| e.into_inner()).take() {
        PERF_NS[PERF_TICK].fetch_add(u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX), Ordering::Relaxed);
        PERF_COUNT[PERF_TICK].fetch_add(1, Ordering::Relaxed);
    }
    perf_report(TRANSMITTER_PASS.load(Ordering::Relaxed));
}

/// Uma etapa em andamento: o tempo entre `perf_span` e o fim do escopo entra na soma dela.
pub(in crate::game) struct PerfSpan(usize, Option<Instant>);

pub(in crate::game) fn perf_span(stage: usize) -> PerfSpan {
    PerfSpan(stage, PERF_PROBE.then(Instant::now))
}

impl Drop for PerfSpan {
    fn drop(&mut self) {
        if let Some(start) = self.1 {
            PERF_NS[self.0].fetch_add(u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX), Ordering::Relaxed);
            PERF_COUNT[self.0].fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// O fim de uma passada: a cada `PERF_WINDOW` passadas escreve a média por tick de cada etapa (ms por tick e ocorrências por tick) e o tempo de parede por tick, e zera as somas.
fn perf_report(pass: i64) {
    if !PERF_PROBE || pass % PERF_WINDOW != 0 {
        return;
    }
    let now = Instant::now();
    let wall = PERF_MARK.lock().unwrap_or_else(|e| e.into_inner()).replace(now).map(|before| now.duration_since(before).as_secs_f64() * 1000.0 / PERF_WINDOW as f64);
    let sinks: Vec<&'static str> = TRANSMITTER_SINKS.lock().unwrap_or_else(|e| e.into_inner()).iter().map(|(name, _)| *name).collect();
    let mut fields = Vec::new();
    for (index, name) in PERF_NAMES.iter().enumerate() {
        let ns = PERF_NS[index].swap(0, Ordering::Relaxed);
        let count = PERF_COUNT[index].swap(0, Ordering::Relaxed);
        if count > 0 {
            fields.push(format!("\"{name}\":[{:.3},{:.1}]", ns as f64 / 1e6 / PERF_WINDOW as f64, count as f64 / PERF_WINDOW as f64));
        }
    }
    eprintln!("MBF {{\"t\":{pass},\"wall\":{:.3},\"sinks\":{sinks:?},\"s\":{{{}}}}}", wall.unwrap_or(0.0), fields.join(","));
}

/// As passadas do `server_tick` desde que o jogo abriu (a sonda numera por elas, e não pelo `game_time`, que o `/tick freeze` para).
static TRANSMITTER_PASS: AtomicI64 = AtomicI64::new(0);
/// A sonda leve da decisão 198: o que cada chunk já deu desde que o mundo abriu (a primeira vez de `clearRemoved` e de `tick` por chunk, para saber em que passada cada um entrou), para
/// comparar com os tickets de acessibilidade (`MBK`) e com a carga do chunk (`MBL`). Só com `SETUPS_PROBE`.
static PROBE_FIRST: Mutex<Vec<(u8, i32, i32)>> = Mutex::new(Vec::new());

fn probe_first(kind: u8, name: &str, pos: [i32; 3]) {
    let (cx, cz) = (pos[0] >> 4, pos[2] >> 4);
    let mut seen = PROBE_FIRST.lock().unwrap_or_else(|e| e.into_inner());
    if !seen.contains(&(kind, cx, cz)) {
        seen.push((kind, cx, cz));
        eprintln!("MBU {{\"t\":{},\"e\":\"{name}\",\"cx\":{cx},\"cz\":{cz}}}", TRANSMITTER_PASS.load(Ordering::Relaxed));
    }
}
thread_local! {
    /// Os chunks da volta de um nível descarregado cuja espera venceu e que entram no registro no começo do tick seguinte.
    static ARRIVAL_LOADS: RefCell<Vec<(String, [i32; 2])>> = const { RefCell::new(Vec::new()) };
    /// Os níveis que o servidor já andou com alguém e ainda não soltaram os chunks (decisão 193).
    static IDLE_LEVELS: RefCell<machinery_basis_core::level_idle::IdleLevels> = RefCell::new(machinery_basis_core::level_idle::IdleLevels::new());
}

/// `id` (o `ModBlockEntity.id`) passa a ser um transmissor, e o `sink` de `owner` (um por dono, a primeira vez que o dono o diz) ouve os eventos de todos. Pode ser dito mais de uma vez.
pub fn transmitters_register(owner: &'static str, id: &str, sink: TransmitterSink) {
    transmitters::register(id);
    let mut sinks = TRANSMITTER_SINKS.lock().unwrap_or_else(|e| e.into_inner());
    if !sinks.iter().any(|(name, _)| *name == owner) {
        sinks.push((owner, sink));
    }
}

/// O registro de uma família antes do `reset`, na sonda (`invalid`, `networksToChange` e `networks` do `TransmitterNetworkRegistry`, que no original é um só para as cinco famílias).
pub fn transmitter_probe_reset(family: &str, counts: (usize, usize, usize)) {
    if TRANSMITTER_PROBE {
        eprintln!("MBT {{\"t\":{},\"e\":\"antes_do_reset\",\"familia\":\"{family}\",\"invalid\":{},\"change\":{},\"redes\":{}}}", TRANSMITTER_PASS.load(Ordering::Relaxed), counts.0, counts.1, counts.2);
    }
}

/// A cópia de teste liga e desliga as injeções da primeira passada (`owner_tickets` e `owner_level`) pelo arquivo `/tmp/mb_sem_primeira_passada`: com ele o port se comporta como antes
/// da decisão 195, e o mesmo build mede os dois lados. Fora da cópia de teste (`TRANSMITTER_PROBE` é `false`) a função é a constante `false`.
fn mb_first_pass_off() -> bool {
    TRANSMITTER_PROBE && std::path::Path::new("/tmp/mb_sem_primeira_passada").exists()
}

/// Os ticks do servidor desde o último `open_ready`, para a sonda do estado dos primeiros ticks.
static TRANSMITTER_PASSES_SINCE_OPEN: AtomicI64 = AtomicI64::new(0);

/// Uma marca da sonda (a abertura do mundo, a primeira passada), com o número da passada do servidor.
pub fn transmitter_probe_note(name: &str) {
    if TRANSMITTER_PROBE || SETUPS_PROBE {
        if name == "open_ready" {
            TRANSMITTER_PASSES_SINCE_OPEN.store(0, Ordering::Relaxed);
        }
        let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
        eprintln!("MBT {{\"t\":{},\"e\":\"{name}\",\"ms\":{ms}}}", TRANSMITTER_PASS.load(Ordering::Relaxed));
    }
}

/// A sonda dos eventos está ligada nesta cópia?
pub fn transmitter_probe_enabled() -> bool {
    TRANSMITTER_PROBE
}

/// A sonda leve de cada rede por tick (`TRANSMITTER_PROBE` ou `SETUPS_PROBE`) está ligada nesta cópia?
pub fn setups_probe_enabled() -> bool {
    TRANSMITTER_PROBE || SETUPS_PROBE
}

fn transmitter_probe(game: &Game<'_>, event: &TransmitterEvent) {
    let pass = TRANSMITTER_PASS.load(Ordering::Relaxed);
    let line = match event {
        TransmitterEvent::Transmitter { dim, pos, kind } => {
            let name = match kind {
                TransmitterKind::ClearRemoved => "clearRemoved".to_owned(),
                TransmitterKind::PlacedByItem => "onAdded".to_owned(),
                TransmitterKind::ChunkUnloaded => "onChunkUnloaded".to_owned(),
                TransmitterKind::Removed => "setRemoved".to_owned(),
                TransmitterKind::Update => "tick".to_owned(),
                TransmitterKind::Neighbor => "neighbor".to_owned(),
                TransmitterKind::Capability(side) => format!("cap\",\"side\":\"{}", transmitters::SIDE_NAMES[*side]),
            };
            format!("{{\"t\":{pass},\"e\":\"{name}\",\"dim\":\"{dim}\",\"p\":[{},{},{}]}}", pos[0], pos[1], pos[2])
        }
        TransmitterEvent::Ticket { dim, chunk, loaded } => {
            format!("{{\"t\":{pass},\"e\":\"ticket\",\"dim\":\"{dim}\",\"cx\":{},\"cz\":{},\"loaded\":{loaded}}}", chunk[0], chunk[1])
        }
        TransmitterEvent::ServerTickPost { runs_normally } => format!("{{\"t\":{pass},\"e\":\"post\",\"runs\":{runs_normally}}}"),
        TransmitterEvent::Reset => format!("{{\"t\":{pass},\"e\":\"reset\"}}"),
    };
    let _ = game;
    eprintln!("MBT {line}");
}

fn transmitters_deliver(game: &mut Game<'_>, events: Vec<TransmitterEvent>) {
    if events.is_empty() {
        return;
    }
    let sinks: Vec<TransmitterSink> = TRANSMITTER_SINKS.lock().unwrap_or_else(|e| e.into_inner()).iter().map(|(_, sink)| *sink).collect();
    for event in &events {
        if SWAP_PROBE && let TransmitterEvent::Transmitter { pos, kind, .. } = event && !matches!(kind, TransmitterKind::Update) {
            eprintln!("MBS {{\"e\":\"deliver\",\"kind\":\"{kind:?}\",\"p\":[{},{},{}]}}", pos[0], pos[1], pos[2]);
        }
        if TRANSMITTER_PROBE {
            transmitter_probe(game, event);
        } else if SETUPS_PROBE && let TransmitterEvent::Transmitter { pos, kind, .. } = event {
            match kind {
                TransmitterKind::ClearRemoved => probe_first(0, "clearRemoved", *pos),
                TransmitterKind::Update => probe_first(1, "tick", *pos),
                _ => {}
            }
        } else if SETUPS_PROBE && let TransmitterEvent::Ticket { chunk, loaded, .. } = event {
            // Os tickets de acessibilidade (decisão 196), as únicas ocorrências que a sonda leve conta uma a uma: o chunk, o lado do anel e a passada.
            eprintln!("MBK {{\"t\":{},\"cx\":{},\"cz\":{},\"loaded\":{loaded}}}", TRANSMITTER_PASS.load(Ordering::Relaxed), chunk[0], chunk[1]);
        }
        let class = match event {
            TransmitterEvent::Transmitter { kind: TransmitterKind::Update, .. } => PERF_SINK_UPD,
            TransmitterEvent::ServerTickPost { .. } => PERF_SINK_POST,
            _ => PERF_SINK_OTHER,
        };
        for (index, sink) in sinks.iter().enumerate() {
            let _span = perf_span(class + index.min(PERF_SINKS - 1));
            sink(game, event);
        }
    }
}

/// A ordem dos block entities de cada chunk como o `Object2ObjectOpenHashMap` do original a tem (todos os tipos, na ordem em que entraram e saíram): o chunk que sai percorre os de mod
/// nessa ordem, e contar só os de mod erra quando o chunk tem um baú ou um funil (medido em `transmitter_events_chunks_mixed.jsonl`, decisão 187).
static BLOCK_ENTITY_ORDER: LazyLock<Mutex<machinery_basis_core::sim::block_entity_order::BlockEntityOrder>> = LazyLock::new(Default::default);

/// O que a fila anotou (na ordem) entra no mapa de cada chunk: `put` quando o block entity entra, `remove` quando sai, um mapa novo quando o chunk carrega.
fn transmitters_note_order(dimension: &str, raws: &[transmitters::Raw]) {
    let mut order = BLOCK_ENTITY_ORDER.lock().unwrap_or_else(|e| e.into_inner());
    for raw in raws {
        match raw {
            transmitters::Raw::Added { pos, .. } | transmitters::Raw::AddedLater { pos } => order.put(dimension, sim_pos(*pos)),
            transmitters::Raw::Removed { pos, .. } => order.remove(dimension, sim_pos(*pos)),
            transmitters::Raw::LoadChunk(blocks) => {
                if let Some((first, _)) = blocks.first() {
                    let chunk = (first[0] >> 4, first[2] >> 4);
                    // O chunk que o original segurou volta com o mesmo mapa (os mesmos block entities, na mesma ordem): o mapa dele não recomeça (decisão 199).
                    if !transmitters::is_held(dimension, [chunk.0, chunk.1]) {
                        order.load(dimension, chunk, blocks.iter().map(|(pos, _)| sim_pos(*pos)));
                    }
                }
            }
            transmitters::Raw::Neighbor { .. } | transmitters::Raw::BlockChanged { .. } => {}
        }
    }
}

/// `get_or_insert_with` anota só a posição (o block entity nasce de uma closure): o tipo vem do que está lá agora (`transmitters::resolve`).
fn transmitters_resolve(game: &Game<'_>, raws: Vec<transmitters::Raw>) -> Vec<transmitters::Raw> {
    let entities = &game.world.block_entities;
    transmitters::resolve(raws, |pos| match entities.get(&(pos[0], pos[1], pos[2])) {
        Some(horizon_client::containers::BlockEntity::Mod(entity)) => Some(Some(entity.id.clone())),
        Some(_) => Some(None),
        None => None,
    })
}

/// Esvazia a fila de ocorrências do nível em que o jogo está, tudo: os pontos do tick, o `chunk_load` e os ganchos. Um cliente que só recebe o mundo do servidor não a tem.
pub(in crate::game) fn transmitters_flush(game: &mut Game<'_>) {
    if !transmitters::is_active() {
        return;
    }
    // Um cliente que só recebe o mundo do servidor não tem dono: a fila é do servidor, e o cliente não a esvazia (levava o `load_chunk` que o servidor acabara de anotar, decisão 198).
    if game.remote.is_some() {
        probe_remote_flush("tick");
        return;
    }
    // O block entity criado depois do aviso que a operação segurou vem antes dele.
    let (mut raws, neighbors) = transmitters::split_neighbors(transmitters::take());
    raws.extend(transmitters::take_held());
    raws.extend(neighbors);
    if !raws.is_empty() {
        let raws = transmitters_resolve(game, raws);
        transmitters_note_order(game.native_dimension(), &raws);
        let events = transmitters::translate(game.native_dimension(), raws);
        transmitters_deliver(game, events);
    }
    // Os chunks que carregaram antes do registro do primeiro tipo: o `chunk_loaded` deles sai quando o chunk está inteiro no jogo, com os block entities já na tabela de carga.
    for chunk in transmitters::take_early_chunks() {
        if game.world.chunks.contains_key(&(chunk[0], chunk[1])) {
            let accessible = chunk_accessible(game, game.native_dimension(), chunk);
            let events = transmitters::chunk_loaded_with(game.native_dimension(), chunk, accessible);
            probe_chunk_load(chunk, accessible, &events);
            transmitters_deliver(game, events);
        } else {
            transmitters::keep_early_chunk(chunk);
        }
    }
}

/// O fim de uma operação no mundo (`flush_signals`): o resto sai, depois os avisos de vizinho da operação anterior, e os desta ficam para a seguinte. O original cria o block entity
/// dentro do `setBlock` e avisa o vizinho depois; o Horizon avisa dentro do `setBlock` e cria o block entity de um item depois (o `place_block`), e o aviso espera por ele.
pub(in crate::game) fn transmitters_flush_operation(game: &mut Game<'_>) {
    if !transmitters::is_active() {
        return;
    }
    if game.remote.is_some() {
        probe_remote_flush("operation");
    }
    let raws = transmitters::take_for(game.remote.is_none());
    if raws.is_empty() {
        return;
    }
    let (others, neighbors) = transmitters::split_neighbors(raws);
    if others.is_empty() {
        // Só avisos: esperam o block entity que a operação ainda vai criar, ou o próximo ponto do tick.
        transmitters::hold(neighbors);
        return;
    }
    // O block entity que o `setBlock` criou depois do aviso (o Horizon o insere depois do `with_level`) entra antes do aviso da operação anterior, como no original.
    let mut now = others;
    now.extend(transmitters::take_held());
    let now = transmitters_resolve(game, now);
    transmitters_note_order(game.native_dimension(), &now);
    let events = transmitters::translate(game.native_dimension(), now);
    transmitters::hold(neighbors);
    transmitters_deliver(game, events);
}

/// O aviso de vizinho do bloco em `pos` (o gancho `neighbor_changed`): anota a ocorrência, que a fila entrega na ordem.
pub(in crate::game) fn transmitters_neighbor(dimension: &'static str, pos: BlockPos) {
    if SWAP_PROBE {
        eprintln!("MBS {{\"e\":\"neighbor_hook\",\"p\":[{},{},{}]}}", pos.x, pos.y, pos.z);
    }
    transmitters::push(transmitters::Raw::Neighbor { dim: dimension.to_owned(), pos: [pos.x, pos.y, pos.z] });
}

/// `Level.setBlock` trocou o bloco em `pos` por outro (o gancho `block_changed`): a capability da posição foi invalidada, e o transmissor vizinho a lê de novo.
pub(in crate::game) fn transmitters_block_changed(level: &GameLevel<'_>, pos: BlockPos, old: BlockStateId, new: BlockStateId) {
    if SWAP_PROBE {
        eprintln!("MBS {{\"e\":\"block_changed_hook\",\"p\":[{},{},{}],\"old\":{:?},\"new\":{:?}}}", pos.x, pos.y, pos.z, level.native_state_block(old), level.native_state_block(new));
    }
    if level.native_state_block(old) != level.native_state_block(new) {
        transmitters::push(transmitters::Raw::BlockChanged { pos: [pos.x, pos.y, pos.z] });
    }
}

/// O ticker de um block entity de mod (o gancho `block_entity_tick`): o `onUpdateServer` dos transmissores.
pub(in crate::game) fn transmitters_update(game: &mut Game<'_>, pos: [i32; 3], id: &str) {
    // `TileEntityTransmitter.tickClient`: o transportador do cliente (a janela do anfitrião e o convidado) avança as pilhas dele.
    {
        let _span = perf_span(PERF_UPD_TRANSPORTER_CLIENT);
        crate::game::machinery_transporters::transporter_client_tick(game, pos, id);
    }
    if !transmitters::is_active() || !transmitters::is_registered(id) {
        if TRANSMITTER_PROBE && transmitters::is_active() && (id.contains("cable") || id.contains("pipe")) {
            eprintln!("MBT {{\"e\":\"drop_unregistered\",\"id\":{id:?},\"p\":{pos:?}}}");
        }
        return;
    }
    let _all = perf_span(PERF_UPD_ALL);
    {
        let _span = perf_span(PERF_UPD_FLUSH);
        transmitters_flush(game);
    }
    let events = {
        let _span = perf_span(PERF_UPD_TRACK);
        transmitters::update(game.native_dimension(), pos)
    };
    if TRANSMITTER_PROBE && events.is_empty() {
        eprintln!("MBT {{\"e\":\"drop_unwatched\",\"id\":{id:?},\"p\":{pos:?}}}");
    }
    let _span = perf_span(PERF_UPD_DELIVER);
    transmitters_deliver(game, events);
}

/// Um item pôs o bloco em `pos` (o gancho `place_block`): o `onAdded`, depois do que a fila tem (o block entity que o item criou) e antes do aviso dos vizinhos que a operação segurou.
pub(in crate::game) fn transmitters_placed(game: &mut Game<'_>, pos: [i32; 3]) {
    if !transmitters::is_active() || game.remote.is_some() {
        return;
    }
    let raws = transmitters::take();
    let held = transmitters::take_held();
    let dimension = game.native_dimension();
    let raws = transmitters_resolve(game, raws);
    transmitters_note_order(dimension, &raws);
    let events = transmitters::translate(dimension, raws);
    transmitters_deliver(game, events);
    let events = transmitters::placed(dimension, pos);
    transmitters_deliver(game, events);
    let events = transmitters::translate(dimension, held);
    transmitters_deliver(game, events);
}

/// `ChunkEvent.Load` (o gancho `chunk_load`).
pub(in crate::game) fn transmitters_chunk_loaded(game: &mut Game<'_>, dimension: &str, chunk: [i32; 2]) {
    if !transmitters::is_active() || game.remote.is_some() {
        return;
    }
    transmitters_flush(game);
    // O nível que acabou de descarregar e voltou refaz as redes dois ticks depois do `tp` no original, e o port já o faria um antes: a entrega espera o fim do tick seguinte (decisão 218).
    if IDLE_LEVELS.with_borrow_mut(|levels| levels.delay_arrival_load(dimension, chunk)) {
        return;
    }
    transmitters_chunk_loaded_now(game, dimension, chunk);
}

/// A entrega do `ChunkEvent.Load` de um chunk, sem espera.
fn transmitters_chunk_loaded_now(game: &mut Game<'_>, dimension: &str, chunk: [i32; 2]) {
    let accessible = chunk_accessible(game, dimension, chunk);
    let events = transmitters::chunk_loaded_with(dimension, chunk, accessible);
    probe_chunk_load(chunk, accessible, &events);
    transmitters_deliver(game, events);
}

/// A sonda leve do cliente que passa por um ponto que esvazia a fila (`MBD`): o que ele levaria, se a esvaziasse (decisão 198).
fn probe_remote_flush(point: &str) {
    if SETUPS_PROBE {
        let (queued, loads) = transmitters::queue_load();
        if queued > 0 {
            eprintln!("MBD {{\"remote_would_eat\":{queued},\"loads\":{loads},\"at\":\"{point}\"}}");
        }
    }
}

/// A sonda leve da carga de um chunk (`MBL`): o lado do anel em que ele carregou e quantos `clearRemoved` o `ChunkEvent.Load` trouxe.
fn probe_chunk_load(chunk: [i32; 2], accessible: bool, events: &[TransmitterEvent]) {
    if SETUPS_PROBE {
        let clears = events.iter().filter(|event| matches!(event, TransmitterEvent::Transmitter { kind: TransmitterKind::ClearRemoved, .. })).count();
        eprintln!("MBL {{\"t\":{},\"cx\":{},\"cz\":{},\"acc\":{accessible},\"clears\":{clears}}}", TRANSMITTER_PASS.load(Ordering::Relaxed), chunk[0], chunk[1]);
    }
}

/// O chunk está no anel acessível do original (`ChunkTicketLevelUpdatedEvent`: o nível de ticket de carga é 32 ou menos, `FullChunkStatus.BLOCK_TICKING` ou mais)? O Horizon dá o nível de
/// carga dos chunks do nível em que está (`tickets.levels().loaded_ticking`); de outro nível não sabe, e o chunk conta como dentro.
fn chunk_accessible(game: &Game<'_>, dimension: &str, chunk: [i32; 2]) -> bool {
    dimension != game.native_dimension() || game.tickets.levels().loaded_ticking(chunk[0], chunk[1])
}

/// O chunk está num nível que o original segura (decisão 199)? Os tickets de carga do nível no lugar (`ChunkTickets::mb_sources`, o que `update_chunk_tickets` montou) dão o nível de ticket do
/// chunk como o `DistanceManager` o daria, sem o teto de 33 do `ticket_levels` do Horizon; até `RETAINED_LEVEL` o `LevelChunk` segue na memória do original, com os block entities. De outro nível
/// que o do lugar os tickets não valem, e o chunk não é segurado.
fn chunk_retained(game: &Game<'_>, dimension: &str, chunk: [i32; 2]) -> bool {
    if dimension != game.native_dimension() || RELEASE_ALL.load(Ordering::Relaxed) || mb_hold_off() {
        return false;
    }
    let Some(sources) = game.tickets.mb_sources() else {
        return false;
    };
    let tickets = transmitters::LoadingTickets {
        players: sources.players.iter().map(|&(x, z)| [x, z]).collect(),
        forced: sources.forced.iter().map(|&(x, z)| [x, z]).collect(),
        portal: sources.portal.iter().map(|&(x, z)| [x, z]).collect(),
        dragon: sources.dragon.map(|(x, z)| [x, z]),
        view_distance: sources.view_distance,
    };
    tickets.retains(chunk)
}

/// Só na cópia de teste (`SETUPS_PROBE`): o arquivo `/tmp/mb_sem_retencao` desliga o que a decisão 199 segura, para medir o mesmo build com e sem ele. Fora da cópia de teste é a constante `false`.
fn mb_hold_off() -> bool {
    SETUPS_PROBE && std::path::Path::new("/tmp/mb_sem_retencao").exists()
}

/// O nível está soltando todos os chunks (o servidor fecha, ou o nível que o jogador deixou descarrega): nada fica segurado.
static RELEASE_ALL: AtomicBool = AtomicBool::new(false);

/// O chunk vai sair (o gancho `chunk_unload`, que o jogo chama com os block entities ainda no lugar): os block entities de mod do chunk, na ordem do mapa dele. Se o original ainda segurasse o chunk
/// (`chunk_retained`), os transmissores dele ficam como estão (decisão 199) e a saída de verdade espera `transmitters_release_held`.
pub(in crate::game) fn transmitters_chunk_unloading(game: &mut Game<'_>, dimension: &str, chunk: [i32; 2]) {
    if !transmitters::is_active() || game.remote.is_some() {
        return;
    }
    transmitters_flush(game);
    let hold = chunk_retained(game, dimension, chunk);
    let entities = &game.world.block_entities;
    // Todos os block entities do chunk, de qualquer tipo, na ordem do mapa do original (o mapa guarda a história de cada um).
    let present: Vec<machinery_basis_core::sim::chunks::BlockPos> = entities
        .iter()
        .filter(|(pos, _)| transmitters::chunk_of([pos.0, pos.1, pos.2]) == chunk)
        .map(|(pos, _)| sim_pos([pos.0, pos.1, pos.2]))
        .collect();
    let ordered = {
        let mut order = BLOCK_ENTITY_ORDER.lock().unwrap_or_else(|e| e.into_inner());
        if hold { order.peek(dimension, (chunk[0], chunk[1]), &present) } else { order.unload(dimension, (chunk[0], chunk[1]), &present) }
    };
    let blocks: Vec<([i32; 3], Option<String>)> = ordered
        .into_iter()
        .map(|pos| {
            let id = match entities.get(&(pos.x, pos.y, pos.z)) {
                Some(horizon_client::containers::BlockEntity::Mod(entity)) => Some(entity.id.clone()),
                _ => None,
            };
            ([pos.x, pos.y, pos.z], id)
        })
        .collect();
    let events = transmitters::chunk_unloading_with(dimension, chunk, &blocks, hold);
    if SETUPS_PROBE && hold {
        eprintln!("MBH {{\"t\":{},\"cx\":{},\"cz\":{},\"hold\":true}}", TRANSMITTER_PASS.load(Ordering::Relaxed), chunk[0], chunk[1]);
    }
    transmitters_deliver(game, events);
}

/// O original solta os chunks segurados que já não segura (o nível de ticket passou de `RETAINED_LEVEL`: o `ChunkMap.processUnloads` do `ChunkHolder` que foi para `toDrop`): a saída de verdade deles,
/// o `ChunkEvent.Unload` e o `onChunkUnloaded` e `setRemoved` de cada transmissor, no fim do tick do nível, antes do `Post`. Só os do nível no lugar; `all` solta todos os da dimensão (o nível
/// descarrega, ou o servidor fecha).
pub(in crate::game) fn transmitters_release_held(game: &mut Game<'_>, dimension: &str, all: bool) {
    if !transmitters::is_active() || game.remote.is_some() {
        return;
    }
    let (released, events) = transmitters::release_held(dimension, |chunk| !all && chunk_retained(game, dimension, chunk));
    {
        let mut order = BLOCK_ENTITY_ORDER.lock().unwrap_or_else(|e| e.into_inner());
        for chunk in &released {
            order.forget(dimension, (chunk[0], chunk[1]));
        }
    }
    transmitters_deliver(game, events);
}

/// O `server_tick`: a numeração das passadas (a sonda), a fila no começo do tick e o `ServerTickEvent.Post` no fim, com o `runsNormally`.
pub(in crate::game) fn transmitters_tick(game: &mut Game<'_>, phase: horizon_client::mod_api::ServerTickPhase) {
    use horizon_client::mod_api::ServerTickPhase;
    match phase {
        ServerTickPhase::ServerTickStart { .. } => {
            // O `BlockEntities` do servidor se apresenta: o do espelho do cliente da janela (outro objeto, que recebe os blocos do servidor) deixa de entrar na fila.
            if game.remote.is_none() {
                transmitters::set_server_source(&game.world.block_entities as *const _ as usize);
            }
            TRANSMITTER_PASS.fetch_add(1, Ordering::Relaxed);
            if (TRANSMITTER_PROBE || SETUPS_PROBE) && transmitters::is_active() {
                eprintln!("MBT {{\"t\":{},\"e\":\"pre\"}}", TRANSMITTER_PASS.load(Ordering::Relaxed));
            }
            *PERF_TICK_START.lock().unwrap_or_else(|e| e.into_inner()) = PERF_PROBE.then(Instant::now);
            {
                let _span = perf_span(PERF_START_FLUSH);
                transmitters_flush(game);
                game.mb_deliver_arrival_loads();
            }
            // O `sendUpdatePacket` do tick anterior sai agora, com o estado de agora (a transmissão de mudanças dos chunks).
            let _span = perf_span(PERF_START_CLIENT_FLUSH);
            crate::game::machinery_transmitter_client::flush(game);
        }
        ServerTickPhase::ServerTickEnd { world_running } => {
            if (TRANSMITTER_PROBE || SETUPS_PROBE) && TRANSMITTER_PASSES_SINCE_OPEN.fetch_add(1, Ordering::Relaxed) < 8 {
                // Os primeiros ticks do mundo aberto (decisão 195), o que a `CenaReset` do original grava em cada um: quantos jogadores já estão, os chunks carregados e se o chunk (0, 0) anda.
                let players = game.lan.as_ref().map_or(0, |h| h.hub.guests.iter().filter(|g| g.placed).count());
                let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
                eprintln!(
                    "MBT {{\"t\":{},\"e\":\"estado\",\"ms\":{ms},\"jogadores\":{players},\"carregados\":{},\"anda00\":{},\"dim\":\"{}\"}}",
                    TRANSMITTER_PASS.load(Ordering::Relaxed),
                    game.world.chunks.len(),
                    game.tickets.levels().block_ticking(0, 0),
                    game.dim.id()
                );
            }
            if TRANSMITTER_PROBE || SETUPS_PROBE {
                // A fila de chunks da linha longa dos setups reais (decisão 196): de cada chunk (cx, 0), com cx de -2 a 30, `T` carregado e andando, `L` carregado e parado, `.` descarregado. A
                // `CenaSetups` do original grava a mesma cadeia (`isPositionTicking` e `hasChunk`), a cada tick.
                let row: String = (-2..=30)
                    .map(|cx| match (game.world.chunks.contains_key(&(cx, 0)), game.tickets.levels().block_ticking(cx, 0)) {
                        (true, true) => 'T',
                        (true, false) => 'L',
                        _ => '.',
                    })
                    .collect();
                let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
                eprintln!("MBC {{\"t\":{},\"ms\":{ms},\"carregados\":{},\"dim\":\"{}\",\"fila\":\"{row}\"}}", TRANSMITTER_PASS.load(Ordering::Relaxed), game.world.chunks.len(), game.dim.id());
            }
            // Os níveis parados que descarregam neste tick saem antes do `Post`, como no original (o `ServerLevel.tick` vem antes do `ServerTickEvent.Post`).
            let post_levels = {
                let _span = perf_span(PERF_END_IDLE);
                idle_levels_end_of_tick(game)
            };
            if !transmitters::is_active() {
                return;
            }
            {
                let _span = perf_span(PERF_END_FLUSH);
                transmitters_flush(game);
            }
            // Os chunks que o Horizon soltou e o original segurava, e que o original solta agora (o nível de ticket passou do limite): a saída de verdade, antes do `Post` do tick.
            let held_dim = game.native_dimension().to_owned();
            transmitters_release_held(game, &held_dim, false);
            // Os chunks que entraram ou saíram do anel acessível neste tick (o `ChunkTicketLevelUpdatedEvent`, que o registro anota e trata no `Post`).
            let dim = game.native_dimension().to_owned();
            let events = transmitters::accessibility(&dim, |chunk| chunk_accessible(game, &dim, chunk));
            transmitters_deliver(game, events);
            // O `onTick` do registro (um só no original) trata todas as dimensões: cada nível que o servidor ticou e que tem transmissor recebe o `ServerTickPost` dele, e os parados que
            // ainda estão carregados (o que descarrega neste tick inclusive, onde o original trata os transmissores inválidos).
            let dims = post_levels;
            {
                let _span = perf_span(PERF_END_POST);
                transmitters::deliver_server_tick_post(&mut TickLevels { game, dims }, world_running);
            }
            // Os pacotes de movimento das pilhas (`PacketTransporterSync` e `PacketTransporterBatch`) do tick saem agora.
            {
                let _span = perf_span(PERF_END_PACKETS);
                crate::game::machinery_transmitter_client::flush_packets(game);
            }
            perf_tick_end();
        }
        _ => {}
    }
}

/// Os níveis do servidor no fim do tick, para `transmitters::deliver_server_tick_post`: entrar num nível é o `enter_level` do `mb_in_levels`.
struct TickLevels<'a, 'g> {
    game: &'a mut Game<'g>,
    dims: Vec<crate::game::Dim>,
}

impl transmitters::ServerLevels for TickLevels<'_, '_> {
    fn ticked(&self) -> Vec<String> {
        self.dims.iter().map(|dim| dim.id().to_owned()).collect()
    }

    fn current(&self) -> String {
        self.game.native_dimension().to_owned()
    }

    fn enter(&mut self, dim: &str) -> bool {
        match self.dims.iter().find(|candidate| candidate.id() == dim) {
            Some(level) => self.game.enter_level(None, *level).is_ok(),
            None => false,
        }
    }

    fn deliver(&mut self, events: Vec<TransmitterEvent>) {
        transmitters_deliver(self.game, events);
    }
}

/// A soma das contagens de vários registros (um por dimensão).
pub fn sum_counts(counts: impl Iterator<Item = (usize, usize, usize)>) -> (usize, usize, usize) {
    counts.fold((0, 0, 0), |all, one| (all.0 + one.0, all.1 + one.1, all.2 + one.2))
}

/// O fim de um tick do servidor para os níveis que ninguém segura (o `mb_idle_levels_end_of_tick`), só no servidor: o cliente que recebe o mundo não tem os níveis.
fn idle_levels_end_of_tick(game: &mut Game<'_>) -> Vec<crate::game::Dim> {
    if game.remote.is_some() {
        return game.levels_to_tick();
    }
    game.mb_idle_levels_end_of_tick()
}

/// O servidor fecha: os níveis soltam os chunks antes do `ServerStoppedEvent`, e os que esperavam descarregar são esquecidos (`WorldCloseFinal`).
pub(in crate::game) fn world_closing(game: &mut Game<'_>) {
    game.mb_close_levels();
    IDLE_LEVELS.with_borrow_mut(machinery_basis_core::level_idle::IdleLevels::reset);
    ARRIVAL_LOADS.with_borrow_mut(Vec::clear);
}

/// O mundo fechou (`ServerStoppedEvent`, o `reset` do registro).
pub(in crate::game) fn transmitters_reset(game: &mut Game<'_>) {
    if !transmitters::is_active() {
        return;
    }
    BLOCK_ENTITY_ORDER.lock().unwrap_or_else(|e| e.into_inner()).clear();
    PROBE_FIRST.lock().unwrap_or_else(|e| e.into_inner()).clear();
    let events = transmitters::reset();
    transmitters_deliver(game, events);
    crate::game::machinery_transmitter_client::reset(game);
}

// ---------------------------------------------------------------------------------------------------------------------------
// Os cabos de energia (frente `cabo-jogo`, Fase 3). O dono dos transmissores (`machinery_basis_core::transmission::owner`) guarda por dimensão o registro, os
// `Transmitter` e a família de energia; esta parte o liga ao jogo: os eventos da decisão 107 entram nele, as conexões que ele calcula viram as propriedades de lado do
// bloco, a capability de energia do cabo (o contêiner da rede) responde pelo `energy_capacity`, e o NBT do cabo (`TileEntityTransmitter.saveAdditional`) sai no
// salvamento. A lib não conhece nenhum id: o mod diz quais blocos são cabos e a capacidade de cada um em `cables_register`.
// ---------------------------------------------------------------------------------------------------------------------------

use machinery_basis_core::distribution::EnergyInsert;
use machinery_basis_core::transmission::host_acceptor::{BufferSnapshot, SnapshotAcceptor};
use machinery_basis_core::transmission::owner::capability_changed;
use machinery_basis_core::energy_cube::EnergyNeighbours;
use machinery_basis_core::sim::chunks::{BlockPos as SimPos, ChunkPos};
use machinery_basis_core::transaction::Transaction;
use machinery_basis_core::transmission::connection::ConnectionType;
use machinery_basis_core::transmission::owner::{
    Acceptor, CableInfo, CableView, ENERGY_KEY, EnergyTransmitters, Event as CableEvent, OwnerWorld,
};
use machinery_basis_core::transmission::transmitter::{Compound as CableNbt, Overlay as CableOverlay, Tag as CableTag};
use machinery_basis_core::transmission::{DimensionId, Uuid};
use machinery_basis_core::transmission::reading::{AcceptorSides, TransmitterReading};
use std::cell::RefCell;

/// A capacidade de um bloco de cabo (`CableTier.getCableCapacity`), ou `None` se o bloco não é um cabo.
pub type CableCapacity = fn(&str) -> Option<i64>;

static CABLE_KINDS: Mutex<Vec<(&'static str, CableCapacity)>> = Mutex::new(Vec::new());
/// `UPDATE_CLIENTS | UPDATE_KNOWN_SHAPE`: as propriedades de lado só vão aos clientes (`sendUpdatePacket` não avisa vizinho nem refaz forma).
const ARM_FLAGS: i32 = 2 | 16;

/// `(dimensão, posição do vizinho, lado dele que olha o cabo)`.
type AcceptorKey = (&'static str, [i32; 3], usize);

thread_local! {
    /// Os transmissores de energia de cada dimensão do nível deste servidor.
    static CABLES: RefCell<HashMap<&'static str, EnergyTransmitters>> = RefCell::new(HashMap::new());
    /// A última vista publicada de cada cabo (as faces da capability).
    static CABLE_VIEWS: RefCell<HashMap<(&'static str, [i32; 3]), CableView>> = RefCell::new(HashMap::new());
    /// O que do `energy` inteiro do cabo (a caixa de entrada do Horizon) o dono já recebeu.
    static CABLE_ABSORBED: RefCell<HashMap<(&'static str, [i32; 3]), i32>> = RefCell::new(HashMap::new());
    /// Os avisos de tela que o Configurator provocou, para quem chamou entregar ao jogador.
    static CABLE_OVERLAYS: RefCell<Vec<CableOverlay>> = const { RefCell::new(Vec::new()) };
    /// Os `ClearRemoved` que chegaram antes de o block entity entrar no mundo do Horizon (a fila anota o `insert` no começo dele), tentados de novo.
    static CABLE_PENDING: RefCell<transmitters::PendingClears> = RefCell::new(transmitters::PendingClears::new());
    /// Os vizinhos que o dono guarda como aceitadores (o mesmo objeto que o registro tem no cache de aceitadores).
    static CABLE_ACCEPTORS: RefCell<HashMap<AcceptorKey, Rc<HostAcceptor>>> = RefCell::new(HashMap::new());
}

/// `id` (o `ModBlockEntity.id`) passa a ser um cabo de energia de `owner`: os eventos do jogo dele vão ao dono dos cabos, e `capacity` dá a capacidade.
pub fn cables_register(owner: &'static str, id: &str, capacity: CableCapacity) {
    transmitters_register(owner, id, cables_event);
    crate::game::machinery_transmitter_client::register_source(crate::game::machinery_transmitter_client::TagSource { tag: cable_tag, positions: cable_positions });
    let mut kinds = CABLE_KINDS.lock().unwrap_or_else(|e| e.into_inner());
    if !kinds.iter().any(|(name, _)| *name == owner) {
        kinds.push((owner, capacity));
    }
}

/// O pacote de atualização do cabo em `pos` (`writeReducedUpdatedTag`), para o cliente.
fn cable_tag(_game: &Game<'_>, dimension: &'static str, pos: [i32; 3]) -> Option<crate::game::machinery_transmitter_client::TagOut> {
    let (tag, capacity) = with_owner(dimension, |owner| owner.client_update(sim_pos(pos))).flatten()?;
    Some(crate::game::machinery_transmitter_client::TagOut { kind: machinery_basis_core::transmission::client::ClientKind::Energy, capacity, tag })
}

fn cable_positions() -> Vec<(&'static str, [i32; 3])> {
    CABLES.with(|cables| {
        cables
            .try_borrow()
            .map(|cables| cables.iter().flat_map(|(dimension, owner)| owner.positions().into_iter().map(move |pos| (*dimension, arr(pos)))).collect())
            .unwrap_or_default()
    })
}

fn cable_capacity(block: &str) -> Option<i64> {
    let kinds: Vec<CableCapacity> = CABLE_KINDS.lock().unwrap_or_else(|e| e.into_inner()).iter().map(|(_, capacity)| *capacity).collect();
    kinds.into_iter().find_map(|capacity| capacity(block))
}

pub(in crate::game) fn sim_pos(pos: [i32; 3]) -> SimPos {
    SimPos::new(pos[0], pos[1], pos[2])
}

pub(in crate::game) fn arr(pos: SimPos) -> [i32; 3] {
    [pos.x, pos.y, pos.z]
}

/// O que os cinco transmissores publicam de uma conexão: o braço de cada lado vira a propriedade do bloco (só para os clientes, `ARM_FLAGS`), e com a sonda ligada a vista sai no
/// stderr com a etiqueta `probe` (`MBV`, `MBW`, `MTV`, `MBH`).
pub(in crate::game) fn publish_arms(game: &mut Game<'_>, at: [i32; 3], view: &CableView, probe: &str) {
    if SWAP_PROBE {
        eprintln!("MBS {{\"e\":\"publish\",\"p\":[{},{},{}],\"conn\":{:?},\"raw\":{:?}}}", at[0], at[1], at[2], view.connected, view.raw.iter().map(|kind| kind.serialized_name()).collect::<Vec<_>>());
    }
    for side in Face::ALL {
        let value = if view.connected[side as usize] { "true" } else { "false" };
        game.native_set_block_property_with(at, side.name(), value, ARM_FLAGS);
    }
    publish_collision(game, at, view);
    if TRANSMITTER_PROBE {
        let kinds: Vec<&str> = view.kind.iter().map(|kind| kind.serialized_name()).collect();
        let raws: Vec<&str> = view.raw.iter().map(|kind| kind.serialized_name()).collect();
        eprintln!("{probe} {{\"p\":[{},{},{}],\"conn\":{:?},\"kind\":{:?},\"raw\":{:?},\"net\":{}}}", at[0], at[1], at[2], view.connected, kinds, raws, view.has_network);
    }
}

/// As caixas da forma inteira de um transmissor (`BlockTransmitter.getRealShape`) pelo id do bloco e o tipo visível de cada lado, na ordem de `Direction`; `None` se o bloco não tem a
/// forma que o mod declara. O mod diz as caixas de cada bloco em `transmitter_shape_register` (a lib não conhece nenhum id nem medida).
pub type TransmitterShape = fn(&str, &[ConnectionType; 6]) -> Option<Vec<[f64; 6]>>;

static TRANSMITTER_SHAPE: std::sync::OnceLock<TransmitterShape> = std::sync::OnceLock::new();

/// O mod diz como é a forma de seleção e de colisão de um transmissor dele (`getSideForType` de cada tipo de conexão e o centro).
pub fn transmitter_shape_register(shape: TransmitterShape) {
    let _ = TRANSMITTER_SHAPE.set(shape);
}

/// As caixas da forma do transmissor `block` com os tipos `kinds`, pelo que o mod registrou (`transmitter_shape_register`).
pub(in crate::game) fn transmitter_shape_boxes(block: &str, kinds: &[ConnectionType; 6]) -> Option<Vec<[f64; 6]>> {
    TRANSMITTER_SHAPE.get().and_then(|shape| shape(block, kinds))
}

/// A forma de colisão por instância do transmissor em `at` (`BlockTransmitter.getCollisionShape`, `machinery_transmitter_collision`): o estado do Horizon só sabe quais lados têm
/// braço, e o braço `PUSH` e o `PULL` não são o braço normal. Só os transmissores com algum lado `PUSH` ou `PULL` guardam a forma (nos outros a forma do estado, o centro e o braço normal de
/// cada lado ligado, é a mesma, volume a volume: `transmitter_shape_all.rs`).
fn publish_collision(game: &Game<'_>, at: [i32; 3], view: &CableView) {
    let dimension = game.native_dimension();
    horizon_client::collisions::machinery_transmitter_collision::enter(dimension);
    let special = view.kind.iter().any(|kind| matches!(kind, ConnectionType::Push | ConnectionType::Pull));
    let boxes = if special {
        let state = game.world.block(at[0], at[1], at[2]);
        let name = game.res.facts().get(state).name.clone();
        TRANSMITTER_SHAPE.get().and_then(|shape| shape(&name, &view.kind))
    } else {
        None
    };
    horizon_client::collisions::machinery_transmitter_collision::set(dimension, at, boxes);
}

/// O evento do jogo como o dono dos transmissores o recebe (decisão 107). É o mesmo para os cabos, os canos, os tubos, os condutores e os transportadores; `None` é o
/// `ServerStoppedEvent` (`TransmitterNetworkRegistry.reset`), que cada cola atende esvaziando as tabelas dela.
pub(in crate::game) fn owner_event(event: &TransmitterEvent) -> Option<CableEvent> {
    Some(match event {
        TransmitterEvent::Transmitter { pos, kind, .. } => {
            let pos = sim_pos(*pos);
            match kind {
                TransmitterKind::ClearRemoved => CableEvent::ClearRemoved(pos),
                TransmitterKind::PlacedByItem => CableEvent::PlacedByItem(pos),
                TransmitterKind::ChunkUnloaded => CableEvent::ChunkUnloaded(pos),
                TransmitterKind::Removed => CableEvent::Removed(pos),
                TransmitterKind::Update => CableEvent::Update(pos),
                TransmitterKind::Neighbor => CableEvent::Neighbor(pos),
                TransmitterKind::Capability(side) => CableEvent::Capability(pos, Face::ALL[*side]),
            }
        }
        TransmitterEvent::Ticket { chunk, loaded, .. } => CableEvent::Ticket { chunk: ChunkPos::new(chunk[0], chunk[1]), loaded: *loaded },
        TransmitterEvent::ServerTickPost { runs_normally } => CableEvent::ServerTickPost { runs_normally: *runs_normally },
        TransmitterEvent::Reset => return None,
    })
}

/// Um vizinho que dá ou recebe energia pela capability dele. O Horizon não desfaz uma inserção e a lib não tem o jogo na mão quando distribui (`probe_insert` só simula e
/// `commit_probe` insere de verdade, veja `distribution`), então o aceitador (`SnapshotAcceptor`, da lib) faz a conta de `SimpleEnergyHandler.insert` e `extract` sobre o que o buffer
/// dizia no começo do evento (`insert_amount` e `extract_amount` do Horizon) e guarda o que entregou e o que tirou; `acceptors_flush` aplica isso no jogo no fim do evento.
struct HostAcceptor {
    dimension: &'static str,
    pos: [i32; 3],
    /// O lado do vizinho que olha o cabo.
    side: Direction,
    inner: SnapshotAcceptor,
}

impl EnergyInsert for HostAcceptor {
    fn insert(&self, amount: i32, transaction: &Transaction) -> i32 {
        self.inner.insert(amount, transaction)
    }

    fn probe_insert(&self, amount: i32, transaction: &Transaction) -> i32 {
        self.inner.probe_insert(amount, transaction)
    }

    fn commit_probe(&self, amount: i32, transaction: &Transaction) {
        self.inner.commit_probe(amount, transaction);
    }

    fn extract(&self, amount: i32, simulate: bool) -> i32 {
        self.inner.extract(amount, simulate)
    }
}

fn snapshot_of(game: &Game<'_>, dimension: &'static str, pos: [i32; 3], side: Direction) -> Option<BufferSnapshot> {
    let energy = game.native_energy(dimension, pos, side)?;
    Some(BufferSnapshot {
        amount: energy.amount,
        capacity: energy.capacity,
        max_insert: energy.max_insert,
        max_extract: energy.max_extract,
        can_insert: energy.can_insert,
        can_extract: energy.can_extract,
    })
}

/// Antes de um evento que pode distribuir ou puxar energia: os aceitadores leem o buffer de novo (`around` limita aos vizinhos de um cabo).
fn acceptors_refresh(game: &Game<'_>, dimension: &'static str, around: Option<[i32; 3]>) {
    CABLE_ACCEPTORS.with_borrow_mut(|acceptors| {
        // Quem só a tabela guarda já não está no cache de nenhum cabo.
        acceptors.retain(|_, acceptor| Rc::strong_count(acceptor) > 1);
        for ((dim, pos, _), acceptor) in acceptors.iter() {
            if *dim != dimension {
                continue;
            }
            if let Some(cable) = around
                && !Face::ALL.iter().any(|face| neighbour(cable, *face) == *pos)
            {
                continue;
            }
            let snapshot = snapshot_of(game, dimension, *pos, acceptor.side).unwrap_or_default();
            if TRANSMITTER_PROBE && around.is_none() {
                eprintln!(
                    "MBA {{\"p\":[{},{},{}],\"amount\":{},\"cap\":{},\"maxin\":{},\"ins\":{}}}",
                    pos[0], pos[1], pos[2], snapshot.amount, snapshot.capacity, snapshot.max_insert, snapshot.can_insert
                );
            }
            acceptor.inner.snapshot.set(snapshot);
        }
    });
}

/// Depois do evento: o que os aceitadores entregaram e tiraram vai para os buffers dos vizinhos.
fn acceptors_flush(game: &mut Game<'_>, dimension: &'static str) {
    let pending: Vec<Rc<HostAcceptor>> = CABLE_ACCEPTORS.with_borrow(|acceptors| {
        acceptors.values().filter(|a| a.dimension == dimension && a.inner.has_pending()).cloned().collect()
    });
    for acceptor in pending {
        let (delivered, taken) = acceptor.inner.take_pending();
        if delivered > 0 {
            game.native_energy_insert(acceptor.pos, acceptor.side, delivered, false);
        }
        if taken > 0 {
            game.native_energy_extract(dimension, acceptor.pos, acceptor.side, taken, false);
        }
    }
}

/// O jogo visto pelo dono durante um evento.
struct GameWorld<'a, 'g> {
    game: RefCell<&'a mut Game<'g>>,
    dimension: &'static str,
}

pub(in crate::game) fn nbt_to_cable(nbt: &Compound) -> CableNbt {
    let mut out = CableNbt::new();
    for (key, tag) in nbt.iter() {
        let Ok(key) = key.to_string() else {
            continue;
        };
        let converted = match tag {
            Tag::Byte(v) => CableTag::Byte(*v),
            Tag::Short(v) => CableTag::Short(*v),
            Tag::Int(v) => CableTag::Int(*v),
            Tag::Long(v) => CableTag::Long(*v),
            Tag::IntArray(v) => CableTag::IntArray(v.clone()),
            _ => continue,
        };
        out.insert(key, converted);
    }
    out
}

pub(in crate::game) fn cable_to_nbt(tag: &CableNbt, nbt: &mut Compound) {
    for (key, value) in tag.iter().filter(|(key, _)| key.as_str() != ENERGY_KEY) {
        let converted = match value {
            CableTag::Byte(v) => Tag::Byte(*v),
            CableTag::Short(v) => Tag::Short(*v),
            CableTag::Int(v) => Tag::Int(*v),
            CableTag::Long(v) => Tag::Long(*v),
            CableTag::IntArray(v) => Tag::IntArray(v.clone()),
            _ => continue,
        };
        nbt.insert(JavaString::from(key.as_str()), converted);
    }
}

impl OwnerWorld for GameWorld<'_, '_> {
    fn is_block_loaded(&self, pos: SimPos) -> bool {
        self.game.borrow().world.chunks.contains_key(&(pos.x >> 4, pos.z >> 4))
    }

    fn is_getting_powered(&self, pos: SimPos) -> bool {
        self.game.borrow_mut().native_neighbor_signal(self.dimension, arr(pos)) > 0
    }

    fn energy_handler(&self, pos: SimPos, side: Face) -> Option<Acceptor> {
        let side = direction(side);
        let key = (self.dimension, arr(pos), side as usize);
        if let Some(acceptor) = CABLE_ACCEPTORS.with_borrow(|acceptors| acceptors.get(&key).cloned()) {
            // `getCapability` não nulo: o vizinho tem um buffer que alguma configuração de lados deixa encher ou esvaziar por este lado.
            let alive = snapshot_of(&self.game.borrow(), self.dimension, arr(pos), side);
            return alive.map(|snapshot| {
                acceptor.inner.snapshot.set(snapshot);
                acceptor as Acceptor
            });
        }
        let snapshot = snapshot_of(&self.game.borrow(), self.dimension, arr(pos), side)?;
        let acceptor = Rc::new(HostAcceptor { dimension: self.dimension, pos: arr(pos), side, inner: SnapshotAcceptor::new(snapshot) });
        CABLE_ACCEPTORS.with_borrow_mut(|acceptors| acceptors.insert(key, acceptor.clone()));
        Some(acceptor)
    }

    fn cable_info(&mut self, pos: SimPos) -> Option<CableInfo> {
        let key = (pos.x, pos.y, pos.z);
        let game = self.game.borrow();
        let Some(horizon_client::containers::BlockEntity::Mod(entity)) = game.world.block_entities.get(&key) else {
            return None;
        };
        let capacity = cable_capacity(&entity.id)?;
        let mut saved = nbt_to_cable(&entity.nbt);
        // A parte do cabo na energia da rede, pelo `long` exato quando o `energy` inteiro não a comporta (veja `cable_write`).
        if let Some(Tag::Long(exact)) = entity.nbt.get(&JavaString::from(ENERGY_AMOUNT)) {
            saved.insert(ENERGY_KEY.to_owned(), CableTag::Long(*exact));
        }
        if TRANSMITTER_PROBE {
            eprintln!("MBT {{\"t\":{},\"e\":\"cable_info\",\"p\":[{},{},{}],\"conn\":{:?},\"chunk\":{}}}", TRANSMITTER_PASS.load(Ordering::Relaxed), pos.x, pos.y, pos.z, saved.get(machinery_basis_core::transmission::transmitter::keys::CURRENT_CONNECTIONS), game.world.chunks.contains_key(&(pos.x >> 4, pos.z >> 4)));
        }
        let from_save = saved.contains_key(machinery_basis_core::transmission::transmitter::keys::CONNECTION) || saved.contains_key(ENERGY_KEY);
        Some(CableInfo { capacity, saved: from_save.then_some(saved) })
    }

    fn publish(&mut self, pos: SimPos, view: &CableView) {
        let at = arr(pos);
        publish_arms(&mut self.game.borrow_mut(), at, view, "MBV");
        CABLE_VIEWS.with_borrow_mut(|views| views.insert((self.dimension, at), view.clone()));
        // `sendUpdatePacket`: o cliente recebe o pacote de atualização no começo do tick seguinte.
        crate::game::machinery_transmitter_client::dirty(self.dimension, at);
    }

    fn overlay(&mut self, _pos: SimPos, overlay: CableOverlay) {
        CABLE_OVERLAYS.with_borrow_mut(|list| list.push(overlay));
    }
}

fn new_cable_owner() -> EnergyTransmitters {
    EnergyTransmitters::new(DimensionId(0), Uuid::random, |_| Uuid::random().hash_code())
}

/// Roda `f` no dono dos cabos da dimensão. `None` se o dono já está ocupado (um evento dentro de outro).
fn with_owner<R>(dimension: &'static str, f: impl FnOnce(&mut EnergyTransmitters) -> R) -> Option<R> {
    CABLES.with(|cables| {
        let mut cables = cables.try_borrow_mut().ok()?;
        let owner = cables.entry(dimension).or_insert_with(new_cable_owner);
        debug_prepare(owner.registry_mut());
        let result = f(owner);
        debug_flush(owner.registry_mut());
        Some(result)
    })
}

/// `MekanismAPI.debug` no registro de uma família: o estado global (`debug_mode`) vale a cada vez que o dono é usado.
pub(in crate::game) fn debug_prepare<A: Clone + 'static>(registry: &mut machinery_basis_core::transmission::registry::TransmitterNetworkRegistry<A>) {
    registry.set_debug(machinery_basis_core::debug_mode::enabled());
}

/// As linhas de `Mekanism.logger.info` que o registro escreveu com a depuração ligada, no log do servidor.
pub(in crate::game) fn debug_flush<A: Clone + 'static>(registry: &mut machinery_basis_core::transmission::registry::TransmitterNetworkRegistry<A>) {
    for line in registry.take_debug_log() {
        log::info!("{line}");
    }
}

/// `MekanismAPI.debug`.
pub(in crate::game) fn debug_mode() -> bool {
    machinery_basis_core::debug_mode::enabled()
}

/// `MekanismAPI.debug = !MekanismAPI.debug`: o valor novo.
pub(in crate::game) fn debug_toggle() -> bool {
    machinery_basis_core::debug_mode::toggle()
}

/// As redes dos cabos de todas as dimensões, com a chave de cada uma no espelho do conjunto único.
fn cable_networks() -> Vec<(machinery_basis_core::transmission::registry::MirrorKey, usize, usize)> {
    CABLES.with(|cables| {
        cables
            .try_borrow()
            .map(|cables| cables.values().flat_map(|owner| owner.registry().mirror_summaries()).collect())
            .unwrap_or_default()
    })
}

/// `TransmitterNetworkRegistry.toComponents`: todas as redes do registro (energia, fluido, químico, calor e itens, de todas as dimensões), na ordem do conjunto único do original
/// (o espelho do `networks`), com o tipo, o número de transmissores e o de aceitadores de cada uma.
pub(in crate::game) fn registry_networks() -> Vec<(machinery_basis_core::side_config::TransmissionType, usize, usize)> {
    use machinery_basis_core::side_config::TransmissionType;
    let mut known = HashMap::new();
    let families = [
        (TransmissionType::Energy, cable_networks()),
        (TransmissionType::Fluid, crate::game::machinery_resources::pipe_networks()),
        (TransmissionType::Chemical, crate::game::machinery_resources::tube_networks()),
        (TransmissionType::Heat, crate::game::machinery_heat::conductor_networks()),
        (TransmissionType::Item, crate::game::machinery_transporters::transporter_networks()),
    ];
    for (kind, networks) in families {
        for (key, transmitters, acceptors) in networks {
            known.insert(key, (kind, transmitters, acceptors));
        }
    }
    machinery_basis_core::transmission::registry::NetworkMirror::current()
        .in_order()
        .into_iter()
        .filter_map(|key| known.remove(&key))
        .collect()
}

/// O sink dos eventos dos cabos: traduz o evento do jogo para o do dono.
fn cables_event(game: &mut Game<'_>, event: &TransmitterEvent) {
    let dimension = game.native_dimension();
    // O `Update` de um block entity que não é deste dono (cada tick entrega o de cada transmissor a todos os donos) não faz nada nele (`update` sai no `live`), e sem `ClearRemoved` esperando
    // nada mais do que o evento faria roda: nem a espera, nem o espelho dos vizinhos, nem a entrega. Sai antes de tudo isso (decisão 202).
    if let TransmitterEvent::Transmitter { pos, kind: TransmitterKind::Update, .. } = event
        && CABLE_PENDING.with_borrow(transmitters::PendingClears::is_empty)
        && !CABLES.with(|cables| cables.try_borrow().ok().and_then(|cables| cables.get(dimension).map(|owner| owner.cable_at(sim_pos(*pos)).is_some())).unwrap_or(true))
    {
        return;
    }
    {
        let _span = perf_span(PERF_CAB_RETRY);
        cables_retry_pending(game, dimension);
    }
    if TRANSMITTER_PROBE && let TransmitterEvent::Transmitter { pos, kind: TransmitterKind::Update, .. } = event {
        let has = |p: [i32; 3]| with_owner(dimension, |owner| owner.cable_at(sim_pos(p)).is_some()).unwrap_or(false);
        eprintln!("MBT {{\"t\":{},\"e\":\"cable_update_antes\",\"p\":[{},{},{}],\"self\":{},\"s\":{},\"n\":{}}}", TRANSMITTER_PASS.load(Ordering::Relaxed), pos[0], pos[1], pos[2], has(*pos), has([pos[0], pos[1], pos[2] + 1]), has([pos[0], pos[1], pos[2] - 1]));
    }
    cables_deliver(game, dimension, event);
    if TRANSMITTER_PROBE {
        let mask = |p: [i32; 3]| with_owner(dimension, |owner| owner.transmitter(sim_pos(p)).map(|tx| tx.get_all_current_connections())).flatten();
        eprintln!("MBT {{\"t\":{},\"e\":\"cable_mask\",\"m\":{:?},\"s\":{:?}}}", TRANSMITTER_PASS.load(Ordering::Relaxed), mask([0, 150, 0]), mask([0, 150, 1]));
    }
    // Um `ClearRemoved` sem block entity no mundo ainda: tenta de novo nos eventos seguintes.
    if let TransmitterEvent::Transmitter { pos, kind: TransmitterKind::ClearRemoved, .. } = event {
        let created = with_owner(dimension, |owner| owner.cable_at(sim_pos(*pos)).is_some()).unwrap_or(true);
        if TRANSMITTER_PROBE {
            eprintln!("MBT {{\"t\":{},\"e\":\"cable_clear\",\"created\":{created},\"p\":[{},{},{}],\"chunks\":{}}}", TRANSMITTER_PASS.load(Ordering::Relaxed), pos[0], pos[1], pos[2], game.world.chunks.len());
        }
        if !created {
            let now = game.world.game_time;
            CABLE_PENDING.with_borrow_mut(|pending| pending.wait(dimension, *pos, now));
        }
    }
}

/// Os `ClearRemoved` que esperavam o block entity: o que já o tem entra, o que esperou demais (o bloco já não é um cabo) sai da fila.
fn cables_retry_pending(game: &mut Game<'_>, dimension: &'static str) {
    let now = game.world.game_time;
    let ready = {
        let entities = &game.world.block_entities;
        CABLE_PENDING.with_borrow_mut(|pending| {
            pending.retry_known(
                dimension,
                now,
                |pos| match entities.get(&(pos[0], pos[1], pos[2])) {
                    Some(horizon_client::containers::BlockEntity::Mod(entity)) => Some(entity.id.clone()),
                    _ => None,
                },
                |id| cable_capacity(id).is_some(),
            )
        })
    };
    if TRANSMITTER_PROBE && !ready.is_empty() {
        eprintln!("MBT {{\"t\":{},\"e\":\"cable_retry\",\"ready\":{}}}", TRANSMITTER_PASS.load(Ordering::Relaxed), ready.len());
    }
    for pos in ready {
        cables_deliver(game, dimension, &TransmitterEvent::Transmitter { dim: dimension.to_owned(), pos, kind: TransmitterKind::ClearRemoved });
    }
}

fn cables_deliver(game: &mut Game<'_>, dimension: &'static str, event: &TransmitterEvent) {
    let Some(event) = owner_event(event) else {
        transmitter_probe_reset("cabo", CABLES.with_borrow(|owners| sum_counts(owners.values().map(|owner| owner.registry().probe_counts()))));
        CABLES.with_borrow_mut(HashMap::clear);
        CABLE_VIEWS.with_borrow_mut(HashMap::clear);
        CABLE_ABSORBED.with_borrow_mut(HashMap::clear);
        CABLE_ACCEPTORS.with_borrow_mut(HashMap::clear);
        // O que esperava um block entity do mundo que fechou, e os avisos de tela dele, não passam ao seguinte.
        CABLE_PENDING.with_borrow_mut(transmitters::PendingClears::clear);
        CABLE_OVERLAYS.with_borrow_mut(Vec::clear);
        CAPABILITY_CHANGED.lock().unwrap_or_else(|e| e.into_inner()).clear();
        return;
    };
    let post = matches!(event, CableEvent::ServerTickPost { .. });
    let runs_normally = matches!(event, CableEvent::ServerTickPost { runs_normally: true });
    let cleared = if let CableEvent::ClearRemoved(pos) = &event { Some(arr(*pos)) } else { None };
    let removed = if let CableEvent::Removed(pos) = &event { Some(arr(*pos)) } else { None };
    // `onChunkUnloaded` toma a parte do cabo e o chunk vai para o disco: o NBT sai agora, antes do `setRemoved` que o tira do dono.
    let unloaded = if let CableEvent::ChunkUnloaded(pos) = &event { Some(arr(*pos)) } else { None };
    // Quem puxa (`Update`) ou distribui (`ServerTickPost`) fala com os vizinhos pelos aceitadores, que leem o buffer deles antes e escrevem depois.
    {
        let _span = perf_span(PERF_CAB_REFRESH);
        match &event {
            CableEvent::Update(pos) => acceptors_refresh(game, dimension, Some(arr(*pos))),
            CableEvent::ServerTickPost { .. } => acceptors_refresh(game, dimension, None),
            _ => {}
        }
    }
    if post {
        // As capabilities que mudaram durante o tick chegam aos cabos vizinhos antes do `onTick` do registro.
        let changed: Vec<[i32; 3]> = {
            let mut list = CAPABILITY_CHANGED.lock().unwrap_or_else(|e| e.into_inner());
            let (here, other): (Vec<_>, Vec<_>) = std::mem::take(&mut *list).into_iter().partition(|(dim, _)| *dim == dimension);
            *list = other;
            here.into_iter().map(|(_, pos)| pos).collect()
        };
        for pos in changed {
            for side in Face::ALL {
                let neighbor = neighbour(pos, side);
                let present = with_owner(dimension, |owner| owner.cable_at(sim_pos(neighbor)).is_some()).unwrap_or(false);
                if present {
                    let mut world = GameWorld { game: RefCell::new(&mut *game), dimension };
                    with_owner(dimension, |owner| owner.handle(&mut world, CableEvent::Capability(sim_pos(neighbor), side.opposite())));
                }
            }
        }
        // O que entrou em cada cabo depois do tick dele chega ao contêiner (a rede só emite o que já guarda).
        let time = game.world.game_time;
        let _span = perf_span(PERF_CAB_INBOXES);
        let inboxes: Vec<([i32; 3], i32)> = with_owner(dimension, |owner| owner.positions().into_iter().map(|pos| (arr(pos), cable_inbox_value(game, arr(pos)))).collect()).unwrap_or_default();
        with_owner(dimension, |owner| {
            for (pos, inbox) in inboxes {
                cables_absorb(owner, dimension, pos, inbox);
            }
            owner.set_game_time(time);
        });
    }
    let mut scale_packets = Vec::new();
    let handle_span = perf_span(PERF_CAB_HANDLE);
    let dirty = with_owner(dimension, |owner| {
        {
            let mut world = GameWorld { game: RefCell::new(&mut *game), dimension };
            owner.handle(&mut world, event);
        }
        if post {
            scale_packets = owner.take_scale_packets();
        }
        if let Some(pos) = unloaded {
            cable_write(game, owner, pos);
        }
        if post && (TRANSMITTER_PROBE || SETUPS_PROBE) {
            // A sonda da energia (`MBE`): a energia, a capacidade e o que saiu de cada rede neste tick, e quantos cabos e aceitadores ela tem.
            let nets: Vec<String> = owner
                .registry()
                .networks_in_order()
                .into_iter()
                .filter_map(|id| {
                    let net = owner.family().network(id)?;
                    let registered = owner.registry().network(id);
                    Some(format!(
                        "[{},{},{},{},{}]",
                        net.buffer(),
                        net.capacity(),
                        net.prev_transfer_amount(),
                        registered.transmitters_size(),
                        registered.acceptor_count()
                    ))
                })
                .collect();
            eprintln!("MBE {{\"t\":{},\"runs\":{},\"nets\":[{}]}}", TRANSMITTER_PASS.load(Ordering::Relaxed), runs_normally, nets.join(","));
        }
        if post { owner.family_mut().take_dirty_chunks() } else { Vec::new() }
    });
    drop(handle_span);
    let _flush_span = perf_span(PERF_CAB_FLUSH);
    acceptors_flush(game, dimension);
    // `Mekanism.onEnergyTransferred`: o `PacketNetworkScale` de cada evento de transferência, a quem está no raio da rede.
    if !scale_packets.is_empty() {
        crate::game::machinery_transmitter_client::scale(game, dimension, scale_packets.into_iter().map(|out| (out.packet, out.range, None)).collect());
    }
    if let Some(pos) = cleared {
        // O `energy` que o save trouxe já é a parte do cabo (o dono a leu em `cable_info`): não é energia que um vizinho empurrou.
        let inbox = cable_inbox_value(game, pos);
        CABLE_ABSORBED.with_borrow_mut(|absorbed| absorbed.insert((dimension, pos), inbox));
    }
    if let Some(pos) = removed {
        crate::game::machinery_transmitter_client::gone(dimension, pos);
        CABLE_VIEWS.with_borrow_mut(|views| views.remove(&(dimension, pos)));
        CABLE_ABSORBED.with_borrow_mut(|absorbed| absorbed.remove(&(dimension, pos)));
    }
    // `markChunkDirty` da rede: o chunk do cabo vai para o próximo salvamento.
    for key in dirty.unwrap_or_default() {
        let chunk = ChunkPos::unpack(key);
        game.world.modified.insert((chunk.x, chunk.z));
    }
}

/// O que um vizinho empurrou para um cabo desde a última conta está no `energy` inteiro do block entity (o Horizon grava a inserção ali): passa para o
/// contêiner da rede. Quem empurra em `native_energy_insert` só vê o `energy_capacity` do cabo, que chama isto antes de dizer a sobra.
fn cable_inbox_value(game: &Game<'_>, pos: [i32; 3]) -> i32 {
    match game.world.block_entities.get(&(pos[0], pos[1], pos[2])) {
        Some(horizon_client::containers::BlockEntity::Mod(entity)) => stored_energy(entity),
        _ => 0,
    }
}

fn cables_absorb(owner: &mut EnergyTransmitters, dimension: &'static str, pos: [i32; 3], inbox: i32) {
    let key = (dimension, pos);
    let before = CABLE_ABSORBED.with_borrow(|absorbed| absorbed.get(&key).copied().unwrap_or(0));
    let pending = inbox - before;
    if pending > 0 {
        owner.insert_external(sim_pos(pos), pending);
    }
    CABLE_ABSORBED.with_borrow_mut(|absorbed| absorbed.insert(key, inbox));
}

/// O tick do block entity do cabo (que está fora do mundo): a caixa de entrada passa para a rede e se zera, e o `energy` volta a ser só o que o salvamento põe.
pub(in crate::game) fn cable_tick(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    let dimension = game.native_dimension();
    // Um cabo que o dono ainda não conhece (o `clearRemoved` do chunk que acabou de carregar chega depois do primeiro tick do block entity) guarda o que o save trouxe: o
    // `energy` dele é a parte do cabo na energia da rede, e o dono a lê quando o cabo entra.
    if !with_owner(dimension, |owner| owner.cable_at(sim_pos(pos)).is_some()).unwrap_or(false) {
        return;
    }
    let inbox = stored_energy(entity);
    with_owner(dimension, |owner| cables_absorb(owner, dimension, pos, inbox));
    entity.nbt.remove(&JavaString::from(ENERGY));
    entity.nbt.remove(&JavaString::from(ENERGY_AMOUNT));
    CABLE_ABSORBED.with_borrow_mut(|absorbed| absorbed.insert((dimension, pos), 0));
}

/// `energy_capacity` (H1) do cabo: a capacidade que o Horizon enxerga é a energia da caixa de entrada mais o que ainda cabe no contêiner da rede, e as faces são as que
/// `canInsert` e o contêiner dão. `Continue` se o bloco em `pos` não é um cabo que o dono conhece.
pub(in crate::game) fn cable_energy(
    game: &Game<'_>,
    pos: [i32; 3],
) -> std::ops::ControlFlow<Option<(Arc<horizon_client::containers::ModEnergyBuffer>, i32)>> {
    use std::ops::ControlFlow::{Break, Continue};
    let dimension = game.native_dimension();
    let known = CABLES.with(|cables| cables.try_borrow().ok().is_some_and(|cables| cables.get(dimension).is_some_and(|owner| owner.cable_at(sim_pos(pos)).is_some())));
    if !known {
        return Continue(());
    }
    let key = (pos[0], pos[1], pos[2]);
    let Some(horizon_client::containers::BlockEntity::Mod(entity)) = game.world.block_entities.get(&key) else {
        // O block entity que está no tick está fora do mundo: sem buffer, como o Horizon.
        return Break(None);
    };
    let inbox = stored_energy(entity);
    let state = with_owner(dimension, |owner| {
        cables_absorb(owner, dimension, pos, inbox);
        owner.container_state(sim_pos(pos))
    })
    .flatten();
    let Some((capacity, amount)) = state else {
        return Break(None);
    };
    let view = CABLE_VIEWS.with_borrow(|views| views.get(&(dimension, pos)).cloned());
    let Some(view) = view else {
        return Break(None);
    };
    let room = i32::try_from((capacity - amount).max(0)).unwrap_or(i32::MAX).min(i32::MAX - inbox.max(0));
    let mut insert_faces = [false; 6];
    let mut extract_faces = [false; 6];
    for side in 0..6 {
        insert_faces[side] = view.insert[side];
        // A capability existe no lado, mas não deixa tirar (`ReadOnly`): `max_extract` é 0.
        extract_faces[side] = view.exists[side] && !view.insert[side];
    }
    let buffer = horizon_client::containers::ModEnergyBuffer {
        capacity: inbox.saturating_add(room),
        max_insert: i32::MAX,
        max_extract: 0,
        insert_faces,
        extract_faces,
    };
    Break(Some((Arc::new(buffer), inbox)))
}

/// As faces da capability de um cabo, para o ejetor de um vizinho (`BlockCapabilityCache.getCapability` não nulo): `(entrar, existir sem entrar)`.
pub(in crate::game) fn cable_faces(dimension: &'static str, pos: [i32; 3]) -> Option<([bool; 6], [bool; 6])> {
    CABLE_VIEWS.with_borrow(|views| {
        views.get(&(dimension, pos)).map(|view| {
            let mut extract = [false; 6];
            for side in 0..6 {
                extract[side] = view.exists[side] && !view.insert[side];
            }
            (view.insert, extract)
        })
    })
}

/// `TileEntityTransmitter.saveAdditional`: grava no block entity de `pos` o que o cabo guarda, e põe o `energy` (a parte do cabo na energia da rede).
fn cable_write(game: &mut Game<'_>, owner: &mut EnergyTransmitters, pos: [i32; 3]) {
    let dimension = game.native_dimension();
    let inbox = cable_inbox_value(game, pos);
    cables_absorb(owner, dimension, pos, inbox);
    let Some(tag) = owner.write(sim_pos(pos)) else {
        return;
    };
    if TRANSMITTER_PROBE {
        eprintln!("MBS {{\"p\":[{},{},{}],\"energy\":{:?},\"inbox\":{}}}", pos[0], pos[1], pos[2], tag.get(ENERGY_KEY), inbox);
    }
    let mut written = 0;
    if let Some(horizon_client::containers::BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&(pos[0], pos[1], pos[2])) {
        entity.nbt.remove(&JavaString::from(ENERGY));
        cable_to_nbt(&tag, &mut entity.nbt);
        // O Horizon grava o `energy` de um block entity de mod como `int` (um `long` não sobrevive ao salvar), então a parte do cabo na energia da rede, que no original é o `long`
        // `energy` (`UniversalCable.write`), sai no `energy` inteiro e, se passa dele, no `energy_amount` (a convenção da energia exata do port, decisão 2).
        match tag.get(ENERGY_KEY) {
            Some(CableTag::Long(share)) => set_stored_energy_long(entity, *share),
            _ => set_stored_energy(entity, 0),
        }
        written = stored_energy(entity);
    }
    // O `energy` que acabou de sair é a parte do cabo, e não energia que um vizinho empurrou: o dono não a recebe de novo como caixa de entrada.
    CABLE_ABSORBED.with_borrow_mut(|absorbed| absorbed.insert((dimension, pos), written));
}

/// O salvamento do nível (`Game::save_level`, antes de ele esvaziar os chunks alterados): os cabos dos chunks que vão para o disco gravam o NBT deles.
pub(in crate::game) fn cables_save(game: &mut Game<'_>) {
    let dimension = game.native_dimension();
    let chunks: Vec<(i32, i32)> = game.world.modified.iter().copied().collect();
    if chunks.is_empty() {
        return;
    }
    let positions: Vec<[i32; 3]> = with_owner(dimension, |owner| owner.positions().into_iter().map(arr).collect()).unwrap_or_default();
    for pos in positions {
        if chunks.contains(&(pos[0] >> 4, pos[2] >> 4)) {
            with_owner(dimension, |owner| cable_write(game, owner, pos));
        }
    }
}

/// O clique do Configurator num cabo (`ItemConfigurator.useOn`: `IConfigurable.onSneakRightClick` agachado, `onRightClick` senão). `side` é a face clicada e `hit_side` o
/// lado do braço que o olhar toca. Devolve se o clique foi do cabo e os avisos para o jogador.
pub(in crate::game) fn cable_configure(
    game: &mut Game<'_>,
    pos: [i32; 3],
    side: Face,
    hit_side: Option<Face>,
    sneaking: bool,
) -> Option<(bool, Vec<CableOverlay>)> {
    let dimension = game.native_dimension();
    CABLE_OVERLAYS.with_borrow_mut(Vec::clear);
    let consumed = with_owner(dimension, |owner| {
        let mut world = GameWorld { game: RefCell::new(&mut *game), dimension };
        if sneaking { owner.sneak_right_click(&mut world, sim_pos(pos), side, hit_side) } else { owner.right_click(&mut world, sim_pos(pos)) }
    })?;
    let overlays = CABLE_OVERLAYS.with_borrow_mut(std::mem::take);
    Some((consumed, overlays))
}

/// `BlockStateHelper.copyStateData(current, block)` e `Level.setBlockAndUpdate`: o bloco em `pos` vira o `block`, com as propriedades que os dois têm (os braços e a água), e os
/// eventos que isso provoca (o transmissor velho sai, o novo entra) chegam aos donos antes de a troca devolver. `false` se o bloco novo não existe ou o jogo não trocou.
pub(in crate::game) fn alloy_replace(game: &mut Game<'_>, pos: [i32; 3], block: &str) -> bool {
    let dimension = game.native_dimension();
    let current = game.world.block(pos[0], pos[1], pos[2]);
    let properties = match game.pack.assets.registry.state(current) {
        Some(state) => state.properties.clone(),
        None => return false,
    };
    let state = game.with_level(|_, level, _| {
        let mut state = level.registry.default_state(block)?;
        for (key, value) in &properties {
            if let Some(next) = level.native_state_with_property(state, key, value) {
                state = next;
            }
        }
        Some(state)
    });
    let Some(state) = state else {
        return false;
    };
    // `UPDATE_ALL`.
    let changed = game.native_set_block(dimension, pos, state, 3);
    transmitters_flush(game);
    changed
}

/// `ItemAlloy.useOn` num cabo: a liga de nível `alloy_level` (1 avançada, 2 elite, 3 suprema) sobe os cabos da rede que `canUpgrade` aceita (`blocks` dá o bloco de cada nível, do
/// básico ao supremo). `None` se o bloco clicado não é um cabo do dono; senão se algum subiu (a liga se gasta).
pub(in crate::game) fn cable_alloy(game: &mut Game<'_>, pos: [i32; 3], alloy_level: i32, blocks: &[&str]) -> Option<bool> {
    let dimension = game.native_dimension();
    let level_of = |game: &Game<'_>, at: SimPos| -> Option<i32> {
        let block = game.native_block(arr(at));
        blocks.iter().position(|id| *id == block).map(|level| level as i32)
    };
    let mut run = {
        let game_ref: &Game<'_> = game;
        with_owner(dimension, |owner| owner.alloy_start(sim_pos(pos), alloy_level, &|at| level_of(game_ref, at)))??
    };
    let upgraded_block = *blocks.get(usize::try_from(alloy_level).ok()?)?;
    if TRANSMITTER_PROBE {
        eprintln!("MBL {{\"e\":\"start\",\"level\":{alloy_level},\"targets\":{}}}", run.targets.len());
    }
    for target in run.targets.clone() {
        let data = {
            let mut world = GameWorld { game: RefCell::new(&mut *game), dimension };
            with_owner(dimension, |owner| owner.alloy_take(&mut world, &mut run, target)).flatten()
        };
        let Some(data) = data else {
            continue;
        };
        let replaced = alloy_replace(game, arr(target), upgraded_block);
        let given = with_owner(dimension, |owner| owner.alloy_give(&mut run, target, &data)).unwrap_or(false);
        if TRANSMITTER_PROBE {
            eprintln!("MBL {{\"e\":\"step\",\"p\":{:?},\"replaced\":{replaced},\"given\":{given}}}", arr(target));
        }
        if given && run.full() {
            break;
        }
    }
    {
        let mut world = GameWorld { game: RefCell::new(&mut *game), dimension };
        with_owner(dimension, |owner| owner.alloy_finish(&mut world, &run));
    }
    Some(run.did_upgrade())
}

/// O gasto da liga (`stack.consume(1, player)`: o criativo não gasta).
pub(in crate::game) fn alloy_consume(game: &mut Game<'_>) {
    let creative = game.native_acting_player().is_some_and(|player| player.game_mode == 1);
    if creative {
        return;
    }
    let held = game.held_mut();
    let emptied = held.as_mut().is_some_and(|in_hand| {
        in_hand.count = in_hand.count.saturating_sub(1);
        in_hand.count == 0
    });
    if emptied {
        *held = None;
    }
}

/// Os tipos de conexão visíveis do cabo em `pos` (`getConnectionType(side)`: `NONE` onde não há braço), na ordem dos lados, como o último `publish` os deixou. `None` fora do servidor,
/// que é onde o dono vive, ou se o cabo ainda não publicou.
pub(in crate::game) fn cable_arm_kinds(game: &Game<'_>, pos: [i32; 3]) -> Option<[ConnectionType; 6]> {
    CABLE_VIEWS.with_borrow(|views| views.get(&(game.native_dimension(), pos)).map(|view| view.kind))
}

/// Os tipos de conexão crus do cabo em `pos`, na ordem dos lados (para os testes em jogo).
pub(in crate::game) fn cable_connection_types(game: &Game<'_>, pos: [i32; 3]) -> Option<[ConnectionType; 6]> {
    with_owner(game.native_dimension(), |owner| owner.transmitter(sim_pos(pos)).map(|tx| *tx.connection_types_raw())).flatten()
}

/// `ItemNetworkReader.displayTransmitterInfo` para o cabo em `pos`: o que a rede dele sabe de si.
pub(in crate::game) fn cable_read(game: &Game<'_>, pos: [i32; 3]) -> Option<TransmitterReading<()>> {
    use machinery_basis_core::transmission::reading::NetworkReader;
    with_owner(game.native_dimension(), |owner| owner.read_network(sim_pos(pos)))
}

/// `displayConnectedNetworks`: os lados do aceitador em `acceptor` por onde a rede do cabo em `transmitter` entra, se ela o tem.
pub(in crate::game) fn cable_acceptor_sides(game: &Game<'_>, transmitter: [i32; 3], acceptor: [i32; 3]) -> Option<AcceptorSides> {
    use machinery_basis_core::transmission::reading::NetworkReader;
    with_owner(game.native_dimension(), |owner| owner.acceptor_sides(sim_pos(transmitter), sim_pos(acceptor))).flatten()
}
