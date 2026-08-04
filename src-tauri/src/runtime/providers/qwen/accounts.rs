use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use uuid::Uuid;

/* Every SQL statement for the account store, gathered here so the schema and each
query read at a glance instead of scattered across the call sites below. Bound
arguments (params![...]) stay at the call sites since they are runtime values. */
mod queries {
    pub const INIT_SCHEMA: &str = r#"
        CREATE TABLE IF NOT EXISTS accounts (
          id TEXT PRIMARY KEY,
          email TEXT UNIQUE NOT NULL,
          password TEXT NOT NULL DEFAULT '',
          created_at TEXT NOT NULL DEFAULT (datetime('now')),
          updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_accounts_email ON accounts(email);
        "#;
    pub const INSERT_IGNORE: &str =
        "INSERT OR IGNORE INTO accounts (id, email, password) VALUES (?1, ?2, ?3)";
    pub const LIST: &str =
        "SELECT id, email, password, created_at FROM accounts ORDER BY created_at ASC";
    pub const COUNT: &str = "SELECT COUNT(*) FROM accounts";
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct QwenAccount {
    pub id: String,
    pub email: String,
    // ponytail: stored plaintext in the local app-data sqlite; encrypting the column
    // would need an OS keychain dependency. We at least never serialize it to JSON
    // so it cannot leak via IPC or admin/status responses.
    #[serde(skip_serializing)]
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

        // ponytail: passwords are plaintext in sqlite; lock the file to 0600 so
        // other local users can't read it. Full fix: encrypt the column with an
        // OS-keychain-backed key (add `keyring` crate when ready).
        #[cfg(unix)]
        Self::enforce_db_perms(&db_path);

        let store = Self { db_path };
        store.initialize(legacy_json_candidates)?;
        Ok(store)
    }

    #[cfg(unix)]
    fn enforce_db_perms(db_path: &std::path::Path) {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(db_path) {
            let mut perms = meta.permissions();
            perms.set_mode(0o600);
            let _ = fs::set_permissions(db_path, perms);
        }
    }

    fn open(&self) -> Result<Connection> {
        let connection = Connection::open(&self.db_path)
            .with_context(|| format!("failed to open {}", self.db_path.display()))?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "busy_timeout", 5000)?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        // Re-assert 0600 on every open in case SQLite recreated the file via WAL.
        #[cfg(unix)]
        Self::enforce_db_perms(&self.db_path);
        Ok(connection)
    }

    fn initialize(&self, legacy_json_candidates: &[PathBuf]) -> Result<()> {
        let connection = self.open()?;
        connection.execute_batch(queries::INIT_SCHEMA)?;

        let mut insert = connection.prepare(queries::INSERT_IGNORE)?;

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

    pub fn list_accounts(&self) -> Result<Vec<QwenAccount>> {
        let connection = self.open()?;
        let mut statement = connection.prepare(queries::LIST)?;
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

    pub fn count(&self) -> Result<usize> {
        let connection = self.open()?;
        let count: i64 = connection.query_row(queries::COUNT, [], |row| row.get(0))?;
        Ok(count.max(0) as usize)
    }
}
