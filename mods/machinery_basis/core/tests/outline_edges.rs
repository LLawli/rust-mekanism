//! O módulo `outline-edges` da `machinery_basis` (`mods/machinery_basis/outline_edges.rs`), que o launcher enxerta em `blocks.rs` do `horizon-items`, compilado aqui como é: ele só usa
//! `std`. Guarda, por thread, as arestas de uma forma de contexto que é a união de várias caixas, e o RETURN de `cube_edges` as entrega no lugar do cubo, uma vez.
#[allow(dead_code)]
mod machinery_outline_edges {
    include!("../../outline_edges.rs");
}

use machinery_outline_edges as outline;

const CUBE: [f64; 6] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0];
const MINE: [f64; 6] = [0.25, 0.0, 0.0, 0.75, 0.0, 0.0];

#[test]
fn without_arming_the_cube_stays() {
    outline::disarm();
    assert_eq!(outline::take_or(vec![CUBE]), vec![CUBE]);
}

#[test]
fn an_armed_outline_replaces_the_cube_once() {
    outline::arm(vec![MINE, MINE]);
    assert_eq!(outline::take_or(vec![CUBE]), vec![MINE, MINE]);
    // Uma vez só: o `cube_edges` seguinte (outro bloco, a tabela do mesher) volta ao cubo.
    assert_eq!(outline::take_or(vec![CUBE]), vec![CUBE]);
}

#[test]
fn disarming_drops_the_armed_outline() {
    // Toda chamada de `context_outline` arma ou desarma: a de um bloco que não tem forma de contexto apaga a do anterior.
    outline::arm(vec![MINE]);
    outline::disarm();
    assert_eq!(outline::take_or(vec![CUBE]), vec![CUBE]);
}

#[test]
fn arming_again_replaces_the_previous_outline() {
    outline::arm(vec![MINE]);
    outline::arm(vec![CUBE, CUBE]);
    assert_eq!(outline::take_or(vec![MINE]), vec![CUBE, CUBE]);
}

#[test]
fn each_thread_has_its_own_outline() {
    // O jogo do cliente e o do servidor integrado chamam `context_outline` em threads diferentes: o que uma arma a outra não leva.
    outline::arm(vec![MINE]);
    let other = std::thread::spawn(|| {
        outline::arm(vec![CUBE, CUBE]);
        outline::take_or(vec![MINE])
    })
    .join()
    .unwrap();
    assert_eq!(other, vec![CUBE, CUBE]);
    assert_eq!(outline::take_or(vec![CUBE]), vec![MINE]);
}
