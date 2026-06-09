use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use uuid::Uuid;

#[cfg(feature = "standalone-provider-cli")]
use anyhow::anyhow;
#[cfg(feature = "standalone-provider-cli")]
use rusqlite::OptionalExtension;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct QwenAccount {
    pub id: String,
    pub email: String,
    pub password: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

impl QwenAccount {
    pub fn masked(&self) -> Self {
        Self {
            id: self.id.clone(),
            email: self.email.clone(),
            password: if self.password.is_empty() {
                String::new()
            } else {
                "***".to_owned()
            },
            created_at: self.created_at.clone(),
        }
    }
}

pub fn global_account() -> QwenAccount {
    QwenAccount {
        id: "global".to_owned(),
        email: "default-profile".to_owned(),
        password: String::new(),
        created_at: None,
    }
}

#[derive(Clone)]
pub struct AccountStore {
    db_path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct LegacyAccount {
    id: Option<String>,
    email: String,
    #[serde(default)]
    password: String,
}

impl AccountStore {
    pub fn new(
        db_path: PathBuf,
        legacy_db_candidates: &[PathBuf],
        legacy_json_candidates: &[PathBuf],
    ) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent)?;
        }

        if !db_path.exists() {
            if let Some(found) = legacy_db_candidates.iter().find(|path| path.exists()) {
                fs::copy(found, &db_path).with_context(|| {
                    format!(
                        "failed to import legacy qwen database from {}",
                        found.display()
                    )
                })?;
            }
        }

        let store = Self { db_path };
        store.initialize(legacy_json_candidates)?;
        Ok(store)
    }

    fn open(&self) -> Result<Connection> {
        let connection = Connection::open(&self.db_path)
            .with_context(|| format!("failed to open {}", self.db_path.display()))?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "busy_timeout", 5000)?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        Ok(connection)
    }

    fn initialize(&self, legacy_json_candidates: &[PathBuf]) -> Result<()> {
        let connection = self.open()?;
        connection.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS accounts (
              id TEXT PRIMARY KEY,
              email TEXT UNIQUE NOT NULL,
              password TEXT NOT NULL DEFAULT '',
              created_at TEXT NOT NULL DEFAULT (datetime('now')),
              updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_accounts_email ON accounts(email);
            "#,
        )?;

        let mut insert = connection
            .prepare("INSERT OR IGNORE INTO accounts (id, email, password) VALUES (?1, ?2, ?3)")?;

        for path in legacy_json_candidates {
            if !path.exists() {
                continue;
            }

            let raw = match fs::read_to_string(path) {
                Ok(raw) => raw,
                Err(_) => continue,
            };

            let parsed: Vec<LegacyAccount> = match serde_json::from_str(&raw) {
                Ok(parsed) => parsed,
                Err(_) => {
                    let backup = path.with_extension("json.bak");
                    let _ = fs::rename(path, backup);
                    continue;
                }
            };

            for account in parsed {
                if account.email.trim().is_empty() {
                    continue;
                }
                insert.execute(params![
                    account.id.unwrap_or_else(|| Uuid::new_v4().to_string()),
                    account.email.trim(),
                    account.password
                ])?;
            }

            let backup = path.with_extension("json.bak");
            let _ = fs::rename(path, backup);
        }

        Ok(())
    }

    #[cfg(feature = "standalone-provider-cli")]
    pub fn add_account(
        &self,
        email: &str,
        password: &str,
        id: Option<&str>,
    ) -> Result<QwenAccount> {
        let email = email.trim();
        if email.is_empty() {
            return Err(anyhow!("email is required"));
        }

        let connection = self.open()?;
        let existing: Option<String> = connection
            .query_row(
                "SELECT id FROM accounts WHERE email = ?1",
                params![email],
                |row| row.get(0),
            )
            .optional()?;
        if existing.is_some() {
            return Err(anyhow!("account with email {email} already exists"));
        }

        let id = id
            .map(str::to_owned)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        connection.execute(
            "INSERT INTO accounts (id, email, password) VALUES (?1, ?2, ?3)",
            params![id, email, password],
        )?;

        Ok(QwenAccount {
            id,
            email: email.to_owned(),
            password: password.to_owned(),
            created_at: None,
        })
    }

    #[cfg(feature = "standalone-provider-cli")]
    pub fn remove_account(&self, id: &str) -> Result<bool> {
        let connection = self.open()?;
        let changed = connection.execute("DELETE FROM accounts WHERE id = ?1", params![id])?;
        Ok(changed > 0)
    }

    pub fn list_accounts(&self) -> Result<Vec<QwenAccount>> {
        let connection = self.open()?;
        let mut statement = connection.prepare(
            "SELECT id, email, password, created_at FROM accounts ORDER BY created_at ASC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(QwenAccount {
                id: row.get(0)?,
                email: row.get(1)?,
                password: row.get(2)?,
                created_at: row.get(3).ok(),
            })
        })?;

        let mut accounts = Vec::new();
        for row in rows {
            accounts.push(row?);
        }
        Ok(accounts)
    }

    pub fn list_masked_accounts(&self) -> Result<Vec<QwenAccount>> {
        Ok(self
            .list_accounts()?
            .into_iter()
            .map(|account| account.masked())
            .collect())
    }

    #[cfg(feature = "standalone-provider-cli")]
    pub fn get_account(&self, id: &str) -> Result<Option<QwenAccount>> {
        let connection = self.open()?;
        let account = connection
            .query_row(
                "SELECT id, email, password, created_at FROM accounts WHERE id = ?1",
                params![id],
                |row| {
                    Ok(QwenAccount {
                        id: row.get(0)?,
                        email: row.get(1)?,
                        password: row.get(2)?,
                        created_at: row.get(3).ok(),
                    })
                },
            )
            .optional()?;
        Ok(account)
    }

    pub fn count(&self) -> Result<usize> {
        let connection = self.open()?;
        let count: i64 =
            connection.query_row("SELECT COUNT(*) FROM accounts", [], |row| row.get(0))?;
        Ok(count.max(0) as usize)
    }
}
