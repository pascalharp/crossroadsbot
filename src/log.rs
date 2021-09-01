use crate::data::LogConfigData;
use crate::utils;
use serenity::{framework::standard::CommandResult, model::prelude::*, prelude::*};
use std::future::Future;
use std::ops::FnOnce;

pub enum LogType<'a> {
    Command(&'a serenity::model::channel::Message),
    Interaction(&'a str),
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
    LogReplyCustom {
        err: Box<dyn std::error::Error + Send + Sync>,
        reply: ReplyInfo,
        reply_msg: String,
    },
}

impl std::fmt::Display for LogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogError::LogOnly(err) => write!(f, "{}", err),
            LogError::LogReply { err, reply: _ } => write!(f, "{}", err),
            LogError::LogReplyCustom {
                err,
                reply: _,
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
        return match err {
            LogError::LogOnly(err) => err,
            LogError::LogReply { err, reply: _ } => err,
            LogError::LogReplyCustom {
                err,
                reply: _,
                reply_msg: _,
            } => err,
        };
    }
}

impl LogError {
    pub fn new<S>(s: S, reply: &serenity::model::channel::Message) -> Self
    where
        S: ToString,
    {
        LogError::LogOnly(s.to_string().into()).with_reply(reply)
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
            LogError::LogReplyCustom {
                err,
                reply: _,
                reply_msg: _,
            } => err,
        };
        LogError::LogOnly(err)
    }

    pub fn with_reply(self, msg: &serenity::model::channel::Message) -> Self {
        let err = match self {
            LogError::LogOnly(e) => e,
            LogError::LogReply { err, reply: _ } => err,
            LogError::LogReplyCustom {
                err,
                reply: _,
                reply_msg: _,
            } => err,
        };
        LogError::LogReply {
            err,
            reply: msg.into(),
        }
    }

    pub fn with_custom_reply(
        self,
        msg: &serenity::model::channel::Message,
        reply_msg: String,
    ) -> Self {
        let err = match self {
            LogError::LogOnly(e) => e,
            LogError::LogReply { err, reply: _ } => err,
            LogError::LogReplyCustom {
                err,
                reply: _,
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
        self.log_only().map_err(|e| e.with_reply(&msg))
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
            .map_err(|e| e.with_custom_reply(&msg, reply_msg.to_string()))
    }

    fn log_unexpected_reply(self, msg: &serenity::model::channel::Message) -> LogResult<T> {
        self.log_custom_reply(msg, UNEXPECTED_ERROR_REPLY)
    }
}

async fn log_to_channel<'a, T>(
    ctx: &Context,
    result: &LogResult<T>,
    kind: LogType<'a>,
    user: &User,
) -> () {
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
                    LogType::Interaction(i) => {
                        e.field("Interaction", i, true);
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
                }
                match result {
                    Ok(_) => {
                        e.field(format!("{} OK", utils::CHECK_EMOJI), "\n\u{200b}", false);
                    }
                    Err(err) => {
                        e.field(format!("{} Error", utils::CROSS_EMOJI), err, false);
                        e.field(
                            format!("Reply"),
                            match err {
                                LogError::LogOnly(_) => "_None_",
                                LogError::LogReply { err: _, reply: _ } => "_Same as Error_",
                                LogError::LogReplyCustom {
                                    err: _,
                                    reply: _,
                                    reply_msg,
                                } => &reply_msg,
                            },
                            false,
                        );
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

