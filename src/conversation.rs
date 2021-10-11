use crate::data::*;
use dashmap::DashSet;
use serenity::{builder::CreateEmbed, model::prelude::*, prelude::*};
use std::{error::Error, fmt, sync::Arc};

pub static NOT_REGISTERED: &str = "User not registered";
pub static NOT_OPEN: &str = "Training not found or not open";
pub static NOT_SIGNED_UP: &str = "Not signup found for user";

type ConvResult = std::result::Result<Conversation, ConversationError>;

pub struct Conversation {
    lock: Arc<DashSet<UserId>>,
    pub user: User,
    pub chan: PrivateChannel,
    pub msg: Message,
}

#[derive(Debug)]
pub enum ConversationError {
    ConversationLocked,
    NoDmChannel,
    DmBlocked,
    TimedOut,
    Canceled,
    InvalidInput,
    Other(String),
}

impl fmt::Display for ConversationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConversationError::ConversationLocked => {
                write!(
                    f,
                    "Already in another DM conversation. Please finish that one first =D"
                )
            }
            ConversationError::NoDmChannel => write!(f, "Unable to load DM channel"),
            ConversationError::DmBlocked => {
                write!(f, "Unable to send message in DM's. Make sure DM's are allowed")
            }
            ConversationError::TimedOut => {
                write!(f, "Conversation timed out")
            }
            ConversationError::Canceled => {
                write!(f, "Conversation canceled")
            }
            ConversationError::InvalidInput => {
                write!(f, "Invalid Input")
            }
            ConversationError::Other(s) => {
                write!(f, "{}", s)
            }
        }
    }
}

impl Error for ConversationError {}

impl Conversation {
    pub async fn start(ctx: &Context, user: &User) -> ConvResult {
        let lock = {
            let data_read = ctx.data.read().await;
            data_read.get::<ConversationLock>().unwrap().clone()
        };

        if !lock.insert(user.id) {
            return Err(ConversationError::ConversationLocked);
        }

        // Check if we can open a dm channel
        let chan = match user.create_dm_channel(ctx).await {
            Ok(c) => c,
            Err(_) => {
                lock.remove(&user.id);
                return Err(ConversationError::NoDmChannel);
            }
        };

        // Send initial message to channel
        let msg = match chan.send_message(ctx, |m| m.content("Loading ...")).await {
            Ok(m) => m,
            Err(_) => {
                lock.remove(&user.id);
                return Err(ConversationError::DmBlocked);
            }
        };

        Ok(Conversation {
            lock,
            user: user.clone(),
            chan,
            msg,
        })
    }

    // Same as start but instead sends an embed as initial message
    pub async fn init(ctx: &Context, user: &User, emb: CreateEmbed) -> ConvResult {
        let lock = {
            let data_read = ctx.data.read().await;
            data_read.get::<ConversationLock>().unwrap().clone()
        };

        if !lock.insert(user.id) {
            return Err(ConversationError::ConversationLocked);
        }

        // Check if we can open a dm channel
        let chan = match user.create_dm_channel(ctx).await {
            Ok(c) => c,
            Err(_) => {
                lock.remove(&user.id);
                return Err(ConversationError::NoDmChannel);
            }
        };

        // Send initial message to channel
        let msg = match chan.send_message(ctx, |m| m.set_embed(emb)).await {
            Ok(m) => m,
            Err(_) => {
                lock.remove(&user.id);
                return Err(ConversationError::DmBlocked);
            }
        };

        Ok(Conversation {
            lock,
            user: user.clone(),
            chan,
            msg,
        })
    }
}

impl Drop for Conversation {
    fn drop(&mut self) {
        self.lock.remove(&self.user.id);
    }
}
