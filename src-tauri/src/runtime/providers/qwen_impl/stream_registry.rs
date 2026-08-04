use serde::Serialize;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, Serialize)]
pub struct ActiveStreamSnapshot {
    pub completion_id: String,
    pub chat_id: String,
    pub response_id: Option<String>,
    pub account_id: String,
    pub created_at: u64,
}

#[derive(Clone)]
pub struct ActiveStreamHandle {
    pub snapshot: ActiveStreamSnapshot,
    pub headers: HashMap<String, String>,
    pub cancel: CancellationToken,
}

#[derive(Clone)]
struct ActiveStreamEntry {
    snapshot: ActiveStreamSnapshot,
    headers: HashMap<String, String>,
    cancel: CancellationToken,
    created_at_instant: std::time::Instant,
}

#[derive(Clone, Default)]
pub struct StreamRegistry {
    inner: Arc<Mutex<HashMap<String, ActiveStreamEntry>>>,
}

impl StreamRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn register(
        &self,
        completion_id: String,
        chat_id: String,
        account_id: String,
        headers: HashMap<String, String>,
    ) -> CancellationToken {
        let token = CancellationToken::new();
        let entry = ActiveStreamEntry {
            snapshot: ActiveStreamSnapshot {
                completion_id: completion_id.clone(),
                chat_id,
                response_id: None,
                account_id,
                created_at: crate::proxy_core::current_timestamp(),
            },
            headers,
            cancel: token.clone(),
            created_at_instant: std::time::Instant::now(),
        };
        self.inner.lock().await.insert(completion_id, entry);
        token
    }

    pub async fn update_response_id(&self, completion_id: &str, response_id: String) {
        if let Some(entry) = self.inner.lock().await.get_mut(completion_id) {
            entry.snapshot.response_id = Some(response_id);
        }
    }

    pub async fn get_by_completion_id(&self, completion_id: &str) -> Option<ActiveStreamHandle> {
        let guard = self.inner.lock().await;
        let entry = guard.get(completion_id)?.clone();
        Some(ActiveStreamHandle {
            snapshot: entry.snapshot,
            headers: entry.headers,
            cancel: entry.cancel,
        })
    }

    pub async fn get_by_chat_id(&self, chat_id: &str) -> Option<ActiveStreamHandle> {
        let guard = self.inner.lock().await;
        let entry = guard
            .values()
            .find(|entry| entry.snapshot.chat_id == chat_id)?
            .clone();
        Some(ActiveStreamHandle {
            snapshot: entry.snapshot,
            headers: entry.headers,
            cancel: entry.cancel,
        })
    }

    pub async fn remove_by_completion_id(&self, completion_id: &str) {
        self.inner.lock().await.remove(completion_id);
    }

    pub async fn active_count(&self) -> usize {
        self.inner.lock().await.len()
    }

    pub async fn snapshots(&self) -> Vec<ActiveStreamSnapshot> {
        self.inner
            .lock()
            .await
            .values()
            .map(|entry| entry.snapshot.clone())
            .collect()
    }

    pub async fn prune_older_than(&self, max_age: Duration) -> usize {
        let mut guard = self.inner.lock().await;
        let now = std::time::Instant::now();
        let keys = guard
            .iter()
            .filter(|(_, entry)| now.duration_since(entry.created_at_instant) > max_age)
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();

        for key in &keys {
            if let Some(entry) = guard.remove(key) {
                entry.cancel.cancel();
            }
        }

        keys.len()
    }

    /* RAII cleanup for a registered stream. Held inside the streaming generator so
    the slot is freed even when the client disconnects early and the generator's
    explicit remove never runs — cleanup can't be forgotten, only the prune
    fallback would otherwise reap it. */
    pub fn guard(&self, completion_id: impl Into<String>) -> ActiveStreamGuard {
        ActiveStreamGuard {
            registry: self.clone(),
            completion_id: completion_id.into(),
        }
    }
}

pub struct ActiveStreamGuard {
    registry: StreamRegistry,
    completion_id: String,
}

impl Drop for ActiveStreamGuard {
    fn drop(&mut self) {
        let completion_id = std::mem::take(&mut self.completion_id);
        /* remove is async but Drop is sync: take the lock without blocking when we
        can (the common case — registry locks are brief), else hand the async
        remove to the runtime. Removing an already-gone slot is a no-op, so the
        normal-completion path that already removed stays correct. */
        if let Ok(mut map) = self.registry.inner.try_lock() {
            map.remove(&completion_id);
            return;
        }
        let registry = self.registry.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                registry.remove_by_completion_id(&completion_id).await;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn guard_frees_slot_when_dropped() {
        let registry = StreamRegistry::new();
        registry
            .register(
                "c1".to_owned(),
                "chat".to_owned(),
                "acct".to_owned(),
                HashMap::new(),
            )
            .await;
        assert_eq!(registry.active_count().await, 1);
        {
            let _guard = registry.guard("c1");
        }
        /* try_lock succeeds with no other holder, so Drop removes synchronously */
        assert_eq!(registry.active_count().await, 0);
    }

    #[tokio::test]
    async fn guard_drop_after_explicit_remove_is_a_noop() {
        let registry = StreamRegistry::new();
        registry
            .register(
                "c2".to_owned(),
                "chat".to_owned(),
                "acct".to_owned(),
                HashMap::new(),
            )
            .await;
        let guard = registry.guard("c2");
        registry.remove_by_completion_id("c2").await;
        drop(guard);
        assert_eq!(registry.active_count().await, 0);
    }
}
