// As máquinas do Mekanism no jogo. As regras ficam em mekanism_rules, e o que é genérico em
// machinery_basis.
use crate::game::mekanism_rules as rules;
use crate::game::{Game, machinery_basis as basis};
use horizon_client::containers::{ModBlockEntity, ModContainer, ModEnergyBuffer};
use horizon_client::hud::HudBatch;
use horizon_client::items::ItemStack;
use horizon_client::lang;
use horizon_client::mod_api::NativeScreen;
use horizon_world::nbt::{Compound, JavaString, Tag};
use machinery_basis_core::cached_recipe::{ItemIngredient, ItemOutput, ItemToItemRecipe};
use machinery_basis_core::containers::{InventorySlot, ItemResource};
use machinery_basis_core::data_slots;
use machinery_basis_core::item_handler::{ItemCapabilities, ItemHandler, RuledHandler, VanillaContainer};
use machinery_basis_core::sides::Face;
use machinery_basis_core::resource;
use machinery_basis_core::tank::Action;
use machinery_basis_core::tile::{ElectricMachine, RECIPE_CHECK_FREQUENCY, RedstoneControl, SavedState};
use machinery_basis_core::units::format_energy;
use basis::RecipeSource::{Cooking, Custom};
use std::cell::{Cell, RefCell};
use horizon_core::geometry::Direction;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const TANKS: [u64; 1] = [rules::TANK_CAPACITY];
const DUMP_BUTTON: i32 = 0;
const INFUSING: basis::RecipeSource = Custom(rules::INFUSING, rules::INFUSING_FIELDS);
const INFUSER: basis::RecipeMachine = basis::RecipeMachine {
    recipes: INFUSING,
    ticks: rules::TICKS_REQUIRED,
    energy_per_tick: rules::INFUSER_USAGE,
    input: rules::INPUT_SLOT,
    output: rules::OUTPUT_SLOT,
};

/// Uma máquina elétrica: as três só mudam no id, nas receitas e no consumo.
struct Electric {
    block: &'static str,
    title: &'static str,
    machine: basis::RecipeMachine,
}

const fn electric(
    block: &'static str,
    title: &'static str,
    recipes: basis::RecipeSource,
    energy_per_tick: i32,
) -> Electric {
    Electric {
        block,
        title,
        machine: basis::RecipeMachine {
            recipes,
            ticks: rules::TICKS_REQUIRED,
            energy_per_tick,
            input: rules::ELECTRIC_INPUT_SLOT,
            output: rules::ELECTRIC_OUTPUT_SLOT,
        },
    }
}

static ELECTRIC_MACHINES: [Electric; 3] = [
    electric(
        rules::ENRICHMENT_CHAMBER,
        "container.mekanism.enrichment_chamber",
        Custom(rules::ENRICHING, rules::ELECTRIC_FIELDS),
        rules::ENRICHMENT_USAGE,
    ),
    electric(
        rules::CRUSHER,
        "container.mekanism.crusher",
        Custom(rules::CRUSHING, rules::ELECTRIC_FIELDS),
        rules::CRUSHER_USAGE,
    ),
    electric(
        rules::ENERGIZED_SMELTER,
        "container.mekanism.energized_smelter",
        Cooking(rules::SMELTING),
        rules::SMELTER_USAGE,
    ),
];

// Os data slots das telas.
const DATA_ENERGY: usize = 0;
const DATA_INFUSION: usize = 2;
const DATA_COLOR: usize = 3;
const DATA_PROGRESS: usize = 5;
const DATA_SLOTS: usize = 6;
const ELECTRIC_DATA_PROGRESS: usize = 2;
const ELECTRIC_DATA_SLOTS: usize = 3;

// As telas, nas posições de GuiMetallurgicInfuser e GuiElectricMachine. Um slot guarda a posição do
// item, e a moldura fica um pixel antes.
const WIDTH: f32 = 176.0;
const HEIGHT: f32 = 166.0;
const SLOT: &str = "mekanism:gui/sprites/slot/normal";
const ENERGY_SLOT_FRAME: &str = "mekanism:gui/sprites/slot/power";
const PROGRESS_EMPTY: &str = "mekanism:gui/sprites/progress/right/empty";
const PROGRESS_FULL: &str = "mekanism:gui/sprites/progress/right/full";
const BAR_EMPTY: &str = "mekanism:gui/sprites/progress/bar/empty";
const BAR_FULL: &str = "mekanism:gui/sprites/progress/bar/full";
const UP_ARROW: &str = "mekanism:gui/sprites/arrow/up";
const DUMP_SPRITE: &str = "mekanism:gui/sprites/button/dump";
const PANEL: basis::PanelStyle = basis::PanelStyle {
    base: "mekanism:gui/sprites/base",
    base_size: 16.0,
    base_border: 4.0,
    slot: SLOT,
};
const SLOT_SIZE: f32 = 18.0;
const SLOT_POSITIONS: [[f32; 2]; rules::SLOTS] = [[17.0, 35.0], [51.0, 43.0], [109.0, 43.0], [143.0, 35.0]];
const ELECTRIC_SLOT_POSITIONS: [[f32; 2]; rules::ELECTRIC_SLOTS] = [[64.0, 17.0], [116.0, 35.0], [64.0, 53.0]];
const INFUSION_BAR: [f32; 4] = [7.0, 15.0, 6.0, 54.0];
const ENERGY_BAR: [f32; 4] = [164.0, 15.0, 6.0, 54.0];
const ELECTRIC_ENERGY_BAR: [f32; 4] = [164.0, 16.0, 6.0, 54.0];
const PROGRESS_BAR: [f32; 4] = [72.0, 47.0, 32.0, 8.0];
const ELECTRIC_PROGRESS_BAR: [f32; 4] = [86.0, 38.0, 25.0, 9.0];
const ELECTRIC_ARROW: [f32; 4] = [68.0, 38.0, 8.0, 10.0];
const DUMP_RECT: [f32; 4] = [16.0, 59.0, 21.0, 10.0];
const ENERGY_COLOR: u32 = 0xFF_1E_B4_32;
const OPAQUE: u32 = 0xFF_00_00_00;

fn is_infuser(block: &str) -> bool {
    block == rules::INFUSER
}

fn electric_machine(block: &str) -> Option<&'static Electric> {
    ELECTRIC_MACHINES.iter().find(|machine| machine.block == block)
}

fn is_machine(block: &str) -> bool {
    is_infuser(block) || electric_machine(block).is_some()
}

fn is_cable(block: &str) -> bool {
    rules::cable_capacity(block).is_some()
}

fn inside(rect: [f32; 4], at: [f32; 2]) -> bool {
    let [x, y, w, h] = rect;
    (x..x + w).contains(&at[0]) && (y..y + h).contains(&at[1])
}

fn sprite_rect(game: &Game<'_>, batch: &mut HudBatch, sprite: &str, rect: [f32; 4]) {
    let [_, _, w, h] = rect;
    game.native_sprite(batch, sprite, [0.0, 0.0, w, h], rect);
}

pub(in crate::game) fn block_entity(block: &str, entity: &mut Option<(String, Compound)>) {
    if (is_machine(block) || is_cable(block)) && entity.is_none() {
        *entity = Some((block.to_owned(), Compound::default()));
    }
}

pub(in crate::game) fn ticks(block: &str, ticks: &mut bool) {
    if is_machine(block) || is_cable(block) {
        *ticks = true;
    }
}

pub(in crate::game) fn energy(block: &str, energy: &mut Option<ModEnergyBuffer>) {
    if let Some(capacity) = rules::cable_capacity(block) {
        *energy = Some(ModEnergyBuffer {
            capacity,
            max_insert: capacity,
            insert_faces: [true; 6],
            ..Default::default()
        });
        return;
    }
    let usage = match electric_machine(block) {
        Some(electric) => electric.machine.energy_per_tick,
        None if is_infuser(block) => rules::INFUSER_USAGE,
        None => return,
    };
    // `LightConfigInfo.INPUT_ONLY`: energia entra por todas as faces.
    *energy = Some(ModEnergyBuffer {
        capacity: rules::energy_capacity(usage),
        max_insert: rules::energy_capacity(usage),
        insert_faces: [true; 6],
        ..Default::default()
    });
}

pub(in crate::game) fn container(block: &str, container: &mut Option<ModContainer>) {
    let slots = match electric_machine(block) {
        Some(_) => rules::ELECTRIC_SLOTS,
        None if is_infuser(block) => rules::SLOTS,
        None => return,
    };
    // Sem interface vanilla: hoppers passam pela regra da machinery_basis.
    *container = Some(ModContainer {
        slots,
        menu: Some(block.to_owned()),
        ..Default::default()
    });
}

fn native_screen(positions: &[[f32; 2]]) -> NativeScreen {
    NativeScreen {
        width: WIDTH,
        height: HEIGHT,
        slots: positions.iter().map(|&position| Some(position)).collect(),
        inventory_y: 84.0,
        inventory_x: 8.0,
        own_labels: true,
        own_background: true,
    }
}

pub(in crate::game) fn screen(block: &str, screen: &mut Option<NativeScreen>) {
    if is_infuser(block) {
        *screen = Some(native_screen(&SLOT_POSITIONS));
    } else if electric_machine(block).is_some() {
        *screen = Some(native_screen(&ELECTRIC_SLOT_POSITIONS));
    }
}

fn tick_infuser(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    let recipes = basis::machine_recipes(game, &INFUSING);
    let conversions = basis::conversions(game, rules::CHEMICAL_CONVERSION, &rules::CONVERSION_FIELDS);
    let mut tanks = basis::load_tanks(entity, &TANKS);
    let tank = &mut tanks[0];

    let input = entity.items[rules::INPUT_SLOT].clone();
    let usable = |infusion: &str| basis::is_recipe_resource(&recipes, infusion, input.as_ref());
    basis::convert_into_tank(entity, rules::INFUSION_SLOT, tank, &conversions, &usable);
    let (step, progress) = basis::run_machine(game, entity, Some(&mut *tank), &INFUSER);

    let front = basis::front(game, pos);
    let held = tank.resource().map(str::to_owned);
    let accepts = |_: &Game<'_>, slot: usize, stack: &ItemStack| match slot {
        rules::INPUT_SLOT => basis::is_recipe_input(&recipes, stack, held.as_deref()),
        rules::INFUSION_SLOT => conversions.iter().any(|conversion| conversion.item.test(stack)),
        _ => false,
    };
    let slots = |face| rules::item_slots(front, face);
    let sides = rules::sides(front, rules::item_slots);
    basis::tick_hoppers(game, pos, entity, &sides, &slots, &accepts);
    let lit = if step.active { "true" } else { "false" };
    game.native_set_block_property(pos, "lit", lit);

    let color = held.and_then(|id| resource::current().get(&id).map(|infusion| infusion.color));
    let mut data = vec![0; DATA_SLOTS];
    let energy = data_slots::split(basis::stored_energy(entity) as u32);
    data[DATA_ENERGY] = energy[0];
    data[DATA_ENERGY + 1] = energy[1];
    data[DATA_INFUSION] = tank.amount() as i32;
    let color = data_slots::split(color.unwrap_or(0));
    data[DATA_COLOR] = color[0];
    data[DATA_COLOR + 1] = color[1];
    data[DATA_PROGRESS] = progress as i32;
    entity.data = data;
    basis::save_tanks(entity, &tanks);
}

// Os objetos vivos da Enrichment Chamber (o desenho do Java, com contêineres e receita em cache como objetos).
//
// Moram aqui, numa tabela por thread, uma por block entity, na chave (dimensão, posição). O Horizon entrega a block entity
// por `&mut` só durante o tick, e `ModBlockEntity` guarda `Items`, `nbt` e `data`, sem lugar para um objeto vivo: então o
// objeto é um cache, e a verdade que sobrevive é a `ModBlockEntity`.
//
// - Nasce no primeiro tick do bloco, ou no primeiro depois de uma lacuna (o chunk saiu, o mundo foi salvo e reaberto, o bloco
//   foi trocado): refeito dos itens, da energia e do progresso que a `ModBlockEntity` tem. É o `deserialize` do Java.
// - Vive enquanto o tick seguinte for o do tick seguinte do jogo. Todo tick começa lendo da `ModBlockEntity` o que mudou
//   fora dele (a tela, um hopper, um cabo), como um `setContents` do Java, que dispara os listeners; e termina escrevendo o
//   que mudou, em tags simples com os nomes que o Java grava. Salvar, abrir a tela e mandar energia funcionam sem o objeto saber.
// - Morre pela varredura: sem andar por `STALE_TICKS`, o que acontece com bloco quebrado ou chunk descarregado, o objeto sai
//   da tabela. Um bloco novo na mesma posição não herda nada: sem a chave `redstone` na `ModBlockEntity` (o Java a grava sempre), nasce outro.
thread_local! {
    static CHAMBERS: RefCell<HashMap<(&'static str, [i32; 3]), Live>> = RefCell::new(HashMap::new());
    static LAST_SWEEP: Cell<i64> = const { Cell::new(i64::MIN) };
}

/// Os nomes de `SerializationConstants` que o Java grava, em tags simples no NBT da `ModBlockEntity`. `REDSTONE` é a
/// marca de que o tile já foi salvo: o Java a escreve sempre.
const REDSTONE: &str = "redstone";
const CONTROL_TYPE: &str = "control_type";
const ACTIVE_STATE: &str = "active_state";
const UPDATE_DELAY: &str = "update_delay";
const CURRENT_REDSTONE: &str = "current_redstone";
const PROGRESS: &str = "progress";
const RECIPES_USED: &str = "recipes_used";
const LAST_RECIPE_USED: &str = "last_recipe_used";
const LAST_RECIPE_AMOUNT: &str = "last_recipe_amount";
/// `SerializationConstants.EJECT + TransmissionType.ITEM.ordinal()`: ENERGY, FLUID, CHEMICAL, ITEM.
const EJECT_ITEM: &str = "eject3";
/// A chave que liga o ejetor por comando: `/data merge block x y z {mekanism_eject:1}`.
const EJECT_SWITCH: &str = "mekanism_eject";
/// De quanto em quanto tempo a tabela é varrida, e quanto sem andar um objeto vive.
const SWEEP_TICKS: i64 = 200;
const STALE_TICKS: i64 = 100;
/// Quando o item não é conhecido, a pilha dele.
const UNKNOWN_STACK: u32 = 64;

struct Live {
    machine: ElectricMachine,
    /// As receitas de onde as da máquina saíram: uma carga nova de dados as troca.
    recipes: Arc<Vec<basis::MachineRecipe>>,
    last_seen: i64,
    /// O que já está no NBT, para só escrever quando muda.
    saved: Option<SavedState>,
    used_version: Option<u32>,
    /// O `lit` que o bloco tem, para só escrever quando muda.
    lit: Option<bool>,
    /// A face de saída do ejetor, a direita da frente.
    eject_side: Option<Face>,
}

fn resource_of(game: &Game<'_>, stack: &ItemStack) -> ItemResource {
    let limit = game.native_max_stack_size(&stack.item).unwrap_or(UNKNOWN_STACK);
    ItemResource::new(&stack.item, limit)
}

/// `ItemToItemRecipe` das receitas do jogo. O ingrediente do Horizon (item ou tag) vira o predicado da lib, e guarda a
/// última resposta, porque a receita em cache pergunta todo tick pelo mesmo item.
fn core_recipes(game: &Game<'_>, recipes: &Arc<Vec<basis::MachineRecipe>>) -> Vec<Rc<ItemToItemRecipe>> {
    let mut converted = Vec::with_capacity(recipes.len());
    for (index, recipe) in recipes.iter().enumerate() {
        let source = recipes.clone();
        let last: RefCell<Option<(Rc<str>, bool)>> = RefCell::new(None);
        let input = ItemIngredient::matching(move |resource| {
            if let Some((id, answer)) = &*last.borrow()
                && *id == resource.id
            {
                return *answer;
            }
            let stack = ItemStack {
                item: Arc::from(&*resource.id),
                count: 1,
                damage: 0,
                extra: None,
            };
            let answer = source[index].item.test(&stack);
            *last.borrow_mut() = Some((resource.id.clone(), answer));
            answer
        });
        converted.push(Rc::new(ItemToItemRecipe {
            id: recipe.id.clone(),
            input,
            input_count: recipe.count,
            output: ItemOutput {
                resource: resource_of(game, &recipe.output),
                count: recipe.output.count,
            },
        }));
    }
    converted
}

fn has_saved(entity: &ModBlockEntity) -> bool {
    entity.nbt.contains_key(&JavaString::from(REDSTONE))
}

fn nbt_int(entity: &ModBlockEntity, key: &str) -> i32 {
    match entity.nbt.get(&JavaString::from(key)) {
        Some(Tag::Int(value)) => *value,
        _ => 0,
    }
}

fn nbt_bool(entity: &ModBlockEntity, key: &str) -> bool {
    matches!(entity.nbt.get(&JavaString::from(key)), Some(Tag::Byte(value)) if *value != 0)
}

fn nbt_string(entity: &ModBlockEntity, key: &str) -> Option<String> {
    match entity.nbt.get(&JavaString::from(key)) {
        Some(Tag::String(value)) => Some(value.to_string_lossy()),
        _ => None,
    }
}

/// `loadAdditional` do `TileEntityMekanism` e do monitor, do que a `ModBlockEntity` guarda.
fn read_saved(entity: &ModBlockEntity) -> SavedState {
    let mut recipes_used = BTreeMap::new();
    if let Some(Tag::Compound(used)) = entity.nbt.get(&JavaString::from(RECIPES_USED)) {
        for (id, amount) in used {
            if let Tag::Int(amount) = amount {
                recipes_used.insert(id.to_string_lossy(), *amount);
            }
        }
    }
    SavedState {
        redstone: nbt_bool(entity, REDSTONE),
        control_type: RedstoneControl::from_serialized_name(
            &nbt_string(entity, CONTROL_TYPE).unwrap_or_default(),
        ),
        active: nbt_bool(entity, ACTIVE_STATE),
        update_delay: nbt_int(entity, UPDATE_DELAY),
        current_redstone: nbt_int(entity, CURRENT_REDSTONE),
        progress: nbt_int(entity, PROGRESS),
        recipes_used,
        last_recipe_used: nbt_string(entity, LAST_RECIPE_USED),
        last_recipe_amount: nbt_int(entity, LAST_RECIPE_AMOUNT),
        ejecting: nbt_bool(entity, EJECT_ITEM),
    }
}

fn put(entity: &mut ModBlockEntity, key: &str, tag: Tag) {
    entity.nbt.insert(JavaString::from(key), tag);
}

/// `saveAdditional` do que muda a cada tick, só dos campos que mudaram desde `old`.
fn write_scalars(entity: &mut ModBlockEntity, old: Option<&SavedState>, new: &SavedState) {
    let changed = |same: fn(&SavedState, &SavedState) -> bool| old.is_none_or(|old| !same(old, new));
    if changed(|a, b| a.redstone == b.redstone) {
        put(entity, REDSTONE, Tag::Byte(i8::from(new.redstone)));
    }
    if changed(|a, b| a.control_type == b.control_type) {
        let name = JavaString::from(new.control_type.serialized_name());
        put(entity, CONTROL_TYPE, Tag::String(name));
    }
    if changed(|a, b| a.active == b.active) {
        put(entity, ACTIVE_STATE, Tag::Byte(i8::from(new.active)));
    }
    if changed(|a, b| a.update_delay == b.update_delay) {
        put(entity, UPDATE_DELAY, Tag::Int(new.update_delay));
    }
    if changed(|a, b| a.current_redstone == b.current_redstone) {
        put(entity, CURRENT_REDSTONE, Tag::Int(new.current_redstone));
    }
    if changed(|a, b| a.progress == b.progress) {
        put(entity, PROGRESS, Tag::Int(new.progress));
    }
    if changed(|a, b| a.ejecting == b.ejecting) {
        put(entity, EJECT_ITEM, Tag::Byte(i8::from(new.ejecting)));
    }
}

/// `saveAdditional` do monitor: as receitas usadas, a última e quanto dela saiu. Como no Java, a quantidade só vai se
/// for maior que zero, e a última só se existe.
fn write_used(entity: &mut ModBlockEntity, new: &SavedState) {
    let mut used = Compound::default();
    for (id, amount) in &new.recipes_used {
        used.insert(JavaString::from(id.as_str()), Tag::Int(*amount));
    }
    put(entity, RECIPES_USED, Tag::Compound(used));
    match &new.last_recipe_used {
        Some(id) => put(entity, LAST_RECIPE_USED, Tag::String(JavaString::from(id.as_str()))),
        None => {
            entity.nbt.remove(&JavaString::from(LAST_RECIPE_USED));
        }
    }
    if new.last_recipe_amount > 0 {
        put(entity, LAST_RECIPE_AMOUNT, Tag::Int(new.last_recipe_amount));
    } else {
        entity.nbt.remove(&JavaString::from(LAST_RECIPE_AMOUNT));
    }
}

/// `ElectricMachine` nova, do que a `ModBlockEntity` guarda.
fn born(
    game: &Game<'_>,
    pos: [i32; 3],
    entity: &ModBlockEntity,
    recipes: &Arc<Vec<basis::MachineRecipe>>,
) -> Live {
    // `ThreadLocalRandom.nextInt(RECIPE_CHECK_FREQUENCY)`: o sorteio de `shouldRecheckAllErrors`.
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| u64::from(since.subsec_nanos()));
    let config = rules::chamber_config(core_recipes(game, recipes), nanos % RECIPE_CHECK_FREQUENCY);
    let mut machine = ElectricMachine::new(config);
    let block_active = game.native_block_property(pos, "lit").as_deref() == Some("true");
    machine.restore(&read_saved(entity), block_active);
    Live {
        machine,
        recipes: recipes.clone(),
        last_seen: i64::MIN,
        saved: None,
        used_version: None,
        lit: None,
        eject_side: None,
    }
}

/// O que a `ModBlockEntity` mudou fora do tick vira `setContents` no slot, que dispara os listeners como o Java.
fn slot_from_entity(game: &Game<'_>, slot: &InventorySlot, stack: Option<&ItemStack>) {
    match stack {
        None => slot.set_contents(None, 0, None),
        Some(stack) => {
            let same = slot.resource().is_some_and(|held| *held.id == *stack.item) && slot.amount() == stack.count;
            if !same {
                slot.set_contents(Some(resource_of(game, stack)), stack.count, None);
            }
        }
    }
}

/// O que o tick mudou vai para a `ModBlockEntity`. Uma pilha do mesmo item guarda o que tinha (dano, componentes); um item
/// novo é o da saída de uma receita.
fn slot_to_entity(entity: &mut ModBlockEntity, index: usize, slot: &InventorySlot, recipes: &[basis::MachineRecipe]) {
    let contents = slot.contents();
    let current = entity.items[index].as_ref();
    let same = match (&contents.resource, current) {
        (None, None) => true,
        (Some(resource), Some(stack)) => *resource.id == *stack.item && contents.amount == stack.count,
        _ => false,
    };
    if same {
        return;
    }
    let new = contents.resource.map(|resource| {
        let mut stack = match current {
            Some(old) if *old.item == *resource.id => old.clone(),
            _ => recipes
                .iter()
                .find(|recipe| *recipe.output.item == *resource.id)
                .map_or_else(
                    || ItemStack {
                        item: Arc::from(&*resource.id),
                        count: 1,
                        damage: 0,
                        extra: None,
                    },
                    |recipe| recipe.output.clone(),
                ),
        };
        stack.count = contents.amount;
        stack
    });
    entity.items[index] = new;
}

/// Os vizinhos como o ejetor os vê: o inventário do outro lado, copiado do jogo, com a regra real de cada slot para o item que
/// está saindo. O Horizon não simula inserção, então o diário guarda a cópia, e o que entrou nela vai ao vizinho de verdade, por
/// `native_insert_item`, depois que a transação commitou. O ejetor roda antes da receita: só o item da saída pode sair no tick.
struct GameNeighbours<'a, 'g> {
    game: &'a Game<'g>,
    dimension: &'static str,
    pos: [i32; 3],
    offered: Option<(Rc<str>, ItemStack)>,
    created: RefCell<Vec<CopiedContainer>>,
}

struct CopiedContainer {
    at: [i32; 3],
    reached: Direction,
    container: Rc<VanillaContainer>,
    original: Vec<(usize, ItemResource, u32)>,
}

impl ItemCapabilities for GameNeighbours<'_, '_> {
    fn item_handler(&self, face: Face) -> Option<Rc<dyn ItemHandler>> {
        let [x, y, z] = face.offset();
        let at = [self.pos[0] + x, self.pos[1] + y, self.pos[2] + z];
        let reached = basis::direction(face.opposite());
        let slots = self.game.native_container_slots(self.dimension, at, reached)?;
        let container = VanillaContainer::new(slots.len(), Rc::new(|| {}));
        for (index, slot) in slots.iter().enumerate() {
            if let Some(stack) = &slot.stack {
                container.set_item(index, Some(resource_of(self.game, stack)), stack.count);
            }
        }
        let original = container.stacks();
        self.created.borrow_mut().push(CopiedContainer {
            at,
            reached,
            container: container.clone(),
            original,
        });
        let (offered, accepted) = match &self.offered {
            Some((id, stack)) => (
                Some(id.clone()),
                slots
                    .iter()
                    .map(|slot| self.game.machinery_slot_accepts(at, slot.slot, stack, reached))
                    .collect(),
            ),
            None => (None, Vec::new()),
        };
        Some(Rc::new(RuledHandler::new(container, move |index, resource: &ItemResource| {
            offered.as_ref() == Some(&resource.id) && accepted.get(index).copied().unwrap_or(false)
        })))
    }
}

/// O que entrou nas cópias durante o tick: por vizinho, quanto de cada item.
struct Ejected {
    at: [i32; 3],
    reached: Direction,
    id: Rc<str>,
    count: u32,
}

impl GameNeighbours<'_, '_> {
    fn ejected(self) -> Vec<Ejected> {
        let mut ejected = Vec::new();
        for copied in self.created.into_inner() {
            for (index, resource, amount) in copied.container.stacks() {
                let before = copied
                    .original
                    .iter()
                    .find(|(slot, held, _)| *slot == index && *held == resource)
                    .map_or(0, |(_, _, amount)| *amount);
                if amount > before {
                    ejected.push(Ejected {
                        at: copied.at,
                        reached: copied.reached,
                        id: resource.id.clone(),
                        count: amount - before,
                    });
                }
            }
        }
        ejected
    }
}

/// A pilha de um item que sai da máquina: a da receita, com dano e componentes, ou a do id.
fn stack_of(recipes: &[basis::MachineRecipe], id: &str) -> ItemStack {
    let template = recipes.iter().find(|recipe| *recipe.output.item == *id);
    template.map_or_else(
        || ItemStack {
            item: Arc::from(id),
            count: 1,
            damage: 0,
            extra: None,
        },
        |recipe| recipe.output.clone(),
    )
}

/// Entrega ao vizinho de verdade o que o diário mandou. A regra de cada slot já foi perguntada e nada mudou no vizinho desde
/// então, então ele aceita tudo. O resto, se houver, volta ao slot de saída da `ModBlockEntity`.
fn deliver(game: &mut Game<'_>, entity: &mut ModBlockEntity, recipes: &[basis::MachineRecipe], ejected: Vec<Ejected>) {
    for sent in ejected {
        let mut stack = stack_of(recipes, &sent.id);
        stack.count = sent.count;
        let Some(rest) = game.native_insert_item(sent.at, sent.reached, stack) else {
            continue;
        };
        let slot = &mut entity.items[rules::ELECTRIC_OUTPUT_SLOT];
        match slot.as_mut() {
            None => *slot = Some(rest),
            Some(held) if held.item == rest.item => held.count += rest.count,
            Some(_) => eprintln!(
                "mekanism: {} x{} recusados pelo vizinho se perderam",
                rest.item, rest.count
            ),
        }
    }
}

/// O baú como o rastro o grava, lido do jogo: `[slot, id, quantidade]` dos slots com algo.
fn chest_of(game: &Game<'_>, dimension: &'static str, pos: [i32; 3], face: Face) -> Vec<(usize, String, u32)> {
    let [x, y, z] = face.offset();
    let at = [pos[0] + x, pos[1] + y, pos[2] + z];
    let reached = basis::direction(face.opposite());
    let slots = game.native_container_slots(dimension, at, reached).unwrap_or_default();
    slots
        .into_iter()
        .filter_map(|slot| {
            let stack = slot.stack?;
            let path = stack.item.split_once(':').map_or(&*stack.item, |(_, path)| path).to_owned();
            Some((slot.slot, path, stack.count))
        })
        .collect()
}

/// Um tick da Enrichment Chamber sobre o objeto vivo: a base nova, no lugar de `run_machine`.
fn tick_chamber(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, electric: &Electric) {
    let now = game.world.game_time;
    let dimension = game.native_dimension();
    let key = (dimension, pos);
    let recipes = basis::machine_recipes(game, &electric.machine.recipes);

    let started = basis::MEDIR.then(Instant::now);
    let alive = CHAMBERS.with_borrow_mut(|chambers| chambers.remove(&key));
    let mut live = match alive {
        Some(live) if live.last_seen + 1 == now && has_saved(entity) => live,
        _ => born(game, pos, entity, &recipes),
    };
    if !Arc::ptr_eq(&live.recipes, &recipes) {
        live.machine.set_recipes(core_recipes(game, &recipes));
        live.recipes = recipes.clone();
    }
    let machine = &live.machine;
    slot_from_entity(game, machine.input(), entity.items[rules::ELECTRIC_INPUT_SLOT].as_ref());
    slot_from_entity(game, machine.output(), entity.items[rules::ELECTRIC_OUTPUT_SLOT].as_ref());
    slot_from_entity(game, machine.energy_slot(), entity.items[rules::ELECTRIC_ENERGY_SLOT].as_ref());
    let stored = i64::from(basis::stored_energy(entity));
    if machine.energy().amount() != stored {
        machine.energy().set_energy(stored, None);
    }
    // `updatePower`: o sinal dos vizinhos, lido a cada tick.
    let powered = game.native_neighbor_signal(dimension, pos) > 0;
    if machine.tile().is_powered() != powered {
        machine.tile_mut().update_power(powered);
    }
    // O ejetor liga e desliga por `mekanism_eject`; sai pela direita da frente.
    let switch = match entity.nbt.get(&JavaString::from(EJECT_SWITCH)) {
        Some(Tag::Byte(on)) => Some(*on != 0),
        Some(Tag::Int(on)) => Some(*on != 0),
        _ => None,
    };
    if let Some(on) = switch
        && live.machine.ejector().is_ejecting() != on
    {
        live.machine.ejector_mut().set_ejecting(on);
    }
    let front = basis::front(game, pos);
    let right = front.right_of();
    if live.machine.ejector().is_ejecting() && live.eject_side != Some(right) {
        live.machine.ejector_mut().set_sides(0, vec![right]);
        live.eject_side = Some(right);
    }

    let offered = live.machine.output().resource().map(|resource| {
        let stack = stack_of(&recipes, &resource.id);
        (resource.id, stack)
    });
    let neighbours = GameNeighbours {
        game: &*game,
        dimension,
        pos,
        offered,
        created: RefCell::new(Vec::new()),
    };
    let comparator_changed = live.machine.tick_server(now as u64, &neighbours);
    let ejected = neighbours.ejected();

    let machine = &live.machine;
    slot_to_entity(entity, rules::ELECTRIC_INPUT_SLOT, machine.input(), &recipes);
    slot_to_entity(entity, rules::ELECTRIC_OUTPUT_SLOT, machine.output(), &recipes);
    slot_to_entity(entity, rules::ELECTRIC_ENERGY_SLOT, machine.energy_slot(), &recipes);
    let energy = machine.energy().amount_as_int();
    if basis::stored_energy(entity) != energy {
        basis::set_stored_energy(entity, energy);
    }
    let scalars = machine.saved_scalars();
    if live.saved.as_ref() != Some(&scalars) {
        write_scalars(entity, live.saved.as_ref(), &scalars);
        live.saved = Some(scalars);
    }
    let version = machine.used_version();
    if live.used_version != Some(version) {
        write_used(entity, &machine.saved_state());
        live.used_version = Some(version);
    }
    deliver(game, entity, &recipes, ejected);
    if comparator_changed {
        game.native_update_output_neighbors(dimension, pos);
    }
    let progress = machine.recipe_progress();
    if let Some(started) = started {
        basis::bench_record(now, u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX));
        let chest = machine
            .ejector()
            .is_ejecting()
            .then(|| chest_of(game, dimension, pos, right));
        basis::trace(&rules::trace_line(now, machine, chest.as_deref()));
    }

    let lit = machine.tile().block_active();
    if live.lit != Some(lit) {
        game.native_set_block_property(pos, "lit", if lit { "true" } else { "false" });
        live.lit = Some(lit);
    }
    let split = data_slots::split(energy as u32);
    let data = [split[0], split[1], progress];
    if entity.data != data {
        entity.data = data.to_vec();
    }

    let accepts = |_: &Game<'_>, _: usize, stack: &ItemStack| basis::is_recipe_input(&recipes, stack, None);
    let slots = |face| rules::electric_item_slots(front, face);
    let sides = rules::sides(front, rules::electric_item_slots);
    basis::tick_hoppers(game, pos, entity, &sides, &slots, &accepts);

    live.last_seen = now;
    CHAMBERS.with_borrow_mut(|chambers| {
        chambers.insert(key, live);
        if now.saturating_sub(LAST_SWEEP.get()) >= SWEEP_TICKS {
            LAST_SWEEP.set(now);
            chambers.retain(|_, live| now - live.last_seen <= STALE_TICKS);
        }
    });
}

fn tick_electric(
    game: &mut Game<'_>,
    pos: [i32; 3],
    entity: &mut ModBlockEntity,
    electric: &Electric,
) {
    let (step, progress) = basis::run_machine(game, entity, None, &electric.machine);
    let recipes = basis::machine_recipes(game, &electric.machine.recipes);
    let front = basis::front(game, pos);
    let accepts = |_: &Game<'_>, _: usize, stack: &ItemStack| basis::is_recipe_input(&recipes, stack, None);
    let slots = |face| rules::electric_item_slots(front, face);
    let sides = rules::sides(front, rules::electric_item_slots);
    basis::tick_hoppers(game, pos, entity, &sides, &slots, &accepts);
    let lit = if step.active { "true" } else { "false" };
    game.native_set_block_property(pos, "lit", lit);

    let mut data = vec![0; ELECTRIC_DATA_SLOTS];
    let energy = data_slots::split(basis::stored_energy(entity) as u32);
    data[DATA_ENERGY] = energy[0];
    data[DATA_ENERGY + 1] = energy[1];
    data[ELECTRIC_DATA_PROGRESS] = progress as i32;
    entity.data = data;
}

pub(in crate::game) fn tick(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    if is_cable(&entity.id) {
        basis::transmit_energy(game, pos, entity);
    } else if is_infuser(&entity.id) {
        tick_infuser(game, pos, entity);
    } else if let Some(electric) = electric_machine(&entity.id) {
        if electric.block == rules::ENRICHMENT_CHAMBER {
            tick_chamber(game, pos, entity, electric);
        } else {
            tick_electric(game, pos, entity, electric);
        }
    }
}

/// Shift-click do inventário: vai para o slot que aceita o item, e o que nenhum aceita não sai.
pub(in crate::game) fn quick_move(
    game: &Game<'_>,
    entity: &mut ModBlockEntity,
    container_slots: usize,
    slot: usize,
    stack: &mut ItemStack,
    handled: &mut bool,
) {
    if slot < container_slots {
        return;
    }
    if let Some(electric) = electric_machine(&entity.id) {
        let recipes = basis::machine_recipes(game, &electric.machine.recipes);
        if basis::is_recipe_input(&recipes, stack, None) {
            basis::move_into_slot(game, entity, rules::ELECTRIC_INPUT_SLOT, stack);
        }
        *handled = true;
        return;
    }
    if !is_infuser(&entity.id) {
        return;
    }
    let recipes = basis::machine_recipes(game, &INFUSING);
    let conversions = basis::conversions(game, rules::CHEMICAL_CONVERSION, &rules::CONVERSION_FIELDS);
    let tanks = basis::load_tanks(entity, &TANKS);
    if basis::is_recipe_input(&recipes, stack, tanks[0].resource()) {
        basis::move_into_slot(game, entity, rules::INPUT_SLOT, stack);
    } else if conversions.iter().any(|conversion| conversion.item.test(stack)) {
        basis::move_into_slot(game, entity, rules::INFUSION_SLOT, stack);
    }
    *handled = true;
}

pub(in crate::game) fn menu_button(entity: &mut ModBlockEntity, id: i32) {
    if !is_infuser(&entity.id) || id != DUMP_BUTTON {
        return;
    }
    let mut tanks = basis::load_tanks(entity, &TANKS);
    let amount = tanks[0].amount();
    tanks[0].extract(amount, Action::Execute);
    basis::save_tanks(entity, &tanks);
}

pub(in crate::game) fn screen_click(block: &str, at: [f32; 2], right: bool, button: &mut Option<i32>) {
    if is_infuser(block) && !right && inside(DUMP_RECT, at) {
        *button = Some(DUMP_BUTTON);
    }
}

pub(in crate::game) fn screen_tooltip(block: &str, data: &[i32], at: [f32; 2], lines: &mut Vec<String>) {
    let get = |i: usize| data.get(i).copied().unwrap_or(0);
    let energy = || format_energy(data_slots::join(get(DATA_ENERGY), get(DATA_ENERGY + 1)));
    if electric_machine(block).is_some() {
        if inside(ELECTRIC_ENERGY_BAR, at) {
            lines.push(energy());
        }
    } else if is_infuser(block) {
        if inside(ENERGY_BAR, at) {
            lines.push(energy());
        } else if inside(INFUSION_BAR, at) {
            lines.push(format!("{} / {} mB", get(DATA_INFUSION), rules::TANK_CAPACITY));
        }
    }
}

fn draw_slots(
    game: &Game<'_>,
    batch: &mut HudBatch,
    origin: [f32; 2],
    positions: &[[f32; 2]],
    energy_slot: usize,
) {
    let [x, y] = origin;
    for (i, [slot_x, slot_y]) in positions.iter().copied().enumerate() {
        let sprite = if i == energy_slot { ENERGY_SLOT_FRAME } else { SLOT };
        let dst = [x + slot_x - 1.0, y + slot_y - 1.0, SLOT_SIZE, SLOT_SIZE];
        game.native_sprite(batch, sprite, [0.0, 0.0, SLOT_SIZE, SLOT_SIZE], dst);
    }
}

fn draw_infuser(game: &Game<'_>, data: &[i32], batch: &mut HudBatch, origin: [f32; 2]) {
    let [x, y] = origin;
    let at = |rect: [f32; 4]| [x + rect[0], y + rect[1], rect[2], rect[3]];
    let get = |i: usize| data.get(i).copied().unwrap_or(0);

    let title = lang::get("container.mekanism.metallurgic_infuser");
    basis::machine_panel(game, batch, &PANEL, [x, y, WIDTH, HEIGHT], &title);
    draw_slots(game, batch, origin, &SLOT_POSITIONS, rules::ENERGY_SLOT);
    let infusion = get(DATA_INFUSION) as f32 / rules::TANK_CAPACITY as f32;
    let color = OPAQUE | data_slots::join(get(DATA_COLOR), get(DATA_COLOR + 1));
    basis::gauge(game, batch, at(INFUSION_BAR), infusion, color);
    let energy = data_slots::join(get(DATA_ENERGY), get(DATA_ENERGY + 1));
    let fill = energy as f32 / rules::energy_capacity(rules::INFUSER_USAGE) as f32;
    basis::gauge(game, batch, at(ENERGY_BAR), fill, ENERGY_COLOR);
    let progress = get(DATA_PROGRESS) as f32 / rules::TICKS_REQUIRED as f32;
    basis::progress_bar(game, batch, PROGRESS_EMPTY, PROGRESS_FULL, at(PROGRESS_BAR), progress);
    sprite_rect(game, batch, DUMP_SPRITE, at(DUMP_RECT));
}

fn draw_electric(
    game: &Game<'_>,
    electric: &Electric,
    data: &[i32],
    batch: &mut HudBatch,
    origin: [f32; 2],
) {
    let [x, y] = origin;
    let at = |rect: [f32; 4]| [x + rect[0], y + rect[1], rect[2], rect[3]];
    let get = |i: usize| data.get(i).copied().unwrap_or(0);

    let title = lang::get(electric.title);
    basis::machine_panel(game, batch, &PANEL, [x, y, WIDTH, HEIGHT], &title);
    draw_slots(game, batch, origin, &ELECTRIC_SLOT_POSITIONS, rules::ELECTRIC_ENERGY_SLOT);
    sprite_rect(game, batch, UP_ARROW, at(ELECTRIC_ARROW));
    let energy = data_slots::join(get(DATA_ENERGY), get(DATA_ENERGY + 1));
    let capacity = rules::energy_capacity(electric.machine.energy_per_tick);
    basis::gauge(game, batch, at(ELECTRIC_ENERGY_BAR), energy as f32 / capacity as f32, ENERGY_COLOR);
    let progress = get(ELECTRIC_DATA_PROGRESS) as f32 / rules::TICKS_REQUIRED as f32;
    basis::progress_bar(game, batch, BAR_EMPTY, BAR_FULL, at(ELECTRIC_PROGRESS_BAR), progress);
}

pub(in crate::game) fn screen_draw(
    game: &Game<'_>,
    block: &str,
    data: &[i32],
    batch: &mut HudBatch,
    origin: [f32; 2],
) {
    if is_infuser(block) {
        draw_infuser(game, data, batch, origin);
    } else if let Some(electric) = electric_machine(block) {
        draw_electric(game, electric, data, batch, origin);
    }
}
