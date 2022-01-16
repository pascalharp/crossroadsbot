use std::{fmt::Display, str::FromStr};

use serenity::{
    builder::{CreateApplicationCommand, CreateApplicationCommandPermissions},
    client::Context,
    model::interactions::application_command::{
        ApplicationCommand, ApplicationCommandInteraction, ApplicationCommandPermissionType,
    },
};

use tracing::error;

use crate::data::ConfigValues;

/// The trait that every Slash Command should have
/// to ease up configuration and parsing
pub trait SlashCommand: FromStr + Display {
    fn create() -> CreateApplicationCommand;
    fn permission(&self, conf: &ConfigValues) -> (u64, ApplicationCommandPermissionType);
}

#[derive(Debug)]
pub struct SlashCommandParseError(String);

impl std::fmt::Display for SlashCommandParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Unknown slash command: {}", self.0)
    }
}

impl std::error::Error for SlashCommandParseError {}

mod training;
mod training_boss;

/// All slash commands
#[derive(Debug)]
pub enum AppCommands {
    Training,
    TrainingBoss,
}

/// All commands that should be created when the bot starts
const DEFAULT_COMMANDS: [AppCommands; 2] = [AppCommands::Training, AppCommands::TrainingBoss];

impl FromStr for AppCommands {
    type Err = SlashCommandParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            training::CMD_TRAINING => Ok(Self::Training),
            training_boss::CMD_TRAINING_BOSS => Ok(Self::TrainingBoss),
            _ => Err(SlashCommandParseError(s.to_owned())),
        }
    }
}

impl AppCommands {
    pub fn create(&self) -> CreateApplicationCommand {
        match self {
            Self::Training => training::create(),
            Self::TrainingBoss => training_boss::create(),
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
            Self::Training | Self::TrainingBoss => perms.create_permissions(|p| {
                p.permission(true)
                    .kind(ApplicationCommandPermissionType::Role)
                    .id(conf.squadmaker_role_id.0)
            }),
        };

        perms
    }

    async fn handle(&self, ctx: &Context, aci: &ApplicationCommandInteraction) {
        match self {
            Self::Training => training::handle(ctx, aci).await,
            Self::TrainingBoss => training_boss::handle(ctx, aci).await,
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

// helper functions for quick replies.
// Edits nuke the previous content. Always ephemeral
pub mod helpers {
    use std::collections::HashMap;

    use serde_json::Value;
    use serenity::{
        client::Context,
        model::{
            id::MessageId,
            interactions::{
                application_command::{
                    ApplicationCommandInteraction, ApplicationCommandInteractionDataOption,
                },
                InteractionApplicationCommandCallbackDataFlags, InteractionResponseType,
            },
        },
    };

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

    /// Creates an Interaction response with: ChannelMessageWithSource
    pub async fn quick_ch_msg_with_src<C: ToString>(
        ctx: &Context,
        aci: &ApplicationCommandInteraction,
        cont: C,
    ) -> anyhow::Result<()> {
        aci.create_interaction_response(ctx, |r| {
            r.kind(InteractionResponseType::ChannelMessageWithSource);
            r.interaction_response_data(|d| {
                d.content(cont.to_string());
                d.flags(InteractionApplicationCommandCallbackDataFlags::EPHEMERAL)
            })
        })
        .await?;
        Ok(())
    }

    /// Creates an Interaction response with: UpdateMessage
    pub async fn quick_update_msg<C: ToString>(
        ctx: &Context,
        aci: &ApplicationCommandInteraction,
        cont: C,
    ) -> anyhow::Result<()> {
        aci.create_interaction_response(ctx, |r| {
            r.kind(InteractionResponseType::UpdateMessage);
            r.interaction_response_data(|d| {
                d.content(cont.to_string());
                d.embeds(Vec::new());
                d.components(|c| c);
                d.flags(InteractionApplicationCommandCallbackDataFlags::EPHEMERAL)
            })
        })
        .await?;
        Ok(())
    }

    /// Edits the original interaction response
    pub async fn quick_edit_orig_rsp<C: ToString>(
        ctx: &Context,
        aci: &ApplicationCommandInteraction,
        cont: C,
    ) -> anyhow::Result<()> {
        aci.edit_original_interaction_response(ctx, |d| {
            d.content("");
            d.set_embeds(Vec::new());
            d.create_embed(|e| e.field("Info", cont, false));
            d.components(|c| c)
        })
        .await?;
        Ok(())
    }

    /// Creates a follwup up message
    pub async fn quick_create_flup_msg<C: ToString>(
        ctx: &Context,
        aci: &ApplicationCommandInteraction,
        cont: C,
    ) -> anyhow::Result<()> {
        aci.create_followup_message(ctx, |d| {
            d.content(cont.to_string());
            d.flags(InteractionApplicationCommandCallbackDataFlags::EPHEMERAL)
        })
        .await?;
        Ok(())
    }

    /// Edits the follwup up message
    pub async fn quick_edit_flup_msg<M: Into<MessageId>, C: ToString>(
        ctx: &Context,
        aci: &ApplicationCommandInteraction,
        msg_id: M,
        cont: C,
    ) -> anyhow::Result<()> {
        aci.edit_followup_message(ctx, msg_id, |d| {
            d.content(cont.to_string());
            d.embeds(Vec::new());
            d.components(|c| c)
        })
        .await?;
        Ok(())
    }
}
