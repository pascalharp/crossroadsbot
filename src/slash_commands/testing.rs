use anyhow::{bail, Result};
use serenity::{
    builder::CreateApplicationCommand,
    client::Context,
    model::{
        channel::ReactionType,
        id::EmojiId,
        interactions::application_command::{
            ApplicationCommandInteraction, ApplicationCommandInteractionDataOption,
            ApplicationCommandOptionType,
        },
    },
};
use serenity_tools::{collectors::*, interactions::ApplicationCommandInteractionExt};

use crate::{
    db,
    logging::{log_discord, LogTrace},
};

pub(super) const CMD_TESTING: &str = "xtest";
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
    let mut select = UpdatAbleMessage::ApplicationCommand(aci, &mut msg);
    let selected = select
        .paged_selector(ctx, conf, &bosses, |b| {
            (
                ReactionType::from(EmojiId::from(b.emoji as u64)),
                b.repr.to_string(),
            )
        })
        .await?;

    match selected {
        None => aci.edit_quick_info(ctx, "Aborted").await?,
        Some(s) => {
            aci.edit_quick_info(ctx, format!("Selected {} bosses", s.len()))
                .await?
        }
    };

    Ok(())
}
