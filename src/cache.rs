use crate::models::user::User;
use dashmap::DashMap;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use uuid::Uuid;

const TTL: Duration = Duration::from_secs(60);

#[derive(Debug)]
struct Entry {
    data: Vec<u8>,
    created_at: Instant,
}

/// In-process cache for serialized query responses.
///
/// Keys are SHA-256 hashes of the normalized query parameter set.
/// Entries expire after 60 seconds. The entire cache is cleared on any
/// write (insert, delete, import) to prevent stale reads.
///
/// This is intentionally cheap: `DashMap` is sharded so concurrent reads
/// do not contend, and the `Arc` makes cloning the handle free.
#[derive(Clone, Debug)]
pub struct QueryCache {
    store: Arc<DashMap<String, Entry>>,
}

impl QueryCache {
    pub fn new() -> Self {
        Self {
            store: Arc::new(DashMap::new()),
        }
    }

    /// Returns the cached response bytes if the entry exists and has not expired.
    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        let entry = self.store.get(key)?;
        if entry.created_at.elapsed() >= TTL {
            return None;
        }
        Some(entry.data.clone())
    }

    /// Stores a serialized response under the given key.
    pub fn set(&self, key: String, data: Vec<u8>) {
        self.store.insert(
            key,
            Entry {
                data,
                created_at: Instant::now(),
            },
        );
    }

    /// Clears all entries. Called after any write operation.
    pub fn clear(&self) {
        self.store.clear();
    }
}

impl Default for QueryCache {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
struct AuthEntry {
    user: User,
    created_at: Instant,
}

/// In-process cache for authenticated users to avoid repeating database
/// lookups on every single authenticated request. Entries expire after 60s.
#[derive(Clone, Debug)]
pub struct AuthCache {
    store: Arc<DashMap<Uuid, AuthEntry>>,
}

impl AuthCache {
    pub fn new() -> Self {
        Self {
            store: Arc::new(DashMap::new()),
        }
    }

    pub fn get(&self, user_id: &Uuid) -> Option<User> {
        let entry = self.store.get(user_id)?;
        if entry.created_at.elapsed() >= TTL {
            return None;
        }
        Some(entry.user.clone())
    }

    pub fn set(&self, user_id: Uuid, user: User) {
        self.store.insert(
            user_id,
            AuthEntry {
                user,
                created_at: Instant::now(),
            },
        );
    }
}

impl Default for AuthCache {
    fn default() -> Self {
        Self::new()
    }
}
