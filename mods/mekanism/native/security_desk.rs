// Entra em `crates/horizon-client/src/game.rs` como `mod mekanism_desk`. A Security Desk (`TileEntitySecurityDesk`, `BlockTypeTile SECURITY_DESK`): o tile com o dono e
// os dois slots (o de cima destrava o item do dono, o de baixo o trava com o dono e o modo da frequência), o bloco de apoio em cima (`AttributeHasBounding.ABOVE_ONLY`) e
// os pacotes da tela (`PacketGuiInteract` da mesa e `PacketAddTrusted`). A frequência de segurança, a lista de confiança e a sobreposição são da `machinery_basis`
// (`security_frequency`, `security_desk`); aqui fica só o que é deste bloco.
//
// A tela (`GuiSecurityDesk`) ainda não existe (frente `mesa-seguranca`, 1.ª rodada): o tile não declara contêiner, então nada abre e os dois slots ficam vazios. O
// `tick`, o botão de menu e o pacote do nome já estão ligados para quando ela chegar.
use crate::game::{Game, machinery_basis as basis};
use horizon_client::containers::ModBlockEntity;
use horizon_client::items::ItemStack;
use horizon_world::nbt::Compound;

/// `MekanismBlocks.SECURITY_DESK` e `BOUNDING_BLOCK`.
pub(in crate::game) const DESK: &str = "mekanism:security_desk";
pub(in crate::game) const BOUNDING: &str = "mekanism:bounding_block";
/// `AttributeHasBounding.ABOVE_ONLY`: um apoio, em cima.
const ABOVE_ONLY: &[[i32; 3]] = &[[0, 1, 0]];

pub(in crate::game) fn is_desk(block: &str) -> bool {
    block == DESK
}

/// Os blocos desta parte que têm block entity: a mesa e o bloco de apoio (`TileEntityBoundingBlock`, que guarda o deslocamento até o principal).
pub(in crate::game) fn block_entity(block: &str, entity: &mut Option<(String, Compound)>) {
    if is_desk(block) {
        // A mesa só tem dono (`ownerCapability`, sem `securityCapability`): só ele a abre, a quebra e a configura.
        basis::security_owner_only_register(DESK);
        basis::bounding_register(DESK, BOUNDING, ABOVE_ONLY);
    }
    if (is_desk(block) || block == BOUNDING) && entity.is_none() {
        *entity = Some((block.to_owned(), Compound::default()));
    }
}

/// `serverTicker(TileEntityMekanism::tickServer)`.
pub(in crate::game) fn ticks(block: &str, ticks: &mut bool) {
    if is_desk(block) {
        *ticks = true;
    }
}

/// `TileEntitySecurityDesk.onUpdateServer`: a frequência do tile é validada (`TileComponentFrequency.tickServer`) e os dois slots agem. `ownable` diz que itens têm
/// dono (`SecurityInventorySlot.VALIDATOR`).
pub(in crate::game) fn tick(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity, ownable: &dyn Fn(&str) -> bool) {
    basis::security_apply_pending(entity, pos);
    basis::security_tile_tick(game, pos, entity);
    if basis::security_desk_tick(game, entity, ownable) {
        game.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
    }
}

/// `BlockMekanism.setPlacedBy`: o dono de quem pôs (ou o do item) e o bloco de apoio em cima.
pub(in crate::game) fn place_block(game: &mut Game<'_>, stack: &ItemStack, pos: [i32; 3]) {
    if !is_desk(&game.native_block(pos)) {
        return;
    }
    basis::name_placed(game, stack, pos, &is_desk);
    basis::security_placed(game, stack, pos, &is_desk);
    basis::bounding_placed(game, pos);
}

/// O jogador quebra a mesa ou o apoio dela. Devolve se o bloco era desta parte.
///
/// A mesa: quem não é o dono não a quebra (`BlockMekanism.getDestroyProgress` e o `BreakBlockEvent`); o dono e o nome ficam guardados para o item solto. O apoio
/// (`BlockBounding.playerDestroy` e `onDestroyedByPlayer`): quebrá-lo é quebrar o principal como o mesmo jogador (os itens, o som e as partículas são os do
/// principal); se o principal recusa, o apoio fica.
pub(in crate::game) fn destroy_block(game: &mut Game<'_>, pos: [i32; 3], canceled: &mut bool) -> bool {
    let block = game.native_block(pos);
    if block == BOUNDING {
        if *canceled {
            return true;
        }
        let Some(main) = basis::bounding_main(game, pos).filter(|main| is_desk(&game.native_block(*main))) else {
            return true;
        };
        if let Err(error) = game.break_block_as_player(main) {
            eprintln!("mekanism: o bloco principal em {main:?} não foi quebrado pelo apoio: {error}");
        }
        // O principal saiu e levou os apoios; se ficou (sem acesso), o apoio também fica. De todo jeito a quebra do apoio em si não segue.
        basis::bounding_flush(game);
        *canceled = true;
        return true;
    }
    if !is_desk(&block) {
        return false;
    }
    if !*canceled && basis::security_break(game, pos, &is_desk) {
        *canceled = true;
        return true;
    }
    if !*canceled {
        basis::security_destroying(game, pos, &is_desk);
        basis::name_destroying(game, pos, &is_desk);
    }
    true
}

/// `copy_name` e `copy_components` de `owner` e `security` do loot da mesa (`loot_table/blocks/security_desk.json`). O `mekanism:items` (os dois slots dentro do
/// item) entra com a tela.
pub(in crate::game) fn loot_drops(game: &Game<'_>, context: &horizon_client::mod_api::NativeLootDrops<'_>, drops: &mut [ItemStack]) {
    basis::name_loot(game, context, drops, &is_desk);
    basis::security_loot(game, context, drops, &is_desk);
}

/// `TileEntityMekanism.openGui`: só o dono abre a mesa; os outros recebem a mensagem. Devolve se a abertura deve ser cancelada.
pub(in crate::game) fn container_opened(game: &mut Game<'_>, player: u128, pos: [i32; 3]) -> bool {
    is_desk(&game.native_block(pos)) && basis::security_open(game, player, pos, &is_desk)
}

/// Os três `GuiInteraction` da mesa (`SECURITY_DESK_MODE`, `OVERRIDE_BUTTON` e `REMOVE_TRUSTED`). Devolve se o tile é a mesa.
pub(in crate::game) fn menu_button(game: &mut Game<'_>, pos: [i32; 3], entity: &ModBlockEntity, id: i32) -> bool {
    if !is_desk(&entity.id) {
        return false;
    }
    basis::security_desk_button(game, pos, entity, id);
    true
}
