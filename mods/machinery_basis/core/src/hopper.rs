//! Máquinas sem interface vanilla, que fazem por conta própria o que um hopper faria com elas.
//!
//! A máquina declara `inputs` e `outputs` vazios, então o hopper não a enxerga. Quem age é ela:
//! entrega ao hopper logo abaixo, porque um hopper só puxa do bloco acima, e recebe de um hopper
//! que aponta para ela. As faces configuradas valem nos dois casos, e o ritmo é o do hopper.
use crate::containers::{ItemResource, SlotContents, room_for};
use crate::sides::{Face, Sides};

/// Ticks entre duas transferências de um hopper (`HopperBlockEntity.MOVE_ITEM_SPEED`).
pub const COOLDOWN_TICKS: u8 = 8;

/// A espera do ejetor depois de um disparo (`MekanismUtils.TICKS_PER_HALF_SECOND`).
pub const EJECT_DELAY_TICKS: u8 = 10;

/// O que importa de um hopper vizinho.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hopper {
    /// Para onde ele entrega.
    pub facing: Face,
    /// Falso quando a redstone o trava.
    pub enabled: bool,
}

/// O que a máquina faz com um hopper vizinho.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transfer {
    /// A máquina entrega um item ao hopper.
    Give,
    /// A máquina recebe um item do hopper.
    Take,
}

/// O que fazer com um hopper na face `face` da máquina, se algo.
pub fn transfer(face: Face, hopper: Hopper, sides: &Sides) -> Option<Transfer> {
    if !hopper.enabled {
        return None;
    }
    if face == Face::Down && sides.gives_output(face) {
        return Some(Transfer::Give);
    }
    if hopper.facing == face.opposite() && sides.accepts_input(face) {
        return Some(Transfer::Take);
    }
    None
}

/// Um item do vizinho que a máquina pode puxar: o número do slot dele, o recurso, e o que mais quem chama quiser levar junto
/// (a pilha do jogo, por exemplo).
pub struct Offer<T> {
    pub slot: usize,
    pub resource: ItemResource,
    pub payload: T,
}

/// Como o hopper escolhe o que levar de um vizinho para os slots de entrada de uma máquina: o primeiro item do vizinho, na ordem dos
/// slots dele, que algum slot de entrada aceita (o slot decide `accepts`), e dentre os slots o primeiro, na ordem de `inputs`, que
/// o aceita e tem lugar para ele: vazio, ou com o mesmo recurso (o item e os componentes) e abaixo da capacidade. Devolve o índice
/// do item em `offers` e o do slot em `inputs`.
pub fn first_taker<T>(
    offers: &[Offer<T>],
    inputs: &[(usize, SlotContents)],
    accepts: impl Fn(usize, &Offer<T>) -> bool,
) -> Option<(usize, usize)> {
    offers.iter().enumerate().find_map(|(index, offer)| {
        let slot = inputs.iter().find(|(slot, held)| {
            accepts(*slot, offer)
                && room_for(held, &offer.resource, offer.resource.slot_capacity()) > 0
        })?;
        Some((index, slot.0))
    })
}

/// O intervalo de um elo entre máquina e hopper. Como no hopper, só um sucesso espera.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cooldown {
    pub ticks: u8,
}

impl Cooldown {
    /// Conta um tick e diz se o elo pode tentar agora.
    pub fn tick(&mut self) -> bool {
        self.ticks = self.ticks.saturating_sub(1);
        self.ticks == 0
    }

    pub fn moved(&mut self) {
        self.ticks = COOLDOWN_TICKS;
    }
}

/// O ritmo do push, como `TileComponentEjector`: dispara com a espera em zero e a refaz, tenha
/// saído algo ou não. A contagem até zero consome os ticks, então os disparos ficam a 11 ticks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ejector {
    pub delay: u8,
}

impl Ejector {
    /// Conta um tick e diz se o ejetor dispara agora.
    pub fn tick(&mut self) -> bool {
        if self.delay == 0 {
            self.delay = EJECT_DELAY_TICKS;
            return true;
        }
        self.delay -= 1;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open() -> Sides {
        Sides {
            input: [true; 6],
            output: [true; 6],
        }
    }

    fn offer(slot: usize, id: &str, amount: u32) -> Offer<u32> {
        Offer {
            slot,
            resource: ItemResource::new(id, 64),
            payload: amount,
        }
    }

    #[test]
    fn the_hopper_takes_the_first_item_a_slot_accepts_and_has_room_for() {
        let held = |id: &str, amount| SlotContents::new(Some(ItemResource::new(id, 64)), amount);
        let offers = [
            offer(0, "stick", 2),
            offer(1, "clay", 2),
            offer(2, "dirt", 2),
        ];
        let accepts = |_: usize, offer: &Offer<u32>| &*offer.resource.id != "stick";
        // O primeiro item que o slot aceita, na ordem do vizinho.
        let empty = [(7, SlotContents::default())];
        assert_eq!(first_taker(&offers, &empty, accepts), Some((1, 7)));
        // Um slot cheio com outro item não serve, e o seguinte da fila é que recebe.
        let inputs = [(7, held("dirt", 3)), (8, SlotContents::default())];
        assert_eq!(first_taker(&offers, &inputs, accepts), Some((1, 8)));
        // Um slot cheio do mesmo item não tem lugar: cai para o item seguinte.
        let full = [(7, held("clay", 64))];
        assert_eq!(first_taker(&offers, &full, accepts), None);
        let full_clay = [(7, held("clay", 64)), (8, held("dirt", 5))];
        assert_eq!(first_taker(&offers, &full_clay, accepts), Some((2, 8)));
        // Com dois slots que servem, o primeiro da fila.
        let two = [(7, SlotContents::default()), (8, SlotContents::default())];
        assert_eq!(first_taker(&offers, &two, accepts), Some((1, 7)));
        // Um item só vai a um slot com o mesmo item, e o que o slot não aceita fica.
        assert_eq!(first_taker(&offers[..1], &empty, accepts), None);
    }

    fn hopper(facing: Face) -> Hopper {
        Hopper {
            facing,
            enabled: true,
        }
    }

    #[test]
    fn gives_to_a_hopper_below_whichever_way_it_points() {
        for facing in [Face::Down, Face::North, Face::East] {
            assert_eq!(
                transfer(Face::Down, hopper(facing), &open()),
                Some(Transfer::Give)
            );
        }
    }

    #[test]
    fn never_gives_through_another_face() {
        for face in [Face::Up, Face::North, Face::South, Face::West, Face::East] {
            assert_ne!(
                transfer(face, hopper(Face::Down), &open()),
                Some(Transfer::Give)
            );
        }
    }

    #[test]
    fn takes_from_a_hopper_pointing_at_it() {
        assert_eq!(
            transfer(Face::Up, hopper(Face::Down), &open()),
            Some(Transfer::Take)
        );
        assert_eq!(
            transfer(Face::North, hopper(Face::South), &open()),
            Some(Transfer::Take)
        );
    }

    #[test]
    fn ignores_a_hopper_pointing_elsewhere() {
        assert_eq!(transfer(Face::North, hopper(Face::North), &open()), None);
        assert_eq!(transfer(Face::North, hopper(Face::Down), &open()), None);
        assert_eq!(transfer(Face::North, hopper(Face::East), &open()), None);
    }

    #[test]
    fn a_locked_hopper_does_nothing() {
        let locked = Hopper {
            facing: Face::Down,
            enabled: false,
        };
        assert_eq!(transfer(Face::Down, locked, &open()), None);
        assert_eq!(transfer(Face::Up, locked, &open()), None);
    }

    #[test]
    fn the_configured_faces_win() {
        let mut sides = open();
        sides.output[Face::Down as usize] = false;
        sides.input[Face::North as usize] = false;
        assert_eq!(transfer(Face::Down, hopper(Face::Down), &sides), None);
        assert_eq!(transfer(Face::North, hopper(Face::South), &sides), None);
    }

    #[test]
    fn moves_once_every_eight_ticks() {
        let mut cooldown = Cooldown::default();
        let mut moved = 0;
        for _ in 0..(COOLDOWN_TICKS as u32 * 10) {
            if cooldown.tick() {
                cooldown.moved();
                moved += 1;
            }
        }
        assert_eq!(moved, 10);
    }

    /// Os ticks em que `fires` diz sim, de `0` a `ticks`.
    fn firing_ticks(ticks: u32, mut fires: impl FnMut() -> bool) -> Vec<u32> {
        (0..ticks).filter(|_| fires()).collect()
    }

    #[test]
    fn hopper_transfers_are_exactly_eight_ticks_apart() {
        let mut cooldown = Cooldown::default();
        let fired = firing_ticks(81, || {
            let ready = cooldown.tick();
            if ready {
                cooldown.moved();
            }
            ready
        });
        assert_eq!(fired, (0..=80).step_by(8).collect::<Vec<u32>>());
    }

    #[test]
    fn ejector_fires_at_once_then_every_eleven_ticks() {
        let mut ejector = Ejector::default();
        let fired = firing_ticks(111, || ejector.tick());
        assert_eq!(fired, (0..=110).step_by(11).collect::<Vec<u32>>());
    }

    #[test]
    fn ejector_rhythm_matches_the_java_loop() {
        // Reprodução direta de TileComponentEjector.tickServer, para comparar tick a tick.
        let mut tick_delay = 0;
        let mut java = || {
            if tick_delay == 0 {
                tick_delay = 10;
                true
            } else {
                tick_delay -= 1;
                false
            }
        };
        let mut ejector = Ejector::default();
        for tick in 0..1_000 {
            assert_eq!(ejector.tick(), java(), "tick {tick}");
        }
    }

    #[test]
    fn a_failed_try_does_not_wait() {
        let mut cooldown = Cooldown::default();
        assert!(cooldown.tick());
        assert!(cooldown.tick());
    }
}
