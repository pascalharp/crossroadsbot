use crate::data::LogConfigData;
use crate::signup_board::SignupBoardAction;
use diesel::result::Error as DieselError;
use serenity::{async_trait, framework::standard::CommandResult, model::prelude::*, prelude::*};
use std::future::Future;
use std::ops::FnOnce;

pub type LogResult = std::result::Result<Option<String>, Box<dyn std::error::Error + Send + Sync>>;

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
    async fn value<T: std::marker::Send, F: std::marker::Send, Fut: std::marker::Send>(
        ctx: &Context,
        msg: &Message,
        f: F,
    ) -> Option<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>>;
}

#[async_trait]
impl DiscordChannelLog for LogResult {
    async fn log<'a>(self, ctx: &Context, kind: LogType<'a>, user: &User) {
        match self {
            Ok(ok) => log_info(ctx, kind, user, ok).await,
            Err(err) => {
                // Only log deep underlying errors as actual erros
                // Currently: SerenityError, DieselError
                if let Some(_) = err.downcast_ref::<SerenityError>() {
                    log_error(ctx, kind, user, &err).await
                } else if let Some(_) = err.downcast_ref::<DieselError>() {
                    log_error(ctx, kind, user, &err).await
                } else {
                    log_info(ctx, kind, user, Some(err.to_string())).await
                }
            }
        }
    }

    async fn reply(&self, ctx: &Context, msg: &Message) -> serenity::Result<()> {
        match self {
            Ok(s) => {
                if let Some(info) = s {
                    msg.reply(ctx, info.as_str()).await?;
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
}

pub enum LogType<'a> {
    Command(&'a str),
    Interaction(&'a SignupBoardAction),
}

impl<'a> From<&'a Message> for LogType<'a> {
    fn from(msg: &'a Message) -> LogType<'a> {
        LogType::Command(&msg.content)
    }
}

async fn log_info<'a>(ctx: &Context, kind: LogType<'a>, user: &User, info: Option<String>) {
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
                }
                e.field("Error", err, false);
                e
            })
        })
        .await
        .ok();
    }
}
