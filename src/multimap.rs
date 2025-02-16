use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    iter::{FromIterator, FusedIterator},
};

/// A map which associates a set of values to each key.
#[derive(Debug)]
pub struct MultiMap<K, V> {
    map: HashMap<K, HashSet<V>>,
    empty: HashSet<V>, // stays empty over the entire life of the struct (for convenience in the get method)
}

impl<K, V> MultiMap<K, V>
where
    K: Eq + PartialEq + Hash,
    V: Eq + PartialEq + Hash,
{
    /// Creates an empty multimap
    pub fn new() -> MultiMap<K, V> {
        MultiMap {
            map: HashMap::new(),
            empty: HashSet::new(),
        }
    }

    /// Gets the set of values associated to the key (which might be empty)
    pub fn get<Q>(&self, key: &Q) -> &HashSet<V>
    where
        K: std::borrow::Borrow<Q>,
        Q: Eq + Hash,
    {
        self.map.get(key).unwrap_or(&self.empty)
    }

    /// Adds a mapping from a key to a value.
    /// Returns whether the mapping was already added or if it already existed.
    pub fn add(&mut self, key: K, value: V) -> bool {
        let set = self.map.entry(key).or_default();
        set.insert(value)
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
    pub fn iter(&self) -> impl Iterator<Item = (&K, &HashSet<V>)> {
        self.map.iter()
    }

    /// Iterates over all values stored in this container
    pub fn iter_values(&self) -> impl Iterator<Item = &V> {
        self.map.iter().flat_map(|(_k, v_set)| v_set.iter())
    }

    pub fn contains_key<Q>(&self, k: &Q) -> bool
    where
        K: std::borrow::Borrow<Q>,
        Q: Hash + Eq,
    {
        self.map.get(k).is_some_and(|s| !s.is_empty())
    }
}

impl<K, V> Default for MultiMap<K, V>
where
    K: Eq + PartialEq + Hash,
    V: Eq + PartialEq + Hash,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> FromIterator<(K, V)> for MultiMap<K, V>
where
    K: Eq + PartialEq + Hash,
    V: Eq + PartialEq + Hash,
{
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
    {
        let mut result = Self::new();
        for (k, v) in iter {
            result.add(k, v);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use itertools::Itertools;

    use super::*;

    #[test]
    fn simple_lifecycle() {
        let mut multimap = MultiMap::new();

        assert!(multimap.get(&23).is_empty());
        assert!(multimap.add(23, 45));
        assert_eq!(multimap.get(&23).iter().copied().collect_vec(), vec![45]);
        assert!(!multimap.add(23, 45));
        assert_eq!(multimap.get(&23).iter().copied().collect_vec(), vec![45]);
        assert!(multimap.add(23, 67));

        let expected_slice = [45, 67];
        let expected_set = expected_slice.iter().collect::<HashSet<&i32>>();
        assert_eq!(
            multimap.get(&23).iter().collect::<HashSet<&i32>>(),
            expected_set
        );

        let full_set = multimap
            .iter_pairs()
            .map(|(x, y)| (*x, *y))
            .collect::<HashSet<(i32, i32)>>();
        let expected_full_set = vec![(23, 45), (23, 67)]
            .into_iter()
            .collect::<HashSet<(i32, i32)>>();
        assert_eq!(full_set, expected_full_set);
    }
}
