//! O componente de segurança de um bloco (`TileComponentSecurity`, `SecurityUtils`, `BlockSecurityUtils`, `SecurityMode`): o dono, o modo, quem tem acesso, o NBT do tile,
//! os componentes do item (`mekanism:owner` e `mekanism:security`) e as duas linhas do tooltip (`Owner:` e `Security:`).
//!
//! A lib não conhece nenhum bloco: quem usa diz quais têm segurança. Cada conta é a do original e vem de um rastro
//! (`dev/referencia/rastros/security_<bloco>.jsonl`, `testes/SecurityTraceTest.java`).
//!
//! **Sem a Security Desk** (decisão 250): a frequência de segurança (`SecurityFrequency`) nasce com o dono (`setOwnerUUID` chama `tile.setFrequency(SECURITY, ...)`) e guarda a
//! lista dos jogadores de confiança, que só a Security Desk preenche. O port não tem a mesa, então a lista é sempre vazia: no servidor `TRUSTED` age como `PRIVATE` para
//! quem não é o dono (`frequency.isTrusted(player)` é falso), e no cliente ele "assume que pode" (`canAccessObject`, `isClient`). A sobreposição de segurança da mesa
//! (`SecurityData.override`) também não existe: o modo efetivo é o do próprio bloco.
pub use crate::recipe_data::SecurityMode;

/// `SerializationConstants.COMPONENT_SECURITY`: o filho do NBT do tile onde o `TileComponentSecurity` grava.
pub const COMPONENT_SECURITY: &str = "component_security";
/// `SerializationConstants.COMPONENT_FREQUENCY`: o filho do `TileComponentFrequency`, que guarda a identidade da frequência de segurança.
pub const COMPONENT_FREQUENCY: &str = "component_frequency";
/// A chave da frequência de segurança dentro do `component_frequency` (`FrequencyTypes.SECURITY`).
pub const FREQUENCY_SECURITY: &str = "security";
/// `SerializationConstants.OWNER_UUID` e `SECURITY_MODE`.
pub const OWNER_UUID: &str = "owner";
pub const SECURITY_MODE: &str = "security_mode";
/// Os componentes do item (`MekanismDataComponents.OWNER` e `SECURITY`).
pub const OWNER_COMPONENT: &str = "mekanism:owner";
pub const SECURITY_COMPONENT: &str = "mekanism:security";

/// `EnumColor` do tooltip: `GRAY`, `DARK_GRAY`, `BRIGHT_GREEN`, `RED` e `INDIGO` (`EnumColor.java`).
pub const COLOR_GRAY: u32 = 0xCFCFCF;
pub const COLOR_DARK_GRAY: u32 = 0x7A7A7A;
pub const COLOR_BRIGHT_GREEN: u32 = 0x75FF89;
pub const COLOR_RED: u32 = 0xFF383C;
pub const COLOR_INDIGO: u32 = 0x559EFF;

impl SecurityMode {
    /// `SecurityMode.VALUES`.
    pub const VALUES: [SecurityMode; 3] = [
        SecurityMode::Public,
        SecurityMode::Private,
        SecurityMode::Trusted,
    ];

    /// `IIncrementalEnum.getNext`: o próximo, voltando ao primeiro depois do último.
    pub fn next(self) -> Self {
        Self::VALUES[(self.ordinal() + 1) % Self::VALUES.len()]
    }

    /// `IIncrementalEnum.getPrevious`: o anterior, indo ao último antes do primeiro.
    pub fn previous(self) -> Self {
        Self::VALUES[(self.ordinal() + Self::VALUES.len() - 1) % Self::VALUES.len()]
    }

    pub fn ordinal(self) -> usize {
        match self {
            SecurityMode::Public => 0,
            SecurityMode::Private => 1,
            SecurityMode::Trusted => 2,
        }
    }

    /// `SecurityMode.BY_ID`: o índice fora da faixa dá `PUBLIC` (`OutOfBoundsStrategy.ZERO`).
    pub fn by_id(id: i32) -> Self {
        usize::try_from(id)
            .ok()
            .and_then(|id| Self::VALUES.get(id).copied())
            .unwrap_or(SecurityMode::Public)
    }

    /// `SecurityMode.getSerializedName`: o nome do enum em minúsculas.
    pub fn serialized_name(self) -> &'static str {
        match self {
            SecurityMode::Public => "public",
            SecurityMode::Private => "private",
            SecurityMode::Trusted => "trusted",
        }
    }

    /// `SecurityMode.CODEC`: o nome gravado, ou `None` se não é um dos três.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::VALUES
            .into_iter()
            .find(|mode| mode.serialized_name() == name)
    }

    /// `APILang.PUBLIC`, `PRIVATE` e `TRUSTED` (`security.mekanism.<modo>`).
    pub fn lang_key(self) -> &'static str {
        match self {
            SecurityMode::Public => "security.mekanism.public",
            SecurityMode::Private => "security.mekanism.private",
            SecurityMode::Trusted => "security.mekanism.trusted",
        }
    }

    /// A cor do modo (`SecurityMode(.., EnumColor.BRIGHT_GREEN | RED | INDIGO)`).
    pub fn color(self) -> u32 {
        match self {
            SecurityMode::Public => COLOR_BRIGHT_GREEN,
            SecurityMode::Private => COLOR_RED,
            SecurityMode::Trusted => COLOR_INDIGO,
        }
    }
}

/// O `UUID` como o `UUIDUtil.CODEC` o grava: quatro `int`, os 32 bits mais altos primeiro.
pub fn uuid_ints(uuid: u128) -> [i32; 4] {
    [
        (uuid >> 96) as i32,
        (uuid >> 64) as i32,
        (uuid >> 32) as i32,
        uuid as i32,
    ]
}

/// O contrário de [`uuid_ints`].
pub fn ints_uuid(ints: [i32; 4]) -> u128 {
    ints.iter()
        .fold(0u128, |uuid, part| (uuid << 32) | u128::from(*part as u32))
}

/// A configuração que a conta lê (`MekanismConfig.general.allowProtection` e `opsBypassRestrictions`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SecurityConfig {
    pub allow_protection: bool,
    pub ops_bypass: bool,
}

impl Default for SecurityConfig {
    /// Os padrões do original (`GeneralConfig.java:256-259`): a proteção ligada e os operadores sem atalho.
    fn default() -> Self {
        Self {
            allow_protection: true,
            ops_bypass: false,
        }
    }
}

/// O dono e o modo de um bloco (o `TileComponentSecurity`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SecurityState {
    pub owner: Option<u128>,
    pub mode: SecurityMode,
}

impl SecurityState {
    /// `canAccessObject(UUID, ISecurityObject, boolean isClient)`: sem dono, ou sendo o dono, sempre; senão pelo modo. `TRUSTED` sem jogador nega, no cliente concede, e no
    /// servidor pergunta à lista de confiança (`trusted`, vazia sem a Security Desk).
    pub fn can_access(
        &self,
        config: SecurityConfig,
        player: Option<u128>,
        is_client: bool,
        trusted: &dyn Fn(u128, u128) -> bool,
    ) -> bool {
        if !config.allow_protection {
            return true;
        }
        let Some(owner) = self.owner else { return true };
        if Some(owner) == player {
            return true;
        }
        match self.mode {
            SecurityMode::Public => true,
            SecurityMode::Private => false,
            SecurityMode::Trusted => match player {
                None => false,
                Some(_) if is_client => true,
                Some(player) => trusted(owner, player),
            },
        }
    }

    /// `SecurityUtils.canAccess(Player, ...)`: o operador com `opsBypassRestrictions` passa por cima de tudo (`isOp`, só no servidor).
    pub fn can_access_player(
        &self,
        config: SecurityConfig,
        player: u128,
        is_op: bool,
        is_client: bool,
        trusted: &dyn Fn(u128, u128) -> bool,
    ) -> bool {
        (config.ops_bypass && is_op && !is_client)
            || self.can_access(config, Some(player), is_client, trusted)
    }

    /// `getEffectiveSecurityMode`: o modo do bloco (sem a sobreposição da mesa), `PUBLIC` com a proteção desligada.
    pub fn effective_mode(&self, config: SecurityConfig) -> SecurityMode {
        if config.allow_protection {
            self.mode
        } else {
            SecurityMode::Public
        }
    }

    /// `ISecurityObject.ownerMatches`: o jogador é o dono.
    pub fn owner_matches(&self, player: u128) -> bool {
        self.owner == Some(player)
    }

    /// `incrementSecurityMode` e `decrementSecurityMode`: só o dono troca o modo.
    pub fn click(&mut self, player: u128, forward: bool) {
        if self.owner_matches(player) {
            self.mode = if forward {
                self.mode.next()
            } else {
                self.mode.previous()
            };
        }
    }

    /// `BlockMekanism.setPlacedBy`: um tile sem dono ganha o de quem o pôs (`getOwnerUUID() == null && placer != null`).
    pub fn placed_by(&mut self, placer: Option<u128>) -> bool {
        if self.owner.is_none()
            && let Some(placer) = placer
        {
            self.owner = Some(placer);
            return true;
        }
        false
    }

    /// `BlockMekanism.getDestroyProgress`: quem não acessa quebra com progresso zero (nenhuma mensagem).
    pub fn can_break(
        &self,
        config: SecurityConfig,
        player: u128,
        is_op: bool,
        trusted: &dyn Fn(u128, u128) -> bool,
    ) -> bool {
        self.can_access_player(config, player, is_op, false, trusted)
    }
}

/// `TileComponentSecurity.serialize`: o filho `component_security`, com o modo só se não é `PUBLIC` e o dono se existe. `None` se não há nada a gravar, e o
/// `component_frequency` ao lado (`TileComponentFrequency.serialize` da frequência de segurança, que `setOwnerUUID` cria).
pub fn serialize(state: &SecurityState) -> (Option<serde_json::Value>, Option<serde_json::Value>) {
    use serde_json::{Map, Value, json};
    let mut security = Map::new();
    if state.mode != SecurityMode::Public {
        security.insert(
            SECURITY_MODE.to_owned(),
            json!(state.mode.serialized_name()),
        );
    }
    let mut frequency = None;
    if let Some(owner) = state.owner {
        security.insert(OWNER_UUID.to_owned(), json!(uuid_ints(owner)));
        // `SecurityFrequency` guarda o dono e o modo de identidade, que é sempre `PUBLIC` (`FrequencyIdentity(ownerUUID, SecurityMode.PUBLIC, ownerUUID)`).
        frequency = Some(
            json!({ FREQUENCY_SECURITY: { OWNER_UUID: uuid_ints(owner), SECURITY_MODE: SecurityMode::Public.serialized_name() } }),
        );
    }
    (
        (!security.is_empty()).then_some(Value::Object(security)),
        frequency,
    )
}

/// `TileComponentSecurity.deserialize`: o modo (o nome desconhecido dá `PUBLIC`) e o dono (`ifPresent`).
pub fn deserialize(component: Option<&serde_json::Value>) -> SecurityState {
    let mode = component
        .and_then(|component| component.get(SECURITY_MODE))
        .and_then(serde_json::Value::as_str)
        .and_then(SecurityMode::from_name)
        .unwrap_or(SecurityMode::Public);
    let owner = component
        .and_then(|component| component.get(OWNER_UUID))
        .and_then(owner_of);
    SecurityState { owner, mode }
}

/// O dono de um valor `int[4]` (`UUIDUtil.CODEC`).
pub fn owner_of(value: &serde_json::Value) -> Option<u128> {
    let ints = value.as_array()?;
    if ints.len() != 4 {
        return None;
    }
    let mut out = [0i32; 4];
    for (slot, part) in out.iter_mut().zip(ints) {
        *slot = i32::try_from(part.as_i64()?).ok()?;
    }
    Some(ints_uuid(out))
}

/// `TileComponentSecurity.collectImplicitComponents`: o item leva o modo (o padrão `PUBLIC` não vira componente: o `DataComponentPatch` omite o que é igual ao do item) e
/// o dono, se existe.
pub fn collect(state: &SecurityState) -> (Option<serde_json::Value>, Option<serde_json::Value>) {
    use serde_json::json;
    let owner = state.owner.map(|owner| json!(uuid_ints(owner)));
    let mode = (state.mode != SecurityMode::Public).then(|| json!(state.mode.serialized_name()));
    (owner, mode)
}

/// `TileComponentSecurity.applyImplicitComponents`: o modo do item (`getOrDefault(SECURITY, securityMode)`: sem componente, o que o tile tinha) e o dono dele (`setOwnerUUID`
/// só se há componente). O dono novo substitui o do tile.
pub fn apply(
    tile: SecurityState,
    owner: Option<&serde_json::Value>,
    mode: Option<&serde_json::Value>,
) -> SecurityState {
    let mode = mode
        .and_then(serde_json::Value::as_str)
        .and_then(SecurityMode::from_name)
        .unwrap_or(tile.mode);
    let owner = owner.and_then(owner_of).or(tile.owner);
    SecurityState { owner, mode }
}

/// Uma linha do tooltip do item: a chave, a cor do rótulo, o argumento (texto literal ou chave de tradução) e a cor dele.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TooltipLine {
    pub key: &'static str,
    pub color: u32,
    pub arg: Arg,
    pub arg_color: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Arg {
    Literal(String),
    Translate(&'static str),
}

/// `OwnerDisplay.of(player, owner).getTextComponent()`: `Owner: <nome>`, o rótulo em `DARK_GRAY` e o nome em `BRIGHT_GREEN` se o espectador é o dono, em `RED` se não é.
pub fn owner_line(viewer: Option<u128>, owner: u128, name: &str) -> TooltipLine {
    TooltipLine {
        key: "owner.mekanism.format",
        color: COLOR_DARK_GRAY,
        arg: Arg::Literal(name.to_owned()),
        arg_color: if viewer == Some(owner) {
            COLOR_BRIGHT_GREEN
        } else {
            COLOR_RED
        },
    }
}

/// `MekanismLang.SECURITY.translateColored(GRAY, mode)`: `Security: <modo>`, o modo na cor dele.
pub fn security_line(mode: SecurityMode) -> TooltipLine {
    TooltipLine {
        key: "tooltip.mekanism.security",
        color: COLOR_GRAY,
        arg: Arg::Translate(mode.lang_key()),
        arg_color: mode.color(),
    }
}

/// As linhas que o item leva ao tooltip, na ordem do original (`Owner:` antes de `Security:`, as duas depois do nome): o `SECURITY` do item é sempre presente (o padrão
/// `PUBLIC` do item de bloco), o `OWNER` só se o item o leva. Com a proteção desligada o modo mostrado é `PUBLIC`.
pub fn tooltip_lines(
    config: SecurityConfig,
    viewer: Option<u128>,
    owner: Option<(u128, &str)>,
    mode: SecurityMode,
) -> Vec<TooltipLine> {
    let mut lines = Vec::new();
    if let Some((owner, name)) = owner {
        lines.push(owner_line(viewer, owner, name));
    }
    lines.push(security_line(if config.allow_protection {
        mode
    } else {
        SecurityMode::Public
    }));
    lines
}

/// `SecurityUtils.displayNoAccess`: `MekanismUtils.logFormat(RED, NO_ACCESS)`, o texto que o jogador recebe ao ser recusado.
pub const NO_ACCESS_KEY: &str = "security.mekanism.no_access";

#[cfg(test)]
mod tests {
    use super::*;

    fn nobody(_: u128, _: u128) -> bool {
        false
    }

    #[test]
    fn the_uuid_round_trips_through_four_ints() {
        let uuid = 0x7249_8bc4_6e0d_46ac_a5a2_3a1f_6275_4e52_u128;
        assert_eq!(ints_uuid(uuid_ints(uuid)), uuid);
        assert_eq!(
            uuid_ints(0x0000_0001_0000_0002_ffff_ffff_8000_0000)[..],
            [1, 2, -1, i32::MIN]
        );
    }

    #[test]
    fn the_mode_cycles_both_ways() {
        assert_eq!(SecurityMode::Public.next(), SecurityMode::Private);
        assert_eq!(SecurityMode::Trusted.next(), SecurityMode::Public);
        assert_eq!(SecurityMode::Public.previous(), SecurityMode::Trusted);
        assert_eq!(SecurityMode::by_id(7), SecurityMode::Public);
    }

    #[test]
    fn only_the_owner_gets_past_a_private_block() {
        let state = SecurityState {
            owner: Some(1),
            mode: SecurityMode::Private,
        };
        let config = SecurityConfig::default();
        assert!(state.can_access(config, Some(1), false, &nobody));
        assert!(!state.can_access(config, Some(2), false, &nobody));
        assert!(!state.can_access(config, None, false, &nobody));
    }

    #[test]
    fn a_trusted_block_trusts_the_client_and_nobody_on_the_server() {
        let state = SecurityState {
            owner: Some(1),
            mode: SecurityMode::Trusted,
        };
        let config = SecurityConfig::default();
        assert!(state.can_access(config, Some(2), true, &nobody));
        assert!(!state.can_access(config, Some(2), false, &nobody));
        assert!(!state.can_access(config, None, true, &nobody));
        assert!(
            state.can_access(config, Some(2), false, &|owner, player| owner == 1
                && player == 2)
        );
    }

    #[test]
    fn a_block_without_owner_or_with_protection_off_is_open() {
        let config = SecurityConfig::default();
        let ownerless = SecurityState {
            owner: None,
            mode: SecurityMode::Private,
        };
        assert!(ownerless.can_access(config, Some(2), false, &nobody));
        let private = SecurityState {
            owner: Some(1),
            mode: SecurityMode::Private,
        };
        let off = SecurityConfig {
            allow_protection: false,
            ..config
        };
        assert!(private.can_access(off, Some(2), false, &nobody));
        assert_eq!(private.effective_mode(off), SecurityMode::Public);
    }

    #[test]
    fn an_op_gets_past_only_with_the_bypass_on_the_server() {
        let private = SecurityState {
            owner: Some(1),
            mode: SecurityMode::Private,
        };
        let bypass = SecurityConfig {
            ops_bypass: true,
            ..SecurityConfig::default()
        };
        assert!(private.can_access_player(bypass, 2, true, false, &nobody));
        assert!(!private.can_access_player(bypass, 2, true, true, &nobody));
        assert!(!private.can_access_player(SecurityConfig::default(), 2, true, false, &nobody));
    }

    #[test]
    fn only_the_owner_clicks_the_tab() {
        let mut state = SecurityState {
            owner: Some(1),
            mode: SecurityMode::Public,
        };
        state.click(2, true);
        assert_eq!(state.mode, SecurityMode::Public);
        state.click(1, true);
        assert_eq!(state.mode, SecurityMode::Private);
        state.click(1, false);
        state.click(1, false);
        assert_eq!(state.mode, SecurityMode::Trusted);
    }

    #[test]
    fn the_placer_owns_a_tile_without_owner_only() {
        let mut state = SecurityState::default();
        assert!(!state.placed_by(None));
        assert!(state.placed_by(Some(5)));
        assert!(!state.placed_by(Some(6)));
        assert_eq!(state.owner, Some(5));
    }

    #[test]
    fn the_nbt_keeps_the_mode_only_when_it_is_not_public() {
        let public = SecurityState {
            owner: Some(0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10),
            mode: SecurityMode::Public,
        };
        let (security, frequency) = serialize(&public);
        assert!(security.as_ref().unwrap().get(SECURITY_MODE).is_none());
        assert_eq!(
            security.as_ref().unwrap()[OWNER_UUID],
            serde_json::json!([0x0102_0304, 0x0506_0708, 0x090a_0b0c, 0x0d0e_0f10])
        );
        assert_eq!(
            frequency.unwrap()[FREQUENCY_SECURITY][SECURITY_MODE],
            "public"
        );
        let private = SecurityState {
            mode: SecurityMode::Private,
            ..public
        };
        let (security, _) = serialize(&private);
        assert_eq!(deserialize(security.as_ref()), private);
        assert_eq!(serialize(&SecurityState::default()), (None, None));
    }

    #[test]
    fn the_item_carries_the_owner_and_a_non_default_mode() {
        let state = SecurityState {
            owner: Some(7),
            mode: SecurityMode::Trusted,
        };
        let (owner, mode) = collect(&state);
        assert_eq!(mode.as_ref().unwrap(), "trusted");
        let back = apply(SecurityState::default(), owner.as_ref(), mode.as_ref());
        assert_eq!(back, state);
        assert_eq!(
            collect(&SecurityState {
                owner: None,
                mode: SecurityMode::Public
            }),
            (None, None)
        );
        // Sem componente de modo o tile fica com o que tinha.
        let kept = apply(
            SecurityState {
                owner: Some(3),
                mode: SecurityMode::Private,
            },
            None,
            None,
        );
        assert_eq!(
            kept,
            SecurityState {
                owner: Some(3),
                mode: SecurityMode::Private
            }
        );
    }

    #[test]
    fn the_tooltip_puts_the_owner_before_the_mode_and_colors_by_viewer() {
        let lines = tooltip_lines(
            SecurityConfig::default(),
            Some(1),
            Some((1, "Ana")),
            SecurityMode::Private,
        );
        assert_eq!(lines[0].key, "owner.mekanism.format");
        assert_eq!(lines[0].arg_color, COLOR_BRIGHT_GREEN);
        assert_eq!(lines[1].arg, Arg::Translate("security.mekanism.private"));
        assert_eq!(lines[1].arg_color, COLOR_RED);
        let other = tooltip_lines(
            SecurityConfig::default(),
            Some(2),
            Some((1, "Ana")),
            SecurityMode::Trusted,
        );
        assert_eq!(other[0].arg_color, COLOR_RED);
        assert_eq!(other[1].arg_color, COLOR_INDIGO);
        assert_eq!(
            tooltip_lines(SecurityConfig::default(), None, None, SecurityMode::Public).len(),
            1
        );
    }
}
