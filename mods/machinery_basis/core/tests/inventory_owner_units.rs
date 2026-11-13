//! O dono dos transportadores logísticos (`InventoryTransmitters`) pelo que os rastros não cobrem: o NBT que sai e volta (a lista de pilhas em trânsito, a cor e o modo do desviador), o
//! chunk que descarrega (as pilhas ficam no disco, nada cai), as cores que se contradizem, o clique do Configurator (a cor do logístico, o modo do desviador) e a capability de item. Os
//! cenários do original (o movimento tick a tick, a quebra, o desviador pelo sinal, o funil) passam por ele em `transporter_move_reference.rs`.
use machinery_basis_core::containers::ItemResource;
use machinery_basis_core::item_handler::{ItemHandler, VanillaContainer};
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::BlockPos;
use machinery_basis_core::transmission::connection::{
    ConnectionType, TransmissionSet, TransmissionType,
};
use machinery_basis_core::transmission::inventory_owner::{
    InventoryOwnerWorld, InventoryTransmitters, ItemAcceptor, SavedTransporter, TransporterInfo,
};
use machinery_basis_core::transmission::owner::{CableView, Event};
use machinery_basis_core::transmission::transmitter::{Overlay, Transmitter};
use machinery_basis_core::transmission::{DimensionId, Uuid};
use machinery_basis_core::transporter::movement::WorldDrop;
use machinery_basis_core::transporter::nbt::{Compound, Nbt};
use machinery_basis_core::transporter::node::{
    CapabilityChange, DiversionControl, TransporterKind, TransporterTier,
};
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
}

impl InventoryOwnerWorld for SimWorld {
    fn is_block_loaded(&self, _: BlockPos) -> bool {
        true
    }

    fn is_getting_powered(&self, _: BlockPos) -> bool {
        self.powered.get()
    }

    fn item_handler(&self, pos: BlockPos, _: Face) -> Option<ItemAcceptor> {
        self.chests.get(&pos).map(|chest| chest.clone() as Rc<dyn ItemHandler>)
    }

    fn transporter_info(&mut self, pos: BlockPos) -> Option<TransporterInfo> {
        self.infos.borrow_mut().remove(&pos)
    }

    fn decode_item(&self, tag: &Compound) -> Option<ItemResource> {
        tag.get("id").and_then(Nbt::as_str).map(|id| ItemResource::new(id, 64))
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
            TransporterKind::Diversion { modes } => {
                Some(Nbt::IntArray(modes.iter().map(|mode| mode.ordinal()).collect()))
            }
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
    world
        .chests
        .insert(at(kinds.len() as i32), VanillaContainer::new(27, Rc::new(|| {})));
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
    owner.handle(&mut world, Event::ServerTickPost { runs_normally: true });
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
        owner.handle(world, Event::ServerTickPost { runs_normally: true });
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
    assert_eq!(dirt_in(&source) + dirt_in(&target), 64 - transit_total(&owner, 3));
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
    assert!(transit_total(&owner, 3) > 0, "nenhuma pilha em trânsito para salvar");
    let encode = |item: &ItemResource| machinery_basis_core::transporter::stack::plain_item_encode(item);
    let disk: Vec<SavedTransporter> = (0..3)
        .map(|x| owner.write(at(x), &encode).expect("o transportador existe"))
        .collect();
    assert!(disk.iter().all(|saved| saved.color == Some(Nbt::String("cyan".into()))), "a cor 3 (ciano) sai pelo nome");
    let expected: Vec<Vec<(String, u32)>> = (0..3)
        .map(|x| {
            owner
                .transporter_at(at(x))
                .unwrap()
                .transit_stacks()
                .iter()
                .map(|(_, stack)| (stack.borrow().item_type().unwrap().id.to_string(), stack.borrow().size()))
                .collect()
        })
        .collect();
    // O mundo reabre: o dono é outro e os block entities chegam do disco.
    let mut fresh = SimWorld::default();
    fresh.chests = world.chests.clone();
    for (x, saved) in disk.into_iter().enumerate() {
        fresh.infos.borrow_mut().insert(
            at(x as i32),
            TransporterInfo { tier: TransporterTier::Basic, kind: logistical(), saved: Some(saved) },
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
            .map(|(_, stack)| (stack.borrow().item_type().unwrap().id.to_string(), stack.borrow().size()))
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
                !owner.transporter_at(at(*x)).unwrap().transit_stacks().is_empty()
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
            .map(|x| owner.transporter_at(at(x)).map_or(0, |base| base.transit_stacks().len() as u32))
            .sum();
        assert_eq!(flowing as u32, others, "`remove()` tira as pilhas do transportador das em voo");
    }
}

/// Duas cores diferentes não se ligam (`isValidTransmitterBasic`): a rede se parte; as cores nulas ou iguais ligam.
#[test]
fn contradicting_colors_do_not_connect() {
    let (owner, _) = line(&[
        TransporterKind::Logistical { color: Some(1) },
        TransporterKind::Logistical { color: Some(2) },
    ]);
    assert_ne!(owner.network_of(at(0)), owner.network_of(at(1)));
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
    assert!(owner.sneak_right_click(&mut world, at(1), Face::Up, None));
    assert_eq!(owner.transporter_at(at(1)).unwrap().get_color(), Some(0));
    assert!(world.overlays.contains(&Overlay::ToggleColor { color: Some(0) }));
    assert!(world.changed > before, "marca para salvar");
    // O vizinho tem uma cor que se contradiz agora? Ainda não: a nula liga com tudo.
    assert_eq!(owner.network_of(at(0)), owner.network_of(at(1)));
    // O clique agachado num braço (o lado leste do primeiro) cicla o tipo de conexão, não a cor.
    assert!(owner.sneak_right_click(&mut world, at(0), Face::Up, Some(Face::East)));
    assert_eq!(owner.transporter_at(at(0)).unwrap().get_color(), None);
    assert_eq!(
        owner.transporter_at(at(0)).unwrap().transmitter.get_connection_type_raw(Face::East),
        ConnectionType::Push
    );
    // O clique sem agachar só mostra a cor.
    assert!(owner.right_click(&mut world, at(1), Face::Up, None));
    assert_eq!(world.overlays.last(), Some(&Overlay::CurrentColor { color: Some(0) }));
}

/// O desviador: o clique troca o modo do lado (`DiversionControl.getNext`) e avisa a capability se o modo muda o que o lado expõe; o restritivo não tem clique.
#[test]
fn the_configurator_cycles_the_mode_of_a_diversion_transporter() {
    let diversion = TransporterKind::Diversion { modes: [DiversionControl::Disabled; 6] };
    let (mut owner, mut world) = line(&[logistical(), diversion, logistical()]);
    assert!(owner.right_click(&mut world, at(1), Face::Up, None));
    assert_eq!(
        world.overlays.last(),
        Some(&Overlay::ToggleDiverter { mode: DiversionControl::High })
    );
    // Sem sinal, `HIGH` não está cumprido: o lado de cima (`NORMAL`, aceitava) deixa de expor a capability.
    assert_eq!(world.capabilities, vec![CapabilityChange::Hidden(Face::Up)]);
    // O mesmo clique de novo: `HIGH` -> `LOW`.
    assert!(owner.right_click(&mut world, at(1), Face::Up, None));
    assert_eq!(
        world.overlays.last(),
        Some(&Overlay::ToggleDiverter { mode: DiversionControl::Low })
    );
    // `LOW` com o sinal desligado está cumprido: a capability volta (`invalidateCapabilities`).
    assert_eq!(world.capabilities.last(), Some(&CapabilityChange::Exposed));
    let (mut owner, mut world) = line(&[TransporterKind::Restrictive, logistical()]);
    let before = world.overlays.len();
    assert!(owner.right_click(&mut world, at(0), Face::Up, None));
    assert_eq!(world.overlays.len(), before, "o restritivo não manda aviso");
}

/// A capability de item (`TransporterItemHandler`): existe onde `exposesInsertCap` vale, e só põe algo com rede.
#[test]
fn the_item_capability_exists_where_the_side_accepts() {
    let (mut owner, mut world) = line(&[logistical(), logistical()]);
    let dirt = ItemResource::new("minecraft:dirt", 64);
    // De cima: `NORMAL` aceita.
    let inserted = owner.insert(&mut world, at(0), Face::Up, &dirt, 5);
    assert_eq!(inserted, Some(1), "o `basic` leva uma por vez (`pullAmount` 1)");
    // Sem a capability: o lado `NONE`.
    let pos = at(5);
    let mut raw = normal();
    raw[Face::Up as usize] = ConnectionType::None;
    world.infos.borrow_mut().insert(
        pos,
        TransporterInfo { tier: TransporterTier::Basic, kind: logistical(), saved: Some(saved(pos, raw, &logistical())) },
    );
    owner.handle(&mut world, Event::ClearRemoved(pos));
    owner.handle(&mut world, Event::PlacedByItem(pos));
    assert_eq!(owner.insert(&mut world, pos, Face::Up, &dirt, 1), None);
    // Sem rede (o primeiro tick ainda não passou), nada entra.
    assert_eq!(owner.insert(&mut world, pos, Face::Down, &dirt, 1), Some(0));
}
