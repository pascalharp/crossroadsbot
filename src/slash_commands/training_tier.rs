use anyhow::{bail, Context as ErrContext, Result};
use diesel::QueryResult;
use serenity::{
    builder::{CreateApplicationCommand, CreateEmbed},
    client::Context,
    model::{
        id::RoleId,
        interactions::{
            application_command::{
                ApplicationCommandInteraction, ApplicationCommandInteractionDataOption,
                ApplicationCommandOptionType,
            },
            InteractionApplicationCommandCallbackDataFlags, InteractionResponseType,
        },
        mention::Mention,
        Permissions,
    },
};
use serenity_tools::interactions::ApplicationCommandInteractionExt;

use crate::{
    db,
    embeds::CrossroadsEmbeds,
    logging::{log_discord, LogTrace, ReplyHelper},
};

pub(super) const CMD_TRAINING_TIER: &str = "training_tier";

pub fn create() -> CreateApplicationCommand {
    let mut app = CreateApplicationCommand::default();
    app.name(CMD_TRAINING_TIER);
    app.description("Training tier configurations");
    app.default_member_permissions(Permissions::empty());
    app.dm_permission(false);
    app.create_option(|o| {
        o.kind(ApplicationCommandOptionType::SubCommand);
        o.name("new");
        o.description("Create a new tier");
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::String);
            o.required(true);
            o.name("name");
            o.description("Name of the tier")
        })
    });
    app.create_option(|o| {
        o.kind(ApplicationCommandOptionType::SubCommand);
        o.name("add");
        o.description("add a discord role to the tier");
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::String);
            o.required(true);
            o.name("name");
            o.description("Name of the tier to add to")
        });
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::Role);
            o.required(true);
            o.name("role");
            o.description("The discord role to add to the tier")
        })
    });
    app.create_option(|o| {
        o.kind(ApplicationCommandOptionType::SubCommand);
        o.name("remove");
        o.description("Remove a discord role from the tier");
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::String);
            o.required(true);
            o.name("name");
            o.description("Name of the tier to remove from")
        });
        o.create_sub_option(|o| {
            o.kind(ApplicationCommandOptionType::Role);
            o.required(true);
            o.name("role");
            o.description("The discord role to remove from the tier")
        })
    });
    app.create_option(|o| {
        o.kind(ApplicationCommandOptionType::SubCommand);
        o.name("list");
        o.description("List all tiers and the corresponding roles")
    });
    app
}

pub async fn handle(ctx: &Context, aci: &ApplicationCommandInteraction) {
    log_discord(ctx, aci, |trace| async move {
        trace.step("Parsing command");
        if let Some(sub) = aci.data.options.get(0) {
            match sub.name.as_ref() {
                "new" => new(ctx, aci, sub, trace).await,
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

async fn new(
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

    trace.step("Saving new tier");
    let tier = db::Tier::insert(ctx, name.to_string())
        .await
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?;

    aci.create_quick_success(
        ctx,
        format!("Created Tier: {} with id {}", tier.name, tier.id),
        true,
    )
    .await?;

    Ok(())
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
    let role = RoleId::from(
        cmds.get("role")
            .and_then(|v| v.as_str())
            .context("Unexpected missing field role")
            .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
            .await?
            .parse::<u64>()
            .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
            .await?,
    );

    trace.step("Loading tier");
    let tier = match db::Tier::by_name(ctx, name.to_string()).await {
        Ok(t) => t,
        Err(diesel::NotFound) => {
            Err(diesel::NotFound)
                .context(format!("Tier **{}** does not exist", name))
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

    trace.step("Save new discord role");
    tier.add_discord_role(ctx, role.0)
        .await
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?;

    aci.create_quick_success(
        ctx,
        format!("Added {} to {}", Mention::from(role), tier.name),
        true,
    )
    .await?;

    Ok(())
}

async fn remove(
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
    let role = RoleId::from(
        cmds.get("role")
            .and_then(|v| v.as_str())
            .context("Unexpected missing field role")
            .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
            .await?
            .parse::<u64>()
            .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
            .await?,
    );

    trace.step("Loading tier");
    let tier = match db::Tier::by_name(ctx, name.to_string()).await {
        Ok(t) => t,
        Err(diesel::NotFound) => {
            Err(diesel::NotFound)
                .context(format!("Tier **{}** does not exist", name))
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

    trace.step("Looking for discord role mapping");
    let mapping = tier
        .get_tier_mapping_by_discord_role(ctx, role.0)
        .await
        .context("The role is not linked to this tier")
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?;

    trace.step("Removing mapping");
    mapping
        .delete(ctx)
        .await
        .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
        .await?;

    aci.create_quick_success(
        ctx,
        format!("Removed {} from {}", Mention::from(role), tier.name),
        true,
    )
    .await?;

    Ok(())
}

async fn list(ctx: &Context, aci: &ApplicationCommandInteraction, trace: LogTrace) -> Result<()> {
    trace.step("Loading tiers");
    let tiers = db::Tier::all(ctx).await?;
    trace.step("Loading roles");
    let tiers = serenity::futures::future::join_all(tiers.into_iter().map(|t| async {
        let r = t.get_discord_roles(ctx).await?;
        Ok::<(db::Tier, Vec<db::TierMapping>), diesel::result::Error>((t, r))
    }))
    .await
    .into_iter()
    .collect::<QueryResult<Vec<_>>>()
    .context("Unexpected error loading tier information")
    .map_err_reply(|what| aci.create_quick_error(ctx, what, true))
    .await?;

    let mut emb = CreateEmbed::xdefault();
    emb.title("Tiers");
    for (tier, roles) in &tiers {
        emb.field(
            &tier.name,
            if !roles.is_empty() {
                roles
                    .iter()
                    .map(|r| Mention::from(RoleId::from(r.discord_role_id as u64)).to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                String::from("_None_")
            },
            true,
        );
    }

    aci.create_interaction_response(ctx, |r| {
        r.kind(InteractionResponseType::ChannelMessageWithSource);
        r.interaction_response_data(|d| {
            if !tiers.is_empty() {
                d.add_embed(emb);
            } else {
                d.content("There are no tiers set up");
            }
            d.flags(InteractionApplicationCommandCallbackDataFlags::EPHEMERAL)
        })
    })
    .await?;

    Ok(())
}
