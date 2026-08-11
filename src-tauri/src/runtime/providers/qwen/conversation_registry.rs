use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QwenConversation {
    pub chat_id: String,
    pub account_id: String,
    pub parent_id: Option<String>,
}

#[derive(Clone, Default)]
pub struct ConversationRegistry {
    conversations: Arc<Mutex<HashMap<String, QwenConversation>>>,
}

impl ConversationRegistry {
    pub async fn get(&self, session_key: &str) -> Option<QwenConversation> {
        self.conversations.lock().await.get(session_key).cloned()
    }

    pub async fn upsert(&self, session_key: String, conversation: QwenConversation) {
        self.conversations.lock().await.insert(session_key, conversation);
    }

    pub async fn update_parent(&self, session_key: &str, chat_id: &str, parent_id: String) {
        let mut conversations = self.conversations.lock().await;
        if let Some(conversation) = conversations.get_mut(session_key) {
            if conversation.chat_id == chat_id {
                conversation.parent_id = Some(parent_id);
            }
        }
    }

    pub async fn remove(&self, session_key: &str) {
        self.conversations.lock().await.remove(session_key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn preserves_parent_for_same_session_and_chat() {
        let registry = ConversationRegistry::default();
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
    }
}
