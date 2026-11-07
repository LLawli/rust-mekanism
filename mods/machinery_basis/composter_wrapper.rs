// Entra em `game/composter.rs` para enxergar o que é privado dali (`compostable`, `provider_int` com o contexto do bloco): o
// `ComposterWrapper` do NeoForge sorteia as camadas do item uma vez por tentativa de inserir e só sobe o nível em um, e o Horizon só
// oferece o caminho do vanilla (`composter_container_insert`), que soma as camadas sorteadas e gasta o item mesmo quando nenhuma entra.
use super::Game;
use horizon_client::items::ItemStack;
use horizon_core::position::BlockPos;

impl Game<'_> {
    /// `Compostable.layers().get(lootContext, 0)` do item, calculado agora (a cada chamada sorteia, com o contexto de interação do
    /// bloco em `pos`): `None` quando o item não tem o componente `compostable`.
    pub(in crate::game) fn machinery_composter_layers(&mut self, pos: [i32; 3], stack: &ItemStack) -> Option<i32> {
        let layers = self.compostable(stack)?;
        let context = super::ComputeContext {
            block: Some(pos),
            target: None,
        };
        Some(self.provider_int(&layers, &context, 0).unwrap_or(0))
    }

    /// O que o `onRootCommit` do `ComposterWrapper` faz depois de subir um nível: o bloco passa a ter um nível a mais, com os avisos
    /// e o evento de jogo, agenda o tick de 20 ticks ao chegar em 7 e manda o evento de nível 1500 (partículas e som de enchimento).
    /// É o `ComposterBlock.addLayer` com uma camada.
    pub(in crate::game) fn machinery_composter_raise(&mut self, pos: [i32; 3]) {
        if self.composter_level(pos).is_none() {
            return;
        }
        let at = BlockPos::new(pos[0], pos[1], pos[2]);
        self.with_level(|b, level, ticks| b.composter_insert(level, ticks, at, 1));
        self.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
        let _ = self.process_level_events();
    }
}
