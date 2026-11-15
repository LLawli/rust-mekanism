//! Porte de `TileComponentUpgrade`, `UpgradeInventorySlot`, `Upgrade` e `IUpgradeHelper`. A lib não conhece nenhum upgrade: os
//! tipos e os máximos chegam como dados (`data/<ns>/machinery_basis/upgrade/<nome>.json`), as tags de suporte também
//! (`data/<ns>/tags/machinery_basis/upgrade/...`), e quem usa diz qual item os carrega.
use crate::components::Components;
use crate::containers::{
    Access, AutomationType, InventorySlot, ItemResource, ItemTest, SlotChecks, SlotContents,
};
use crate::sim::open_hash::OpenHashMap;
use crate::transaction::{Callback, Transaction};
use crate::upgrade_order::{UpgradeKey, entries_from_disk, entries_from_file, file_order, loaded_map};
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::rc::Rc;
use std::sync::{Arc, Mutex, RwLock};

/// `SharedConstants.TICKS_PER_SECOND`: quanto uma máquina leva para instalar um upgrade.
pub const UPGRADE_TICKS_REQUIRED: i32 = 20;

/// A pasta dos tipos, dentro de `data/<ns>/`.
pub const TYPES_FOLDER: &str = "machinery_basis/upgrade";

/// A pasta das tags de suporte, dentro de `data/<ns>/`. O caminho é o de uma tag de registro.
pub const TAGS_FOLDER: &str = "tags/machinery_basis/upgrade";

/// O que marca outra tag dentro de uma tag.
const TAG_PREFIX: char = '#';

/// Um tipo de upgrade declarado (`Upgrade`): o máximo e a cor. O nome e a descrição são chaves de tradução que a tela lê.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpgradeType {
    /// `max`: quantos desse tipo cabem numa máquina.
    pub max: i32,
    /// `color`: um `EnumColor` ou uma cor de texto, como o arquivo diz.
    pub color: String,
}

/// Por que um arquivo de tipo foi recusado.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpgradeError {
    BadId,
    /// Falta `color`, `display_name` ou `description`, que o `DIRECT_CODEC` exige.
    MissingField(&'static str),
    /// `max` não é um inteiro positivo (`ExtraCodecs.POSITIVE_INT`).
    BadMax,
}

impl fmt::Display for UpgradeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadId => f.write_str("o id precisa ser <ns>:<nome>"),
            Self::MissingField(field) => write!(f, "falta `{field}`"),
            Self::BadMax => f.write_str("`max` precisa ser um inteiro positivo"),
        }
    }
}

fn valid_id(id: &str) -> bool {
    id.split_once(':')
        .is_some_and(|(namespace, path)| !namespace.is_empty() && !path.is_empty())
}

impl UpgradeType {
    /// `Upgrade.DIRECT_CODEC`: `max` vale 1 quando falta.
    pub fn parse(id: &str, json: &Value) -> Result<Self, UpgradeError> {
        if !valid_id(id) {
            return Err(UpgradeError::BadId);
        }
        for field in ["display_name", "description"] {
            if json.get(field).is_none() {
                return Err(UpgradeError::MissingField(field));
            }
        }
        let color = json
            .get("color")
            .and_then(Value::as_str)
            .ok_or(UpgradeError::MissingField("color"))?;
        let max = match json.get("max") {
            None => 1,
            Some(max) => max
                .as_i64()
                .filter(|max| *max > 0)
                .and_then(|max| i32::try_from(max).ok())
                .ok_or(UpgradeError::BadMax)?,
        };
        Ok(Self {
            max,
            color: color.to_owned(),
        })
    }
}

/// Os tipos de upgrade de uma carga de dados.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpgradeTypes {
    types: BTreeMap<String, UpgradeType>,
}

impl UpgradeTypes {
    /// Um arquivo ruim fica de fora e volta na lista de erros, sem derrubar os outros.
    pub fn load(files: &[(String, Value)]) -> (Self, Vec<(String, UpgradeError)>) {
        let mut types = Self::default();
        let mut errors = Vec::new();
        for (id, json) in files {
            match UpgradeType::parse(id, json) {
                Ok(parsed) => {
                    types.types.insert(id.clone(), parsed);
                }
                Err(error) => errors.push((id.clone(), error)),
            }
        }
        (types, errors)
    }

    pub fn get(&self, id: &str) -> Option<&UpgradeType> {
        self.types.get(id)
    }

    /// Os ids declarados, na ordem do registro.
    pub fn ids(&self) -> impl Iterator<Item = &String> {
        self.types.keys()
    }

    pub fn len(&self) -> usize {
        self.types.len()
    }

    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }
}

/// As tags de suporte de uma carga de dados: `values` lista tipos e, com `#`, outras tags.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpgradeTags {
    tags: BTreeMap<String, Vec<String>>,
}

impl UpgradeTags {
    pub fn load(files: &[(String, Value)]) -> Self {
        let mut tags = Self::default();
        for (id, json) in files {
            let values = json.get("values").and_then(Value::as_array);
            let values = values.into_iter().flatten().filter_map(Value::as_str);
            tags.tags
                .entry(id.clone())
                .or_default()
                .extend(values.map(str::to_owned));
        }
        tags
    }

    /// Os tipos de uma tag, com as tags dentro dela já abertas. Uma tag que se cita de volta não entra duas vezes.
    pub fn members(&self, tag: &str) -> BTreeSet<String> {
        let mut found = BTreeSet::new();
        self.collect(tag, &mut BTreeSet::new(), &mut found);
        found
    }

    fn collect(&self, tag: &str, seen: &mut BTreeSet<String>, found: &mut BTreeSet<String>) {
        if !seen.insert(tag.to_owned()) {
            return;
        }
        for value in self.tags.get(tag).into_iter().flatten() {
            match value.strip_prefix(TAG_PREFIX) {
                Some(inner) => self.collect(inner, seen, found),
                None => {
                    found.insert(value.clone());
                }
            }
        }
    }
}

static CURRENT_TYPES: RwLock<Option<Arc<UpgradeTypes>>> = RwLock::new(None);
static CURRENT_TAGS: RwLock<Option<Arc<UpgradeTags>>> = RwLock::new(None);
static TYPE_SOURCES: Mutex<BTreeMap<String, UpgradeTypes>> = Mutex::new(BTreeMap::new());
static TAG_SOURCES: Mutex<BTreeMap<String, UpgradeTags>> = Mutex::new(BTreeMap::new());

/// Troca os tipos em uso: a cada carga de dados, inclusive no `/reload`.
pub fn set_types_source(source: &str, types: UpgradeTypes) {
    let mut sources = TYPE_SOURCES.lock().unwrap_or_else(|e| e.into_inner());
    sources.insert(source.to_owned(), types);
    let mut merged = UpgradeTypes::default();
    for types in sources.values() {
        merged.types.extend(types.types.clone());
    }
    let mut current = CURRENT_TYPES.write().unwrap_or_else(|e| e.into_inner());
    *current = Some(Arc::new(merged));
}

/// Os tipos em uso: vazios antes da primeira carga.
pub fn current_types() -> Arc<UpgradeTypes> {
    let current = CURRENT_TYPES.read().unwrap_or_else(|e| e.into_inner());
    current.clone().unwrap_or_default()
}

/// Troca as tags em uso: a cada carga de dados, inclusive no `/reload`.
pub fn set_tags_source(source: &str, tags: UpgradeTags) {
    let mut sources = TAG_SOURCES.lock().unwrap_or_else(|e| e.into_inner());
    sources.insert(source.to_owned(), tags);
    let mut merged = UpgradeTags::default();
    for tags in sources.values() {
        for (tag, values) in &tags.tags {
            merged
                .tags
                .entry(tag.clone())
                .or_default()
                .extend(values.iter().cloned());
        }
    }
    let mut current = CURRENT_TAGS.write().unwrap_or_else(|e| e.into_inner());
    *current = Some(Arc::new(merged));
}

/// As tags em uso: vazias antes da primeira carga.
pub fn current_tags() -> Arc<UpgradeTags> {
    let current = CURRENT_TAGS.read().unwrap_or_else(|e| e.into_inner());
    current.clone().unwrap_or_default()
}

/// O que o servidor manda ao cliente que não leu os dados dele: os tipos e as tags. No original os tipos são um registro
/// sincronizado; aqui eles vêm da carga de dados do servidor, que um cliente de outro processo não faz.
pub fn encode_sync(types: &UpgradeTypes, tags: &UpgradeTags) -> Vec<u8> {
    let types: serde_json::Map<String, Value> = types
        .types
        .iter()
        .map(|(id, ty)| {
            (
                id.clone(),
                serde_json::json!({ "max": ty.max, "color": ty.color }),
            )
        })
        .collect();
    let tags: serde_json::Map<String, Value> = tags
        .tags
        .iter()
        .map(|(id, values)| (id.clone(), Value::from(values.clone())))
        .collect();
    serde_json::json!({ "types": types, "tags": tags })
        .to_string()
        .into_bytes()
}

/// O inverso de [`encode_sync`]. `None` quando os bytes não são o que o servidor manda.
pub fn decode_sync(bytes: &[u8]) -> Option<(UpgradeTypes, UpgradeTags)> {
    let json: Value = serde_json::from_slice(bytes).ok()?;
    let mut types = UpgradeTypes::default();
    for (id, ty) in json.get("types")?.as_object()? {
        let max = i32::try_from(ty.get("max")?.as_i64()?)
            .ok()
            .filter(|max| *max > 0)?;
        let color = ty.get("color")?.as_str()?.to_owned();
        types.types.insert(id.clone(), UpgradeType { max, color });
    }
    let mut tags = UpgradeTags::default();
    for (id, values) in json.get("tags")?.as_object()? {
        let values = values
            .as_array()?
            .iter()
            .map(|v| v.as_str().map(str::to_owned))
            .collect::<Option<Vec<_>>>()?;
        tags.tags.insert(id.clone(), values);
    }
    Some((types, tags))
}

/// O que uma máquina sabe dos upgrades: os tipos e seus máximos, o que a tag dela aceita (`getSupportedUpgrade`) e o item que os
/// carrega (`IUpgradeHelper`: `asResource` e `dataComponent`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpgradeData {
    item: Rc<str>,
    /// O componente do item que guarda o tipo (`IUpgradeHelper.dataComponent`).
    component: Rc<str>,
    item_max_stack_size: u32,
    types: BTreeMap<Rc<str>, i32>,
    supported: BTreeSet<Rc<str>>,
}

impl UpgradeData {
    /// Da carga de dados: os membros da tag que não são um tipo declarado ficam de fora.
    pub fn new(
        item: &str,
        component: &str,
        item_max_stack_size: u32,
        types: &UpgradeTypes,
        tags: &UpgradeTags,
        supported_tag: &str,
    ) -> Self {
        let declared: Vec<(&str, i32)> = types
            .types
            .iter()
            .map(|(id, declared)| (id.as_str(), declared.max))
            .collect();
        let members = tags.members(supported_tag);
        let supported: Vec<&str> = members.iter().map(String::as_str).collect();
        Self::from_parts(item, component, item_max_stack_size, &declared, &supported)
    }

    /// Dos pedaços já resolvidos, como os testes montam.
    pub fn from_parts(
        item: &str,
        component: &str,
        item_max_stack_size: u32,
        types: &[(&str, i32)],
        supported: &[&str],
    ) -> Self {
        let types: BTreeMap<Rc<str>, i32> = types
            .iter()
            .map(|(id, max)| (Rc::from(*id), *max))
            .collect();
        let supported = supported
            .iter()
            .filter(|id| types.contains_key(**id))
            .map(|id| Rc::from(*id))
            .collect();
        Self {
            item: Rc::from(item),
            component: Rc::from(component),
            item_max_stack_size,
            types,
            supported,
        }
    }

    /// Todos os tipos declarados, na ordem do registro (a dos ids): o `Registry<Upgrade>` que a janela percorre.
    pub fn type_ids(&self) -> Vec<String> {
        self.types.keys().map(|id| id.to_string()).collect()
    }

    /// `upgrade.value().max()`.
    pub fn max(&self, upgrade: &str) -> Option<i32> {
        self.types.get(upgrade).copied()
    }

    /// `upgrade.is(supported)`.
    pub fn supports(&self, upgrade: &str) -> bool {
        self.supported.contains(upgrade)
    }

    /// Os tipos da tag da máquina, na ordem do registro.
    pub fn supported(&self) -> impl Iterator<Item = &Rc<str>> {
        self.supported.iter()
    }

    /// `itemType.get(IUpgradeHelper.INSTANCE.dataComponent())`: o tipo que o item carrega, se carrega um que existe. Os outros
    /// componentes do item não contam: a pergunta é só pelo tipo.
    pub fn upgrade_of(&self, resource: &ItemResource) -> Option<Rc<str>> {
        if resource.id != self.item {
            return None;
        }
        let kind = Components::parse_text_value(resource.components.get(&self.component)?)?;
        self.types
            .get_key_value(kind.as_str())
            .map(|(id, _)| id.clone())
    }

    /// `IUpgradeHelper.asResource`: o item com o tipo no componente, e mais nada.
    pub fn resource_of(&self, upgrade: &str) -> ItemResource {
        ItemResource::with_component(
            &self.item,
            self.item_max_stack_size,
            &self.component,
            &Components::text_value(upgrade),
        )
    }

    /// O id do componente que guarda o tipo.
    pub fn component(&self) -> &str {
        &self.component
    }
}

/// O que o componente guarda no mundo (`serialize`): os upgrades instalados e os dois slots.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UpgradeSaved {
    pub upgrades: BTreeMap<String, i32>,
    /// A ordem em que o original escreveria as chaves de `upgrades` no arquivo (o `HashMap` do `CompoundTag`, [`file_order`] sobre a tabela do tile). O NBT do Horizon é ordenado e
    /// não a guarda, então o tile a leva junto, fora do compound do original (`machinery_basis.upgrade_order`), e a leitura a usa no lugar da ordem alfabética (decisão 233, H259).
    /// Vazia num item e num NBT que não a trouxe.
    pub file_order: Vec<String>,
    pub input: SlotContents,
    pub output: SlotContents,
}

impl UpgradeSaved {
    /// Se o `serialize` do Java não escreveria nada: sem upgrade e sem item nos dois slots, o `write` do componente apaga o filho.
    pub fn is_empty(&self) -> bool {
        self.upgrades.is_empty() && self.input.is_empty() && self.output.is_empty()
    }
}

/// `tile.recalculateUpgrades(lookup, upgrade, totalInstalled)`: o tile se ajusta ao que o componente tem agora.
pub type Recalculate = Box<dyn Fn(&UpgradeComponent, &str, i32)>;

type SharedData = Rc<RefCell<Rc<UpgradeData>>>;

struct Inner {
    data: SharedData,
    /// `upgrades`.
    /// A tabela do fastutil, com a ordem de iteração do original (decisão 233).
    installed: RefCell<OpenHashMap<UpgradeKey, i32>>,
    /// `upgradeTicks`.
    upgrade_ticks: Cell<i32>,
    /// `canCheckUpgrades`: só um slot de entrada que mudou, ou uma remoção, o liga de novo.
    can_check: Rc<Cell<bool>>,
    upgrade_slot: InventorySlot,
    upgrade_output_slot: InventorySlot,
    recalculate: Recalculate,
    /// Sobe a cada mudança do que se salva, para quem salva olhar uma vez em vez de comparar tudo.
    version: Rc<Cell<u32>>,
}

/// `TileComponentUpgrade`.
#[derive(Clone)]
pub struct UpgradeComponent(Rc<Inner>);

/// `UpgradeInventorySlot.input`: não entrega a quem é de fora, e só aceita o tipo que a máquina suporta.
fn input_slot(data: &SharedData, listener: Callback) -> InventorySlot {
    let accepts: ItemTest = {
        let data = data.clone();
        Rc::new(move |resource| {
            let data = data.borrow();
            data.upgrade_of(resource)
                .is_some_and(|upgrade| data.supports(&upgrade))
        })
    };
    InventorySlot::with_checks(
        Access::NotExternal,
        Access::Any,
        SlotChecks {
            validator: Some(has_upgrade(data)),
            can_insert: Some(accepts),
            ..SlotChecks::default()
        },
        Some(listener),
        None,
    )
}

/// `UpgradeInventorySlot.output`: só quem tira à mão esvazia, e só o próprio componente enche.
fn output_slot(data: &SharedData, listener: Callback) -> InventorySlot {
    InventorySlot::with_checks(
        Access::ManualOnly,
        Access::InternalOnly,
        SlotChecks {
            validator: Some(has_upgrade(data)),
            can_insert: None,
            ..SlotChecks::default()
        },
        Some(listener),
        None,
    )
}

/// `itemType.has(IUpgradeHelper.INSTANCE.dataComponent())`.
fn has_upgrade(data: &SharedData) -> ItemTest {
    let data = data.clone();
    Rc::new(move |resource| data.borrow().upgrade_of(resource).is_some())
}

impl UpgradeComponent {
    /// `tile_changed` é o `onContentsChanged` do tile, que os dois slots avisam.
    pub fn new(data: Rc<UpgradeData>, tile_changed: Callback, recalculate: Recalculate) -> Self {
        let data: SharedData = Rc::new(RefCell::new(data));
        let can_check = Rc::new(Cell::new(true));
        let version = Rc::new(Cell::new(0_u32));
        let input_listener: Callback = {
            let (tile_changed, can_check, version) =
                (tile_changed.clone(), can_check.clone(), version.clone());
            Rc::new(move || {
                tile_changed();
                can_check.set(true);
                version.set(version.get().wrapping_add(1));
            })
        };
        let output_listener: Callback = {
            let version = version.clone();
            Rc::new(move || {
                tile_changed();
                version.set(version.get().wrapping_add(1));
            })
        };
        Self(Rc::new(Inner {
            upgrade_slot: input_slot(&data, input_listener),
            upgrade_output_slot: output_slot(&data, output_listener),
            data,
            installed: RefCell::new(OpenHashMap::new()),
            upgrade_ticks: Cell::new(0),
            can_check,
            recalculate,
            version,
        }))
    }

    /// Troca os dados quando eles recarregam.
    pub fn set_data(&self, data: Rc<UpgradeData>) {
        *self.0.data.borrow_mut() = data;
    }

    pub fn data(&self) -> Rc<UpgradeData> {
        self.0.data.borrow().clone()
    }

    pub fn upgrade_slot(&self) -> &InventorySlot {
        &self.0.upgrade_slot
    }

    pub fn upgrade_output_slot(&self) -> &InventorySlot {
        &self.0.upgrade_output_slot
    }

    /// `upgradeTicks`: o que o servidor sincroniza para quem tem a janela de upgrades aberta.
    pub fn upgrade_ticks(&self) -> i32 {
        self.0.upgrade_ticks.get()
    }

    /// `getScaledUpgradeProgress`.
    pub fn scaled_upgrade_progress(&self) -> f32 {
        self.0.upgrade_ticks.get() as f32 / UPGRADE_TICKS_REQUIRED as f32
    }

    /// `getUpgrades`.
    pub fn get_upgrades(&self, upgrade: &str) -> i32 {
        self.0
            .installed
            .borrow()
            .get(UpgradeKey::of(upgrade))
            .copied()
            .unwrap_or(0)
    }

    /// `supports`.
    pub fn supports(&self, upgrade: &str) -> bool {
        self.0.data.borrow().supports(upgrade)
    }

    /// `getInstalledTypes`, com quantos de cada.
    pub fn installed(&self) -> BTreeMap<String, i32> {
        self.installed_in_order().into_iter().collect()
    }

    /// `getInstalledTypes()` com quantos de cada, na ordem da tabela do tile (`Object2IntMaps.fastIterator`).
    pub fn installed_in_order(&self) -> Vec<(String, i32)> {
        self.0
            .installed
            .borrow()
            .entries_in_order()
            .into_iter()
            .map(|(key, total)| (key.id(), *total))
            .collect()
    }

    /// Muda quando os upgrades ou os slots mudam.
    pub fn version(&self) -> u32 {
        self.0.version.get()
    }

    fn bump(&self) {
        self.0.version.set(self.0.version.get().wrapping_add(1));
    }

    /// `getUpgradesToAdd`.
    fn upgrades_to_add(&self, upgrade: &str, installed: i32, max_available: i32) -> i32 {
        let max = self.0.data.borrow().max(upgrade).unwrap_or(0);
        if installed < max {
            (max - installed).min(max_available)
        } else {
            0
        }
    }

    /// `setUpgrades`: guarda, avisa o tile e marca para salvar.
    fn set_upgrades(&self, upgrade: &Rc<str>, total: i32) {
        self.0
            .installed
            .borrow_mut()
            .insert(UpgradeKey::of(upgrade), total);
        (self.0.recalculate)(self, upgrade, total);
        self.bump();
    }

    /// `addUpgrades`: quantos entraram. Quem chama já checou se o tipo é suportado.
    pub fn add_upgrades(&self, upgrade: &str, max_available: i32) -> i32 {
        let Some(upgrade) = self
            .0
            .data
            .borrow()
            .types
            .get_key_value(upgrade)
            .map(|(id, _)| id.clone())
        else {
            return 0;
        };
        let installed = self.get_upgrades(&upgrade);
        let to_add = self.upgrades_to_add(&upgrade, installed, max_available);
        if to_add > 0 {
            self.set_upgrades(&upgrade, installed + to_add);
            // Quem instala pelo bloco não precisa olhar o slot de novo: se ele não coube, já foi visto, e senão era de outro tipo.
            return to_add;
        }
        0
    }

    /// `removeUpgrade`: devolve ao slot de saída o que couber, um ou todos.
    pub fn remove_upgrade(&self, upgrade: &str, remove_all: bool) {
        let installed = self.get_upgrades(upgrade);
        if installed <= 0 {
            return;
        }
        let data = self.data();
        let Some(id) = data.types.get_key_value(upgrade).map(|(id, _)| id.clone()) else {
            return;
        };
        let transaction = Transaction::open_root();
        let wanted = if remove_all { installed } else { 1 };
        let removed = i32::try_from(self.0.upgrade_output_slot.insert(
            &data.resource_of(upgrade),
            u32::try_from(wanted).unwrap_or(0),
            &transaction,
            AutomationType::Internal,
        ))
        .unwrap_or(i32::MAX);
        if removed > 0 {
            // Cabe ao menos um no slot de saída: tira de verdade e põe lá.
            transaction.commit();
            if installed == removed {
                self.0.installed.borrow_mut().remove(UpgradeKey::of(&id));
                (self.0.recalculate)(self, upgrade, 0);
            } else {
                let total = installed - removed;
                self.0
                    .installed
                    .borrow_mut()
                    .insert(UpgradeKey::of(&id), total);
                (self.0.recalculate)(self, upgrade, total);
            }
            self.bump();
            // Com upgrades no slot de entrada, vale olhar de novo se eles passam.
            self.0.can_check.set(!self.0.upgrade_slot.is_empty());
        }
    }

    /// `tickServer`, sem a transação de fora, que o Java passa como `null`.
    pub fn tick_server(&self) {
        let inner = &self.0;
        if !inner.can_check.get() {
            return;
        }
        let data = self.data();
        if let Some(item_type) = inner.upgrade_slot.resource()
            && let Some(upgrade) = data.upgrade_of(&item_type)
            && data.supports(&upgrade)
        {
            let upgrades = self.get_upgrades(&upgrade);
            if upgrades < data.max(&upgrade).unwrap_or(0) {
                if inner.upgrade_ticks.get() < UPGRADE_TICKS_REQUIRED {
                    inner.upgrade_ticks.set(inner.upgrade_ticks.get() + 1);
                    return;
                } else if inner.upgrade_ticks.get() == UPGRADE_TICKS_REQUIRED {
                    let in_slot = i32::try_from(inner.upgrade_slot.amount()).unwrap_or(i32::MAX);
                    let to_add = self.upgrades_to_add(&upgrade, upgrades, in_slot);
                    if to_add > 0 {
                        let transaction = Transaction::open_root();
                        let extracted = inner.upgrade_slot.extract(
                            &item_type,
                            u32::try_from(to_add).unwrap_or(0),
                            &transaction,
                            AutomationType::Internal,
                        );
                        if extracted > 0 {
                            // Se entrou algum (mesmo menos do que se esperava) soma, e o commit tira do slot.
                            self.set_upgrades(
                                &upgrade,
                                upgrades + i32::try_from(extracted).unwrap_or(0),
                            );
                            transaction.commit();
                        }
                    }
                }
            }
        }
        inner.upgrade_ticks.set(0);
        // Dá para pular a checagem até o slot de entrada mudar.
        inner.can_check.set(false);
    }

    /// `serialize`.
    pub fn saved(&self) -> UpgradeSaved {
        let ids: Vec<String> = self
            .installed_in_order()
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        UpgradeSaved {
            upgrades: self.installed(),
            file_order: file_order(&ids),
            input: self.0.upgrade_slot.contents(),
            output: self.0.upgrade_output_slot.contents(),
        }
    }

    /// A tabela que `deserialize` (o NBT) ou `applyImplicitComponents` (o item) monta (decisão 233): `new Object2IntOpenHashMap<>(mapa)`, a tabela do tamanho do mapa e as
    /// entradas na ordem do `HashMap` do NBT; sem upgrades no NBT, `deserialize` só esvazia a tabela (`clear()`, que não encolhe), e o item sempre troca por uma nova.
    fn load_table(&self, data: &UpgradeData, saved: &UpgradeSaved, from_item: bool) {
        // O `UPGRADES_CODEC` só lê tipos que existem, com um total positivo.
        let known: Vec<(String, i32)> = saved
            .upgrades
            .iter()
            .filter(|(id, total)| data.types.contains_key(id.as_str()) && **total > 0)
            .map(|(id, total)| (id.clone(), *total))
            .collect();
        let mut installed = self.0.installed.borrow_mut();
        if from_item || !known.is_empty() {
            let entries = if from_item {
                entries_from_disk(&known)
            } else {
                entries_from_file(&known, &saved.file_order)
            };
            *installed = loaded_map(&entries);
        } else {
            installed.clear_keeping_table();
        }
    }

    /// `applyImplicitComponents` com o `UPGRADES` do item: a máquina posta de um item (os upgrades, os dois slots) recalcula cada tipo que aceita.
    pub fn restore_item(&self, saved: &UpgradeSaved) {
        self.restore_from(saved, true);
    }

    /// `deserialize`: lê os upgrades, recalcula cada tipo que a tag aceita e só então carrega os slots.
    pub fn restore(&self, saved: &UpgradeSaved) {
        self.restore_from(saved, false);
    }

    fn restore_from(&self, saved: &UpgradeSaved, from_item: bool) {
        let data = self.data();
        self.load_table(&data, saved, from_item);
        for upgrade in data.supported() {
            (self.0.recalculate)(self, upgrade, self.get_upgrades(upgrade));
        }
        self.0
            .upgrade_slot
            .set_contents(saved.input.resource.clone(), saved.input.amount, None);
        self.0.upgrade_output_slot.set_contents(
            saved.output.resource.clone(),
            saved.output.amount,
            None,
        );
        self.bump();
    }
}

/// O que o componente guarda só na memória, fora do NBT: `upgradeTicks` e `canCheckUpgrades`. Um tile com objeto vivo os tem no
/// próprio componente; um tile que se refaz do NBT a cada tick (o Heat Generator) os leva de um tick ao seguinte, e o que o Java faz ao
/// carregar o mundo (`upgradeTicks = 0` e `canCheckUpgrades = true`) é o [`Default`].
#[derive(Clone, Debug)]
pub struct UpgradeTransient {
    pub ticks: i32,
    pub can_check: bool,
    /// A tabela do fastutil com a história de inserção da sessão (decisão 233): o NBT só guarda os totais, e a ordem do original depende de como a tabela cresceu.
    pub table: Option<OpenHashMap<UpgradeKey, i32>>,
}

impl PartialEq for UpgradeTransient {
    fn eq(&self, other: &Self) -> bool {
        let entries = |table: &Option<OpenHashMap<UpgradeKey, i32>>| {
            table.as_ref().map(|table| {
                table
                    .entries_in_order()
                    .into_iter()
                    .map(|(key, total)| (key, *total))
                    .collect::<Vec<_>>()
            })
        };
        self.ticks == other.ticks
            && self.can_check == other.can_check
            && entries(&self.table) == entries(&other.table)
    }
}

impl Default for UpgradeTransient {
    fn default() -> Self {
        Self {
            ticks: 0,
            can_check: true,
            table: None,
        }
    }
}

impl UpgradeComponent {
    /// O `upgradeTicks` e o `canCheckUpgrades` de agora.
    pub fn transient(&self) -> UpgradeTransient {
        UpgradeTransient {
            ticks: self.0.upgrade_ticks.get(),
            can_check: self.0.can_check.get(),
            table: Some(self.0.installed.borrow().clone()),
        }
    }

    /// Devolve ao componente o que ele guardava só na memória.
    pub fn set_transient(&self, transient: UpgradeTransient) {
        self.0.upgrade_ticks.set(transient.ticks);
        self.0.can_check.set(transient.can_check);
        // A tabela da sessão vale enquanto tem os mesmos totais que o NBT acabou de dar: o que mudou por fora (o menu, um comando) volta pela ordem do NBT.
        if let Some(table) = transient.table {
            let same = |a: &OpenHashMap<UpgradeKey, i32>, b: &OpenHashMap<UpgradeKey, i32>| {
                let sorted = |map: &OpenHashMap<UpgradeKey, i32>| {
                    let mut entries: Vec<(String, i32)> = map
                        .entries_in_order()
                        .into_iter()
                        .map(|(key, total)| (key.id(), *total))
                        .collect();
                    entries.sort();
                    entries
                };
                sorted(a) == sorted(b)
            };
            let mut installed = self.0.installed.borrow_mut();
            if same(&table, &installed) {
                *installed = table;
            }
        }
    }
}

/// O `TileComponentUpgrade` de um tile sem objeto vivo, refeito do que o tile guarda: o NBT (`deserialize`), o que a memória levava
/// (`transient`) e, quando o menu pode ter mudado os dois slots virtuais desde o último tick, o que o menu tem
/// (`menu`: a entrada e a saída). O menu manda sobre o NBT, como o `setContents` de um clique manda sobre o slot de um objeto vivo; sem
/// menu (o tile acabou de nascer) o NBT manda. Um slot de entrada que o menu mudou volta a checar (`canCheckUpgrades`), como o
/// `UpgradeInventorySlot.input` faz pelo ouvinte dele.
pub fn rebuild(
    data: Rc<UpgradeData>,
    nbt: &UpgradeSaved,
    menu: Option<&[SlotContents; 2]>,
    transient: UpgradeTransient,
) -> UpgradeComponent {
    let component = UpgradeComponent::new(data, Rc::new(|| {}), Box::new(|_, _, _| {}));
    component.restore(nbt);
    component.set_transient(transient);
    if let Some([input, output]) = menu {
        component
            .upgrade_slot()
            .set_contents(input.resource.clone(), input.amount, None);
        component
            .upgrade_output_slot()
            .set_contents(output.resource.clone(), output.amount, None);
    }
    component
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ITEM: &str = "mod:upgrade";
    const COMPONENT: &str = "mod:upgrade_type";
    const SPEED: &str = "mod:speed";
    const ENERGY: &str = "mod:energy";
    const OTHER: &str = "mod:other";

    fn data() -> Rc<UpgradeData> {
        Rc::new(UpgradeData::from_parts(
            ITEM,
            COMPONENT,
            64,
            &[(SPEED, 8), (ENERGY, 8), (OTHER, 1)],
            &[SPEED, ENERGY],
        ))
    }

    fn upgrade(id: &str) -> ItemResource {
        data().resource_of(id)
    }

    #[test]
    fn the_type_comes_from_the_component_and_other_components_do_not_matter() {
        let data = data();
        let named = ItemResource::with_components(
            ITEM,
            64,
            Components::new([
                (COMPONENT, Components::text_value(SPEED).as_str()),
                ("minecraft:custom_name", "\"x\""),
            ]),
        );
        assert_eq!(data.upgrade_of(&named).as_deref(), Some(SPEED));
        // O recurso que a lib monta é o item com o tipo e mais nada, e é outro recurso que o item com nome.
        assert_eq!(data.upgrade_of(&upgrade(SPEED)).as_deref(), Some(SPEED));
        assert_ne!(named, upgrade(SPEED));
        // Sem o componente, com um tipo que não existe ou em outro item, não é upgrade.
        assert_eq!(data.upgrade_of(&ItemResource::new(ITEM, 64)), None);
        let unknown =
            ItemResource::with_component(ITEM, 64, COMPONENT, &Components::text_value("mod:nope"));
        assert_eq!(data.upgrade_of(&unknown), None);
        let other = ItemResource::with_component(
            "mod:other_item",
            64,
            COMPONENT,
            &Components::text_value(SPEED),
        );
        assert_eq!(data.upgrade_of(&other), None);
        // O valor que não é um texto também não.
        let number = ItemResource::with_component(ITEM, 64, COMPONENT, "5");
        assert_eq!(data.upgrade_of(&number), None);
    }

    #[test]
    fn sync_round_trips_types_and_tags() {
        let files = vec![
            (
                "mod:speed".to_owned(),
                json!({"display_name": "a", "description": "b", "color": "red", "max": 8}),
            ),
            (
                "mod:muffling".to_owned(),
                json!({"display_name": "a", "description": "b", "color": "#ff00ff"}),
            ),
        ];
        let (types, errors) = UpgradeTypes::load(&files);
        assert!(errors.is_empty());
        let tags = UpgradeTags::load(&[(
            "mod:supported/machine".to_owned(),
            json!({"values": ["mod:speed", "#mod:other"]}),
        )]);
        let (back_types, back_tags) = decode_sync(&encode_sync(&types, &tags)).unwrap();
        assert_eq!(back_types, types);
        assert_eq!(back_tags, tags);
        assert_eq!(back_types.len(), 2);
        assert_eq!(back_types.get("mod:muffling").unwrap().max, 1);
    }

    #[test]
    fn sync_refuses_garbage() {
        assert!(decode_sync(b"nope").is_none());
        assert!(decode_sync(br#"{"types":{"a:b":{"max":0,"color":"red"}},"tags":{}}"#).is_none());
        assert!(decode_sync(br#"{"types":{},"tags":{"t":[1]}}"#).is_none());
    }

    type Calls = Rc<RefCell<Vec<(String, i32)>>>;

    /// O componente com um registro do que o tile foi mandado recalcular.
    fn fresh() -> (UpgradeComponent, Calls) {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let recorded = calls.clone();
        let component = UpgradeComponent::new(
            data(),
            Rc::new(|| {}),
            Box::new(move |_, upgrade, total| {
                recorded.borrow_mut().push((upgrade.to_owned(), total));
            }),
        );
        (component, calls)
    }

    fn tick(component: &UpgradeComponent, times: i32) {
        for _ in 0..times {
            component.tick_server();
        }
    }

    #[test]
    fn installing_takes_twenty_ticks_and_then_one_more() {
        let (component, calls) = fresh();
        component
            .upgrade_slot()
            .set_contents(Some(upgrade(SPEED)), 3, None);
        tick(&component, 20);
        assert_eq!(component.get_upgrades(SPEED), 0, "ainda contando");
        assert_eq!(component.scaled_upgrade_progress(), 1.0);
        tick(&component, 1);
        assert_eq!(component.get_upgrades(SPEED), 3);
        assert!(component.upgrade_slot().is_empty());
        assert_eq!(component.scaled_upgrade_progress(), 0.0);
        assert_eq!(*calls.borrow(), [(SPEED.to_owned(), 3)]);
    }

    #[test]
    fn only_up_to_the_maximum_goes_in_and_the_rest_stays() {
        let (component, _) = fresh();
        component.add_upgrades(SPEED, 3);
        component
            .upgrade_slot()
            .set_contents(Some(upgrade(SPEED)), 10, None);
        tick(&component, 21);
        assert_eq!(component.get_upgrades(SPEED), 8);
        assert_eq!(component.upgrade_slot().amount(), 5);
        // Parado até o slot mudar: nada anda mais.
        tick(&component, 40);
        assert_eq!(component.scaled_upgrade_progress(), 0.0);
        assert_eq!(component.upgrade_slot().amount(), 5);
    }

    #[test]
    fn emptying_the_slot_halfway_resets_the_progress() {
        let (component, _) = fresh();
        component
            .upgrade_slot()
            .set_contents(Some(upgrade(ENERGY)), 2, None);
        tick(&component, 10);
        assert_eq!(component.scaled_upgrade_progress(), 0.5);
        component.upgrade_slot().set_contents(None, 0, None);
        assert_eq!(component.scaled_upgrade_progress(), 0.5, "só o tick zera");
        tick(&component, 1);
        assert_eq!(component.scaled_upgrade_progress(), 0.0);
        assert_eq!(component.get_upgrades(ENERGY), 0);
    }

    #[test]
    fn a_type_the_machine_does_not_support_stays_in_the_slot() {
        let (component, _) = fresh();
        component
            .upgrade_slot()
            .set_contents(Some(upgrade(OTHER)), 1, None);
        tick(&component, 30);
        assert_eq!(component.get_upgrades(OTHER), 0);
        assert_eq!(component.upgrade_slot().amount(), 1);
        assert_eq!(component.scaled_upgrade_progress(), 0.0);
    }

    #[test]
    fn removing_one_and_then_all_moves_them_to_the_output() {
        let (component, calls) = fresh();
        component.add_upgrades(SPEED, 8);
        component.remove_upgrade(SPEED, false);
        assert_eq!(component.get_upgrades(SPEED), 7);
        assert_eq!(component.upgrade_output_slot().amount(), 1);
        component.remove_upgrade(SPEED, true);
        assert_eq!(component.get_upgrades(SPEED), 0);
        assert!(component.installed().is_empty());
        assert_eq!(component.upgrade_output_slot().amount(), 8);
        assert_eq!(
            calls.borrow().last(),
            Some(&(SPEED.to_owned(), 0)),
            "o tile recalcula com zero"
        );
    }

    #[test]
    fn nothing_is_removed_when_the_output_holds_another_type() {
        let (component, _) = fresh();
        component.add_upgrades(SPEED, 8);
        component.add_upgrades(ENERGY, 1);
        component.remove_upgrade(SPEED, true);
        component.remove_upgrade(ENERGY, true);
        assert_eq!(component.get_upgrades(ENERGY), 1);
        assert_eq!(component.upgrade_output_slot().amount(), 8);
    }

    #[test]
    fn removing_checks_the_input_slot_again() {
        let (component, _) = fresh();
        component.add_upgrades(SPEED, 8);
        component
            .upgrade_slot()
            .set_contents(Some(upgrade(SPEED)), 2, None);
        tick(&component, 1);
        assert_eq!(component.scaled_upgrade_progress(), 0.0, "cheio: parou");
        component.remove_upgrade(SPEED, false);
        tick(&component, 21);
        assert_eq!(component.get_upgrades(SPEED), 8);
        assert_eq!(component.upgrade_slot().amount(), 1);
    }

    #[test]
    fn the_slots_follow_the_automation_rules() {
        let (component, _) = fresh();
        let transaction = Transaction::open_root();
        let input = component.upgrade_slot();
        let output = component.upgrade_output_slot();
        // Entrada: aceita o tipo suportado de qualquer um, e não entrega a quem é de fora.
        assert_eq!(
            input.insert(&upgrade(SPEED), 2, &transaction, AutomationType::External),
            2
        );
        assert_eq!(
            input.insert(&upgrade(OTHER), 1, &transaction, AutomationType::Internal),
            0
        );
        assert_eq!(
            input.extract(&upgrade(SPEED), 1, &transaction, AutomationType::External),
            0
        );
        assert_eq!(
            input.extract(&upgrade(SPEED), 1, &transaction, AutomationType::Manual),
            1
        );
        // Saída: só o componente enche e só uma mão esvazia, e um item que não é upgrade não cabe.
        assert_eq!(
            output.insert(&upgrade(SPEED), 2, &transaction, AutomationType::External),
            0
        );
        assert_eq!(
            output.insert(&upgrade(OTHER), 2, &transaction, AutomationType::Internal),
            2
        );
        assert_eq!(
            output.extract(&upgrade(OTHER), 1, &transaction, AutomationType::Internal),
            0
        );
        assert_eq!(
            output.extract(&upgrade(OTHER), 1, &transaction, AutomationType::Manual),
            1
        );
        let plain = ItemResource::new(ITEM, 64);
        assert_eq!(
            output.insert(&plain, 1, &transaction, AutomationType::Internal),
            0
        );
        assert!(!output.is_valid(&ItemResource::new("mod:clay", 64)));
    }

    #[test]
    fn the_upgrade_item_stacks_to_its_own_limit() {
        let (component, _) = fresh();
        let transaction = Transaction::open_root();
        let inserted = component.upgrade_output_slot().insert(
            &upgrade(SPEED),
            99,
            &transaction,
            AutomationType::Internal,
        );
        assert_eq!(inserted, 64);
    }

    #[test]
    fn saving_and_reopening_keeps_the_upgrades_and_the_slots() {
        let (component, _) = fresh();
        component.add_upgrades(SPEED, 5);
        component.add_upgrades(ENERGY, 2);
        component.remove_upgrade(ENERGY, false);
        component
            .upgrade_slot()
            .set_contents(Some(upgrade(SPEED)), 7, None);
        let saved = component.saved();
        assert_eq!(saved.upgrades.get(SPEED), Some(&5));
        assert_eq!(saved.upgrades.get(ENERGY), Some(&1));
        assert_eq!(saved.input.amount, 7);
        assert_eq!(saved.output.amount, 1);

        let (reborn, calls) = fresh();
        reborn.restore(&saved);
        assert_eq!(reborn.saved(), saved);
        // O tile recalcula cada tipo da tag, com o total lido.
        assert_eq!(
            *calls.borrow(),
            [(ENERGY.to_owned(), 1), (SPEED.to_owned(), 5)]
        );
    }

    #[test]
    fn a_saved_type_that_no_longer_exists_is_dropped() {
        let (component, _) = fresh();
        let mut saved = UpgradeSaved::default();
        saved.upgrades.insert("mod:gone".to_owned(), 3);
        saved.upgrades.insert(SPEED.to_owned(), 0);
        component.restore(&saved);
        assert!(component.installed().is_empty());
    }

    #[test]
    fn types_and_tags_come_from_data() {
        let (types, errors) = UpgradeTypes::load(&[
            (
                SPEED.to_owned(),
                json!({"color": "red", "description": {}, "display_name": {}, "max": 8}),
            ),
            (
                OTHER.to_owned(),
                json!({"color": "cyan", "description": {}, "display_name": {}}),
            ),
            (
                "mod:bad".to_owned(),
                json!({"color": "red", "description": {}, "display_name": {}, "max": 0}),
            ),
            (
                "mod:nocolor".to_owned(),
                json!({"description": {}, "display_name": {}}),
            ),
        ]);
        assert_eq!(types.len(), 2);
        assert_eq!(types.get(OTHER).map(|t| t.max), Some(1), "max padrão");
        assert_eq!(
            errors,
            [
                ("mod:bad".to_owned(), UpgradeError::BadMax),
                (
                    "mod:nocolor".to_owned(),
                    UpgradeError::MissingField("color")
                ),
            ]
        );
        let tags = UpgradeTags::load(&[
            ("mod:simple".to_owned(), json!({"values": [SPEED]})),
            (
                "mod:default".to_owned(),
                json!({"values": ["#mod:simple", OTHER, "#mod:default"]}),
            ),
        ]);
        let data = UpgradeData::new(ITEM, COMPONENT, 64, &types, &tags, "mod:default");
        assert!(data.supports(SPEED) && data.supports(OTHER));
        assert!(!data.supports(ENERGY));
        assert_eq!(data.max(SPEED), Some(8));
    }
    /// O que um tile vivo e um tile refeito do NBT a cada tick guardam, tick a tick: os upgrades, os dois slots, o progresso e o
    /// que a memória leva. `menu_edit` mexe no que o menu mostra antes do tick `at` (um clique), e `act` age nos dois componentes
    /// (um pacote de remoção).
    fn run_both(
        ticks: i32,
        menu_edit: &dyn Fn(i32, &mut [SlotContents; 2]),
        act: &dyn Fn(i32, &UpgradeComponent),
    ) {
        let (live, _) = fresh();
        let mut nbt = UpgradeSaved::default();
        let mut transient = UpgradeTransient::default();
        // O que o menu do jogo mostra: os dois slots virtuais.
        let mut menu = [SlotContents::default(), SlotContents::default()];
        for at in 0..ticks {
            menu_edit(at, &mut menu);
            // O tile vivo vê o menu pelo `setContents`; o refeito, pelo `menu` de `rebuild`.
            live.upgrade_slot()
                .set_contents(menu[0].resource.clone(), menu[0].amount, None);
            live.upgrade_output_slot()
                .set_contents(menu[1].resource.clone(), menu[1].amount, None);
            let rebuilt = rebuild(data(), &nbt, Some(&menu), transient);
            act(at, &live);
            act(at, &rebuilt);
            live.tick_server();
            rebuilt.tick_server();
            assert_eq!(rebuilt.saved(), live.saved(), "tick {at}: o que se salva");
            assert_eq!(
                rebuilt.transient(),
                live.transient(),
                "tick {at}: a memória"
            );
            nbt = rebuilt.saved();
            transient = rebuilt.transient();
            menu = [nbt.input.clone(), nbt.output.clone()];
        }
    }

    #[test]
    fn a_tile_rebuilt_every_tick_installs_like_a_live_one() {
        // Entrada com 3 de velocidade: 20 ticks contando e o 21º instala; depois um tipo que o tile não aceita; depois a remoção.
        run_both(
            90,
            &|at, menu| match at {
                3 => menu[0] = SlotContents::new(Some(upgrade(SPEED)), 3),
                40 => menu[0] = SlotContents::new(Some(upgrade(OTHER)), 2),
                55 => menu[0] = SlotContents::default(),
                _ => {}
            },
            &|at, component| {
                if at == 60 {
                    component.remove_upgrade(SPEED, false);
                }
            },
        );
    }

    #[test]
    fn a_rebuilt_tile_finishes_the_install_only_because_the_memory_is_carried() {
        let mut nbt = UpgradeSaved::default();
        let mut transient = UpgradeTransient::default();
        let mut menu = [
            SlotContents::new(Some(upgrade(SPEED)), 3),
            SlotContents::default(),
        ];
        for tick in 0..20 {
            let rebuilt = rebuild(data(), &nbt, Some(&menu), transient);
            rebuilt.tick_server();
            assert_eq!(rebuilt.get_upgrades(SPEED), 0, "tick {tick}");
            nbt = rebuilt.saved();
            transient = rebuilt.transient();
            menu = [nbt.input.clone(), nbt.output.clone()];
        }
        assert_eq!(transient.ticks, 20);
        let rebuilt = rebuild(data(), &nbt, Some(&menu), transient);
        rebuilt.tick_server();
        assert_eq!(rebuilt.get_upgrades(SPEED), 3);
        assert_eq!(
            rebuilt.transient(),
            UpgradeTransient {
                ticks: 0,
                can_check: false,
                table: rebuilt.transient().table
            }
        );
        // Sem levar a memória, cada tick recomeça e nada instala.
        let forgetful = rebuild(data(), &nbt, Some(&menu), UpgradeTransient::default());
        forgetful.tick_server();
        assert_eq!(forgetful.upgrade_ticks(), 1);
    }

    #[test]
    fn the_menu_wins_over_the_nbt_and_a_new_tile_takes_the_nbt() {
        let nbt = UpgradeSaved {
            input: SlotContents::new(Some(upgrade(SPEED)), 5),
            ..UpgradeSaved::default()
        };
        // Sem menu (o tile acabou de nascer), o slot é o do NBT.
        let born = rebuild(data(), &nbt, None, UpgradeTransient::default());
        assert_eq!(born.upgrade_slot().amount(), 5);
        // Com menu, o que o jogador deixou no slot manda, e um slot de entrada que mudou volta a checar.
        let menu = [
            SlotContents::new(Some(upgrade(ENERGY)), 2),
            SlotContents::default(),
        ];
        let still = UpgradeTransient {
            ticks: 0,
            can_check: false,
            table: None,
        };
        let edited = rebuild(data(), &nbt, Some(&menu), still.clone());
        assert_eq!(edited.upgrade_slot().resource(), Some(upgrade(ENERGY)));
        assert!(edited.transient().can_check);
        // O menu igual ao NBT não mexe na memória.
        let same = [nbt.input.clone(), nbt.output.clone()];
        let quiet = rebuild(data(), &nbt, Some(&same), still.clone());
        let after = quiet.transient();
        assert_eq!(
            (after.ticks, after.can_check),
            (still.ticks, still.can_check)
        );
    }
}
