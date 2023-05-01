use fmc_networking_derive::{ClientBound, NetworkMessage, ServerBound};
use serde::{Deserialize, Serialize};

/// A chat message, sent by either the client or the server.
#[derive(NetworkMessage, ClientBound, ServerBound, Serialize, Deserialize, Clone, Debug)]
pub struct ChatMessage {
    /// Name of user which sent message.
    pub username: String,
    /// Content of the message.
    pub message: String,
}
