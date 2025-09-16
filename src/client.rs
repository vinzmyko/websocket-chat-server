use axum::extract::ws::Message;
use tokio::sync::mpsc;

pub struct ConnectedClient {
    pub user_name: String,
    pub sender: mpsc::UnboundedSender<Message>,
}

#[derive(serde::Serialize)]
pub struct ChatMessage {
    pub user_name: String,
    pub content: String,
}
