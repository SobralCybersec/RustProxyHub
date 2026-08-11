use crate::runtime::session_store::SessionStore;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QwenConversation {
    pub chat_id: String,
    pub account_id: String,
    pub parent_id: Option<String>,
}

#[derive(Clone)]
pub struct ConversationRegistry {
    store: SessionStore,
}

impl ConversationRegistry {
    pub fn open(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        Ok(Self {
            store: SessionStore::open(path.as_ref())?,
        })
    }

    pub async fn get(&self, session_key: &str) -> Option<QwenConversation> {
        self.store.get("qwen", session_key).ok().flatten()
    }

    pub async fn upsert(&self, session_key: String, conversation: QwenConversation) {
        if let Err(err) = self.store.set("qwen", &session_key, &conversation) {
            eprintln!("[qwen] failed to persist conversation {session_key}: {err}");
        }
    }

    pub async fn update_parent(&self, session_key: &str, chat_id: &str, parent_id: String) {
        if let Some(mut conversation) = self.get(session_key).await {
            if conversation.chat_id == chat_id {
                conversation.parent_id = Some(parent_id);
                self.upsert(session_key.to_owned(), conversation).await;
            }
        }
    }

    pub async fn remove(&self, session_key: &str) {
        if let Err(err) = self.store.remove("qwen", session_key) {
            eprintln!("[qwen] failed to remove persisted conversation {session_key}: {err}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn preserves_parent_for_same_session_and_chat() {
        let path = std::env::temp_dir().join(format!(
            "rust-proxy-hub-qwen-conversation-{}.db",
            uuid::Uuid::new_v4()
        ));
        let registry = ConversationRegistry::open(&path).unwrap();
        registry
            .upsert(
                "default".to_owned(),
                QwenConversation {
                    chat_id: "chat-1".to_owned(),
                    account_id: "global".to_owned(),
                    parent_id: None,
                },
            )
            .await;

        registry
            .update_parent("default", "chat-1", "response-1".to_owned())
            .await;
        registry
            .update_parent("default", "different-chat", "response-2".to_owned())
            .await;

        assert_eq!(
            registry.get("default").await.unwrap().parent_id.as_deref(),
            Some("response-1")
        );
        drop(registry);
        let _ = std::fs::remove_file(path);
    }
}
