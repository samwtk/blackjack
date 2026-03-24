//! Session store: thread-safe HashMap of active game sessions with TTL expiry.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use uuid::Uuid;

use crate::game::state::GameState;

/// How long a session lives without activity before being reaped.
pub const SESSION_TTL: Duration = Duration::from_secs(30 * 60);

/// Thread-safe map of session_id → GameState.
#[derive(Clone)]
pub struct SessionStore {
    inner: Arc<Mutex<HashMap<Uuid, GameState>>>,
}

impl SessionStore {
    /// Create a new empty session store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Insert a new game state, returning the session ID.
    pub fn create(&self, gs: GameState) -> Uuid {
        let id = gs.session_id;
        if let Ok(mut map) = self.inner.lock() {
            map.insert(id, gs);
        }
        id
    }

    /// Run a closure with a mutable reference to the session. Returns None if not found.
    pub fn with_mut<F, R>(&self, id: Uuid, f: F) -> Option<R>
    where
        F: FnOnce(&mut GameState) -> R,
    {
        self.inner.lock().ok()?.get_mut(&id).map(f)
    }

    /// Run a closure with a shared reference to the session. Returns None if not found.
    pub fn with<F, R>(&self, id: Uuid, f: F) -> Option<R>
    where
        F: FnOnce(&GameState) -> R,
    {
        self.inner.lock().ok()?.get(&id).map(f)
    }

    /// Remove a session. Returns true if it existed.
    pub fn remove(&self, id: Uuid) -> bool {
        self.inner
            .lock()
            .ok()
            .and_then(|mut map| map.remove(&id))
            .is_some()
    }

    /// Remove all sessions whose last_activity exceeds SESSION_TTL.
    pub fn reap_expired(&self) {
        if let Ok(mut map) = self.inner.lock() {
            map.retain(|_, gs| gs.last_activity.elapsed() < SESSION_TTL);
        }
    }

    /// Spawn a background tokio task that calls reap_expired every 60 seconds.
    pub fn spawn_reaper(store: SessionStore) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                store.reap_expired();
            }
        });
    }
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new()
    }
}
