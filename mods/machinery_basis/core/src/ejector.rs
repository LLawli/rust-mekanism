//! Porte da parte de itens de `TileComponentEjector`: `tickServer` e `outputItems`. A transação de cada envio cobre a
//! máquina e o inventário do vizinho, então os dois lados voltam juntos se algo falhar.
use crate::containers::InventorySlot;
use crate::item_handler::{ItemCapabilities, ItemHandler, SlotHandler};
use crate::sides::Face;
use crate::transaction::Transaction;
use crate::transit::{HandlerTransitRequest, eject_item_map};
use std::rc::Rc;

/// `MekanismUtils.TICKS_PER_HALF_SECOND`.
pub const TICKS_PER_HALF_SECOND: u32 = 10;

/// Os slots de um `DataType` que sai (`InventorySlotInfo`), e as faces que ele tem.
pub struct EjectGroup {
    slots: Vec<InventorySlot>,
    /// Na ordem do `EnumSet<Direction>` do Java: a ordem de `Face::ALL`.
    sides: Vec<Face>,
    /// O que o bloco mostra por estas faces: no Java, `getHandler(side)`. Os slots de um tipo são os mesmos em todo
    /// lado, e é nisso que `EjectTransitRequest` se apoia.
    handler: Rc<dyn ItemHandler>,
}

impl EjectGroup {
    pub fn new(slots: Vec<InventorySlot>, sides: Vec<Face>) -> Self {
        let handler: Rc<dyn ItemHandler> = Rc::new(SlotHandler::new(slots.clone()));
        let mut sides = sides;
        sides.sort_by_key(|face| *face as usize);
        sides.dedup();
        Self {
            slots,
            sides,
            handler,
        }
    }

    /// `InventorySlotInfo.isEmpty`: nenhum slot tem item.
    fn is_empty(&self) -> bool {
        self.slots.iter().all(InventorySlot::is_empty)
    }
}

/// `java.util.Random.nextInt(bound)` não se repete entre rodadas; aqui o sorteio vem de um xorshift fixo, que só decide a
/// ordem em que os slots são vistos.
struct Xorshift(u64);

impl Xorshift {
    fn below(&mut self, bound: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % bound as u64) as usize
    }
}

pub struct ItemEjector {
    ejecting: bool,
    groups: Vec<EjectGroup>,
    tick_delay: u32,
    rng: Xorshift,
}

impl ItemEjector {
    pub fn new(groups: Vec<EjectGroup>) -> Self {
        Self {
            ejecting: false,
            groups,
            tick_delay: 0,
            rng: Xorshift(0x9E37_79B9_7F4A_7C15),
        }
    }

    /// `ConfigInfo.setEjecting`.
    pub fn set_ejecting(&mut self, ejecting: bool) {
        self.ejecting = ejecting;
    }

    pub fn is_ejecting(&self) -> bool {
        self.ejecting
    }

    /// Troca as faces de um grupo (`ConfigInfo.setDataType` de um lado).
    pub fn set_sides(&mut self, group: usize, sides: Vec<Face>) {
        let slots = self.groups[group].slots.clone();
        self.groups[group] = EjectGroup::new(slots, sides);
    }

    /// `tickServer`, só da parte de itens.
    pub fn tick_server(&mut self, neighbours: &dyn ItemCapabilities) {
        if !self.ejecting {
            return;
        }
        if self.tick_delay == 0 {
            self.output_items(neighbours);
        } else {
            self.tick_delay -= 1;
        }
    }

    /// `outputItems`, com `transaction == null`: cada envio abre a sua raiz.
    fn output_items(&mut self, neighbours: &dyn ItemCapabilities) {
        for group in &self.groups {
            if group.is_empty() || group.sides.is_empty() {
                continue;
            }
            let mut eject_map: Option<HandlerTransitRequest> = None;
            for &side in &group.sides {
                let Some(capability) = neighbours.item_handler(side) else {
                    // Sem alvo neste lado.
                    continue;
                };
                let handler = group.handler.clone();
                if let Some(request) = &eject_map {
                    // O handler do lado de onde o item sai, para a resposta usar as regras dele.
                    request.set_handler(Some(handler));
                } else {
                    let request = eject_item_map(
                        HandlerTransitRequest::new(Some(handler)),
                        &group.slots,
                        &mut |bound| self.rng.below(bound),
                        None,
                    );
                    if request.is_empty() {
                        break;
                    }
                    eject_map = Some(request);
                }
                let Some(request) = &eject_map else {
                    break;
                };
                let sub = Transaction::open(None);
                let response = request.eject(Some(&capability), 1, &sub);
                if response.use_all(&sub) {
                    sub.commit();
                    if request.is_empty() {
                        break;
                    }
                }
            }
        }
        self.tick_delay = TICKS_PER_HALF_SECOND;
    }
}
