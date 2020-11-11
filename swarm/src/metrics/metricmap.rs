use crossbeam_epoch::{Atomic, Owned};
use smallvec::alloc::fmt::{Debug, Display, Formatter};
use std::collections::hash_map::IntoIter;
use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;
use std::option::Option::Some;
use std::sync::atomic::Ordering::SeqCst;

/// MetricMap is a lock-free hash map that supports concurrent operations.
pub struct MetricMap<K, V> {
    data: Atomic<HashMap<K, V>>,
}

impl<K, V> fmt::Debug for MetricMap<K, V> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("MetricMap").field("data", &self.data).finish()
    }
}

impl<K, V> Default for MetricMap<K, V>
where
    K: Eq + Hash + Clone + Debug,
    V: Clone + Display,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> MetricMap<K, V>
where
    K: Eq + Hash + Clone + Debug,
    V: Clone + Display,
{
    /// Create a new MetricMap
    pub fn new() -> Self {
        MetricMap {
            data: Atomic::new(HashMap::<K, V>::new()),
        }
    }

    /// If map contains key, replaces original value with the result that return by F.
    /// Otherwise, create a new key-value and insert.
    pub fn store_or_modify<F: Fn(&K, &V) -> V>(&self, key: &K, value: V, on_modify: F) {
        let guard = crossbeam_epoch::pin();

        loop {
            let shared = self.data.load(SeqCst, &guard);

            let mut new_hash = HashMap::new();

            match unsafe { shared.as_ref() } {
                Some(old_hash) => {
                    new_hash = old_hash.clone();
                    if let Some(old_value) = new_hash.get(key) {
                        let new_value = on_modify(key, old_value);
                        new_hash.insert(key.clone(), new_value.clone());
                    } else {
                        new_hash.insert(key.clone(), value.clone());
                    }
                }
                None => {
                    new_hash.insert(key.clone(), value.clone());
                }
            }

            let owned = Owned::new(new_hash);

            match self.data.compare_and_set(shared, owned, SeqCst, &guard) {
                Ok(_) => {
                    unsafe {
                        guard.defer_destroy(shared);
                        break;
                    }
                    // break;
                }
                Err(_e) => {}
            }
        }
    }

    pub fn load(&self, key: &K) -> Option<V> {
        let guard = crossbeam_epoch::pin();

        let shared = self.data.load(SeqCst, &guard);

        let hmap = unsafe { shared.as_ref().unwrap() };

        match hmap.get(key) {
            Some(value) => Some(value.clone()),
            None => None,
        }
    }

    #[cfg(test)]
    pub fn delete(&self, key: K) {
        let guard = crossbeam_epoch::pin();

        loop {
            let shared = self.data.load(SeqCst, &guard);

            let old_hash = unsafe { shared.as_ref().unwrap() };

            let mut new_hash = HashMap::new();
            for (k, v) in old_hash {
                if k.clone() == key {
                    continue;
                }
                new_hash.insert(k.clone(), v.clone());
            }

            let owned = Owned::new(new_hash);

            match self.data.compare_and_set(shared, owned, SeqCst, &guard) {
                Ok(_) => unsafe {
                    guard.defer_destroy(shared);
                    break;
                },
                Err(_e) => {
                    // Err(io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
                }
            }
        }
    }

    /// Return an iterator
    pub fn iterator(&self) -> Option<IntoIter<K, V>> {
        let guard = crossbeam_epoch::pin();

        let shared = self.data.load(SeqCst, &guard);

        match unsafe { shared.as_ref() } {
            Some(map) => Some(map.clone().into_iter()),
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::metrics::metricmap::MetricMap;
    use async_std::task;
    use smallvec::alloc::sync::Arc;
    use std::ops::Add;

    #[test]
    pub fn test_store_and_modify() {
        let key = String::from("abc");
        let map = Arc::new(MetricMap::new());
        task::block_on(async {
            let inside_future_map = map.clone();
            for index in 0..16 {
                let k = key.clone();
                let inside_map = inside_future_map.clone();

                task::spawn(async move { inside_map.store_or_modify(&k, index, |_, value| value.add(index)) }).await;
            }
        });

        assert_eq!(map.load(&key), Some(120))
    }

    #[test]
    pub fn test_delete() {
        let key = String::from("abc");
        let map = Arc::new(MetricMap::new());

        task::block_on(async {
            let delete_map = map.clone();
            for index in 0..18 {
                let k = key.clone();
                let inside_map = delete_map.clone();

                task::spawn(async move { inside_map.store_or_modify(&k, index, |_, value| value.add(index)) }).await;
            }

            map.delete(key.clone());

            assert_eq!(map.load(&key), None);

            for index in 0..20 {
                let inside_map = delete_map.clone();
                let k = key.clone();
                task::spawn(async move { inside_map.store_or_modify(&k, index, |_, value| value.add(index)) }).await;
            }
        });

        map.delete(key.clone());

        assert_eq!(map.load(&key), None)
    }
}
