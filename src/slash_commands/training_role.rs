use anyhow::{anyhow, bail, Context as ErrContext, Result};
use serenity::{
    builder::{CreateApplicationCommand, CreateEmbed},
    client::Context,
    model::{
        guild::Guild,
        id::EmojiId,
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

use serenity_tools::{builder::CreateEmbedExt, interactions::ApplicationCommandInteractionExt};

use crate::{data::ConfigValuesData, db, embeds::CrossroadsEmbeds, logging::*};

pub(super) const CMD_TRAINING_ROLE: &'static str = "training_role";
pub fn create() -> CreateApplicationCommand {
    let mut app = CreateApplicationCommand::default();
    app.name(CMD_TRAINING_ROLE);
    app.description("Testing");
    app.default_permission(false);
    app.create_option(|o| {
        o.kind(ApplicationCommandOptionType::SubCommand);
        o.name("add");
        o.description("Add a new training role");
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::String);
            o.name("name");
            o.description("The full name of the role");
            o.required(true)
        });
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::String);
            o.name("repr");
            o.description(
                "The short identifier for the role. Must be unique and may not contain spaces",
            );
            o.required(true)
        });
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::String);
            o.name("emoji");
            o.description("Use \"/config emoji_list\" for a list of options");
            o.required(true)
        });
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::String);
            o.name("priority");
            o.description("Higher priorities are listed first in select menus");
            o.required(true);
            o.add_string_choice("Very High Priority", "very_high");
            o.add_string_choice("High Priority", "high");
            o.add_string_choice("Normal", "normal");
            o.add_string_choice("Low Priority", "low");
            o.add_string_choice("Very Low Priority", "very_low")
        })
    });
    app.create_option(|o| {
        o.kind(ApplicationCommandOptionType::SubCommand);
        o.name("remove");
        o.description("Remove a role. This only deactivates the role for future training's. Old training's are not affected");
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::String);
            o.name("repr");
            o.description("The short identifier for the role.");
            o.required(true)
        })
    });
    app.create_option(|o| {
        o.kind(ApplicationCommandOptionType::SubCommand);
        o.name("list");
        o.description("List all available training roles")
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
                "list" => list(ctx, aci, trace).await,
                _ => bail!("{} not yet available", sub.name),
            }
        } else {
            bail!("Invalid command")
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
    let cmds = super::helpers::command_map(option);

    let name = cmds
        .get("name")
        .and_then(|v| v.as_str())
        .context("Unexpected missing field name")
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?;

    let repr = cmds
        .get("repr")
        .and_then(|v| v.as_str())
        .context("Unexpected missing field repr")
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?;

    if repr.contains(' ') {
        Err(anyhow!("repr may not contain spaces"))
            .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
            .await?;
    }

    let priority = match cmds
        .get("priority")
        .and_then(|v| v.as_str())
        .context("Unexpected missing field priority")
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?
    {
        "very_high" => Ok(4),
        "high" => Ok(3),
        "normal" => Ok(2),
        "low" => Ok(1),
        "very_low" => Ok(0),
        s => Err(anyhow!("Unexpected priority value: {}", s)),
    }
    .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
    .await?;

    let emoji_str = cmds
        .get("emoji")
        .and_then(|v| v.as_str())
        .context("Unexpected missing field emoji")
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?;

    trace.step("Searching for emoji");
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

    trace.step("Saving role");
    let training_role = db::Role::insert(
        ctx,
        name.to_string(),
        repr.to_string(),
        emoji_id.0,
        Some(priority),
    )
    .await
    .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
    .await?;

    aci.create_quick_success(ctx, format!("New Role {}", training_role), true)
        .await?;

    Ok(())
}

async fn remove(
    ctx: &Context,
    aci: &ApplicationCommandInteraction,
    option: &ApplicationCommandInteractionDataOption,
    trace: LogTrace,
) -> Result<()> {
    trace.step("Load role");
    let cmds = super::helpers::command_map(option);
    let repr = cmds
        .get("repr")
        .and_then(|v| v.as_str())
        .context("Unexpected missing field repr")
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?;

    let role = match db::Role::by_repr(ctx, repr.to_string()).await {
        Ok(r) => r,
        Err(diesel::NotFound) => {
            Err(diesel::NotFound)
                .context(format!("The role with the repr {} does not exist", repr))
                .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
                .await?;
            return Ok(());
        }
        Err(e) => {
            Err(e)
                .context("Unexpected error")
                .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
                .await?;
            return Ok(());
        }
    };

    role.deactivate(ctx)
        .await
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?;

    aci.create_quick_success(ctx, format!("Role removed: {}", repr), true)
        .await?;

    Ok(())
}

async fn list(ctx: &Context, aci: &ApplicationCommandInteraction, trace: LogTrace) -> Result<()> {
    trace.step("Load roles from db");
    let mut roles = db::Role::all_active(ctx).await?;
    roles.sort_by_key(|r| r.title.clone());
    roles.sort_by_key(|r| r.priority);

    let mut emb = CreateEmbed::xdefault();
    emb.fields_chunked_fmt(
        &roles,
        |r| {
            format!(
                "{} | {} | {}",
                Mention::from(EmojiId::from(r.emoji as u64)),
                r.repr,
                r.title
            )
        },
        "Roles",
        true,
        10,
    );
    emb.title("Training Roles");

    aci.create_interaction_response(ctx, |r| {
        r.kind(InteractionResponseType::ChannelMessageWithSource);
        r.interaction_response_data(|d| {
            d.flags(InteractionApplicationCommandCallbackDataFlags::EPHEMERAL);
            d.add_embed(emb)
        })
    })
    .await?;
    Ok(())
}
