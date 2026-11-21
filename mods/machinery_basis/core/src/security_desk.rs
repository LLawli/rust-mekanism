//! A Security Desk (`TileEntitySecurityDesk`, `SecurityInventorySlot`, `GuiSecurityDesk`, `PacketAddTrusted` e os três `GuiInteraction` dela), sem o jogo: o que
//! cada ação faz à frequência do dono e o que ela provoca, os dois slots e o estado da tela.
//!
//! A mesa é um objeto com dono e sem modo de segurança (`ownerCapability`, sem `securityCapability`): só o dono a abre e a quebra, em qualquer modo.
//!
//! Conferido contra `dev/referencia/rastros/security_desk.jsonl` (`testes/SecurityDeskTraceTest.java`).

use crate::containers::AutomationType;
use crate::recipe_data::{SecurityMode, more_restrictive};
use crate::security_frequency::SecurityFrequency;

/// O que uma ação da mesa provoca além de mudar a frequência.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeskEffect {
    /// `markForSave()` do tile da mesa.
    MarkForSave,
    /// `PacketDistributor.sendToAllPlayers(new PacketSyncSecurity(frequency))`.
    SyncSecurity,
    /// `validateAccess()`: todo jogador com um menu de segurança aberto a que deixou de ter acesso é posto para fora.
    ValidateAccess,
    /// `closeIfNoAccess(jogador tirado da lista)`.
    CloseIfNoAccess(u128),
}

/// `TileEntitySecurityDesk.toggleOverride`.
pub fn toggle_override(frequency: &mut SecurityFrequency) -> Vec<DeskEffect> {
    frequency.set_overridden(!frequency.is_overridden());
    vec![
        DeskEffect::MarkForSave,
        DeskEffect::SyncSecurity,
        DeskEffect::ValidateAccess,
    ]
}

/// `TileEntitySecurityDesk.removeTrusted(index)`: o tile é marcado mesmo quando o índice não existe.
pub fn remove_trusted(frequency: &mut SecurityFrequency, index: i32) -> Vec<DeskEffect> {
    let mut effects = vec![DeskEffect::MarkForSave];
    if let Some(removed) = frequency.remove_trusted(index) {
        effects.push(DeskEffect::CloseIfNoAccess(removed));
    }
    effects
}

/// `TileEntitySecurityDesk.setSecurityDeskMode(mode)`: nada se o modo é o mesmo; a validação de acesso só se o novo é mais restritivo que o antigo.
pub fn set_mode(frequency: &mut SecurityFrequency, mode: SecurityMode) -> Vec<DeskEffect> {
    let old = frequency.security();
    if old == mode {
        return Vec::new();
    }
    frequency.set_security_mode(mode);
    let mut effects = vec![DeskEffect::MarkForSave, DeskEffect::SyncSecurity];
    if more_restrictive(old, mode) {
        effects.push(DeskEffect::ValidateAccess);
    }
    effects
}

/// `TileEntitySecurityDesk.addTrusted(name)`: `resolved` é o que `nameToIdCache.get(name)` devolveu. Nenhum pacote, nenhum `markForSave`.
pub fn add_trusted(frequency: &mut SecurityFrequency, resolved: Option<(u128, &str)>) -> bool {
    resolved.is_some_and(|(uuid, name)| frequency.add_trusted(uuid, name))
}

/// `SharedConstants.MAX_PLAYER_NAME_LENGTH`: o limite da caixa de texto.
pub const MAX_NAME_LENGTH: usize = 16;

/// `InputValidator.USERNAME`: letra ASCII, dígito ou `_`. É o que a caixa de texto deixa digitar.
pub fn username_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `PacketAddTrusted.handle`: o nome não é vazio e só tem caracteres de `USERNAME` (o tamanho não é conferido no servidor).
pub fn server_accepts_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(username_char)
}

/// `GuiSecurityDesk.addTrusted`: o cliente manda o pacote se `StringUtil.isValidPlayerName(texto.trim())` (até 16 unidades, nenhuma menor ou igual a espaço nem maior
/// ou igual a 127): o nome vazio passa aqui e é o servidor que o recusa.
pub fn client_sends_name(text: &str) -> Option<&str> {
    let name = text.trim();
    (name.encode_utf16().count() <= MAX_NAME_LENGTH
        && name.encode_utf16().all(|unit| unit > 32 && unit < 127))
    .then_some(name)
}

/// `UUIDUtil.createOfflinePlayerUUID(name)`: `UUID.nameUUIDFromBytes("OfflinePlayer:" + name)` (MD5, versão 3).
pub fn offline_uuid(name: &str) -> u128 {
    let mut digest = md5(format!("OfflinePlayer:{name}").as_bytes());
    digest[6] = (digest[6] & 0x0F) | 0x30;
    digest[8] = (digest[8] & 0x3F) | 0x80;
    u128::from_be_bytes(digest)
}

/// MD5 (RFC 1321), para o UUID de nome do Java.
fn md5(input: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    let k: Vec<u32> = (0..64)
        .map(|i| (f64::from(i + 1).sin().abs() * 4_294_967_296.0) as u32)
        .collect();
    let mut state: [u32; 4] = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
    let mut message = input.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((input.len() as u64).wrapping_mul(8)).to_le_bytes());
    for chunk in message.as_chunks::<64>().0 {
        let words: Vec<u32> = chunk
            .as_chunks::<4>()
            .0
            .iter()
            .map(|word| u32::from_le_bytes(*word))
            .collect();
        let [mut a, mut b, mut c, mut d] = state;
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let rotated = a
                .wrapping_add(f)
                .wrapping_add(k[i])
                .wrapping_add(words[g])
                .rotate_left(S[i]);
            (a, d, c, b) = (d, c, b, b.wrapping_add(rotated));
        }
        state = [
            state[0].wrapping_add(a),
            state[1].wrapping_add(b),
            state[2].wrapping_add(c),
            state[3].wrapping_add(d),
        ];
    }
    let mut out = [0u8; 16];
    for (slot, word) in out.as_chunks_mut::<4>().0.iter_mut().zip(state) {
        *slot = word.to_le_bytes();
    }
    out
}

/// `CachedUserNameToIdResolver.get(name)` do servidor integrado, com o que o port sabe: quem o servidor já viu (`known`, que compara sem diferenciar maiúsculas e
/// devolve o nome como foi visto) e, para quem nunca viu, o perfil offline do nome **em minúsculas** (`createUnknownProfile(userName)`, com `resolveOfflineUsers`
/// ligado, o padrão do servidor integrado). A consulta à Mojang que o original faz antes disso (`profileRepository.findProfileByName`) não existe no port
/// (decisão 270).
pub fn resolve_name(name: &str, known: &dyn Fn(&str) -> Option<(u128, String)>) -> (u128, String) {
    let lower = name.to_lowercase();
    known(&lower).unwrap_or_else(|| (offline_uuid(&lower), lower))
}

/// O que um slot da mesa vê de um item: se ele tem dono (`ownerCapability`, todo item com o componente de dono ou de bloco com segurança), o dono, e o modo que o
/// componente `security` traz (ausente: o padrão do item, `PUBLIC`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemSecurity {
    /// `SecurityInventorySlot.VALIDATOR`: o item tem a capability de dono.
    pub ownable: bool,
    /// O item tem a capability de segurança (um modo).
    pub securable: bool,
    pub owner: Option<u128>,
    pub mode: Option<SecurityMode>,
}

/// O que `lock` ou `unlock` fez ao item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemChange {
    pub owner: Option<u128>,
    /// `Some(None)`: o componente `security` foi tirado (`resource.without(SECURITY)`, que vira a marca `!mekanism:security` no item); `Some(Some(modo))`: posto;
    /// `None`: intocado.
    pub mode: Option<Option<SecurityMode>>,
}

/// Qual dos dois slots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeskSlot {
    /// O de cima, (146, 18): destrava.
    Unlock,
    /// O de baixo, (146, 97): trava.
    Lock,
}

/// As posições dos dois slots no menu (`SecurityInventorySlot.unlock(..., 146, 18)` e `lock(..., 146, 97)`).
pub const SLOT_POSITIONS: [(i32, i32); 2] = [(146, 18), (146, 97)];

/// `canInsert` do slot com o `VALIDATOR`: o item tem de ter dono possível; por dentro (`INTERNAL`) entra sempre; no de destravar entra o item do dono da mesa, no de
/// travar o item sem dono.
pub fn can_insert(
    slot: DeskSlot,
    item: &ItemSecurity,
    automation: AutomationType,
    desk_owner: Option<u128>,
) -> bool {
    if !item.ownable {
        return false;
    }
    if automation == AutomationType::Internal {
        return true;
    }
    match slot {
        DeskSlot::Unlock => item.owner.is_some() && item.owner == desk_owner,
        DeskSlot::Lock => item.owner.is_none(),
    }
}

/// `canExtract`: só a automação de fora (`EXTERNAL`) é barrada, e ela só tira o item pronto: sem dono no de destravar, com dono no de travar.
pub fn can_extract(slot: DeskSlot, item: &ItemSecurity, automation: AutomationType) -> bool {
    if automation != AutomationType::External {
        return true;
    }
    match slot {
        DeskSlot::Unlock => item.owner.is_none(),
        DeskSlot::Lock => item.owner.is_some(),
    }
}

/// `SecurityObject.setSecurityMode` do item: nada se o modo já é esse; `PUBLIC` tira o componente, os outros o põem.
fn set_item_mode(item: &ItemSecurity, mode: SecurityMode) -> Option<Option<SecurityMode>> {
    if !item.securable || item.mode.unwrap_or(SecurityMode::Public) == mode {
        return None;
    }
    Some((mode != SecurityMode::Public).then_some(mode))
}

/// `SecurityInventorySlot.unlock(ownerUUID)`: o item do dono da mesa perde o dono e volta a `PUBLIC`. `None` se nada muda.
pub fn unlock(item: &ItemSecurity, desk_owner: u128) -> Option<ItemChange> {
    if !item.ownable || item.owner != Some(desk_owner) {
        return None;
    }
    Some(ItemChange {
        owner: None,
        mode: set_item_mode(item, SecurityMode::Public),
    })
}

/// `SecurityInventorySlot.lock(ownerUUID, frequency)`: o item sem dono ganha o dono da mesa, e o item do dono da mesa ganha o modo da frequência. `None` se nada
/// muda.
pub fn lock(
    item: &ItemSecurity,
    desk_owner: u128,
    frequency_mode: SecurityMode,
) -> Option<ItemChange> {
    if !item.ownable {
        return None;
    }
    let owner = item.owner.unwrap_or(desk_owner);
    let mode = if owner == desk_owner {
        set_item_mode(item, frequency_mode)
    } else {
        None
    };
    (item.owner.is_none() || mode.is_some()).then_some(ItemChange {
        owner: Some(owner),
        mode,
    })
}

/// `GuiSecurityLight`: a luz ao lado do botão de sobreposição.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecurityLight {
    NotApplicable,
    Enabled,
    Disabled,
}

/// O que a tela da mesa lê do tile do cliente: o dono (do `update tag` do tile), a frequência que o menu sincronizou e quem olha.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeskView<'a> {
    pub owner: Option<u128>,
    pub frequency: Option<&'a SecurityFrequency>,
    pub viewer: Option<u128>,
}

/// O estado dos botões (`GuiSecurityDesk.updateButtons`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeskButtons {
    pub public: bool,
    pub private: bool,
    pub trusted: bool,
    pub override_button: bool,
}

impl DeskView<'_> {
    /// `isOwner(frequency)`: há frequência e quem olha é o dono do tile.
    pub fn is_owner(&self) -> bool {
        self.frequency.is_some() && self.owner.is_some() && self.owner == self.viewer
    }

    /// `updateButtons`: cada botão de modo liga para o dono se a frequência não está nesse modo; o de sobreposição liga para o dono.
    pub fn buttons(&self) -> DeskButtons {
        let owner = self.is_owner();
        let mode = self.frequency.map(SecurityFrequency::security);
        DeskButtons {
            public: owner && mode != Some(SecurityMode::Public),
            private: owner && mode != Some(SecurityMode::Private),
            trusted: owner && mode != Some(SecurityMode::Trusted),
            override_button: owner,
        }
    }

    /// A lista que `updateButtons` põe no `GuiTextScrollList` (só mexe nela se o tile tem dono: `None` deixa a lista como estava).
    pub fn names(&self) -> Option<&[String]> {
        self.owner?;
        Some(
            self.frequency
                .map_or(&[][..], SecurityFrequency::trusted_names),
        )
    }

    /// O `GuiSecurityLight` em (144, 77).
    pub fn light(&self) -> SecurityLight {
        match self.frequency {
            Some(frequency) if self.is_owner() => {
                if frequency.is_overridden() {
                    SecurityLight::Enabled
                } else {
                    SecurityLight::Disabled
                }
            }
            _ => SecurityLight::NotApplicable,
        }
    }

    /// O estado do `TooltipToggleButton` de sobreposição (o sprite e o tooltip `Security Override: On/Off`).
    pub fn override_on(&self) -> bool {
        self.frequency.is_some_and(SecurityFrequency::is_overridden)
    }

    /// O texto em (13, 103): `Security Offline` em vermelho sem frequência, senão `Security: <modo da frequência>`.
    pub fn mode_text(&self) -> Option<SecurityMode> {
        self.frequency.map(SecurityFrequency::security)
    }

    /// `setTrusted(field)`: o Enter ou o botão de confirmar da caixa de texto. Só o dono; devolve o nome a mandar (`PacketAddTrusted`), se o cliente o manda, e a
    /// caixa é limpa de qualquer jeito. `None`: quem olha não é o dono, e nada acontece (a caixa fica como está).
    pub fn submit_name<'t>(&self, text: &'t str) -> Option<Option<&'t str>> {
        self.is_owner().then(|| client_sends_name(text))
    }
}

/// As chaves de idioma da tela.
pub const TRUSTED_PLAYERS_KEY: &str = "security.mekanism.trusted_players";
pub const SECURITY_OFFLINE_KEY: &str = "security.mekanism.offline";
pub const SECURITY_ADD_KEY: &str = "security.mekanism.add";
pub const SECURITY_OVERRIDE_KEY: &str = "security.mekanism.override";
pub const BUTTON_REMOVE_KEY: &str = "button.mekanism.remove";
pub const MODE_TOOLTIP_KEYS: [&str; 3] = [
    "security.mekanism.public.mode",
    "security.mekanism.private.mode",
    "security.mekanism.trusted.mode",
];

/// Os botões de menu que a tela manda ao servidor: o modo (`SECURITY_DESK_MODE`, com o ordinal), a sobreposição (`OVERRIDE_BUTTON`) e tirar da lista
/// (`REMOVE_TRUSTED`, com o índice). Os ids ficam longe dos da aba de segurança (100 e 101) e dos botões de receita.
pub const BUTTON_MODE: i32 = 110;
pub const BUTTON_OVERRIDE: i32 = 113;
pub const BUTTON_REMOVE: i32 = 1000;

/// O que um botão de menu da mesa pede.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeskAction {
    Mode(SecurityMode),
    ToggleOverride,
    Remove(i32),
}

impl DeskAction {
    pub fn button(self) -> i32 {
        match self {
            Self::Mode(mode) => BUTTON_MODE + mode.ordinal() as i32,
            Self::ToggleOverride => BUTTON_OVERRIDE,
            Self::Remove(index) => BUTTON_REMOVE.saturating_add(index.max(0)),
        }
    }

    pub fn of_button(button: i32) -> Option<Self> {
        match button {
            b if (BUTTON_MODE..BUTTON_MODE + 3).contains(&b) => {
                Some(Self::Mode(SecurityMode::by_id(b - BUTTON_MODE)))
            }
            BUTTON_OVERRIDE => Some(Self::ToggleOverride),
            b if b >= BUTTON_REMOVE => Some(Self::Remove(b - BUTTON_REMOVE)),
            _ => None,
        }
    }

    /// O que o servidor faz com a ação (os três `GuiInteraction` não conferem o dono: é o cliente que desliga os botões).
    pub fn apply(self, frequency: &mut SecurityFrequency) -> Vec<DeskEffect> {
        match self {
            Self::Mode(mode) => set_mode(frequency, mode),
            Self::ToggleOverride => toggle_override(frequency),
            Self::Remove(index) => remove_trusted(frequency, index),
        }
    }
}
