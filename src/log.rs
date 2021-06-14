use crate::data::LogConfigData;
use crate::signup_board::SignupBoardAction;
use serenity::{async_trait, framework::standard::CommandResult, model::prelude::*, prelude::*};

pub type LogResult = std::result::Result<String, Box<dyn std::error::Error + Send + Sync>>;

#[async_trait]
pub trait DiscordChannelLog {
    async fn log<'a>(&self, ctx: &Context, kind: LogType<'a>, user: &User);
    async fn reply(&self, ctx: &Context, msg: &Message) -> serenity::Result<Message>;
    fn cmd_result(self) -> CommandResult;
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
            // Do not reply with serenity api errors to the user directly
            Err(_) => msg.reply(ctx, String::from("Unexpected error. =(")).await,
        }
    }

    fn cmd_result(self) -> CommandResult {
        self?;
        Ok(())
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
                e.description("[ERROR] Command failed");
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
