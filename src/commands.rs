use crate::data::*;
use dashmap::DashSet;
use serenity::{
    client::bridge::gateway::ShardMessenger,
    collector::{message_collector::*, reaction_collector::*},
    framework::standard::{
        help_commands,
        macros::{check, help},
        Args, CommandGroup, CommandOptions, CommandResult, HelpOptions, Reason,
    },
    model::prelude::*,
    prelude::*,
};
use std::{collections::HashSet, error::Error, fmt, sync::Arc, time::Duration};

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

#[help]
#[individual_command_tip = "Hello! This is a list of all Crossroads Inn Bot commands\n\
If you want more information about a specific command, just pass the command as argument."]
#[command_not_found_text = "Could not find: `{}`."]
#[max_levenshtein_distance(3)]
#[indention_prefix = "-"]
#[lacking_conditions = "strike"]
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
mod misc;
pub use misc::MISC_GROUP;

mod signup;
pub use signup::SIGNUP_GROUP;

mod config;
pub use config::CONFIG_GROUP;

mod role;
pub use role::ROLE_GROUP;

mod training;
pub use training::TRAINING_GROUP;

mod tier;
pub use tier::TIER_GROUP;
