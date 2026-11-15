//! O valor vivo dos dados que a estrutura do tile sincroniza, lido de uma `ProgressMachine` (a base das quatro máquinas portadas):
//! o [`TrackedSource`] das fontes de [`crate::tile_trackers::source`]. O que o jogo guarda fora do objeto (a energia e o tanque de
//! químico, que chegam de fora do tick da máquina) entra como argumento.
use crate::container_sync::wire::write_var_int;
use crate::container_sync::{StackValue, TrackedSource, Value};
use crate::tile::ProgressMachine;
use crate::tile_trackers::source;
use crate::upgrade_window::UpgradeSync;

/// O que o servidor tem de uma máquina de progresso no momento de ler os dados.
pub struct MachineSync<'a> {
    pub machine: &'a ProgressMachine,
    /// A energia que o tile guarda agora (`getAmountAsLong`).
    pub stored: i64,
    /// O tanque de químico (`SyncableLargeResourceStack.create(chemicalTank)`), nas máquinas que têm um.
    pub tank: Option<StackValue>,
}

impl<'a> MachineSync<'a> {
    /// A energia e o tanque como o objeto da máquina os tem (a simulação). O jogo os lê do block entity.
    pub fn of(machine: &'a ProgressMachine) -> Self {
        let tank = machine.chemical_side().map(|side| StackValue {
            resource: side
                .tank
                .resource()
                .map_or_else(String::new, |resource| resource.id.to_string()),
            amount: side.tank.amount(),
        });
        Self {
            machine,
            stored: machine.energy().amount(),
            tank,
        }
    }
}

impl TrackedSource for MachineSync<'_> {
    fn value(&self, id: u16) -> Value {
        let machine = self.machine;
        let tile = machine.tile();
        let redstone = tile.redstone_sync();
        match id {
            // O port ainda não tem o componente de segurança: sem dono, a frequência é `null` (`hashCode` zero, um byte `false`) e o
            // modo é `PUBLIC`.
            source::SECURITY_FREQUENCY => Value::Codec {
                hash: 0,
                bytes: vec![0],
            },
            source::SECURITY_MODE => Value::Enum(0),
            source::CONTROL_TYPE => Value::Enum(redstone.control_type.ordinal()),
            source::POWERED => Value::Boolean(redstone.powered),
            source::WAS_POWERED => Value::Boolean(redstone.was_powered),
            source::LAST_ENERGY_RECEIVED => Value::Long(machine.last_energy_received()),
            // `getCapacityAsLong`: a capacidade nunca fica abaixo da energia (veja `ScreenData::of_machine`).
            source::MAX_ENERGY => Value::Long(machine.energy().capacity().max(self.stored)),
            source::ENERGY_PER_TICK => Value::Int(machine.energy().energy_per_tick()),
            source::ENERGY => Value::Long(self.stored),
            source::HAS_XP_CONTAINMENT => Value::Boolean(machine.has_xp_containment()),
            source::OPERATING_TICKS => Value::Int(machine.recipe_progress()),
            source::TICKS_REQUIRED => Value::Int(machine.ticks_required()),
            source::UPGRADE_TICKS => Value::Int(machine.upgrades().upgrade_ticks()),
            source::UPGRADE_MAP => upgrade_map(&UpgradeSync::of(machine.upgrades())),
            source::CHEMICAL_TANK => Value::Stack(
                self.tank
                    .clone()
                    .expect("a máquina não tem tanque de químico"),
            ),
            error if (source::TRACKED_ERROR..source::MOD).contains(&error) => {
                let bit = u32::from(error - source::TRACKED_ERROR);
                Value::Boolean(machine.tracked_errors_mask().bits() >> bit & 1 != 0)
            }
            other => panic!("fonte {other} desconhecida"),
        }
    }
}

/// `SyncableStreamCodec.upgradeMap` sobre as contagens por tipo (a ordem do registro): o `hashCode` do mapa (zero se vazio, como o de
/// um `Object2IntMap` vazio) e os bytes do `ByteBufCodecs.map(Upgrade.STREAM_CODEC, VAR_INT)`: o tamanho, e o id do registro e a
/// contagem de cada tipo instalado. O Java escreve os tipos na ordem do mapa do tile (que muda de uma execução para a outra); aqui
/// vão na ordem da tabela do tile (`UpgradeSync::tile_order`).
pub fn upgrade_map(sync: &UpgradeSync) -> Value {
    let installed: Vec<(usize, i32)> = sync
        .written_order()
        .into_iter()
        .map(|index| (index, sync.count(index)))
        .collect();
    let mut hash: i32 = 0;
    let mut bytes = Vec::new();
    write_var_int(&mut bytes, installed.len() as i32);
    for (index, count) in &installed {
        hash = hash.wrapping_add((*index as i32 + 1).wrapping_mul(0x9E37_79B1_u32 as i32) ^ count);
        write_var_int(&mut bytes, *index as i32);
        write_var_int(&mut bytes, *count);
    }
    Value::Codec { hash, bytes }
}

/// O inverso de [`upgrade_map`], como o cliente lê o `SyncableStreamCodec` do mapa: o tamanho, e o índice do registro e a contagem de cada
/// tipo instalado. Bytes que não fecham dão o mapa vazio, como um valor que o cliente não conseguiu decodificar.
pub fn read_upgrade_map(ticks: i32, bytes: &[u8]) -> UpgradeSync {
    fn next(bytes: &mut &[u8]) -> Option<i32> {
        let mut value = 0u32;
        for shift in 0..5 {
            let (&byte, rest) = bytes.split_first()?;
            *bytes = rest;
            value |= u32::from(byte & 0x7F) << (7 * shift);
            if byte & 0x80 == 0 {
                return Some(value as i32);
            }
        }
        None
    }
    let mut counts: Vec<i32> = Vec::new();
    let mut tile_order: Vec<usize> = Vec::new();
    let mut rest = bytes;
    let mut read = || -> Option<()> {
        let size = next(&mut rest)?;
        for _ in 0..size {
            let index = usize::try_from(next(&mut rest)?).ok()?;
            let count = next(&mut rest)?;
            if counts.len() <= index {
                counts.resize(index + 1, 0);
            }
            counts[index] = count;
            tile_order.push(index);
        }
        rest.is_empty().then_some(())
    };
    if read().is_none() {
        counts.clear();
        tile_order.clear();
    }
    UpgradeSync {
        ticks,
        counts,
        tile_order,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn var_ints_follow_the_minecraft_encoding() {
        let encode = |value: i32| {
            let mut bytes = Vec::new();
            write_var_int(&mut bytes, value);
            bytes
        };
        assert_eq!(encode(0), [0]);
        assert_eq!(encode(5), [5]);
        assert_eq!(encode(127), [127]);
        assert_eq!(encode(128), [0x80, 1]);
        assert_eq!(encode(300), [0xAC, 2]);
    }

    #[test]
    fn the_client_reads_back_the_map_the_server_wrote() {
        let sync = UpgradeSync {
            ticks: 7,
            counts: vec![0, 3, 0, 0, 0, 2, 0, 0],
            tile_order: vec![5, 1],
        };
        let Value::Codec { bytes, .. } = upgrade_map(&sync) else {
            unreachable!()
        };
        let read = read_upgrade_map(7, &bytes);
        assert_eq!(read.ticks, 7);
        assert_eq!(read.count(1), 3);
        assert_eq!(read.count(5), 2);
        assert_eq!(read.count(4), 0);
        // Os tipos vão na ordem da tabela do tile, e o cliente os lê nela.
        assert_eq!(read.written_order(), [5, 1]);
        assert_eq!(read_upgrade_map(0, &[0]), UpgradeSync::default());
        // Bytes cortados: o cliente não lê nada.
        assert_eq!(
            read_upgrade_map(4, &[1, 5]).written_order(),
            Vec::<usize>::new()
        );
    }

    #[test]
    fn the_upgrade_map_is_empty_with_hash_zero_and_changes_with_any_count() {
        let empty = UpgradeSync {
            ticks: 0,
            counts: vec![0; 8],
            tile_order: Vec::new(),
        };
        assert_eq!(
            upgrade_map(&empty),
            Value::Codec {
                hash: 0,
                bytes: vec![0]
            }
        );
        let mut speed_two = empty.clone();
        speed_two.counts[5] = 2;
        // O mapa com a velocidade em 2: um tipo, o id 5 do registro e a contagem.
        let Value::Codec { hash, bytes } = upgrade_map(&speed_two) else {
            unreachable!()
        };
        assert_eq!(bytes, [1, 5, 2]);
        assert_ne!(hash, 0);
        speed_two.counts[5] = 3;
        let Value::Codec { hash: other, .. } = upgrade_map(&speed_two) else {
            unreachable!()
        };
        assert_ne!(other, hash);
    }
}
