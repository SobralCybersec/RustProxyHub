use super::accounts::QwenAccount;
use serde::Serialize;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::Mutex;

const DEFAULT_COOLDOWN_MS: u64 = 3 * 60 * 1000;

#[derive(Clone, Debug, Serialize)]
pub struct CooldownInfo {
    pub on_cooldown: bool,
    pub remaining_ms: u64,
    pub reason: String,
}

#[derive(Clone, Debug)]
struct CooldownEntry {
    until: std::time::Instant,
    reason: String,
}

#[derive(Default)]
struct AccountManagerState {
    cooldowns: HashMap<String, CooldownEntry>,
    active: HashMap<String, usize>,
    current_index: usize,
}

#[derive(Clone, Default)]
pub struct AccountManager {
    inner: Arc<Mutex<AccountManagerState>>,
}

impl AccountManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn mark_rate_limited(
        &self,
        account_id: &str,
        cooldown_ms: Option<u64>,
        reason: &str,
    ) {
        let mut state = self.inner.lock().await;
        let until = std::time::Instant::now()
            + Duration::from_millis(cooldown_ms.unwrap_or(DEFAULT_COOLDOWN_MS));
        if state
            .cooldowns
            .get(account_id)
            .is_some_and(|existing| existing.until >= until)
        {
            return;
        }
        state.cooldowns.insert(
            account_id.to_owned(),
            CooldownEntry {
                until,
                reason: reason.to_owned(),
            },
        );
    }

    pub async fn get_cooldown_info(&self, account_id: &str) -> Option<CooldownInfo> {
        let mut state = self.inner.lock().await;
        let entry = state.cooldowns.get(account_id)?.clone();
        let remaining = entry
            .until
            .saturating_duration_since(std::time::Instant::now())
            .as_millis() as u64;
        if remaining == 0 {
            state.cooldowns.remove(account_id);
            return None;
        }
        Some(CooldownInfo {
            on_cooldown: true,
            remaining_ms: remaining,
            reason: entry.reason,
        })
    }

    pub async fn cooldown_status(&self) -> HashMap<String, CooldownInfo> {
        let keys = {
            let state = self.inner.lock().await;
            state.cooldowns.keys().cloned().collect::<Vec<_>>()
        };

        let mut result = HashMap::new();
        for key in keys {
            if let Some(info) = self.get_cooldown_info(&key).await {
                result.insert(key, info);
            }
        }
        result
    }

    pub async fn lease(&self, account_id: impl Into<String>) -> AccountLease {
        let account_id = account_id.into();
        let mut state = self.inner.lock().await;
        *state.active.entry(account_id.clone()).or_default() += 1;
        AccountLease {
            manager: self.clone(),
            account_id,
        }
    }

    pub async fn active_status(&self) -> HashMap<String, usize> {
        self.inner.lock().await.active.clone()
    }

    async fn release(&self, account_id: &str) {
        let mut state = self.inner.lock().await;
        let Some(active) = state.active.get_mut(account_id) else {
            return;
        };
        *active = active.saturating_sub(1);
        if *active == 0 {
            state.active.remove(account_id);
        }
    }

    pub async fn select_next(
        &self,
        accounts: &[QwenAccount],
        force_reset: bool,
    ) -> Option<QwenAccount> {
        self.select_inner(accounts, force_reset, None).await
    }

    pub async fn select_next_available(
        &self,
        accounts: &[QwenAccount],
        skip_account_id: Option<&str>,
    ) -> Option<QwenAccount> {
        self.select_inner(accounts, false, skip_account_id).await
    }

    async fn select_inner(
        &self,
        accounts: &[QwenAccount],
        force_reset: bool,
        skip_account_id: Option<&str>,
    ) -> Option<QwenAccount> {
        if accounts.is_empty() {
            return None;
        }

        let mut state = self.inner.lock().await;
        if force_reset {
            state.current_index = 0;
        }

        state
            .cooldowns
            .retain(|_, entry| entry.until > std::time::Instant::now());

        let mut selected: Option<(usize, usize)> = None;
        for offset in 0..accounts.len() {
            let index = (state.current_index + offset) % accounts.len();
            let account = &accounts[index];
            if skip_account_id == Some(account.id.as_str()) {
                continue;
            }
            if !state.cooldowns.contains_key(&account.id) {
                let active = state.active.get(&account.id).copied().unwrap_or_default();
                if selected.is_none_or(|(_, selected_active)| active < selected_active) {
                    selected = Some((index, active));
                    if active == 0 {
                        break;
                    }
                }
            }
        }
        let (index, _) = selected?;
        state.current_index = (index + 1) % accounts.len();
        Some(accounts[index].clone())
    }
}

pub struct AccountLease {
    manager: AccountManager,
    account_id: String,
}

impl Drop for AccountLease {
    fn drop(&mut self) {
        let account_id = std::mem::take(&mut self.account_id);
        if account_id.is_empty() {
            return;
        }
        if let Ok(mut state) = self.manager.inner.try_lock() {
            let Some(active) = state.active.get_mut(&account_id) else {
                return;
            };
            *active = active.saturating_sub(1);
            if *active == 0 {
                state.active.remove(&account_id);
            }
            return;
        }
        let manager = self.manager.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move { manager.release(&account_id).await });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::accounts::QwenAccount;
    use super::AccountManager;

    fn account(id: &str) -> QwenAccount {
        QwenAccount {
            id: id.to_owned(),
            email: format!("{id}@example.test"),
            password: String::new(),
            created_at: None,
        }
    }

    #[tokio::test]
    async fn skips_rate_limited_accounts_and_returns_none_when_all_are_cooling_down() {
        let manager = AccountManager::new();
        let accounts = vec![account("one"), account("two")];
        manager
            .mark_rate_limited("one", Some(60_000), "RateLimited")
            .await;
        assert_eq!(
            manager.select_next(&accounts, true).await.unwrap().id,
            "two"
        );

        manager
            .mark_rate_limited("two", Some(60_000), "RateLimited")
            .await;
        assert!(manager.select_next(&accounts, false).await.is_none());
    }

    #[tokio::test]
    async fn never_shortens_an_existing_cooldown() {
        let manager = AccountManager::new();
        manager
            .mark_rate_limited("one", Some(60_000), "RateLimited")
            .await;
        manager
            .mark_rate_limited("one", Some(1), "ServerError")
            .await;

        let info = manager.get_cooldown_info("one").await.unwrap();
        assert_eq!(info.reason, "RateLimited");
        assert!(info.remaining_ms > 30_000);
    }

    #[tokio::test]
    async fn selects_an_idle_account_before_a_leased_one() {
        let manager = AccountManager::new();
        let accounts = vec![account("one"), account("two")];
        let lease = manager.lease("one").await;

        assert_eq!(
            manager.select_next(&accounts, true).await.unwrap().id,
            "two"
        );
        assert_eq!(manager.active_status().await.get("one"), Some(&1));
        drop(lease);
        assert!(manager.active_status().await.is_empty());
    }
}
