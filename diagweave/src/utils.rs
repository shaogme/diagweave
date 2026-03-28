#[cfg(not(feature = "std"))]
use alloc::collections::{BTreeMap, BTreeSet};
#[cfg(feature = "std")]
use std::collections::{HashMap, HashSet};

#[cfg(feature = "std")]
/// Fast map alias optimized for the current target environment.
pub type FastMap<K, V> = HashMap<K, V>;
#[cfg(not(feature = "std"))]
/// Fast map alias optimized for the current target environment.
pub type FastMap<K, V> = BTreeMap<K, V>;

#[cfg(feature = "std")]
/// Fast set alias optimized for the current target environment.
pub type FastSet<T> = HashSet<T>;
#[cfg(not(feature = "std"))]
/// Fast set alias optimized for the current target environment.
pub type FastSet<T> = BTreeSet<T>;

#[cfg(any(feature = "trace", feature = "otel"))]
/// Creates an empty fast map.
pub fn fast_map_new<K, V>() -> FastMap<K, V> {
    FastMap::default()
}

/// Creates an empty fast set.
pub fn fast_set_new<T>() -> FastSet<T> {
    FastSet::default()
}

#[cfg(all(feature = "std", feature = "json"))]
/// Creates a fast map with the requested capacity.
pub fn fast_map_with_capacity<K, V>(capacity: usize) -> FastMap<K, V> {
    HashMap::with_capacity(capacity)
}

#[cfg(all(not(feature = "std"), feature = "json"))]
/// Creates a fast map with the requested capacity hint.
pub fn fast_map_with_capacity<K, V>(_: usize) -> FastMap<K, V> {
    BTreeMap::new()
}

#[cfg(feature = "std")]
/// Creates a fast set with the requested capacity.
pub fn fast_set_with_capacity<T>(capacity: usize) -> FastSet<T> {
    HashSet::with_capacity(capacity)
}

#[cfg(not(feature = "std"))]
/// Creates a fast set with the requested capacity hint.
pub fn fast_set_with_capacity<T>(_: usize) -> FastSet<T> {
    BTreeSet::new()
}
