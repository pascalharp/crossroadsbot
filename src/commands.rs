use crate::data::*;

use serenity::{
    framework::standard::{
        help_commands,
        macros::{check, help},
        Args, CommandGroup, CommandOptions, CommandResult, HelpOptions, Reason,
    },
    model::prelude::*,
    prelude::*,
};
use std::collections::HashSet;

// --- Checks ---
#[check]
#[name = "admin_role"]
async fn admin_rol_check(
    ctx: &Context,
    msg: &Message,
    _: &mut Args,
    _: &CommandOptions,
) -> Result<(), Reason> {
    let (g, r) = {
        let config = ctx
            .data
            .read()
            .await
            .get::<ConfigValuesData>()
            .unwrap()
            .clone();
        (config.main_guild_id, config.admin_role_id)
    };

    match msg.author.has_role(ctx, g, r).await {
        Ok(b) => match b {
            true => Ok(()),
            false => Err(Reason::Log(String::from("No permissions"))),
        },
        Err(_) => Err(Reason::Unknown),
    }
}

// --- Checks ---
#[check]
#[name = "squadmaker_role"]
async fn squadmaker_rol_check(
    ctx: &Context,
    msg: &Message,
    _: &mut Args,
    _: &CommandOptions,
) -> Result<(), Reason> {
    let (g, r) = {
        let config = ctx
            .data
            .read()
            .await
            .get::<ConfigValuesData>()
            .unwrap()
            .clone();
        (config.main_guild_id, config.squadmaker_role_id)
    };

    match msg.author.has_role(ctx, g, r).await {
        Ok(b) => match b {
            true => Ok(()),
            false => Err(Reason::Log(String::from("No permissions"))),
        },
        Err(_) => Err(Reason::Unknown),
    }
}

#[help]
#[individual_command_tip = "Hello! This is a list of all Crossroads Inn Bot commands\n\
If you want more information about a specific command, just pass the command as argument."]
#[command_not_found_text = "Could not find: `{}`."]
#[max_levenshtein_distance(3)]
#[indention_prefix = "-"]
#[lacking_conditions = "hide"]
async fn help_cmd(
    context: &Context,
    msg: &Message,
    args: Args,
    help_options: &'static HelpOptions,
    groups: &[&'static CommandGroup],
    owners: HashSet<UserId>,
) -> CommandResult {
    let _ = help_commands::with_embeds(context, msg, args, help_options, groups, owners).await;
    Ok(())
}

// --- Command Setup ---
mod tier;
pub use tier::TIER_GROUP;
