//! O dono dos transportadores logísticos (`InventoryTransmitters`) pelo que os rastros não cobrem: o NBT que sai e volta (a lista de pilhas em trânsito, a cor e o modo do desviador), o
//! chunk que descarrega (as pilhas ficam no disco, nada cai), as cores que se contradizem, o clique do Configurator (a cor do logístico, o modo do desviador) e a capability de item. Os
//! cenários do original (o movimento tick a tick, a quebra, o desviador pelo sinal, o funil) passam por ele em `transporter_move_reference.rs`.
use machinery_basis_core::components::Components;
use machinery_basis_core::containers::ItemResource;
use machinery_basis_core::item_handler::{ItemHandler, VanillaContainer};
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::BlockPos;
use machinery_basis_core::transmission::client::{
    ClientKind, ClientTransmitters, Op, decode_batch, encode_batches,
};
use machinery_basis_core::transmission::connection::{
    ConnectionType, TransmissionSet, TransmissionType,
};
use machinery_basis_core::transmission::host_item::{HostItemHandler, MirrorSlot, SlotChange};
use machinery_basis_core::transmission::inventory_owner::{
    InventoryOwnerWorld, InventoryTransmitters, ItemAcceptor, SavedTransporter, TransporterInfo,
    TransporterPacket, hopper_outcome,
};
use machinery_basis_core::transmission::owner::{CableView, Event};
use machinery_basis_core::transmission::transmitter::{Overlay, Transmitter};
use machinery_basis_core::transmission::{DimensionId, Uuid};
use machinery_basis_core::transporter::movement::WorldDrop;
use machinery_basis_core::transporter::nbt::{Compound, Nbt};
use machinery_basis_core::transporter::node::{
    CapabilityChange, DiversionControl, TransporterKind, TransporterTier,
};
use machinery_basis_core::transporter::pos::NO_POS;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

const DIMENSION: DimensionId = DimensionId(0);

#[derive(Default)]
struct SimWorld {
    chests: HashMap<BlockPos, Rc<VanillaContainer>>,
    powered: Rc<Cell<bool>>,
    infos: RefCell<HashMap<BlockPos, TransporterInfo>>,
    drops: Vec<WorldDrop>,
    overlays: Vec<Overlay>,
    capabilities: Vec<CapabilityChange>,
    changed: usize,
    published: usize,
    packets: Vec<(BlockPos, TransporterPacket)>,
    /// Com o espelho ligado (`mirrored`), o contêiner é visto como o jogo o mostra: uma cópia dos slots (`HostItemHandler`), relida no começo do evento e aplicada no fim.
    mirrored: bool,
    mirrors: RefCell<HashMap<BlockPos, Rc<HostItemHandler>>>,
}

impl SimWorld {
    fn slots_of(&self, pos: BlockPos) -> Option<Vec<MirrorSlot>> {
        let chest = self.chests.get(&pos)?;
        Some(
            (0..chest.size())
                .map(|index| MirrorSlot {
                    index,
                    contents: chest
                        .resource(index)
                        .map(|resource| (resource, chest.amount(index))),
                    can_take: true,
                    refuses: Vec::new(),
                })
                .collect(),
        )
    }

    /// `handlers_refresh` da cola: os espelhos leem os contêineres de novo.
    fn refresh_mirrors(&self) {
        for (pos, handler) in self.mirrors.borrow().iter() {
            handler.refresh(self.slots_of(*pos).unwrap_or_default());
        }
    }

    /// `handlers_flush` da cola: o que o evento mudou nos espelhos vai para os contêineres.
    fn flush_mirrors(&self) {
        for (pos, handler) in self.mirrors.borrow().iter() {
            let chest = self.chests[pos].clone();
            for change in handler.changes() {
                match change {
                    SlotChange::Take { index, count } => {
                        let resource = chest.resource(index).unwrap();
                        let left = chest.amount(index) - count;
                        chest.set_item(index, (left > 0).then_some(resource), left);
                    }
                    SlotChange::Put { resource, count } => {
                        let mut left = count;
                        for index in 0..chest.size() {
                            if left == 0 {
                                break;
                            }
                            let transaction =
                                machinery_basis_core::transaction::Transaction::open_root();
                            let put = chest.insert_at(index, &resource, left, &transaction);
                            transaction.commit();
                            left -= put;
                        }
                        assert_eq!(left, 0, "o jogo recusou o que o espelho aceitou");
                    }
                }
            }
            handler.settle();
        }
    }
}

impl InventoryOwnerWorld for SimWorld {
    fn is_block_loaded(&self, _: BlockPos) -> bool {
        true
    }

    fn is_getting_powered(&self, _: BlockPos) -> bool {
        self.powered.get()
    }

    fn item_handler(&self, pos: BlockPos, _: Face) -> Option<ItemAcceptor> {
        if self.mirrored {
            let slots = self.slots_of(pos)?;
            if let Some(handler) = self.mirrors.borrow().get(&pos).cloned() {
                handler.refresh_if_clean(slots);
                return Some(handler as ItemAcceptor);
            }
            let handler = Rc::new(HostItemHandler::new(slots));
            self.mirrors.borrow_mut().insert(pos, handler.clone());
            return Some(handler as ItemAcceptor);
        }
        self.chests
            .get(&pos)
            .map(|chest| chest.clone() as Rc<dyn ItemHandler>)
    }

    fn transporter_info(&mut self, pos: BlockPos) -> Option<TransporterInfo> {
        self.infos.borrow_mut().remove(&pos)
    }

    fn decode_item(&self, tag: &Compound) -> Option<ItemResource> {
        component_decode(tag)
    }

    fn publish(&mut self, _: BlockPos, _: &CableView) {
        self.published += 1;
    }

    fn set_changed(&mut self, _: BlockPos) {
        self.changed += 1;
    }

    fn overlay(&mut self, _: BlockPos, overlay: Overlay) {
        self.overlays.push(overlay);
    }

    fn capability_changed(&mut self, _: BlockPos, change: CapabilityChange) {
        self.capabilities.push(change);
    }

    fn world_drop(&mut self, drop: WorldDrop) {
        self.drops.push(drop);
    }

    fn transporter_packet(&mut self, pos: BlockPos, packet: TransporterPacket) {
        self.packets.push((pos, packet));
    }
}

/// `ItemResource.CODEC` ao ler, para o teste: o `id` e, se há, os `components` (cada um como texto).
fn component_decode(tag: &Compound) -> Option<ItemResource> {
    let id = tag.get("id").and_then(Nbt::as_str)?;
    let texts: Vec<(String, String)> = tag
        .get("components")
        .and_then(Nbt::as_compound)
        .map(|components| {
            components
                .iter()
                .filter_map(|(kind, value)| Some((kind.clone(), value.as_str()?.to_owned())))
                .collect()
        })
        .unwrap_or_default();
    Some(ItemResource::with_components(
        id,
        64,
        Components::new(
            texts
                .iter()
                .map(|(kind, value)| (kind.as_str(), value.as_str())),
        ),
    ))
}

/// `ItemResource.CODEC` ao escrever, para o teste: o `components` só existe quando o recurso tem algum.
fn component_encode(item: &ItemResource) -> Compound {
    let mut resource = Compound::new();
    resource.insert("id".into(), Nbt::String(item.id.to_string()));
    if !item.components.is_empty() {
        let components: Compound = item
            .components
            .iter()
            .map(|(kind, value)| (kind.to_owned(), Nbt::String(value.to_owned())))
            .collect();
        resource.insert("components".into(), Nbt::Compound(components));
    }
    resource
}

fn owner() -> InventoryTransmitters {
    InventoryTransmitters::new(
        DIMENSION,
        {
            let mut next = 0;
            move || {
                next += 1;
                Uuid::with_hash_code(next * 7919)
            }
        },
        {
            let mut index = 0;
            move |_| {
                index += 1;
                100_003 * index
            }
        },
    )
}

fn at(x: i32) -> BlockPos {
    BlockPos::new(x, 4, 0)
}

const WEST: usize = 4;

/// O que o block entity de um transportador traz do disco: as ligações, a cor ou o modo, e as pilhas.
fn saved(pos: BlockPos, raw: [ConnectionType; 6], kind: &TransporterKind) -> SavedTransporter {
    let mut template: Transmitter<ItemAcceptor> = Transmitter::new(
        [pos.x, pos.y, pos.z],
        TransmissionSet::of(&[TransmissionType::Item]),
        false,
    );
    template.set_connection_types_raw(raw);
    SavedTransporter {
        tag: template.write(),
        color: match kind {
            TransporterKind::Logistical { color: Some(color) } => Some(Nbt::String(
                machinery_basis_core::side_config::ENUM_COLOR_NAMES[usize::from(*color)].into(),
            )),
            _ => None,
        },
        modes: match kind {
            TransporterKind::Diversion { modes } => Some(Nbt::IntArray(
                modes.iter().map(|mode| mode.ordinal()).collect(),
            )),
            _ => None,
        },
        items: None,
    }
}

fn normal() -> [ConnectionType; 6] {
    [ConnectionType::Normal; 6]
}

fn pulling_west() -> [ConnectionType; 6] {
    let mut raw = normal();
    raw[WEST] = ConnectionType::Pull;
    raw
}

/// Uma linha de transportadores em `xs` (cada um com o tipo dado), um baú em `x = -1` com terra e outro em `x = xs.len()`.
fn line(kinds: &[TransporterKind]) -> (InventoryTransmitters, SimWorld) {
    let mut world = SimWorld::default();
    let source = VanillaContainer::new(27, Rc::new(|| {}));
    source.set_item(0, Some(ItemResource::new("minecraft:dirt", 64)), 64);
    world.chests.insert(at(-1), source);
    world.chests.insert(
        at(kinds.len() as i32),
        VanillaContainer::new(27, Rc::new(|| {})),
    );
    for (x, kind) in kinds.iter().enumerate() {
        let pos = at(x as i32);
        let raw = if x == 0 { pulling_west() } else { normal() };
        world.infos.borrow_mut().insert(
            pos,
            TransporterInfo {
                tier: TransporterTier::Basic,
                kind: kind.clone(),
                saved: Some(saved(pos, raw, kind)),
            },
        );
    }
    let mut owner = owner();
    for x in 0..kinds.len() as i32 {
        owner.handle(&mut world, Event::ClearRemoved(at(x)));
    }
    for x in 0..kinds.len() as i32 {
        owner.handle(&mut world, Event::PlacedByItem(at(x)));
    }
    owner.handle(
        &mut world,
        Event::ServerTickPost {
            runs_normally: true,
        },
    );
    (owner, world)
}

fn logistical() -> TransporterKind {
    TransporterKind::Logistical { color: None }
}

fn tick(owner: &mut InventoryTransmitters, world: &mut SimWorld, count: usize, length: i32) {
    for _ in 0..count {
        for x in 0..length {
            owner.handle(world, Event::Update(at(x)));
        }
        owner.handle(
            world,
            Event::ServerTickPost {
                runs_normally: true,
            },
        );
    }
}

fn dirt_in(chest: &Rc<VanillaContainer>) -> u32 {
    chest
        .stacks()
        .iter()
        .filter(|(_, item, _)| &*item.id == "minecraft:dirt")
        .map(|(_, _, amount)| *amount)
        .sum()
}

/// A linha de verdade leva a terra de um baú ao outro (`pull_line` do original, com os eventos do jogo).
#[test]
fn a_pulling_line_carries_items_from_one_chest_to_the_other() {
    let (mut owner, mut world) = line(&[logistical(), logistical(), logistical()]);
    tick(&mut owner, &mut world, 200, 3);
    let source = world.chests[&at(-1)].clone();
    let target = world.chests[&at(3)].clone();
    assert!(dirt_in(&target) > 0, "nada chegou");
    assert_eq!(
        dirt_in(&source) + dirt_in(&target),
        64 - transit_total(&owner, 3)
    );
}

fn transit_total(owner: &InventoryTransmitters, length: i32) -> u32 {
    (0..length)
        .map(|x| {
            owner
                .transporter_at(at(x))
                .unwrap()
                .transit_stacks()
                .iter()
                .map(|(_, stack)| stack.borrow().size())
                .sum::<u32>()
        })
        .sum()
}

/// `for_each_transit_item` (o que a cola pergunta a cada evento, sem ordenar as posições) vê as mesmas pilhas que o passeio por `positions` e `transit_stacks`, a cada tick, com a linha
/// carregando itens e depois de um transportador sair (decisão 202).
#[test]
fn the_unsorted_walk_over_the_transit_items_sees_what_the_sorted_one_sees() {
    let (mut owner, mut world) = line(&[logistical(), logistical(), logistical()]);
    let mut seen_in_transit = 0;
    for step in 0..120 {
        tick(&mut owner, &mut world, 1, 3);
        if step == 90 {
            owner.handle(&mut world, Event::Removed(at(1)));
        }
        let mut sorted: Vec<ItemResource> = Vec::new();
        for pos in owner.positions() {
            for (_, stack) in owner.transporter_at(pos).unwrap().transit_stacks() {
                if let Some(item) = stack.borrow().item_type() {
                    sorted.push(item.clone());
                }
            }
        }
        let mut walked: Vec<ItemResource> = Vec::new();
        owner.for_each_transit_item(|item| walked.push(item.clone()));
        let key = |item: &ItemResource| format!("{item:?}");
        sorted.sort_by_key(key);
        walked.sort_by_key(key);
        assert_eq!(walked, sorted, "passo {step}");
        seen_in_transit += walked.len();
    }
    assert!(seen_in_transit > 0, "a linha nunca teve nada em trânsito: o teste não olhou nada");
}

/// O NBT do transportador com pilhas em trânsito volta ao ser lido (`LogisticalTransporterBase.write` e `read`, a cor, as ligações): as pilhas entram com ids novos.
#[test]
fn the_transit_list_and_the_color_survive_a_save_and_a_reload() {
    let kinds = [
        TransporterKind::Logistical { color: Some(3) },
        TransporterKind::Logistical { color: Some(3) },
        TransporterKind::Logistical { color: Some(3) },
    ];
    let (mut owner, mut world) = line(&kinds);
    tick(&mut owner, &mut world, 25, 3);
    assert!(
        transit_total(&owner, 3) > 0,
        "nenhuma pilha em trânsito para salvar"
    );
    let encode =
        |item: &ItemResource| machinery_basis_core::transporter::stack::plain_item_encode(item);
    let disk: Vec<SavedTransporter> = (0..3)
        .map(|x| owner.write(at(x), &encode).expect("o transportador existe"))
        .collect();
    assert!(
        disk.iter()
            .all(|saved| saved.color == Some(Nbt::String("cyan".into()))),
        "a cor 3 (ciano) sai pelo nome"
    );
    let expected: Vec<Vec<(String, u32)>> = (0..3)
        .map(|x| {
            owner
                .transporter_at(at(x))
                .unwrap()
                .transit_stacks()
                .iter()
                .map(|(_, stack)| {
                    (
                        stack.borrow().item_type().unwrap().id.to_string(),
                        stack.borrow().size(),
                    )
                })
                .collect()
        })
        .collect();
    // O mundo reabre: o dono é outro e os block entities chegam do disco.
    let fresh = SimWorld {
        chests: world.chests.clone(),
        ..SimWorld::default()
    };
    let mut fresh = fresh;
    for (x, saved) in disk.into_iter().enumerate() {
        fresh.infos.borrow_mut().insert(
            at(x as i32),
            TransporterInfo {
                tier: TransporterTier::Basic,
                kind: logistical(),
                saved: Some(saved),
            },
        );
    }
    let mut reopened = self::owner();
    for x in 0..3 {
        reopened.handle(&mut fresh, Event::ClearRemoved(at(x)));
    }
    for x in 0..3 {
        let base = reopened.transporter_at(at(x)).unwrap();
        assert_eq!(base.get_color(), Some(3), "a cor de {x}");
        let got: Vec<(String, u32)> = base
            .transit_stacks()
            .iter()
            .map(|(_, stack)| {
                (
                    stack.borrow().item_type().unwrap().id.to_string(),
                    stack.borrow().size(),
                )
            })
            .collect();
        assert_eq!(got, expected[x as usize], "as pilhas de {x}");
    }
}

/// O chunk que descarrega (`onChunkUnloaded` e depois `setRemoved`) não solta nada: as pilhas ficam no NBT. O bloco quebrado solta o que leva (`preRemoveSideEffects`).
#[test]
fn an_unloading_chunk_keeps_the_stacks_and_a_broken_block_drops_them() {
    for unload in [true, false] {
        let (mut owner, mut world) = line(&[logistical(), logistical(), logistical()]);
        tick(&mut owner, &mut world, 25, 3);
        let carrying = (0..3)
            .find(|x| {
                !owner
                    .transporter_at(at(*x))
                    .unwrap()
                    .transit_stacks()
                    .is_empty()
            })
            .expect("algum transportador leva uma pilha");
        let held: u32 = owner
            .transporter_at(at(carrying))
            .unwrap()
            .transit_stacks()
            .iter()
            .map(|(_, stack)| stack.borrow().size())
            .sum();
        if unload {
            owner.handle(&mut world, Event::ChunkUnloaded(at(carrying)));
        }
        owner.handle(&mut world, Event::Removed(at(carrying)));
        let dropped: u32 = world.drops.iter().map(|drop| drop.amount).sum();
        if unload {
            assert_eq!(dropped, 0, "o chunk que descarrega não solta nada");
        } else {
            assert_eq!(dropped, held, "o bloco quebrado solta tudo o que levava");
        }
        assert!(owner.transporter_at(at(carrying)).is_none());
        let flowing: usize = (-1..=4)
            .map(|x| owner.manager().flowing_to(DIMENSION, at(x).as_long()).len())
            .sum();
        let others: u32 = (0..3)
            .filter(|x| *x != carrying)
            .map(|x| {
                owner
                    .transporter_at(at(x))
                    .map_or(0, |base| base.transit_stacks().len() as u32)
            })
            .sum();
        assert_eq!(
            flowing as u32, others,
            "`remove()` tira as pilhas do transportador das em voo"
        );
    }
}

/// Duas cores diferentes não se ligam (`isValidTransmitterBasic`): a rede se parte; as cores nulas ou iguais ligam.
#[test]
fn contradicting_colors_do_not_connect() {
    let (owner, world) = line(&[
        TransporterKind::Logistical { color: Some(1) },
        TransporterKind::Logistical { color: Some(2) },
    ]);
    assert_ne!(owner.network_of(at(0)), owner.network_of(at(1)));
    // O braço entre os dois não existe: o lado leste do primeiro e o oeste do segundo (a regra de `Transmitter`, antes de a rede ver os dois).
    let east = owner.view_at(&world, at(0)).unwrap();
    let west = owner.view_at(&world, at(1)).unwrap();
    assert!(!east.connected[Face::East as usize] && !west.connected[Face::West as usize]);
    let (owner, _) = line(&[
        TransporterKind::Logistical { color: Some(1) },
        TransporterKind::Logistical { color: None },
    ]);
    assert_eq!(owner.network_of(at(0)), owner.network_of(at(1)));
    let (owner, _) = line(&[
        TransporterKind::Logistical { color: Some(1) },
        TransporterKind::Logistical { color: Some(1) },
    ]);
    assert_eq!(owner.network_of(at(0)), owner.network_of(at(1)));
}

/// O Configurator agachado num lado com conexão e fora de um braço: `onConfigure` troca a cor (nenhuma, a primeira, ..., a última, nenhuma), esvazia a cache, salva e avisa; sem
/// o braço e sem conexão, cai no tipo de conexão do lado.
#[test]
fn the_configurator_bumps_the_color_of_a_logistical_transporter() {
    let (mut owner, mut world) = line(&[logistical(), logistical()]);
    let before = world.changed;
    // O puxar da linha já achou um caminho e a cache o guarda; trocar a cor o esquece (`PathfinderCache.onChanged`).
    tick(&mut owner, &mut world, 30, 2);
    assert!(
        owner.cached_paths(at(1)).unwrap() > 0,
        "a cache tem o caminho"
    );
    assert!(owner.sneak_right_click(&mut world, at(1), Face::Up, None));
    assert_eq!(
        owner.cached_paths(at(1)),
        Some(0),
        "a cor nova esvazia a cache da rede"
    );
    assert_eq!(owner.transporter_at(at(1)).unwrap().get_color(), Some(0));
    assert!(
        world
            .overlays
            .contains(&Overlay::ToggleColor { color: Some(0) })
    );
    assert!(world.changed > before, "marca para salvar");
    // O vizinho tem uma cor que se contradiz agora? Ainda não: a nula liga com tudo.
    assert_eq!(owner.network_of(at(0)), owner.network_of(at(1)));
    // O clique agachado num braço (o lado leste do primeiro) cicla o tipo de conexão, não a cor.
    assert!(owner.sneak_right_click(&mut world, at(0), Face::Up, Some(Face::East)));
    assert_eq!(owner.transporter_at(at(0)).unwrap().get_color(), None);
    assert_eq!(
        owner
            .transporter_at(at(0))
            .unwrap()
            .transmitter
            .get_connection_type_raw(Face::East),
        ConnectionType::Push
    );
    // O clique sem agachar só mostra a cor.
    assert!(owner.right_click(&mut world, at(1), Face::Up, None));
    assert_eq!(
        world.overlays.last(),
        Some(&Overlay::CurrentColor { color: Some(0) })
    );
}

/// O desviador: o clique troca o modo do lado (`DiversionControl.getNext`) e avisa a capability se o modo muda o que o lado expõe; o restritivo não tem clique.
#[test]
fn the_configurator_cycles_the_mode_of_a_diversion_transporter() {
    let diversion = TransporterKind::Diversion {
        modes: [DiversionControl::Disabled; 6],
    };
    let (mut owner, mut world) = line(&[logistical(), diversion, logistical()]);
    assert!(owner.right_click(&mut world, at(1), Face::Up, None));
    assert_eq!(
        world.overlays.last(),
        Some(&Overlay::ToggleDiverter {
            mode: DiversionControl::High
        })
    );
    // Sem sinal, `HIGH` não está cumprido: o lado de cima (`NORMAL`, aceitava) deixa de expor a capability.
    assert_eq!(world.capabilities, vec![CapabilityChange::Hidden(Face::Up)]);
    // O mesmo clique de novo: `HIGH` -> `LOW`.
    assert!(owner.right_click(&mut world, at(1), Face::Up, None));
    assert_eq!(
        world.overlays.last(),
        Some(&Overlay::ToggleDiverter {
            mode: DiversionControl::Low
        })
    );
    // `LOW` com o sinal desligado está cumprido: a capability volta (`invalidateCapabilities`).
    assert_eq!(world.capabilities.last(), Some(&CapabilityChange::Exposed));
    let (mut owner, mut world) = line(&[TransporterKind::Restrictive, logistical()]);
    let before = world.overlays.len();
    assert!(owner.right_click(&mut world, at(0), Face::Up, None));
    assert_eq!(world.overlays.len(), before, "o restritivo não manda aviso");
}

/// O que a sobreposição do Configurator lê (`transporter.getTransmitter().modes[face.ordinal()]`): o modo de cada lado do desviador, que o clique troca; o logístico e o restritivo
/// não têm modos.
#[test]
fn the_diversion_modes_are_what_the_overlay_reads() {
    let diversion = TransporterKind::Diversion {
        modes: [DiversionControl::Disabled; 6],
    };
    let (mut owner, mut world) = line(&[logistical(), diversion, TransporterKind::Restrictive]);
    assert_eq!(
        owner.transporter_at(at(1)).unwrap().diversion_modes(),
        Some([DiversionControl::Disabled; 6])
    );
    assert!(owner.right_click(&mut world, at(1), Face::Up, None));
    assert!(owner.right_click(&mut world, at(1), Face::Up, None));
    let mut expected = [DiversionControl::Disabled; 6];
    expected[Face::Up as usize] = DiversionControl::Low;
    assert_eq!(
        owner.transporter_at(at(1)).unwrap().diversion_modes(),
        Some(expected)
    );
    owner.set_diversion_mode(&mut world, at(1), Face::South, DiversionControl::High);
    expected[Face::South as usize] = DiversionControl::High;
    assert_eq!(
        owner.transporter_at(at(1)).unwrap().diversion_modes(),
        Some(expected)
    );
    assert_eq!(owner.transporter_at(at(0)).unwrap().diversion_modes(), None);
    assert_eq!(owner.transporter_at(at(2)).unwrap().diversion_modes(), None);
}

/// A capability de item (`TransporterItemHandler`): existe onde `exposesInsertCap` vale, e só põe algo com rede.
#[test]
fn the_item_capability_exists_where_the_side_accepts() {
    let (mut owner, mut world) = line(&[logistical(), logistical()]);
    let dirt = ItemResource::new("minecraft:dirt", 64);
    // De cima: `NORMAL` aceita.
    let inserted = owner.insert(&mut world, at(0), Face::Up, &dirt, 5);
    assert_eq!(
        inserted,
        Some(1),
        "o `basic` leva uma por vez (`pullAmount` 1)"
    );
    // Sem a capability: o lado `NONE`.
    let pos = at(5);
    let mut raw = normal();
    raw[Face::Up as usize] = ConnectionType::None;
    world.infos.borrow_mut().insert(
        pos,
        TransporterInfo {
            tier: TransporterTier::Basic,
            kind: logistical(),
            saved: Some(saved(pos, raw, &logistical())),
        },
    );
    owner.handle(&mut world, Event::ClearRemoved(pos));
    owner.handle(&mut world, Event::PlacedByItem(pos));
    assert_eq!(owner.insert(&mut world, pos, Face::Up, &dirt, 1), None);
    // Sem rede (o primeiro tick ainda não passou), nada entra.
    assert_eq!(owner.insert(&mut world, pos, Face::Down, &dirt, 1), Some(0));
}

/// O sinal de redstone de um desviador vale para quem pergunta por ele: o vizinho que recebe o aviso lê o modo do outro lado contra o sinal de agora (`canConnect` do vizinho), e não contra o
/// que o outro lia da última vez que andou.
#[test]
fn a_neighbor_diversion_transporter_is_read_against_the_current_signal() {
    let mut modes = [DiversionControl::Disabled; 6];
    modes[Face::West as usize] = DiversionControl::High;
    modes[Face::East as usize] = DiversionControl::High;
    let diversion = TransporterKind::Diversion { modes };
    let (mut owner, mut world) = line(&[diversion.clone(), diversion]);
    assert!(!owner.view_at(&world, at(0)).unwrap().connected[Face::East as usize]);
    world.powered.set(true);
    owner.handle(&mut world, Event::Neighbor(at(0)));
    assert!(owner.view_at(&world, at(0)).unwrap().connected[Face::East as usize]);
}

/// Um item com componentes (um nome, o dano) que passa por uma linha de transportadores sai do NBT salvo com os componentes e volta ao reabrir o mundo igual, sem virar o item sem
/// componentes (`ItemResource.CODEC` leva o `components` na lista `items`). Com o codec só do id, o item volta sem eles.
#[test]
fn an_item_with_components_survives_a_save_and_a_reload() {
    let named = ItemResource::with_components(
        "minecraft:iron_pickaxe",
        1,
        Components::new([
            ("minecraft:custom_name", "\"Rápida\""),
            ("minecraft:damage", "40"),
        ]),
    );
    for with_codec in [true, false] {
        let (mut owner, mut world) = line(&[logistical(), logistical(), logistical()]);
        world.chests[&at(-1)].set_item(0, Some(named.clone()), 1);
        tick(&mut owner, &mut world, 25, 3);
        assert!(
            transit_total(&owner, 3) > 0,
            "nenhuma pilha em trânsito para salvar"
        );
        let encode = |item: &ItemResource| {
            if with_codec {
                component_encode(item)
            } else {
                machinery_basis_core::transporter::stack::plain_item_encode(item)
            }
        };
        let mut seen = 0;
        let mut fresh = SimWorld {
            chests: world.chests.clone(),
            ..SimWorld::default()
        };
        for x in 0..3 {
            let saved = owner.write(at(x), &encode).expect("o transportador existe");
            let list = saved.items.clone();
            seen += list.as_ref().map_or(0, |items| {
                let text = machinery_basis_core::transporter::nbt::to_snbt(items);
                usize::from(text.contains("minecraft:damage"))
            });
            fresh.infos.borrow_mut().insert(
                at(x),
                TransporterInfo {
                    tier: TransporterTier::Basic,
                    kind: logistical(),
                    saved: Some(saved),
                },
            );
        }
        let mut reopened = self::owner();
        for x in 0..3 {
            reopened.handle(&mut fresh, Event::ClearRemoved(at(x)));
        }
        let back: Vec<ItemResource> = (0..3)
            .flat_map(|x| {
                reopened
                    .transporter_at(at(x))
                    .unwrap()
                    .transit_stacks()
                    .iter()
                    .filter_map(|(_, stack)| stack.borrow().item_type().cloned())
                    .collect::<Vec<_>>()
            })
            .collect();
        // O pacote de atualização (`writeReducedUpdatedTag`) leva o item inteiro também.
        let updates: Vec<Nbt> = (0..3)
            .filter_map(|x| {
                owner
                    .transporter_at(at(x))
                    .unwrap()
                    .write_reduced_updated_items_with(&encode)
            })
            .collect();
        let carries = updates.iter().any(|list| {
            machinery_basis_core::transporter::nbt::to_snbt(list).contains("minecraft:damage")
        });
        assert_eq!(
            carries, with_codec,
            "o pacote de atualização e os componentes"
        );
        let pickaxes = back
            .iter()
            .filter(|item| &*item.id == "minecraft:iron_pickaxe")
            .count();
        assert!(pickaxes > 0, "a ferramenta não estava em trânsito");
        if with_codec {
            assert!(seen > 0, "o NBT salvo não leva os componentes");
            assert!(
                back.iter()
                    .filter(|item| &*item.id == "minecraft:iron_pickaxe")
                    .all(|item| item.components == named.components),
                "o item voltou sem os componentes"
            );
        } else {
            assert_eq!(seen, 0);
            assert!(
                back.iter()
                    .filter(|item| &*item.id == "minecraft:iron_pickaxe")
                    .all(|item| item.components.is_empty())
            );
        }
    }
}

/// O que o funil recebe de volta (`hopper_insert`): o que não entrou fica com ele, e `moved` só vale se algo entrou.
#[test]
fn the_hopper_gets_back_what_the_transporter_did_not_take() {
    assert_eq!(hopper_outcome(64, Some(34)).rest, 30);
    assert!(hopper_outcome(64, Some(34)).moved);
    assert_eq!(hopper_outcome(64, Some(64)).rest, 0);
    assert!(hopper_outcome(64, Some(64)).moved);
    // Nada entrou: tudo volta e nada se moveu, como quando o lado não expõe a capability (`None`).
    for inserted in [Some(0), None] {
        let outcome = hopper_outcome(64, inserted);
        assert_eq!(outcome.rest, 64);
        assert!(!outcome.moved);
    }
    // O dono nunca devolve mais do que foi oferecido; se devolvesse, o resto não pode passar do oferecido nem ficar negativo.
    let outcome = hopper_outcome(5, Some(9));
    assert_eq!(outcome.rest, 0);
    assert!(outcome.moved);
}

/// A conta inteira com o dono: um funil que oferece 64 de terra a um transportador básico (vazão `pullAmount` do tier) volta com o resto, e o que entrou está em trânsito.
#[test]
fn a_hopper_offering_more_than_the_pull_amount_gets_the_rest_back() {
    let (mut owner, mut world) = line(&[logistical(), logistical(), logistical()]);
    let dirt = ItemResource::new("minecraft:dirt", 64);
    let up = Face::ALL[1];
    let inserted = owner.insert(&mut world, at(0), up, &dirt, 64);
    let outcome = hopper_outcome(64, inserted);
    assert!(outcome.moved, "nada entrou");
    assert_eq!(u64::from(outcome.rest), 64 - u64::from(inserted.unwrap()));
    assert!(outcome.rest > 0, "o básico não leva 64 de uma vez");
    // O que entrou está em trânsito (mais o que a linha já puxava do baú: a soma é só do funil).
    let inside = transit_total(&owner, 3);
    assert!(
        inside >= inserted.unwrap(),
        "a pilha do funil não entrou em trânsito"
    );
}

/// `writeReducedUpdatedTag` do transportador (`LogisticalTransporterBase` e `LogisticalTransporter`): a lista `items` com o `index` e o item inteiro de cada pilha em trânsito, a cor e
/// a rede; o cliente (`handleUpdateTag`) guarda as pilhas pelo índice e a cor, e o pacote atravessa o fio.
#[test]
fn the_update_packet_carries_the_stacks_and_the_color_and_the_client_keeps_them() {
    use machinery_basis_core::transmission::client::{ClientKind, ClientTransmitters, TagMessage};
    let named = ItemResource::with_components(
        "minecraft:iron_pickaxe",
        1,
        Components::new([("minecraft:damage", "40")]),
    );
    let (mut owner, mut world) = line(&[logistical(), logistical(), logistical()]);
    world.chests[&at(-1)].set_item(0, Some(named.clone()), 3);
    tick(&mut owner, &mut world, 25, 3);
    owner.set_color(&mut world, at(1), Some(3));
    let (x, count) = (0..3)
        .map(|x| {
            (
                x,
                owner.transporter_at(at(x)).unwrap().transit_stacks().len(),
            )
        })
        .max_by_key(|(_, count)| *count)
        .unwrap();
    assert!(count > 0, "nenhuma pilha em trânsito");
    let (tag, capacity) = owner
        .client_update(at(x), &component_encode)
        .expect("pacote");
    assert_eq!(capacity, 0, "a rede de itens não tem buffer");
    let message = TagMessage {
        dimension: "minecraft:overworld".into(),
        pos: at(x),
        kind: ClientKind::Items,
        capacity,
        tag,
    };
    let message = TagMessage::decode(&message.encode()).expect("o pacote atravessa o fio");
    let mut client = ClientTransmitters::new();
    client.add_tile(at(x), ClientKind::Items, 0);
    client.handle_update_tag(at(x), &message.tag);
    let extra = client.extra_at(at(x)).expect("tile");
    assert_eq!(
        extra.transit.len(),
        count,
        "uma pilha do cliente por pilha do servidor"
    );
    for stack in &extra.transit {
        let (id, amount) = stack.item.clone().expect("o item");
        assert_eq!(id, "minecraft:iron_pickaxe");
        assert!(amount > 0);
        assert!(
            stack.components.is_some(),
            "os componentes vão no pacote: {stack:?}"
        );
    }
    let server: Vec<i32> = owner
        .transporter_at(at(x))
        .unwrap()
        .transit_stacks()
        .iter()
        .map(|(id, _)| *id)
        .collect();
    let mut indices: Vec<i32> = extra.transit.iter().map(|stack| stack.index).collect();
    indices.sort_unstable();
    let mut expected = server;
    expected.sort_unstable();
    assert_eq!(indices, expected, "o índice é o id da pilha no servidor");
    // A cor só existe no transportador que a tem (`LogisticalTransporter`).
    let colored = owner.client_update(at(1), &component_encode).unwrap().0;
    let mut other = ClientTransmitters::new();
    other.add_tile(at(1), ClientKind::Items, 0);
    other.handle_update_tag(at(1), &colored);
    assert_eq!(other.extra_at(at(1)).unwrap().color, Some(3));
    assert_eq!(
        client.extra_at(at(x)).unwrap().color,
        if x == 1 { Some(3) } else { None }
    );
    // Um pacote seguinte sem pilhas esvazia o `transit` (`transit.clear()` no `handleUpdateTag`).
    let mut empty = colored.clone();
    empty.remove("items");
    other.handle_update_tag(at(1), &empty);
    assert!(other.extra_at(at(1)).unwrap().transit.is_empty());
}

/// `ItemAlloy` num transportador logístico (`LogisticalTransporterUpgradeData`): as pilhas em trânsito, a cor e os tipos de conexão crus passam ao bloco novo, nada cai no chão
/// (`isUpgrading` impede o `preRemoveSideEffects`) e a rede se refaz.
#[test]
fn the_alloy_raises_the_transporters_and_keeps_the_stacks_the_color_and_the_connections() {
    let (mut owner, mut world) = line(&[logistical(), logistical(), logistical()]);
    tick(&mut owner, &mut world, 25, 3);
    owner.set_color(&mut world, at(1), Some(5));
    let before = transit_total(&owner, 3);
    assert!(before > 0, "nenhuma pilha em trânsito");
    let network = owner.network_of(at(0));
    let mut run = owner.alloy_start(at(1), 1, &|_| Some(0)).expect("há rede");
    assert_eq!(
        run.targets,
        vec![at(1), at(0), at(2)],
        "do clique para fora"
    );
    for target in run.targets.clone() {
        let data = owner
            .alloy_take(&mut world, &mut run, target)
            .expect("dado");
        // `Level.setBlockAndUpdate`: o bloco velho sai e o novo entra, vazio (o dado chega depois).
        owner.handle(&mut world, Event::Removed(target));
        world.infos.borrow_mut().insert(
            target,
            TransporterInfo {
                tier: TransporterTier::Advanced,
                kind: logistical(),
                saved: None,
            },
        );
        owner.handle(&mut world, Event::ClearRemoved(target));
        assert!(owner.alloy_give(&mut run, target, data));
    }
    owner.alloy_finish(&mut world, &run);
    assert_eq!(run.upgraded, 3);
    assert!(
        world.drops.is_empty(),
        "a troca não solta as pilhas: {:?}",
        world.drops
    );
    assert_eq!(
        transit_total(&owner, 3),
        before,
        "as pilhas passaram ao bloco novo"
    );
    assert_eq!(owner.transporter_at(at(1)).unwrap().get_color(), Some(5));
    let raw = *owner
        .transporter_at(at(0))
        .unwrap()
        .transmitter
        .connection_types_raw();
    assert_eq!(raw[WEST], ConnectionType::Pull, "o puxar do baú continua");
    tick(&mut owner, &mut world, 2, 3);
    assert_ne!(
        owner.network_of(at(0)),
        network,
        "a rede velha se invalidou"
    );
    assert_eq!(owner.network_of(at(0)), owner.network_of(at(2)));
    // O restritivo e o desviador não são `IUpgradeableTransmitter`: a lista os deixa de fora.
    let none = owner.alloy_start(at(1), 1, &|_| None).expect("há rede");
    assert!(none.targets.is_empty());
}

/// Nenhuma terra se perde na troca (em jogo, com a liga no meio da linha, chegaram 62 de 64): a linha de três com a terra em trânsito sobe de nível e entrega as 64.
#[test]
fn the_alloy_loses_no_item_in_flight() {
    let (mut owner, mut world) = line(&[logistical(), logistical(), logistical()]);
    tick(&mut owner, &mut world, 25, 3);
    let mut run = owner.alloy_start(at(1), 1, &|_| Some(0)).expect("há rede");
    for target in run.targets.clone() {
        let data = owner
            .alloy_take(&mut world, &mut run, target)
            .expect("dado");
        owner.handle(&mut world, Event::Removed(target));
        world.infos.borrow_mut().insert(
            target,
            TransporterInfo {
                tier: TransporterTier::Advanced,
                kind: logistical(),
                saved: None,
            },
        );
        owner.handle(&mut world, Event::ClearRemoved(target));
        assert!(owner.alloy_give(&mut run, target, data));
    }
    owner.alloy_finish(&mut world, &run);
    tick(&mut owner, &mut world, 1_500, 3);
    assert_eq!(
        dirt_in(&world.chests[&at(3)]),
        64,
        "tudo chega ao baú de destino"
    );
}

type Seen = (i32, String, u32, Option<i64>, Option<i64>);

/// O que o servidor tem em trânsito num transportador: o índice, o item, a quantidade e o próximo e o anterior (`getNext` e `getPrev` do transportador) de cada pilha, em ordem de índice.
fn server_stacks(owner: &InventoryTransmitters, pos: BlockPos) -> Vec<Seen> {
    let known = |position: i64| (position != NO_POS).then_some(position);
    let mut out: Vec<Seen> = owner
        .transporter_at(pos)
        .unwrap()
        .transit_stacks()
        .iter()
        .map(|(id, stack)| {
            let stack = stack.borrow();
            (
                *id,
                stack.item_type().unwrap().id.to_string(),
                stack.size(),
                known(stack.get_next(pos.as_long())),
                known(stack.get_prev(pos.as_long())),
            )
        })
        .collect();
    out.sort();
    out
}

/// O que o cliente guardou de um transportador, no mesmo formato.
fn client_stacks(client: &ClientTransmitters, pos: BlockPos) -> Vec<Seen> {
    let mut out: Vec<Seen> = client
        .extra_at(pos)
        .unwrap()
        .transit
        .iter()
        .map(|stack| {
            let (item, amount) = stack.item.clone().expect("pilha sem item");
            (stack.index, item, amount as u32, stack.next, stack.previous)
        })
        .collect();
    out.sort();
    out
}

fn packet_op(pos: BlockPos, packet: TransporterPacket) -> Op {
    match packet {
        TransporterPacket::Sync { id, stack } => Op::TransporterSync {
            dimension: "o".into(),
            pos,
            id,
            stack,
        },
        TransporterPacket::Batch { deletes, updates } => Op::TransporterBatch {
            dimension: "o".into(),
            pos,
            deletes,
            updates,
        },
    }
}

/// Os pacotes de movimento (`PacketTransporterSync` ao entrar a pilha, `PacketTransporterBatch` ao fim do tick do transportador) bastam para o cliente seguir a lista de pilhas de cada
/// transportador, tick a tick, pelo fio (lotes codificados e lidos): depois de cada tick, a lista do cliente é a que o servidor tinha ao fim do `Update` do próprio transportador.
#[test]
fn the_client_follows_the_stacks_of_every_transporter_from_the_packets_alone() {
    let length = 5;
    let (mut owner, mut world) = line(&vec![logistical(); length]);
    world.packets.clear();
    let mut client = ClientTransmitters::new();
    for x in 0..length as i32 {
        client.add_tile(at(x), ClientKind::Items, 0);
    }
    let (mut syncs, mut batches, mut seen) = (0, 0, 0);
    for tick_number in 0..1_500 {
        let mut expected = Vec::new();
        for x in 0..length as i32 {
            owner.handle(&mut world, Event::Update(at(x)));
            expected.push(server_stacks(&owner, at(x)));
        }
        owner.handle(
            &mut world,
            Event::ServerTickPost {
                runs_normally: true,
            },
        );
        let ops: Vec<Op> = world
            .packets
            .drain(..)
            .map(|(pos, packet)| {
                match &packet {
                    TransporterPacket::Sync { .. } => syncs += 1,
                    TransporterPacket::Batch { .. } => batches += 1,
                }
                packet_op(pos, packet)
            })
            .collect();
        for batch in encode_batches(&ops) {
            for op in decode_batch(&batch).expect("o lote volta") {
                client.apply(&op);
            }
        }
        for (x, want) in expected.iter().enumerate() {
            seen += want.len();
            assert_eq!(
                &client_stacks(&client, at(x as i32)),
                want,
                "tick {tick_number}, transportador {x}"
            );
        }
    }
    assert!(
        syncs >= 20 && batches >= 20,
        "{syncs} sincronias, {batches} lotes"
    );
    assert!(seen > 100, "o cliente quase não viu pilhas: {seen}");
    assert_eq!(dirt_in(&world.chests[&at(length as i32)]), 64);
}

/// O pacote leva a pilha como o `STREAM_CODEC` a leva: o progresso zero que o servidor manda ao emitir a pilha é lido como 5, e o próximo e o anterior vêm do `updateForPos`.
#[test]
fn the_sync_packet_carries_the_stack_as_the_stream_codec_does() {
    let (mut owner, mut world) = line(&vec![logistical(); 3]);
    world.packets.clear();
    for _ in 0..8 {
        for x in 0..3 {
            owner.handle(&mut world, Event::Update(at(x)));
        }
        if !world.packets.is_empty() {
            break;
        }
    }
    let (pos, packet) = world.packets.first().cloned().expect("a primeira pilha");
    assert_eq!(pos, at(0));
    let TransporterPacket::Sync { id, stack } = packet else {
        panic!("o primeiro pacote é a sincronia da pilha nova");
    };
    assert_eq!(id, 0);
    let mut client = ClientTransmitters::new();
    client.add_tile(at(0), ClientKind::Items, 0);
    client.apply(&Op::TransporterSync {
        dimension: "o".into(),
        pos: at(0),
        id,
        stack,
    });
    let seen = &client.extra_at(at(0)).unwrap().transit[0];
    assert_eq!(seen.item, Some(("minecraft:dirt".into(), 1)));
    assert!(seen.progress > 0, "progresso 0 vira 5 no cliente");
    assert!(seen.next.is_some() && seen.previous.is_some(), "{seen:?}");
    // Sem o tile no cliente o pacote se perde.
    let mut empty = ClientTransmitters::new();
    empty.on_transporter_batch(at(0), &[], &[]);
    assert!(empty.extra_at(at(0)).is_none());
}

/// O destino visto pelo espelho que a cola do jogo usa (`HostItemHandler`, relido no começo de cada evento e aplicado no fim): a linha entrega os 64.
#[test]
fn the_mirror_of_the_destination_chest_takes_all_64() {
    let length = 5;
    let (mut owner, mut world) = line(&vec![logistical(); length]);
    world.mirrored = true;
    for tick_number in 0..1_500 {
        for x in 0..length as i32 {
            world.refresh_mirrors();
            owner.handle(&mut world, Event::Update(at(x)));
            world.flush_mirrors();
        }
        world.refresh_mirrors();
        owner.handle(
            &mut world,
            Event::ServerTickPost {
                runs_normally: true,
            },
        );
        world.flush_mirrors();
        let _ = tick_number;
    }
    assert_eq!(
        dirt_in(&world.chests[&at(length as i32)]),
        64,
        "entregue ao baú de destino pelo espelho"
    );
}

/// A liga no meio da linha com o destino visto pelo espelho (como o jogo o mostra), e a linha segue com a velocidade e o puxar do avançado: os 64 chegam ao baú.
#[test]
fn the_alloy_in_the_middle_of_a_line_with_the_mirrored_destination_delivers_all_64() {
    let length = 5;
    let (mut owner, mut world) = line(&vec![logistical(); length]);
    world.mirrored = true;
    let run_ticks = |owner: &mut InventoryTransmitters, world: &mut SimWorld, count: usize| {
        for _ in 0..count {
            for x in 0..length as i32 {
                world.refresh_mirrors();
                owner.handle(world, Event::Update(at(x)));
                world.flush_mirrors();
            }
            world.refresh_mirrors();
            owner.handle(
                world,
                Event::ServerTickPost {
                    runs_normally: true,
                },
            );
            world.flush_mirrors();
        }
    };
    run_ticks(&mut owner, &mut world, 26);
    let mut run = owner.alloy_start(at(2), 1, &|_| Some(0)).expect("há rede");
    for target in run.targets.clone() {
        let data = owner
            .alloy_take(&mut world, &mut run, target)
            .expect("dado");
        owner.handle(&mut world, Event::Removed(target));
        world.infos.borrow_mut().insert(
            target,
            TransporterInfo {
                tier: TransporterTier::Advanced,
                kind: logistical(),
                saved: None,
            },
        );
        owner.handle(&mut world, Event::ClearRemoved(target));
        assert!(owner.alloy_give(&mut run, target, data));
    }
    owner.alloy_finish(&mut world, &run);
    run_ticks(&mut owner, &mut world, 1_500);
    assert_eq!(
        dirt_in(&world.chests[&at(length as i32)]),
        64,
        "entregue ao baú de destino pelo espelho"
    );
}

/// `TransitRequest.eject` de uma máquina para um `TransporterItemHandler` (`LogisticalTransporterBase.insert` com `outputColor == null ? transporter.getColor() : outputColor`): a cor do
/// ejetor vai para a pilha, e um transportador de outra cor recusa (`canInsertToTransporter`); sem cor de saída vale a do transportador.
#[test]
fn the_output_color_of_an_ejector_decides_what_a_colored_transporter_takes() {
    use machinery_basis_core::transaction::Transaction;
    use machinery_basis_core::transit::TransitRequest;
    let dirt = ItemResource::new("minecraft:dirt", 64);
    let try_insert =
        |owner: &mut InventoryTransmitters, world: &mut SimWorld, color: Option<u8>| {
            let transaction = Transaction::open_root();
            let request = TransitRequest::simple(&dirt, 8);
            let response = owner
                .insert_request(world, at(0), at(-1), &request, color, 1, &transaction)
                .expect("o lado expõe a capability");
            let sent = response.sending_amount();
            transaction.commit();
            sent
        };
    let (mut owner, mut world) = line(&[
        TransporterKind::Logistical { color: Some(1) },
        logistical(),
        logistical(),
    ]);
    assert_eq!(
        try_insert(&mut owner, &mut world, Some(2)),
        0,
        "azul no verde"
    );
    assert_eq!(
        try_insert(&mut owner, &mut world, Some(1)),
        8,
        "a cor do transportador"
    );
    assert_eq!(
        try_insert(&mut owner, &mut world, None),
        8,
        "sem cor de saída vale a do transportador"
    );
    let (mut owner, mut world) = line(&[logistical(), logistical(), logistical()]);
    assert_eq!(
        try_insert(&mut owner, &mut world, Some(2)),
        8,
        "o transportador sem cor aceita qualquer"
    );
    assert_eq!(try_insert(&mut owner, &mut world, None), 8);
    // Um lado que não expõe a capability não responde (o ejetor nem o enxerga como alvo): longe do transportador.
    let transaction = Transaction::open_root();
    let request = TransitRequest::simple(&dirt, 8);
    assert!(
        owner
            .insert_request(&mut world, at(1), at(-1), &request, None, 1, &transaction)
            .is_none(),
        "quem não é vizinho do transportador"
    );
}
