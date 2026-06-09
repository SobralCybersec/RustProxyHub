use anyhow::{anyhow, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct QwenAccountSummary {
    pub id: String,
    pub email: String,
    pub has_password: bool,
    pub created_at: Option<String>,
}

pub fn ensure_qwen_db(qwen_runtime_dir: &Path, workspace_root: &Path) -> Result<PathBuf> {
    let runtime_dir = qwen_runtime_dir.join("data");
    fs::create_dir_all(&runtime_dir)?;
    let db_path = runtime_dir.join("qwenproxy.db");

    if !db_path.exists() {
        for legacy_db in [
            workspace_root
                .join("runtime")
                .join("qwen")
                .join("data")
                .join("qwenproxy.db"),
            workspace_root
                .join("proxy")
                .join("qwenproxy")
                .join("data")
                .join("qwenproxy.db"),
        ] {
            if legacy_db.exists() {
                fs::copy(&legacy_db, &db_path)?;
                break;
            }
        }
    }

    let connection = Connection::open(&db_path)?;
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

    let legacy_json = workspace_root
        .join("proxy")
        .join("qwenproxy")
        .join("accounts.json");
    if legacy_json.exists() {
        migrate_legacy_accounts_json(&connection, &legacy_json)?;
    }

    Ok(db_path)
}

pub fn list_qwen_accounts(qwen_runtime_dir: &Path, workspace_root: &Path) -> Result<Vec<QwenAccountSummary>> {
    let db_path = ensure_qwen_db(qwen_runtime_dir, workspace_root)?;
    let connection = Connection::open(db_path)?;
    let mut statement = connection.prepare(
        "SELECT id, email, password, created_at FROM accounts ORDER BY created_at ASC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(QwenAccountSummary {
            id: row.get(0)?,
            email: row.get(1)?,
            has_password: !row.get::<_, String>(2)?.is_empty(),
            created_at: row.get(3).ok(),
        })
    })?;

    let mut accounts = Vec::new();
    for row in rows {
        accounts.push(row?);
    }
    Ok(accounts)
}

pub fn add_qwen_account(
    qwen_runtime_dir: &Path,
    workspace_root: &Path,
    email: &str,
    password: &str,
) -> Result<()> {
    let email = email.trim();
    if email.is_empty() {
        return Err(anyhow!("email is required"));
    }

    let db_path = ensure_qwen_db(qwen_runtime_dir, workspace_root)?;
    let connection = Connection::open(db_path)?;
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

    connection.execute(
        "INSERT INTO accounts (id, email, password) VALUES (?1, ?2, ?3)",
        params![Uuid::new_v4().to_string(), email, password],
    )?;
    Ok(())
}

pub fn remove_qwen_account(qwen_runtime_dir: &Path, workspace_root: &Path, account_id: &str) -> Result<()> {
    let db_path = ensure_qwen_db(qwen_runtime_dir, workspace_root)?;
    let connection = Connection::open(db_path)?;
    connection.execute("DELETE FROM accounts WHERE id = ?1", params![account_id])?;
    Ok(())
}

fn migrate_legacy_accounts_json(connection: &Connection, json_path: &Path) -> Result<()> {
    #[derive(Deserialize)]
    struct LegacyAccount {
        id: Option<String>,
        email: String,
        #[serde(default)]
        password: String,
    }

    let raw = fs::read_to_string(json_path)?;
    let parsed: Vec<LegacyAccount> = serde_json::from_str(&raw).unwrap_or_default();
    let mut statement = connection.prepare(
        "INSERT OR IGNORE INTO accounts (id, email, password) VALUES (?1, ?2, ?3)",
    )?;
    for account in parsed {
        if account.email.trim().is_empty() {
            continue;
        }
        statement.execute(params![
            account.id.unwrap_or_else(|| Uuid::new_v4().to_string()),
            account.email.trim(),
            account.password
        ])?;
    }
    let backup = json_path.with_extension("json.bak");
    let _ = fs::rename(json_path, backup);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{add_qwen_account, ensure_qwen_db, list_qwen_accounts, remove_qwen_account};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("rustproxyhub-qwen-{name}-{unique}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn creates_db_and_roundtrips_accounts() {
        let workspace_root = temp_dir("workspace");
        let runtime_dir = temp_dir("runtime");

        let db_path = ensure_qwen_db(&runtime_dir, &workspace_root).unwrap();
        assert!(db_path.exists());

        add_qwen_account(&runtime_dir, &workspace_root, "one@example.com", "secret").unwrap();
        let accounts = list_qwen_accounts(&runtime_dir, &workspace_root).unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].email, "one@example.com");
        assert!(accounts[0].has_password);

        remove_qwen_account(&runtime_dir, &workspace_root, &accounts[0].id).unwrap();
        assert!(list_qwen_accounts(&runtime_dir, &workspace_root).unwrap().is_empty());
    }

    #[test]
    fn migrates_legacy_json_and_creates_backup() {
        let workspace_root = temp_dir("legacy-workspace");
        let runtime_dir = temp_dir("legacy-runtime");
        let legacy_dir = workspace_root.join("proxy").join("qwenproxy");
        fs::create_dir_all(&legacy_dir).unwrap();
        let legacy_json = legacy_dir.join("accounts.json");
        fs::write(
            &legacy_json,
            r#"[{"id":"a1","email":"legacy@example.com","password":"pw"}]"#,
        )
        .unwrap();

        let accounts = list_qwen_accounts(&runtime_dir, &workspace_root).unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].id, "a1");
        assert!(legacy_dir.join("accounts.json.bak").exists());
    }
}
