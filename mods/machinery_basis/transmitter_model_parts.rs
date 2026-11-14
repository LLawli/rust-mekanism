// Roda no começo de `StateModels::models_at`, o pedido de modelos de um bloco na posição dele que o mesher faz. A conta (se o estado é de transmissor e a block entity já deu o dado do modelo) é
// a expressão de `handled`, que o HEAD avalia com os tipos do alvo, sem emprestar o catálogo ao helper: ela põe em `out` as partes do `TransmitterBlockStateModel.collectParts` (uma por lado,
// pelo estado visual de cada um) e devolve `true`. Sem o dado (ou fora de um transmissor) devolve `false` e o corpo original segue, com o modelo do blockstate.
fn machinery_transmitter_model_parts(handled: bool) -> core::ops::ControlFlow<Result<()>> {
    if handled {
        core::ops::ControlFlow::Break(Ok(()))
    } else {
        core::ops::ControlFlow::Continue(())
    }
}
