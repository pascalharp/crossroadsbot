use std::time::Duration;

use anyhow::{bail, Result};
use serenity::{builder::CreateApplicationCommand, model::{interactions::application_command::{ApplicationCommandOptionType, ApplicationCommandInteraction, ApplicationCommandInteractionDataOption}, channel::ReactionType, id::EmojiId}, client::Context};
use serenity_tools::{interactions::ApplicationCommandInteractionExt, collectors::*};

use crate::{logging::{log_discord, LogTrace}, db};

use super::helpers::command_map;


pub const CMD_TESTING: &'static str = "xtest";
pub fn create() -> CreateApplicationCommand {
    let mut app = CreateApplicationCommand::default();
    app.name(CMD_TESTING);
    app.description("Testing");
    app.default_permission(false);
    app.create_option(|o| {
        o.kind(ApplicationCommandOptionType::SubCommand);
        o.name("selector");
        o.description("Add a new Training")
    });
    app
}

pub async fn handle(ctx: &Context, aci: &ApplicationCommandInteraction) {
    log_discord(ctx, aci, |trace| async move {
        trace.step("Parsing command");
        if let Some(sub) = aci.data.options.get(0) {
            match sub.name.as_ref() {
                "selector" => selector(ctx, aci, sub, trace).await,
                _ => bail!("{} not yet available", sub.name),
            }
        } else {
            bail!("Invalid command")
        }
    })
    .await;
}

async fn selector(
    ctx: &Context,
    aci: &ApplicationCommandInteraction,
    _option: &ApplicationCommandInteractionDataOption,
    trace: LogTrace,
) -> Result<()> {
    trace.step("Loading bosses");
    aci.create_quick_info(ctx, "Loading bosses", true).await?;

    let bosses = db::TrainingBoss::all(ctx).await?;
    let conf = PagedSelectorConfig::default();
    let mut msg = aci.get_interaction_response(ctx).await?;
    let mut select = UpdatAbleMessage::ApplicationCommand(&aci, &mut msg);
    select.paged_selector(
        ctx,
        conf,
        &bosses,
        |b| (ReactionType::from(EmojiId::from(b.emoji as u64)), b.repr.to_string())
    ).await?;

    Ok(())
}
