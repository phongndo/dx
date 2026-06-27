use std::{collections::HashMap, hash::Hash};

#[derive(Debug)]
pub(crate) struct LruCache<K, V> {
    entries: HashMap<K, LruEntry<V>>,
    capacity: usize,
    tick: u64,
}

#[derive(Debug)]
struct LruEntry<V> {
    value: V,
    last_used: u64,
}

impl<K, V> LruCache<K, V>
where
    K: Copy + Eq + Hash,
{
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            capacity,
            tick: 0,
        }
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &V> {
        self.entries.values().map(|entry| &entry.value)
    }

    pub(crate) fn contains_key(&self, key: &K) -> bool {
        self.entries.contains_key(key)
    }

    pub(crate) fn insert(&mut self, key: K, value: V) {
        if self.capacity == 0 {
            return;
        }

        let last_used = self.next_tick();

        if let Some(entry) = self.entries.get_mut(&key) {
            entry.value = value;
            entry.last_used = last_used;
            return;
        }

        if self.entries.len() >= self.capacity
            && let Some(oldest) = self.oldest_key()
        {
            self.entries.remove(&oldest);
        }

        self.entries.insert(key, LruEntry { value, last_used });
    }

    pub(crate) fn get(&mut self, key: &K) -> Option<&V> {
        let last_used = self.next_tick();
        let entry = self.entries.get_mut(key)?;
        entry.last_used = last_used;
        Some(&entry.value)
    }

    pub(crate) fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        let last_used = self.next_tick();
        let entry = self.entries.get_mut(key)?;
        entry.last_used = last_used;
        Some(&mut entry.value)
    }

    fn next_tick(&mut self) -> u64 {
        self.tick = self.tick.wrapping_add(1);
        self.tick
    }

    fn oldest_key(&self) -> Option<K> {
        self.entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_used)
            .map(|(key, _)| *key)
    }
}
