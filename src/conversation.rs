use crate::{
    utils::*,
    data::*
};
use dashmap::DashSet;
use serenity::{
    client::bridge::gateway::ShardMessenger,
    collector::{message_collector::*, reaction_collector::*},
    framework::standard::{
        help_commands,
        macros::{check, help},
        Args, CommandGroup, CommandOptions, CommandResult, HelpOptions, Reason,
    },
    model::prelude::*,
    prelude::*,
};
use std::{collections::HashSet, error::Error, fmt, sync::Arc, time::Duration};

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
    Other(String),
}

impl fmt::Display for ConversationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConversationError::ConversationLocked => {
                write!(f, "Already in another DM conversation")
            }
            ConversationError::NoDmChannel => write!(f, "Unable to load DM channel"),
            ConversationError::DmBlocked => {
                write!(f, "Unable to send message in DM channel")
            }
            ConversationError::TimedOut => {
                write!(f, "Conversation timed out")
            }
            ConversationError::Canceled => {
                write!(f, "Conversation canceled")
            }
            ConversationError::Other(s) => {
                write!(f, "{}", s)
            }
        }
    }
}

impl ConversationError {
    pub fn is_init_err(&self) -> bool {
        match self {
            ConversationError::DmBlocked
            | ConversationError::NoDmChannel
            | ConversationError::ConversationLocked => true,
            _ => false,
        }
    }
}

impl Error for ConversationError {}

impl Conversation {
    pub async fn start(ctx: &Context, user: &User) -> Result<Conversation, ConversationError> {
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

    // Consumes the conversation
    pub async fn timeout_msg(self, ctx: &Context) -> serenity::Result<Message> {
        self.chan
            .send_message(&ctx.http, |m| m.content("Conversation timed out"))
            .await
    }

    // Consumes the conversation
    pub async fn canceled_msg(self, ctx: &Context) -> serenity::Result<Message> {
        self.chan
            .send_message(&ctx.http, |m| m.content("Conversation got canceled"))
            .await
    }

    pub async fn finish_with_msg(
        self,
        ctx: &Context,
        content: impl fmt::Display,
    ) -> serenity::Result<()> {
        self.chan.say(ctx, content).await?;
        return Ok(());
    }

    // Consumes the conversation
    pub async fn abort(
        self,
        ctx: &Context,
        msg: Option<&str>,
    ) -> serenity::Result<Option<Message>> {
        if let Some(msg) = msg {
            let msg = self.chan.say(ctx, msg).await?;
            return Ok(Some(msg));
        }
        Ok(None)
    }

    pub async fn await_reply(&self, ctx: &Context) -> Option<Arc<Message>> {
        self.user
            .await_reply(ctx)
            .channel_id(self.chan.id)
            .timeout(DEFAULT_TIMEOUT)
            .await
    }

    pub async fn await_replies(&self, ctx: &Context) -> MessageCollector {
        self.user
            .await_replies(ctx)
            .channel_id(self.chan.id)
            .timeout(DEFAULT_TIMEOUT)
            .await
    }

    /// Awaits a reaction on the conversation message. Returns the Collector
    /// to further modify it. eg with a filter
    pub fn await_reaction<'a>(
        &self,
        shard_messenger: &'a impl AsRef<ShardMessenger>,
    ) -> CollectReaction<'a> {
        self.msg
            .await_reaction(shard_messenger)
            .author_id(self.user.id)
            .timeout(DEFAULT_TIMEOUT)
    }

    /// Same as await_reaction but returns a Stream
    pub fn await_reactions<'a>(
        &self,
        shard_messenger: &'a impl AsRef<ShardMessenger>,
    ) -> ReactionCollectorBuilder<'a> {
        self.msg
            .await_reactions(shard_messenger)
            .author_id(self.user.id)
            .timeout(DEFAULT_TIMEOUT)
    }
}

impl Drop for Conversation {
    fn drop(&mut self) {
        self.lock.remove(&self.user.id);
    }
}

