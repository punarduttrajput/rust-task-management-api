use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

/// Per-process TTL cache. Limitation: not shared between instances and lost on
/// restart; a Redis-backed type with the same get/set/invalidate API would replace it.
pub struct TtlCache<V> {
    ttl: Duration,
    entries: RwLock<HashMap<String, (Instant, V)>>,
}

impl<V: Clone> TtlCache<V> {
    pub fn new(ttl: Duration) -> Self {
        Self { ttl, entries: RwLock::new(HashMap::new()) }
    }

    pub async fn get(&self, key: &str) -> Option<V> {
        let entries = self.entries.read().await;
        entries
            .get(key)
            .filter(|(expires, _)| *expires > Instant::now())
            .map(|(_, v)| v.clone())
    }

    pub async fn set(&self, key: String, value: V) {
        self.entries.write().await.insert(key, (Instant::now() + self.ttl, value));
    }

    pub async fn invalidate(&self, key: &str) {
        self.entries.write().await.remove(key);
    }
}

pub fn my_tasks_key(user_id: &str) -> String {
    format!("my_tasks:{user_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn get_set_invalidate_and_expire() {
        let cache = TtlCache::new(Duration::from_millis(50));
        cache.set("k".into(), 1).await;
        assert_eq!(cache.get("k").await, Some(1));
        cache.invalidate("k").await;
        assert_eq!(cache.get("k").await, None);
        cache.set("k".into(), 2).await;
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert_eq!(cache.get("k").await, None);
    }
}
