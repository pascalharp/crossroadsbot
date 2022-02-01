use anyhow::{anyhow, bail, Result};
use serenity::{
    builder::CreateApplicationCommand,
    client::Context,
    model::{
        id::ChannelId,
        interactions::application_command::{
            ApplicationCommandInteraction, ApplicationCommandInteractionDataOption,
            ApplicationCommandOptionType,
        },
    },
};
use serenity_tools::interactions::ApplicationCommandInteractionExt;

use crate::{
    data::{LogConfigData, INFO_LOG_NAME},
    db,
    logging::{log_discord, LogTrace, ReplyHelper},
    signup_board,
};

pub const CMD_CONFIG: &'static str = "config";

pub fn create() -> CreateApplicationCommand {
    let mut app = CreateApplicationCommand::default();
    app.name(CMD_CONFIG);
    app.description("Bot configurations");
    app.default_permission(false);
    app.create_option(|o| {
        o.kind(ApplicationCommandOptionType::SubCommand);
        o.name("overview");
        o.description("set the channel for the overview message");
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::Channel);
            o.required(true);
            o.name("channel");
            o.description("The channel in which the overview message will be posted")
        })
    });
    app.create_option(|o| {
        o.kind(ApplicationCommandOptionType::SubCommand);
        o.name("log");
        o.description("set the log channel");
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::Channel);
            o.required(true);
            o.name("channel");
            o.description("The channel to which all logs are posted")
        })
    });
    app
}

pub async fn handle(ctx: &Context, aci: &ApplicationCommandInteraction) {
    log_discord(ctx, aci, |trace| async move {
        trace.step("Parsing command");
        if let Some(sub) = aci.data.options.get(0) {
            match sub.name.as_ref() {
                "overview" => overview(ctx, aci, sub, trace).await,
                "log" => log(ctx, aci, sub, trace).await,
                _ => bail!("{} not yet available", sub.name),
            }
        } else {
            bail!("Invalid command")
        }
    })
    .await;
}

async fn overview(
    ctx: &Context,
    aci: &ApplicationCommandInteraction,
    option: &ApplicationCommandInteractionDataOption,
    trace: LogTrace,
) -> Result<()> {
    let channel_id = option
        .options
        .get(0)
        .ok_or(anyhow!("Unexpected missing option"))
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?
        .value
        .as_ref()
        .and_then(|v| v.as_str())
        .and_then(|v| Some(v.parse::<ChannelId>()))
        .ok_or(anyhow!("Unexpected missing value"))
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?;

    trace.step("Loading signup board");
    let board = signup_board::SignupBoard::get(ctx).await;
    let mut lock = board.write().await;

    trace.step("Set channel");
    lock.set_channel(ctx, channel_id, trace.clone())
        .await
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?;

    trace.step("Create message");
    lock.create_overview(ctx, trace.clone())
        .await
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?;

    trace.step("Save to db");
    lock.save_to_db(ctx)
        .await
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?;

    trace.step("Update overview");
    lock.update_overview(ctx, trace.clone())
        .await
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?;

    aci.create_quick_info(ctx, "Channel and message successfully set", true)
        .await?;

    Ok(())
}

async fn log(
    ctx: &Context,
    aci: &ApplicationCommandInteraction,
    option: &ApplicationCommandInteractionDataOption,
    trace: LogTrace,
) -> Result<()> {
    let channel_id = option
        .options
        .get(0)
        .ok_or(anyhow!("Unexpected missing option"))
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?
        .value
        .as_ref()
        .and_then(|v| v.as_str())
        .and_then(|v| Some(v.parse::<ChannelId>()))
        .ok_or(anyhow!("Unexpected missing value"))
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?;

    trace.step("Setting log channel internally");
    {
        let write_lock = ctx
            .data
            .read()
            .await
            .get::<LogConfigData>()
            .unwrap()
            .clone();
        write_lock.write().await.log = Some(channel_id);
    }

    trace.step("Saving log channel to db");
    let conf = db::Config {
        name: String::from(INFO_LOG_NAME),
        value: channel_id.to_string(),
    };

    conf.save(ctx)
        .await
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?;

    aci.create_quick_info(ctx, "Log channel updated", true)
        .await?;

    Ok(())
}