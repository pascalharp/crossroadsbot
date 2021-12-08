use crate::components::ButtonInteraction;
use crate::data::LogConfigData;
use crate::utils;
use serenity::model::interactions::application_command::ApplicationCommandInteraction;
use serenity::{framework::standard::CommandResult, model::prelude::*, prelude::*};
use std::future::Future;
use std::ops::FnOnce;
use tracing::info;

pub enum LogType<'a> {
    Command(&'a serenity::model::channel::Message),
    Interaction {
        i: &'a ButtonInteraction,
        m: &'a serenity::model::interactions::message_component::InteractionMessage,
    },
    Automatic(&'a str),
    AppCmD(&'a ApplicationCommandInteraction),
}

impl std::fmt::Display for LogType<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Command(m) => write!(f, "Command ({})", m.content),
            Self::Interaction { i, m: _ } => write!(f, "Interaction ({})", i),
            Self::Automatic(s) => write!(f, "Automatic ({})", s),
            Self::AppCmD(a) => write!(f, "Slash Command ({})", a.data.name),
        }
    }
}

#[derive(Debug)]
pub struct ReplyInfo {
    msg_id: serenity::model::id::MessageId,
    channel_id: serenity::model::id::ChannelId,
}

impl From<&serenity::model::channel::Message> for ReplyInfo {
    fn from(msg: &serenity::model::channel::Message) -> Self {
        ReplyInfo {
            msg_id: msg.id,
            channel_id: msg.channel_id,
        }
    }
}

#[derive(Debug)]
pub enum LogError {
    LogOnly(Box<dyn std::error::Error + Send + Sync>),
    LogReply {
        err: Box<dyn std::error::Error + Send + Sync>,
        reply: ReplyInfo,
    },
    LogSlashReply {
        err: Box<dyn std::error::Error + Send + Sync>,
        aci: ApplicationCommandInteraction,
    },
    LogReplyCustom {
        err: Box<dyn std::error::Error + Send + Sync>,
        reply: ReplyInfo,
        reply_msg: String,
    },
    LogSlashCustomReply {
        err: Box<dyn std::error::Error + Send + Sync>,
        aci: ApplicationCommandInteraction,
        reply_msg: String,
    },
}

impl std::fmt::Display for LogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogError::LogOnly(err) => write!(f, "{}", err),
            LogError::LogReply { err, reply: _ } => write!(f, "{}", err),
            LogError::LogSlashReply { err, aci: _ } => write!(f, "{}", err),
            LogError::LogReplyCustom {
                err,
                reply: _,
                reply_msg: _,
            } => write!(f, "{}", err),
            LogError::LogSlashCustomReply {
                err,
                aci: _,
                reply_msg: _,
            } => write!(f, "{}", err),
        }
    }
}

impl<E: 'static + std::error::Error + Send + Sync> From<E> for LogError {
    fn from(err: E) -> Self {
        LogError::LogOnly(err.into())
    }
}

impl From<LogError> for Box<dyn std::error::Error + Send + Sync> {
    fn from(err: LogError) -> Self {
        match err {
            LogError::LogOnly(err) => err,
            LogError::LogReply { err, reply: _ } => err,
            LogError::LogSlashReply { err, aci: _ } => err,
            LogError::LogReplyCustom {
                err,
                reply: _,
                reply_msg: _,
            } => err,
            LogError::LogSlashCustomReply {
                err,
                aci: _,
                reply_msg: _,
            } => err,
        }
    }
}

impl LogError {
    pub fn new<S>(s: S, reply: &serenity::model::channel::Message) -> Self
    where
        S: ToString,
    {
        LogError::LogOnly(s.to_string().into()).with_reply(reply)
    }

    pub fn new_slash<S>(s: S, aci: ApplicationCommandInteraction) -> Self
    where
        S: ToString,
    {
        LogError::LogOnly(s.to_string().into()).with_slash_reply(aci)
    }

    pub fn new_silent<S>(s: S) -> Self
    where
        S: ToString,
    {
        LogError::LogOnly(s.to_string().into())
    }

    pub fn new_custom<S>(
        log_text: S,
        reply_text: S,
        reply: &serenity::model::channel::Message,
    ) -> Self
    where
        S: ToString,
    {
        LogError::LogOnly(log_text.to_string().into())
            .with_custom_reply(reply, reply_text.to_string())
    }

    pub fn silent(self) -> Self {
        let err = match self {
            LogError::LogOnly(e) => e,
            LogError::LogReply { err, reply: _ } => err,
            LogError::LogSlashReply { err, aci: _ } => err,
            LogError::LogReplyCustom {
                err,
                reply: _,
                reply_msg: _,
            } => err,
            LogError::LogSlashCustomReply {
                err,
                aci: _,
                reply_msg: _,
            } => err,
        };
        LogError::LogOnly(err)
    }

    pub fn with_reply(self, msg: &serenity::model::channel::Message) -> Self {
        let err = match self {
            LogError::LogOnly(e) => e,
            LogError::LogReply { err, reply: _ } => err,
            LogError::LogSlashReply { err, aci: _ } => err,
            LogError::LogReplyCustom {
                err,
                reply: _,
                reply_msg: _,
            } => err,
            LogError::LogSlashCustomReply {
                err,
                aci: _,
                reply_msg: _,
            } => err,
        };
        LogError::LogReply {
            err,
            reply: msg.into(),
        }
    }

    pub fn with_slash_reply(self, aci: ApplicationCommandInteraction) -> Self {
        let err = match self {
            LogError::LogOnly(e) => e,
            LogError::LogReply { err, reply: _ } => err,
            LogError::LogSlashReply { err, aci: _ } => err,
            LogError::LogReplyCustom {
                err,
                reply: _,
                reply_msg: _,
            } => err,
            LogError::LogSlashCustomReply {
                err,
                aci: _,
                reply_msg: _,
            } => err,
        };
        LogError::LogSlashReply { err, aci }
    }

    pub fn with_custom_reply(
        self,
        msg: &serenity::model::channel::Message,
        reply_msg: String,
    ) -> Self {
        let err = match self {
            LogError::LogOnly(e) => e,
            LogError::LogReply { err, reply: _ } => err,
            LogError::LogSlashReply { err, aci: _ } => err,
            LogError::LogReplyCustom {
                err,
                reply: _,
                reply_msg: _,
            } => err,
            LogError::LogSlashCustomReply {
                err,
                aci: _,
                reply_msg: _,
            } => err,
        };
        LogError::LogReplyCustom {
            err,
            reply: msg.into(),
            reply_msg,
        }
    }
}

pub type LogResult<T> = std::result::Result<T, LogError>;

pub trait LogResultConversion<T> {
    fn log_only(self) -> LogResult<T>;

    fn log_reply(self, msg: &serenity::model::channel::Message) -> LogResult<T>;

    fn log_slash_reply(self, aci: &ApplicationCommandInteraction) -> LogResult<T>;

    fn log_custom_reply<S>(
        self,
        msg: &serenity::model::channel::Message,
        reply_msg: S,
    ) -> LogResult<T>
    where
        S: ToString;

    fn log_unexpected_reply(self, msg: &serenity::model::channel::Message) -> LogResult<T>;
}

impl<T> From<LogError> for LogResult<T> {
    fn from(err: LogError) -> Self {
        Err(err)
    }
}

const UNEXPECTED_ERROR_REPLY: &str = "Unexpected error 😵";

impl<T, E> LogResultConversion<T> for std::result::Result<T, E>
where
    E: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    fn log_only(self) -> LogResult<T> {
        match self {
            Ok(ok) => Ok(ok),
            Err(err) => Err(LogError::LogOnly(err.into())),
        }
    }

    fn log_reply(self, msg: &serenity::model::channel::Message) -> LogResult<T> {
        self.log_only().map_err(|e| e.with_reply(msg))
    }

    fn log_slash_reply(self, aci: &ApplicationCommandInteraction) -> LogResult<T> {
        self.log_only().map_err(|e| e.with_slash_reply(aci.clone()))
    }

    fn log_custom_reply<S>(
        self,
        msg: &serenity::model::channel::Message,
        reply_msg: S,
    ) -> LogResult<T>
    where
        S: ToString,
    {
        self.log_only()
            .map_err(|e| e.with_custom_reply(msg, reply_msg.to_string()))
    }

    fn log_unexpected_reply(self, msg: &serenity::model::channel::Message) -> LogResult<T> {
        self.log_custom_reply(msg, UNEXPECTED_ERROR_REPLY)
    }
}

async fn log_to_channel<T: std::fmt::Debug>(
    ctx: &Context,
    result: &LogResult<T>,
    kind: LogType<'_>,
    user: &User,
) {
    info!("{} | {}, {:?}", user, kind, result);
    let log_info = {
        ctx.data
            .read()
            .await
            .get::<LogConfigData>()
            .unwrap()
            .clone()
            .read()
            .await
            .log
    };
    // We can only log to the discord channel if it is set
    if let Some(chan) = log_info {
        chan.send_message(ctx, |m| {
            m.allowed_mentions(|m| m.empty_parse());
            m.embed(|e| {
                e.description("[LOG]");
                e.field(
                    "User",
                    format!("`{}`\n{}", user.id, Mention::from(user)),
                    true,
                );
                match kind {
                    LogType::Interaction { i, m } => {
                        e.field(
                            "Interaction",
                            format!(
                                "`{}`\n{}",
                                i,
                                match m.clone().regular() {
                                    Some(m) => {
                                        format!("[Link]({})", m.link())
                                    }
                                    None => "Hidden message".to_string(),
                                }
                            ),
                            true,
                        );
                    }
                    LogType::Command(c) => {
                        e.field(
                            "Command",
                            format!(
                                "`{}`\n{}",
                                &c.content,
                                if c.is_private() {
                                    "_In DM's_".to_string()
                                } else {
                                    format!("[Link]({})", c.link())
                                }
                            ),
                            true,
                        );
                    }
                    LogType::Automatic(a) => {
                        e.field("Automatic", format!("`{}`", a), true);
                    }
                    LogType::AppCmD(a) => {
                        e.field("Slash Command", format!("`{}`", a.data.name), true);
                    }
                }
                match result {
                    Ok(_) => {
                        e.field(format!("{} OK", utils::CHECK_EMOJI), "\u{200b}", false);
                    }
                    Err(err) => {
                        e.field(format!("{} Error", utils::CROSS_EMOJI), err, false);
                    }
                }
                e
            })
        })
        .await
        .ok();
    }
}

async fn log_reply(ctx: &Context, err: &LogError) {
    match err {
        LogError::LogOnly(_) => (),
        LogError::LogReply { err, reply } => {
            reply
                .channel_id
                .send_message(ctx, |m| {
                    m.reference_message((reply.channel_id, reply.msg_id));
                    m.content(err.to_string())
                })
                .await
                .ok();
        }
        LogError::LogSlashReply { err, aci } => {
            aci.create_interaction_response(ctx, |r| {
                r.kind(InteractionResponseType::ChannelMessageWithSource);
                r.interaction_response_data(|d| {
                    d.content(err.to_string());
                    d.flags(InteractionApplicationCommandCallbackDataFlags::EPHEMERAL)
                })
            })
            .await
            .ok();
        }
        LogError::LogReplyCustom {
            err: _,
            reply,
            reply_msg,
        } => {
            reply
                .channel_id
                .send_message(ctx, |m| {
                    m.reference_message((reply.channel_id, reply.msg_id));
                    m.content(reply_msg)
                })
                .await
                .ok();
        }
        LogError::LogSlashCustomReply {
            err: _,
            aci,
            reply_msg,
        } => {
            aci.create_interaction_response(ctx, |r| {
                r.kind(InteractionResponseType::ChannelMessageWithSource);
                r.interaction_response_data(|d| {
                    d.content(reply_msg);
                    d.flags(InteractionApplicationCommandCallbackDataFlags::EPHEMERAL)
                })
            })
            .await
            .ok();
        }
    }
}

pub async fn log_command<F, Fut>(ctx: &Context, cmd_msg: &Message, f: F) -> CommandResult
where
    F: FnOnce() -> Fut + Send,
    Fut: Future<Output = LogResult<()>> + Send,
{
    let res = f().await;

    // Reply with error
    if let Err(err) = &res {
        log_reply(ctx, err).await;
    }

    // Log to channel
    log_to_channel(ctx, &res, LogType::Command(cmd_msg), &cmd_msg.author).await;

    // Convert to CommandError
    match res {
        Ok(_) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

pub async fn log_interaction<F, Fut>(
    ctx: &Context,
    action: &serenity::model::interactions::message_component::MessageComponentInteraction,
    i: &ButtonInteraction,
    f: F,
) where
    F: FnOnce() -> Fut + Send,
    Fut: Future<Output = LogResult<()>> + Send,
{
    let res = f().await;

    // Reply with error
    if let Err(err) = &res {
        log_reply(ctx, err).await;
    }

    log_to_channel(
        ctx,
        &res,
        LogType::Interaction {
            i,
            m: &action.message,
        },
        &action.user,
    )
    .await;
}

pub async fn log_automatic<F, Fut>(ctx: &Context, what: &str, user: &User, f: F)
where
    F: FnOnce() -> Fut + Send,
    Fut: Future<Output = LogResult<()>> + Send,
{
    let res = f().await;
    log_to_channel(ctx, &res, LogType::Automatic(what), user).await;
}

pub async fn log_slash<F, Fut>(ctx: &Context, cmd: &ApplicationCommandInteraction, f: F)
where
    F: FnOnce() -> Fut + Send,
    Fut: Future<Output = LogResult<()>> + Send,
{
    let res = f().await;
    if let Err(err) = &res {
        log_reply(ctx, err).await;
    }
    log_to_channel(ctx, &res, LogType::AppCmD(cmd), &cmd.user).await;
}
