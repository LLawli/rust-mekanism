// Entra em `crates/horizon-client/src/menu.rs` (crate `horizon_client_lib`) como `machinery_bar_menu`, para enxergar `Slot`, `Role` e os campos
// privados de `Menu`. O menu de um tipo de contêiner de mod que declarou uma barra de armadura (`ContainerBuilder.armorSideBar`) ganha, depois
// dos 36 do inventário, os slots que o `MekanismContainer.addArmorSlots` adiciona: quatro `ArmorSlot` (a cabeça primeiro) sobre os índices 39 a
// 36 do inventário de quem clica, e o `OffhandSlot` sobre o 40, que é como o menu do inventário do próprio jogo os tem. As regras de colocar,
// tirar, trocar, arrastar e equipar são as do motor verificado do Horizon (`Role::Armor`), e quem lê o menu os vê no índice `slots + 36 + k`.
//
// Este crate não enxerga a `machinery-basis-core` (o Cargo a liga só ao `horizon-client`, que depende deste), então o cliente do jogo diz aqui,
// a cada declaração, o que o menu ganha: `declare(menu, slots)`, com a armadura (`Some` do `EquipmentSlot.getIndex`, 0 nos pés a 3 na cabeça)
// e a mão (`None`), cada uma com o índice do inventário que espelha.
use super::{Kind, Menu, PLAYER, Role, Slot};
use std::sync::Mutex;

/// Um slot da barra: o `EquipmentSlot.getIndex` do `ArmorSlot` (`None` na mão) e o índice do inventário do jogador que ele espelha.
pub type BarSlot = (Option<usize>, usize);

static BARS: Mutex<Vec<(String, Vec<BarSlot>)>> = Mutex::new(Vec::new());

fn bars() -> std::sync::MutexGuard<'static, Vec<(String, Vec<BarSlot>)>> {
    BARS.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Declara (ou troca) os slots que o menu do tipo `menu` ganha depois dos 36 do inventário, na ordem do menu.
pub fn declare(menu: &str, slots: Vec<BarSlot>) {
    let mut bars = bars();
    match bars.iter_mut().find(|(name, _)| name == menu) {
        Some((_, existing)) => *existing = slots,
        None => bars.push((menu.to_owned(), slots)),
    }
}

/// Se o tipo de menu tem barra declarada.
pub fn declared(menu: &str) -> bool {
    bars().iter().any(|(name, _)| name == menu)
}

fn slots_of(menu: &str) -> Vec<BarSlot> {
    bars()
        .iter()
        .find(|(name, _)| name == menu)
        .map(|(_, slots)| slots.clone())
        .unwrap_or_default()
}

/// O menu de um tipo com barra declarada ganha os slots no fim, uma vez: um menu que já os tem não muda. Qualquer outro `Kind` fica como está.
pub fn extend(menu: &mut Menu) {
    let Kind::Mod(menu_type) = menu.kind else {
        return;
    };
    if menu.slots.len() != menu_type.slots + 36 {
        return;
    }
    for (equipment, index) in slots_of(menu_type.id) {
        let role = equipment.map_or(Role::Plain, Role::Armor);
        menu.slots.push(Slot {
            container: PLAYER,
            index,
            role,
        });
    }
}
