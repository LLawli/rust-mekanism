// Entra em `crates/horizon-items/src/blocks.rs` como `pub mod machinery_outline_edges` (`visibility = "pub"`). Guarda, por thread, as arestas que o contorno do bloco apontado desenha
// no lugar das 12 do cubo unitário: o `context_outline` do Horizon devolve uma caixa só, e o contorno é desenhado a partir de `cube_edges()` escalado por essa caixa; uma forma de
// contexto que é a união de várias caixas (o braço `PUSH` ou `PULL` dos transmissores do Mekanism, três caixas empilhadas) arma aqui as arestas dela em fração da caixa que devolveu, e
// o RETURN de `cube_edges` (injeção `outline_edges`) as entrega no lugar do cubo, uma vez. É por thread porque o jogo do cliente e o do servidor integrado rodam em threads
// diferentes e os dois chamam `context_outline` (o raio de seleção): o contorno do quadro desenha na thread que armou, no chamado seguinte de `cube_edges`.
use std::cell::RefCell;

thread_local! {
    static ARMED: RefCell<Option<Vec<[f64; 6]>>> = const { RefCell::new(None) };
}

/// Arma as arestas da forma de contexto (`[x0, y0, z0, x1, y1, z1]` em fração da caixa que o `context_outline` devolveu). Toda chamada de `context_outline` arma ou desarma: o
/// `cube_edges` que vem logo depois, no laço do contorno, é o dela.
pub fn arm(edges: Vec<[f64; 6]>) {
    ARMED.with(|armed| *armed.borrow_mut() = Some(edges));
}

/// Desarma: a forma de contexto é uma caixa só (ou o bloco não tem forma de contexto), e o `cube_edges` seguinte devolve o cubo.
pub fn disarm() {
    ARMED.with(|armed| *armed.borrow_mut() = None);
}

/// O RETURN de `cube_edges`: as arestas armadas (uma vez) no lugar do `cube` que a função devolveu, ou o próprio `cube` sem arma.
pub fn take_or(cube: Vec<[f64; 6]>) -> Vec<[f64; 6]> {
    ARMED.with(|armed| armed.borrow_mut().take()).unwrap_or(cube)
}
