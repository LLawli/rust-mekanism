//! O nível que ninguém segura: o que o original faz com os chunks de uma dimensão que o jogador deixou (decisão 193).
//!
//! No original todo `ServerLevel` anda a cada tick do servidor, com jogador ou sem (`MinecraftServer.tickChildren`), e o `ServerChunkCache` de cada um tira os tickets dos
//! chunks que ninguém mais segura (o jogador saiu, nenhum chunk forçado): o `ChunkMap.processUnloads` os descarrega (`ChunkEvent.Unload`, depois `onChunkUnloaded` e
//! `setRemoved` de cada block entity) e o `ServerTickEvent.Post` do mesmo tick (`TransmitterNetworkRegistry.onTick`) trata os transmissores inválidos. O Horizon só anda os níveis
//! que `levels_to_tick` diz (jogador, chunk forçado, ticket de mod) e nunca descarregava o Overworld parado: este modelo diz, a cada fim de tick, quais níveis parados já
//! descarregam e quais ainda estão carregados e esperando.
//!
//! Medido no original (`dev/referencia/rastros/descarga_nivel.jsonl`, a cena `CenaReset`, passo `a4_no_nether`): o jogador vai ao Nether entre o tick 441 e o 442; o tick 442 não
//! descarrega nada, o 443 descarrega os primeiros 558 chunks do Overworld (o chunk (0, 0), com os 29 transmissores, entre eles), e o `Post` do mesmo tick 443 já trata os 29
//! inválidos (as redes caem de 6 para 1). O resto dos 1.369 chunks sai nos ticks 448 a 451 (o orçamento de tempo do `processUnloads`, que depende da máquina).
use std::collections::BTreeMap;

/// Os ticks sem ninguém até o nível descarregar: o primeiro tick sem o jogador não descarrega, o segundo sim (o 442 e o 443 do rastro).
pub const UNLOAD_DELAY_TICKS: u32 = 2;

/// O que o fim de um tick manda fazer.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Step {
    /// Os níveis que descarregam agora, antes do `ServerTickEvent.Post` do tick.
    pub unload: Vec<String>,
    /// Os níveis parados que ainda estão carregados neste tick (os que esperam e os que acabaram de descarregar): o `Post` do original trata os transmissores deles.
    pub waiting: Vec<String>,
}

/// Os níveis que o servidor já andou com alguém e ainda não descarregaram, com os ticks seguidos sem ninguém.
#[derive(Debug, Default)]
pub struct IdleLevels {
    loaded: BTreeMap<String, u32>,
}

impl IdleLevels {
    pub fn new() -> Self {
        Self::default()
    }

    /// O fim de um tick do servidor: `occupied` são os níveis que o jogo andou (jogador, chunk forçado, ticket de mod, veículo à espera).
    pub fn end_of_tick(&mut self, occupied: &[&str]) -> Step {
        for dim in occupied {
            self.loaded.insert((*dim).to_owned(), 0);
        }
        let mut step = Step::default();
        let idle: Vec<String> = self
            .loaded
            .keys()
            .filter(|dim| !occupied.contains(&dim.as_str()))
            .cloned()
            .collect();
        for dim in idle {
            let ticks = self.loaded.get_mut(&dim).map(|ticks| {
                *ticks += 1;
                *ticks
            });
            step.waiting.push(dim.clone());
            if ticks.is_some_and(|ticks| ticks >= UNLOAD_DELAY_TICKS) {
                self.loaded.remove(&dim);
                step.unload.push(dim);
            }
        }
        step
    }

    /// Os níveis em que o `ServerTickPost` do tick roda: os que o servidor andou e os parados que ainda estão carregados.
    pub fn post_levels(occupied: &[&str], step: &Step) -> Vec<String> {
        let mut dims: Vec<String> = occupied.iter().map(|dim| (*dim).to_owned()).collect();
        for dim in &step.waiting {
            if !dims.contains(dim) {
                dims.push(dim.clone());
            }
        }
        dims
    }

    /// O mundo fechou: nada fica a esperar.
    pub fn reset(&mut self) {
        self.loaded.clear();
    }

    /// Quantos ticks seguidos o nível está sem ninguém (`None` se não está carregado).
    pub fn idle_ticks(&self, dim: &str) -> Option<u32> {
        self.loaded.get(dim).copied()
    }
}

/// A ordem em que os níveis soltam os chunks quando o servidor fecha (`MinecraftServer.stopServer`: `deactivateTicketsOnClosing` e `ChunkMap.processUnloads` de cada nível, um
/// laço de ticks até o `chunkMap.hasWork()` acabar). Cada chunk grava antes de sair (`scheduleUnload`, um futuro assíncrono) e o nível com menos chunks termina primeiro: medido
/// com o Nether (60 chunks) e o Overworld (1.369), o Nether descarrega todo antes de o Overworld começar, nas duas saídas com os dois carregados (`descarga_fechamento.jsonl`).
/// `levels` traz, na ordem dos níveis do servidor, quantos chunks cada um tem carregados; os vazios saem.
pub fn closing_order(levels: &[(String, usize)]) -> Vec<String> {
    let mut loaded: Vec<&(String, usize)> =
        levels.iter().filter(|(_, chunks)| *chunks > 0).collect();
    loaded.sort_by_key(|(_, chunks)| *chunks);
    loaded.into_iter().map(|(dim, _)| dim.clone()).collect()
}
