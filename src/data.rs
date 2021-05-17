use crate::utils::*;
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

// --- Global Config ---
pub struct ConfigValues {
    pub main_guild_id: GuildId,
    pub admin_role_id: RoleId,
    pub squadmaker_role_id: RoleId,
    pub emoji_guild_id: GuildId,
}

pub struct LogginConfig {
    pub info: Option<ChannelId>,
    pub error: Option<ChannelId>,
}

// --- Global Data ---
pub struct ConversationLock;
impl TypeMapKey for ConversationLock {
    type Value = Arc<DashSet<UserId>>;
}

pub struct ConfigValuesData;
impl TypeMapKey for ConfigValuesData {
    type Value = Arc<ConfigValues>;
}

pub struct LogginConfigData;
impl TypeMapKey for LogginConfigData {
    type Value = Arc<RwLock<LogginConfig>>;
}


