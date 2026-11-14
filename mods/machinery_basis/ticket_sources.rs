// Entra em `crates/horizon-client/src/ticket_levels.rs` como `pub mod machinery_ticket_sources` (`visibility = "pub"`). Dá a quem está fora do módulo os tickets que o `ChunkTickets` guarda
// (`Sources`: onde estão os jogadores, os chunks forçados, os tickets de portal, a arena do dragão e a distância de visão), que o campo privado `sources` não deixa ler. O `Levels` do Horizon
// corta o nível de carga em 33 (`FULL` e acima contam como nenhum ticket), e o que a `machinery_basis` precisa é o nível de ticket de um chunk que o original ainda segura na memória, até
// 44 (`ChunkLevel.MAX_LEVEL`): ela o calcula dos mesmos tickets, como o `DistanceManager` (decisão 199).
impl super::ChunkTickets {
    /// Os tickets de agora, como `update` os recebeu; `None` num jogo sem servidor nem jogador (tica em todo lugar).
    pub fn mb_sources(&self) -> Option<&super::Sources> {
        self.sources.as_ref()
    }
}
