use std::collections::HashMap;
use std::convert::TryInto;

use anyhow::{anyhow, bail, Context as ErrContext, Result};
use serenity::{
    builder::{CreateApplicationCommand, CreateEmbed},
    client::Context,
    model::interactions::{
        application_command::{
            ApplicationCommandInteraction, ApplicationCommandInteractionDataOption,
            ApplicationCommandOptionType,
        },
        InteractionApplicationCommandCallbackDataFlags, InteractionResponseType,
    },
};

use crate::{
    db,
    embeds::{self, CrossroadsEmbeds},
    logging::*,
};

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
    let cmds = option
        .options
        .iter()
        .map(|o| (o.name.clone(), o))
        .collect::<HashMap<_, _>>();

    let name = cmds
        .get("name")
        .and_then(|d| d.value.as_ref())
        .and_then(|d| d.as_str())
        .ok_or(anyhow!("name is required"))?
        .to_owned();

    let repr = cmds
        .get("repr")
        .and_then(|d| d.value.as_ref())
        .and_then(|d| d.as_str())
        .ok_or(anyhow!("repr is required"))?
        .to_owned();

    let wing: i32 = cmds
        .get("wing")
        .and_then(|d| d.value.as_ref())
        .and_then(|d| d.as_i64())
        .ok_or(anyhow!("wing is required"))?
        .try_into()?;

    let position: i32 = cmds
        .get("position")
        .and_then(|d| d.value.as_ref())
        .and_then(|d| d.as_i64())
        .ok_or(anyhow!("position is required"))?
        .try_into()?;

    trace.step("Inserting into database");
    let boss = db::TrainingBoss::insert(ctx, name, repr, wing, position)
        .await
        .map_err_reply(|what| super::helpers::quick_ch_msg_with_src(ctx, aci, what))
        .await?;

    trace.step("Replying with data");
    aci.create_interaction_response(ctx, |r| {
        r.kind(InteractionResponseType::ChannelMessageWithSource);
        r.interaction_response_data(|d| {
            d.flags(InteractionApplicationCommandCallbackDataFlags::EPHEMERAL);
            let mut emb = CreateEmbed::xdefault();
            emb.title("New Training Boss");
            emb.field("Id", boss.id, true);
            emb.field("Name", boss.name, true);
            emb.field("Repr", boss.repr, true);
            emb.field("Wing", boss.wing, true);
            emb.field("Boss", boss.position, true);
            d.add_embed(emb)
        })
    })
    .await?;

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
        .map_err_reply(|what| super::helpers::quick_ch_msg_with_src(ctx, aci, what))
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
