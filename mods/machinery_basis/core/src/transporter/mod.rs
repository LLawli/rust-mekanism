//! O transportador logístico, a parte que decide para onde uma pilha vai e por que caminho (Mekanism `content/transporter`): a pilha
//! ([`stack::TransporterStack`]), a busca de caminho com o custo dos transportadores ([`pathfinder`]), a cache dela ([`cache`]), o
//! que as pilhas em voo já ocupam nos destinos ([`manager::TransporterManager`]) e o que a busca lê de cada transportador e do mundo
//! ([`world`], [`node`]). O movimento por tick (`onUpdateServer`, a entrega, o puxar) é da parte 2.
//!
//! # A ordem e o desempate
//!
//! Tudo o que decide o caminho é determinístico no original: o `Pathfinder` percorre um `LongOpenHashSet` (a ordem da tabela,
//! `sim::open_hash`), os aceitadores vêm de um `Long2ObjectOpenHashMap` por `BlockPos.asLong`, a lista de destinos é ordenada de forma
//! estável (`Collections.sort`) por custo e depois por tamanho do caminho, e os lados andam na ordem do `Direction`. A única ordem que
//! o original deixa a um sorteio da JVM é a de `TransporterManager.flowingStacks[pos].stacks`, um `HashSet<TransporterStack>` por
//! identidade; o port percorre na ordem de inserção (decisão 118).
pub mod cache;
pub mod manager;
pub mod nbt;
pub mod node;
pub mod pathfinder;
pub mod pos;
pub mod stack;
pub mod world;

pub use cache::{CachedPath, PathfinderCache};
pub use manager::TransporterManager;
pub use stack::{Path, SidedBlockPos, StackRef, TransporterStack};
