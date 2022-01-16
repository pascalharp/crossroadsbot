use std::future::Future;

use anyhow::{Error, Result};
use serenity::{
    async_trait,
    builder::{CreateEmbed, CreateEmbedAuthor},
    model::{
        interactions::application_command::{
            ApplicationCommandInteraction, ApplicationCommandInteractionData,
            ApplicationCommandInteractionDataOption,
        },
        prelude::*,
    },
    prelude::Context as SerenityContext,
};
use tracing::error;

use crate::data::LogConfigData;

pub struct LogInfo {
    /// The user that initiated
    user: Option<User>,
    /// The kind
    kind: &'static str,
    /// What exactly happened
    what: String,
}

impl LogInfo {
    pub fn automatic<W: ToString>(what: W) -> Self {
        Self {
            user: None,
            kind: "Automatic",
            what: what.to_string(),
        }
    }
}

impl From<&Message> for LogInfo {
    fn from(msg: &Message) -> Self {
        LogInfo {
            user: Some(msg.author.clone()),
            kind: "Message",
            what: msg.content.clone(),
        }
    }
}

fn fmt_app_command_data_opt(data: &ApplicationCommandInteractionDataOption) -> String {
    match data.kind {
        application_command::ApplicationCommandOptionType::SubCommand => {
            let opts = data
                .options
                .iter()
                .map(|o| fmt_app_command_data_opt(o))
                .collect::<Vec<String>>()
                .join(" ");

            format!("{} {}", data.name, opts)
        }
        application_command::ApplicationCommandOptionType::SubCommandGroup => {
            let opts = data
                .options
                .iter()
                .map(|o| fmt_app_command_data_opt(o))
                .collect::<Vec<String>>()
                .join(" ");

            format!("{} {}", data.name, opts)
        }
        _ => format!(
            "{}:{}",
            data.name,
            data.value
                .as_ref()
                .and_then(|v| v.as_str())
                .unwrap_or("<empty>")
        ),
    }
}

fn fmt_app_command_data(data: &ApplicationCommandInteractionData) -> String {
    let opts = data
        .options
        .iter()
        .map(|o| fmt_app_command_data_opt(o))
        .collect::<Vec<String>>()
        .join(" ");

    format!("/{} {}", data.name, opts)
}

impl From<&ApplicationCommandInteraction> for LogInfo {
    fn from(aci: &ApplicationCommandInteraction) -> Self {
        LogInfo {
            user: Some(aci.user.clone()),
            kind: "Application Command",
            // TODO properly recreate the whole command
            what: fmt_app_command_data(&aci.data),
        }
    }
}

fn log_basic_embed(info: LogInfo) -> CreateEmbed {
    let mut emb = CreateEmbed::default();

    if let Some(u) = &info.user {
        let mut auth: CreateEmbedAuthor = CreateEmbedAuthor::default();
        auth.name(u.tag());
        if let Some(icon) = u.avatar_url() {
            auth.icon_url(icon);
        }

        emb.set_author(auth);
    }

    emb.field("Kind", info.kind, false);
    emb.field("What", info.what, false);

    emb
}

async fn log_to_channel(ctx: &SerenityContext, info: LogInfo, res: Result<()>) {
    let log_channel_info = {
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

    if let Some(chan) = log_channel_info {
        let mut emb = log_basic_embed(info);

        match res {
            Ok(_) => {
                emb.color((0, 255, 0));
            }
            Err(err) => {
                emb.color((255, 0, 0));
                emb.field("Error", format!("```\n{:?}\n```", err), false);
            }
        }

        let log_err = chan.send_message(ctx, |m| m.set_embed(emb)).await;

        if let Err(log_err) = log_err {
            error!("Failed to log message to discord: {:?}", log_err);
        }
    }
}

/// This function can be used to neatly wrap code that
/// should be logged to the log channel on discord
pub async fn log_discord<I, F, Fut>(ctx: &SerenityContext, info: I, f: F)
where
    I: Into<LogInfo>,
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<()>> + Send,
{
    let log_info: LogInfo = info.into();
    let result = f().await;
    log_to_channel(ctx, log_info, result).await;
}

// A trait to help to reply with information to the user
#[async_trait]
pub trait ReplyHelper<T, E> {
    async fn map_err_reply<F, Fut>(self, f: F) -> Result<T>
    where
        F: FnOnce(String) -> Fut + Send,
        Fut: Future<Output = Result<()>> + Send;
}

#[async_trait]
impl<T: Send, E: Into<Error> + Send + Sync> ReplyHelper<T, E> for Result<T, E> {
    async fn map_err_reply<F, Fut>(self, f: F) -> Result<T>
    where
        F: FnOnce(String) -> Fut + Send,
        Fut: Future<Output = Result<()>> + Send,
    {
        match self {
            Ok(ok) => Ok(ok),
            Err(err) => {
                let err: Error = err.into();
                let res = f(err.to_string()).await;
                match res {
                    Err(rerr) => Err(err
                        .context(rerr)
                        .context("Failed to respond to user with error")),
                    Ok(_) => Err(err),
                }
            }
        }
    }
}
