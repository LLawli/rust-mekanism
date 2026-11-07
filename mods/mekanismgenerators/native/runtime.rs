// Regras do Heat Generator, sem tipos do jogo. Este arquivo é a lib de teste do mod e também o
// módulo crate::game::mekanismgenerators_rules. Segue TileEntityHeatGenerator.
use machinery_basis_core::gui::NineSlice;
use machinery_basis_core::heat::Capacitor;
use machinery_basis_core::slot_view::{ContainerSlotType, SlotOverlay, SlotSpec};
use machinery_basis_core::temperature::{self, TemperatureUnit};
use machinery_basis_core::tile::TileBase;
use machinery_basis_core::units::{energy_fraction, stored_mb};

pub const HEAT_GENERATOR: &str = "mekanismgenerators:heat_generator";
pub const MENU: &str = "mekanismgenerators:heat_generator";
pub const LAVA: &str = "minecraft:lava";

pub const HEAT_CAPACITY: f64 = 10.0;
pub const INVERSE_CONDUCTION: f64 = 5.0;
pub const INVERSE_INSULATION: f64 = 100.0;
/// A face de baixo usa a isolação padrão: é por ela que o calor sairia para um vizinho.
pub const INSULATION_BY_FACE: [f64; 6] = [0.0, 100.0, 100.0, 100.0, 100.0, 100.0];
const THERMAL_EFFICIENCY: f64 = 0.5;

/// Valores padrão de `GeneratorsConfig` e `GeneratorsStorageConfig`.
pub const HEAT_GENERATION: f64 = 100.0;
pub const HEAT_GENERATION_LAVA: f64 = 7.0;
pub const HEAT_GENERATION_NETHER: f64 = 10.0;
pub const FLUID_RATE: u64 = 100;
pub const TANK_CAPACITY: u64 = 1_000;
pub const ENERGY_CAPACITY: i32 = 240;
/// As seis faces mais a lava dentro do bloco, o Nether e a queima.
pub const MAX_PRODUCTION: i32 = 7 * 7 + 10 + 100;
pub const BUCKET_VOLUME: u64 = 1_000;
/// `blockDeactivationDelay`, o padrão de `GeneralConfig` do Mekanism: quantos ticks o bloco aceso espera para apagar depois que o
/// gerador para (`TileEntityMekanism.setActive`). O Heat Generator não troca o `delaySupplier`.
pub const BLOCK_DEACTIVATION_DELAY: i32 = 60;

/// Os slots de `TileEntityHeatGenerator.getInitialInventory`, na ordem dos índices: o do combustível (`FluidInventorySlot`, `EXTRA` no
/// contêiner) e o de energia (`EnergyInventorySlot`, com o raio por cima). A posição é a do item. O gerador não tem configuração de
/// lados de itens, então a moldura vem do tipo no contêiner.
pub const SLOTS_SPEC: [SlotSpec; 2] = [
    SlotSpec {
        item: [17.0, 35.0],
        container: ContainerSlotType::Extra,
        overlay: None,
    },
    SlotSpec {
        item: [143.0, 35.0],
        container: ContainerSlotType::Power,
        overlay: Some(SlotOverlay::Power),
    },
];

/// O `gui.scaling` dos sprites do Mekanism que a tela usa, como os `.png.mcmeta` do original dizem (um teste confere contra os
/// arquivos): o painel `base`, a moldura `gauge/normal` e o mosaico de `bar/vertical_power`. As molduras e o painel são os mesmos
/// que as telas das máquinas usam.
pub const PANEL_SLICE: NineSlice = NineSlice {
    size: [16.0, 16.0],
    border: [4.0; 4],
    stretch_inner: false,
};
pub const BAR_FRAME_SLICE: NineSlice = NineSlice {
    size: [5.0, 5.0],
    border: [1.0; 4],
    stretch_inner: false,
};
pub const GAUGE_FRAME_SLICE: NineSlice = NineSlice {
    size: [5.0, 5.0],
    border: [2.0; 4],
    stretch_inner: false,
};
pub const POWER_BAR_TILE: [f32; 2] = [2.0, 2.0];
/// As chaves de idioma do Mekanism que os tooltips usam (`MekanismLang.GENERIC_FRACTION`, `GENERIC_STORED_MB`, `EMPTY` e
/// `ENERGY_FORGE`) e o nome da lava (`FluidType.getDescription`).
pub const FRACTION_LANG: &str = "generic.mekanism.fraction";
pub const STORED_MB_LANG: &str = "generic.mekanism.stored.mb";
pub const EMPTY_LANG: &str = "gui.mekanism.empty";
pub const ENERGY_UNIT_LANG: &str = "energy.mekanism.forge.short";
pub const LAVA_NAME_LANG: &str = "block.minecraft.lava";

/// O tooltip de `GuiVerticalPowerBar`: a energia sobre a capacidade, em FE.
pub fn energy_tooltip(energy: u64, capacity: u64, translate: &dyn Fn(&str) -> String) -> String {
    energy_fraction(
        energy,
        capacity,
        &translate(ENERGY_UNIT_LANG),
        &translate(FRACTION_LANG),
    )
}

/// `GuiEnergyTab`: o elemento de textura em (-26, 137), 26 por 26, sem clique, e o sprite que o Mekanism guarda.
pub const ENERGY_TAB: machinery_basis_core::side_tab::InfoTab =
    machinery_basis_core::side_tab::InfoTab {
        rect: [-26.0, 137.0, 26.0, 26.0],
    };
pub const ENERGY_TAB_SPRITE: &str = "mekanism:gui/sprites/tab/energy_info";
/// `GeneratorsLang.PRODUCING_AMOUNT` e `MekanismLang.UNIT`.
pub const PRODUCING_LANG: &str = "gui.mekanismgenerators.producing.amount";
pub const UNIT_LANG: &str = "gui.mekanism.unit";

/// O tooltip de `GuiEnergyTab` do Heat Generator (`tile.getProductionRate()`): o que produz por tick e a unidade.
pub fn energy_tab_tooltip(production: u64, translate: &dyn Fn(&str) -> String) -> Vec<String> {
    machinery_basis_core::units::energy_info_lines(
        &[(&translate(PRODUCING_LANG), production)],
        &translate(ENERGY_UNIT_LANG),
        &translate(UNIT_LANG),
    )
}

/// `GuiHeatTab`: o elemento de textura em (-26, 109), 26 por 26. O sprite muda com a unidade de temperatura da configuração local.
pub const HEAT_TAB: machinery_basis_core::side_tab::InfoTab =
    machinery_basis_core::side_tab::InfoTab {
        rect: [-26.0, 109.0, 26.0, 26.0],
    };
/// `GeneratorsContainerTypes.java:29`: o Heat Generator registra `.armorSideBar(-20, 11, 0)` (o `x`, o `y` e o `offhandOffset` de
/// `ContainerBuilder.armorSideBar`), e `MekanismContainer.addArmorSlots` põe os quatro slots de armadura e o da mão a partir dali.
pub const ARMOR_BAR_X: i32 = -20;
pub const ARMOR_BAR_Y: i32 = 11;
pub const ARMOR_BAR_OFFHAND_OFFSET: i32 = 0;
/// `GuiHeatGenerator.java:35`: `GuiSideHolder.create(this, -26, 6, 98, true, true, SpecialColors.TAB_ARMOR_SLOTS)`, o suporte de 26 por 98 atrás
/// dos cinco slots, à esquerda (`holder_left`). A cor é o pixel 21 de `colormap/gui_objects.png` (o 22º `register()` de `SpecialColors`), branco.
pub const ARMOR_HOLDER: machinery_basis_core::gui::Rect = [-26.0, 6.0, 26.0, 98.0];
pub const ARMOR_HOLDER_SPRITE: &str = "mekanism:gui/sprites/holder_left";
pub const ARMOR_HOLDER_TINT: u32 = 0xFF_FF_FF_FF;

/// O que a tela do gerador declara à `machinery_basis` do tipo de menu: a barra de armadura e os elementos que ficam fora da imagem de
/// 176 por 166 e recebem o mouse (`GuiMekanism.hasClickedOutside`): a aba de calor e a de energia. A aba de upgrades é da lib.
pub fn armor_bar_declaration() -> machinery_basis_core::armor_bar::Declaration {
    machinery_basis_core::armor_bar::Declaration {
        x: ARMOR_BAR_X,
        y: ARMOR_BAR_Y,
        offhand_offset: ARMOR_BAR_OFFHAND_OFFSET,
        elements: vec![HEAT_TAB.rect, ENERGY_TAB.rect],
    }
}

/// `GuiHeatTab.getResource`: `Mekanism.rl("tab/heat_info_" + type.getTabName())`, no atlas de sprites da tela.
pub fn heat_tab_sprite(unit: TemperatureUnit) -> String {
    format!("mekanism:gui/sprites/tab/heat_info_{}", unit.tab_name())
}
/// O arquivo da configuração local do Mekanism (`MekanismConfig.local`, `Mekanism/local.toml`), na pasta do namespace dele.
pub const LOCAL_CONFIG_NAMESPACE: &str = "mekanism";
/// `MekanismLang.TEMPERATURE`, `TRANSFERRED_RATE` e `DISSIPATED_RATE`: os modelos das três linhas da aba de calor.
pub const TEMPERATURE_LANG: &str = "temperature.mekanism.short";
pub const TRANSFERRED_RATE_LANG: &str = "gui.mekanism.transferred_rate";
pub const DISSIPATED_RATE_LANG: &str = "gui.mekanism.dissipated";
/// `TemperatureUnit.getTranslationKey`: o nome curto que `MekanismLang.UNIT` escreve (`K`, `C`, `R`, `F`, `STP`).
pub fn unit_lang(unit: TemperatureUnit) -> &'static str {
    match unit {
        TemperatureUnit::Kelvin => "temperature.mekanism.kelvin.short",
        TemperatureUnit::Celsius => "temperature.mekanism.celsius.short",
        TemperatureUnit::Rankine => "temperature.mekanism.rankine.short",
        TemperatureUnit::Fahrenheit => "temperature.mekanism.fahrenheit.short",
        TemperatureUnit::Ambient => "temperature.mekanism.ambient.short",
    }
}

/// O que `GuiHeatGenerator` põe no `infoHandler` de `GuiHeatTab`: a temperatura do tile (`getTemperature`, o calor sobre a capacidade) e
/// as duas perdas do último tick, em kelvin, escritas na unidade da configuração local (`MekanismUtils.getTemperatureDisplay` com
/// `TemperatureUnit.KELVIN`): a temperatura desloca o zero da escala (`shift`), as perdas são diferenças e não.
pub fn heat_tab_tooltip(
    heat: f64,
    transfer_loss: f64,
    environment_loss: f64,
    local: TemperatureUnit,
    translate: &dyn Fn(&str) -> String,
) -> Vec<String> {
    let show = |value: f64, shift: bool| {
        temperature::local_display(value, TemperatureUnit::Kelvin, local, shift)
    };
    let (temperature_template, transferred_template, dissipated_template) = (
        translate(TEMPERATURE_LANG),
        translate(TRANSFERRED_RATE_LANG),
        translate(DISSIPATED_RATE_LANG),
    );
    temperature::heat_info_lines(
        &[
            (&temperature_template, show(heat / HEAT_CAPACITY, true)),
            (&transferred_template, show(transfer_loss, false)),
            (&dissipated_template, show(environment_loss, false)),
        ],
        &translate(unit_lang(local)),
        &translate(UNIT_LANG),
    )
}

/// O clique na aba (`GuiHeatTab.onClick`): sobre o retângulo dela, e fora de uma janela por cima (`blocked`, a janela de upgrades
/// aberta sobre o ponto), o botão esquerdo avança a unidade da configuração local e o direito volta. Devolve a unidade nova quando a
/// aba tomou o clique, e `None` quando ele segue para o resto da tela.
pub fn heat_tab_click(
    unit: TemperatureUnit,
    at: [f32; 2],
    button: i32,
    blocked: bool,
) -> Option<TemperatureUnit> {
    if !HEAT_TAB.is_mouse_over(at, blocked) {
        return None;
    }
    temperature::rotate(unit, button)
}

/// O tooltip de `GuiFluidGauge` (`GuiTankGauge.getContentsTooltips`): `Empty`, ou `Lava: <mB> mB`.
pub fn lava_tooltip(amount: u64, translate: &dyn Fn(&str) -> String) -> String {
    if amount == 0 {
        return translate(EMPTY_LANG);
    }
    stored_mb(
        &translate(STORED_MB_LANG),
        &translate(LAVA_NAME_LANG),
        amount,
    )
}

/// A lava no medidor: o sprite do jogo (`MekanismRenderer.getFluidTexture(lava, STILL)`), do tamanho de um quadro.
pub const LAVA_SPRITE: &str = "block/lava_still";
pub const LAVA_FRAME: [f32; 2] = [16.0, 16.0];

/// O que o gerador sabe do mundo neste tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surroundings {
    /// Lados com lava, fonte ou corrente. A lava dentro do bloco conta como mais um.
    pub lava_faces: u32,
    pub in_nether: bool,
    /// O ar do bloco, que o `simulate` do gerador usa para a eficiência de Carnot.
    pub ambient: f64,
    /// O ar de cada face, na ordem das direções do jogo (baixo, cima, norte, sul, oeste, leste): a perda para o ambiente usa o do bloco
    /// vizinho (`getAmbientTemperature(side)`).
    pub face_ambient: [f64; 6],
}

/// O que aconteceu no tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Produced {
    pub active: bool,
    pub lava_used: u64,
    pub energy: i32,
    /// `lastTransferLoss`, o `adjacentTransfer` de `simulate`: a temperatura que passou para o vizinho de baixo. Fica em zero porque
    /// nenhum bloco portado tem calor para receber (a capacidade `HEAT` do original só existe nas máquinas que a declaram).
    pub transfer_loss: f64,
    /// `lastEnvironmentLoss`, o `environmentTransfer` de `simulate`: a temperatura que o ar levou, só das faces mais quentes que ele.
    pub environment_loss: f64,
}

pub fn new_capacitor(ambient: f64) -> Capacitor {
    Capacitor::new(
        HEAT_CAPACITY,
        INVERSE_CONDUCTION,
        INVERSE_INSULATION,
        ambient,
    )
}

fn boost(around: &Surroundings) -> f64 {
    let lava = HEAT_GENERATION_LAVA * f64::from(around.lava_faces);
    if around.in_nether {
        lava + HEAT_GENERATION_NETHER
    } else {
        lava
    }
}

/// Um tick. `lava` é o que há no tanque, e a energia sai sem o limite do buffer.
pub fn tick(heat: &mut Capacitor, around: &Surroundings, lava: u64, energy_full: bool) -> Produced {
    heat.handle(boost(around));
    let active = lava >= FLUID_RATE && !energy_full;
    if active {
        heat.handle(HEAT_GENERATION);
    }
    let temperature = heat.temperature();
    let carnot = 1.0 - around.ambient.min(temperature) / around.ambient.max(temperature);
    let heat_lost = THERMAL_EFFICIENCY * (temperature - around.ambient);
    heat.handle(-heat_lost);
    let energy = ((heat_lost.abs() * carnot) as i32).min(MAX_PRODUCTION);
    let environment_loss = heat.lose_to_faces(around.face_ambient, INSULATION_BY_FACE);
    Produced {
        active,
        lava_used: if active { FLUID_RATE } else { 0 },
        energy,
        transfer_loss: 0.0,
        environment_loss,
    }
}

/// O que o Heat Generator guarda e muda a cada tick: o calor, a lava do tanque e a energia do buffer, mais o que todo tile do Mekanism
/// guarda (`currentActive`, `updateDelay` e o estado `lit` do bloco).
#[derive(Clone, Debug)]
pub struct State {
    pub heat: Capacitor,
    pub lava: u64,
    pub energy: i32,
    pub tile: TileBase,
}

impl State {
    pub fn new(ambient: f64) -> Self {
        Self {
            heat: new_capacitor(ambient),
            lava: 0,
            energy: 0,
            tile: TileBase::new(BLOCK_DEACTIVATION_DELAY),
        }
    }
}

/// Um tick do tile (`tickServer` e `onUpdateServer`, depois do slot de combustível): conta o atraso do bloco aceso, queima a lava,
/// guarda a energia no buffer, liga ou desliga o tile (`setActive`) e devolve o que entrou no buffer (`producingEnergy`: a diferença
/// da energia antes e depois, que o limite do buffer corta).
pub fn step(state: &mut State, around: &Surroundings) -> Produced {
    state.tile.tick_delay();
    let full = state.energy >= ENERGY_CAPACITY;
    let mut produced = tick(&mut state.heat, around, state.lava, full);
    state.lava -= produced.lava_used;
    let filled = state
        .energy
        .saturating_add(produced.energy)
        .min(ENERGY_CAPACITY);
    produced.energy = filled - state.energy;
    state.energy = filled;
    state.tile.set_active(produced.active);
    produced
}

/// O `MEKTRACE` de um tick: os campos de `dev/referencia/rastros/heat_generator_*` que a regra calcula, com os mesmos nomes. `now` é o
/// tempo do jogo, `fuel` o item do slot de combustível (caminho e quantidade) e `state` o que o tick deixou.
pub fn trace_line(
    now: i64,
    state: &State,
    around: &Surroundings,
    produced: &Produced,
    fuel: (&str, u64),
) -> String {
    let faces: Vec<String> = around.face_ambient.iter().map(f64::to_string).collect();
    format!(
        "{{\"t\":{now},\"heat\":{},\"temperature\":{},\"ambient\":{},\"ambient_faces\":[{}],\"lava_faces\":{},\"energy\":{},\"capacity\":{ENERGY_CAPACITY},\"lava\":{},\"produced\":{},\"transfer_loss\":{:?},\"environment_loss\":{:?},\"active\":{},\"block_active\":{},\"update_delay\":{},\"fuel\":[\"{}\",{}]}}",
        state.heat.heat,
        state.heat.temperature(),
        around.ambient,
        faces.join(","),
        around.lava_faces,
        state.energy,
        state.lava,
        produced.energy,
        produced.transfer_loss,
        produced.environment_loss,
        state.tile.active(),
        state.tile.block_active(),
        state.tile.update_delay(),
        fuel.0,
        fuel.1,
    )
}

/// O que há no slot de combustível.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FuelItem {
    /// Um balde de lava, que o slot esvazia no tanque.
    LavaBucket,
    /// Um item que queima por `burn_time` ticks, convertido em lava em proporção ao balde de lava.
    Solid {
        burn_time: i32,
        lava_bucket_burn_time: i32,
    },
}

/// `FluidFuelInventorySlot.fillOrBurn`: quanta lava o item põe no tanque, que tem `room` livre. `None` deixa o item no slot.
pub fn fuel_to_tank(item: FuelItem, room: u64) -> Option<u64> {
    if room == 0 {
        return None;
    }
    match item {
        FuelItem::LavaBucket => (room >= BUCKET_VOLUME).then_some(BUCKET_VOLUME),
        FuelItem::Solid {
            burn_time,
            lava_bucket_burn_time,
        } => {
            let fuel = lava_from_fuel(burn_time, lava_bucket_burn_time);
            (fuel > 0 && fuel <= room).then_some(fuel)
        }
    }
}

/// Quanto de lava um item de `burn_time` rende, em proporção a um balde de lava.
pub fn lava_from_fuel(burn_time: i32, lava_bucket_burn_time: i32) -> u64 {
    if burn_time <= 0 || lava_bucket_burn_time <= 0 {
        return 0;
    }
    BUCKET_VOLUME * burn_time as u64 / lava_bucket_burn_time as u64
}

// ---------------------------------------------------------------------------------------------------------------------------
// Upgrades (`TileComponentUpgrade`) e a janela deles. O gerador só aceita o de silêncio (`AttributeUpgradeSupport.MUFFLING_ONLY`). O
// item, o componente que guarda o tipo, os tipos e as tags são do Mekanism (o Generators usa os dados dele, como os sprites e o idioma),
// então os ids ficam aqui e não na `machinery_basis`.
use machinery_basis_core::upgrade::{UpgradeData, UpgradeTags, UpgradeTypes};
use machinery_basis_core::upgrade_window::UpgradeSync;
use machinery_basis_core::window::{SelectedWindow, WindowType};
use std::rc::Rc;

/// `MekanismItems.UPGRADE` e o componente `MekanismDataComponents.UPGRADE_TYPE`, que guarda o tipo no item.
pub const UPGRADE_ITEM: &str = "mekanism:upgrade";
pub const UPGRADE_TYPE_COMPONENT: &str = "mekanism:upgrade_type";
/// `UpgradeIds.MUFFLING`, o único tipo que o gerador aceita.
pub const MUFFLING_UPGRADE: &str = "mekanism:muffling";
/// `MekanismTags.Upgrades.MUFFLING_ONLY`: o que `AttributeUpgradeSupport.MUFFLING_ONLY` aceita.
pub const MUFFLING_ONLY: &str = "mekanism:upgrade_support/muffling_only";
/// `UpgradeUtils`: o multiplicador dos upgrades (`upgradesMultiplier`).
pub const UPGRADE_MULTIPLIER: f64 = 10.0;

/// Os dados de upgrade do gerador: os tipos, o que a tag `muffling_only` aceita, e o item que os carrega, com o limite de pilha dele.
pub fn upgrade_data(
    types: &UpgradeTypes,
    tags: &UpgradeTags,
    item_max_stack_size: u32,
) -> Rc<UpgradeData> {
    Rc::new(UpgradeData::new(
        UPGRADE_ITEM,
        UPGRADE_TYPE_COMPONENT,
        item_max_stack_size,
        types,
        tags,
        MUFFLING_ONLY,
    ))
}

/// Os dois slots virtuais da janela de upgrades vêm depois dos do gerador (`TileComponentUpgrade`: entrada e saída).
pub const UPGRADE_SLOT: usize = SLOTS_SPEC.len();
pub const UPGRADE_OUTPUT_SLOT: usize = UPGRADE_SLOT + 1;
/// Quantos slots o contêiner do gerador declara, os virtuais inclusos.
pub const MENU_SLOTS: usize = UPGRADE_SLOT + 2;

/// Os slots do gerador no menu: o de combustível e o de energia, na ordem de `getInitialInventory`.
pub const FUEL_SLOT: usize = 0;
pub const ENERGY_SLOT: usize = 1;

/// Os slots do contêiner do menu, na ordem em que o Java os adiciona (`inventoryContainerSlots`): os dois virtuais da janela de upgrades
/// vêm ANTES dos do gerador (`MekanismTileContainer.addSlots`: "add them before the main inventory to make sure they take priority in
/// targeting"), e a prioridade do shift-clique vem daí. No Horizon eles ficam no fim, para não deslocar o NBT de mundos antigos.
pub const CONTAINER_ORDER: [usize; 4] = [UPGRADE_SLOT, UPGRADE_OUTPUT_SLOT, FUEL_SLOT, ENERGY_SLOT];

/// `ITransactionalSlot.exists(selectedWindow)`: um slot virtual só existe com a janela dele escolhida por quem clica.
pub fn slot_exists(slot: usize, selected: Option<SelectedWindow>) -> bool {
    if slot == UPGRADE_SLOT || slot == UPGRADE_OUTPUT_SLOT {
        return SelectedWindow::exists(SelectedWindow::new(UPGRADE_WINDOW, 0), selected);
    }
    true
}

/// `isValidForInsertion(.., MANUAL)` do slot virtual de entrada (`UpgradeInventorySlot.input`): o item de upgrade com um tipo da tag
/// do gerador. `kind` é o tipo que o item carrega no componente. A saída de upgrades não aceita nada de quem clica.
pub fn accepts_upgrade(data: &UpgradeData, item: &str, kind: Option<&str>) -> bool {
    item == UPGRADE_ITEM && kind.is_some_and(|kind| data.supports(kind))
}

/// `MekanismContainer.UPGRADE_WINDOW`: a chave de `startTracking` da janela de upgrades.
pub const UPGRADE_TRACKING_KEY: u8 = 2;
/// Quantos dados a janela acrescenta ao contêiner (`getSpecificSyncableData`: `upgradeTicks` e o mapa).
pub const UPGRADE_TRACKED_ITEMS: usize = 2;
/// Quantos dados o contêiner do Heat Generator já sincroniza ao nascer (`addContainerTrackers`), medidos no rastro
/// `heat_generator_container_tracking`.
pub const HEAT_TRACKED_BASE: usize = 13;

/// Quantos tipos de upgrade o Mekanism declara em `data/mekanism/mekanism/upgrade`: cada um tem a contagem dele num data slot.
pub const UPGRADE_TYPES: usize = 8;
/// Os data slots dos upgrades: `upgradeTicks` e uma contagem por tipo, na ordem do registro. O Java os sincroniza só com a janela aberta,
/// por jogador, e aqui eles são do bloco.
pub const UPGRADE_SYNC_LEN: usize = 1 + UPGRADE_TYPES;
/// Onde eles começam. Os índices de 6 a 13 são da aba de calor (quatro perdas de transferência e quatro do ambiente).
pub const UPGRADE_DATA: usize = 14;
/// O calor exato do capacitor (um `double`, quatro slots de 16 bits), depois dos upgrades: a aba de calor mostra o calor sobre a capacidade.
pub const DATA_HEAT: usize = UPGRADE_DATA + UPGRADE_SYNC_LEN;
/// Quantos data slots o menu do gerador declara: o `data_slots` do arquivo de menu.
pub const DATA_SLOTS: usize = DATA_HEAT + 4;

/// Escreve o `upgradeTicks` e o mapa de upgrades nos data slots.
pub fn write_upgrade_sync(data: &mut [i32], sync: &UpgradeSync) {
    for (slot, value) in data
        .iter_mut()
        .skip(UPGRADE_DATA)
        .zip(sync.write(UPGRADE_SYNC_LEN))
    {
        *slot = value;
    }
}

/// O que o cliente lê dos data slots dos upgrades.
pub fn read_upgrade_sync(data: &[i32]) -> UpgradeSync {
    let slots = data.get(UPGRADE_DATA..).unwrap_or_default();
    UpgradeSync::read(&slots[..slots.len().min(UPGRADE_SYNC_LEN)])
}

/// `WindowType`, na ordem do enum do Java. `CRAFTING` aceita `MAX_CRAFTING_WINDOWS` (3) janelas. É a tabela do Mekanism: o gerador abre a
/// mesma janela.
pub const WINDOW_TYPES: [WindowType; 11] = [
    window_type(0, Some("color"), false, 1),
    window_type(1, Some("confirmation"), false, 1),
    window_type(2, Some("crafting"), true, 3),
    window_type(3, Some("mekasuit_helmet"), false, 1),
    window_type(4, Some("recipe_book"), false, 1),
    window_type(5, Some("rename"), false, 1),
    window_type(6, Some("skin_select"), false, 1),
    window_type(7, Some("side_config"), true, 1),
    window_type(8, Some("transporter_config"), true, 1),
    window_type(9, Some("upgrade"), true, 1),
    window_type(10, None, false, 1),
];
/// `WindowType.UPGRADE`.
pub const UPGRADE_WINDOW: WindowType = WINDOW_TYPES[9];

const fn window_type(
    ordinal: u8,
    save_name: Option<&'static str>,
    can_pin: bool,
    max_data: u8,
) -> WindowType {
    WindowType {
        ordinal,
        save_name,
        can_pin,
        max_data,
    }
}

/// `EnumColor.getColor` pelo nome que o arquivo do tipo de upgrade usa (`color`), a cor primária (`colormap/primary.png`) em ARGB. Só as
/// que os tipos declarados usam; um nome que não é daqui vale branco, o que o `Upgrade.argb()` de uma cor sem tinta dá.
pub fn enum_color(name: &str) -> u32 {
    match name {
        "green" => 0xFF_59_C1_5F,
        "yellow" => 0xFF_FF_DD_4F,
        "lime" => 0xFF_75_FF_89,
        "cyan" => 0xFF_00_F3_D0,
        "light_blue" => 0xFF_55_9E_FF,
        "red" => 0xFF_FF_38_3C,
        "orange" => 0xFF_FF_A1_60,
        _ => 0xFF_FF_FF_FF,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use machinery_basis_core::heat::AMBIENT_TEMP;

    const PLAINS: Surroundings = Surroundings {
        lava_faces: 0,
        in_nether: false,
        ambient: AMBIENT_TEMP,
        face_ambient: [AMBIENT_TEMP; 6],
    };

    fn run(ticks: u32, around: &Surroundings, lava: u64) -> (Capacitor, Produced) {
        let mut heat = new_capacitor(AMBIENT_TEMP);
        let mut last = Produced::default();
        for _ in 0..ticks {
            last = tick(&mut heat, around, lava, false);
        }
        (heat, last)
    }

    /// Valores calculados à parte, em Python, seguindo as linhas do Java.
    fn assert_matches(ticks: u32, around: &Surroundings, lava: u64, temperature: f64, energy: i32) {
        let (heat, produced) = run(ticks, around, lava);
        assert!(
            (heat.temperature() - temperature).abs() < 0.001,
            "{ticks} ticks: {}",
            heat.temperature()
        );
        assert_eq!(produced.energy, energy, "{ticks} ticks");
    }

    #[test]
    fn matches_the_reference_curve() {
        assert_matches(1, &PLAINS, TANK_CAPACITY, 309.4944, 0);
        assert_matches(2, &PLAINS, TANK_CAPACITY, 318.5086, 0);
        assert_matches(10, &PLAINS, TANK_CAPACITY, 376.0101, 8);
        assert_matches(100, &PLAINS, TANK_CAPACITY, 486.7183, 38);
        assert_matches(3_000, &PLAINS, TANK_CAPACITY, 487.7657, 39);
    }

    #[test]
    fn surrounded_by_lava_it_produces_with_no_fuel() {
        let around = Surroundings {
            lava_faces: 7,
            ..PLAINS
        };
        assert_matches(3_000, &around, 0, 392.0052, 11);
        assert!(!run(3_000, &around, 0).1.active);
    }

    #[test]
    fn one_lava_face_warms_it_without_producing() {
        let around = Surroundings {
            lava_faces: 1,
            ..PLAINS
        };
        assert_matches(3_000, &around, 0, 313.1436, 0);
    }

    #[test]
    fn stays_cold_without_lava() {
        let (heat, produced) = run(1, &PLAINS, 0);
        assert_eq!(produced, Produced::default());
        assert_eq!(heat.temperature(), AMBIENT_TEMP);
    }

    #[test]
    fn stops_burning_when_full() {
        let mut heat = new_capacitor(AMBIENT_TEMP);
        let produced = tick(&mut heat, &PLAINS, TANK_CAPACITY, true);
        assert!(!produced.active);
        assert_eq!(produced.lava_used, 0);
    }

    #[test]
    fn less_than_a_tick_of_lava_does_not_burn() {
        assert!(!run(1, &PLAINS, FLUID_RATE - 1).1.active);
    }

    #[test]
    fn the_nether_adds_heat() {
        let nether = Surroundings {
            in_nether: true,
            ..PLAINS
        };
        assert!(run(1, &nether, 0).0.temperature() > AMBIENT_TEMP);
    }

    #[test]
    fn fuel_converts_by_burn_time() {
        assert_eq!(lava_from_fuel(1_600, 20_000), 80);
        assert_eq!(lava_from_fuel(20_000, 20_000), 1_000);
        assert_eq!(lava_from_fuel(0, 20_000), 0);
    }

    #[test]
    fn the_block_stays_lit_for_the_deactivation_delay() {
        let mut state = State::new(AMBIENT_TEMP);
        state.lava = FLUID_RATE;
        step(&mut state, &PLAINS);
        assert!(
            state.tile.active() && state.tile.block_active(),
            "liga na hora"
        );
        // Sem atraso armado, o primeiro desligamento apaga o bloco na hora e arma o atraso.
        step(&mut state, &PLAINS);
        assert!(!state.tile.active() && !state.tile.block_active());
        assert_eq!(state.tile.update_delay(), BLOCK_DEACTIVATION_DELAY);
        // Ligar de novo é instantâneo, e o atraso armado segura o desligamento seguinte.
        state.lava = FLUID_RATE;
        step(&mut state, &PLAINS);
        assert!(state.tile.block_active());
        step(&mut state, &PLAINS);
        assert!(!state.tile.active());
        assert_eq!(state.tile.update_delay(), BLOCK_DEACTIVATION_DELAY);
        for _ in 1..BLOCK_DEACTIVATION_DELAY {
            assert!(state.tile.block_active());
            step(&mut state, &PLAINS);
        }
        assert!(state.tile.block_active());
        step(&mut state, &PLAINS);
        assert!(!state.tile.block_active());
    }
}
