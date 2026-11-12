//! `AcceptorCache` pelo que o Java faz (`AcceptorCache.java`): o cache por lado que guarda a capability até ela ser invalidada,
//! o `getCachedAcceptor` que só responde com o bit do lado ligado e o ouvinte de invalidação que só suja o lado se o transmissor vale.

use machinery_basis_core::sides::Face;
use machinery_basis_core::transmission::acceptor_cache::AcceptorCache;
use machinery_basis_core::transmission::connection::side_bit;
use std::cell::Cell;

#[test]
fn nothing_is_cached_before_initialize_cache() {
    let cache: AcceptorCache<u32> = AcceptorCache::new();
    assert!(!cache.is_initialized());
    assert_eq!(cache.get_connected_acceptor(Face::Up, &|_| Some(1)), None);
}

#[test]
fn the_capability_is_looked_up_once_until_it_is_invalidated() {
    let mut cache: AcceptorCache<u32> = AcceptorCache::new();
    cache.initialize_cache();
    let calls = Cell::new(0);
    let lookup = |_: Face| {
        calls.set(calls.get() + 1);
        Some(calls.get())
    };
    assert_eq!(cache.get_connected_acceptor(Face::North, &lookup), Some(1));
    assert_eq!(cache.get_connected_acceptor(Face::North, &lookup), Some(1));
    // Outro lado tem o próprio cache.
    assert_eq!(cache.get_connected_acceptor(Face::South, &lookup), Some(2));
    cache.invalidate(Face::North);
    assert_eq!(cache.get_connected_acceptor(Face::North, &lookup), Some(3));
    assert_eq!(cache.get_connected_acceptor(Face::South, &lookup), Some(2));
    assert_eq!(calls.get(), 3);
}

#[test]
fn a_missing_capability_is_cached_too() {
    let mut cache: AcceptorCache<u32> = AcceptorCache::new();
    cache.initialize_cache();
    let calls = Cell::new(0);
    let lookup = |_: Face| {
        calls.set(calls.get() + 1);
        None
    };
    assert_eq!(cache.get_connected_acceptor(Face::East, &lookup), None);
    assert_eq!(cache.get_connected_acceptor(Face::East, &lookup), None);
    assert_eq!(calls.get(), 1);
}

#[test]
fn a_second_initialize_cache_keeps_the_values() {
    let mut cache: AcceptorCache<u32> = AcceptorCache::new();
    cache.initialize_cache();
    assert_eq!(
        cache.get_connected_acceptor(Face::Up, &|_| Some(7)),
        Some(7)
    );
    cache.initialize_cache();
    assert_eq!(
        cache.get_connected_acceptor(Face::Up, &|_| Some(9)),
        Some(7)
    );
}

#[test]
fn get_cached_acceptor_needs_the_side_bit() {
    let mut cache: AcceptorCache<u32> = AcceptorCache::new();
    cache.initialize_cache();
    assert_eq!(cache.get_cached_acceptor(Face::West, &|_| Some(4)), None);
    cache.current_acceptor_connections = side_bit(Face::West);
    assert_eq!(cache.get_cached_acceptor(Face::West, &|_| Some(4)), Some(4));
    assert_eq!(cache.get_cached_acceptor(Face::East, &|_| Some(4)), None);
}

#[test]
fn get_connected_acceptors_skips_the_missing_ones_in_the_order_given() {
    let mut cache: AcceptorCache<u32> = AcceptorCache::new();
    cache.initialize_cache();
    let lookup = |side: Face| match side {
        Face::Down => Some(1),
        Face::North => None,
        Face::East => Some(3),
        _ => Some(9),
    };
    assert_eq!(
        cache.get_connected_acceptors(&[Face::East, Face::North, Face::Down], &lookup),
        vec![3, 1]
    );
}

#[test]
fn the_listener_marks_the_side_dirty_only_for_a_valid_transmitter() {
    let mut cache: AcceptorCache<u32> = AcceptorCache::new();
    cache.initialize_cache();
    assert_eq!(
        cache.get_connected_acceptor(Face::Up, &|_| Some(1)),
        Some(1)
    );
    assert_eq!(cache.refresh_listener_run(Face::Up, false), None);
    // O cache se invalidou mesmo assim: o `BlockCapabilityCache` se invalida antes de chamar o ouvinte.
    assert_eq!(
        cache.get_connected_acceptor(Face::Up, &|_| Some(2)),
        Some(2)
    );
    assert_eq!(cache.refresh_listener_run(Face::Up, true), Some(Face::Up));
}
