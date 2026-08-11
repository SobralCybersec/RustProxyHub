use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use std::path::PathBuf;

/// Small durable key/value store for upstream conversation cursors. Providers own
/// their payload schema; this module only makes the cursor survive a restart.
#[derive(Clone)]
pub struct SessionStore {
    path: PathBuf,
}

impl SessionStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let store = Self { path: path.into() };
        if let Some(parent) = store.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        store.connection()?.execute_batch(
            "CREATE TABLE IF NOT EXISTS provider_sessions (
                provider TEXT NOT NULL,
                session_key TEXT NOT NULL,
                payload TEXT NOT NULL,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (provider, session_key)
            );",
        )?;
        Ok(store)
    }

    pub fn get<T: DeserializeOwned>(&self, provider: &str, session_key: &str) -> Result<Option<T>> {
        let payload = self
            .connection()?
            .query_row(
                "SELECT payload FROM provider_sessions WHERE provider = ?1 AND session_key = ?2",
                params![provider, session_key],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        payload
            .map(|payload| serde_json::from_str(&payload).map_err(Into::into))
            .transpose()
    }

    pub fn set<T: Serialize>(&self, provider: &str, session_key: &str, payload: &T) -> Result<()> {
        let payload = serde_json::to_string(payload)?;
        self.connection()?.execute(
            "INSERT INTO provider_sessions (provider, session_key, payload, updated_at)
             VALUES (?1, ?2, ?3, unixepoch())
             ON CONFLICT(provider, session_key) DO UPDATE SET
                payload = excluded.payload,
                updated_at = excluded.updated_at",
            params![provider, session_key, payload],
        )?;
        Ok(())
    }

    pub fn remove(&self, provider: &str, session_key: &str) -> Result<()> {
        self.connection()?.execute(
            "DELETE FROM provider_sessions WHERE provider = ?1 AND session_key = ?2",
            params![provider, session_key],
        )?;
        Ok(())
    }

    fn connection(&self) -> Result<Connection> {
        Ok(Connection::open(&self.path)?)
    }
}

#[cfg(test)]
mod tests {
    use super::SessionStore;
    use serde_json::json;

    #[test]
    fn persists_provider_scoped_sessions() {
        let path = std::env::temp_dir().join(format!(
            "rust-proxy-hub-session-{}.db",
            uuid::Uuid::new_v4()
        ));
        let store = SessionStore::open(&path).unwrap();
        store
            .set("qwen", "client", &json!({ "parent": "one" }))
            .unwrap();
        store
            .set("kimi", "client", &json!({ "parent": "two" }))
            .unwrap();

        assert_eq!(
            store.get::<serde_json::Value>("qwen", "client").unwrap(),
            Some(json!({ "parent": "one" }))
        );
        assert_eq!(
            store.get::<serde_json::Value>("kimi", "client").unwrap(),
            Some(json!({ "parent": "two" }))
        );

        store.remove("qwen", "client").unwrap();
        assert!(store
            .get::<serde_json::Value>("qwen", "client")
            .unwrap()
            .is_none());
        let _ = std::fs::remove_file(path);
    }
}
