use dashmap::DashSet;
use serenity::{model::prelude::*, prelude::*};
use std::sync::Arc;

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
