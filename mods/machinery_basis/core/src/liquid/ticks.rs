//! A fila de ticks agendados do mundo: `LevelTicks` (`LevelTicks.java`), com um `LevelChunkTicks` por chunk (`LevelChunkTicks.java`) e
//! a ordem de `ScheduledTick` (`ScheduledTick.java:5-17`). O jogo já tem a dele; esta serve ao mundo de teste.
//!
//! - Dentro de um chunk, a fila anda por `DRAIN_ORDER`: tempo de disparo, prioridade, ordem de agendamento (`subTickOrder`).
//! - Entre chunks, `INTRA_TICK_DRAIN_ORDER`: prioridade e ordem de agendamento, sem o tempo (`LevelTicks.java:32`).
//! - Um tick repetido (mesma posição, mesmo tipo) é descartado enquanto o primeiro espera (`LevelChunkTicks.schedule`), mas o
//!   `subTickOrder` já foi gasto (`LevelAccessor.createTick`).
//! - O tick agendado durante a rodada não roda na mesma rodada: a coleta (`collectTicks`) vem antes de rodar (`runCollectedTicks`).
use crate::sim::chunks::{BlockPos, ChunkPos};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::Hash;

/// O `maxTicksToProcess` que o `ServerLevel` passa (`ServerLevel.java:411-413`).
pub const MAX_TICKS_PER_LEVEL_TICK: usize = 65536;

/// `TickPriority.NORMAL`.
pub const NORMAL_PRIORITY: i32 = 0;

/// `ScheduledTick`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScheduledTick<T> {
    pub kind: T,
    pub pos: BlockPos,
    pub trigger_tick: i64,
    pub priority: i32,
    pub sub_tick_order: i64,
}

impl<T> ScheduledTick<T> {
    /// `DRAIN_ORDER` (`ScheduledTick.java:5-12`).
    fn drain_key(&self) -> (i64, i32, i64) {
        (self.trigger_tick, self.priority, self.sub_tick_order)
    }

    /// `INTRA_TICK_DRAIN_ORDER` (`ScheduledTick.java:13-16`).
    fn intra_tick_cmp(&self, other: &Self) -> Ordering {
        (self.priority, self.sub_tick_order).cmp(&(other.priority, other.sub_tick_order))
    }
}

/// `LevelChunkTicks`: a fila de um chunk e o conjunto `(pos, tipo)` que descarta o repetido.
struct ChunkTicks<T> {
    queue: BTreeMap<(i64, i32, i64), ScheduledTick<T>>,
    per_position: HashSet<(BlockPos, T)>,
}

impl<T: Copy + Eq + Hash> ChunkTicks<T> {
    fn new() -> Self {
        Self {
            queue: BTreeMap::new(),
            per_position: HashSet::new(),
        }
    }

    fn peek(&self) -> Option<ScheduledTick<T>> {
        self.queue.values().next().copied()
    }

    fn poll(&mut self) -> Option<ScheduledTick<T>> {
        let (_, tick) = self.queue.pop_first()?;
        self.per_position.remove(&(tick.pos, tick.kind));
        Some(tick)
    }
}

/// `LevelTicks`.
pub struct LevelTicks<T> {
    containers: HashMap<ChunkPos, ChunkTicks<T>>,
    next_tick_for_container: HashMap<ChunkPos, i64>,
}

impl<T: Copy + Eq + Hash> Default for LevelTicks<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy + Eq + Hash> LevelTicks<T> {
    pub fn new() -> Self {
        Self {
            containers: HashMap::new(),
            next_tick_for_container: HashMap::new(),
        }
    }

    /// `LevelTicks.schedule` e `LevelChunkTicks.schedule` (`LevelTicks.java:70-79`, `LevelChunkTicks.java:56-61`), com o
    /// `chunkScheduleUpdater` (`LevelTicks.java:40-44`). Todo chunk do mundo de teste está carregado.
    pub fn schedule(&mut self, tick: ScheduledTick<T>) {
        let chunk = tick.pos.chunk();
        let container = self.containers.entry(chunk).or_insert_with(ChunkTicks::new);
        if !container.per_position.insert((tick.pos, tick.kind)) {
            return;
        }
        container.queue.insert(tick.drain_key(), tick);
        if container.peek() == Some(tick) {
            self.next_tick_for_container
                .insert(chunk, tick.trigger_tick);
        }
    }

    /// `hasScheduledTick`.
    pub fn has_scheduled_tick(&self, pos: BlockPos, kind: T) -> bool {
        self.containers
            .get(&pos.chunk())
            .is_some_and(|container| container.per_position.contains(&(pos, kind)))
    }

    /// `collectTicks` (`LevelTicks.java:93-98`): os ticks que rodam em `current_tick`, na ordem em que `runCollectedTicks` os entrega.
    pub fn collect(&mut self, current_tick: i64, max_ticks: usize) -> Vec<ScheduledTick<T>> {
        let mut to_tick = self.sort_containers_to_tick(current_tick);
        let mut out = Vec::new();
        // drainContainers (`LevelTicks.java:126-141`).
        while out.len() < max_ticks {
            let Some(index) = best_container(&self.containers, &to_tick) else {
                break;
            };
            let chunk = to_tick.swap_remove(index);
            let container = self.containers.get_mut(&chunk).expect("chunk na fila");
            out.extend(container.poll());
            // drainFromCurrentContainer (`LevelTicks.java:153-172`).
            if out.len() < max_ticks {
                let next_from_next = best_container(&self.containers, &to_tick)
                    .and_then(|index| self.containers[&to_tick[index]].peek());
                let container = self.containers.get_mut(&chunk).expect("chunk na fila");
                while out.len() < max_ticks {
                    let Some(next) = container.peek() else { break };
                    if next.trigger_tick > current_tick
                        || next_from_next
                            .is_some_and(|other| next.intra_tick_cmp(&other) == Ordering::Greater)
                    {
                        break;
                    }
                    container.poll();
                    out.push(next);
                }
            }
            if let Some(next) = self.containers[&chunk].peek() {
                if next.trigger_tick <= current_tick && out.len() < max_ticks {
                    to_tick.push(chunk);
                } else {
                    self.next_tick_for_container
                        .insert(chunk, next.trigger_tick);
                }
            }
        }
        // rescheduleLeftoverContainers (`LevelTicks.java:143-147`).
        for chunk in to_tick {
            if let Some(next) = self.containers[&chunk].peek() {
                self.next_tick_for_container
                    .insert(chunk, next.trigger_tick);
            }
        }
        out
    }

    /// `sortContainersToTick` (`LevelTicks.java:100-124`).
    fn sort_containers_to_tick(&mut self, current_tick: i64) -> Vec<ChunkPos> {
        let mut to_tick = Vec::new();
        let containers = &self.containers;
        self.next_tick_for_container.retain(|chunk, next_tick| {
            if *next_tick > current_tick {
                return true;
            }
            match containers.get(chunk).and_then(ChunkTicks::peek) {
                None => false,
                Some(tick) if tick.trigger_tick > current_tick => {
                    *next_tick = tick.trigger_tick;
                    true
                }
                Some(_) => {
                    to_tick.push(*chunk);
                    false
                }
            }
        });
        to_tick
    }

    /// Os ticks agendados dentro da caixa `[min, max]` (inclusive), como `pendingFluidTicks` do `FluidsTraceTest` os conta.
    pub fn count_in(&self, min: BlockPos, max: BlockPos) -> usize {
        self.containers
            .values()
            .flat_map(|container| container.queue.values())
            .filter(|tick| {
                (min.x..=max.x).contains(&tick.pos.x)
                    && (min.y..=max.y).contains(&tick.pos.y)
                    && (min.z..=max.z).contains(&tick.pos.z)
            })
            .count()
    }

    /// `count()`.
    pub fn count(&self) -> usize {
        self.containers
            .values()
            .map(|container| container.queue.len())
            .sum()
    }
}

/// O topo do `PriorityQueue` `containersToTick` (`CONTAINER_DRAIN_ORDER`, `LevelTicks.java:32`).
fn best_container<T: Copy + Eq + Hash>(
    containers: &HashMap<ChunkPos, ChunkTicks<T>>,
    to_tick: &[ChunkPos],
) -> Option<usize> {
    to_tick
        .iter()
        .enumerate()
        .filter_map(|(index, chunk)| containers[chunk].peek().map(|tick| (index, tick)))
        .min_by(|(_, a), (_, b)| a.intra_tick_cmp(b))
        .map(|(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick(x: i32, trigger: i64, sub: i64) -> ScheduledTick<u8> {
        ScheduledTick {
            kind: 1,
            pos: BlockPos::new(x, 0, 0),
            trigger_tick: trigger,
            priority: NORMAL_PRIORITY,
            sub_tick_order: sub,
        }
    }

    #[test]
    fn a_repeated_tick_is_dropped_while_the_first_waits() {
        let mut ticks = LevelTicks::new();
        ticks.schedule(tick(0, 5, 0));
        ticks.schedule(tick(0, 3, 1));
        assert_eq!(ticks.count(), 1);
        assert!(ticks.collect(3, MAX_TICKS_PER_LEVEL_TICK).is_empty());
        assert_eq!(
            ticks.collect(5, MAX_TICKS_PER_LEVEL_TICK),
            vec![tick(0, 5, 0)]
        );
        ticks.schedule(tick(0, 9, 2));
        assert_eq!(ticks.count(), 1);
    }

    #[test]
    fn ticks_of_the_same_moment_run_in_scheduling_order_across_chunks() {
        let mut ticks = LevelTicks::new();
        ticks.schedule(tick(40, 5, 0));
        ticks.schedule(tick(0, 5, 1));
        ticks.schedule(tick(20, 5, 2));
        ticks.schedule(tick(1, 5, 3));
        ticks.schedule(tick(41, 6, 4));
        let order: Vec<i32> = ticks
            .collect(5, MAX_TICKS_PER_LEVEL_TICK)
            .iter()
            .map(|t| t.pos.x)
            .collect();
        assert_eq!(order, vec![40, 0, 20, 1]);
        assert_eq!(ticks.count(), 1);
    }

    #[test]
    fn a_late_tick_waits_for_the_next_moment_and_keeps_its_order() {
        let mut ticks = LevelTicks::new();
        ticks.schedule(tick(0, 4, 0));
        ticks.schedule(tick(16, 5, 1));
        assert_eq!(ticks.collect(5, 1), vec![tick(0, 4, 0)]);
        assert_eq!(
            ticks.collect(6, MAX_TICKS_PER_LEVEL_TICK),
            vec![tick(16, 5, 1)]
        );
    }
}
