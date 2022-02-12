use crate::db::DBPool;
use crate::signup_board::SignupBoard;
use serenity::{model::prelude::*, prelude::*};
use std::sync::Arc;

pub struct ConfigValues {
    pub main_guild_id: GuildId,
    pub admin_role_id: RoleId,
    pub squadmaker_role_id: RoleId,
    pub emoji_guild_id: GuildId,
}

pub static INFO_LOG_NAME: &str = "log_channel_id";

pub struct LogConfig {
    pub log: Option<ChannelId>,
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
