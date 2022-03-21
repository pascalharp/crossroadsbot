use std::{convert::TryInto, time::Duration};

use anyhow::{anyhow, bail, Context as ErrContext, Result};
use itertools::Itertools;
use serenity::{
    builder::{CreateApplicationCommand, CreateEmbed},
    client::Context,
    model::{
        guild::Guild,
        interactions::{
            application_command::{
                ApplicationCommandInteraction, ApplicationCommandInteractionDataOption,
                ApplicationCommandOptionType,
            },
            InteractionApplicationCommandCallbackDataFlags, InteractionResponseType,
        },
        misc::Mention,
    },
};
use url::Url;

use crate::{
    data::ConfigValuesData,
    db::{self, TrainingBoss},
    embeds::CrossroadsEmbeds,
    logging::*,
};

use serenity_tools::{
    builder::CreateComponentsExt,
    collectors::MessageCollectorExt,
    components::Button,
    interactions::{ApplicationCommandInteractionExt, MessageComponentInteractionExt},
};

use super::helpers::command_map;

pub(super) const CMD_TRAINING_BOSS: &str = "training_boss";

pub fn create() -> CreateApplicationCommand {
    let mut app = CreateApplicationCommand::default();
    app.name(CMD_TRAINING_BOSS);
    app.description("Manage bosses for training");
    app.default_permission(false);
    app.create_option(|o| {
        o.kind(ApplicationCommandOptionType::SubCommand);
        o.name("add");
        o.description("Add a boss");
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::String);
            o.name("name");
            o.description("The full name of the boss");
            o.required(true)
        });
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::String);
            o.name("repr");
            o.description(
                "A short identifier for the boss. Has to be unique. Will be exported on download",
            );
            o.required(true)
        });
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::Integer);
            o.name("wing");
            o.description("The wing the boss belongs to");
            o.required(true);
            o.add_int_choice("Wing 1", 1);
            o.add_int_choice("Wing 2", 2);
            o.add_int_choice("Wing 3", 3);
            o.add_int_choice("Wing 4", 4);
            o.add_int_choice("Wing 5", 5);
            o.add_int_choice("Wing 6", 6);
            o.add_int_choice("Wing 7", 7)
        });
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::Integer);
            o.name("position");
            o.description("Which boss it is in the specified wing");
            o.required(true);
            o.add_int_choice("Boss 1", 1);
            o.add_int_choice("Boss 2", 2);
            o.add_int_choice("Boss 3", 3);
            o.add_int_choice("Boss 4", 4);
            o.add_int_choice("Boss 5", 5)
        });
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::String);
            o.name("emoji");
            o.required(true);
            o.description("The emoji for the boss")
        });
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::String);
            o.name("link");
            o.description("A Link to more information about the boss. Eg the wiki")
        })
    });
    app.create_option(|o| {
        o.kind(ApplicationCommandOptionType::SubCommand);
        o.name("remove");
        o.description("Remove a boss");
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::String);
            o.name("repr");
            o.description("The unique identifier of the boss");
            o.required(true)
        })
    });
    app.create_option(|o| {
        o.kind(ApplicationCommandOptionType::SubCommand);
        o.name("list");
        o.description("List all available bosses")
    });
    app
}

pub async fn handle(ctx: &Context, aci: &ApplicationCommandInteraction) {
    log_discord(ctx, aci, |trace| async move {
        trace.step("Parsing command");
        if let Some(sub) = aci.data.options.get(0) {
            match sub.name.as_ref() {
                "add" => add(ctx, aci, sub, trace).await,
                "remove" => remove(ctx, aci, sub, trace).await,
                "list" => list(ctx, aci, sub, trace).await,
                _ => bail!("{} not yet available", sub.name),
            }
        } else {
            bail!("Invalid command");
        }
    })
    .await;
}

async fn add(
    ctx: &Context,
    aci: &ApplicationCommandInteraction,
    option: &ApplicationCommandInteractionDataOption,
    trace: LogTrace,
) -> Result<()> {
    let cmds = command_map(option);

    let name = cmds
        .get("name")
        .and_then(|d| d.as_str())
        .context("name is required")
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?
        .to_owned();

    let repr = cmds
        .get("repr")
        .and_then(|d| d.as_str())
        .context("repr is required")
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?
        .to_owned();

    let wing: i32 = cmds
        .get("wing")
        .and_then(|d| d.as_i64())
        .context("wing is required")
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?
        .try_into()?;

    let position: i32 = cmds
        .get("position")
        .and_then(|d| d.as_i64())
        .context("position is required")
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?
        .try_into()?;

    let url = cmds
        .get("link")
        .and_then(|d| d.as_str())
        .map(|s| s.parse::<Url>());

    let url = match url {
        None => None,
        Some(url) => {
            let u = url
                .context("Could not parse Url")
                .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
                .await?;

            if u.scheme() != "https" {
                Err(anyhow!("Only https is allowed: {}", u))
                    .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
                    .await?;
            }

            Some(u)
        }
    };

    let emoji_str = cmds
        .get("emoji")
        .and_then(|d| d.as_str())
        .context("emoji is required")
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?;

    // load all emojis from discord emoji guild
    let gid = ctx
        .data
        .read()
        .await
        .get::<ConfigValuesData>()
        .unwrap()
        .emoji_guild_id;
    let emoji_guild = Guild::get(ctx, gid).await?;

    let emoji_id = match emoji_guild
        .emojis(ctx)
        .await?
        .into_iter()
        .find(|e| e.name == emoji_str)
    {
        Some(e) => e.id,
        None => {
            Err(anyhow!(
                "The emoji {} was not found in the emoji guild",
                emoji_str
            ))
            .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
            .await?;
            return Ok(());
        }
    };

    trace.step("Replying with data");
    aci.create_interaction_response(ctx, |r| {
        r.kind(InteractionResponseType::ChannelMessageWithSource);
        r.interaction_response_data(|d| {
            d.flags(InteractionApplicationCommandCallbackDataFlags::EPHEMERAL);
            let mut emb = CreateEmbed::xdefault();
            emb.title("New Training Boss");
            emb.field("Name", &name, false);
            emb.field("Repr", &repr, true);
            emb.field("Wing", wing, true);
            emb.field("Boss", position, true);
            emb.field("Emoji", Mention::from(emoji_id), true);
            if let Some(url) = &url {
                emb.field("Url", url, false);
            } else {
                emb.field("Url", "No url provided", false);
            }
            d.add_embed(emb);
            d.components(|c| c.confirm_abort_row())
        })
    })
    .await?;

    let msg = aci.get_interaction_response(ctx).await?;
    trace.step("Waiting for confirm");

    if let Some(react) = msg
        .await_confirm_abort_interaction(ctx)
        .timeout(Duration::from_secs(60))
        .await
    {
        react.defer(ctx).await?;
        match react.parse_button()? {
            Button::Confirm => {
                trace.step("Confirmed, inserting to database");
                let boss = db::TrainingBoss::insert(ctx, name, repr, wing, position, emoji_id, url)
                    .await
                    .map_err_reply(|what| aci.edit_quick_error(ctx, what))
                    .await?;
                aci.edit_quick_info(ctx, format!("Created boss:\n{}", boss))
                    .await?;
            }
            Button::Abort => {
                trace.step("Aborted");
                aci.edit_quick_info(ctx, "Aborted").await?;
            }
            _ => bail!("Unexpected interaction"),
        }
    } else {
        Err(anyhow!("Timed out"))
            .map_err_reply(|what| aci.edit_quick_info(ctx, what))
            .await?;
    }

    Ok(())
}

async fn remove(
    ctx: &Context,
    aci: &ApplicationCommandInteraction,
    option: &ApplicationCommandInteractionDataOption,
    trace: LogTrace,
) -> Result<()> {
    let boss_repr = option
        .options
        .get(0)
        .and_then(|o| o.value.as_ref())
        .and_then(|o| o.as_str())
        .context("Unexpected missing field")
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?;

    trace.step("Loading boss");
    let boss = match db::TrainingBoss::by_repr(ctx, boss_repr.to_string()).await {
        Ok(o) => o,
        Err(diesel::NotFound) => {
            Err(diesel::NotFound)
                .context(format!("Boss {} does not exist", boss_repr))
                .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
                .await?
        }
        Err(e) => {
            Err(e)
                .context("Unexpected error")
                .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
                .await?
        }
    };

    trace.step("Deleting boss");
    boss.delete(ctx)
        .await
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?;

    aci.create_quick_success(ctx, format!("Deleted: {}", boss), true)
        .await?;

    Ok(())
}

async fn list(
    ctx: &Context,
    aci: &ApplicationCommandInteraction,
    _option: &ApplicationCommandInteractionDataOption,
    trace: LogTrace,
) -> Result<()> {
    trace.step("Loading training bosses");
    let mut bosses = db::TrainingBoss::all(ctx)
        .await
        .context("Failed to load training bosses =(")
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?;

    trace.step("Sorting bosses");

    bosses.sort_by_key(|b| b.wing);
    let mut bosses_grouped: Vec<(i32, Vec<TrainingBoss>)> = Vec::new();
    for (w, b) in &bosses.into_iter().group_by(|b| b.wing) {
        bosses_grouped.push((w, b.collect()));
    }
    bosses_grouped.sort_by_key(|(k, _)| *k);

    trace.step("Replying with data");
    aci.create_interaction_response(ctx, |r| {
        r.kind(InteractionResponseType::ChannelMessageWithSource);
        r.interaction_response_data(|d| {
            d.flags(InteractionApplicationCommandCallbackDataFlags::EPHEMERAL);
            d.create_embed(|e| {
                for (w, b) in bosses_grouped {
                    e.field(
                        format!("Wing {}", w),
                        b.iter()
                            .map(|b| b.to_string())
                            .collect::<Vec<_>>()
                            .join("\n"),
                        false,
                    );
                }
                e
            })
        })
    })
    .await?;

    Ok(())
}
