use std::{
    future::Future,
    result::Result as StdResult,
    sync::{Arc, Mutex},
};

use anyhow::{Error, Result};
use chrono::{NaiveDateTime, Utc};
use serenity::{
    async_trait,
    builder::{CreateEmbed, CreateEmbedAuthor},
    model::{
        interactions::{
            application_command::{
                ApplicationCommandInteraction, ApplicationCommandInteractionData,
                ApplicationCommandInteractionDataOption,
            },
            message_component::MessageComponentInteraction,
        },
        prelude::*,
    },
    prelude::Context as SerenityContext,
};
use tracing::{error, info};

use crate::data::LogConfigData;

/// An error that isn't really an error. Yeah that makes sense
#[derive(Debug)]
pub enum InfoError {
    TimedOut,
    Aborted,
    AbortedOrTimedOut,
    NotRegistered,
}

impl InfoError {
    pub fn err(self) -> Result<(), Self> {
        Err(self)
    }
}

impl std::fmt::Display for InfoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TimedOut => write!(f, "Timed out"),
            Self::Aborted => write!(f, "Aborted"),
            Self::AbortedOrTimedOut => write!(f, "Aborted or Timed out"),
            Self::NotRegistered => write!(f, "Not registered"),
        }
    }
}

impl std::error::Error for InfoError {}

#[derive(Debug)]
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

    pub fn add_user(&mut self, user: User) {
        self.user = Some(user);
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
                .map(fmt_app_command_data_opt)
                .collect::<Vec<String>>()
                .join(" ");

            format!("{} {}", data.name, opts)
        }
        application_command::ApplicationCommandOptionType::SubCommandGroup => {
            let opts = data
                .options
                .iter()
                .map(fmt_app_command_data_opt)
                .collect::<Vec<String>>()
                .join(" ");

            format!("{} {}", data.name, opts)
        }
        _ => format!(
            "{}:{}",
            data.name,
            data.value
                .as_ref()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "<empty>".to_string())
        ),
    }
}

fn fmt_app_command_data(data: &ApplicationCommandInteractionData) -> String {
    let opts = data
        .options
        .iter()
        .map(fmt_app_command_data_opt)
        .collect::<Vec<String>>()
        .join(" ");

    format!("/{} {}", data.name, opts)
}

impl From<&ApplicationCommandInteraction> for LogInfo {
    fn from(aci: &ApplicationCommandInteraction) -> Self {
        LogInfo {
            user: Some(aci.user.clone()),
            kind: "Application Command",
            what: fmt_app_command_data(&aci.data),
        }
    }
}

impl From<&MessageComponentInteraction> for LogInfo {
    fn from(mci: &MessageComponentInteraction) -> Self {
        LogInfo {
            user: Some(mci.user.clone()),
            kind: "Message Interaction",
            what: mci.data.custom_id.to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogTrace(Arc<Mutex<Vec<(NaiveDateTime, &'static str)>>>);

impl LogTrace {
    fn new() -> Self {
        LogTrace(Arc::new(Mutex::new(Vec::new())))
    }

    pub fn step(&self, step: &'static str) {
        let time = Utc::now().naive_utc();
        self.0.lock().unwrap().push((time, step));
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
        emb.description(format!("User Id: {}", u.id));
    }

    emb.field("Kind", info.kind, false);
    emb.field("What", info.what, false);

    emb
}

async fn log_to_channel(ctx: &SerenityContext, info: LogInfo, trace: LogTrace, res: Result<()>) {
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

        match Arc::<std::sync::Mutex<Vec<(NaiveDateTime, &'static str)>>>::try_unwrap(trace.0) {
            Ok(trace) => {
                // We are the only holder of the trace at this moment
                let trace = trace.into_inner().unwrap();

                let mut trace_split: Vec<String> = Vec::new();
                let mut curr_str = String::from("");
                for (time, step) in trace {
                    let line_str = format!("<t:{}:T> {}", time.timestamp(), step);

                    if curr_str.len() + line_str.len() + 1 > 1024 {
                        // would exceed discord field len max
                        trace_split.push(curr_str);
                        curr_str = line_str;
                    } else {
                        curr_str.push('\n');
                        curr_str.push_str(&line_str);
                    }
                }
                trace_split.push(curr_str);

                for t in trace_split {
                    emb.field("Trace", t, true);
                }
            }
            Err(_) => {
                emb.field(
                    "Trace",
                    "__The trace is still in use somewhere! Fix code =(__",
                    false,
                );
            }
        };

        match res {
            Ok(_) => {
                emb.color((0, 255, 0));
            }
            Err(err) => {
                if err.downcast_ref::<InfoError>().is_some() {
                    emb.color((255, 255, 0));
                    emb.field("Info", format!("```\n{:?}\n```", err), false);
                } else {
                    emb.color((255, 0, 0));
                    emb.field("Error", format!("```\n{:?}\n```", err), false);
                }
            }
        }

        let log_err = chan.send_message(ctx, |m| m.set_embed(emb)).await;

        if let Err(log_err) = log_err {
            error!("Failed to log message to discord: {:?}", log_err);
        }
    } else {
        info!("Discord log channel not set up");
    }
}

/// This function can be used to neatly wrap code that
/// should be logged to the log channel on discord
pub async fn log_discord<I, F, Fut>(ctx: &SerenityContext, info: I, f: F)
where
    I: Into<LogInfo>,
    F: FnOnce(LogTrace) -> Fut,
    Fut: Future<Output = Result<()>> + Send,
{
    let log_info: LogInfo = info.into();
    let log_trace = LogTrace::new();
    log_trace.step("Start");
    let result = f(log_trace.clone()).await;
    log_trace.step("End");
    log_to_channel(ctx, log_info, log_trace, result).await;
}

pub async fn log_discord_err_only<I, F, Fut>(ctx: &SerenityContext, info: I, f: F)
where
    I: Into<LogInfo>,
    F: FnOnce(LogTrace) -> Fut,
    Fut: Future<Output = Result<()>> + Send,
{
    let log_info: LogInfo = info.into();
    let log_trace = LogTrace::new();
    log_trace.step("Start");
    let result = f(log_trace.clone()).await;
    log_trace.step("End");
    if result.is_err() {
        log_to_channel(ctx, log_info, log_trace, result).await;
    }
}

// A trait to help to reply with information to the user
#[async_trait]
pub trait ReplyHelper<T, E> {
    async fn map_err_reply<F, Fut, R, U>(self, f: F) -> Result<T>
    where
        F: FnOnce(String) -> Fut + Send,
        R: Into<Error>,
        Fut: Future<Output = StdResult<U, R>> + Send;
}

#[async_trait]
impl<T: Send, E: Into<Error> + Send + Sync> ReplyHelper<T, E> for Result<T, E> {
    async fn map_err_reply<F, Fut, R, U>(self, f: F) -> Result<T>
    where
        F: FnOnce(String) -> Fut + Send,
        R: Into<Error>,
        Fut: Future<Output = StdResult<U, R>> + Send,
    {
        match self {
            Ok(ok) => Ok(ok),
            Err(err) => {
                let err: Error = err.into();
                match f(err.to_string()).await {
                    Err(rerr) => {
                        let rerr: Error = rerr.into();
                        Err(err
                            .context(rerr)
                            .context("Failed to respond to user with error"))
                    }
                    Ok(_) => Err(err),
                }
            }
        }
    }
}
