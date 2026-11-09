//! Repete o rastro `config_lados_nbt` do original: o que a configuração de lados e o ejetor gravam no tile e no item, e o que o tile
//! relido e o item reposto recuperam. O formato do componente do item (o codec) é lido do JSON do rastro.
mod common;
use common::side::{
    born, config_nbt_from, default_of, ejector_from, ejector_nbt_from, load_state,
    side_config_from, state_of,
};
use machinery_basis_core::ejector::AttachedEjector;
use serde_json::{Value, json};
use std::path::Path;

fn trace() -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../dev/referencia/rastros/config_lados_nbt.jsonl");
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|line| line.get("meta").is_none())
        .collect()
}

#[test]
fn the_saved_item_and_reloaded_forms_match_the_original() {
    let (mut tiles, mut drops, mut placed) = (0, 0, 0);
    for line in trace() {
        let name = line["machine"].as_str().unwrap().to_owned();
        if name == "heat_generator" {
            continue;
        }
        let label = format!("{name} {}", line["variant"]);
        let state = &line["state"];
        // O tile relido do NBT que o Java gravou tem o estado que o Java tinha.
        let nbt = &line["tile_nbt"];
        let mut loaded = born(&name);
        let config = config_nbt_from(&nbt["component_config"]["json"]);
        let ejector = nbt["component_ejector"]["present"]
            .as_bool()
            .unwrap()
            .then(|| ejector_nbt_from(&nbt["component_ejector"]["json"]));
        loaded.restore_side_config(Some(&config), ejector.as_ref(), false);
        let mut expected = state.clone();
        expected["control_type"] = json!("disabled");
        assert_eq!(
            state_of(&loaded)["config"],
            state["config"],
            "{label}: lados lidos do NBT"
        );
        assert_eq!(
            state_of(&loaded)["ejecting"],
            state["ejecting"],
            "{label}: ejetar lido do NBT"
        );
        assert_eq!(
            state_of(&loaded)["ejector"],
            state["ejector"],
            "{label}: ejetor lido do NBT"
        );
        assert_eq!(
            state_of(&loaded)["config"],
            line["loaded_tile"]["state"]["config"],
            "{label}: tile relido"
        );
        // O que o objeto grava é o que o Java gravou.
        let mut machine = born(&name);
        load_state(&mut machine, state);
        assert_eq!(
            machine.side_config().serialize(),
            config,
            "{label}: component_config"
        );
        let written = machine.ejector().serialize();
        match &ejector {
            Some(data) => assert_eq!(&written, data, "{label}: component_ejector"),
            None => assert!(
                written.is_empty(),
                "{label}: o ejetor no padrão não deixa filho"
            ),
        }
        tiles += 1;
        // O item que o tile entrega (`collectImplicitComponents`).
        let (side_config, ejector_item) = machine.collect_item_side_config();
        let components = &line["item_components"];
        assert_eq!(
            side_config,
            side_config_from(&components["side_config"]),
            "{label}: side_config do item"
        );
        assert_eq!(
            ejector_item,
            ejector_from(&components["ejector"]),
            "{label}: ejector do item"
        );
        // O que cai ao quebrar: os componentes que diferem do padrão do item.
        let drop = &line["broken"]["drops"][0];
        let patch: Vec<&str> = drop["patch"]
            .as_array()
            .unwrap()
            .iter()
            .map(|key| key.as_str().unwrap())
            .collect();
        assert_eq!(
            side_config != default_of(&name),
            patch.contains(&"mekanism:side_config"),
            "{label}: retalho side_config"
        );
        assert_eq!(
            ejector_item != AttachedEjector::DEFAULT,
            patch.contains(&"mekanism:ejector"),
            "{label}: retalho ejector"
        );
        drops += 1;
        // O item reposto traz os componentes e a máquina nasce com o estado do rastro.
        let mut again = born(&name);
        again.apply_item_side_config(
            Some(&side_config_from(&drop["side_config"])),
            Some(&ejector_from(&drop["ejector"])),
            false,
        );
        let mut wanted = line["placed_from_item"]["state"].clone();
        wanted["control_type"] = json!("disabled");
        assert_eq!(
            state_of(&again)["config"],
            wanted["config"],
            "{label}: lados do item reposto"
        );
        assert_eq!(
            state_of(&again)["ejecting"],
            wanted["ejecting"],
            "{label}: ejetar do item reposto"
        );
        assert_eq!(
            state_of(&again)["ejector"],
            wanted["ejector"],
            "{label}: ejetor do item reposto"
        );
        placed += 1;
    }
    assert!(
        tiles >= 24 && drops >= 24 && placed >= 24,
        "{tiles} {drops} {placed}"
    );
}
