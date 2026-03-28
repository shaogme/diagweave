#[cfg(not(feature = "std"))]
use alloc::collections::{BTreeMap, BTreeSet};

#[cfg(feature = "std")]
pub type FastMap<K, V> = std::collections::HashMap<K, V>;
#[cfg(not(feature = "std"))]
pub type FastMap<K, V> = BTreeMap<K, V>;

#[cfg(feature = "std")]
pub type FastSet<T> = std::collections::HashSet<T>;
#[cfg(not(feature = "std"))]
pub type FastSet<T> = BTreeSet<T>;

#[cfg(feature = "trace")]
pub fn fast_map_new<K, V>() -> FastMap<K, V> {
    FastMap::default()
}

pub fn fast_set_new<T>() -> FastSet<T> {
    FastSet::default()
}

#[cfg(feature = "std")]
pub fn fast_map_with_capacity<K, V>(capacity: usize) -> FastMap<K, V> {
    std::collections::HashMap::with_capacity(capacity)
}

#[cfg(not(feature = "std"))]
pub fn fast_map_with_capacity<K, V>(_: usize) -> FastMap<K, V> {
    BTreeMap::new()
}

#[cfg(feature = "std")]
pub fn fast_set_with_capacity<T>(capacity: usize) -> FastSet<T> {
    std::collections::HashSet::with_capacity(capacity)
}

#[cfg(not(feature = "std"))]
pub fn fast_set_with_capacity<T>(_: usize) -> FastSet<T> {
    BTreeSet::new()
}
