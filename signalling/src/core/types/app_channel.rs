use async_channel::{Receiver, Sender};

use super::res::message_response::MessageResponse;

#[derive(Debug, Clone)]
pub struct AppChannel {
    pub async_channel_tx: Sender<AppEvent>,
    pub async_channel_rx: Receiver<AppEvent>,
}

pub enum AppEvent {
    SendMessage(MessageResponse),
    UpdateMessage(MessageResponse),
    DeleteMessage(MessageResponse),
}
