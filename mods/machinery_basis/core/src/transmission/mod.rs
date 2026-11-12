//! A rede de transmissores, em código puro. Aqui ficam a conexão por lado de um transmissor
//! (`connection`, `transmitter`) e o cache dos aceitadores dele (`acceptor_cache`); a rede em si
//! (registro, topologia, aceitadores da rede) é de outra frente.
pub mod acceptor_cache;
pub mod connection;
pub mod transmitter;
