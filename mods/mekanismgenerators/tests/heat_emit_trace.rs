//! Repete, tick a tick, os rastros `heat_emit_*.jsonl` do Heat Generator original (`dev/referencia/testes/HeatEmitTraceTest.java`): `TileEntityGenerator.onUpdateServer` faz
//! `EnergyUtils.emit(outputCaches, energyContainer, null)` com um cache só, o da frente (`ENERGY_SIDES = FRONT`), e só com `canFunction()`, e o `TileEntityHeatGenerator.onUpdateServer`
//! o chama antes de queimar e produzir (`super.onUpdateServer`): sai a energia que o buffer guardava no fim do tick anterior. Os vizinhos são máquinas com buffer (a Enrichment Chamber do
//! rastro, com todos os lados em `INPUT`) e uma fila de cabos universais no dono dos transmissores (`EnergyTransmitters`), movido pelos eventos do jogo como o jogo os entrega.
//!
//! Cada linha do rastro é um tick, gravada depois do tick do tile (e do `onTick` do registro) e das marcas do cenário: o teste roda o tick e só depois aplica as marcas.
use machinery_basis_core::containers::AutomationType;
use machinery_basis_core::containers::InventorySlot;
use machinery_basis_core::containers::ItemResource;
use machinery_basis_core::distribution::EnergyInsert;
use machinery_basis_core::energy_cube::EnergyNeighbours;
use machinery_basis_core::energy_cube::ItemEnergy;
use machinery_basis_core::energy_cube::ItemEnergySpec;
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::BlockPos;
use machinery_basis_core::tile::RedstoneControl;
use machinery_basis_core::transaction::Transaction;
use machinery_basis_core::transmission::energy_container::{BufferEnergyContainer, always};
use machinery_basis_core::transmission::owner::{
    Acceptor, CableInfo, CableView, EnergyTransmitters, Event, OwnerWorld,
};
use machinery_basis_core::transmission::{DimensionId, Uuid};
use mekanismgenerators_rules::{
    ENERGY_CAPACITY, State, Surroundings, drain_into_slot, emit_energy, step, step_emitting,
};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::rc::Rc;

const GENERATOR: BlockPos = BlockPos::new(2, 0, 2);
/// O gerador do rastro olha para o leste.
const FRONT: Face = Face::East;
const CABLE_CAPACITY: i64 = 8_000;

/// A Enrichment Chamber do rastro: um buffer que aceita energia de fora por qualquer lado.
struct Machine {
    container: BufferEnergyContainer,
}

impl EnergyInsert for Machine {
    fn insert(&self, amount: i32, transaction: &Transaction) -> i32 {
        self.container
            .insert(amount, transaction, AutomationType::External)
    }
}

#[derive(Default)]
struct Level {
    machines: HashMap<BlockPos, Rc<Machine>>,
    cables: HashSet<BlockPos>,
}

impl OwnerWorld for Level {
    fn is_block_loaded(&self, _: BlockPos) -> bool {
        true
    }

    fn is_getting_powered(&self, _: BlockPos) -> bool {
        false
    }

    fn energy_handler(&self, pos: BlockPos, _: Face) -> Option<Acceptor> {
        let machine: Acceptor = self.machines.get(&pos)?.clone();
        Some(machine)
    }

    fn cable_info(&mut self, pos: BlockPos) -> Option<CableInfo> {
        self.cables.contains(&pos).then_some(CableInfo {
            capacity: CABLE_CAPACITY,
            saved: None,
        })
    }

    fn publish(&mut self, _: BlockPos, _: &CableView) {}
}

/// O mundo em volta do gerador: as máquinas, e os cabos com o dono deles.
struct World {
    level: RefCell<Level>,
    owner: RefCell<EnergyTransmitters>,
    time: RefCell<i64>,
}

impl World {
    fn new() -> Self {
        let mut uuid = 0i32;
        let mut tile = 0i32;
        Self {
            level: RefCell::new(Level::default()),
            owner: RefCell::new(EnergyTransmitters::new(
                DimensionId(0),
                move || {
                    uuid += 1;
                    Uuid::with_hash_code(uuid.wrapping_mul(0x1234_5679))
                },
                move |_| {
                    tile += 1;
                    tile.wrapping_mul(0x2545_F491)
                },
            )),
            time: RefCell::new(0),
        }
    }

    fn send(&self, event: Event) {
        self.owner
            .borrow_mut()
            .handle(&mut *self.level.borrow_mut(), event);
    }

    /// `setBlock` de um cabo: o `clearRemoved` e a capability e o aviso de cada cabo vizinho.
    fn place_cable(&self, pos: BlockPos) {
        self.level.borrow_mut().cables.insert(pos);
        self.send(Event::ClearRemoved(pos));
        for side in Face::ALL {
            let neighbor = pos.relative(side);
            if self.owner.borrow().cable_at(neighbor).is_some() {
                self.send(Event::Capability(neighbor, side.opposite()));
            }
        }
        for side in Face::ALL {
            let neighbor = pos.relative(side);
            if self.owner.borrow().cable_at(neighbor).is_some() {
                self.send(Event::Neighbor(neighbor));
            }
        }
    }

    fn place_machine(&self, pos: BlockPos, max: i64, stored: i64) {
        let container = BufferEnergyContainer::new(max, always, always, None);
        container.set_energy(stored, None);
        self.level
            .borrow_mut()
            .machines
            .insert(pos, Rc::new(Machine { container }));
    }

    /// O tick do servidor dos cabos: cada block entity anda e o registro fecha o tick.
    fn tick_cables(&self) {
        let positions = self.owner.borrow().positions();
        for pos in positions {
            self.send(Event::Update(pos));
        }
        let time = *self.time.borrow();
        self.owner.borrow_mut().set_game_time(time);
        self.send(Event::ServerTickPost {
            runs_normally: true,
        });
        *self.time.borrow_mut() += 1;
    }

    /// A energia da rede do cabo em `pos`, ou -1 sem rede (`getTransmitterNetwork() == null`).
    fn network_energy(&self, pos: BlockPos) -> i64 {
        let owner = self.owner.borrow();
        owner
            .network_of(pos)
            .and_then(|network| owner.family().network(network))
            .map_or(-1, |network| network.buffer())
    }
}

/// Os vizinhos do gerador, como o `BlockCapabilityCache` e o `EnergyHandler` de cada face.
struct Neighbours<'a> {
    world: &'a World,
}

impl Neighbours<'_> {
    fn at(&self, face: Face) -> BlockPos {
        GENERATOR.relative(face)
    }
}

impl EnergyNeighbours for Neighbours<'_> {
    fn has_handler(&self, face: Face) -> bool {
        let pos = self.at(face);
        if self.world.level.borrow().machines.contains_key(&pos) {
            return true;
        }
        // Um cabo tem capability em todo lado que não é `NONE` cru nem está ativado por redstone, com rede ou sem.
        let level = self.world.level.borrow();
        self.world
            .owner
            .borrow()
            .view_at(&*level, pos)
            .is_some_and(|view| view.exists[face.opposite() as usize])
    }

    fn insert(&self, face: Face, amount: i32, simulate: bool) -> i32 {
        let transaction = Transaction::open_root();
        let accepted = self.insert_in(face, amount, simulate, &transaction);
        if !simulate {
            transaction.commit();
        }
        accepted
    }

    /// O `insert` dentro da transação do `EnergyUtils.emit`: os vizinhos que têm diário entram nela.
    fn insert_in(&self, face: Face, amount: i32, simulate: bool, transaction: &Transaction) -> i32 {
        let pos = self.at(face);
        let machine = self.world.level.borrow().machines.get(&pos).cloned();
        if let Some(machine) = machine {
            if simulate {
                let probe = Transaction::open(Some(transaction));
                return EnergyInsert::insert(&*machine, amount, &probe);
            }
            return EnergyInsert::insert(&*machine, amount, transaction);
        }
        if simulate {
            let owner = self.world.owner.borrow();
            return owner.container_state(pos).map_or(0, |(capacity, now)| {
                i32::try_from((capacity - now).max(0))
                    .unwrap_or(i32::MAX)
                    .min(amount)
            });
        }
        self.world
            .owner
            .borrow_mut()
            .insert_external_in(pos, amount, transaction)
    }
}

fn control_of(name: &str) -> RedstoneControl {
    match name {
        "HIGH" => RedstoneControl::High,
        "DISABLED" => RedstoneControl::Disabled,
        other => panic!("controle desconhecido: {other}"),
    }
}

fn pos_of(x: &str, y: &str, z: &str) -> BlockPos {
    BlockPos::new(x.parse().unwrap(), y.parse().unwrap(), z.parse().unwrap())
}

fn apply(
    world: &World,
    state: &mut State,
    slot: &InventorySlot,
    names: &mut Vec<(String, BlockPos)>,
    mark: &str,
) {
    let words: Vec<&str> = mark.split(' ').collect();
    match words.as_slice() {
        ["lava", amount] => state.lava = amount.parse().unwrap(),
        ["energy", amount] => state.energy = amount.parse().unwrap(),
        ["control", kind] => state.tile.set_control_type(control_of(kind)),
        // O slot de energia (`EnergyInventorySlot.drain`): o item, a energia da pilha e a quantidade.
        ["slot", name, energy, count] => slot.set_contents(
            Some(slot_resource(name, energy.parse().unwrap())),
            count.parse().unwrap(),
            None,
        ),
        ["chamber", name, x, y, z, max, stored] => {
            let pos = pos_of(x, y, z);
            world.place_machine(pos, max.parse().unwrap(), stored.parse().unwrap());
            names.push(((*name).to_owned(), pos));
        }
        ["cable", x, z] => world.place_cable(pos_of(x, "0", z)),
        ["set", name, energy] => {
            let pos = names.iter().find(|(known, _)| known == name).unwrap().1;
            world.level.borrow().machines[&pos]
                .container
                .set_energy(energy.parse().unwrap(), None);
        }
        _ => panic!("marca desconhecida: {mark}"),
    }
}

/// Os itens do cenário do slot, com o namespace que o recurso tem no jogo.
fn slot_resource(name: &str, energy: i64) -> ItemResource {
    let id = if name == "stick" {
        name.to_owned()
    } else {
        format!("mekanism:{name}")
    };
    let stack = if name == "basic_energy_cube" { 64 } else { 1 };
    if energy == 0 {
        ItemResource::new(&id, stack)
    } else {
        ItemResource::with_component(
            &id,
            stack,
            mekanism_rules::ENERGY_COMPONENT,
            &energy.to_string(),
        )
    }
}

/// O handler de energia dos itens do cenário: o equipamento, o cubo e o item da Enrichment Chamber, como o Mekanism os dá, com a energia no componente do recurso.
fn slot_items() -> ItemEnergy {
    use machinery_basis_core::components::Components;
    let config = mekanism_rules::ServerConfig::DEFAULT;
    ItemEnergy {
        lookup: Rc::new(move |resource: &ItemResource| {
            let energy = resource
                .components
                .get(mekanism_rules::ENERGY_COMPONENT)
                .and_then(|text| text.trim_end_matches('L').parse::<i64>().ok())
                .unwrap_or(0);
            mekanism_rules::item_energy_spec(&resource.id, energy, &config).or_else(|| {
                (&*resource.id == "mekanism:enrichment_chamber").then(|| -> ItemEnergySpec {
                    mekanism_rules::machine_item_spec(
                        energy,
                        i64::from(mekanism_rules::energy_capacity(
                            mekanism_rules::ENRICHMENT_USAGE,
                        )),
                    )
                })
            })
        }),
        write: Rc::new(|resource: &ItemResource, energy: i64| {
            let text = energy.to_string();
            let mut entries: Vec<(&str, &str)> = resource
                .components
                .iter()
                .filter(|(id, _)| *id != mekanism_rules::ENERGY_COMPONENT)
                .collect();
            if energy != 0 {
                entries.push((mekanism_rules::ENERGY_COMPONENT, text.as_str()));
            }
            Some(ItemResource::with_components(
                &resource.id,
                resource.max_stack_size,
                Components::new(entries),
            ))
        }),
        conversions: machinery_basis_core::energy_conversion::EnergyConversion::cache_of(Vec::new()),
    }
}

/// O slot como o rastro o grava: o caminho do item, a quantidade e a energia da pilha.
fn slot_line(slot: &InventorySlot) -> Value {
    let Some(resource) = slot.resource() else {
        return serde_json::json!(["", 0, 0]);
    };
    let energy = resource
        .components
        .get(mekanism_rules::ENERGY_COMPONENT)
        .and_then(|text| text.parse::<i64>().ok())
        .unwrap_or(0);
    let path = resource.id.rsplit(':').next().unwrap();
    serde_json::json!([path, slot.amount(), energy])
}

fn replay(scenario: &str, cables: &[(i32, i32)], minimum_lines: usize) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(format!("heat_emit_{scenario}.jsonl"));
    let text = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(
        lines.len() >= minimum_lines,
        "rastro curto demais: {}",
        lines.len()
    );
    assert_eq!(
        lines[0]["epsilon"].as_f64().unwrap(),
        machinery_basis_core::heat::EPSILON
    );
    assert_eq!(
        lines[0]["capacity"].as_i64().unwrap(),
        i64::from(ENERGY_CAPACITY)
    );
    let world = World::new();
    let mut state = State::new(lines[0]["ambient"].as_f64().unwrap());
    state.heat.heat = lines[0]["heat_before"].as_f64().unwrap();
    let mut names: Vec<(String, BlockPos)> = Vec::new();
    let mut emitted = 0i64;
    // O cenário `slot` grava também o slot de energia: a ordem do tick é a entrega, o slot e a produção.
    let with_slot = lines[0].get("energy_slot").is_some();
    let slot = InventorySlot::new(None);
    let items = slot_items();
    let mut drained_to_item = 0i64;
    for expected in &lines {
        let t = expected["t"].as_u64().unwrap();
        let faces: Vec<f64> = expected["ambient_faces"]
            .as_array()
            .unwrap()
            .iter()
            .map(|face| face.as_f64().unwrap())
            .collect();
        let around = Surroundings {
            lava_faces: 0,
            in_nether: false,
            ambient: expected["ambient"].as_f64().unwrap(),
            face_ambient: faces.try_into().unwrap(),
        };
        let mut produced = 0;
        // O primeiro tick gravado já aconteceu quando o cenário começa: o estado dele é o semeado.
        if t > 0 {
            let before = state.energy;
            if with_slot {
                emit_energy(&mut state, &Neighbours { world: &world }, FRONT);
                let full = state.energy;
                drain_into_slot(&mut state, &items, &slot);
                drained_to_item += i64::from(full) - i64::from(state.energy);
                produced = step(&mut state, &around).energy;
            } else {
                produced =
                    step_emitting(&mut state, &around, &Neighbours { world: &world }, FRONT).energy;
            }
            emitted += i64::from(before) + i64::from(produced) - i64::from(state.energy);
            world.tick_cables();
        }
        for mark in expected["marks"].as_array().unwrap() {
            apply(
                &world,
                &mut state,
                &slot,
                &mut names,
                mark.as_str().unwrap(),
            );
        }
        let context = format!("{scenario} t {t}");
        if with_slot {
            assert_eq!(
                slot_line(&slot),
                expected["energy_slot"],
                "{context}: o slot de energia"
            );
        }
        assert_eq!(
            i64::from(state.energy),
            expected["energy"].as_i64().unwrap(),
            "{context}: energy"
        );
        assert_eq!(
            i64::from(produced),
            expected["produced"].as_i64().unwrap(),
            "{context}: produced"
        );
        assert_eq!(
            state.lava,
            expected["lava"].as_u64().unwrap(),
            "{context}: lava"
        );
        assert_eq!(
            state.tile.active(),
            expected["active"].as_bool().unwrap(),
            "{context}: active"
        );
        assert_eq!(
            state.tile.is_redstone_activated(),
            expected["can_function"].as_bool().unwrap(),
            "{context}: can_function"
        );
        assert_eq!(
            Neighbours { world: &world }.has_handler(FRONT),
            expected["front_handler"].as_bool().unwrap(),
            "{context}: a capability da frente"
        );
        let actors = expected["actors"].as_object().unwrap();
        for (name, pos) in &names {
            let machine = world.level.borrow().machines[pos].container.amount();
            assert_eq!(
                machine,
                actors[name].as_i64().unwrap(),
                "{context}: a máquina {name}"
            );
        }
        for (index, (x, z)) in cables.iter().enumerate() {
            if let Some(expected_net) = actors.get(&format!("net{index}")) {
                assert_eq!(
                    world.network_energy(BlockPos::new(*x, 0, *z)),
                    expected_net.as_i64().unwrap(),
                    "{context}: a rede do cabo {index}"
                );
            }
        }
    }
    if with_slot {
        assert!(
            drained_to_item > 1_000,
            "{scenario}: o slot nunca recebeu energia"
        );
    } else {
        assert!(emitted > 0, "{scenario}: o gerador nunca entregou nada");
    }
}

#[test]
fn only_the_front_machine_gets_the_energy_and_the_buffer_goes_out_before_the_production() {
    replay("sides", &[], 100);
}

#[test]
fn a_front_machine_with_room_for_part_of_the_energy_leaves_the_rest_in_the_buffer() {
    replay("partial", &[], 100);
}

#[test]
fn the_front_cable_takes_the_energy_into_its_network_and_the_network_passes_it_on() {
    replay("cable", &[(3, 2), (4, 2), (5, 2)], 100);
}

#[test]
fn a_cable_beside_the_generator_that_is_not_the_front_gets_nothing() {
    replay("cable_beside", &[(2, 3)], 80);
}

#[test]
fn the_generator_gives_energy_only_while_it_can_function() {
    replay("control", &[], 120);
}

/// O slot de energia (`EnergyInventorySlot.drain`): a cada tick o buffer entrega ao item o que a taxa de carga dele deixa, depois da entrega ao vizinho e antes da produção, e só um
/// item que aceita energia a recebe; o gerador sem controle de redstone (`HIGH` sem sinal) entrega ao item do mesmo jeito.
#[test]
fn the_energy_slot_drains_the_buffer_into_the_item_before_the_production() {
    replay("slot", &[], 150);
}
