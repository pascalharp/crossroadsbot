use crate::{data::GLOB_COMMAND_PREFIX, data::*, db, utils::*};
use dashmap::DashSet;
use serenity::{
    client::bridge::gateway::ShardMessenger,
    collector::{message_collector::*, reaction_collector::*},
    futures::future,
    model::prelude::*,
    prelude::*,
};
use std::{collections::HashSet, error::Error, fmt, sync::Arc};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
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

pub async fn join_training(ctx: &Context, user: &User, training_id: i32) -> Result<()> {
    let mut conv = Conversation::start(ctx, user).await?;

    let db_user = match db::User::get(*user.id.as_u64()).await {
        Ok(u) => u,
        Err(diesel::NotFound) => {
            conv.msg
                .edit(ctx, |m| {
                    m.content("");
                    m.embed(|e| {
                        e.description("Not yet registerd");
                        e.field(
                            "User not found. Use the register command first",
                            "For more information type: __~help register__",
                            false,
                        )
                    })
                })
                .await?;
            return Ok(());
        }
        Err(e) => {
            conv.msg.reply(ctx, "Unexpected error, Sorry =(").await?;
            return Err(e.into());
        }
    };

    // Get training with id
    let training = match db::Training::by_id_and_state(training_id, db::TrainingState::Open).await {
        Ok(t) => Arc::new(t),
        Err(diesel::NotFound) => {
            conv.msg
                .edit(ctx, |m| {
                    m.content(format!("No open training found with id {}", training_id))
                })
                .await?;
            return Ok(());
        }
        Err(e) => {
            conv.msg.reply(ctx, "Unexpected error, Sorry =(").await?;
            return Err(e.into());
        }
    };

    // verify if tier requirements pass
    match verify_tier(ctx, &training, &conv.user).await {
        Ok((pass, tier)) => {
            if !pass {
                conv.msg
                    .edit(ctx, |m| {
                        m.content("");
                        m.embed(|e| {
                            e.description("Tier requirement not fulfilled");
                            e.field("Missing tier:", tier, false)
                        })
                    })
                    .await?;
                return Ok(());
            }
        }
        Err(e) => {
            conv.msg.reply(ctx, "Unexpected error, Sorry =(").await?;
            return Err(e.into());
        }
    };

    // Check if signup already exist
    match db::Signup::by_user_and_training(&db_user, &training).await {
        Ok(_) => {
            conv.msg
                .edit(ctx, |m| {
                    m.content("");
                    m.embed(|e| {
                        e.description("Already signed up for this training");
                        e.field(
                            "You can edit your signup with:",
                            format!("{}edit {}", GLOB_COMMAND_PREFIX, training.id),
                            false,
                        )
                    })
                })
                .await?;
            return Ok(());
        }
        Err(diesel::NotFound) => (), // This is what we want
        Err(e) => {
            conv.msg.reply(ctx, "Unexpected error, Sorry =(").await?;
            return Err(e.into());
        }
    };

    let new_signup = db::NewSignup {
        training_id: training.id,
        user_id: db_user.id,
    };

    // register new signup
    let signup = match new_signup.add().await {
        Ok(s) => s,
        Err(e) => {
            conv.msg.reply(ctx, "Unexpected error, Sorry =(").await?;
            return Err(e.into());
        }
    };

    conv.msg
        .edit(ctx, |m| {
            m.content(format!(
                "You signed up for **{}**. Please select your roles:",
                training.title
            ))
        })
        .await?;

    // training role mapping
    let training_roles = training.clone().get_training_roles().await?;
    // The actual roles. ignoring deactivated ones (or db load errors in general)
    let roles: Vec<db::Role> = future::join_all(training_roles.iter().map(|tr| tr.role()))
        .await
        .into_iter()
        .filter_map(|r| r.ok())
        .collect();

    // Create sets for selected and unselected
    let selected: HashSet<&db::Role> = HashSet::with_capacity(roles.len());
    let mut unselected: HashSet<&db::Role> = HashSet::with_capacity(roles.len());
    for r in &roles {
        unselected.insert(r);
    }

    let selected = match select_roles(ctx, &mut conv, selected, unselected).await {
        Ok((selected, _)) => selected,
        Err(e) => {
            if let Some(e) = e.downcast_ref::<ConversationError>() {
                match e {
                    ConversationError::TimedOut => {
                        conv.timeout_msg(ctx).await?;
                        return Ok(());
                    }
                    ConversationError::Canceled => {
                        conv.canceled_msg(ctx).await?;
                        return Ok(());
                    }
                    _ => (),
                }
            }
            return Err(e.into());
        }
    };

    // Save roles
    conv.msg.edit(ctx, |m| m.content("Saving roles...")).await?;
    let futs = selected.iter().map(|r| {
        let new_signup_role = db::NewSignupRole {
            role_id: r.id,
            signup_id: signup.id,
        };
        new_signup_role.add()
    });
    match future::try_join_all(futs).await {
        Ok(r) => {
            conv.msg
                .edit(ctx, |m| {
                    m.content("");
                    m.embed(|e| {
                        e.description("Successfully signed up");
                        e.field(training.title.clone(), format!("Training id: {}", training.id), true);
                        e.field("Roles", format!("{} role(s) added to your sign up", r.len()), true);
                        e
                    })
                })
                .await?;
        }
        Err(e) => {
            conv.msg.reply(ctx, "Unexpected error, Sorry =(").await?;
            return Err(e.into());
        }
    }
    Ok(())
}
