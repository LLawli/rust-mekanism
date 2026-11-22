// A radiação no jogo (`RadiationManager`, `PlayerExposure` e `RadiationEntity`), sobre `machinery_basis_core::radiation`. Código nosso no `Game`, módulo do
// `horizon-client`: o gerenciador de cada servidor (a attachment `radiation_data` de cada nível), o arquivo de cada dimensão, o sorteio do decaimento, a dose
// de cada entidade viva e o que ela faz. A lib não conhece id nenhum: o mod entra com os nomes das attachments, o tipo de dano, a proteção de cada item, os
// tipos imunes e a radioatividade de cada recurso (`register`), e com a seção `radiation` do `general.toml` (`set_config`).
//
// Onde o original guarda o que a radiação lembra, e onde fica aqui:
// - as fontes: a attachment do nível, gravada pelo `LevelAttachmentsSavedData` do NeoForge em `dimensions/<ns>/<caminho>/data/neoforge/data_attachments.dat`
//   (`{data: {<attachment>: {radiation: [{pos: [I; x, y, z], radiation: <double>}]}}, DataVersion}`). O NeoForge grava o arquivo de todo nível a cada
//   salvamento (`isDirty` sempre verdadeiro), com `data: {}` num nível sem attachment, e guarda as chaves de outras attachments: aqui também.
// - a dose: a attachment da entidade, no NBT dela (`neoforge:attachments: {<attachment>: {radiation: <double>}}`, só acima do fundo). O Horizon guarda o
//   composto com que cada mob foi lido (`persist.raw`) e o do jogador (`level.player_extra`) e grava por cima deles o que simula: a dose vive ali.
use crate::game::Game;
use crate::game::commands::Target;
use horizon_client::mobs::{Damage, DamageKind, NamedDamage};
use horizon_core::random::{LegacyRandom, RandomSource};
use horizon_world::nbt::{Compound, JavaString, Tag};
use horizon_world::player_inventory::EquipmentSlot;
use machinery_basis_core::radiation::{
    self as rad, BlockPos, ExposedEntity, RadiationConfig, RadiationLevelData, RadiationManager, RadiationRandom, RadiationSource,
};
use std::cell::RefCell;
use std::collections::HashMap;

/// `AttachmentHolder.ATTACHMENTS_NBT_KEY`.
const ATTACHMENTS_KEY: &str = "neoforge:attachments";
/// `SerializationConstants.RADIATION` e `POS`: a lista das fontes, a magnitude de cada uma e a dose.
const RADIATION: &str = "radiation";
const POS: &str = "pos";
/// O arquivo das attachments de um nível (`LevelAttachmentsSavedData.TYPE`), dentro da pasta `data` da dimensão.
const LEVEL_ATTACHMENTS_FILE: &str = "data/neoforge/data_attachments.dat";

/// O que o mod diz da radiação dele.
#[derive(Clone, Copy)]
pub struct RadiationSpec {
    /// A attachment do nível (`mekanism:radiation_data`).
    pub level_attachment: &'static str,
    /// A attachment da dose (`mekanism:radiation`).
    pub entity_attachment: &'static str,
    /// O tipo de dano, com as tags dele que o Horizon sabe ler.
    pub damage: NamedDamage,
    /// `IRadiationShielding.getRadiationShielding` do item vestido.
    pub shielding: fn(&str) -> f64,
    /// O tipo da entidade está na tag `radiation_immune` (`checkEntityInvulnerability`: o dano de radiação não a fere).
    pub immune: fn(&str) -> bool,
    /// `Chemical.getRadioactivity` (o mapa de dados), 0 sem.
    pub radioactivity: fn(&str) -> f64,
}

thread_local! {
    /// O `RadiationManager` do servidor que está aberto.
    static MANAGER: RefCell<RadiationManager> = RefCell::new(RadiationManager::new());
    /// As dimensões cujo arquivo já foi lido neste mundo, com as attachments de outros donos, que voltam ao disco como estavam.
    static FILES: RefCell<HashMap<String, Compound>> = RefCell::new(HashMap::new());
    /// `RadiationManager.RAND` (`RandomSource.create()`: semente única, sem semente fixa).
    static RAND: RefCell<LegacyRandom> = RefCell::new(LegacyRandom::new(unique_seed()));
}

/// O que o mod registrou e a configuração lida: do processo, porque o arquivo de configuração e o registro podem vir de outra thread que a do servidor.
static SPEC: std::sync::RwLock<Option<RadiationSpec>> = std::sync::RwLock::new(None);
static CONFIG: std::sync::RwLock<Option<RadiationConfig>> = std::sync::RwLock::new(None);

fn spec() -> Option<RadiationSpec> {
    *SPEC.read().unwrap_or_else(|error| error.into_inner())
}

/// `RandomSupport.generateUniqueSeed`: o relógio misturado a um contador.
fn unique_seed() -> i64 {
    static UNIQUIFIER: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(8_682_522_807_148_012);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as i64);
    let next = UNIQUIFIER.load(std::sync::atomic::Ordering::Relaxed).wrapping_mul(1_181_783_497_276_652_981);
    UNIQUIFIER.store(next, std::sync::atomic::Ordering::Relaxed);
    next ^ nanos
}

/// Os sorteios da lib por um `RandomSource` do jogo.
struct Draws<'r>(&'r mut LegacyRandom);

impl RadiationRandom for Draws<'_> {
    fn next_int(&mut self, bound: i32) -> i32 {
        self.0.next_int_bound(bound).unwrap_or(0)
    }
    fn next_double(&mut self) -> f64 {
        self.0.next_double()
    }
    fn next_boolean(&mut self) -> bool {
        self.0.next_boolean()
    }
}

/// O mod registra a radiação dele.
pub(in crate::game) fn register(spec: RadiationSpec) {
    *SPEC.write().unwrap_or_else(|error| error.into_inner()) = Some(spec);
}

/// A seção `radiation` do `general.toml`, lida a cada uso como o original lê os `CachedValue`.
pub(in crate::game) fn set_config(config: RadiationConfig) {
    *CONFIG.write().unwrap_or_else(|error| error.into_inner()) = Some(config);
}

/// A configuração de agora (o padrão antes de o arquivo ser lido).
pub(in crate::game) fn config() -> RadiationConfig {
    CONFIG.read().unwrap_or_else(|error| error.into_inner()).unwrap_or_default()
}

/// `RadiationManager.isGlobalRadiationEnabled`.
pub(in crate::game) fn enabled() -> bool {
    config().enabled
}

// ---------------------------------------------------------------------------------------------------------------------------
// As fontes de cada nível e o arquivo.

/// A pasta da dimensão no mundo (`DimensionType.getStorageFolder`): `dimensions/<ns>/<caminho>`.
fn level_file(game: &Game<'_>, dimension: &str) -> Option<std::path::PathBuf> {
    let (namespace, path) = dimension.split_once(':').unwrap_or(("minecraft", dimension));
    Some(game.native_world_dir()?.join("dimensions").join(namespace).join(path).join(LEVEL_ATTACHMENTS_FILE))
}

/// Lê o arquivo da dimensão na primeira vez (o `LevelAttachmentsSavedData` é lido quando o nível nasce): a attachment da radiação, se a chave está lá,
/// e as outras chaves, que voltam ao disco intactas.
fn ensure_loaded(game: &Game<'_>, dimension: &str) {
    if game.remote.is_some() || FILES.with_borrow(|files| files.contains_key(dimension)) {
        return;
    }
    let Some(spec) = spec() else {
        return;
    };
    let mut data = match level_file(game, dimension).and_then(|file| horizon_client::save::read_dat(&file)).and_then(|root| root.get("data").cloned()) {
        Some(Tag::Compound(data)) => data,
        _ => Compound::new(),
    };
    if let Some(Tag::Compound(saved)) = data.remove(&JavaString::from(spec.level_attachment)) {
        let sources: Vec<RadiationSource> = match saved.get(&JavaString::from(RADIATION)) {
            Some(Tag::List(list)) => list.iter().filter_map(read_source).collect(),
            _ => Vec::new(),
        };
        let block_range = config().block_range();
        MANAGER.with_borrow_mut(|manager| manager.loaded(dimension, RadiationLevelData::load(&sources, block_range)));
    }
    FILES.with_borrow_mut(|files| files.insert(dimension.to_owned(), data));
}

/// `RadiationSource.CODEC`: `pos` (`BlockPos.CODEC`, três ints) e `radiation`.
fn read_source(tag: &Tag) -> Option<RadiationSource> {
    let Tag::Compound(source) = tag else {
        return None;
    };
    let pos = match source.get(&JavaString::from(POS)) {
        Some(Tag::IntArray(ints)) if ints.len() == 3 => [ints[0], ints[1], ints[2]],
        _ => return None,
    };
    let magnitude = match source.get(&JavaString::from(RADIATION)) {
        Some(Tag::Double(bits)) => f64::from_bits(*bits),
        _ => return None,
    };
    Some(RadiationSource { pos, magnitude })
}

/// `RadiationLevelData.serialize`: a lista na ordem do mapa, e nada (um composto vazio) sem fonte.
fn level_tag(data: &RadiationLevelData) -> Tag {
    let mut out = Compound::new();
    if !data.is_empty() {
        let sources = data
            .sources()
            .into_iter()
            .map(|source| Tag::compound([(POS, Tag::IntArray(source.pos.to_vec())), (RADIATION, Tag::Double(source.magnitude.to_bits()))]))
            .collect();
        out.insert(JavaString::from(RADIATION), Tag::List(sources));
    }
    Tag::Compound(out)
}

/// O nível vai para o disco (`LevelAttachmentsSavedData`, que sempre se diz sujo): o arquivo da dimensão que salva e o de toda outra cujas fontes este
/// servidor tem (o Horizon só salva os níveis que estão abertos, e o original tem todos os níveis abertos).
pub(in crate::game) fn save(game: &Game<'_>) {
    if game.remote.is_some() {
        return;
    }
    let Some(spec) = spec() else {
        return;
    };
    let current = game.native_dimension().to_owned();
    ensure_loaded(game, &current);
    let dimensions: Vec<String> = FILES.with_borrow(|files| files.keys().cloned().collect());
    for dimension in dimensions {
        let Some(file) = level_file(game, &dimension) else {
            continue;
        };
        let mut data = FILES.with_borrow(|files| files.get(&dimension).cloned().unwrap_or_default());
        if let Some(level) = MANAGER.with_borrow(|manager| manager.data(&dimension).map(level_tag)) {
            data.insert(JavaString::from(spec.level_attachment), level);
        }
        let root = Tag::compound([("data", Tag::Compound(data)), ("DataVersion", Tag::Int(horizon_world::DATA_VERSION))]);
        if let Some(parent) = file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(error) = horizon_client::save::write_dat(&file, root) {
            eprintln!("machinery_basis: as attachments do nível {dimension} não foram gravadas em {}: {error}", file.display());
        }
    }
}

/// O mundo abriu: cada dimensão que já tem pasta lê o arquivo dela (o original abre todos os níveis com o servidor), para o decaimento andar nelas.
pub(in crate::game) fn world_opened(game: &Game<'_>) {
    reset();
    if game.remote.is_some() {
        return;
    }
    let Some(root) = game.native_world_dir().map(|dir| dir.join("dimensions")) else {
        return;
    };
    let mut dimensions = Vec::new();
    for namespace in std::fs::read_dir(&root).into_iter().flatten().flatten() {
        for path in std::fs::read_dir(namespace.path()).into_iter().flatten().flatten() {
            if path.path().join(LEVEL_ATTACHMENTS_FILE).is_file() {
                dimensions.push(format!("{}:{}", namespace.file_name().to_string_lossy(), path.file_name().to_string_lossy()));
            }
        }
    }
    dimensions.sort();
    for dimension in dimensions {
        ensure_loaded(game, &dimension);
    }
}

/// O servidor parou (`ServerStoppedEvent`): nada passa ao próximo mundo.
pub(in crate::game) fn reset() {
    MANAGER.with_borrow_mut(RadiationManager::reset);
    FILES.with_borrow_mut(HashMap::clear);
}

/// `radiate(level, pos, magnitude)`.
pub(in crate::game) fn radiate(game: &Game<'_>, dimension: &str, pos: BlockPos, magnitude: f64) {
    ensure_loaded(game, dimension);
    let config = config();
    MANAGER.with_borrow_mut(|manager| manager.radiate(&config, dimension, pos, magnitude));
}

/// `getRadiationLevel(level, pos)`.
pub(in crate::game) fn level_at(game: &Game<'_>, dimension: &str, pos: BlockPos) -> f64 {
    ensure_loaded(game, dimension);
    MANAGER.with_borrow(|manager| manager.level_at(dimension, pos))
}

/// `dumpRadiation(level, pos, type, amount)`: devolve se despejou.
pub(in crate::game) fn dump(game: &Game<'_>, dimension: &str, pos: BlockPos, resource: &str, amount: u64) -> bool {
    let Some(spec) = spec() else {
        return false;
    };
    ensure_loaded(game, dimension);
    let config = config();
    let radioactivity = if resource.is_empty() { 0.0 } else { (spec.radioactivity)(resource) };
    MANAGER.with_borrow_mut(|manager| manager.dump(&config, dimension, pos, resource.is_empty(), amount, radioactivity))
}

/// `clearSources`.
pub(in crate::game) fn clear_sources() {
    MANAGER.with_borrow_mut(RadiationManager::clear_sources);
}

/// `onTickPre` (`ServerTickEvent.Pre`): o sorteio do decaimento do tick.
pub(in crate::game) fn server_tick_start(runs_normally: bool) {
    let config = config();
    RAND.with_borrow_mut(|random| MANAGER.with_borrow_mut(|manager| manager.tick_pre(&config, runs_normally, &mut Draws(random))));
}

/// `tickServerWorld` de cada nível (`LevelTickEvent.Post`), no fim do tick do servidor: as entidades de todos os níveis já leram o nível deste tick.
pub(in crate::game) fn server_tick_end() {
    let config = config();
    let dimensions: Vec<String> = FILES.with_borrow(|files| files.keys().cloned().collect());
    MANAGER.with_borrow_mut(|manager| {
        for dimension in &dimensions {
            manager.tick_level(&config, dimension);
        }
    });
}

// ---------------------------------------------------------------------------------------------------------------------------
// A dose.

/// A dose gravada num composto de entidade, o fundo sem ela (`AttachmentType.builder(baselineRadiation)`).
fn read_dose(tag: Option<&Tag>, attachment: &str) -> f64 {
    let dose = tag
        .and_then(|tag| tag.get(ATTACHMENTS_KEY))
        .and_then(|attachments| attachments.get(attachment))
        .and_then(|value| value.get(RADIATION));
    match dose {
        Some(Tag::Double(bits)) => f64::from_bits(*bits),
        _ => rad::BASELINE,
    }
}

/// `entity.setData(RADIATION, dose)`: o composto grava a dose só acima do fundo, e a chave das attachments some vazia.
fn write_dose(tag: &mut Option<Tag>, attachment: &str, dose: f64) {
    let saved = rad::dose_saved(dose);
    if saved.is_none() && tag.is_none() {
        return;
    }
    let Tag::Compound(root) = tag.get_or_insert_with(|| Tag::Compound(Compound::new())) else {
        return;
    };
    let key = JavaString::from(ATTACHMENTS_KEY);
    match saved {
        Some(dose) => {
            let Tag::Compound(attachments) = root.entry(key).or_insert_with(|| Tag::Compound(Compound::new())) else {
                return;
            };
            attachments.insert(JavaString::from(attachment), Tag::compound([(RADIATION, Tag::Double(dose.to_bits()))]));
        }
        None => {
            let empty = match root.get_mut(&key) {
                Some(Tag::Compound(attachments)) => {
                    attachments.remove(&JavaString::from(attachment));
                    attachments.is_empty()
                }
                _ => false,
            };
            if empty {
                root.remove(&key);
            }
        }
    }
}

fn mob_dose(game: &Game<'_>, id: u32, attachment: &str) -> f64 {
    read_dose(game.persist.raw.get(&id), attachment)
}

fn set_mob_dose(game: &mut Game<'_>, id: u32, attachment: &str, dose: f64) {
    let mut tag = game.persist.raw.remove(&id);
    write_dose(&mut tag, attachment, dose);
    if let Some(tag) = tag {
        game.persist.raw.insert(id, tag);
    }
}

fn player_dose(game: &Game<'_>, attachment: &str) -> f64 {
    read_dose(game.level.player_extra.as_ref(), attachment)
}

fn set_player_dose(game: &mut Game<'_>, attachment: &str, dose: f64) {
    write_dose(&mut game.level.player_extra, attachment, dose);
}

/// A dose de quem um seletor achou (`getCapability(RADIATION_ENTITY)`): `None` para o que não é entidade viva que o port simula.
pub(in crate::game) fn target_dose(game: &mut Game<'_>, target: Target) -> Option<f64> {
    let spec = spec()?;
    match target {
        Target::Player | Target::Guest(_) => game.for_player(target, |g| player_dose(g, spec.entity_attachment)),
        Target::Mob(id) => Some(mob_dose(game, id, spec.entity_attachment)),
        _ => None,
    }
}

/// `IRadiationEntity.set`: nunca abaixo do fundo.
pub(in crate::game) fn set_target_dose(game: &mut Game<'_>, target: Target, dose: f64) {
    let Some(spec) = spec() else {
        return;
    };
    let dose = rad::set_dose(dose);
    match target {
        Target::Player | Target::Guest(_) => {
            game.for_player(target, |g| set_player_dose(g, spec.entity_attachment, dose));
        }
        Target::Mob(id) => set_mob_dose(game, id, spec.entity_attachment, dose),
        _ => {}
    }
}

/// `IRadiationEntity.radiate`: só uma magnitude positiva soma.
pub(in crate::game) fn radiate_target(game: &mut Game<'_>, target: Target, magnitude: f64) {
    if let Some(dose) = target_dose(game, target) {
        let after = rad::radiate_dose(dose, magnitude);
        if after.to_bits() != dose.to_bits() {
            set_target_dose(game, target, after);
        }
    }
}

/// `RadiationUtil.getRadiationResistance` sem o Curios: os quatro slots de armadura, pés, pernas, peito e cabeça.
fn resistance(spec: &RadiationSpec, worn: impl IntoIterator<Item = Option<horizon_client::items::ItemStack>>) -> f64 {
    rad::resistance(worn.into_iter().flatten().filter(|stack| stack.count > 0).map(|stack| (spec.shielding)(&stack.item)))
}

/// O bloco de uma posição (`Entity.blockPosition`).
fn block_of(position: [f64; 3]) -> BlockPos {
    [position[0].floor() as i32, position[1].floor() as i32, position[2].floor() as i32]
}

/// `PlayerExposure.onLivingTick` (`EntityTickEvent.Post` de uma entidade viva que não é jogador): a exposição, o decaimento e o `update` com o
/// `RandomSource` do nível.
pub(in crate::game) fn mob_ticked(game: &mut Game<'_>, dimension: &str, id: u32) {
    let config = config();
    let Some(spec) = spec() else {
        return;
    };
    if !config.enabled || game.remote.is_some() {
        return;
    }
    let Some(index) = game.mobs.iter().position(|mob| mob.id == id) else {
        return;
    };
    let (pos, alive, worn, kind) = {
        let mob = &game.mobs[index];
        let stacks = mob.equipment_stacks();
        (
            block_of([mob.position.x, mob.position.y, mob.position.z]),
            mob.alive() && !mob.removed,
            [stacks[2].clone(), stacks[3].clone(), stacks[4].clone(), stacks[5].clone()],
            if mob.kind.id.contains(':') { mob.kind.id.clone() } else { format!("minecraft:{}", mob.kind.id) },
        )
    };
    ensure_loaded(game, dimension);
    let dose = mob_dose(game, id, spec.entity_attachment);
    let entity = ExposedEntity { dose, playing: true, alive, resistance: resistance(&spec, worn) };
    // `entity.level().getRandom()`: o do nível em que a entidade está, que é o aberto no `Game` durante o tick dela (o `native_level_random` do Horizon
    // só responde com o mundo aberto em LAN ou num servidor dedicado).
    let random = &mut game.world.random;
    let (after, effect) = rad::entity_tick(&config, entity, || MANAGER.with_borrow(|manager| manager.level_at(dimension, pos)), &mut Draws(random));
    if after.to_bits() != dose.to_bits() {
        set_mob_dose(game, id, spec.entity_attachment, after);
    }
    // `hurtServer` com o dano de radiação: o tipo na tag `radiation_immune` é invulnerável a ele.
    if let Some(strength) = effect.hurt
        && !(spec.immune)(&kind)
        && let Some(mob) = game.mobs.iter_mut().find(|mob| mob.id == id)
    {
        mob.damage(strength, Damage { kind: DamageKind::Named(spec.damage), attacker: None, from: None, attacker_species: None });
    }
}

/// `PlayerExposure.tickServer` (o `PlayerTickEvent.Post` do servidor): a exposição com o modo de jogo, o dano e a exaustão.
pub(in crate::game) fn player_ticked(game: &mut Game<'_>) {
    let config = config();
    let Some(spec) = spec() else {
        return;
    };
    if !config.enabled || game.remote.is_some() {
        return;
    }
    let Some(player) = game.native_acting_player() else {
        return;
    };
    // `MekanismUtils.isPlayingMode`: nem criativo nem espectador.
    let playing = !matches!(player.game_mode, 1 | 3);
    let worn = [EquipmentSlot::Feet, EquipmentSlot::Legs, EquipmentSlot::Chest, EquipmentSlot::Head].map(|slot| game.native_equipment(slot));
    let entity = ExposedEntity {
        dose: player_dose(game, spec.entity_attachment),
        playing,
        alive: player.health > 0.0,
        resistance: resistance(&spec, worn),
    };
    let dimension = player.dimension.clone();
    ensure_loaded(game, &dimension);
    let pos = block_of(player.position);
    // `entity.level().getRandom()`: o do nível em que a entidade está, que é o aberto no `Game` durante o tick dela (o `native_level_random` do Horizon
    // só responde com o mundo aberto em LAN ou num servidor dedicado).
    let random = &mut game.world.random;
    let (after, effect) = rad::entity_tick(&config, entity, || MANAGER.with_borrow(|manager| manager.level_at(&dimension, pos)), &mut Draws(random));
    if after.to_bits() != entity.dose.to_bits() {
        set_player_dose(game, spec.entity_attachment, after);
    }
    if let Some(strength) = effect.strength {
        if effect.hurt.is_some() {
            game.hurt_player_from(strength, None, spec.damage.name, Some(None));
        }
        // `player.getFoodData().addExhaustion(strength)`.
        if let Some(motion) = game.player.as_mut() {
            motion.health.food.add_exhaustion(strength);
        }
    }
}

/// O jogador renasceu (`PlayerRespawnEvent`, depois do `PlayerEvent.Clone` da morte): a dose não passa para o corpo novo (a attachment não é `copyOnDeath`).
pub(in crate::game) fn player_respawned(game: &mut Game<'_>) {
    if let Some(spec) = spec() {
        set_player_dose(game, spec.entity_attachment, rad::BASELINE);
    }
}
