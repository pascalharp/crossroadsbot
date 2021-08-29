use crate::{data::GLOB_COMMAND_PREFIX, data::*, db, embeds, log::*, utils::*};
use dashmap::DashSet;
use serenity::{
    builder::CreateEmbed,
    client::bridge::gateway::ShardMessenger,
    collector::{message_collector::*, reaction_collector::*},
    futures::future,
    model::prelude::*,
    prelude::*,
};
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt,
    sync::Arc,
};

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

    pub async fn unexpected_error(self, ctx: &Context) -> serenity::Result<Message> {
        self.msg
            .reply(&ctx.http, "Unexpected error, Sorry =(")
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

pub static NOT_REGISTERED: &str = "User not registered";
pub static NOT_OPEN: &str = "Training not found or not open";
pub static NOT_SIGNED_UP: &str = "Not signup found for user";

pub async fn join_training(
    ctx: &Context,
    conv: &mut Conversation,
    training: &db::Training,
    db_user: &db::User,
) -> LogResult {
    // verify if tier requirements pass
    match verify_tier(ctx, training, &conv.user).await {
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
                return Ok(LogAction::LogOnly("Tier requirement not fulfilled".into()));
            }
        }
        Err(e) => return Err(e.into()),
    };

    // Check if signup already exist
    match db::Signup::by_user_and_training(ctx, &db_user, &training).await {
        Ok(_) => {
            conv.msg
                .edit(ctx, |m| {
                    m.add_embed(|e| {
                        e.description("Already signed up for this training");
                        e.field(
                            "You can edit your signup with:",
                            format!("`{}edit {}`", GLOB_COMMAND_PREFIX, training.id),
                            false,
                        );
                        e.field(
                            "You can remove your signup with:",
                            format!("`{}leave {}`", GLOB_COMMAND_PREFIX, training.id),
                            false,
                        )
                    })
                })
                .await?;
            return Ok(LogAction::LogOnly("Already signed up".into()));
        }
        Err(diesel::NotFound) => (), // This is what we want
        Err(e) => return Err(e.into()),
    };

    let roles = training.active_roles(ctx).await?;
    let roles_lookup: HashMap<String, &db::Role> =
        roles.iter().map(|r| (String::from(&r.repr), r)).collect();

    // Gather selected roles
    let selected: HashSet<String> = HashSet::with_capacity(roles.len());
    let selected = select_roles(ctx, &mut conv.msg, &conv.user, &roles, selected).await?;

    let signup = db::Signup::insert(ctx, &db_user, &training).await?;

    // Save roles
    // We inserted all roles into the HashMap, so it is save to unwrap
    let futs = selected
        .iter()
        .map(|r| signup.add_role(ctx, roles_lookup.get(r).unwrap()));
    future::try_join_all(futs).await?;

    conv.msg
        .edit(ctx, |m| {
            m.add_embed(|e| {
                e.color((0, 255, 0));
                e.description("Successfully signed up");
                e.field(
                    "To edit your sign up:",
                    format!("`{}edit {}`", GLOB_COMMAND_PREFIX, training.id),
                    false,
                );
                e.field(
                    "To remove your sign up:",
                    format!("`{}leave {}`", GLOB_COMMAND_PREFIX, training.id),
                    false,
                );
                e.field(
                    "To list all your current sign ups:",
                    format!("`{}list`", GLOB_COMMAND_PREFIX),
                    false,
                );
                e
            })
        })
        .await?;

    Ok(LogAction::LogOnly("Sign up completed".into()))
}

pub async fn edit_signup(
    ctx: &Context,
    conv: &mut Conversation,
    training: &db::Training,
    signup: &db::Signup,
) -> LogResult {
    let roles = training.active_roles(ctx).await?;
    let roles_lookup: HashMap<String, &db::Role> =
        roles.iter().map(|r| (String::from(&r.repr), r)).collect();

    // Get new roles from user
    let mut selected: HashSet<String> = HashSet::with_capacity(roles.len());
    let already_selected = signup.get_roles(ctx).await?;
    for r in already_selected {
        selected.insert(r.repr);
    }
    let selected = select_roles(ctx, &mut conv.msg, &conv.user, &roles, selected).await?;

    // Save new roles
    signup.clear_roles(ctx).await?;
    // We inserted all roles into the HashMap, so it is save to unwrap
    let futs = selected.iter().filter_map(|r| {
        roles_lookup
            .get(r)
            .and_then(|r| Some(signup.add_role(ctx, *r)))
    });
    future::try_join_all(futs).await?;

    conv.msg
        .edit(ctx, |m| {
            m.add_embed(|e| {
                e.color((0, 255, 0));
                e.description("Successfully edited");
                e.field(
                    "To edit your sign up:",
                    format!("`{}edit {}`", GLOB_COMMAND_PREFIX, training.id),
                    false,
                );
                e.field(
                    "To remove your sign up:",
                    format!("`{}leave {}`", GLOB_COMMAND_PREFIX, training.id),
                    false,
                );
                e.field(
                    "To list all your current sign ups:",
                    format!("`{}list`", GLOB_COMMAND_PREFIX),
                    false,
                );
                e
            })
        })
        .await?;

    Ok(LogAction::LogOnly("Signup edited".into()))
}

pub async fn remove_signup(ctx: &Context, user: &User, training_id: i32) -> LogResult {
    let mut conv = Conversation::start(ctx, user).await?;

    let db_user = match db::User::by_discord_id(ctx, user.id).await {
        Ok(u) => u,
        Err(diesel::NotFound) => {
            let emb = embeds::not_registered_embed();
            conv.msg
                .edit(ctx, |m| {
                    m.content("");
                    m.embed(|e| {
                        e.0 = emb.0;
                        e
                    })
                })
                .await?;
            return Ok(LogAction::LogOnly(NOT_REGISTERED.into()));
        }
        Err(e) => {
            conv.unexpected_error(ctx).await?;
            return Err(e.into());
        }
    };

    let training =
        match db::Training::by_id_and_state(ctx, training_id, db::TrainingState::Open).await {
            Ok(t) => Arc::new(t),
            Err(diesel::NotFound) => {
                conv.msg
                    .reply(
                        ctx,
                        format!("No **open** training with id {} found", training_id),
                    )
                    .await?;
                return Err(NOT_OPEN.into());
            }
            Err(e) => {
                conv.unexpected_error(ctx).await?;
                return Err(e.into());
            }
        };

    let signup = match db::Signup::by_user_and_training(ctx, &db_user, &training).await {
        Ok(s) => s,
        Err(diesel::NotFound) => {
            conv.msg
                .edit(ctx, |m| {
                    m.content("");
                    m.embed(|e| {
                        e.description(format!("{} No signup found", CROSS_EMOJI));
                        e.field(
                            "You are not signed up for training:",
                            &training.title,
                            false,
                        );
                        e.field(
                            "If you want to join this training use:",
                            format!("`{}join {}`", GLOB_COMMAND_PREFIX, training.id),
                            false,
                        )
                    })
                })
                .await?;
            return Ok(LogAction::LogOnly(NOT_SIGNED_UP.into()));
        }
        Err(e) => {
            conv.unexpected_error(ctx).await?;
            return Err(e.into());
        }
    };

    match signup.remove(ctx).await {
        Ok(1) => (),
        Ok(a) => {
            conv.unexpected_error(ctx).await?;
            return Err(format!("Unexpected amount of signups removed. Amount: {}", a).into());
        }
        Err(e) => {
            conv.unexpected_error(ctx).await?;
            return Err(e.into());
        }
    }

    conv.msg
        .edit(ctx, |m| {
            m.content("");
            m.embed(|e| {
                e.description(format!("{} Signup removed", CHECK_EMOJI));
                e.field("Signup removed for training:", &training.title, false)
            })
        })
        .await?;

    Ok(LogAction::LogOnly("Success".into()))
}

pub async fn _list_signup(ctx: &Context, conv: &mut Conversation, user: &db::User) -> LogResult {
    let signups = user.active_signups(ctx).await?;
    let mut roles: HashMap<i32, Vec<db::Role>> = HashMap::with_capacity(signups.len());
    for (s, _) in &signups {
        let signup_roles = s.clone().get_roles(ctx).await?;
        roles.insert(s.id, signup_roles);
    }

    conv.msg
        .edit(ctx, |m| {
            m.add_embed(|e| {
                e.description("All current active signups");
                if signups.is_empty() {
                    e.field(
                        "No active sign ups found",
                        "You should join some trainings ;)",
                        false,
                    );
                }
                for (s, t) in signups {
                    e.field(
                        &t.title,
                        format!(
                            "`Date (YYYY-MM-DD)`\n{}\n\
                            `Time (Utc)       `\n{}\n\
                            `Training Id      `\n{}\n\
                            `Roles            `\n{}\n",
                            t.date.date(),
                            t.date.time(),
                            t.id,
                            match roles.get(&s.id) {
                                Some(r) => r
                                    .iter()
                                    .map(|r| r.repr.clone())
                                    .collect::<Vec<_>>()
                                    .join(", "),
                                None => String::from("Failed to load roles =("),
                            }
                        ),
                        true,
                    );
                }
                e.footer(|f| {
                    f.text(format!(
                        "To edit or remove your sign up reply with:\n\
                        {}edit <training id>\n\
                        {}leave <training id>",
                        GLOB_COMMAND_PREFIX, GLOB_COMMAND_PREFIX
                    ))
                });
                e
            })
        })
        .await?;

    return Ok(LogAction::LogOnly("Success".into()));
}
