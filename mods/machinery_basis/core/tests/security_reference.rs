//! O componente de segurança contra o original: `dev/referencia/rastros/security_<bloco>.jsonl` (`SecurityTraceTest.java`) traz, para o Crusher, o Heat Generator e o Basic
//! Energy Cube postos por um jogador, uma linha por modo (`PUBLIC`, `PRIVATE`, `TRUSTED`) com o NBT do tile, quem acessa (o dono, outro jogador e o UUID nulo, no servidor e
//! no cliente), o progresso de quebra, o resultado de `openGui`, a mensagem de acesso negado, o item solto com o tooltip visto por cada um, o dono do tile posto de volta
//! e o clique da aba. A sem Security Desk (decisão 250): a lista de confiança é vazia, e o rastro é gravado sem ela.
use machinery_basis_core::security::{
    self as sec, Arg, SecurityConfig, SecurityMode, SecurityState,
};
use serde_json::Value;
use std::path::PathBuf;

const BLOCKS: [&str; 3] = ["crusher", "heat_generator", "basic_energy_cube"];

fn rows(block: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../../dev/referencia/rastros/security_{block}.jsonl"
    ));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let rows: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).expect("linha JSON válida"))
        .collect();
    assert_eq!(rows.len(), 3, "{block}: uma linha por modo");
    rows
}

fn uuid(text: &Value) -> u128 {
    let text = text.as_str().expect("uuid").replace('-', "");
    u128::from_str_radix(&text, 16).expect("uuid em hexadecimal")
}

fn mode_of(row: &Value) -> SecurityMode {
    SecurityMode::from_name(&row["mode"].as_str().expect("modo").to_ascii_lowercase())
        .expect("modo conhecido")
}

fn color_of(value: &Value) -> u32 {
    let text = value.as_str().expect("cor");
    u32::from_str_radix(text.trim_start_matches('#'), 16).expect("cor em hexadecimal")
}

fn nobody(_: u128, _: u128) -> bool {
    false
}

fn state_of(row: &Value) -> SecurityState {
    SecurityState {
        owner: Some(uuid(&row["owner"])),
        mode: mode_of(row),
    }
}

fn each(mut check: impl FnMut(&str, &Value)) {
    for block in BLOCKS {
        for row in rows(block) {
            check(block, &row);
        }
    }
}

#[test]
fn the_three_modes_are_recorded_in_order() {
    for block in BLOCKS {
        let modes: Vec<_> = rows(block).iter().map(mode_of).collect();
        assert_eq!(modes, SecurityMode::VALUES.to_vec(), "{block}");
    }
}

#[test]
fn the_saved_tile_is_what_the_original_wrote() {
    each(|block, row| {
        let nbt = &row["nbt"];
        let state = sec::deserialize(nbt.get(sec::COMPONENT_SECURITY));
        assert_eq!(state, state_of(row), "{block} {}", row["mode"]);
        let (security, frequency) = sec::serialize(&state);
        assert_eq!(
            security.as_ref(),
            nbt.get(sec::COMPONENT_SECURITY),
            "{block} {}",
            row["mode"]
        );
        assert_eq!(
            frequency.as_ref(),
            nbt.get(sec::COMPONENT_FREQUENCY),
            "{block} {}",
            row["mode"]
        );
    });
}

#[test]
fn who_can_access_is_the_original_can_access_object() {
    let config = SecurityConfig::default();
    each(|block, row| {
        let state = state_of(row);
        let owner = uuid(&row["owner"]);
        let other = uuid(&row["other"]);
        let access = &row["access"];
        for (side, client) in [("server", false), ("client", true)] {
            let tag = format!("{block} {} {side}", row["mode"]);
            assert_eq!(
                state.can_access(config, Some(owner), client, &nobody),
                access[format!("owner_{side}")] == true,
                "{tag} dono"
            );
            assert_eq!(
                state.can_access(config, Some(other), client, &nobody),
                access[format!("other_{side}")] == true,
                "{tag} outro"
            );
            assert_eq!(
                state.can_access(config, None, client, &nobody),
                access[format!("null_{side}")] == true,
                "{tag} sem jogador"
            );
            assert_eq!(
                state
                    .effective_mode(config)
                    .serialized_name()
                    .to_ascii_uppercase(),
                access[format!("mode_{side}")].as_str().expect("modo"),
                "{tag} modo efetivo"
            );
        }
        assert_eq!(
            state.can_access_player(config, owner, false, false, &nobody),
            access["owner_block"] == true,
            "{block} {} dono no bloco",
            row["mode"]
        );
        assert_eq!(
            state.can_access_player(config, other, false, false, &nobody),
            access["other_block"] == true,
            "{block} {} outro no bloco",
            row["mode"]
        );
    });
}

#[test]
fn breaking_is_refused_to_who_cannot_access() {
    let config = SecurityConfig::default();
    each(|block, row| {
        let state = state_of(row);
        let tag = format!("{block} {}", row["mode"]);
        for (who, key) in [("owner", "owner_break"), ("other", "other_break")] {
            let player = uuid(&row[who]);
            let progress = row["use"][key].as_f64().expect("progresso");
            assert_eq!(
                state.can_break(config, player, false, &nobody),
                progress > 0.0,
                "{tag} {who}: progresso {progress}"
            );
        }
    });
}

#[test]
fn opening_is_refused_to_who_cannot_access() {
    let config = SecurityConfig::default();
    each(|block, row| {
        let state = state_of(row);
        let tag = format!("{block} {}", row["mode"]);
        for (who, key) in [("owner", "owner_open"), ("other", "other_open")] {
            let player = uuid(&row[who]);
            let result = row["use"][key].as_str().expect("resultado");
            let allowed = state.can_access_player(config, player, false, false, &nobody);
            // `InteractionResult.Success[...]` (a tela abre) ou `Fail[]` (`canAccessOrDisplayError` recusou).
            assert_eq!(
                result.starts_with("Success"),
                allowed,
                "{tag} {who}: {result}"
            );
            assert_eq!(
                result.starts_with("Fail"),
                !allowed,
                "{tag} {who}: {result}"
            );
        }
    });
}

#[test]
fn the_no_access_message_is_the_red_translation() {
    each(|block, row| {
        let component = &row["use"]["no_access"];
        let text = component.to_string();
        assert!(
            text.contains(sec::NO_ACCESS_KEY),
            "{block}: {text} leva a chave {}",
            sec::NO_ACCESS_KEY
        );
        assert!(
            text.contains(&format!("{:06X}", sec::COLOR_RED)),
            "{block}: {text} é vermelho"
        );
    });
}

#[test]
fn the_dropped_item_carries_the_owner_and_the_mode() {
    each(|block, row| {
        let state = state_of(row);
        let (owner, mode) = sec::collect(&state);
        let components = &row["drops"][0]["stack"]["components"];
        assert_eq!(
            owner.as_ref(),
            components.get(sec::OWNER_COMPONENT),
            "{block} {}",
            row["mode"]
        );
        assert_eq!(
            mode.as_ref(),
            components.get(sec::SECURITY_COMPONENT),
            "{block} {}",
            row["mode"]
        );
    });
}

#[test]
fn the_tooltip_follows_who_looks() {
    let config = SecurityConfig::default();
    each(|block, row| {
        let state = state_of(row);
        let owner = uuid(&row["owner"]);
        let name = row["owner_name"].as_str().expect("nome");
        for (viewer, key) in [
            (owner, "tooltip_owner"),
            (uuid(&row["other"]), "tooltip_other"),
        ] {
            let tooltip = row["drops"][0][key].as_array().expect("tooltip");
            let lines = sec::tooltip_lines(config, Some(viewer), Some((owner, name)), state.mode);
            assert_eq!(lines.len(), 2);
            for (line, original) in lines.iter().zip(&tooltip[1..3]) {
                let tag = format!("{block} {} {key} {}", row["mode"], line.key);
                let component = &original["component"];
                assert_eq!(component["translate"], line.key, "{tag}");
                assert_eq!(color_of(&component["color"]), line.color, "{tag}");
                let arg = &component["with"][0];
                assert_eq!(color_of(&arg["color"]), line.arg_color, "{tag}");
                match &line.arg {
                    Arg::Literal(literal) => assert_eq!(arg["text"], literal.as_str(), "{tag}"),
                    Arg::Translate(translation) => {
                        assert_eq!(arg["translate"], *translation, "{tag}")
                    }
                }
            }
        }
    });
}

#[test]
fn the_item_placed_back_gives_the_owner_of_the_item_or_of_who_places() {
    each(|block, row| {
        let tag = format!("{block} {}", row["mode"]);
        let components = &row["drops"][0]["stack"]["components"];
        let other = uuid(&row["other"]);
        let owner = uuid(&row["owner"]);
        let place = |item: bool, placer: u128| {
            let mut state = if item {
                sec::apply(
                    SecurityState::default(),
                    components.get(sec::OWNER_COMPONENT),
                    components.get(sec::SECURITY_COMPONENT),
                )
            } else {
                SecurityState::default()
            };
            state.placed_by(Some(placer));
            state
        };
        for (key, item, placer) in [
            ("replaced_by_other", true, other),
            ("fresh_by_other", false, other),
            ("replaced_by_owner", true, owner),
        ] {
            let placed = &row[key];
            let state = place(item, placer);
            assert_eq!(
                sec::deserialize(placed["nbt"].get(sec::COMPONENT_SECURITY)),
                state,
                "{tag} {key} (nbt)"
            );
            assert_eq!(
                uuid(&placed["owner"]),
                state.owner.expect("dono"),
                "{tag} {key} (dono)"
            );
            assert_eq!(
                placed["mode"].as_str().expect("modo"),
                state.mode.serialized_name().to_ascii_uppercase(),
                "{tag} {key} (modo)"
            );
        }
    });
}

#[test]
fn the_tab_click_changes_the_mode_only_for_the_owner() {
    each(|block, row| {
        let start = state_of(row);
        let owner = uuid(&row["owner"]);
        let other = uuid(&row["other"]);
        let clicks = &row["clicks"];
        let tag = format!("{block} {}", row["mode"]);
        let after = |player: u128, forward: bool| {
            let mut state = start;
            state.click(player, forward);
            state.mode.serialized_name().to_ascii_uppercase()
        };
        assert_eq!(
            after(owner, true),
            clicks["owner_next"].as_str().unwrap(),
            "{tag}"
        );
        assert_eq!(
            after(owner, false),
            clicks["owner_previous"].as_str().unwrap(),
            "{tag}"
        );
        assert_eq!(
            after(other, true),
            clicks["other_next"].as_str().unwrap(),
            "{tag}"
        );
        assert_eq!(
            after(other, false),
            clicks["other_previous"].as_str().unwrap(),
            "{tag}"
        );
        assert_eq!(
            start.owner_matches(owner),
            clicks["owner_matches"] == true,
            "{tag}"
        );
        assert_eq!(
            start.owner_matches(other),
            clicks["other_matches"] == true,
            "{tag}"
        );
        // O pacote da aba só sai do dono, com o botão esquerdo ou o direito.
        assert_eq!(sec::tab_click(&start, owner, 0), Some(true), "{tag}");
        assert_eq!(sec::tab_click(&start, owner, 1), Some(false), "{tag}");
        assert_eq!(sec::tab_click(&start, owner, 2), None, "{tag}");
        assert_eq!(sec::tab_click(&start, other, 0), None, "{tag}");
    });
}

#[test]
fn the_uuid_parser_matches_the_codec_ints() {
    // O `UUIDUtil.CODEC` grava os quatro `int` do UUID: a conversão do rastro e a do port dão o mesmo.
    each(|block, row| {
        let owner = uuid(&row["owner"]);
        let text = row["owner"].as_str().unwrap();
        assert_eq!(
            sec::ints_uuid(sec::uuid_ints(owner)),
            owner,
            "{block} {text}"
        );
        assert_eq!(
            sec::owner_of(&serde_json::json!(sec::uuid_ints(owner))),
            Some(owner),
            "{block}"
        );
    });
}

#[test]
fn the_menu_data_is_the_original_frequency_and_mode() {
    each(|block, row| {
        let menu = &row["menu"];
        let owner = uuid(&row["owner"]);
        let name = menu["owner_name"].as_str().expect("nome do dono");
        let (hash, bytes) = sec::frequency_sync(Some(owner), name);
        let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        let tag = format!("{block} {}", row["mode"]);
        assert_eq!(hex, menu["bytes"].as_str().expect("bytes"), "{tag} bytes");
        assert_eq!(
            i64::from(hash),
            menu["hash"].as_i64().expect("hash"),
            "{tag} hash"
        );
        assert_eq!(
            mode_of(row).ordinal() as i64,
            menu["mode"].as_i64().expect("modo"),
            "{tag} modo"
        );
        assert_eq!(
            sec::frequency_owner(&bytes),
            Some((owner, name.to_owned())),
            "{tag}"
        );
    });
}

#[test]
fn a_tile_without_owner_syncs_a_null_frequency() {
    assert_eq!(sec::frequency_sync(None, ""), (0, vec![0]));
    assert_eq!(sec::frequency_owner(&[0]), None);
}

/// `CommonWorldTickHandler.onBlockBreak`: o `BreakBlockEvent` é cancelado para quem não acessa, e `ServerPlayerGameMode.handleBlockBreakAction` o dispara também em criativo
/// (`destroyAndAck`), então quem não acessa não quebra em nenhum modo de jogo (a decisão 251 supunha o contrário, pelo vanilla).
#[test]
fn creative_breaking_is_refused_to_who_cannot_access_like_survival() {
    let config = SecurityConfig::default();
    each(|block, row| {
        let state = state_of(row);
        for who in ["owner", "other"] {
            let breaks = row["creative"][format!("{who}_creative_breaks")]
                .as_bool()
                .unwrap_or_else(|| panic!("{block} {}: sem {who}_creative_breaks", row["mode"]));
            assert_eq!(
                breaks,
                state.can_break(config, uuid(&row[who]), false, &nobody),
                "{block} {} {who}",
                row["mode"]
            );
        }
    });
}
