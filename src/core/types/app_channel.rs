use async_channel::{Receiver, Sender};

use crate::core::entities::models::Message;

#[derive(Debug, Clone)]
pub struct AppChannel {
    pub async_channel_tx: Sender<AppEvent>,
    pub async_channel_rx: Receiver<AppEvent>,
}

pub enum AppEvent {
    SendMessage(Message),
    UpdateMessage(Message),
}
