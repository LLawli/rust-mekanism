//! A radiação do original (`mekanism.common.lib.radiation`), sem nenhum id do Mekanism: as fontes de cada nível, o nível num ponto, o
//! decaimento, a dose de cada entidade, a proteção e o que a dose faz. Quem a usa entra com a configuração (a seção `radiation` do
//! `general.toml`), a radioatividade de cada recurso e os sorteios.
//!
//! - [`RadiationLevelData`]: o `RadiationLevelData` (a attachment `radiation_data` do nível) sobre o `IndexedCuboidMap`: o mapa das
//!   caixas para as fontes (um `HashMap` do Java, cuja ordem é a da gravação e a do decaimento) e o índice por chunk (um
//!   `Long2ObjectAVLTreeMap` de `HashSet`, cuja ordem é a da soma do nível num ponto). As duas ordens são as do Java
//!   ([`crate::sim::java_hash::JavaHashMap`]), com o `hashCode` do record `CenteredBoundingBox`: a soma de doubles muda com a ordem.
//! - A dose ([`radiate_dose`], [`decay_dose`], [`set_dose`]), a proteção ([`resistance`], [`shielded`]) e o tick de cada entidade
//!   ([`entity_tick`], o `PlayerExposure.updateEntityRadiation` com o `RadiationEntity.update`).
//! - [`RadiationScale`], [`SeverityColor`], [`scaled_dose_severity`], [`decay_time`] e a regra de quando o nível em volta vai ao
//!   cliente ([`ClientSync`], o `PreviousRadiationData`).
use crate::sim::java_hash::JavaHashMap;
use std::collections::{BTreeMap, BTreeSet};

/// `RadiationManager.BASELINE`: 100 nSv/h, o fundo de todo lugar e a menor dose.
pub const BASELINE: f64 = 0.000_000_100;
/// `RadiationManager.MIN_MAGNITUDE`: 10 uSv/h, abaixo disso uma fonte some ao decair.
pub const MIN_MAGNITUDE: f64 = 0.000_010;
/// `SharedConstants.TICKS_PER_SECOND`: a chance de 1 em 20 do decaimento das fontes e da exposição de cada entidade.
pub const TICKS_PER_SECOND: i32 = 20;

/// A seção `radiation` do `general.toml` (`GeneralConfig.java:187-203`) que a radiação lê.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadiationConfig {
    /// `enabled` (padrão `true`).
    pub enabled: bool,
    /// `chunkCheckRadius` (padrão 5, de 1 a 100): o raio da caixa de cada fonte é `chunkCheckRadius * 16` blocos.
    pub chunk_check_radius: i32,
    /// `sourceDecayRate` (padrão 0,9995).
    pub source_decay_rate: f64,
    /// `targetDecayRate` (padrão 0,9995).
    pub target_decay_rate: f64,
    /// `negativeEffectsMinSeverity` (padrão 0,1).
    pub negative_effects_min_severity: f64,
}

impl Default for RadiationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            chunk_check_radius: 5,
            source_decay_rate: 0.9995,
            target_decay_rate: 0.9995,
            negative_effects_min_severity: 0.1,
        }
    }
}

impl RadiationConfig {
    /// `RadiationLevelData.MAX_BLOCK_RANGE`.
    pub fn block_range(&self) -> i32 {
        self.chunk_check_radius * 16
    }
}

/// Uma posição de bloco.
pub type BlockPos = [i32; 3];

/// `BlockPos.asLong`: 26 bits de x, 12 de y e 26 de z.
pub fn block_as_long(pos: BlockPos) -> i64 {
    const HORIZONTAL: i64 = (1 << 26) - 1;
    const VERTICAL: i64 = (1 << 12) - 1;
    ((pos[0] as i64 & HORIZONTAL) << 38)
        | (pos[1] as i64 & VERTICAL)
        | ((pos[2] as i64 & HORIZONTAL) << 12)
}

/// `BlockPos.getX(long)` e `getZ(long)` de [`block_as_long`].
fn long_xz(packed: i64) -> (i32, i32) {
    ((packed >> 38) as i32, ((packed << 26) >> 38) as i32)
}

/// `ChunkPos.pack(x, z)`.
pub fn chunk_pack(chunk_x: i32, chunk_z: i32) -> i64 {
    (chunk_x as u32 as i64) | ((chunk_z as u32 as i64) << 32)
}

/// `ChunkPos.pack(BlockPos)`: o chunk do bloco.
pub fn chunk_of(pos: BlockPos) -> i64 {
    chunk_pack(pos[0] >> 4, pos[2] >> 4)
}

/// `Long.hashCode`.
fn long_hash(value: i64) -> i32 {
    (value ^ ((value as u64) >> 32) as i64) as i32
}

/// `IndexedCuboidMap.CenteredBoundingBox`: o centro (`BlockPos.asLong`) e a caixa, com os dois cantos inclusos.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CenteredBox {
    center: i64,
    min: [i32; 3],
    max: [i32; 3],
}

impl CenteredBox {
    fn around(center: BlockPos, radius: i32) -> Self {
        Self {
            center: block_as_long(center),
            min: [center[0] - radius, center[1] - radius, center[2] - radius],
            max: [center[0] + radius, center[1] + radius, center[2] + radius],
        }
    }

    /// O `hashCode` do record: `31 * h + hash(campo)` na ordem dos campos, começando de 0.
    fn hash(&self) -> i32 {
        let mut hash = long_hash(self.center);
        for value in self.min.iter().chain(self.max.iter()) {
            hash = hash.wrapping_mul(31).wrapping_add(*value);
        }
        hash
    }

    fn is_inside(&self, pos: BlockPos) -> bool {
        (0..3).all(|axis| pos[axis] >= self.min[axis] && pos[axis] <= self.max[axis])
    }

    fn center_chunk(&self) -> i64 {
        let (x, z) = long_xz(self.center);
        chunk_pack(x >> 4, z >> 4)
    }
}

/// `RadiationSource`: onde e quanto (Sv/h).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadiationSource {
    pub pos: BlockPos,
    pub magnitude: f64,
}

/// `RadiationUtil.computeExposure`: a magnitude sobre o quadrado da distância, nunca dividida por menos de 1.
pub fn compute_exposure(source: &RadiationSource, pos: BlockPos) -> f64 {
    let dx = (pos[0] - source.pos[0]) as f64;
    let dy = (pos[1] - source.pos[1]) as f64;
    let dz = (pos[2] - source.pos[2]) as f64;
    source.magnitude / f64::max(1.0, dx * dx + dy * dy + dz * dz)
}

/// `LevelAndMaxMagnitude`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LevelAndMaxMagnitude {
    pub level: f64,
    pub max_magnitude: f64,
}

impl LevelAndMaxMagnitude {
    pub const BASELINE: Self = Self {
        level: BASELINE,
        max_magnitude: BASELINE,
    };
}

/// As fontes de radiação de um nível (`RadiationLevelData` sobre o `IndexedCuboidMap`). Ver o começo do módulo.
#[derive(Clone, Debug, Default)]
pub struct RadiationLevelData {
    values: JavaHashMap<CenteredBox, RadiationSource>,
    chunk_index: BTreeMap<i64, JavaHashMap<CenteredBox, ()>>,
    reverse: JavaHashMap<CenteredBox, BTreeSet<i64>>,
}

impl RadiationLevelData {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// As fontes na ordem do mapa (`values()`), que é a da gravação.
    pub fn sources(&self) -> Vec<RadiationSource> {
        self.values.iter().map(|(_, _, source)| *source).collect()
    }

    /// `getRadiationLevelAndMaxMagnitude`: o fundo mais a exposição de cada fonte cuja caixa tem o ponto, na ordem do `HashSet` do chunk.
    pub fn level_and_max(&self, pos: BlockPos) -> LevelAndMaxMagnitude {
        if self.is_empty() {
            return LevelAndMaxMagnitude::BASELINE;
        }
        let mut level = BASELINE;
        let mut max_magnitude = level;
        if let Some(boxes) = self.chunk_index.get(&chunk_of(pos)) {
            for (hash, cuboid, ()) in boxes.iter() {
                if !cuboid.is_inside(pos) {
                    continue;
                }
                let source = self.values.get(hash, cuboid).expect("caixa sem fonte");
                level += compute_exposure(source, pos);
                max_magnitude = f64::max(max_magnitude, source.magnitude);
            }
        }
        if level <= BASELINE && max_magnitude <= BASELINE {
            return LevelAndMaxMagnitude::BASELINE;
        }
        LevelAndMaxMagnitude {
            level,
            max_magnitude,
        }
    }

    /// `IndexedCuboidMap.findFirstAt`: a primeira caixa do `HashSet` do chunk do centro com esse centro.
    fn find_first_at(&self, center: BlockPos) -> Option<(i32, CenteredBox)> {
        let packed = block_as_long(center);
        self.chunk_index
            .get(&chunk_of(center))?
            .iter()
            .find(|(_, cuboid, ())| cuboid.center == packed)
            .map(|(hash, cuboid, ())| (hash, *cuboid))
    }

    /// `IndexedCuboidMap.track`: a caixa entra no mapa e, se é nova, em cada chunk que ela cobre.
    fn track(&mut self, source: RadiationSource, block_radius: i32) {
        let cuboid = CenteredBox::around(source.pos, block_radius);
        let hash = cuboid.hash();
        if self.values.put(hash, cuboid, source).is_some() {
            return;
        }
        let mut keys = BTreeSet::new();
        for chunk_x in (cuboid.min[0] >> 4)..=(cuboid.max[0] >> 4) {
            for chunk_z in (cuboid.min[2] >> 4)..=(cuboid.max[2] >> 4) {
                let key = chunk_pack(chunk_x, chunk_z);
                self.chunk_index
                    .entry(key)
                    .or_default()
                    .put(hash, cuboid, ());
                keys.insert(key);
            }
        }
        match self.reverse.get_mut(hash, &cuboid) {
            Some(existing) => existing.extend(keys),
            None => {
                self.reverse.put(hash, cuboid, keys);
            }
        }
    }

    /// `BiLongMultimap.removeValue`: a caixa sai de cada chunk (o chunk que fica vazio sai do índice) e do mapa reverso.
    fn untrack(&mut self, hash: i32, cuboid: &CenteredBox) {
        if let Some(keys) = self.reverse.remove(hash, cuboid) {
            for key in keys {
                if let Some(set) = self.chunk_index.get_mut(&key) {
                    set.remove(hash, cuboid);
                    if set.is_empty() {
                        self.chunk_index.remove(&key);
                    }
                }
            }
        }
    }

    /// `RadiationLevelData.radiate`: soma à fonte que já está no ponto ou cria uma nova com a caixa de `block_radius`.
    pub fn radiate(&mut self, pos: BlockPos, magnitude: f64, block_radius: i32) {
        match self.find_first_at(pos) {
            Some((hash, cuboid)) => {
                self.values
                    .get_mut(hash, &cuboid)
                    .expect("caixa sem fonte")
                    .magnitude += magnitude;
            }
            None => self.track(RadiationSource { pos, magnitude }, block_radius),
        }
    }

    /// `removeRadiationSource` (`IndexedCuboidMap.removeAt`): tira toda caixa com esse centro; diz se tirou alguma.
    pub fn remove_at(&mut self, center: BlockPos) -> bool {
        if self.values.is_empty() {
            return false;
        }
        let packed = block_as_long(center);
        let found: Vec<(i32, CenteredBox)> = self
            .values
            .iter()
            .filter(|(_, cuboid, _)| cuboid.center == packed)
            .map(|(hash, cuboid, _)| (hash, *cuboid))
            .collect();
        for (hash, cuboid) in &found {
            self.values.remove(*hash, cuboid);
            self.untrack(*hash, cuboid);
        }
        !found.is_empty()
    }

    /// `allCenteredInChunk`: as fontes com centro no chunk, na ordem do `HashSet` dele.
    pub fn centered_in_chunk(&self, chunk_x: i32, chunk_z: i32) -> Vec<RadiationSource> {
        let key = chunk_pack(chunk_x, chunk_z);
        let Some(set) = self.chunk_index.get(&key) else {
            return Vec::new();
        };
        set.iter()
            .filter(|(_, cuboid, ())| cuboid.center_chunk() == key)
            .filter_map(|(hash, cuboid, ())| self.values.get(hash, cuboid).copied())
            .collect()
    }

    /// `removeRadiationSources(chunkX, chunkZ)`: tira as fontes com centro no chunk (`IndexedCuboidMap.remove` por valor, que tira toda
    /// entrada com a mesma posição e a mesma magnitude); diz se tirou alguma.
    pub fn remove_chunk(&mut self, chunk_x: i32, chunk_z: i32) -> bool {
        if self.is_empty() {
            return false;
        }
        let to_remove = self.centered_in_chunk(chunk_x, chunk_z);
        for source in &to_remove {
            let found: Vec<(i32, CenteredBox)> = self
                .values
                .iter()
                .filter(|(_, _, value)| *value == source)
                .map(|(hash, cuboid, _)| (hash, *cuboid))
                .collect();
            for (hash, cuboid) in &found {
                self.values.remove(*hash, cuboid);
                self.untrack(*hash, cuboid);
            }
        }
        !to_remove.is_empty()
    }

    /// `RadiationLevelData.decay`: cada fonte, na ordem do mapa, vezes `rate`; a que fica abaixo de [`MIN_MAGNITUDE`] sai.
    pub fn decay(&mut self, rate: f64) {
        let removed = self.values.remove_if(|_, source| {
            source.magnitude *= rate;
            source.magnitude < MIN_MAGNITUDE
        });
        for (hash, cuboid) in removed {
            self.untrack(hash, &cuboid);
        }
    }

    /// `clearAll`.
    pub fn clear(&mut self) {
        self.values.clear();
        self.chunk_index.clear();
        self.reverse.clear();
    }

    /// `deserialize`: cada fonte da lista gravada, nessa ordem, entra pelo `addNew` (uma posição repetida troca o valor e fica no lugar).
    pub fn load(sources: &[RadiationSource], block_radius: i32) -> Self {
        let mut data = Self::new();
        for source in sources {
            data.track(*source, block_radius);
        }
        data
    }
}

/// `RadiationManager.dumpRadiation(level, pos, type, amount)`: a magnitude que vira fonte, ou `None` se nada é despejado (a radiação
/// desligada, nada, ou um recurso sem radioatividade). Com `Some`, o original chama `radiate(level, pos, radioactivity * amount)`.
pub fn dump_magnitude(enabled: bool, empty: bool, amount: u64, radioactivity: f64) -> Option<f64> {
    if enabled && !empty && amount > 0 && radioactivity > 0.0 {
        Some(radioactivity * amount as f64)
    } else {
        None
    }
}

/// `RadiationCommand.getReturnLevelFromRadiation`: 0 abaixo de [`MIN_MAGNITUDE`], senão a magnitude em uSv/h (`MathUtils.clampToInt`).
pub fn command_return_level(magnitude: f64) -> i32 {
    if magnitude < MIN_MAGNITUDE {
        0
    } else {
        (magnitude * 1_000_000.0) as i32
    }
}

/// `RadiationManager.onTickPre`: decai neste tick com a radiação ligada, o tick andando normal e o sorteio de 1 em 20 (o sorteio só
/// acontece se as duas primeiras valem).
pub fn should_decay_this_tick(
    config: &RadiationConfig,
    runs_normally: bool,
    random: &mut dyn RadiationRandom,
) -> bool {
    config.enabled && runs_normally && random.next_int(TICKS_PER_SECOND) == 0
}

/// Os sorteios da radiação: o `RandomSource` do nível (`entity.level().getRandom()`) e o do `RadiationManager`.
pub trait RadiationRandom {
    fn next_int(&mut self, bound: i32) -> i32;
    fn next_double(&mut self) -> f64;
    fn next_boolean(&mut self) -> bool;
}

/// `RadiationEntity.set`: nunca abaixo do fundo.
pub fn set_dose(magnitude: f64) -> f64 {
    f64::max(BASELINE, magnitude)
}

/// `RadiationEntity.radiate`: só uma magnitude positiva soma.
pub fn radiate_dose(dose: f64, magnitude: f64) -> f64 {
    if magnitude > 0.0 {
        set_dose(dose + magnitude)
    } else {
        dose
    }
}

/// `RadiationEntity.decay`.
pub fn decay_dose(dose: f64, rate: f64) -> f64 {
    set_dose(dose * rate)
}

/// `RadiationUtil.getRadiationResistance` sem o Curios: a soma da proteção de cada peça de armadura, na ordem dos slots.
pub fn resistance(shields: impl IntoIterator<Item = f64>) -> f64 {
    shields
        .into_iter()
        .fold(0.0, |total, shield| total + shield)
}

/// O que `RadiationManager.radiate(entity, magnitude)` soma à dose: `magnitude * (1 - min(1, resistência))`.
pub fn shielded(magnitude: f64, resistance: f64) -> f64 {
    magnitude * (1.0 - f64::min(1.0, resistance))
}

/// O que a dose fez num `update`: o dano (só metade das vezes) e a exaustão (só no jogador, com a força do efeito).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DoseEffect {
    pub hurt: Option<f32>,
    pub strength: Option<f32>,
}

/// `RadiationEntity.update` com uma entidade viva e, se jogador, no modo de jogo: com a severidade acima do fundo sorteia a chance
/// (`min + nextDouble() * (1 - min)`) e, se a severidade passa dela, a força `max(1, (float) log1p(dose))` e a moeda do dano.
pub fn dose_effect(
    config: &RadiationConfig,
    dose: f64,
    random: &mut dyn RadiationRandom,
) -> DoseEffect {
    let severity = scaled_dose_severity(dose);
    if severity <= BASELINE {
        return DoseEffect::default();
    }
    let min_severity = config.negative_effects_min_severity;
    let chance = min_severity + random.next_double() * (1.0 - min_severity);
    if severity <= chance {
        return DoseEffect::default();
    }
    let strength = f32::max(1.0, dose.ln_1p() as f32);
    DoseEffect {
        hurt: random.next_boolean().then_some(strength),
        strength: Some(strength),
    }
}

/// Quem passa por [`entity_tick`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExposedEntity {
    /// A dose (`mekanism:radiation`).
    pub dose: f64,
    /// Não é jogador, ou é um jogador no modo de jogo (`MekanismUtils.isPlayingMode`: nem criativo nem espectador).
    pub playing: bool,
    /// `isAlive()`.
    pub alive: bool,
    /// [`resistance`] da armadura.
    pub resistance: f64,
}

/// `PlayerExposure.updateEntityRadiation`: com a radiação ligada, o sorteio de 1 em 20 do nível; quando sai, o nível no bloco da
/// entidade (`level`), acima do fundo e com a entidade jogando, entra na dose em Sv/s (`/ 3600`, já com a proteção) e a dose decai;
/// depois, todo tick, o `update` ([`dose_effect`]) se ela está viva e jogando. Devolve a dose nova e o efeito.
pub fn entity_tick(
    config: &RadiationConfig,
    entity: ExposedEntity,
    level: impl FnOnce() -> f64,
    random: &mut dyn RadiationRandom,
) -> (f64, DoseEffect) {
    if !config.enabled {
        return (entity.dose, DoseEffect::default());
    }
    let mut dose = entity.dose;
    if random.next_int(TICKS_PER_SECOND) == 0 {
        let magnitude = level();
        if magnitude > BASELINE && entity.playing {
            dose = radiate_dose(dose, shielded(magnitude / 3_600.0, entity.resistance));
        }
        dose = decay_dose(dose, config.target_decay_rate);
    }
    if !entity.alive || !entity.playing {
        return (dose, DoseEffect::default());
    }
    (dose, dose_effect(config, dose, random))
}

/// `RadiationScale`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadiationScale {
    None,
    Low,
    Medium,
    Elevated,
    High,
    Extreme,
}

impl RadiationScale {
    /// `RadiationScale.get`.
    pub fn get(magnitude: f64) -> Self {
        if magnitude < 0.00001 {
            Self::None
        } else if magnitude < 0.001 {
            Self::Low
        } else if magnitude < 0.1 {
            Self::Medium
        } else if magnitude < 10.0 {
            Self::Elevated
        } else if magnitude < 100.0 {
            Self::High
        } else {
            Self::Extreme
        }
    }

    /// O nome do enum (`getSerializedName`).
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::Low => "LOW",
            Self::Medium => "MEDIUM",
            Self::Elevated => "ELEVATED",
            Self::High => "HIGH",
            Self::Extreme => "EXTREME",
        }
    }
}

/// A cor de `RadiationScale.getSeverityColor`, pelo nome do `EnumColor`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeverityColor {
    BrightGreen,
    Gray,
    Yellow,
    Orange,
    Red,
    DarkRed,
}

impl SeverityColor {
    pub fn get(magnitude: f64) -> Self {
        if magnitude <= BASELINE {
            Self::BrightGreen
        } else if magnitude < 0.00001 {
            Self::Gray
        } else if magnitude < 0.001 {
            Self::Yellow
        } else if magnitude < 0.1 {
            Self::Orange
        } else if magnitude < 10.0 {
            Self::Red
        } else {
            Self::DarkRed
        }
    }

    /// O nome do `EnumColor`.
    pub fn name(self) -> &'static str {
        match self {
            Self::BrightGreen => "BRIGHT_GREEN",
            Self::Gray => "GRAY",
            Self::Yellow => "YELLOW",
            Self::Orange => "ORANGE",
            Self::Red => "RED",
            Self::DarkRed => "DARK_RED",
        }
    }
}

/// `RadiationScale.getScaledDoseSeverity`: 0 abaixo de [`MIN_MAGNITUDE`], senão a posição do log10 da dose entre 10 uSv e 100 Sv.
pub fn scaled_dose_severity(magnitude: f64) -> f64 {
    if magnitude < MIN_MAGNITUDE {
        return 0.0;
    }
    let log_baseline = MIN_MAGNITUDE.log10();
    let scale = 100f64.log10() - log_baseline;
    ((-log_baseline + magnitude.log10()) / scale).clamp(0.0, 1.0)
}

/// `RadiationUtil.getDecayTime`: em ticks, de 20 em 20, quanto `magnitude` leva para chegar a [`MIN_MAGNITUDE`] decaindo a `rate`.
pub fn decay_time(magnitude: f64, rate: f64) -> i64 {
    let mut ticks = 0i64;
    let mut local = magnitude;
    while local > MIN_MAGNITUDE {
        local *= rate;
        ticks += TICKS_PER_SECOND as i64;
    }
    ticks
}

/// `PlayerExposure.PreviousRadiationData`: o último nível em volta que foi ao cliente de um jogador.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClientSync {
    pub magnitude: f64,
    power: i32,
    base: f64,
}

impl ClientSync {
    fn power(magnitude: f64) -> i32 {
        magnitude.log10().floor() as i32
    }

    fn with_power(magnitude: f64, power: i32) -> Self {
        // A unidade anda de mil em mil (`floorDiv(power, 3) * 3`), e o cliente mostra duas casas: a base é 10^(siPower - 2).
        let si_power = power.div_euclid(3) * 3;
        Self {
            magnitude,
            power,
            base: 10f64.powi(si_power - 2),
        }
    }

    /// `PreviousRadiationData.compareTo`: `Some` quando o nível novo vai ao cliente (nada guardado, mudou pelo menos a menor unidade
    /// mostrada, ou caiu um dígito), com o que passa a ficar guardado.
    pub fn compare(previous: Option<&ClientSync>, magnitude: f64) -> Option<ClientSync> {
        match previous {
            None => Some(Self::with_power(magnitude, Self::power(magnitude))),
            Some(previous) if (magnitude - previous.magnitude).abs() >= previous.base => {
                Some(Self::with_power(magnitude, Self::power(magnitude)))
            }
            Some(previous) if magnitude < previous.magnitude => {
                let power = Self::power(magnitude);
                (power < previous.power).then(|| Self::with_power(magnitude, power))
            }
            Some(_) => None,
        }
    }
}

/// O que `/mek radiation reduce` faz com a dose: `max(fundo, dose - magnitude)` e quanto saiu (`radiation - newValue`), o número que a mensagem mostra.
pub fn reduce_dose(dose: f64, magnitude: f64) -> (f64, f64) {
    let new_value = f64::max(BASELINE, dose - magnitude);
    (new_value, dose - new_value)
}

/// A attachment da dose (`MekanismAttachmentTypes.RADIATION`): só grava o valor acima do fundo (`radiation > baselineRadiation()`), e o
/// mesmo predicado decide se a cópia (`copyHandler`, uma conversão) leva a dose.
pub fn dose_saved(dose: f64) -> Option<f64> {
    (dose > BASELINE).then_some(dose)
}

/// O `RadiationManager` de um servidor: a attachment `radiation_data` de cada nível (só os que já a criaram: `getData` devolve `null` num nível que
/// nunca recebeu radiação nem a leu do disco) e o sorteio do decaimento do tick (`shouldDecayThisTick`).
#[derive(Debug, Default)]
pub struct RadiationManager {
    levels: BTreeMap<String, RadiationLevelData>,
    should_decay: bool,
}

impl RadiationManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// `getExistingDataOrNull(RADIATION_LEVEL_DATA)`.
    pub fn data(&self, level: &str) -> Option<&RadiationLevelData> {
        self.levels.get(level)
    }

    /// A attachment lida do disco (`deserialize`): a chave gravada, mesmo vazia, cria a attachment.
    pub fn loaded(&mut self, level: &str, data: RadiationLevelData) {
        self.levels.insert(level.to_owned(), data);
    }

    /// O nível fechou: a attachment sai com ele.
    pub fn unload(&mut self, level: &str) {
        self.levels.remove(level);
    }

    /// `radiate(level, pos, magnitude)`: com a radiação ligada, cria a attachment se preciso (`getOrCreateData`) e soma a fonte.
    pub fn radiate(
        &mut self,
        config: &RadiationConfig,
        level: &str,
        pos: BlockPos,
        magnitude: f64,
    ) -> bool {
        if !config.enabled {
            return false;
        }
        self.levels.entry(level.to_owned()).or_default().radiate(
            pos,
            magnitude,
            config.block_range(),
        );
        true
    }

    /// `getRadiationLevel(level, pos)`: o fundo num nível sem a attachment.
    pub fn level_at(&self, level: &str, pos: BlockPos) -> f64 {
        self.level_and_max(level, pos).level
    }

    /// `getRadiationLevelAndMaxMagnitude`.
    pub fn level_and_max(&self, level: &str, pos: BlockPos) -> LevelAndMaxMagnitude {
        self.levels
            .get(level)
            .map_or(LevelAndMaxMagnitude::BASELINE, |data| {
                data.level_and_max(pos)
            })
    }

    /// `dumpRadiation(level, pos, type, amount)`: devolve se despejou (a fonte entrou).
    pub fn dump(
        &mut self,
        config: &RadiationConfig,
        level: &str,
        pos: BlockPos,
        empty: bool,
        amount: u64,
        radioactivity: f64,
    ) -> bool {
        match dump_magnitude(config.enabled, empty, amount, radioactivity) {
            Some(magnitude) => self.radiate(config, level, pos, magnitude),
            None => false,
        }
    }

    /// `onTickPre` (`ServerTickEvent.Pre`).
    pub fn tick_pre(
        &mut self,
        config: &RadiationConfig,
        runs_normally: bool,
        random: &mut dyn RadiationRandom,
    ) {
        self.should_decay = should_decay_this_tick(config, runs_normally, random);
    }

    /// `tickServerWorld` (`LevelTickEvent.Post` de cada nível): decai as fontes do nível se o sorteio do tick saiu e ele tem alguma. Devolve se decaiu.
    pub fn tick_level(&mut self, config: &RadiationConfig, level: &str) -> bool {
        if !self.should_decay {
            return false;
        }
        match self.levels.get_mut(level) {
            Some(data) if !data.is_empty() => {
                data.decay(config.source_decay_rate);
                true
            }
            _ => false,
        }
    }

    /// `clearSources`: toda attachment fica vazia (e continua existindo). Devolve se alguma tinha fonte.
    pub fn clear_sources(&mut self) -> bool {
        let mut had = false;
        for data in self.levels.values_mut() {
            if !data.is_empty() {
                had = true;
                data.clear();
            }
        }
        had
    }

    /// O servidor parou: nada passa ao próximo mundo.
    pub fn reset(&mut self) {
        self.levels.clear();
        self.should_decay = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_packed_block_reads_back_its_x_and_z() {
        for pos in [
            [0, 64, 0],
            [-17, 70, 33],
            [-40, -60, -40],
            [29_999_999, 300, -29_999_999],
        ] {
            assert_eq!(long_xz(block_as_long(pos)), (pos[0], pos[2]));
        }
    }

    #[test]
    fn a_source_at_the_same_block_grows_instead_of_doubling() {
        let mut data = RadiationLevelData::new();
        data.radiate([0, 64, 0], 1.0, 80);
        data.radiate([0, 64, 0], 0.25, 80);
        assert_eq!(
            data.sources(),
            vec![RadiationSource {
                pos: [0, 64, 0],
                magnitude: 1.25
            }]
        );
        assert_eq!(data.level_and_max([2, 64, 0]).level, BASELINE + 1.25 / 4.0);
        assert_eq!(
            data.level_and_max([81, 64, 0]),
            LevelAndMaxMagnitude::BASELINE
        );
    }
    struct Fixed(i32, f64, bool);

    impl RadiationRandom for Fixed {
        fn next_int(&mut self, _bound: i32) -> i32 {
            self.0
        }
        fn next_double(&mut self) -> f64 {
            self.1
        }
        fn next_boolean(&mut self) -> bool {
            self.2
        }
    }

    #[test]
    fn an_entity_is_exposed_and_decays_only_on_the_one_in_twenty_tick() {
        let config = RadiationConfig::default();
        let entity = ExposedEntity {
            dose: BASELINE,
            playing: true,
            alive: true,
            resistance: 0.5,
        };
        let (dose, _) = entity_tick(&config, entity, || 7_200.0, &mut Fixed(0, 0.99, false));
        assert_eq!(dose, decay_dose(radiate_dose(BASELINE, 1.0), 0.9995));
        let (dose, _) = entity_tick(&config, entity, || 7_200.0, &mut Fixed(3, 0.99, false));
        assert_eq!(dose, BASELINE);
        let creative = ExposedEntity {
            playing: false,
            dose: 0.5,
            ..entity
        };
        let (dose, effect) = entity_tick(&config, creative, || 7_200.0, &mut Fixed(0, 0.0, true));
        assert_eq!(
            (dose, effect),
            (decay_dose(0.5, 0.9995), DoseEffect::default())
        );
        let off = RadiationConfig {
            enabled: false,
            ..config
        };
        assert_eq!(
            entity_tick(&off, entity, || 7_200.0, &mut Fixed(0, 0.0, true)).0,
            BASELINE
        );
        let sick = ExposedEntity {
            dose: 50.0,
            ..entity
        };
        let (_, effect) = entity_tick(&config, sick, || BASELINE, &mut Fixed(1, 0.0, true));
        assert_eq!(
            effect,
            DoseEffect {
                hurt: Some(50f64.ln_1p() as f32),
                strength: Some(50f64.ln_1p() as f32)
            }
        );
        let (_, effect) = entity_tick(&config, sick, || BASELINE, &mut Fixed(1, 0.99, true));
        assert_eq!(effect, DoseEffect::default());
    }

    #[test]
    fn the_manager_creates_the_level_data_only_with_radiation_on_and_decays_only_on_the_drawn_tick()
    {
        let on = RadiationConfig::default();
        let off = RadiationConfig {
            enabled: false,
            ..on
        };
        let mut manager = RadiationManager::new();
        assert!(!manager.radiate(&off, "minecraft:overworld", [0, 64, 0], 1.0));
        assert!(manager.data("minecraft:overworld").is_none());
        assert_eq!(
            manager.level_at("minecraft:overworld", [0, 64, 0]),
            BASELINE
        );
        assert!(manager.radiate(&on, "minecraft:overworld", [0, 64, 0], 1.0));
        assert_eq!(
            manager.level_at("minecraft:overworld", [0, 64, 0]),
            BASELINE + 1.0
        );
        assert_eq!(
            manager.level_at("minecraft:the_nether", [0, 64, 0]),
            BASELINE
        );
        // O sorteio só acontece com a radiação ligada e o tick andando; o decaimento só no nível que tem fonte.
        manager.tick_pre(&on, true, &mut Fixed(1, 0.0, false));
        assert!(!manager.tick_level(&on, "minecraft:overworld"));
        manager.tick_pre(&on, false, &mut Fixed(0, 0.0, false));
        assert!(!manager.tick_level(&on, "minecraft:overworld"));
        manager.tick_pre(&on, true, &mut Fixed(0, 0.0, false));
        assert!(manager.tick_level(&on, "minecraft:overworld"));
        assert!(!manager.tick_level(&on, "minecraft:the_nether"));
        assert_eq!(
            manager.data("minecraft:overworld").unwrap().sources()[0].magnitude,
            0.9995
        );
        // `clearSources` esvazia e a attachment continua (o arquivo grava `{}`).
        assert!(manager.clear_sources());
        assert!(manager.data("minecraft:overworld").unwrap().is_empty());
        assert!(!manager.clear_sources());
        // O despejo: só um recurso radioativo com quantidade.
        assert!(!manager.dump(&on, "minecraft:overworld", [1, 2, 3], false, 1000, 0.0));
        assert!(!manager.dump(&on, "minecraft:overworld", [1, 2, 3], true, 1000, 0.05));
        assert!(manager.dump(&on, "minecraft:overworld", [1, 2, 3], false, 1000, 0.05));
        assert_eq!(
            manager.level_at("minecraft:overworld", [1, 2, 3]),
            BASELINE + 0.05 * 1000.0
        );
    }

    #[test]
    fn reduce_keeps_the_dose_at_the_baseline_and_says_how_much_left() {
        assert_eq!(
            reduce_dose(0.0050002, 0.0005),
            (0.0050002 - 0.0005, 0.0050002 - (0.0050002 - 0.0005))
        );
        assert_eq!(
            reduce_dose(0.0045002, 5.0),
            (BASELINE, 0.0045002 - BASELINE)
        );
        assert_eq!(dose_saved(BASELINE), None);
        assert_eq!(dose_saved(0.5), Some(0.5));
    }
}
