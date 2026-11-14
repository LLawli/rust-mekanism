//! O movimento das pilhas nos transportadores logísticos contra o original, tick a tick: `dev/referencia/rastros/transporter_move_<cenário>.jsonl`
//! (`testes/TransporterMoveTraceTest.java`). Cada cenário monta transportadores e contêineres (baús e droppers) e o original anda com eles; o teste monta
//! o mesmo mundo simulado e o faz andar pelo dono de produção (`InventoryTransmitters`, o que o jogo usa): os eventos do jogo (`ClearRemoved` e `PlacedByItem` de cada
//! transportador, `Update` na ordem do original, `Neighbor` do bloco de redstone e do bloco que sai, `Removed` ao quebrar, `ServerTickPost`) entram no dono, que faz o
//! que o `TileEntityLogisticalTransporterBase` e o registro fazem. As ações (`mark`) entram na hora em que o original as aplicou. Depois de cada tick confere cada
//! transportador (as pilhas em trânsito na ordem da tabela do fastutil, com o id, o `System.identityHashCode`, o progresso, o caminho, o próximo e o anterior, o NBT
//! salvo, o de atualização e o `STREAM_CODEC` em hexadecimal), os contêineres e o que caiu no mundo.
//!
//! Os `System.identityHashCode` das pilhas vêm do rastro (decisão 118): o teste os dá na ordem em que as pilhas aparecem.
use machinery_basis_core::containers::ItemResource;
use machinery_basis_core::identity_hash::{feed_identity_hashes, pending_fed_identity_hashes};
use machinery_basis_core::item_handler::{ItemHandler, VanillaContainer};
use machinery_basis_core::neighbors::UPDATE_ORDER;
use machinery_basis_core::sides::Face;
use machinery_basis_core::sim::chunks::BlockPos;
use machinery_basis_core::transmission::client::{ClientKind, ClientStack, ClientTransmitters};
use machinery_basis_core::transmission::connection::{
    ConnectionType, TransmissionSet, TransmissionType,
};
use machinery_basis_core::transmission::inventory_owner::{
    InventoryOwnerWorld, InventoryTransmitters, ItemAcceptor, SavedTransporter, TransporterInfo,
    TransporterPacket,
};
use machinery_basis_core::transmission::owner::{CableView, Event};
use machinery_basis_core::transmission::transmitter::{Compound as ClientCompound, Transmitter};
use machinery_basis_core::transmission::{DimensionId, Uuid};
use machinery_basis_core::transporter::movement::WorldDrop;
use machinery_basis_core::transporter::nbt::{Compound, Nbt, to_snbt};
use machinery_basis_core::transporter::node::{DiversionControl, TransporterKind, TransporterTier};
use machinery_basis_core::transporter::pos::NO_POS;
use machinery_basis_core::transporter::stack::{TransporterStack, read_var_long, write_var_int};
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

const DIMENSION: DimensionId = DimensionId(0);

fn read(name: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../dev/referencia/rastros")
        .join(format!("transporter_move_{name}.jsonl"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn face(name: &str) -> Face {
    Face::from_property(name).unwrap_or_else(|| panic!("lado {name}"))
}

fn color_index(name: &str) -> u8 {
    machinery_basis_core::side_config::ENUM_COLOR_NAMES
        .iter()
        .position(|candidate| *candidate == name)
        .unwrap_or_else(|| panic!("cor {name}")) as u8
}

fn color_of(value: &Value) -> Option<u8> {
    value.as_str().map(color_index)
}

fn item_of(value: &Value) -> ItemResource {
    ItemResource::new(value.as_str().unwrap(), 64)
}

/// O `Level` do cenário: contêineres (os baús e droppers do `setup`), o sinal de redstone, e o que o dono pede ao jogo ao ligar um transportador. Sem configuração de lados
/// nem ejetor avançado.
struct SimWorld {
    chests: HashMap<BlockPos, Rc<VanillaContainer>>,
    /// `WorldUtils.isGettingPowered`: o bloco de redstone do cenário, o mesmo para todos.
    power: Rc<Cell<bool>>,
    /// Os transportadores que o jogo vai entregar ao dono (`transporter_info`).
    infos: RefCell<HashMap<BlockPos, TransporterInfo>>,
    /// O que caiu no mundo desde a última conferência.
    drops: Vec<WorldDrop>,
    /// Os pacotes de movimento (`PacketTransporterSync` e `PacketTransporterBatch`) desde a última conferência.
    packets: Vec<(BlockPos, TransporterPacket)>,
}

impl InventoryOwnerWorld for SimWorld {
    fn is_block_loaded(&self, _: BlockPos) -> bool {
        true
    }

    fn is_getting_powered(&self, _: BlockPos) -> bool {
        self.power.get()
    }

    fn item_handler(&self, pos: BlockPos, _side: Face) -> Option<ItemAcceptor> {
        self.chests
            .get(&pos)
            .map(|chest| chest.clone() as Rc<dyn ItemHandler>)
    }

    fn transporter_info(&mut self, pos: BlockPos) -> Option<TransporterInfo> {
        self.infos.borrow_mut().remove(&pos)
    }

    fn decode_item(&self, tag: &Compound) -> Option<ItemResource> {
        tag.get("id")
            .and_then(|id| id.as_str())
            .map(|id| ItemResource::new(id, 64))
    }

    fn publish(&mut self, _: BlockPos, _: &CableView) {}

    fn world_drop(&mut self, drop: WorldDrop) {
        self.drops.push(drop);
    }

    fn transporter_packet(&mut self, pos: BlockPos, packet: TransporterPacket) {
        self.packets.push((pos, packet));
    }
}

/// `[x, y, z]` do rastro.
fn int3(value: &Value) -> [i32; 3] {
    let array = value.as_array().unwrap();
    [
        array[0].as_i64().unwrap() as i32,
        array[1].as_i64().unwrap() as i32,
        array[2].as_i64().unwrap() as i32,
    ]
}

struct Scenario {
    name: &'static str,
    origin: BlockPos,
    owner: InventoryTransmitters,
    world: SimWorld,
    /// A ordem em que o original anda os transportadores num tick (a dos block entities, `y`, `x`, `z`).
    order: Vec<usize>,
    /// A posição relativa de cada transportador do `setup`, na ordem dele.
    relative: Vec<[i32; 3]>,
    expected: HashMap<usize, Value>,
    statics: HashMap<(usize, i32, i64), Value>,
    expected_chests: Vec<Value>,
    tick: u32,
    /// Os pacotes de movimento que o original mandou desde o último `state`, e o item de cada id do registro que o rastro mostrou.
    packet_rows: Vec<Value>,
    items: HashMap<i64, String>,
    /// Depois da troca de nível os block entities são outros objetos, e o original só anota os pacotes dos que o cenário montou: dali em diante os pacotes não se comparam.
    packets_blind: bool,
}

impl Scenario {
    fn abs(&self, rel: [i32; 3]) -> BlockPos {
        BlockPos::new(
            self.origin.x + rel[0],
            self.origin.y + rel[1],
            self.origin.z + rel[2],
        )
    }

    fn abs_value(&self, rel: &Value) -> BlockPos {
        self.abs(int3(rel))
    }

    fn rel_block(&self, pos: BlockPos) -> Value {
        serde_json::json!([
            pos.x - self.origin.x,
            pos.y - self.origin.y,
            pos.z - self.origin.z
        ])
    }

    fn build(name: &'static str, setup: &Value) -> Self {
        let origin = BlockPos::new(
            setup["x"].as_i64().unwrap() as i32,
            setup["y"].as_i64().unwrap() as i32,
            setup["z"].as_i64().unwrap() as i32,
        );
        let power = Rc::new(Cell::new(false));
        let mut world = SimWorld {
            chests: HashMap::new(),
            power,
            infos: RefCell::new(HashMap::new()),
            drops: Vec::new(),
            packets: Vec::new(),
        };
        for container in setup["containers"].as_array().unwrap() {
            let rel = int3(&container["p"]);
            let pos = BlockPos::new(origin.x + rel[0], origin.y + rel[1], origin.z + rel[2]);
            world.chests.insert(
                pos,
                VanillaContainer::new(container["size"].as_u64().unwrap() as usize, Rc::new(|| {})),
            );
        }
        let mut relative = Vec::new();
        let mut positions = Vec::new();
        for transporter in setup["transporters"].as_array().unwrap() {
            let rel = int3(&transporter["p"]);
            relative.push(rel);
            let pos = BlockPos::new(origin.x + rel[0], origin.y + rel[1], origin.z + rel[2]);
            positions.push(pos);
            let tier = TransporterTier::by_id(transporter["tier"].as_i64().unwrap() as i32);
            let kind = match transporter["kind"].as_str().unwrap() {
                "logistical" => TransporterKind::Logistical {
                    color: color_of(&transporter["color"]),
                },
                "restrictive" => TransporterKind::Restrictive,
                "diversion" => TransporterKind::Diversion {
                    modes: std::array::from_fn(|side| {
                        DiversionControl::by_id(transporter["modes"][side].as_i64().unwrap() as i32)
                    }),
                },
                other => panic!("tipo {other}"),
            };
            // O que o block entity trazia: os tipos de conexão de cada lado (`connection`).
            let raw: [ConnectionType; 6] = std::array::from_fn(|side| {
                ConnectionType::by_id(transporter["raw"][side].as_i64().unwrap() as i32)
            });
            let mut template: Transmitter<ItemAcceptor> = Transmitter::new(
                [pos.x, pos.y, pos.z],
                TransmissionSet::of(&[TransmissionType::Item]),
                false,
            );
            template.set_connection_types_raw(raw);
            // O `write` do block entity: as ligações, a cor ou o `mode` do desviador.
            let saved = SavedTransporter {
                tag: template.write(),
                color: match &kind {
                    TransporterKind::Logistical { color: Some(color) } => Some(Nbt::String(
                        machinery_basis_core::side_config::ENUM_COLOR_NAMES[usize::from(*color)]
                            .into(),
                    )),
                    _ => None,
                },
                modes: match &kind {
                    TransporterKind::Diversion { modes } => Some(Nbt::IntArray(
                        modes.iter().map(|mode| mode.ordinal()).collect(),
                    )),
                    _ => None,
                },
                items: None,
            };
            world.infos.borrow_mut().insert(
                pos,
                TransporterInfo {
                    tier,
                    kind,
                    saved: Some(saved),
                },
            );
        }
        let mut owner = InventoryTransmitters::new(
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
        );
        // O jogo entrega os block entities ao chunk, e cada um é posto por um item (`onAdded`): entra na rede e relê as ligações. O primeiro tick forma as redes.
        for pos in &positions {
            owner.handle(&mut world, Event::ClearRemoved(*pos));
        }
        for pos in &positions {
            owner.handle(&mut world, Event::PlacedByItem(*pos));
        }
        owner.handle(
            &mut world,
            Event::ServerTickPost {
                runs_normally: true,
            },
        );
        let mut order: Vec<usize> = (0..relative.len()).collect();
        order.sort_by_key(|index| {
            (
                relative[*index][1],
                relative[*index][0],
                relative[*index][2],
            )
        });
        Scenario {
            name,
            origin,
            owner,
            world,
            order,
            relative,
            expected: HashMap::new(),
            statics: HashMap::new(),
            expected_chests: Vec::new(),
            tick: 0,
            packet_rows: Vec::new(),
            items: HashMap::new(),
            packets_blind: false,
        }
    }

    fn pos_of(&self, index: usize) -> BlockPos {
        self.abs(self.relative[index])
    }

    fn pos_long(&self, index: usize) -> i64 {
        self.pos_of(index).as_long()
    }

    /// Um tick do servidor: os block entities na ordem, e o `onTick` do registro no `ServerTickEvent.Post`.
    fn run_tick(&mut self) {
        self.tick += 1;
        for index in self.order.clone() {
            let pos = self.pos_of(index);
            self.owner.handle(&mut self.world, Event::Update(pos));
        }
        self.owner.handle(
            &mut self.world,
            Event::ServerTickPost {
                runs_normally: true,
            },
        );
    }

    fn container(&self, rel: &Value) -> Rc<VanillaContainer> {
        let pos = self.abs_value(rel);
        self.world.chests[&pos].clone()
    }

    fn apply_mark(&mut self, row: &Value) {
        let who = format!("{} t{} {}", self.name, row["t"], row["k"]);
        match row["k"].as_str().unwrap() {
            "fill" => self.container(&row["p"]).set_item(
                row["slot"].as_u64().unwrap() as usize,
                Some(item_of(&row["item"])),
                row["n"].as_u64().unwrap() as u32,
            ),
            "cap_insert" => {
                let pos = self.abs_value(&row["p"]);
                let side = face(row["side"].as_str().unwrap());
                let item = item_of(&row["item"]);
                let amount = row["n"].as_u64().unwrap() as u32;
                let inserted = self
                    .owner
                    .insert(&mut self.world, pos, side, &item, amount)
                    .map_or(-1_i64, i64::from);
                assert_eq!(
                    inserted,
                    row["inserted"].as_i64().unwrap(),
                    "{who} inserido"
                );
            }
            "power" => {
                self.world.power.set(row["on"].as_bool().unwrap());
                let at = self.abs_value(&row["p"]);
                self.neighbors_changed(at);
            }
            "break" => self.break_transporter(&row["p"]),
            "alloy" => self.alloy(&row["p"]),
            "finishing" | "finish" => {}
            other => panic!("ação {other}"),
        }
    }

    /// `Level.updateNeighborsAt(at)`: os vizinhos recebem `neighborChanged` na ordem de `UPDATE_ORDER`.
    fn neighbors_changed(&mut self, at: BlockPos) {
        for side in UPDATE_ORDER {
            let neighbor = at.relative(side);
            self.owner
                .handle(&mut self.world, Event::Neighbor(neighbor));
        }
    }

    /// `ItemAlloy.useOn` com a liga infundida num transportador básico (`TileEntityTransmitter.onAlloyInteraction`): cada transportador da rede, do mais perto ao mais longe, é trocado por um
    /// avançado (`setBlockAndUpdate`: o velho sai, o novo entra, os vizinhos recebem o aviso) e recebe o dado do velho; no fim a rede se invalida. Os block entities novos entram no fim da fila
    /// de quem anda no tick (o original registra o ticker novo depois dos que já estavam), na ordem em que subiram.
    fn alloy(&mut self, p: &Value) {
        let click = self.abs_value(p);
        self.packets_blind = true;
        let mut run = self
            .owner
            .alloy_start(click, 1, &|_| Some(0))
            .expect("o transportador tem rede");
        let mut upgraded = Vec::new();
        for target in run.targets.clone() {
            let data = self
                .owner
                .alloy_take(&mut self.world, &mut run, target)
                .expect("o dado do velho");
            self.owner.handle(&mut self.world, Event::Removed(target));
            self.world.infos.borrow_mut().insert(
                target,
                TransporterInfo {
                    tier: TransporterTier::Advanced,
                    kind: TransporterKind::Logistical { color: None },
                    saved: None,
                },
            );
            self.owner
                .handle(&mut self.world, Event::ClearRemoved(target));
            self.neighbors_changed(target);
            assert!(self.owner.alloy_give(&mut run, target, data));
            upgraded.push(
                (0..self.relative.len())
                    .find(|index| self.pos_of(*index) == target)
                    .expect("o transportador é do cenário"),
            );
            if run.full() {
                break;
            }
        }
        self.owner.alloy_finish(&mut self.world, &run);
        self.order.retain(|index| !upgraded.contains(index));
        self.order.extend(upgraded);
    }

    /// Os pacotes de movimento do que passou desde o último `state`: os mesmos, na mesma ordem, com o mesmo conteúdo que o `STREAM_CODEC` deu no original.
    fn check_packets(&mut self) {
        let who = format!("{} t{}", self.name, self.tick);
        let want = std::mem::take(&mut self.packet_rows);
        let got = std::mem::take(&mut self.world.packets);
        if self.packets_blind {
            return;
        }
        assert_eq!(
            got.iter()
                .map(|(pos, packet)| (
                    self.rel_block(*pos),
                    matches!(packet, TransporterPacket::Sync { .. })
                ))
                .collect::<Vec<_>>(),
            want.iter()
                .map(|row| (row["p"].clone(), row["k"] == "sync"))
                .collect::<Vec<_>>(),
            "{who}: os pacotes, na ordem"
        );
        for ((pos, packet), row) in got.iter().zip(&want) {
            match packet {
                TransporterPacket::Sync { id, stack } => {
                    assert_eq!(i64::from(*id), row["id"].as_i64().unwrap(), "{who} sync id");
                    self.same_stack(&who, *pos, *id, stack, &row["iid"], &row["stream"]);
                }
                TransporterPacket::Batch { deletes, updates } => {
                    assert_eq!(
                        deletes.iter().map(|id| i64::from(*id)).collect::<Vec<_>>(),
                        row["deletes"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|id| id.as_i64().unwrap())
                            .collect::<Vec<_>>(),
                        "{who} saídas do lote"
                    );
                    let want_updates = row["updates"].as_array().unwrap();
                    assert_eq!(
                        updates.len(),
                        want_updates.len(),
                        "{who} atualizações do lote"
                    );
                    for ((id, stack), want) in updates.iter().zip(want_updates) {
                        assert_eq!(
                            i64::from(*id),
                            want[0].as_i64().unwrap(),
                            "{who} id do lote"
                        );
                        self.same_stack(&who, *pos, *id, stack, &want[1], &want[2]);
                    }
                }
            }
        }
    }

    /// A pilha do pacote de `pos` é a que o `STREAM_CODEC` do original escreveu (`hex`, com o item de `iid`): o cliente a lê do compound e a lê do fio com o mesmo resultado.
    fn same_stack(
        &self,
        who: &str,
        pos: BlockPos,
        id: i32,
        stack: &ClientCompound,
        iid: &Value,
        hex: &Value,
    ) {
        let mut client = ClientTransmitters::new();
        client.add_tile(pos, ClientKind::Items, 0);
        client.on_transporter_sync(pos, id, stack);
        let got: ClientStack = client.extra_at(pos).unwrap().transit_in_order()[0].clone();
        let hex = hex.as_str().unwrap();
        let bytes: Vec<u8> = (0..hex.len() / 2)
            .map(|at| u8::from_str_radix(&hex[at * 2..at * 2 + 2], 16).unwrap())
            .collect();
        // O rastro só dá o item de um id do registro quando a pilha aparece num `state`; senão vale o do próprio pacote (o resto do fluxo é conferido do mesmo jeito).
        let item = self
            .items
            .get(&iid.as_i64().unwrap())
            .cloned()
            .unwrap_or_else(|| got.item.clone().expect("pilha sem item").0);
        let mut rest = &bytes[..];
        let original = TransporterStack::read_stream(&mut rest, &|input| {
            read_var_long(input)?;
            // Sem patch de componentes: dois zeros.
            *input = &input[2..];
            Some(ItemResource::new(&item, 64))
        })
        .expect("o fluxo do original");
        assert!(rest.is_empty(), "{who}: sobrou fluxo");
        let to_option = |position: i64| (position != NO_POS).then_some(position);
        assert_eq!(got.index, id, "{who}");
        assert_eq!(got.color, original.color, "{who} {id} cor");
        assert_eq!(got.progress, original.progress, "{who} {id} progresso");
        assert_eq!(
            got.original_location, original.original_location,
            "{who} {id} origem"
        );
        assert_eq!(
            got.path_type.as_deref(),
            Some(original.path_type().serialized_name()),
            "{who} {id} tipo de caminho"
        );
        assert_eq!(
            got.next,
            to_option(original.client_next()),
            "{who} {id} próximo"
        );
        assert_eq!(
            got.previous,
            to_option(original.client_prev()),
            "{who} {id} anterior"
        );
        assert_eq!(
            got.item,
            original
                .item_type()
                .map(|item| (item.id.to_string(), i64::from(original.size()))),
            "{who} {id} item"
        );
    }

    /// `setBlock(AIR)` num transportador: o bloco quebrado solta o que leva (`preRemoveSideEffects`) e sai (`setRemoved`); os vizinhos recebem o aviso.
    fn break_transporter(&mut self, p: &Value) {
        let at = self.abs_value(p);
        self.owner.handle(&mut self.world, Event::Removed(at));
        self.neighbors_changed(at);
    }

    fn check_row(&mut self, row: &Value) {
        let who = format!("{} t{}", self.name, row["t"]);
        for entry in row["tx"].as_array().unwrap() {
            let index = self
                .relative
                .iter()
                .position(|rel| *rel == int3(&entry["p"]))
                .unwrap_or_else(|| panic!("{who}: transportador {}", entry["p"]));
            let mut entry = entry.clone();
            entry["fresh"] = Value::Bool(true);
            self.expected.insert(index, entry);
        }
        if let Some(chests) = row.get("chests") {
            self.expected_chests = chests.as_array().unwrap().clone();
        }
        let delays = row["dl"].as_array().unwrap();
        for (index, delay) in delays.iter().enumerate().take(self.relative.len()) {
            let expected = self.expected.get(&index).cloned();
            let delay = delay.as_i64().unwrap();
            let pos = self.pos_long(index);
            let bpos = self.pos_of(index);
            let me = format!("{who} {:?}", self.relative[index]);
            let mut checks: Vec<String> = Vec::new();
            let statics = &mut self.statics;
            let origin = self.origin;
            self.owner.with_transporter(&self.world, bpos, |base, _| {
                let (actual_delay, delay_count, next_id) = base.counters();
                if delay >= 0 {
                    assert_eq!(i64::from(actual_delay), delay, "{me} delay");
                }
                let Some(expected) = &expected else {
                    return;
                };
                if expected.get("gone").is_some() {
                    return;
                }
                let fresh = expected.get("fresh").is_some();
                assert_eq!(
                    i64::from(delay_count),
                    expected["dc"].as_i64().unwrap(),
                    "{me} delayCount"
                );
                assert_eq!(
                    i64::from(next_id),
                    expected["nid"].as_i64().unwrap(),
                    "{me} nextId"
                );
                let rel = |packed: i64| -> Value {
                    if packed == machinery_basis_core::transporter::pos::NO_POS {
                        return Value::Null;
                    }
                    let p = machinery_basis_core::transporter::pos::block_pos_of(packed);
                    serde_json::json!([p.x - origin.x, p.y - origin.y, p.z - origin.z])
                };
                let stacks = base.transit_stacks();
                let expected_stacks = expected["transit"].as_array().unwrap();
                assert_eq!(
                    stacks.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
                    expected_stacks
                        .iter()
                        .map(|stack| stack["id"].as_i64().unwrap() as i32)
                        .collect::<Vec<_>>(),
                    "{me} ids em trânsito, na ordem da tabela"
                );
                for ((id, stack), want) in stacks.iter().zip(expected_stacks) {
                    let state = stack.borrow();
                    let hash = state.identity_hash();
                    assert_eq!(
                        i64::from(hash),
                        want["h"].as_i64().unwrap(),
                        "{me} #{id} hash"
                    );
                    assert_eq!(
                        i64::from(state.progress),
                        want["prog"].as_i64().unwrap(),
                        "{me} #{id} progresso"
                    );
                    let key = (index, *id, i64::from(hash));
                    let full = if want.get("s").is_some() {
                        statics
                            .get(&key)
                            .unwrap_or_else(|| panic!("{me} #{id}: sem a pilha inteira antes"))
                            .clone()
                    } else {
                        statics.insert(key, want.clone());
                        want.clone()
                    };
                    let tag = format!("{me} #{id}");
                    assert_eq!(
                        state.item_type().map(|item| item.id.to_string()),
                        full["item"].as_str().map(str::to_owned),
                        "{tag} item"
                    );
                    assert_eq!(
                        i64::from(state.size()),
                        full["n"].as_i64().unwrap(),
                        "{tag} quantidade"
                    );
                    assert_eq!(
                        state.color.map(
                            |c| machinery_basis_core::side_config::ENUM_COLOR_NAMES[c as usize]
                        ),
                        full["color"].as_str(),
                        "{tag} cor"
                    );
                    assert_eq!(
                        state.initiated_path,
                        full["init"].as_bool().unwrap(),
                        "{tag} initiatedPath"
                    );
                    assert_eq!(
                        state.idle_dir.map(Face::name),
                        full["idle"].as_str(),
                        "{tag} idleDir"
                    );
                    assert_eq!(rel(state.original_location), full["orig"], "{tag} original");
                    assert_eq!(rel(state.home_location), full["home"], "{tag} casa");
                    assert_eq!(
                        state.path_type().serialized_name(),
                        full["pt"].as_str().unwrap(),
                        "{tag} pathType"
                    );
                    assert_eq!(
                        serde_json::Value::Array(state.path().iter().map(|p| rel(*p)).collect()),
                        full["path"],
                        "{tag} caminho"
                    );
                    assert_eq!(rel(state.get_next(pos)), full["next"], "{tag} próximo");
                    assert_eq!(rel(state.get_prev(pos)), full["prev"], "{tag} anterior");
                    assert_eq!(
                        state.get_side(pos).name(),
                        full["side"].as_str().unwrap(),
                        "{tag} lado"
                    );
                    drop(state);
                    if fresh && let Some(stream) = want.get("stream") {
                        let iid = want["iid"].as_i64().unwrap() as i32;
                        stack.borrow_mut().update_for_pos(pos);
                        let bytes = stack.borrow().write_stream(&|_, out| {
                            write_var_int(out, iid);
                            out.extend([0, 0]);
                        });
                        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
                        assert_eq!(hex, stream.as_str().unwrap(), "{tag} STREAM_CODEC");
                    }
                }
                let sync: Vec<i64> = base.pending_sync().into_iter().map(i64::from).collect();
                let want_sync: Vec<i64> = expected["sync"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_i64().unwrap())
                    .collect();
                assert_eq!(sync, want_sync, "{me} needsSync");
                if fresh {
                    for (key, text) in [
                        ("nbt", base.write_items()),
                        ("upd", base.write_reduced_updated_items()),
                    ] {
                        if let Some(want) = expected.get(key) {
                            checks.push(format!("{me} {key}"));
                            assert_eq!(
                                text.as_ref().map(to_snbt),
                                want.as_str().map(str::to_owned),
                                "{me} {key}"
                            );
                        }
                    }
                }
            });
        }
        // Contêineres.
        for expected in &self.expected_chests {
            let container = self.container(&expected["p"]);
            let actual: Vec<Value> = (0..container.size())
                .filter_map(|slot| {
                    container.resource(slot).map(|item| {
                        serde_json::json!([slot, item.id.to_string(), container.amount(slot)])
                    })
                })
                .collect();
            assert_eq!(
                Value::Array(actual),
                expected["slots"],
                "{who} contêiner {}",
                expected["p"]
            );
        }
        // O que caiu no mundo neste tick.
        let mut actual: Vec<String> = std::mem::take(&mut self.world.drops)
            .into_iter()
            .map(|drop| {
                format!(
                    "{} {} {}",
                    drop.item.id,
                    drop.amount,
                    self.rel_block(drop.pos)
                )
            })
            .collect();
        let mut expected: Vec<String> = row
            .get("drops")
            .map(|drops| {
                drops
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|d| format!("{} {} {}", d["item"].as_str().unwrap(), d["n"], d["p"]))
                    .collect()
            })
            .unwrap_or_default();
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected, "{who} itens soltos");
    }
}

/// Os `System.identityHashCode` das pilhas na ordem em que aparecem pela primeira vez no rastro.
fn hashes_in_order(rows: &[Value], order: &[usize], relative: &[[i32; 3]]) -> Vec<i32> {
    let mut seen = HashSet::new();
    let mut hashes = Vec::new();
    for row in rows {
        let Some(entries) = row.get("tx").and_then(Value::as_array) else {
            continue;
        };
        let mut entries: Vec<&Value> = entries.iter().collect();
        entries.sort_by_key(|entry| {
            let rel = int3(&entry["p"]);
            let index = relative.iter().position(|r| *r == rel).unwrap();
            order.iter().position(|o| *o == index).unwrap()
        });
        for entry in entries {
            let Some(transit) = entry.get("transit").and_then(Value::as_array) else {
                continue;
            };
            let mut stacks: Vec<&Value> = transit.iter().collect();
            stacks.sort_by_key(|stack| stack["id"].as_i64().unwrap());
            for stack in stacks {
                let hash = stack["h"].as_i64().unwrap() as i32;
                if seen.insert(hash) {
                    hashes.push(hash);
                }
            }
        }
    }
    hashes
}

fn run(name: &'static str) {
    let rows = read(name);
    assert_eq!(rows[0]["e"], "setup");
    let mut scenario = Scenario::build(name, &rows[0]);
    feed_identity_hashes(hashes_in_order(&rows, &scenario.order, &scenario.relative));
    assert_eq!(rows[1]["e"], "begin");
    for row in &rows {
        for entry in row["tx"].as_array().into_iter().flatten() {
            for stack in entry["transit"].as_array().into_iter().flatten() {
                if let (Some(iid), Some(item)) = (stack["iid"].as_i64(), stack["item"].as_str()) {
                    scenario.items.insert(iid, item.to_owned());
                }
            }
        }
    }
    // O estado de antes do primeiro tick: os contadores de cada transportador (a estrutura já andou os ticks de preparação).
    for entry in rows[1]["tx"].as_array().unwrap() {
        let index = scenario
            .relative
            .iter()
            .position(|rel| *rel == int3(&entry["p"]))
            .unwrap();
        let delay = rows[1]["dl"][index].as_i64().unwrap() as i32;
        let (dc, nid) = (
            entry["dc"].as_i64().unwrap() as i32,
            entry["nid"].as_i64().unwrap() as i32,
        );
        scenario
            .owner
            .with_transporter(&scenario.world, scenario.pos_of(index), |base, _| {
                base.restore_counters(delay, dc, nid)
            });
        assert!(
            entry["transit"].as_array().unwrap().is_empty(),
            "{name}: começa com pilhas"
        );
    }
    if let Some(chests) = rows[1].get("chests") {
        scenario.expected_chests = chests.as_array().unwrap().clone();
    }
    let mut ticks = 0;
    for row in &rows[2..] {
        match row["e"].as_str().unwrap() {
            "mark" => scenario.apply_mark(row),
            // O `ServerTickEvent.Post` do próprio tick em que a gravação começou (`t` 0): o estado de `begin`, sem tick no meio.
            "state" if row["t"] == 0 => {}
            "state" => {
                scenario.run_tick();
                scenario.check_row(row);
                scenario.check_packets();
                ticks += 1;
            }
            "packet" => scenario.packet_rows.push(row.clone()),
            other => panic!("evento {other}"),
        }
    }
    assert!(ticks > 50, "{name}: poucos ticks ({ticks})");
    assert_eq!(
        pending_fed_identity_hashes(),
        0,
        "{name}: sobraram hashes dados que nenhuma pilha pediu"
    );
}

macro_rules! scenarios {
    ($($test:ident => $name:literal),* $(,)?) => {
        $(
            #[test]
            fn $test() {
                run($name);
            }
        )*
    };
}

scenarios! {
    pull_line => "pull_line",
    tiers => "tiers",
    full_midway => "full_midway",
    full_midway_idle => "full_midway_idle",
    capability => "capability",
    restrictive => "restrictive",
    lone => "lone",
    lone_idle => "lone_idle",
    broken => "broken",
    diversion => "diversion",
    contested_1 => "contested_1",
    contested_2 => "contested_2",
    contested_3 => "contested_3",
    contested_4 => "contested_4",
    alloy_a => "alloy_a",
    alloy_b => "alloy_b",
    alloy_c => "alloy_c",
    alloy_d => "alloy_d",
}
