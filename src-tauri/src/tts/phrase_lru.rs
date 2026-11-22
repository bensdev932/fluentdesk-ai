//! Bounded LRU key/chunk store helper for `TtsPhraseCache`.

use crate::domain::AudioChunk;
use std::collections::{HashMap, VecDeque};

pub(super) type CacheKey = (String, u32);

#[derive(Default)]
pub(super) struct PhraseLru {
    capacity: usize,
    pub(super) entries: HashMap<CacheKey, AudioChunk>,
    order: VecDeque<CacheKey>,
}

impl PhraseLru {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            ..Self::default()
        }
    }

    pub(super) fn get(&mut self, key: &CacheKey) -> Option<AudioChunk> {
        let chunk = self.entries.get(key)?.clone();
        self.order.retain(|k| k != key);
        self.order.push_back(key.clone());
        Some(chunk)
    }

    pub(super) fn insert(&mut self, key: CacheKey, chunk: AudioChunk) {
        if self.entries.contains_key(&key) {
            self.order.retain(|k| k != &key);
        } else if self.entries.len() >= self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
        self.order.push_back(key.clone());
        self.entries.insert(key, chunk);
    }
}
