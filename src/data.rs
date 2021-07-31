use crate::signup_board::SignupBoard;
use crate::db::DBPool;
use dashmap::DashSet;
use serenity::{model::prelude::*, prelude::*};
use std::sync::Arc;

pub static GLOB_COMMAND_PREFIX: &str = "~";

pub struct ConfigValues {
    pub main_guild_id: GuildId,
    pub admin_role_id: RoleId,
    pub squadmaker_role_id: RoleId,
    pub emoji_guild_id: GuildId,
}

pub static INFO_LOG_NAME: &str = "info_log";
pub static ERROR_LOG_NAME: &str = "error_log";

pub struct LogConfig {
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

pub struct LogConfigData;
impl TypeMapKey for LogConfigData {
    type Value = Arc<RwLock<LogConfig>>;
}

pub struct SignupBoardData;
impl TypeMapKey for SignupBoardData {
    type Value = Arc<RwLock<SignupBoard>>;
}

pub struct DBPoolData;
impl TypeMapKey for DBPoolData {
    type Value = Arc<DBPool>;
}
