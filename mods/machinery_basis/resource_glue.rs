// Os canos mecânicos e os tubos pressurizados no jogo (frentes `cano-jogo` e `tubo-jogo`, Fase 3; unificados na `limpeza-rede`, decisão 161). O dono dos transmissores de recurso
// (`machinery_basis_core::transmission::resource_owner`) guarda por dimensão o registro, os `Transmitter` e a família do recurso (fluido ou químico); esta parte o liga ao jogo
// como `machinery_basis` liga o dos cabos: os eventos da decisão 107 entram nele, as conexões que ele calcula viram as propriedades de lado do bloco, o NBT do transmissor
// (`TileEntityTransmitter.saveAdditional` e o `stored` de `BufferedResourceTransmitter.write`) sai no salvamento, e o vizinho com tanque (o `lavaTank` do Heat Generator, o tanque
// de infusão do Metallurgic Infuser) responde à rede por um espelho do contêiner dele (`HostHandler`, decisão 122). A lib não conhece nenhum id: o mod diz quais blocos são canos
// e tubos e o tier de cada um em `pipes_register` e `tubes_register`, e quais blocos têm um tanque em `fluid_neighbor_register` e `chemical_neighbor_register` (as faces por
// instância em `fluid_faces_set` e `chemical_faces_set`). Os despejos de radiação (`ChemicalNetwork.disperse`, decisão 130) ficam na família do químico, registrados e não entregues:
// a radiação é de outra frente.
//
// O código é um só, genérico sobre a [`Family`] (o recurso e o que muda de um para o outro: o NBT do `stored`, o que o vizinho diz de cada lado, as sondas); cada família tem as
// tabelas dela (`thread_local` e `static`), que a própria família entrega.
use crate::game::Game;
use crate::game::machinery_basis as basis;
use crate::game::machinery_basis::{TransmitterEvent, TransmitterKind, arr, sim_pos};
use horizon_client::containers::BlockEntity;
use horizon_world::nbt::{Compound, JavaString, Tag};
use crate::game::machinery_transmitter_client as client;
use machinery_basis_core::chemical::ChemicalResource;
use machinery_basis_core::fluid::FluidResource;
use machinery_basis_core::resource_tank::{Resource, StackContents};
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::{BlockPos as SimPos, ChunkPos};
use machinery_basis_core::transmission::chemical_network::chemical_transmitters;
use machinery_basis_core::transmission::connection::ConnectionType;
use machinery_basis_core::transmission::fluid_network::fluid_transmitters;
use machinery_basis_core::transmission::host_resource::{HostHandler, ResourceSnapshot};
use machinery_basis_core::transmission::owner::{CableView, Event as OwnerEvent};
use machinery_basis_core::transmission::reading::{AcceptorSides, TransmitterReading};
use machinery_basis_core::transmission::resource_owner::{Handler, PipeInfo, ResourceOwnerWorld, ResourceTransmitters};
use machinery_basis_core::transmission::transmitter::{Overlay, keys};
use machinery_basis_core::transmission::{DimensionId, Uuid};
use std::cell::RefCell;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Mutex;
use std::thread::LocalKey;

/// A capacidade e a vazão de um bloco de cano ou de tubo (`PipeTier` e `TubeTier`: `getCapacity` e `getTransferRate`), ou `None` se o bloco não é um.
pub type Stats = fn(&str) -> Option<(i64, i32)>;

/// O que o Horizon guarda do transmissor (`stored`: o `saveShare`).
const STORED: &str = "stored";
/// Os `ClearRemoved` que chegaram antes de o block entity estar no mundo do Horizon, tentados de novo nos eventos seguintes (`transmitter_events::PENDING_TICKS`).
type Pending = basis::PendingClears;

/// `(dimensão, posição do vizinho, lado dele que olha o transmissor)`.
type HandlerKey = (&'static str, [i32; 3], usize);
type Write<R> = fn(&mut Game<'_>, &'static str, [i32; 3], StackContents<R>);

/// O que muda de uma família de recurso para a outra. As tabelas que o jogo preenche e lê (os tipos de bloco, os vizinhos, as faces, os donos, os espelhos, os avisos de tela, a última
/// vista de cada transmissor e os `ClearRemoved` à espera) são da família, e cada uma devolve as suas.
trait Family: Sized + 'static {
    type R: Resource;
    /// Como o mod lê o tanque do vizinho (o fluido não olha o lado; o químico, sim).
    type Probe: Copy + Send + 'static;
    /// O que um lado do vizinho diz da capability (o fluido: se expõe; o químico: `(entra, sai)` ou nada).
    type Side: Copy + PartialEq + Send + 'static;
    /// O nome do dono entre os que ouvem os eventos dos transmissores.
    const SINK_OWNER: &'static str;
    /// As sondas (`TRANSMITTER_PROBE`): a rede de cada tick e a vista de cada transmissor.
    const NETS_PROBE: &'static str;
    const VIEW_PROBE: &'static str;
    /// A chave de `probe_push` que leva o id do recurso.
    const PUSH_KEY: &'static str;

    fn kinds() -> &'static Mutex<Vec<Stats>>;
    fn shared() -> &'static Mutex<Shared<Self>>;
    fn owners() -> &'static LocalKey<RefCell<HashMap<&'static str, ResourceTransmitters<Self::R>>>>;
    fn handlers() -> &'static LocalKey<RefCell<HashMap<HandlerKey, Rc<HostHandler<Self::R>>>>>;
    fn overlays() -> &'static LocalKey<RefCell<Vec<Overlay>>>;
    fn views() -> &'static LocalKey<RefCell<HashMap<(&'static str, [i32; 3]), CableView>>>;
    fn pending() -> &'static LocalKey<RefCell<Pending>>;

    fn new_owner() -> ResourceTransmitters<Self::R>;
    fn resource(id: &str) -> Self::R;
    fn resource_id(resource: &Self::R) -> &str;
    /// O tanque criativo que o bloco de teste faz no lugar do que ainda não está portado (só com a sonda ligada).
    fn probe_source(block: &str) -> Option<ResourceSnapshot<Self::R>>;
    fn exposed(side: Self::Side) -> bool;
    /// Um vizinho do vanilla que o NeoForge dá uma capability sem block entity (o caldeirão, `CauldronWrapper`): o contêiner dele agora, em qualquer lado, ou `None`.
    fn vanilla(_game: &Game<'_>, _block: &str, _pos: [i32; 3]) -> Option<ResourceSnapshot<Self::R>> {
        None
    }
    /// Grava no vizinho do vanilla o que a rede mudou; `false` se `block` não é um.
    fn vanilla_write(_game: &mut Game<'_>, _dimension: &'static str, _block: &str, _pos: [i32; 3], _contents: &StackContents<Self::R>) -> bool {
        false
    }
    fn read(probe: Self::Probe, game: &Game<'_>, dimension: &'static str, pos: [i32; 3], side: Face) -> Option<ResourceSnapshot<Self::R>>;
    /// O `saveShare` que o Horizon guardou (`StackHelper.storeNonEmpty` com o `CODEC` do recurso).
    fn read_stored(nbt: &Compound) -> Option<StackContents<Self::R>>;
    fn write_stored(nbt: &mut Compound, stored: &StackContents<Self::R>);
}

/// O que o mod registrou e declarou, por família: os vizinhos com tanque, as faces de cada instância e as posições cuja capability mudou desde o último tick do servidor.
struct Shared<F: Family> {
    neighbors: Vec<Neighbor<F>>,
    /// `(dimensão e posição, (bloco, o que cada lado diz))`.
    faces: Vec<((&'static str, [i32; 3]), (String, [F::Side; 6]))>,
    changed: Vec<(&'static str, [i32; 3])>,
}

impl<F: Family> Shared<F> {
    const fn new() -> Self {
        Self { neighbors: Vec::new(), faces: Vec::new(), changed: Vec::new() }
    }
}

struct Neighbor<F: Family> {
    block: &'static str,
    probe: F::Probe,
    write: Write<F::R>,
}

impl<F: Family> Clone for Neighbor<F> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<F: Family> Copy for Neighbor<F> {}

fn lock<T>(mutex: &'static Mutex<T>) -> std::sync::MutexGuard<'static, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// A capacidade de um caldeirão (`CauldronFluidContent.totalAmount`: um balde) e o fluido e os níveis de cada um (`CauldronFluidContent.init`: o vazio, a água com 3 níveis e a lava com 1).
const CAULDRON_AMOUNT: u64 = 1000;

fn cauldron(block: &str) -> Option<(Option<&'static str>, u8)> {
    match block {
        "minecraft:cauldron" => Some((None, 1)),
        "minecraft:water_cauldron" => Some((Some("minecraft:water"), 3)),
        "minecraft:lava_cauldron" => Some((Some("minecraft:lava"), 1)),
        _ => None,
    }
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

// ---------------------------------------------------------------------------------------------------------------------------
// Os canos mecânicos: o recurso é o fluido.

struct Pipes;

/// Lê o contêiner de fluido do bloco em `pos`, ou `None` se o tile não está no mundo agora (está no tick).
pub type FluidProbe = fn(&Game<'_>, &'static str, [i32; 3]) -> Option<ResourceSnapshot<FluidResource>>;
/// Grava o conteúdo que o contêiner passa a ter.
pub type FluidWrite = fn(&mut Game<'_>, &'static str, [i32; 3], StackContents<FluidResource>);

impl Family for Pipes {
    type R = FluidResource;
    type Probe = FluidProbe;
    type Side = bool;
    const SINK_OWNER: &'static str = "machinery_pipes";
    const NETS_PROBE: &'static str = "MBP";
    const VIEW_PROBE: &'static str = "MBW";
    const PUSH_KEY: &'static str = "fluid";

    fn kinds() -> &'static Mutex<Vec<Stats>> {
        static KINDS: Mutex<Vec<Stats>> = Mutex::new(Vec::new());
        &KINDS
    }

    fn shared() -> &'static Mutex<Shared<Self>> {
        static SHARED: Mutex<Shared<Pipes>> = Mutex::new(Shared::new());
        &SHARED
    }

    fn owners() -> &'static LocalKey<RefCell<HashMap<&'static str, ResourceTransmitters<FluidResource>>>> {
        thread_local! {
            /// Os canos de cada dimensão do nível deste servidor.
            static OWNERS: RefCell<HashMap<&'static str, ResourceTransmitters<FluidResource>>> = RefCell::new(HashMap::new());
        }
        &OWNERS
    }

    fn handlers() -> &'static LocalKey<RefCell<HashMap<HandlerKey, Rc<HostHandler<FluidResource>>>>> {
        thread_local! {
            /// Os vizinhos que o dono guarda como aceitadores (o mesmo objeto que o registro tem no cache de aceitadores).
            static HANDLERS: RefCell<HashMap<HandlerKey, Rc<HostHandler<FluidResource>>>> = RefCell::new(HashMap::new());
        }
        &HANDLERS
    }

    fn overlays() -> &'static LocalKey<RefCell<Vec<Overlay>>> {
        thread_local! {
            /// Os avisos de tela que o Configurator provocou, para quem chamou entregar ao jogador.
            static OVERLAYS: RefCell<Vec<Overlay>> = const { RefCell::new(Vec::new()) };
        }
        &OVERLAYS
    }

    fn views() -> &'static LocalKey<RefCell<HashMap<(&'static str, [i32; 3]), CableView>>> {
        thread_local! {
            /// O último `publish` de cada cano (o tipo de conexão visível de cada lado), para a mira do Configurator achar as peças de `getCollisionBoxes`.
            static VIEWS: RefCell<HashMap<(&'static str, [i32; 3]), CableView>> = RefCell::new(HashMap::new());
        }
        &VIEWS
    }

    fn pending() -> &'static LocalKey<RefCell<Pending>> {
        thread_local! {
            /// Os `ClearRemoved` que chegaram antes de o block entity entrar no mundo do Horizon, tentados de novo.
            static PENDING: RefCell<Pending> = RefCell::new(Pending::new());
        }
        &PENDING
    }

    fn new_owner() -> ResourceTransmitters<FluidResource> {
        fluid_transmitters(DimensionId(0), Uuid::random, |_| Uuid::random().hash_code())
    }

    fn resource(id: &str) -> FluidResource {
        FluidResource::new(id)
    }

    fn resource_id(resource: &FluidResource) -> &str {
        &resource.id
    }

    /// O que um tanque criativo faria no lugar de um bloco de teste: com a sonda ligada (`enviar_fontes.sh`, nunca no repositório), o caldeirão de lava e o de água são tanques
    /// criativos cheios que entregam e aceitam sem mudar (`FluidTankFluidTank.isCreative`), porque o Fluid Tank ainda não está portado.
    fn probe_source(block: &str) -> Option<ResourceSnapshot<FluidResource>> {
        let fluid = match block {
            "minecraft:lava_cauldron" => "minecraft:lava",
            "minecraft:water_cauldron" => "minecraft:water",
            _ => return None,
        };
        Some(creative_tank(FluidResource::new(fluid)))
    }

    fn exposed(side: bool) -> bool {
        side
    }

    fn vanilla(game: &Game<'_>, block: &str, pos: [i32; 3]) -> Option<ResourceSnapshot<FluidResource>> {
        let (fluid, levels) = cauldron(block)?;
        let level: u64 = match (fluid, levels) {
            (None, _) => 0,
            (Some(_), 1) => 1,
            _ => game.native_block_property(pos, "level")?.parse().ok()?,
        };
        let amount = CAULDRON_AMOUNT * level / u64::from(levels);
        let contents = StackContents::new(fluid.map(FluidResource::new), amount);
        // `CauldronWrapper.insert`/`extract`: só passos de `totalAmount / gcd(maxLevel, totalAmount)` (o balde inteiro na água, de 3 níveis; o de lava tem 1 nível); `isValid`: a água e a lava.
        let increment = CAULDRON_AMOUNT / gcd(3, CAULDRON_AMOUNT);
        Some(ResourceSnapshot {
            contents,
            capacity: CAULDRON_AMOUNT,
            accepts: Rc::new(|fluid: &FluidResource| matches!(&*fluid.id, "minecraft:water" | "minecraft:lava")),
            can_insert: true,
            can_extract: true,
            creative: false,
            increment,
        })
    }

    fn vanilla_write(game: &mut Game<'_>, dimension: &'static str, block: &str, pos: [i32; 3], contents: &StackContents<FluidResource>) -> bool {
        if cauldron(block).is_none() {
            return false;
        }
        // `setLevel`: nível 0 volta ao caldeirão vazio; senão o bloco do fluido com o nível que cabe na quantidade (`Level.setBlock(.., 0)`: sem aviso aos vizinhos).
        let (name, level) = match contents.resource.as_ref().filter(|_| contents.amount > 0) {
            None => ("minecraft:cauldron", None),
            Some(fluid) if &*fluid.id == "minecraft:lava" => ("minecraft:lava_cauldron", None),
            Some(_) => ("minecraft:water_cauldron", Some(((contents.amount * 3 + CAULDRON_AMOUNT / 2) / CAULDRON_AMOUNT).clamp(1, 3))),
        };
        let state = game.with_level(|_, level_ref, _| {
            let mut state = level_ref.registry.default_state(name)?;
            if let Some(level) = level {
                state = level_ref.native_state_with_property(state, "level", &level.to_string())?;
            }
            Some(state)
        });
        match state {
            Some(state) => game.native_set_block(dimension, pos, state, 2),
            None => false,
        }
    }

    fn read(probe: FluidProbe, game: &Game<'_>, dimension: &'static str, pos: [i32; 3], _side: Face) -> Option<ResourceSnapshot<FluidResource>> {
        probe(game, dimension, pos)
    }

    /// `stored: {type: {id}, amount}` (`StackHelper.storeNonEmpty` com o `FluidResource.CODEC`).
    fn read_stored(nbt: &Compound) -> Option<StackContents<FluidResource>> {
        let Some(Tag::Compound(stored)) = nbt.get(&JavaString::from(STORED)) else {
            return None;
        };
        let Some(Tag::Compound(kind)) = stored.get(&JavaString::from("type")) else {
            return None;
        };
        let Some(Tag::String(id)) = kind.get(&JavaString::from("id")) else {
            return None;
        };
        Some(StackContents::new(Some(FluidResource::new(&id.to_string_lossy())), stored_amount(stored)))
    }

    fn write_stored(nbt: &mut Compound, stored: &StackContents<FluidResource>) {
        let key = JavaString::from(STORED);
        let Some(resource) = stored.resource.as_ref().filter(|_| stored.amount > 0) else {
            nbt.remove(&key);
            return;
        };
        let mut kind = Compound::default();
        kind.insert(JavaString::from("id"), Tag::String(JavaString::from(&*resource.id)));
        let mut tag = Compound::default();
        tag.insert(JavaString::from("type"), Tag::Compound(kind));
        tag.insert(JavaString::from("amount"), Tag::Long(stored_to_long(stored.amount)));
        nbt.insert(key, Tag::Compound(tag));
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// Os tubos pressurizados: o recurso é o químico.

struct Tubes;

/// Lê o contêiner de químico do bloco em `pos` visto pelo lado `side` dele (as regras do lado: se entra, se sai), ou `None` se o tile não está no mundo agora (está no tick).
pub type ChemicalProbe = fn(&Game<'_>, &'static str, [i32; 3], Face) -> Option<ResourceSnapshot<ChemicalResource>>;
/// Grava o conteúdo que o contêiner passa a ter.
pub type ChemicalWrite = fn(&mut Game<'_>, &'static str, [i32; 3], StackContents<ChemicalResource>);
/// O que um lado do vizinho deixa fazer com a capability de químico: `None` sem capability, e senão `(entra, sai)`.
pub type ChemicalFace = Option<(bool, bool)>;

impl Family for Tubes {
    type R = ChemicalResource;
    type Probe = ChemicalProbe;
    type Side = ChemicalFace;
    const SINK_OWNER: &'static str = "machinery_tubes";
    const NETS_PROBE: &'static str = "MTN";
    const VIEW_PROBE: &'static str = "MTV";
    const PUSH_KEY: &'static str = "chem";

    fn kinds() -> &'static Mutex<Vec<Stats>> {
        static KINDS: Mutex<Vec<Stats>> = Mutex::new(Vec::new());
        &KINDS
    }

    fn shared() -> &'static Mutex<Shared<Self>> {
        static SHARED: Mutex<Shared<Tubes>> = Mutex::new(Shared::new());
        &SHARED
    }

    fn owners() -> &'static LocalKey<RefCell<HashMap<&'static str, ResourceTransmitters<ChemicalResource>>>> {
        thread_local! {
            /// Os tubos de cada dimensão do nível deste servidor.
            static OWNERS: RefCell<HashMap<&'static str, ResourceTransmitters<ChemicalResource>>> = RefCell::new(HashMap::new());
        }
        &OWNERS
    }

    fn handlers() -> &'static LocalKey<RefCell<HashMap<HandlerKey, Rc<HostHandler<ChemicalResource>>>>> {
        thread_local! {
            /// Os vizinhos que o dono guarda como aceitadores (o mesmo objeto que o registro tem no cache de aceitadores).
            static HANDLERS: RefCell<HashMap<HandlerKey, Rc<HostHandler<ChemicalResource>>>> = RefCell::new(HashMap::new());
        }
        &HANDLERS
    }

    fn overlays() -> &'static LocalKey<RefCell<Vec<Overlay>>> {
        thread_local! {
            /// Os avisos de tela que o Configurator provocou, para quem chamou entregar ao jogador.
            static OVERLAYS: RefCell<Vec<Overlay>> = const { RefCell::new(Vec::new()) };
        }
        &OVERLAYS
    }

    fn views() -> &'static LocalKey<RefCell<HashMap<(&'static str, [i32; 3]), CableView>>> {
        thread_local! {
            /// O último `publish` de cada tubo (o tipo de conexão visível de cada lado), para a mira do Configurator achar as peças de `getCollisionBoxes`.
            static VIEWS: RefCell<HashMap<(&'static str, [i32; 3]), CableView>> = RefCell::new(HashMap::new());
        }
        &VIEWS
    }

    fn pending() -> &'static LocalKey<RefCell<Pending>> {
        thread_local! {
            /// Os `ClearRemoved` que chegaram antes de o block entity entrar no mundo do Horizon, tentados de novo.
            static PENDING: RefCell<Pending> = RefCell::new(Pending::new());
        }
        &PENDING
    }

    fn new_owner() -> ResourceTransmitters<ChemicalResource> {
        chemical_transmitters(DimensionId(0), Uuid::random, |_| Uuid::random().hash_code())
    }

    fn resource(id: &str) -> ChemicalResource {
        ChemicalResource::new(id)
    }

    fn resource_id(resource: &ChemicalResource) -> &str {
        &resource.id
    }

    /// O que um tanque criativo faria no lugar de um bloco de teste: com a sonda ligada (`enviar_fontes.sh`, nunca no repositório), o bloco de carvão é um tanque criativo cheio de carbono
    /// que entrega e aceita sem mudar (`ChemicalTankChemicalTank.isCreative`), porque o Chemical Tank ainda não está portado.
    fn probe_source(block: &str) -> Option<ResourceSnapshot<ChemicalResource>> {
        let chemical = match block {
            "minecraft:coal_block" => "mekanism:carbon",
            _ => return None,
        };
        Some(creative_tank(ChemicalResource::new(chemical)))
    }

    fn exposed(side: ChemicalFace) -> bool {
        side.is_some()
    }

    fn read(probe: ChemicalProbe, game: &Game<'_>, dimension: &'static str, pos: [i32; 3], side: Face) -> Option<ResourceSnapshot<ChemicalResource>> {
        probe(game, dimension, pos, side)
    }

    /// `stored: {type: "<id>", amount}` (`StackHelper.storeNonEmpty` com o `ChemicalResource.CODEC`, que é o do holder do químico: o id em texto, como o tanque de infusão do
    /// Metallurgic Infuser grava o dele).
    fn read_stored(nbt: &Compound) -> Option<StackContents<ChemicalResource>> {
        let Some(Tag::Compound(stored)) = nbt.get(&JavaString::from(STORED)) else {
            return None;
        };
        let Some(Tag::String(id)) = stored.get(&JavaString::from("type")) else {
            return None;
        };
        Some(StackContents::new(Some(ChemicalResource::new(&id.to_string_lossy())), stored_amount(stored)))
    }

    fn write_stored(nbt: &mut Compound, stored: &StackContents<ChemicalResource>) {
        let key = JavaString::from(STORED);
        let Some(resource) = stored.resource.as_ref().filter(|_| stored.amount > 0) else {
            nbt.remove(&key);
            return;
        };
        let mut tag = Compound::default();
        tag.insert(JavaString::from("type"), Tag::String(JavaString::from(&*resource.id)));
        tag.insert(JavaString::from("amount"), Tag::Long(stored_to_long(stored.amount)));
        nbt.insert(key, Tag::Compound(tag));
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// O que as duas famílias repetiam igual.

/// A quantidade do `stored`: um `long` (ou `int`) que não é negativo.
fn stored_amount(stored: &Compound) -> u64 {
    match stored.get(&JavaString::from("amount")) {
        Some(Tag::Long(amount)) => u64::try_from(*amount).unwrap_or(0),
        Some(Tag::Int(amount)) => u64::try_from(*amount).unwrap_or(0),
        _ => 0,
    }
}

fn stored_to_long(amount: u64) -> i64 {
    i64::try_from(amount).unwrap_or(i64::MAX)
}

/// Um tanque criativo cheio do recurso, que entrega e aceita sem mudar: o que a sonda põe no lugar do tanque que ainda não está portado.
fn creative_tank<R: Resource>(resource: R) -> ResourceSnapshot<R> {
    let capacity = i64::MAX as u64;
    ResourceSnapshot {
        contents: StackContents::new(Some(resource), capacity),
        capacity,
        accepts: Rc::new(|_| true),
        can_insert: true,
        can_extract: true,
        creative: true,
        increment: 1,
    }
}

/// `id` (o `ModBlockEntity.id`) passa a ser um transmissor da família: os eventos do jogo dele vão ao dono, e `stats` dá a capacidade e a vazão do tier.
fn register<F: Family>(id: &str, stats: Stats) {
    client::register_source(client::TagSource { tag: client_tag::<F>, positions: client_positions::<F> });
    basis::transmitters_register(F::SINK_OWNER, id, sink::<F>);
    let mut kinds = lock(F::kinds());
    if !kinds.iter().any(|known| *known as usize == stats as usize) {
        kinds.push(stats);
    }
}

/// `id` (o `ModBlockEntity.id`) passa a ser um cano mecânico: os eventos do jogo dele vão ao dono dos canos, e `stats` dá a capacidade e a vazão do tier.
pub fn pipes_register(id: &str, stats: Stats) {
    register::<Pipes>(id, stats);
}

/// `id` (o `ModBlockEntity.id`) passa a ser um tubo pressurizado: os eventos do jogo dele vão ao dono dos tubos, e `stats` dá a capacidade e a vazão do tier.
pub fn tubes_register(id: &str, stats: Stats) {
    register::<Tubes>(id, stats);
}

fn stats_of<F: Family>(block: &str) -> Option<(i64, i32)> {
    let kinds: Vec<Stats> = lock(F::kinds()).clone();
    kinds.into_iter().find_map(|stats| stats(block))
}

/// O bloco `block` tem um tanque que a rede alcança: `probe` o lê e `write` grava o que a rede mudou. Pode ser dito mais de uma vez.
fn neighbor_register<F: Family>(block: &'static str, probe: F::Probe, write: Write<F::R>) {
    let mut shared = lock(F::shared());
    if !shared.neighbors.iter().any(|known| known.block == block) {
        shared.neighbors.push(Neighbor { block, probe, write });
    }
}

pub fn fluid_neighbor_register(block: &'static str, probe: FluidProbe, write: FluidWrite) {
    neighbor_register::<Pipes>(block, probe, write);
}

pub fn chemical_neighbor_register(block: &'static str, probe: ChemicalProbe, write: ChemicalWrite) {
    neighbor_register::<Tubes>(block, probe, write);
}

/// Dá ao bloco em `pos` as faces que ele tem agora (a capability por instância). Quem faz o tile andar chama a cada tick: só uma mudança avisa os transmissores vizinhos
/// (`BlockEntity.invalidateCapabilities`, que o cache de aceitadores do transmissor ouve).
fn faces_set<F: Family>(dimension: &'static str, pos: [i32; 3], block: &str, faces: [F::Side; 6]) {
    let mut shared = lock(F::shared());
    let key = (dimension, pos);
    match shared.faces.iter().position(|(known, _)| *known == key) {
        Some(index) if shared.faces[index].1.0 == block && shared.faces[index].1.1 == faces => {}
        Some(index) => {
            shared.faces[index].1 = (block.to_owned(), faces);
            shared.changed.push(key);
        }
        None => {
            shared.faces.push((key, (block.to_owned(), faces)));
            shared.changed.push(key);
        }
    }
}

pub fn fluid_faces_set(dimension: &'static str, pos: [i32; 3], block: &str, faces: [bool; 6]) {
    faces_set::<Pipes>(dimension, pos, block, faces);
}

pub fn chemical_faces_set(dimension: &'static str, pos: [i32; 3], block: &str, faces: [ChemicalFace; 6]) {
    faces_set::<Tubes>(dimension, pos, block, faces);
}

/// O contêiner do bloco em `pos` visto pelo lado `side` dele, ou `None` se a capability não existe ali.
fn snapshot<F: Family>(game: &Game<'_>, dimension: &'static str, pos: [i32; 3], side: Face) -> Option<ResourceSnapshot<F::R>> {
    let block = game.native_block(pos);
    // `/tmp/mb_caldeirao_vanilla` (só na cópia de teste): o caldeirão fica o do vanilla (`CauldronWrapper`) mesmo com a sonda ligada, para medir a rede de um cano que puxa dele (decisão 238).
    if basis::transmitter_probe_enabled()
        && !std::path::Path::new("/tmp/mb_caldeirao_vanilla").exists()
        && let Some(source) = F::probe_source(&block)
    {
        return Some(source);
    }
    if let Some(snapshot) = F::vanilla(game, &block, pos) {
        return Some(snapshot);
    }
    let neighbor = {
        let shared = lock(F::shared());
        let exposed = shared
            .faces
            .iter()
            .any(|((dim, at), (known, faces))| *dim == dimension && *at == pos && known.as_str() == &*block && F::exposed(faces[side as usize]));
        if !exposed {
            return None;
        }
        shared.neighbors.iter().find(|known| known.block == &*block).copied()?
    };
    // O tile que está no tick está fora do mundo: a capability existe, mas não entrega nem recebe nada até ele voltar.
    Some(F::read(neighbor.probe, game, dimension, pos, side).unwrap_or_else(ResourceSnapshot::closed))
}

/// Antes de um evento que pode distribuir ou puxar: os aceitadores leem o contêiner de novo (`around` limita aos vizinhos de um transmissor).
fn handlers_refresh<F: Family>(game: &Game<'_>, dimension: &'static str, around: Option<[i32; 3]>) {
    F::handlers().with_borrow_mut(|handlers| {
        // Quem só a tabela guarda já não está no cache de nenhum transmissor.
        handlers.retain(|_, handler| Rc::strong_count(handler) > 1);
        for ((dim, pos, side), handler) in handlers.iter() {
            if *dim != dimension {
                continue;
            }
            if let Some(transmitter) = around
                && !Face::ALL.iter().any(|face| {
                    let [x, y, z] = face.offset();
                    [transmitter[0] + x, transmitter[1] + y, transmitter[2] + z] == *pos
                })
            {
                continue;
            }
            let snapshot = snapshot::<F>(game, dimension, *pos, Face::ALL[*side]).unwrap_or_else(ResourceSnapshot::closed);
            handler.refresh(snapshot);
        }
    });
}

/// Depois do evento: o que a rede entregou e tirou vai para os contêineres dos vizinhos.
fn handlers_flush<F: Family>(game: &mut Game<'_>, dimension: &'static str) {
    let pending: Vec<([i32; 3], Rc<HostHandler<F::R>>)> = F::handlers().with_borrow(|handlers| {
        handlers
            .iter()
            .filter(|((dim, _, _), handler)| *dim == dimension && handler.changed().is_some())
            .map(|((_, pos, _), handler)| (*pos, handler.clone()))
            .collect()
    });
    for (pos, handler) in pending {
        let Some(contents) = handler.changed() else {
            continue;
        };
        let block = game.native_block(pos);
        if F::vanilla_write(game, dimension, &block, pos, &contents) {
            handler.settle();
            continue;
        }
        let write = lock(F::shared()).neighbors.iter().find(|known| known.block == &*block).map(|known| known.write);
        if let Some(write) = write {
            write(game, dimension, pos, contents);
        }
        handler.settle();
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// O jogo visto pelo dono durante um evento.

struct GameWorld<'a, 'g, F: Family> {
    game: RefCell<&'a mut Game<'g>>,
    dimension: &'static str,
    family: PhantomData<F>,
}

impl<'a, 'g, F: Family> GameWorld<'a, 'g, F> {
    fn new(game: &'a mut Game<'g>, dimension: &'static str) -> Self {
        Self { game: RefCell::new(game), dimension, family: PhantomData }
    }
}

impl<F: Family> ResourceOwnerWorld<F::R> for GameWorld<'_, '_, F> {
    fn is_block_loaded(&self, pos: SimPos) -> bool {
        self.game.borrow().world.chunks.contains_key(&(pos.x >> 4, pos.z >> 4))
    }

    fn is_getting_powered(&self, pos: SimPos) -> bool {
        self.game.borrow_mut().native_neighbor_signal(self.dimension, arr(pos)) > 0
    }

    fn handler(&self, pos: SimPos, side: Face) -> Option<Handler<F::R>> {
        let key = (self.dimension, arr(pos), side as usize);
        // `getCapability` não nulo: o vizinho tem um contêiner que alguma configuração de lados deixa encher ou esvaziar por este lado.
        let snapshot = snapshot::<F>(&self.game.borrow(), self.dimension, arr(pos), side)?;
        if let Some(handler) = F::handlers().with_borrow(|handlers| handlers.get(&key).cloned()) {
            handler.refresh(snapshot);
            return Some(handler as Handler<F::R>);
        }
        let handler = Rc::new(HostHandler::new(snapshot));
        F::handlers().with_borrow_mut(|handlers| handlers.insert(key, handler.clone()));
        Some(handler)
    }

    fn pipe_info(&mut self, pos: SimPos) -> Option<PipeInfo<F::R>> {
        let game = self.game.borrow();
        let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get(&(pos.x, pos.y, pos.z)) else {
            return None;
        };
        let (capacity, transfer_rate) = stats_of::<F>(&entity.id)?;
        let tag = basis::nbt_to_cable(&entity.nbt);
        let stored = F::read_stored(&entity.nbt);
        let from_save = tag.contains_key(keys::CONNECTION) || stored.is_some();
        Some(PipeInfo { capacity, transfer_rate, saved: from_save.then(|| (tag, stored.unwrap_or_default())) })
    }

    fn publish(&mut self, pos: SimPos, view: &CableView) {
        let at = arr(pos);
        basis::publish_arms(&mut self.game.borrow_mut(), at, view, F::VIEW_PROBE);
        F::views().with_borrow_mut(|views| views.insert((self.dimension, at), view.clone()));
        // `sendUpdatePacket`: o cliente recebe o pacote de atualização no começo do tick seguinte.
        client::dirty(self.dimension, at);
    }

    fn overlay(&mut self, _pos: SimPos, overlay: Overlay) {
        F::overlays().with_borrow_mut(|list| list.push(overlay));
    }
}

/// Só com a sonda ligada (a cópia de teste): o NBT do transmissor com `probe_push: {<PUSH_KEY>, amount}` empurra esse recurso para o contêiner da capability dele (`EXTERNAL`), como
/// um vizinho que tem um tanque faria, e a chave some. Serve ao teste em jogo repetir o `push` do rastro; nenhum bloco portado empurra fluido ou químico ainda.
fn probe_push<F: Family>(game: &mut Game<'_>, dimension: &'static str, pos: [i32; 3]) {
    let key = JavaString::from("probe_push");
    let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&(pos[0], pos[1], pos[2])) else {
        return;
    };
    let Some(Tag::Compound(push)) = entity.nbt.remove(&key) else {
        return;
    };
    let (Some(Tag::String(id)), Some(Tag::Int(amount))) = (push.get(&JavaString::from(F::PUSH_KEY)), push.get(&JavaString::from("amount"))) else {
        return;
    };
    let resource = F::resource(&id.to_string_lossy());
    with_owner::<F, _>(dimension, |owner| owner.insert_external(sim_pos(pos), &resource, *amount));
}

/// O pacote de atualização do transmissor em `pos` (`writeReducedUpdatedTag`), para o cliente.
fn client_tag<F: Family>(_game: &Game<'_>, dimension: &'static str, pos: [i32; 3]) -> Option<client::TagOut> {
    let (tag, capacity) = with_owner::<F, _>(dimension, |owner| owner.client_update(sim_pos(pos))).flatten()?;
    Some(client::TagOut { kind: machinery_basis_core::transmission::client::ClientKind::Resource, capacity, tag })
}

fn client_positions<F: Family>() -> Vec<(&'static str, [i32; 3])> {
    F::owners().with(|owners| {
        owners
            .try_borrow()
            .map(|owners| owners.iter().flat_map(|(dimension, owner)| owner.positions().into_iter().map(move |pos| (*dimension, arr(pos)))).collect())
            .unwrap_or_default()
    })
}

/// Roda `f` no dono da dimensão. `None` se o dono já está ocupado (um evento dentro de outro).
fn with_owner<F: Family, T>(dimension: &'static str, f: impl FnOnce(&mut ResourceTransmitters<F::R>) -> T) -> Option<T> {
    F::owners().with(|owners| {
        let mut owners = owners.try_borrow_mut().ok()?;
        let owner = owners.entry(dimension).or_insert_with(F::new_owner);
        basis::debug_prepare(owner.registry_mut());
        let result = f(owner);
        basis::debug_flush(owner.registry_mut());
        Some(result)
    })
}

/// As redes de uma família de recurso de todas as dimensões, com a chave de cada uma no espelho do conjunto único.
fn networks<F: Family>() -> Vec<(machinery_basis_core::transmission::registry::MirrorKey, usize, usize)> {
    F::owners().with(|owners| {
        owners
            .try_borrow()
            .map(|owners| owners.values().flat_map(|owner| owner.registry().mirror_summaries()).collect())
            .unwrap_or_default()
    })
}

pub(in crate::game) fn pipe_networks() -> Vec<(machinery_basis_core::transmission::registry::MirrorKey, usize, usize)> {
    networks::<Pipes>()
}

pub(in crate::game) fn tube_networks() -> Vec<(machinery_basis_core::transmission::registry::MirrorKey, usize, usize)> {
    networks::<Tubes>()
}

/// O sink dos eventos dos transmissores: traduz o evento do jogo para o do dono.
fn sink<F: Family>(game: &mut Game<'_>, event: &TransmitterEvent) {
    let dimension = game.native_dimension();
    // O `Update` de um block entity que não é deste dono (cada tick entrega o de cada transmissor a todos os donos) não faz nada nele (`update` sai no `live`), e sem `ClearRemoved` esperando
    // nada mais do que o evento faria roda: nem a espera, nem o espelho dos vizinhos, nem a entrega. Sai antes de tudo isso (decisão 202).
    if let TransmitterEvent::Transmitter { pos, kind: TransmitterKind::Update, .. } = event
        && F::pending().with_borrow(Pending::is_empty)
        && !owns::<F>(dimension, *pos)
    {
        return;
    }
    {
        let _span = basis::perf_span(basis::PERF_RES_RETRY);
        retry_pending::<F>(game, dimension);
    }
    deliver::<F>(game, dimension, event);
    // Um `ClearRemoved` sem block entity no mundo ainda: tenta de novo nos eventos seguintes.
    if let TransmitterEvent::Transmitter { pos, kind: TransmitterKind::ClearRemoved, .. } = event {
        let created = with_owner::<F, _>(dimension, |owner| owner.pipe_at(sim_pos(*pos)).is_some()).unwrap_or(true);
        if !created && is_transmitter_entity::<F>(game, *pos) {
            let now = game.world.game_time;
            F::pending().with_borrow_mut(|pending| pending.wait(dimension, *pos, now));
        }
    }
}

/// O dono da dimensão tem o transmissor em `pos`? Um dono ocupado (um evento dentro de outro) conta como se tivesse: o caminho de sempre decide.
fn owns<F: Family>(dimension: &'static str, pos: [i32; 3]) -> bool {
    F::owners().with(|owners| owners.try_borrow().ok().and_then(|owners| owners.get(dimension).map(|owner| owner.pipe_at(sim_pos(pos)).is_some())).unwrap_or(true))
}

/// O block entity em `pos` é um transmissor da família (ou ainda não está no mundo, e então a pergunta é de outro dono).
fn is_transmitter_entity<F: Family>(game: &Game<'_>, pos: [i32; 3]) -> bool {
    match game.world.block_entities.get(&(pos[0], pos[1], pos[2])) {
        Some(BlockEntity::Mod(entity)) => stats_of::<F>(&entity.id).is_some(),
        _ => false,
    }
}

/// Os `ClearRemoved` que esperavam o block entity: o que já o tem entra, o que esperou demais sai da fila.
fn retry_pending<F: Family>(game: &mut Game<'_>, dimension: &'static str) {
    let now = game.world.game_time;
    let ready = {
        let entities = &game.world.block_entities;
        F::pending().with_borrow_mut(|pending| {
            pending.retry(dimension, now, |pos| match entities.get(&(pos[0], pos[1], pos[2])) {
                Some(BlockEntity::Mod(entity)) => stats_of::<F>(&entity.id).is_some(),
                _ => false,
            })
        })
    };
    for pos in ready {
        deliver::<F>(game, dimension, &TransmitterEvent::Transmitter { dim: dimension.to_owned(), pos, kind: TransmitterKind::ClearRemoved });
    }
}

fn deliver<F: Family>(game: &mut Game<'_>, dimension: &'static str, event: &TransmitterEvent) {
    let Some(event) = basis::owner_event(event) else {
        basis::transmitter_probe_reset(F::SINK_OWNER, F::owners().with_borrow(|owners| basis::sum_counts(owners.values().map(|owner| owner.registry().probe_counts()))));
        F::owners().with_borrow_mut(HashMap::clear);
        F::views().with_borrow_mut(HashMap::clear);
        F::handlers().with_borrow_mut(HashMap::clear);
        let mut shared = lock(F::shared());
        shared.faces.clear();
        shared.changed.clear();
        drop(shared);
        // O que esperava um block entity do mundo que fechou, e os avisos de tela dele, não passam ao seguinte.
        F::pending().with_borrow_mut(Pending::clear);
        F::overlays().with_borrow_mut(Vec::clear);
        return;
    };
    let post = matches!(event, OwnerEvent::ServerTickPost { .. });
    // `onChunkUnloaded` toma a parte do transmissor e o chunk vai para o disco: o NBT sai agora, antes do `setRemoved` que o tira do dono.
    let unloaded = if let OwnerEvent::ChunkUnloaded(pos) = &event { Some(arr(*pos)) } else { None };
    let removed = if let OwnerEvent::Removed(pos) = &event { Some(arr(*pos)) } else { None };
    // Quem puxa (`Update`) ou distribui (`ServerTickPost`) fala com os vizinhos pelos aceitadores, que leem o contêiner deles antes e escrevem depois.
    {
        let _span = basis::perf_span(basis::PERF_RES_REFRESH);
        match &event {
            OwnerEvent::Update(pos) => handlers_refresh::<F>(game, dimension, Some(arr(*pos))),
            OwnerEvent::ServerTickPost { .. } => handlers_refresh::<F>(game, dimension, None),
            _ => {}
        }
    }
    let pre_span = basis::perf_span(basis::PERF_RES_POST_PRE);
    if post && basis::setups_probe_enabled() {
        // O tile que está no tick está fora do mundo: o empurrão da sonda espera o fim do tick, antes do `onTick` do registro.
        let positions: Vec<[i32; 3]> = with_owner::<F, _>(dimension, |owner| owner.positions().into_iter().map(arr).collect()).unwrap_or_default();
        for pos in positions {
            probe_push::<F>(game, dimension, pos);
        }
    }
    if post {
        // As capabilities que mudaram durante o tick chegam aos transmissores vizinhos antes do `onTick` do registro.
        let changed: Vec<[i32; 3]> = {
            let mut shared = lock(F::shared());
            let (here, other): (Vec<_>, Vec<_>) = std::mem::take(&mut shared.changed).into_iter().partition(|(dim, _)| *dim == dimension);
            shared.changed = other;
            here.into_iter().map(|(_, pos)| pos).collect()
        };
        for pos in changed {
            for side in Face::ALL {
                let [x, y, z] = side.offset();
                let neighbor = [pos[0] + x, pos[1] + y, pos[2] + z];
                if with_owner::<F, _>(dimension, |owner| owner.pipe_at(sim_pos(neighbor)).is_some()).unwrap_or(false) {
                    let mut world = GameWorld::<F>::new(&mut *game, dimension);
                    with_owner::<F, _>(dimension, |owner| owner.handle(&mut world, OwnerEvent::Capability(sim_pos(neighbor), side.opposite())));
                }
            }
        }
        let time = game.world.game_time;
        with_owner::<F, _>(dimension, |owner| owner.set_game_time(time));
    }
    let runs_normally = matches!(event, OwnerEvent::ServerTickPost { runs_normally: true });
    drop(pre_span);
    let mut scale_packets = Vec::new();
    let handle_span = basis::perf_span(basis::PERF_RES_HANDLE);
    let dirty = with_owner::<F, _>(dimension, |owner| {
        {
            let mut world = GameWorld::<F>::new(&mut *game, dimension);
            owner.handle(&mut world, event);
        }
        if post {
            scale_packets = owner.take_scale_packets();
        }
        if let Some(pos) = unloaded {
            transmitter_write::<F>(game, owner, pos);
        }
        if post && basis::setups_probe_enabled() {
            // A sonda da família: o recurso, a quantidade, a capacidade e o que saiu de cada rede neste tick, e quantos transmissores e aceitadores ela tem.
            let nets: Vec<String> = owner
                .registry()
                .networks_in_order()
                .into_iter()
                .filter_map(|id| {
                    let net = owner.family().network(id)?;
                    let registered = owner.registry().network(id);
                    let buffer = net.buffer();
                    Some(format!(
                        "[\"{}\",{},{},{},{},{}]",
                        buffer.resource.as_ref().map_or("", |resource| F::resource_id(resource)),
                        buffer.amount,
                        net.capacity(),
                        net.prev_transfer_amount(),
                        registered.transmitters_size(),
                        registered.acceptor_count()
                    ))
                })
                .collect();
            eprintln!("{} {{\"t\":{},\"runs\":{},\"nets\":[{}]}}", F::NETS_PROBE, game.world.game_time, runs_normally, nets.join(","));
        }
        if post { owner.family_mut().take_dirty_chunks() } else { Vec::new() }
    });
    drop(handle_span);
    let flush_span = basis::perf_span(basis::PERF_RES_FLUSH);
    handlers_flush::<F>(game, dimension);
    drop(flush_span);
    let _post_span = basis::perf_span(basis::PERF_RES_POST_POST);
    // `Mekanism.onLiquidTransferred` e `onChemicalTransferred`: o `PacketNetworkScale` e o do conteúdo saem juntos, a quem está no raio da rede.
    if !scale_packets.is_empty() {
        let packets = scale_packets
            .into_iter()
            .map(|out| (out.packet, out.range, out.contents.map(|resource| resource.map(|resource| F::resource_id(&resource).to_owned()))))
            .collect();
        client::scale(game, dimension, packets);
    }
    if let Some(pos) = removed {
        client::gone(dimension, pos);
        F::views().with_borrow_mut(|views| views.remove(&(dimension, pos)));
    }
    // `markChunkDirty` da rede: o chunk do transmissor vai para o próximo salvamento.
    for key in dirty.unwrap_or_default() {
        let chunk = ChunkPos::unpack(key);
        game.world.modified.insert((chunk.x, chunk.z));
    }
}

/// `TileEntityTransmitter.saveAdditional` e `BufferedResourceTransmitter.write`: grava no block entity de `pos` o que o transmissor guarda.
fn transmitter_write<F: Family>(game: &mut Game<'_>, owner: &mut ResourceTransmitters<F::R>, pos: [i32; 3]) {
    let Some((tag, stored)) = owner.write(sim_pos(pos)) else {
        return;
    };
    if let Some(BlockEntity::Mod(entity)) = game.world.block_entities.get_mut(&(pos[0], pos[1], pos[2])) {
        basis::cable_to_nbt(&tag, &mut entity.nbt);
        F::write_stored(&mut entity.nbt, &stored);
    }
}

/// O salvamento do nível (`Game::save_level`, antes de ele esvaziar os chunks alterados): os transmissores dos chunks que vão para o disco gravam o NBT deles.
fn save<F: Family>(game: &mut Game<'_>) {
    let dimension = game.native_dimension();
    let chunks: Vec<(i32, i32)> = game.world.modified.iter().copied().collect();
    if chunks.is_empty() {
        return;
    }
    let positions: Vec<[i32; 3]> = with_owner::<F, _>(dimension, |owner| owner.positions().into_iter().map(arr).collect()).unwrap_or_default();
    for pos in positions {
        if chunks.contains(&(pos[0] >> 4, pos[2] >> 4)) {
            with_owner::<F, _>(dimension, |owner| transmitter_write::<F>(game, owner, pos));
        }
    }
}

pub(in crate::game) fn pipes_save(game: &mut Game<'_>) {
    save::<Pipes>(game);
}

pub(in crate::game) fn tubes_save(game: &mut Game<'_>) {
    save::<Tubes>(game);
}

/// Os tipos de conexão visíveis do transmissor em `pos` (`getConnectionType(side)`: `NONE` onde não há braço), na ordem dos lados, como o último `publish` os deixou. `None` fora do
/// servidor, que é onde o dono vive, ou se o transmissor ainda não publicou.
fn arm_kinds<F: Family>(game: &Game<'_>, pos: [i32; 3]) -> Option<[ConnectionType; 6]> {
    F::views().with_borrow(|views| views.get(&(game.native_dimension(), pos)).map(|view| view.kind))
}

pub(in crate::game) fn pipe_arm_kinds(game: &Game<'_>, pos: [i32; 3]) -> Option<[ConnectionType; 6]> {
    arm_kinds::<Pipes>(game, pos)
}

pub(in crate::game) fn tube_arm_kinds(game: &Game<'_>, pos: [i32; 3]) -> Option<[ConnectionType; 6]> {
    arm_kinds::<Tubes>(game, pos)
}

/// `ItemAlloy.useOn` num transmissor: a liga de nível `alloy_level` sobe os da rede que `canUpgrade` aceita (`blocks` dá o bloco de cada nível, do básico ao supremo). `None` se o bloco
/// clicado não é do dono; senão se algum subiu (a liga se gasta).
fn alloy<F: Family>(game: &mut Game<'_>, pos: [i32; 3], alloy_level: i32, blocks: &[&str]) -> Option<bool> {
    let dimension = game.native_dimension();
    let level_of = |game: &Game<'_>, at: SimPos| -> Option<i32> {
        let block = game.native_block(arr(at));
        blocks.iter().position(|id| *id == block).map(|level| level as i32)
    };
    let mut run = {
        let game_ref: &Game<'_> = game;
        with_owner::<F, _>(dimension, |owner| owner.alloy_start(sim_pos(pos), alloy_level, &|at| level_of(game_ref, at)))??
    };
    let upgraded_block = *blocks.get(usize::try_from(alloy_level).ok()?)?;
    for target in run.targets.clone() {
        let data = {
            let mut world = GameWorld::<F>::new(&mut *game, dimension);
            with_owner::<F, _>(dimension, |owner| owner.alloy_take(&mut world, &mut run, target)).flatten()
        };
        let Some(data) = data else {
            continue;
        };
        basis::alloy_replace(game, arr(target), upgraded_block);
        let given = with_owner::<F, _>(dimension, |owner| owner.alloy_give(&mut run, target, &data)).unwrap_or(false);
        if given && run.full() {
            break;
        }
    }
    {
        let mut world = GameWorld::<F>::new(&mut *game, dimension);
        with_owner::<F, _>(dimension, |owner| owner.alloy_finish(&mut world, &run));
    }
    Some(run.did_upgrade())
}

pub(in crate::game) fn pipe_alloy(game: &mut Game<'_>, pos: [i32; 3], alloy_level: i32, blocks: &[&str]) -> Option<bool> {
    alloy::<Pipes>(game, pos, alloy_level, blocks)
}

pub(in crate::game) fn tube_alloy(game: &mut Game<'_>, pos: [i32; 3], alloy_level: i32, blocks: &[&str]) -> Option<bool> {
    alloy::<Tubes>(game, pos, alloy_level, blocks)
}

/// O clique do Configurator num transmissor (`ItemConfigurator.useOn`: `IConfigurable.onSneakRightClick` agachado, `onRightClick` senão). `side` é a face clicada e `hit_side` o
/// lado do braço que o olhar toca. Devolve se o clique foi do transmissor e os avisos para o jogador.
fn configure<F: Family>(game: &mut Game<'_>, pos: [i32; 3], side: Face, hit_side: Option<Face>, sneaking: bool) -> Option<(bool, Vec<Overlay>)> {
    let dimension = game.native_dimension();
    F::overlays().with_borrow_mut(Vec::clear);
    let consumed = with_owner::<F, _>(dimension, |owner| {
        let mut world = GameWorld::<F>::new(&mut *game, dimension);
        if sneaking { owner.sneak_right_click(&mut world, sim_pos(pos), side, hit_side) } else { owner.right_click(&mut world, sim_pos(pos)) }
    })?;
    let overlays = F::overlays().with_borrow_mut(std::mem::take);
    Some((consumed, overlays))
}

pub(in crate::game) fn pipe_configure(game: &mut Game<'_>, pos: [i32; 3], side: Face, hit_side: Option<Face>, sneaking: bool) -> Option<(bool, Vec<Overlay>)> {
    configure::<Pipes>(game, pos, side, hit_side, sneaking)
}

pub(in crate::game) fn tube_configure(game: &mut Game<'_>, pos: [i32; 3], side: Face, hit_side: Option<Face>, sneaking: bool) -> Option<(bool, Vec<Overlay>)> {
    configure::<Tubes>(game, pos, side, hit_side, sneaking)
}

/// `ItemNetworkReader.displayTransmitterInfo` para o cano ou tubo em `pos`, com o recurso guardado pelo id.
fn read<F: Family>(game: &Game<'_>, pos: [i32; 3]) -> Option<TransmitterReading<String>> {
    use machinery_basis_core::transmission::reading::NetworkReader;
    with_owner::<F, _>(game.native_dimension(), |owner| owner.read_network(sim_pos(pos)).map(|resource| F::resource_id(&resource).to_owned()))
}

pub(in crate::game) fn pipe_read(game: &Game<'_>, pos: [i32; 3]) -> Option<TransmitterReading<String>> {
    read::<Pipes>(game, pos)
}

pub(in crate::game) fn tube_read(game: &Game<'_>, pos: [i32; 3]) -> Option<TransmitterReading<String>> {
    read::<Tubes>(game, pos)
}

/// `displayConnectedNetworks`: os lados do aceitador em `acceptor` por onde a rede do transmissor em `transmitter` entra, se ela o tem.
fn acceptor_sides<F: Family>(game: &Game<'_>, transmitter: [i32; 3], acceptor: [i32; 3]) -> Option<AcceptorSides> {
    use machinery_basis_core::transmission::reading::NetworkReader;
    with_owner::<F, _>(game.native_dimension(), |owner| owner.acceptor_sides(sim_pos(transmitter), sim_pos(acceptor))).flatten()
}

pub(in crate::game) fn pipe_acceptor_sides(game: &Game<'_>, transmitter: [i32; 3], acceptor: [i32; 3]) -> Option<AcceptorSides> {
    acceptor_sides::<Pipes>(game, transmitter, acceptor)
}

pub(in crate::game) fn tube_acceptor_sides(game: &Game<'_>, transmitter: [i32; 3], acceptor: [i32; 3]) -> Option<AcceptorSides> {
    acceptor_sides::<Tubes>(game, transmitter, acceptor)
}
