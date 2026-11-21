//! As telas dos tanques (`GuiFluidTank` e `GuiChemicalTank`), sem o jogo: os dados do menu, os slots, o medidor, a barra, a tela interna, os modos e os tooltips contra o Java.
use machinery_basis_core::chemical_tank::GasMode;
use machinery_basis_core::container_sync::{Kind, Mirror, PropertyData, ResourceType, StackValue};
use machinery_basis_core::gui;
use machinery_basis_core::handler_slot::ContainerEditMode;
use machinery_basis_core::slot_view::{SlotOverlay, SlotType, slot_type};
use machinery_basis_core::tile_trackers::source;
use mekanism_rules::{
    CHEMICAL_TANK_BAR, CHEMICAL_TANK_ITEM_CONFIG, CHEMICAL_TANK_MODE_BUTTON, CHEMICAL_TANK_SCREEN,
    CHEMICAL_TANK_SLOTS_SPEC, ChemicalTankScreenData, EDIT_MODE_TAB_TINT, FLUID_TANK_GAUGE,
    FLUID_TANK_SLOTS_SPEC, FluidTankScreenData, SOURCE_TANK_MODE, TANK_ARMOR_HOLDER,
    TANK_GAS_MODE_BUTTON, TANK_NEXT_MODE_BUTTON, TANK_PREVIOUS_MODE_BUTTON,
    chemical_tank_bar_level, chemical_tank_screen_lines, chemical_tank_trackers, edit_mode_button,
    edit_mode_click, edit_mode_icon, edit_mode_lang, edit_mode_tab, fluid_tank_trackers,
    gas_mode_icon, gas_mode_lang, gas_mode_text_area, tank_armor_bar_declaration,
    tank_contents_tooltip,
};
use std::path::{Path, PathBuf};

fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("content/assets/mekanism")
}

/// O `en_us.json` do conteúdo: os textos são os do original (o teste `craft_content` confere cada chave contra a datagen).
fn translate(key: &str) -> String {
    let text = std::fs::read_to_string(assets().join("lang/en_us.json")).unwrap();
    let lang: serde_json::Value = serde_json::from_str(&text).unwrap();
    lang.get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("sem a chave {key}"))
        .to_owned()
}

/// `TileEntityFluidTank.addContainerTrackers` (`:298-301`) sobre `TileEntityMekanism.addContainerTrackers` (`:922-965`): a frequência e o modo de segurança (os componentes),
/// nada de redstone (`without(AttributeRedstone.class)`), o tanque de fluido e, por último, o modo de edição.
#[test]
fn the_fluid_tank_menu_data_follows_the_tile_order() {
    let list = fluid_tank_trackers();
    let sources: Vec<u16> = list.iter().map(|tracked| tracked.source).collect();
    assert_eq!(
        sources,
        [
            source::SECURITY_FREQUENCY,
            source::SECURITY_MODE,
            source::FLUID_TANK,
            SOURCE_TANK_MODE,
        ]
    );
    assert_eq!(list[2].kind, Kind::LargeResourceStack(ResourceType::Fluid));
    assert_eq!(list[3].kind, Kind::Enum { default: 0 });
}

/// `TileEntityChemicalTank.addContainerTrackers` (`:205-208`): a segurança, os três da redstone, o tanque de químico e o modo de descarte.
#[test]
fn the_chemical_tank_menu_data_follows_the_tile_order() {
    let list = chemical_tank_trackers();
    let sources: Vec<u16> = list.iter().map(|tracked| tracked.source).collect();
    assert_eq!(
        sources,
        [
            source::SECURITY_FREQUENCY,
            source::SECURITY_MODE,
            source::CONTROL_TYPE,
            source::POWERED,
            source::WAS_POWERED,
            source::CHEMICAL_TANK,
            SOURCE_TANK_MODE,
        ]
    );
    assert_eq!(
        list[5].kind,
        Kind::LargeResourceStack(ResourceType::Chemical)
    );
    assert_eq!(list[6].kind, Kind::Enum { default: 0 });
}

#[test]
fn an_empty_mirror_reads_the_defaults() {
    assert_eq!(
        FluidTankScreenData::read(&Mirror::new(&fluid_tank_trackers())),
        FluidTankScreenData {
            fluid: StackValue::default(),
            mode: ContainerEditMode::Both
        }
    );
    assert_eq!(
        ChemicalTankScreenData::read(&Mirror::new(&chemical_tank_trackers())),
        ChemicalTankScreenData {
            chemical: StackValue::default(),
            dumping: GasMode::Idle
        }
    );
}

/// O que o servidor manda chega ao que a tela lê, e um ordinal fora da lista cai no primeiro (`ByIdMap.OutOfBoundsStrategy.ZERO`).
#[test]
fn the_mirror_values_reach_the_screen() {
    let mut mirror = Mirror::new(&fluid_tank_trackers());
    assert!(mirror.apply(3, &PropertyData::Int(2)));
    assert_eq!(
        FluidTankScreenData::read(&mirror).mode,
        ContainerEditMode::Empty
    );
    assert!(mirror.apply(3, &PropertyData::Int(1)));
    assert_eq!(
        FluidTankScreenData::read(&mirror).mode,
        ContainerEditMode::Fill
    );
    assert!(mirror.apply(3, &PropertyData::Int(7)));
    assert_eq!(
        FluidTankScreenData::read(&mirror).mode,
        ContainerEditMode::Both
    );
    let mut mirror = Mirror::new(&chemical_tank_trackers());
    assert!(mirror.apply(6, &PropertyData::Int(1)));
    assert_eq!(
        ChemicalTankScreenData::read(&mirror).dumping,
        GasMode::DumpingExcess
    );
    assert!(mirror.apply(6, &PropertyData::Int(2)));
    assert_eq!(
        ChemicalTankScreenData::read(&mirror).dumping,
        GasMode::Dumping
    );
    assert!(mirror.apply(6, &PropertyData::Int(-1)));
    assert_eq!(ChemicalTankScreenData::read(&mirror).dumping, GasMode::Idle);
}

/// `TileEntityFluidTank.getInitialInventory` (`:125-128`) e `GuiMekanism.addSlots`: os dois slots com a moldura normal e os desenhos de entrada e de saída.
#[test]
fn the_fluid_tank_slots_are_the_originals() {
    assert_eq!(FLUID_TANK_SLOTS_SPEC[0].item, [146.0, 19.0]);
    assert_eq!(FLUID_TANK_SLOTS_SPEC[1].item, [146.0, 51.0]);
    assert_eq!(FLUID_TANK_SLOTS_SPEC[0].overlay, Some(SlotOverlay::Input));
    assert_eq!(FLUID_TANK_SLOTS_SPEC[1].overlay, Some(SlotOverlay::Output));
    for (index, slot) in FLUID_TANK_SLOTS_SPEC.iter().enumerate() {
        assert_eq!(
            slot_type(None, index, slot.container),
            Some(SlotType::Normal)
        );
    }
}

/// `TileEntityChemicalTank.getInitialInventory` (`:104-109`) e `setupIOConfig(ITEM, drainSlot, fillSlot, true)` (`:87`): o de drenar é a entrada da configuração de itens e
/// o de encher a saída, e a moldura segue esse papel (`GuiMekanism.findDataType`).
#[test]
fn the_chemical_tank_slots_are_the_originals() {
    assert_eq!(CHEMICAL_TANK_SLOTS_SPEC[0].item, [16.0, 16.0]);
    assert_eq!(CHEMICAL_TANK_SLOTS_SPEC[1].item, [16.0, 48.0]);
    assert_eq!(CHEMICAL_TANK_SLOTS_SPEC[0].overlay, Some(SlotOverlay::Plus));
    assert_eq!(
        CHEMICAL_TANK_SLOTS_SPEC[1].overlay,
        Some(SlotOverlay::Minus)
    );
    assert_eq!(
        slot_type(
            Some(&CHEMICAL_TANK_ITEM_CONFIG),
            0,
            CHEMICAL_TANK_SLOTS_SPEC[0].container
        ),
        Some(SlotType::Input)
    );
    assert_eq!(
        slot_type(
            Some(&CHEMICAL_TANK_ITEM_CONFIG),
            1,
            CHEMICAL_TANK_SLOTS_SPEC[1].container
        ),
        Some(SlotType::Output)
    );
}

/// As posições dos elementos: `GuiFluidGauge(.., WIDE, this, 48, 18)`, `GuiChemicalBar(.., 42, 16, 116, 10, true)`, `GuiInnerScreen(this, 42, 37, 118, 28)`,
/// `GuiGasMode(this, 159, 72, true, ..)`, `GuiContainerEditModeTab` (`imageWidth`, 138, 26) e `GuiSideHolder.armorHolder` (-26, 62, 98), com a barra de armadura do
/// `armorSideBar()` sem argumentos (-20, 67, 0).
#[test]
fn the_elements_sit_where_the_original_puts_them() {
    assert_eq!(FLUID_TANK_GAUGE, [48.0, 18.0, 66.0, 50.0]);
    assert_eq!(CHEMICAL_TANK_BAR, [42.0, 16.0, 118.0, 12.0]);
    assert_eq!(CHEMICAL_TANK_SCREEN, [42.0, 37.0, 118.0, 28.0]);
    assert_eq!(CHEMICAL_TANK_MODE_BUTTON, [159.0, 72.0, 10.0, 10.0]);
    assert_eq!(TANK_ARMOR_HOLDER, [-26.0, 62.0, 26.0, 98.0]);
    let tab = edit_mode_tab();
    assert_eq!(tab.holder(), [176.0, 138.0, 26.0, 26.0]);
    assert_eq!(tab.button(), [179.0, 142.0, 18.0, 18.0]);
    assert_eq!(EDIT_MODE_TAB_TINT, 0xFF_36_6B_D0);
    let bar = tank_armor_bar_declaration();
    assert_eq!((bar.x, bar.y, bar.offhand_offset), (-20, 67, 0));
    // `drawScrollingString(.., -69, 1, RIGHT, .., 69, 2, false)` do botão em (159, 72): de 92 a 157, na linha de 73 a 82.
    assert_eq!(gas_mode_text_area(9.0), [92.0, 73.0, 157.0, 82.0]);
}

/// `GuiContainerEditModeTab.onClick` e `TileEntityFluidTank.nextMode`/`previousMode` (`IIncrementalEnum`): o esquerdo avança, o direito volta, e os dois dão a volta.
#[test]
fn the_edit_mode_tab_cycles_both_ways() {
    assert_eq!(edit_mode_click(false), TANK_NEXT_MODE_BUTTON);
    assert_eq!(edit_mode_click(true), TANK_PREVIOUS_MODE_BUTTON);
    use ContainerEditMode::{Both, Empty, Fill};
    for (mode, next, previous) in [
        (Both, Fill, Empty),
        (Fill, Empty, Both),
        (Empty, Both, Fill),
    ] {
        assert_eq!(edit_mode_button(mode, TANK_NEXT_MODE_BUTTON), Some(next));
        assert_eq!(
            edit_mode_button(mode, TANK_PREVIOUS_MODE_BUTTON),
            Some(previous)
        );
        assert_eq!(edit_mode_button(mode, TANK_GAS_MODE_BUTTON), None);
    }
}

/// Os sprites e os textos de cada modo existem no conteúdo, com o nome que o original usa (`GuiContainerEditModeTab`, `GuiGasMode`, `MekanismLang`).
#[test]
fn every_mode_has_its_sprite_and_its_text() {
    let sprite = |name: &str| {
        let path = name.strip_prefix("mekanism:gui/sprites/").unwrap();
        assets()
            .join("textures/gui/sprites")
            .join(format!("{path}.png"))
    };
    let expected = [
        (ContainerEditMode::Both, "both", "Both"),
        (ContainerEditMode::Fill, "fill", "Fill"),
        (ContainerEditMode::Empty, "empty", "Empty"),
    ];
    for (mode, file, text) in expected {
        assert!(edit_mode_icon(mode).ends_with(&format!("container_edit_mode/{file}")));
        assert!(sprite(edit_mode_icon(mode)).is_file(), "{file}");
        assert_eq!(translate(edit_mode_lang(mode)), text);
    }
    let expected = [
        (GasMode::Idle, "idle", "Idle"),
        (GasMode::DumpingExcess, "dump_excess", "Dumping Excess"),
        (GasMode::Dumping, "dump", "Dumping"),
    ];
    for (mode, file, text) in expected {
        assert!(gas_mode_icon(mode).ends_with(&format!("chemical_mode/{file}")));
        assert!(sprite(gas_mode_icon(mode)).is_file(), "{file}");
        assert_eq!(translate(gas_mode_lang(mode)), text);
    }
    for overlay in ["input", "output"] {
        assert!(
            assets()
                .join(format!("textures/gui/sprites/slot/overlay/{overlay}.png"))
                .is_file()
        );
    }
}

/// `GuiChemicalTank.addGuiElements` (`:36-53`), a tela interna: vazio é `Chemical: None` e `0/<capacidade>`; com algo, o nome e a fração; o criativo mostra `Infinite`.
#[test]
fn the_inner_screen_lines_follow_the_java() {
    let lines = |name: Option<&str>, amount, capacity, creative| {
        chemical_tank_screen_lines(name, amount, capacity, creative, &translate)
    };
    assert_eq!(
        lines(None, 0, 64_000, false),
        ["Chemical: None".to_owned(), "0/64,000".to_owned()]
    );
    assert_eq!(
        lines(None, 0, i64::MAX, true),
        ["Chemical: None".to_owned(), "0/Infinite".to_owned()]
    );
    assert_eq!(
        lines(Some("Hydrogen"), 5_000, 64_000, false),
        ["Chemical: Hydrogen".to_owned(), "5,000/64,000".to_owned()]
    );
    assert_eq!(
        lines(Some("Hydrogen"), 8_192_000, 8_192_000, false),
        [
            "Chemical: Hydrogen".to_owned(),
            "8,192,000/8,192,000".to_owned()
        ]
    );
    assert_eq!(
        lines(Some("Hydrogen"), i64::MAX as u64, i64::MAX, true),
        ["Chemical: Hydrogen".to_owned(), "Infinite".to_owned()]
    );
}

/// `GuiTankGauge.getContentsTooltips` e `GuiTankBar.getContentsTooltips`: `Empty`, `<nome>: Infinite` com `Long.MAX_VALUE`, ou `<nome>: <mB> mB`.
#[test]
fn the_contents_tooltip_follows_the_java() {
    assert_eq!(tank_contents_tooltip(None, 0, &translate), "Empty");
    assert_eq!(
        tank_contents_tooltip(Some("Water"), 32_000, &translate),
        "Water: 32,000 mB"
    );
    assert_eq!(
        tank_contents_tooltip(Some("Lava"), i64::MAX as u64, &translate),
        "Lava: Infinite"
    );
    assert_eq!(
        tank_contents_tooltip(Some("Hydrogen"), 1, &translate),
        "Hydrogen: 1 mB"
    );
}

/// `GuiTankGauge.getScaledLevel` no medidor largo do Fluid Tank (48 de miolo): `max(1, round(escala * 48))`, tudo com `Long.MAX_VALUE`, nada vazio.
#[test]
fn the_fluid_gauge_level_follows_the_java() {
    let height = FLUID_TANK_GAUGE[3] as i32;
    assert_eq!(gui::gauge_level(0, 32_000, height), 0);
    assert_eq!(gui::gauge_level(1, 32_000, height), 1);
    assert_eq!(gui::gauge_level(16_000, 32_000, height), 24);
    assert_eq!(gui::gauge_level(32_000, 32_000, height), 48);
    // 1.000 de 32.000 são 1,5 pixel: `Math.round` sobe.
    assert_eq!(gui::gauge_level(1_000, 32_000, height), 2);
    assert_eq!(
        gui::gauge_level(i64::MAX as u64, i64::MAX as u64, height),
        48
    );
}

/// `ResourceTankInfoProvider.getLevel` e `GuiTankBar.renderBarContentsDirect` na barra de 116 do Chemical Tank: `ceil(nível * 115) + 1` pixels a partir da esquerda.
#[test]
fn the_chemical_bar_fills_like_the_java() {
    let width = |amount, capacity| {
        gui::bar_contents_horizontal(CHEMICAL_TANK_BAR, chemical_tank_bar_level(amount, capacity))
            .map(|rect| rect[2])
    };
    assert_eq!(width(0, 64_000), None);
    assert_eq!(width(1, 64_000), Some(2.0));
    assert_eq!(width(32_000, 64_000), Some(59.0));
    assert_eq!(width(64_000, 64_000), Some(116.0));
    assert_eq!(width(i64::MAX as u64, i64::MAX), Some(116.0));
    let rect = gui::bar_contents_horizontal(CHEMICAL_TANK_BAR, 1.0).unwrap();
    assert_eq!(rect, [43.0, 17.0, 116.0, 10.0]);
}

/// As duas linhas da tela interna, no formato padrão de `GuiInnerScreen` (`padding(5).spacing(2).textScale(0.8F)`, centradas): a primeira de 41 a 50 e a segunda de 52 a
/// 61, entre 47 e 155.
#[test]
fn the_inner_screen_text_rows() {
    let rows = gui::inner_screen_rows(
        CHEMICAL_TANK_SCREEN,
        2,
        9,
        gui::INNER_SCREEN_SPACING,
        gui::INNER_SCREEN_PADDING,
    );
    assert_eq!(
        rows,
        vec![[47.0, 41.0, 155.0, 50.0], [47.0, 52.0, 155.0, 61.0]]
    );
}

/// O Horizon só abre o menu de um bloco de mod que tem o tipo de menu declarado no conteúdo, com a mesma contagem de slots do contêiner (`native_containers`): os dez
/// tanques têm os dois slots do tile.
#[test]
fn every_tank_declares_its_menu() {
    let menus = Path::new(env!("CARGO_MANIFEST_DIR")).join("content/data/mekanism/horizon/menu");
    for tank in mekanism_rules::FLUID_TANK_TIERS
        .iter()
        .chain(mekanism_rules::CHEMICAL_TANK_TIERS.iter())
    {
        let name = tank.strip_prefix("mekanism:").unwrap();
        let text = std::fs::read_to_string(menus.join(format!("{name}.json")))
            .unwrap_or_else(|_| panic!("sem o menu de {name}"));
        let menu: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(menu["slots"], 2, "{name}");
    }
}
