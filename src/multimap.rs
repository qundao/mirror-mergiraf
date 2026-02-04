use rustc_hash::{FxHashMap, FxHashSet};
use std::{
    hash::Hash,
    iter::{FromIterator, FusedIterator},
};

/// A map which associates a set of values to each key.
#[derive(Debug, Clone)]
pub struct MultiMap<K, V> {
    map: FxHashMap<K, FxHashSet<V>>,
    empty: FxHashSet<V>, // stays empty over the entire life of the struct (for convenience in the get method)
}

impl<K, V> MultiMap<K, V> {
    /// Creates an empty multimap
    pub fn new() -> Self {
        Self::default()
    }

    /// The keys that are present
    pub fn keys(&self) -> impl ExactSizeIterator<Item = &K> + FusedIterator {
        self.map.keys()
    }

    /// The number of keys mapped
    pub fn len(&self) -> usize {
        self.keys().len()
    }

    /// An iterator over all pairs of key, values stored in this map
    #[allow(dead_code)]
    pub fn iter_pairs(&self) -> impl Iterator<Item = (&K, &V)> {
        self.map
            .iter()
            .flat_map(|(k, v_set)| v_set.iter().map(move |v| (k, v)))
    }

    /// An iterator which groups pairs by key
    pub fn iter(&self) -> impl Iterator<Item = (&K, &FxHashSet<V>)> {
        self.map.iter()
    }

    /// Iterates over all values stored in this container
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.map.values().flatten()
    }
}

impl<K, V> MultiMap<K, V>
where
    K: Eq + Hash,
    V: Eq + Hash,
{
    /// Gets the set of values associated to the key (which might be empty)
    pub fn get<Q>(&self, key: &Q) -> &FxHashSet<V>
    where
        K: std::borrow::Borrow<Q>,
        Q: Eq + Hash,
    {
        self.map.get(key).unwrap_or(&self.empty)
    }

    /// Adds a mapping from a key to a value.
    /// Returns whether the mapping was already added or if it already existed.
    pub fn insert(&mut self, key: K, value: V) -> bool {
        let set = self.map.entry(key).or_default();
        set.insert(value)
    }

    // Deletes one value that matches the predicate, among
    // the elements mapped to a given key.
    // Returns the deleted element, if any.
    #[cfg(feature = "dev")]
    pub fn remove_one<F>(&mut self, key: K, pred: F) -> Option<V>
    where
        F: FnMut(&V) -> bool,
    {
        use std::collections::hash_map::Entry;
        match self.map.entry(key) {
            Entry::Occupied(mut entry) => {
                let set = entry.get_mut();
                let removed = set.extract_if(pred).next();
                if set.is_empty() {
                    entry.remove();
                }
                removed
            }
            Entry::Vacant(_) => None,
        }
    }

    pub fn contains_key<Q>(&self, k: &Q) -> bool
    where
        K: std::borrow::Borrow<Q>,
        Q: Eq + Hash,
    {
        !self.get(k).is_empty()
    }
}

impl<K, V> Default for MultiMap<K, V> {
    fn default() -> Self {
        Self {
            map: FxHashMap::default(),
            empty: FxHashSet::default(),
        }
    }
}

impl<K, V> FromIterator<(K, V)> for MultiMap<K, V>
where
    K: Eq + Hash,
    V: Eq + Hash,
{
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
    {
        let mut result = Self::new();
        for (k, v) in iter {
            result.insert(k, v);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn simple_lifecycle() {
        let mut multimap = MultiMap::new();

        assert!(multimap.get(&23).is_empty());
        assert!(multimap.insert(23, 45));
        assert_eq!(multimap.get(&23), &FxHashSet::from_iter([45]));
        assert!(!multimap.insert(23, 45));
        assert_eq!(multimap.get(&23), &FxHashSet::from_iter([45]));
        assert!(multimap.insert(23, 67));
        assert_eq!(multimap.get(&23), &FxHashSet::from_iter([45, 67]));

        let full_set = multimap
            .iter_pairs()
            .map(|(x, y)| (*x, *y))
            .collect::<HashSet<(i32, i32)>>();
        let expected_full_set = HashSet::from([(23, 45), (23, 67)]);
        assert_eq!(full_set, expected_full_set);
    }

    #[cfg(feature = "dev")]
    #[test]
    fn removals() {
        let mut multimap = MultiMap::new();

        assert!(multimap.insert(23, 45));
        assert!(multimap.insert(23, 67));
        assert_eq!(multimap.remove_one(23, |x| *x == 45), Some(45));
        assert_eq!(multimap.get(&23), &FxHashSet::from_iter([67]));

        // removing an absent value from a present key
        assert_eq!(multimap.remove_one(23, |x| *x == 11), None);

        assert_eq!(multimap.remove_one(11, |x| *x == 67), None);
        // looking up an absent key didn't create an empty set in the multimap
        assert_eq!(multimap.len(), 1);

        // removing the last value from a key removes the corresponding entry from the map
        assert_eq!(multimap.remove_one(23, |x| *x == 67), Some(67));
        assert_eq!(multimap.len(), 0);

        multimap.insert(1, 2);
        multimap.insert(1, 3);

        // if the predicate matches multiple elements, only one is removed
        assert!(multimap.remove_one(1, |_| true).is_some());
        assert_eq!(multimap.get(&1).len(), 1);
    }
}
