//! Os efeitos de tela do Configurator e dos itens de modo (decisão 48) contra o original: o texto, o ícone e o HUD de cada modo (`rastros/configurator_modes.jsonl`), o
//! menu radial do item e a sobreposição da face. As contas de posição, tempo e geometria estão em `machinery_basis_core` (`radial`, `status_overlay`, `item_hud`,
//! `face_overlay`); aqui fica o que é do Mekanism.
use machinery_basis_core::side_config::{DataType, TransmissionType};
use machinery_basis_core::sides::Face;
use machinery_basis_core::tile::ProgressMachine;
use mekanism_rules::{
    CLIENT_CONFIG, CONFIGURATOR_RADIAL_ID, ConfiguratorMode, EffectsConfig,
    advanced_machine_default, configurator_overlay, effects_config, electric_machine_default,
    overlay_argb, overlay_sprite,
};
use serde_json::Value;
use std::path::Path;

mod common;

fn rows(file: &str) -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros")
        .join(file);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|row: &Value| row.get("meta").is_none())
        .collect()
}

fn mode_rows() -> Vec<Value> {
    rows("configurator_modes.jsonl")
        .into_iter()
        .filter(|row| row["case"] == "mode")
        .collect()
}

#[test]
fn the_slice_name_the_icon_the_scroll_text_and_the_hud_line_are_the_originals() {
    let mut checked = 0;
    for row in mode_rows() {
        let mode = ConfiguratorMode::from_serialized_name(row["mode"].as_str().unwrap()).unwrap();
        assert_eq!(mode.slice_name().to_json(), row["slice_name"], "{mode:?}");
        assert_eq!(mode.scroll_text().to_json(), row["scroll_text"], "{mode:?}");
        assert_eq!(mode.hud_line().to_json(), row["hud_strings"][0], "{mode:?}");
        assert_eq!(row["hud_strings"].as_array().unwrap().len(), 1);
        // O ícone do original é o sprite do atlas de telas (`mekanism:transmission/items`, `mekanism:radial/empty`); o do port é o mesmo na pasta de sprites do
        // conteúdo.
        assert_eq!(
            mode.radial_icon(),
            format!(
                "mekanism:gui/sprites/{}",
                row["icon"]
                    .as_str()
                    .unwrap()
                    .trim_start_matches("mekanism:")
            ),
            "{mode:?}"
        );
        checked += 1;
    }
    assert_eq!(checked, 8);
}

#[test]
fn the_configurator_key_opens_the_radial_instead_of_changing_the_mode() {
    // `IModeItem.isModeItem(stack, MAINHAND)` é verdadeiro e sem o menu radial é falso: `handModeSwitchKey` não troca o modo do Configurator.
    for row in mode_rows() {
        assert_eq!(row["is_mode_item"], true);
        assert_eq!(row["is_mode_item_without_radial"], false);
    }
}

#[test]
fn the_radial_data_is_the_enum_in_order() {
    let radial = rows("configurator_modes.jsonl")
        .into_iter()
        .find(|row| row["case"] == "radial")
        .unwrap();
    assert_eq!(radial["id"], CONFIGURATOR_RADIAL_ID);
    assert_eq!(radial["modes"], ConfiguratorMode::VALUES.len());
    assert_eq!(
        radial["default_mode"],
        ConfiguratorMode::DEFAULT.serialized_name()
    );
    for (index, mode) in ConfiguratorMode::VALUES.iter().enumerate() {
        assert_eq!(radial["network_representation"][index], mode.ordinal());
        assert_eq!(radial["index"][index], mode.ordinal());
    }
    assert_eq!(radial["same_data_every_call"], true);
}

fn machine(infuser: bool) -> ProgressMachine {
    let mut machine = if infuser {
        common::infuser()
    } else {
        common::chamber("clay")
    };
    machine.set_facing(Face::North);
    let default = if infuser {
        advanced_machine_default()
    } else {
        electric_machine_default()
    };
    machine.apply_item_side_config(
        Some(&default),
        Some(&machinery_basis_core::ejector::AttachedEjector::DEFAULT),
        false,
    );
    machine
}

#[test]
fn the_overlay_shows_the_data_type_of_the_face_in_a_configure_mode() {
    let chamber = machine(false);
    // A câmara olha para o norte: a frente é a face norte, e a configuração padrão põe a entrada de itens à esquerda (oeste).
    let info = chamber
        .side_config()
        .config(TransmissionType::Item)
        .unwrap();
    let mut seen = 0;
    for face in Face::ALL {
        let side =
            machinery_basis_core::side_config::RelativeSide::from_directions(Face::North, face);
        let shown = configurator_overlay(
            ConfiguratorMode::ConfigurateItems,
            chamber.side_config(),
            Face::North,
            face,
        );
        assert_eq!(
            shown,
            Some((TransmissionType::Item, info.data_type(side))),
            "{face:?}"
        );
        seen += 1;
    }
    assert_eq!(seen, 6);
}

#[test]
fn the_overlay_needs_a_configure_mode_and_a_type_the_machine_has() {
    let chamber = machine(false);
    for mode in [
        ConfiguratorMode::Empty,
        ConfiguratorMode::Rotate,
        ConfiguratorMode::Wrench,
    ] {
        assert_eq!(
            configurator_overlay(mode, chamber.side_config(), Face::North, Face::West),
            None
        );
    }
    // A câmara não tem químico, fluido nem calor.
    for mode in [
        ConfiguratorMode::ConfigurateFluids,
        ConfiguratorMode::ConfigurateChemicals,
        ConfiguratorMode::ConfigurateHeat,
    ] {
        assert_eq!(
            configurator_overlay(mode, chamber.side_config(), Face::North, Face::West),
            None,
            "{mode:?}"
        );
    }
    // O Infuser tem o químico.
    let infuser = machine(true);
    assert!(
        configurator_overlay(
            ConfiguratorMode::ConfigurateChemicals,
            infuser.side_config(),
            Face::North,
            Face::West
        )
        .is_some()
    );
}

#[test]
fn a_disabled_side_has_no_overlay() {
    use machinery_basis_core::side_config::{RelativeSide, TileComponentConfig};
    let mut config = TileComponentConfig::new(&[TransmissionType::Item]);
    config.add_disabled_sides(&[RelativeSide::Front]);
    config.setup_input_config(TransmissionType::Item, 0);
    // A face da frente (norte, com a máquina virada para o norte) é a desligada: sem sobreposição; as outras têm.
    assert_eq!(
        configurator_overlay(
            ConfiguratorMode::ConfigurateItems,
            &config,
            Face::North,
            Face::North
        ),
        None
    );
    assert!(
        configurator_overlay(
            ConfiguratorMode::ConfigurateItems,
            &config,
            Face::North,
            Face::South
        )
        .is_some()
    );
}

#[test]
fn the_overlay_color_is_the_data_type_color_at_sixty_percent() {
    // `MekanismRenderer.getColorARGB(color, 0.6F)`: `floor(0,6F * 255F)` = 153.
    for data_type in DataType::VALUES {
        let argb = overlay_argb(data_type);
        assert_eq!(argb >> 24, 153);
        assert_eq!(
            argb & 0x00FF_FFFF,
            mekanism_rules::enum_color_rgb(data_type.color())
        );
    }
    assert_eq!(overlay_argb(DataType::Input), 0x99_C9071F);
}

#[test]
fn the_overlay_sprite_is_the_transmission_overlay_texture() {
    let files = [
        (TransmissionType::Item, "items_overlay"),
        (TransmissionType::Fluid, "fluids_overlay"),
        (TransmissionType::Chemical, "chemicals_overlay"),
        (TransmissionType::Energy, "energy_overlay"),
        (TransmissionType::Heat, "heat_overlay"),
    ];
    for (kind, file) in files {
        assert_eq!(
            overlay_sprite(kind),
            format!("mekanism:gui/sprites/overlay/{file}")
        );
        // O sprite existe no conteúdo.
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("content/assets/mekanism/textures/gui/sprites/overlay")
            .join(format!("{file}.png"));
        assert!(path.exists(), "{}", path.display());
    }
}

#[test]
fn every_sprite_the_effects_draw_is_in_the_content() {
    let sprites =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("content/assets/mekanism/textures/gui/sprites");
    for mode in ConfiguratorMode::VALUES {
        let name = mode.radial_icon();
        let relative = name.strip_prefix("mekanism:gui/sprites/").unwrap();
        assert!(sprites.join(format!("{relative}.png")).exists(), "{name}");
    }
}

#[test]
fn the_config_defaults_are_the_originals_and_the_file_is_read() {
    assert_eq!(
        effects_config(&CLIENT_CONFIG.load("")),
        EffectsConfig::default()
    );
    let defaults = EffectsConfig::default();
    assert!(defaults.allow_mode_scroll && !defaults.white_radial_text);
    assert!(defaults.hud_enabled && !defaults.hud_reverse);
    assert_eq!(defaults.hud_scale, 0.6);
    let text = "[accessibility]\n\twhiteRadialText = true\n\tallowModeScroll = false\n[hud]\n\tenabled = false\n\tscale = 0.25\n\treverse = true\n";
    let read = effects_config(&CLIENT_CONFIG.load(text));
    assert_eq!(
        read,
        EffectsConfig {
            allow_mode_scroll: false,
            white_radial_text: true,
            hud_enabled: false,
            hud_scale: 0.25,
            hud_reverse: true
        }
    );
    // Fora da faixa: o limite, como o `defineInRange`.
    let tiny = effects_config(&CLIENT_CONFIG.load("[hud]\n\tscale = 0.1\n"));
    assert_eq!(tiny.hud_scale, 0.25);
}

#[test]
fn a_slot_under_the_configurator_names_the_data_type_that_owns_it() {
    use mekanism_rules::configurator_slot_tooltip;
    let chamber = machine(false);
    let info = chamber
        .side_config()
        .config(TransmissionType::Item)
        .unwrap();
    // O slot de entrada é do `INPUT` (vermelho escuro) e o de saída do `OUTPUT` (azul escuro): na ordem do `ordinal`, o primeiro tipo que tem o slot.
    let input = info.slot_info_of(DataType::Input).unwrap().containers[0];
    let output = info.slot_info_of(DataType::Output).unwrap().containers[0];
    let tooltip = configurator_slot_tooltip(chamber.side_config(), input).unwrap();
    assert_eq!(tooltip.translate, "generic.mekanism.with_parenthesis");
    assert_eq!(tooltip.with[0].translate, "side_data.mekanism.input");
    assert_eq!(tooltip.with[1].translate, "color.mekanism.dark_red");
    assert_eq!(
        tooltip.color,
        Some(mekanism_rules::enum_color_rgb(DataType::Input.color()))
    );
    assert_eq!(
        configurator_slot_tooltip(chamber.side_config(), output)
            .unwrap()
            .with[0]
            .translate,
        "side_data.mekanism.output"
    );
    // Um slot que nenhum tipo tem (o de upgrade) segue o tooltip normal.
    assert!(configurator_slot_tooltip(chamber.side_config(), 99).is_none());
}

#[test]
fn a_gauge_under_the_configurator_names_the_data_type_of_its_color() {
    use mekanism_rules::{configurator_gauge_tooltip, enum_color_rgb};
    let chamber = machine(false);
    // A configuração de energia da câmara só tem `INPUT`: o medidor de cor `INPUT` acha o tipo.
    let green = DataType::Input.color();
    let tooltip = configurator_gauge_tooltip(
        Some(chamber.side_config()),
        Some(TransmissionType::Energy),
        green,
    );
    assert_eq!(tooltip.len(), 1);
    assert_eq!(tooltip[0].translate, "generic.mekanism.with_parenthesis");
    assert_eq!(tooltip[0].with[0].translate, "side_data.mekanism.input");
    // Uma cor que nenhum tipo da máquina tem: só o nome da cor entre parênteses.
    let other = configurator_gauge_tooltip(
        Some(chamber.side_config()),
        Some(TransmissionType::Energy),
        DataType::Extra.color(),
    );
    assert_eq!(other[0].translate, "generic.mekanism.parenthesis");
    assert_eq!(
        other[0].color,
        Some(enum_color_rgb(DataType::Extra.color()))
    );
    // Sem a configuração da máquina, ou sem tipo, o tooltip fica vazio.
    assert!(configurator_gauge_tooltip(None, Some(TransmissionType::Energy), green).is_empty());
    assert!(configurator_gauge_tooltip(Some(chamber.side_config()), None, green).is_empty());
}
