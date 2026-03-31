#[cfg(not(feature = "std"))]
use alloc::collections::{BTreeMap, BTreeSet};
use core::borrow::Borrow;
#[cfg(feature = "std")]
use core::hash::Hash;
#[cfg(feature = "std")]
use std::collections::{HashMap, HashSet};

#[cfg(feature = "std")]
type FastMapImpl<K, V> = HashMap<K, V>;
#[cfg(not(feature = "std"))]
type FastMapImpl<K, V> = BTreeMap<K, V>;

#[cfg(feature = "std")]
type FastSetImpl<T> = HashSet<T>;
#[cfg(not(feature = "std"))]
type FastSetImpl<T> = BTreeSet<T>;

/// Fast map wrapper optimized for the current target environment.
#[derive(Debug, Clone)]
pub struct FastMap<K, V>(FastMapImpl<K, V>);

impl<K, V> Default for FastMap<K, V>
where
    FastMapImpl<K, V>: Default,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> PartialEq for FastMap<K, V>
where
    FastMapImpl<K, V>: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<K, V> Eq for FastMap<K, V> where FastMapImpl<K, V>: Eq {}

#[cfg(feature = "json")]
impl<K, V> serde::Serialize for FastMap<K, V>
where
    FastMapImpl<K, V>: serde::Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

#[cfg(feature = "json")]
impl<'de, K, V> serde::Deserialize<'de> for FastMap<K, V>
where
    FastMapImpl<K, V>: serde::Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        FastMapImpl::<K, V>::deserialize(deserializer).map(Self)
    }
}

impl<K, V> FastMap<K, V> {
    /// Creates an empty fast map.
    pub fn new() -> Self {
        Self(FastMapImpl::default())
    }

    /// Returns true if the map contains no elements.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the number of elements in the map.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Removes all key-value pairs from the map.
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Returns an iterator over key-value pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.0.iter()
    }

    /// Returns a mutable iterator over key-value pairs.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&K, &mut V)> {
        self.0.iter_mut()
    }
}

#[cfg(feature = "std")]
impl<K, V> FastMap<K, V> {
    /// Creates a fast map with the requested capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self(HashMap::with_capacity(capacity))
    }
}

#[cfg(not(feature = "std"))]
impl<K, V> FastMap<K, V> {
    /// Creates a fast map with the requested capacity hint.
    pub fn with_capacity(_: usize) -> Self {
        Self(BTreeMap::new())
    }
}

#[cfg(feature = "std")]
impl<K, V> FastMap<K, V>
where
    K: Eq + Hash,
{
    /// Inserts a key-value pair into the map.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.0.insert(key, value)
    }

    /// Returns a shared reference to the value corresponding to the key.
    pub fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.0.get(key)
    }

    /// Returns a mutable reference to the value corresponding to the key.
    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.0.get_mut(key)
    }

    /// Returns true if the map contains a value for the specified key.
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.0.contains_key(key)
    }

    /// Removes a key from the map, returning the value if the key was in the map.
    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.0.remove(key)
    }
}

#[cfg(not(feature = "std"))]
impl<K, V> FastMap<K, V>
where
    K: Ord,
{
    /// Inserts a key-value pair into the map.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.0.insert(key, value)
    }

    /// Returns a shared reference to the value corresponding to the key.
    pub fn get<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Ord,
    {
        self.0.get(key)
    }

    /// Returns a mutable reference to the value corresponding to the key.
    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Ord,
    {
        self.0.get_mut(key)
    }

    /// Returns true if the map contains a value for the specified key.
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Ord,
    {
        self.0.contains_key(key)
    }

    /// Removes a key from the map, returning the value if the key was in the map.
    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Ord,
    {
        self.0.remove(key)
    }
}

impl<K, V> Extend<(K, V)> for FastMap<K, V>
where
    FastMapImpl<K, V>: Extend<(K, V)>,
{
    fn extend<T: IntoIterator<Item = (K, V)>>(&mut self, iter: T) {
        self.0.extend(iter);
    }
}

impl<K, V> FromIterator<(K, V)> for FastMap<K, V>
where
    FastMapImpl<K, V>: FromIterator<(K, V)>,
{
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl<K, V> IntoIterator for FastMap<K, V> {
    type Item = (K, V);
    type IntoIter = <FastMapImpl<K, V> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, K, V> IntoIterator for &'a FastMap<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = <&'a FastMapImpl<K, V> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        (&self.0).into_iter()
    }
}

impl<'a, K, V> IntoIterator for &'a mut FastMap<K, V> {
    type Item = (&'a K, &'a mut V);
    type IntoIter = <&'a mut FastMapImpl<K, V> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        (&mut self.0).into_iter()
    }
}

/// Fast set wrapper optimized for the current target environment.
#[derive(Debug, Clone)]
pub struct FastSet<T>(FastSetImpl<T>);

impl<T> Default for FastSet<T>
where
    FastSetImpl<T>: Default,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> PartialEq for FastSet<T>
where
    FastSetImpl<T>: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T> Eq for FastSet<T> where FastSetImpl<T>: Eq {}

#[cfg(feature = "json")]
impl<T> serde::Serialize for FastSet<T>
where
    FastSetImpl<T>: serde::Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

#[cfg(feature = "json")]
impl<'de, T> serde::Deserialize<'de> for FastSet<T>
where
    FastSetImpl<T>: serde::Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        FastSetImpl::<T>::deserialize(deserializer).map(Self)
    }
}

impl<T> FastSet<T> {
    /// Creates an empty fast set.
    pub fn new() -> Self {
        Self(FastSetImpl::default())
    }

    /// Returns true if the set contains no elements.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the number of elements in the set.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Removes all elements from the set.
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Returns an iterator over all values in the set.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.0.iter()
    }
}

#[cfg(feature = "std")]
impl<T> FastSet<T> {
    /// Creates a fast set with the requested capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self(HashSet::with_capacity(capacity))
    }
}

#[cfg(not(feature = "std"))]
impl<T> FastSet<T> {
    /// Creates a fast set with the requested capacity hint.
    pub fn with_capacity(_: usize) -> Self {
        Self(BTreeSet::new())
    }
}

#[cfg(feature = "std")]
impl<T> FastSet<T>
where
    T: Eq + Hash,
{
    /// Adds a value to the set. Returns true if the value was not present.
    pub fn insert(&mut self, value: T) -> bool {
        self.0.insert(value)
    }

    /// Returns true if the set contains the value.
    pub fn contains<Q>(&self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.0.contains(value)
    }

    /// Removes a value from the set. Returns true if the value existed.
    pub fn remove<Q>(&mut self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: ?Sized + Eq + Hash,
    {
        self.0.remove(value)
    }
}

#[cfg(not(feature = "std"))]
impl<T> FastSet<T>
where
    T: Ord,
{
    /// Adds a value to the set. Returns true if the value was not present.
    pub fn insert(&mut self, value: T) -> bool {
        self.0.insert(value)
    }

    /// Returns true if the set contains the value.
    pub fn contains<Q>(&self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: ?Sized + Ord,
    {
        self.0.contains(value)
    }

    /// Removes a value from the set. Returns true if the value existed.
    pub fn remove<Q>(&mut self, value: &Q) -> bool
    where
        T: Borrow<Q>,
        Q: ?Sized + Ord,
    {
        self.0.remove(value)
    }
}

impl<T> Extend<T> for FastSet<T>
where
    FastSetImpl<T>: Extend<T>,
{
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        self.0.extend(iter);
    }
}

impl<T> FromIterator<T> for FastSet<T>
where
    FastSetImpl<T>: FromIterator<T>,
{
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl<T> IntoIterator for FastSet<T> {
    type Item = T;
    type IntoIter = <FastSetImpl<T> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a FastSet<T> {
    type Item = &'a T;
    type IntoIter = <&'a FastSetImpl<T> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        (&self.0).into_iter()
    }
}
