use std::collections::HashMap;
use std::time::{Duration, SystemTime};

struct CacheEntry<T> {
    timestamp: usize,
    ttl: usize,
    item: T,
}

impl<T> CacheEntry<T> {
    fn new(item: T, ttl: usize) -> Self {
        CacheEntry {
            item,
            ttl,
            timestamp: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs() as usize,
        }
    }

    fn is_expired(&self) -> bool {
        let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs() as usize;
        self.timestamp <= now && now < self.timestamp + self.ttl
    }
}

pub struct Cache<T> {
    entries: HashMap<String, CacheEntry<T>>
}

impl<T> Cache<T> {
    pub fn new() -> Cache<T> {
        Cache {
            entries: HashMap::new()
        }
    }

    pub fn insert(&mut self, key: String, value: T) {
        self.insert_with_ttl(key, value, None);
    }

    pub fn insert_with_ttl(&mut self, key: String, value: T, ttl: Option<usize>) {
        let entry = CacheEntry::new(value, ttl.unwrap_or(300));
        self.entries.insert(key, entry);
    }

    pub fn get(&self, key: &str) -> Option<&T> {
        let entry = self.entries.get(key);
        entry.filter(|x| x.is_expired()).map(|x| &x.item)
    }
}