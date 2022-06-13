use std::str::FromStr;

use serenity::{
    builder::CreateApplicationCommand, client::Context,
    model::interactions::application_command::ApplicationCommandInteraction,
};

use tracing::error;

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
mod training;
mod training_boss;
mod training_role;
mod training_tier;

/// All slash commands
#[derive(Debug)]
pub enum AppCommands {
    Register,
    Unregister,
    Training,
    TrainingBoss,
    TrainingRole,
    TrainingTier,
    Config,
}

/// All commands that should be created when the bot starts
const DEFAULT_COMMANDS: [AppCommands; 7] = [
    AppCommands::Register,
    AppCommands::Unregister,
    AppCommands::Training,
    AppCommands::TrainingBoss,
    AppCommands::TrainingRole,
    AppCommands::TrainingTier,
    AppCommands::Config,
];

impl FromStr for AppCommands {
    type Err = SlashCommandParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            register::CMD_REGISTER => Ok(Self::Register),
            register::CMD_UNREGISTER => Ok(Self::Unregister),
            training::CMD_TRAINING => Ok(Self::Training),
            training_boss::CMD_TRAINING_BOSS => Ok(Self::TrainingBoss),
            training_role::CMD_TRAINING_ROLE => Ok(Self::TrainingRole),
            training_tier::CMD_TRAINING_TIER => Ok(Self::TrainingTier),
            config::CMD_CONFIG => Ok(Self::Config),
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
            Self::TrainingRole => training_role::create(),
            Self::TrainingTier => training_tier::create(),
            Self::Config => config::create(),
        }
    }

    pub fn create_default() -> Vec<CreateApplicationCommand> {
        DEFAULT_COMMANDS
            .iter()
            .map(Self::create)
            .collect::<Vec<_>>()
    }

    async fn handle(&self, ctx: &Context, aci: &ApplicationCommandInteraction) {
        match self {
            Self::Register => register::handle_reg(ctx, aci).await,
            Self::Unregister => register::handle_unreg(ctx, aci).await,
            Self::Training => training::handle(ctx, aci).await,
            Self::TrainingBoss => training_boss::handle(ctx, aci).await,
            Self::TrainingRole => training_role::handle(ctx, aci).await,
            Self::TrainingTier => training_tier::handle(ctx, aci).await,
            Self::Config => config::handle(ctx, aci).await,
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
            .filter_map(|o| o.value.as_ref().map(|v| (o.name.clone(), v.clone())))
            .collect()
    }
}
