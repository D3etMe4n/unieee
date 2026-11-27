use crate::padded::CachePadded;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use thiserror::Error;

pub const DEFAULT_NUM_SHARDS: usize = 256;

type ShardBucket<K, V> = CachePadded<RwLock<HashMap<K, V>>>;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ShardConfigError {
    #[error("shard count must be a non-zero power of two, got {0}")]
    InvalidShardCount(usize),
}

#[derive(Debug)]
pub struct ShardedMap<K, V> {
    shards: Box<[ShardBucket<K, V>]>,
    shard_mask: usize,
}

impl<K, V> Default for ShardedMap<K, V>
where
    K: Eq + Hash,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> ShardedMap<K, V>
where
    K: Eq + Hash,
{
    #[must_use]
    pub fn new() -> Self {
        let mut shards = Vec::with_capacity(DEFAULT_NUM_SHARDS);
        for _ in 0..DEFAULT_NUM_SHARDS {
            shards.push(CachePadded::new(RwLock::new(HashMap::new())));
        }
        Self {
            shards: shards.into_boxed_slice(),
            shard_mask: DEFAULT_NUM_SHARDS - 1,
        }
    }

    pub fn with_shard_count(num_shards: usize) -> Result<Self, ShardConfigError> {
        if num_shards == 0 || !num_shards.is_power_of_two() {
            return Err(ShardConfigError::InvalidShardCount(num_shards));
        }
        let mut shards = Vec::with_capacity(num_shards);
        for _ in 0..num_shards {
            shards.push(CachePadded::new(RwLock::new(HashMap::new())));
        }
        Ok(Self {
            shards: shards.into_boxed_slice(),
            shard_mask: num_shards - 1,
        })
    }

    #[must_use]
    pub fn shard_count(&self) -> usize {
        self.shards.len()
    }

    #[must_use]
    pub fn shard_index(&self, key: &K) -> usize {
        fnv_shard_index(key, self.shard_mask)
    }

    pub fn insert(&self, key: K, value: V) -> Option<V> {
        let idx = self.shard_index(&key);
        let mut guard = self.shards[idx].write();
        guard.insert(key, value)
    }

    pub fn remove(&self, key: &K) -> Option<V> {
        let idx = self.shard_index(key);
        let mut guard = self.shards[idx].write();
        guard.remove(key)
    }

    /// Atomically removes `key` only when `predicate` returns `true` for the stored value.
    pub fn remove_if(&self, key: &K, predicate: impl FnOnce(&V) -> bool) -> Option<V> {
        let idx = self.shard_index(key);
        let mut guard = self.shards[idx].write();
        if guard.get(key).is_some_and(predicate) {
            guard.remove(key)
        } else {
            None
        }
    }

    #[must_use]
    pub fn contains_key(&self, key: &K) -> bool {
        let idx = self.shard_index(key);
        let guard = self.shards[idx].read();
        guard.contains_key(key)
    }

    pub fn get_cloned(&self, key: &K) -> Option<V>
    where
        V: Clone,
    {
        let idx = self.shard_index(key);
        let guard = self.shards[idx].read();
        guard.get(key).cloned()
    }

    pub fn with_read<R>(&self, key: &K, f: impl FnOnce(Option<&V>) -> R) -> R {
        let idx = self.shard_index(key);
        let guard = self.shards[idx].read();
        f(guard.get(key))
    }

    pub fn with_write<R>(&self, key: &K, f: impl FnOnce(Option<&mut V>) -> R) -> R {
        let idx = self.shard_index(key);
        let mut guard = self.shards[idx].write();
        f(guard.get_mut(key))
    }

    pub fn with_entry<R>(
        &self,
        key: K,
        default: impl FnOnce() -> V,
        f: impl FnOnce(&mut V) -> R,
    ) -> R {
        let idx = self.shard_index(&key);
        let mut guard = self.shards[idx].write();
        let entry = guard.entry(key).or_insert_with(default);
        f(entry)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.shards.iter().map(|s| s.read().len()).sum()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.shards.iter().all(|s| s.read().is_empty())
    }

    pub fn clear(&self) {
        for shard in self.shards.iter() {
            shard.write().clear();
        }
    }

    pub fn for_each_read(&self, mut f: impl FnMut(&K, &V)) {
        for shard in self.shards.iter() {
            let guard = shard.read();
            for (k, v) in guard.iter() {
                f(k, v);
            }
        }
    }

    pub fn for_each_write(&self, mut f: impl FnMut(&K, &mut V)) {
        for shard in self.shards.iter() {
            let mut guard = shard.write();
            for (k, v) in guard.iter_mut() {
                f(k, v);
            }
        }
    }

    /// Retains only entries matching `f`, locking one shard at a time, and returns count evicted.
    pub fn retain(&self, mut f: impl FnMut(&K, &mut V) -> bool) -> usize {
        let mut evicted = 0usize;
        for shard in self.shards.iter() {
            let mut guard = shard.write();
            let before = guard.len();
            guard.retain(|k, v| f(k, v));
            evicted = evicted.saturating_add(before.saturating_sub(guard.len()));
        }
        evicted
    }
}

pub(crate) fn fnv_shard_index<K: Hash>(key: &K, shard_mask: usize) -> usize {
    let mut hasher = FnvHasher::default();
    key.hash(&mut hasher);
    (hasher.finish() as usize) & shard_mask
}

struct FnvHasher(u64);

impl Default for FnvHasher {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Hasher for FnvHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        const PRIME: u64 = 0x0100_0000_01b3;
        for &byte in bytes {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(PRIME);
        }
    }
}
