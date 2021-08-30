use crate::data::LogConfigData;
use crate::signup_board::SignupBoardAction;
use crate::utils;
use diesel::result::Error as DieselError;
use serenity::{async_trait, framework::standard::CommandResult, model::prelude::*, prelude::*};
use std::future::Future;
use std::ops::FnOnce;

pub enum LogType<'a> {
    Command(&'a str),
    Interaction(&'a SignupBoardAction),
    Conversation(String),
    Internal,
}

pub enum _LogType<'a> {
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

pub type _LogResult<T> = std::result::Result<T, LogError>;

pub trait LogResultConversion<T> {
    fn log_only(self) -> _LogResult<T>;
    fn log_reply(self, msg: &serenity::model::channel::Message) -> _LogResult<T>;
    fn log_custom_reply(
        self,
        msg: &serenity::model::channel::Message,
        reply_msg: String,
    ) -> _LogResult<T>;
}

impl<T, E> LogResultConversion<T> for std::result::Result<T, E>
where
    E: 'static + std::error::Error + Send + Sync,
{
    fn log_only(self) -> _LogResult<T> {
        match self {
            Ok(ok) => Ok(ok),
            Err(err) => Err(err.into()),
        }
    }

    fn log_reply(self, msg: &serenity::model::channel::Message) -> _LogResult<T> {
        self.log_only().map_err(|e| e.with_reply(&msg))
    }

    fn log_custom_reply(
        self,
        msg: &serenity::model::channel::Message,
        reply_msg: String,
    ) -> _LogResult<T> {
        self.log_only()
            .map_err(|e| e.with_custom_reply(&msg, reply_msg))
    }
}

async fn log_to_channel<'a, T: std::fmt::Display>(
    ctx: &Context,
    result: &_LogResult<T>,
    kind: _LogType<'a>,
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
            .info
    };
    // We can only log to the discord channel if it is set
    if let Some(chan) = log_info {
        chan.send_message(ctx, |m| {
            m.allowed_mentions(|m| m.empty_parse());
            m.embed(|e| {
                e.description("[INFO]");
                e.field("User", Mention::from(user), true);
                match kind {
                    _LogType::Interaction(i) => {
                        e.field("Interaction", i, true);
                    }
                    _LogType::Command(c) => {
                        e.field(
                            "Command",
                            format!(
                                "`{}`\n{}",
                                &c.content,
                                if c.is_private() {
                                    "_In DM's_".to_string()
                                } else {
                                    c.link()
                                }
                            ),
                            true,
                        );
                    }
                }
                match result {
                    Ok(ok) => {
                        e.field(format!("OK {}", utils::CHECK_EMOJI), ok, false);
                    }
                    Err(err) => {
                        e.field(format!("Error {}", utils::CROSS_EMOJI), err, false);
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

pub async fn log_command<R, F, Fut>(ctx: &Context, cmd_msg: &Message, f: F) -> CommandResult
where
    F: FnOnce() -> Fut + Send,
    R: std::fmt::Display,
    Fut: Future<Output = _LogResult<R>> + Send,
{
    let res = f().await;

    // Reply with error
    if let Err(err) = &res {
        log_reply(ctx, err).await;
    }

    // Log to channel
    log_to_channel(ctx, &res, _LogType::Command(cmd_msg), &cmd_msg.author).await;

    // Convert to CommandError
    match res {
        Ok(_) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

pub enum LogAction {
    // Try to avoid
    None,
    // Only log
    LogOnly(String),
    // This will log and as well reply with the message if possible
    Reply(String),
    // Same as Reply but with different messages
    Custom { log_msg: String, reply_msg: String },
}

pub type LogResult = std::result::Result<LogAction, Box<dyn std::error::Error + Send + Sync>>;

#[async_trait]
pub trait DiscordChannelLog {
    async fn log<'a>(self, ctx: &Context, kind: LogType<'a>, user: &User);
    async fn reply(&self, ctx: &Context, msg: &Message) -> serenity::Result<()>;
    fn cmd_result(self) -> CommandResult;
}

#[async_trait]
pub trait LogCalls {
    /// Logs a command. Will reply to the inital message with the result
    async fn command<F: std::marker::Send, Fut: std::marker::Send>(
        ctx: &Context,
        msg: &Message,
        f: F,
    ) -> CommandResult
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = LogResult>;

    /// Logs a command. Will reply to another message and not the first one
    async fn command_separate_reply<F: std::marker::Send, Fut: std::marker::Send>(
        ctx: &Context,
        msg_orig: &Message,
        msg_reply: &Message,
        f: F,
    ) -> CommandResult
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = LogResult>;

    /// Logs an interaction if there is no initial message from the user
    async fn interaction<F: std::marker::Send, Fut: std::marker::Send>(
        ctx: &Context,
        action: &SignupBoardAction,
        user: &User,
        f: F,
    ) -> ()
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = LogResult>;

    /// Only logs on errors, otherwise yields the result
    /// Replies to the message with the error
    async fn value<T: std::marker::Send, F: std::marker::Send, Fut: std::marker::Send>(
        ctx: &Context,
        msg: &Message,
        f: F,
    ) -> Option<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>>;

    /// Only logs on errors, otherwise yields the result
    /// Does not reply to a message on error
    async fn value_silent<T: std::marker::Send, F: std::marker::Send, Fut: std::marker::Send>(
        ctx: &Context,
        user: &User,
        f: F,
    ) -> Option<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>>;
}

impl LogAction {
    fn log_string(&self) -> Option<&String> {
        match self {
            LogAction::None => None,
            LogAction::LogOnly(s) => Some(s),
            LogAction::Reply(s) => Some(s),
            LogAction::Custom {
                log_msg,
                reply_msg: _,
            } => Some(log_msg),
        }
    }

    fn reply_string(&self) -> Option<&String> {
        match self {
            LogAction::None => None,
            LogAction::LogOnly(_) => None,
            LogAction::Reply(s) => Some(s),
            LogAction::Custom {
                log_msg: _,
                reply_msg,
            } => Some(reply_msg),
        }
    }
}

#[async_trait]
impl DiscordChannelLog for LogResult {
    async fn log<'a>(self, ctx: &Context, kind: LogType<'a>, user: &User) {
        match self {
            Ok(ok) => log_info(ctx, kind, user, ok.log_string()).await,
            Err(err) => {
                // Only log deep underlying errors as actual erros
                // Currently: SerenityError, DieselError
                if let Some(_) = err.downcast_ref::<SerenityError>() {
                    log_error(ctx, kind, user, &err).await
                } else if let Some(_) = err.downcast_ref::<DieselError>() {
                    log_error(ctx, kind, user, &err).await
                } else {
                    log_info(ctx, kind, user, Some(&err.to_string())).await
                }
            }
        }
    }

    async fn reply(&self, ctx: &Context, msg: &Message) -> serenity::Result<()> {
        match self {
            Ok(s) => {
                if let Some(info) = s.reply_string() {
                    msg.reply(ctx, info).await?;
                }
            }
            // dont report serenity or diesel errors directly to user
            Err(e) => {
                if let Some(_) = e.downcast_ref::<SerenityError>() {
                    msg.reply(ctx, String::from("Unexpected error. =(")).await?;
                } else if let Some(_) = e.downcast_ref::<DieselError>() {
                    msg.reply(ctx, String::from("Unexpected error. =(")).await?;
                } else {
                    msg.reply(ctx, e).await?;
                }
            }
        };
        Ok(())
    }

    // Only bubbles up serenity and diesel errors to be reported as errors
    // TODO remove
    fn cmd_result(self) -> CommandResult {
        match self {
            Err(e) => {
                if let Some(_) = e.downcast_ref::<SerenityError>() {
                    return Err(e);
                } else if let Some(_) = e.downcast_ref::<DieselError>() {
                    return Err(e);
                } else {
                    return Ok(());
                }
            }
            Ok(_) => return Ok(()),
        }
    }
}

#[async_trait]
impl LogCalls for LogResult {
    async fn command<F: std::marker::Send, Fut: std::marker::Send>(
        ctx: &Context,
        msg: &Message,
        f: F,
    ) -> CommandResult
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = LogResult>,
    {
        let res = f().await;
        res.reply(ctx, msg).await?;
        res.log(ctx, LogType::Command(&msg.content), &msg.author)
            .await;
        Ok(())
    }

    async fn command_separate_reply<F: std::marker::Send, Fut: std::marker::Send>(
        ctx: &Context,
        msg_orig: &Message,
        msg_reply: &Message,
        f: F,
    ) -> CommandResult
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = LogResult>,
    {
        let res = f().await;
        res.reply(ctx, msg_reply).await?;
        res.log(ctx, LogType::Command(&msg_orig.content), &msg_orig.author)
            .await;
        Ok(())
    }

    async fn interaction<F: std::marker::Send, Fut: std::marker::Send>(
        ctx: &Context,
        action: &SignupBoardAction,
        user: &User,
        f: F,
    ) -> ()
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = LogResult>,
    {
        let res = f().await;
        res.log(ctx, LogType::Interaction(action), user).await;
    }

    async fn value<T: std::marker::Send, F: std::marker::Send, Fut: std::marker::Send>(
        ctx: &Context,
        msg: &Message,
        f: F,
    ) -> Option<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>>,
    {
        let res = f().await;
        match res {
            Ok(ok) => Some(ok),
            Err(err) => {
                let err = LogResult::Err(err.into());
                err.reply(ctx, msg).await.ok();
                err.log(ctx, LogType::Command(&msg.content), &msg.author)
                    .await;
                None
            }
        }
    }

    async fn value_silent<T: std::marker::Send, F: std::marker::Send, Fut: std::marker::Send>(
        ctx: &Context,
        user: &User,
        f: F,
    ) -> Option<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>>,
    {
        let res = f().await;
        match res {
            Ok(ok) => Some(ok),
            Err(err) => {
                let err = LogResult::Err(err.into());
                err.log(ctx, LogType::Internal, user).await;
                None
            }
        }
    }
}

impl<'a> From<&'a Message> for LogType<'a> {
    fn from(msg: &'a Message) -> LogType<'a> {
        LogType::Command(&msg.content)
    }
}

async fn log_info<'a>(ctx: &Context, kind: LogType<'a>, user: &User, info: Option<&String>) {
    let log_info = {
        ctx.data
            .read()
            .await
            .get::<LogConfigData>()
            .unwrap()
            .clone()
            .read()
            .await
            .info
    };
    // We can only log to the discord channel if it is set
    if let Some(chan) = log_info {
        chan.send_message(ctx, |m| {
            m.allowed_mentions(|m| m.empty_parse());
            m.embed(|e| {
                e.description("[INFO]");
                e.field("User", Mention::from(user), true);
                match kind {
                    LogType::Interaction(i) => {
                        e.field("Interaction", i, true);
                    }
                    LogType::Command(c) => {
                        e.field("Command", format!("`{}`", c), true);
                    }
                    LogType::Conversation(s) => {
                        e.field("Conversation", s, true);
                    }
                    LogType::Internal => {
                        e.field("Internal", "*omitted*", true);
                    }
                }
                if let Some(info) = info {
                    e.field("Result", info, false);
                }
                e
            })
        })
        .await
        .ok();
    }
}

async fn log_error<'a>(
    ctx: &Context,
    kind: LogType<'a>,
    user: &User,
    err: &Box<dyn std::error::Error + Send + Sync>,
) {
    let err_info = {
        ctx.data
            .read()
            .await
            .get::<LogConfigData>()
            .unwrap()
            .clone()
            .read()
            .await
            .error
    };
    // We can only log to the discord channel if it is set
    if let Some(chan) = err_info {
        chan.send_message(ctx, |m| {
            m.allowed_mentions(|m| m.empty_parse());
            m.embed(|e| {
                e.description("[ERROR]");
                e.field("User", Mention::from(user), true);
                match kind {
                    LogType::Interaction(i) => {
                        e.field("Interaction", i, true);
                    }
                    LogType::Command(c) => {
                        e.field("Command", format!("`{}`", c), true);
                    }
                    LogType::Conversation(s) => {
                        e.field("Conversation", s, true);
                    }
                    LogType::Internal => {
                        e.field("Internal", "*omitted*", true);
                    }
                }
                e.field("Error", err, false);
                e
            })
        })
        .await
        .ok();
    }
}
