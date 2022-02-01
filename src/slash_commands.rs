use std::str::FromStr;

use serenity::{
    builder::{CreateApplicationCommand, CreateApplicationCommandPermissions},
    client::Context,
    model::interactions::application_command::{
        ApplicationCommand, ApplicationCommandInteraction, ApplicationCommandPermissionType,
    },
};

use tracing::error;

use crate::data::ConfigValues;

#[derive(Debug)]
pub struct SlashCommandParseError(String);

impl std::fmt::Display for SlashCommandParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Unknown slash command: {}", self.0)
    }
}

impl std::error::Error for SlashCommandParseError {}

mod config;
mod register;
mod testing;
mod training;
mod training_boss;

/// All slash commands
#[derive(Debug)]
pub enum AppCommands {
    Register,
    Unregister,
    Training,
    TrainingBoss,
    Testing,
    Config,
}

/// All commands that should be created when the bot starts
const DEFAULT_COMMANDS: [AppCommands; 6] = [
    AppCommands::Register,
    AppCommands::Unregister,
    AppCommands::Training,
    AppCommands::TrainingBoss,
    AppCommands::Config,
    AppCommands::Testing,
];

impl FromStr for AppCommands {
    type Err = SlashCommandParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            register::CMD_REGISTER => Ok(Self::Register),
            register::CMD_UNREGISTER => Ok(Self::Unregister),
            training::CMD_TRAINING => Ok(Self::Training),
            training_boss::CMD_TRAINING_BOSS => Ok(Self::TrainingBoss),
            config::CMD_CONFIG => Ok(Self::Config),
            testing::CMD_TESTING => Ok(Self::Testing),
            _ => Err(SlashCommandParseError(s.to_owned())),
        }
    }
}

impl AppCommands {
    pub fn create(&self) -> CreateApplicationCommand {
        match self {
            Self::Register => register::create_reg(),
            Self::Unregister => register::create_unreg(),
            Self::Training => training::create(),
            Self::TrainingBoss => training_boss::create(),
            Self::Config => config::create(),
            Self::Testing => testing::create(),
        }
    }

    pub fn create_default() -> Vec<CreateApplicationCommand> {
        DEFAULT_COMMANDS
            .iter()
            .map(Self::create)
            .collect::<Vec<_>>()
    }

    pub fn permission(
        &self,
        ac: &ApplicationCommand,
        conf: &ConfigValues,
    ) -> CreateApplicationCommandPermissions {
        let mut perms = CreateApplicationCommandPermissions::default();
        perms.id(ac.id.0);

        // Here are all the configurations for Slash Command Permissions
        match self {
            Self::Training | Self::TrainingBoss | Self::Config | Self::Testing => perms
                .create_permissions(|p| {
                    p.permission(true)
                        .kind(ApplicationCommandPermissionType::Role)
                        .id(conf.squadmaker_role_id.0)
                }),
            Self::Register | Self::Unregister => perms.create_permissions(|p| {
                p.permission(true)
                    .kind(ApplicationCommandPermissionType::Role)
                    .id(conf.main_guild_id.0) // Guild id is same as @everyone
            }),
        };

        perms
    }

    async fn handle(&self, ctx: &Context, aci: &ApplicationCommandInteraction) {
        match self {
            Self::Register => register::handle_reg(ctx, aci).await,
            Self::Unregister => register::handle_unreg(ctx, aci).await,
            Self::Training => training::handle(ctx, aci).await,
            Self::TrainingBoss => training_boss::handle(ctx, aci).await,
            Self::Config => config::handle(ctx, aci).await,
            Self::Testing => testing::handle(ctx, aci).await,
        }
    }
}

pub async fn slash_command_interaction(ctx: &Context, aci: &ApplicationCommandInteraction) {
    // Consider reworking to aci.data.id
    match AppCommands::from_str(&aci.data.name) {
        Ok(cmd) => cmd.handle(ctx, aci).await,
        Err(e) => error!("{}", e),
    }
}

pub mod helpers {
    use std::collections::HashMap;

    use serde_json::Value;
    use serenity::model::interactions::application_command::ApplicationCommandInteractionDataOption;

    /// Helps to quickly access commands
    pub fn command_map(opt: &ApplicationCommandInteractionDataOption) -> HashMap<String, Value> {
        opt.options
            .iter()
            .filter_map(|o| {
                if let Some(val) = &o.value {
                    Some((o.name.clone(), val.clone()))
                } else {
                    None
                }
            })
            .collect()
    }
}
