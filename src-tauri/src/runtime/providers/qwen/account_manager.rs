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
        state.cooldowns.insert(
            account_id.to_owned(),
            CooldownEntry {
                until: std::time::Instant::now()
                    + Duration::from_millis(cooldown_ms.unwrap_or(DEFAULT_COOLDOWN_MS)),
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

        for offset in 0..accounts.len() {
            let index = (state.current_index + offset) % accounts.len();
            let account = &accounts[index];
            if skip_account_id == Some(account.id.as_str()) {
                continue;
            }
            if !state.cooldowns.contains_key(&account.id) {
                state.current_index = (index + 1) % accounts.len();
                return Some(account.clone());
            }
        }

        let mut best: Option<(QwenAccount, u64)> = None;
        for account in accounts {
            if skip_account_id == Some(account.id.as_str()) {
                continue;
            }
            if let Some(entry) = state.cooldowns.get(&account.id) {
                let remaining = entry
                    .until
                    .saturating_duration_since(std::time::Instant::now())
                    .as_millis() as u64;
                match &best {
                    Some((_, best_remaining)) if *best_remaining <= remaining => {}
                    _ => best = Some((account.clone(), remaining)),
                }
            }
        }

        best.map(|(account, _)| account)
    }
}
