// Módulo de `creative.rs` (`horizon-client-lib`): os tanques criativos cheios das abas (`MekanismCreativeTabs.addFilledTanks`, `:51-69`) seguem as opções
// `prefilled.fluidTanks` e `prefilled.chemicalTanks` de `general.toml`. O Horizon monta as abas uma vez, do `creative-inventory.json` do plano de conteúdo, antes de
// qualquer mundo; o original as monta quando a tela criativa abre com parâmetros novos (`CreativeModeTabs.tryRebuildTabContents`: registros de outro mundo ou a
// permissão de operador) e lê as opções nessa hora. Aqui a lista inteira fica no `exported` do Horizon, e este módulo tira dela, depois de cada montagem, os
// tanques cheios que a opção desliga. O módulo é de `creative.rs` porque `rebuild`, `permissions`, `search_items` e `search_index` são privados de `Creative`; a regra (o que fica e
// quando montar de novo) é código puro de `creative_tabs_table.rs`, testado em `tests/creative_tabs.rs`.
use super::Creative;
use crate::items::ItemStack;
use crate::mod_content::mekanism_tabs::{prefilled_kept, prefilled_needs_update};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// `MekanismDataComponents.FLUIDS` e `CHEMICALS`: o tanque cheio é o que traz o componente.
const FLUIDS: &str = "mekanism:fluids";
const CHEMICALS: &str = "mekanism:chemicals";

/// As duas opções, como o arquivo as tem agora (`CachedBooleanValue.get`).
static FLUID: AtomicBool = AtomicBool::new(true);
static CHEMICAL: AtomicBool = AtomicBool::new(true);
/// Um mundo abriu desde a última montagem: os `holders` do original são outros, e a tela monta as abas de novo ao abrir.
static WORLD_OPENED: AtomicBool = AtomicBool::new(false);
/// A permissão (`hasPermissions`) da última montagem que este módulo filtrou; `None` antes da primeira.
static BUILT_WITH: Mutex<Option<bool>> = Mutex::new(None);

/// O que o arquivo diz agora.
pub fn configure(fluid: bool, chemical: bool) {
    FLUID.store(fluid, Ordering::Relaxed);
    CHEMICAL.store(chemical, Ordering::Relaxed);
}

/// Um mundo abriu (`ModConfigEvent.Loading` do arquivo de servidor).
pub fn world_opened() {
    WORLD_OPENED.store(true, Ordering::Relaxed);
}

fn keeps(stack: &ItemStack, fluid: bool, chemical: bool) -> bool {
    let has = |component: &str| stack.data().is_some_and(|data| data.other(component).is_some());
    prefilled_kept(&stack.item, has(FLUIDS), has(CHEMICALS), fluid, chemical)
}

/// Depois de `Game::refresh_creative_tabs` (o construtor da tela e o `containerTick`): se o mundo é outro, ou se a permissão mudou (e o `set_permissions` do
/// Horizon acabou de montar as abas de novo), as abas são montadas da lista inteira e perdem os tanques cheios que as opções de agora desligam.
pub fn refresh(creative: &mut Creative) {
    let world_opened = WORLD_OPENED.swap(false, Ordering::Relaxed);
    let mut built_with = BUILT_WITH.lock().unwrap_or_else(|error| error.into_inner());
    if !prefilled_needs_update(world_opened, *built_with, creative.permissions) {
        return;
    }
    *built_with = Some(creative.permissions);
    drop(built_with);
    creative.rebuild();
    let (fluid, chemical) = (FLUID.load(Ordering::Relaxed), CHEMICAL.load(Ordering::Relaxed));
    // A aba dos atalhos salvos e a do inventário não vêm de `displayItems`.
    for tab in creative.tabs.iter_mut().filter(|tab| !tab.hotbar && !tab.inventory) {
        tab.items.retain(|stack| keeps(stack, fluid, chemical));
    }
    let mask: Vec<bool> = creative.search_items.iter().map(|stack| keeps(stack, fluid, chemical)).collect();
    let mut at = mask.iter();
    creative.search_items.retain(|_| *at.next().unwrap_or(&true));
    let mut at = mask.iter();
    creative.search_index.retain(|_| *at.next().unwrap_or(&true));
    *creative.lines_cache.lock().unwrap_or_else(|error| error.into_inner()) = None;
    creative.forget_items();
}
