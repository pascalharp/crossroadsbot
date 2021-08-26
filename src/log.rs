use crate::data::LogConfigData;
use crate::signup_board::SignupBoardAction;
use diesel::result::Error as DieselError;
use serenity::{async_trait, framework::standard::CommandResult, model::prelude::*, prelude::*};
use std::future::Future;
use std::ops::FnOnce;

pub type LogResult = std::result::Result<String, Box<dyn std::error::Error + Send + Sync>>;

#[async_trait]
pub trait DiscordChannelLog {
    async fn log<'a>(&self, ctx: &Context, kind: LogType<'a>, user: &User);
    async fn reply(&self, ctx: &Context, msg: &Message) -> serenity::Result<Message>;
    fn cmd_result(self) -> CommandResult;
}

#[async_trait]
pub trait LogCalls {
    async fn command<F: std::marker::Send, Fut: std::marker::Send>(
        ctx: &Context,
        msg: &Message,
        f: F,
    ) -> CommandResult
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = LogResult>;

    async fn interaction<F: std::marker::Send, Fut: std::marker::Send>(
        ctx: &Context,
        action: &SignupBoardAction,
        user: &User,
        f: F,
    ) -> ()
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = LogResult>;
}

#[async_trait]
impl DiscordChannelLog for LogResult {
    async fn log<'a>(&self, ctx: &Context, kind: LogType<'a>, user: &User) {
        match self {
            Ok(ok) => log_info(ctx, kind, user, &ok).await,
            Err(err) => log_error(ctx, kind, user, &err).await,
        }
    }

    async fn reply(&self, ctx: &Context, msg: &Message) -> serenity::Result<Message> {
        match self {
            Ok(s) => msg.reply(ctx, s).await,
            // dont report serenity or diesel errors directly to user
            Err(e) => {
                if let Some(_) = e.downcast_ref::<SerenityError>() {
                    return msg.reply(ctx, String::from("Unexpected error. =(")).await;
                } else if let Some(_) = e.downcast_ref::<DieselError>() {
                    return msg.reply(ctx, String::from("Unexpected error. =(")).await;
                } else {
                    return msg.reply(ctx, e).await;
                }
            }
        }
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
        res.cmd_result()
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

async fn log_info<'a>(ctx: &Context, kind: LogType<'a>, user: &User, info: &str) {
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
                e.field("Result", info, false)
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
