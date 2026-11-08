//! De que objeto o menu de um tile lê os dados: o `MekanismContainer` lê o tile direto (`broadcastChanges`, `sendAllDataToRemote`), e o
//! tile do Java existe desde o `loadAdditional`, antes do primeiro tick dele. O port tem um objeto vivo por tile que nasce no primeiro
//! tick (`born`), então até lá o menu lê um objeto feito do que o tile carregou, como o `loadAdditional` o deixou.
//!
//! Um objeto vivo só vale para o tile que ele andou (`ticked`): o de uma posição que ganhou um tile novo (outro bloco posto, o chunk
//! que voltou do disco) ficou para trás, e ler o dele mostraria o estado do tile antigo.

/// O objeto que o menu lê. `ticked` diz se o tile atual já foi andado pelo objeto vivo `live`; sem isso o tile ainda não andou, e o
/// objeto é o que `load` faz do NBT dele (guardado em `loaded`, que fica vivo enquanto o chamador o usa). Um tile que já andou sem
/// objeto vivo (o objeto saiu da tabela) não tem o que ler.
pub fn object<'a, M>(
    ticked: bool,
    live: Option<&'a mut M>,
    loaded: &'a mut Option<M>,
    load: impl FnOnce() -> M,
) -> Option<&'a mut M> {
    if ticked {
        live
    } else {
        Some(loaded.insert(load()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tile_that_has_not_ticked_reads_what_it_loaded() {
        let mut loaded = None;
        let object = object(false, None, &mut loaded, || 7);
        assert_eq!(object.copied(), Some(7));
    }

    #[test]
    fn a_stale_live_object_is_not_read_for_a_tile_that_has_not_ticked() {
        let mut stale = 99;
        let mut loaded = None;
        let object = object(false, Some(&mut stale), &mut loaded, || 7);
        assert_eq!(object.copied(), Some(7));
    }

    #[test]
    fn a_tile_that_ticked_reads_the_live_object_and_loads_nothing() {
        let mut live = 5;
        let mut loaded = None;
        let object = object(true, Some(&mut live), &mut loaded, || panic!("não carrega"));
        assert_eq!(object.copied(), Some(5));
    }

    #[test]
    fn a_tile_that_ticked_without_a_live_object_has_nothing_to_read() {
        let mut loaded = None;
        assert_eq!(object(true, None, &mut loaded, || 7), None);
    }
}
