use std::convert::TryInto;

use anyhow::{anyhow, bail, Context as ErrContext, Result};
use serenity::{
    builder::{CreateApplicationCommand, CreateEmbed},
    client::Context,
    model::{interactions::{
        application_command::{
            ApplicationCommandInteraction, ApplicationCommandInteractionDataOption,
            ApplicationCommandOptionType,
        },
        InteractionApplicationCommandCallbackDataFlags, InteractionResponseType,
    }, guild::Guild, misc::Mention, id::EmojiId},
};
use url::Url;

use crate::{
    db,
    embeds::CrossroadsEmbeds,
    logging::*, data::ConfigValuesData,
};

use serenity_tools::interactions::ApplicationCommandInteractionExt;

use super::helpers::command_map;

pub const CMD_TRAINING_BOSS: &str = "training_boss";

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
        .ok_or(anyhow!("name is required"))
        .map_err_reply(|what| aci.create_quick_error(ctx, InteractionResponseType::ChannelMessageWithSource, what, true))
        .await?
        .to_owned();

    let repr = cmds
        .get("repr")
        .and_then(|d| d.as_str())
        .ok_or(anyhow!("repr is required"))
        .map_err_reply(|what| aci.create_quick_error(ctx, InteractionResponseType::ChannelMessageWithSource, what, true))
        .await?
        .to_owned();

    let wing: i32 = cmds
        .get("wing")
        .and_then(|d| d.as_i64())
        .ok_or(anyhow!("wing is required"))
        .map_err_reply(|what| aci.create_quick_error(ctx, InteractionResponseType::ChannelMessageWithSource, what, true))
        .await?
        .try_into()?;

    let position: i32 = cmds
        .get("position")
        .and_then(|d| d.as_i64())
        .ok_or(anyhow!("position is required"))
        .map_err_reply(|what| aci.create_quick_error(ctx, InteractionResponseType::ChannelMessageWithSource, what, true))
        .await?
        .try_into()?;

    let url = cmds
        .get("link")
        .and_then(|d| d.as_str())
        .and_then(|s| Some(s.parse::<Url>()));

    let url = match url {
        None => None,
        Some(url) => {
            let u = url
                .context("Could not parse Url")
                .map_err_reply(|what| aci.create_quick_error(ctx, InteractionResponseType::ChannelMessageWithSource, what, true))
                .await?;

            if u.scheme() != "https" {
                Err(anyhow!("Only https is allowed: {}", u))
                    .map_err_reply(|what| aci.create_quick_error(ctx, InteractionResponseType::ChannelMessageWithSource, what, true))
                    .await?;
            }

            Some(u)
        }
    };

    let emoji_str = cmds
        .get("emoji")
        .and_then(|d| d.as_str())
        .context("emoji is required")
        .map_err_reply(|what| aci.create_quick_error(ctx, InteractionResponseType::ChannelMessageWithSource, what, true))
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

    let emoji_id = match emoji_guild.emojis(ctx).await?.into_iter().find(|e| e.name == emoji_str) {
        Some(e) => e.id,
        None => {
            Err(anyhow!("The emoji: {} was not found in the emoji guild", emoji_str))
                .map_err_reply(|what| aci.create_quick_error(ctx, InteractionResponseType::ChannelMessageWithSource, what, true))
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
            emb.field("Emoji", Mention::from(EmojiId::from(emoji_id)), true);
            if let Some(url) = &url {
                emb.field("Url", url, false);
            } else {
                emb.field("Url", "No url provided", false);
            }
            d.add_embed(emb)
        })
    })
    .await?;

    trace.step("Waiting for confirm TODO");
    //TODO
    Err(anyhow!("TODO"))?;

    //trace.step("Inserting into database");
    //let boss = db::TrainingBoss::insert(ctx, name, repr, wing, position)
    //    .await
    //    .map_err_reply(|what| aci.create_quick_error(ctx, InteractionResponseType::ChannelMessageWithSource, what, true))
    //    .await?;

    Ok(())
}

async fn remove(
    _ctx: &Context,
    _aci: &ApplicationCommandInteraction,
    _option: &ApplicationCommandInteractionDataOption,
    _trace: LogTrace,
) -> Result<()> {
    Ok(())
}

async fn list(
    ctx: &Context,
    aci: &ApplicationCommandInteraction,
    _option: &ApplicationCommandInteractionDataOption,
    trace: LogTrace,
) -> Result<()> {
    trace.step("Loading training bosses");
    let bosses = db::TrainingBoss::all(ctx)
        .await
        .context("Failed to load training bosses =(")
        .map_err_reply(|what| aci.create_quick_error(ctx, InteractionResponseType::ChannelMessageWithSource, what, true))
        .await?;

    trace.step("Replying with data");
    aci.create_interaction_response(ctx, |r| {
        r.kind(InteractionResponseType::ChannelMessageWithSource);
        r.interaction_response_data(|d| {
            d.flags(InteractionApplicationCommandCallbackDataFlags::EPHEMERAL);
            for chunk in bosses.chunks(25) {
                // cause of field limits
                let mut emb = CreateEmbed::xdefault();
                for boss in chunk.iter() {
                    emb.field(
                        &boss.name,
                        format!(
                            "Id: {}\nRepr: {}\nWing: {}\nBoss: {}",
                            boss.id, boss.repr, boss.wing, boss.position
                        ),
                        true,
                    );
                }
                d.add_embed(emb);
            }
            d
        })
    })
    .await?;

    Ok(())
}
