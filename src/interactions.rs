use std::sync::Arc;

use crate::logging::*;

use serenity::{model::{interactions::message_component::{MessageComponentInteraction, ButtonStyle}, channel::ReactionType}, prelude::*, builder::{CreateActionRow, CreateButton}};

mod manage_sign_up;

const COMPONENT_MANAGE_SIGNUPS: &str = "SIGN UP / SIGN OUT / EDIT SIGN-UP";
const MEMO_EMOJI: char = '📝';

#[derive(Debug)]
pub struct GlobalInteractionParseError {}

impl std::fmt::Display for GlobalInteractionParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid format")
    }
}

impl std::error::Error for GlobalInteractionParseError {}

/// Interactions that are not received over a collector
#[derive(Debug)]
#[non_exhaustive]
pub enum GlobalInteraction {
    Overview(OverviewMessageInteraction),
}

impl std::str::FromStr for GlobalInteraction {
    type Err = GlobalInteractionParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(bgi) = s.parse::<OverviewMessageInteraction>() {
            return Ok(Self::Overview(bgi));
        }
        Err(GlobalInteractionParseError {})
    }
}

impl std::fmt::Display for GlobalInteraction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Overview(bgi) => write!(f, "{}", bgi),
        }
    }
}

#[derive(Debug, Clone)]
pub enum OverviewMessageInteraction {
    ManageSignups,
}

impl std::str::FromStr for OverviewMessageInteraction {
    type Err = GlobalInteractionParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<_> = s.split('_').collect();
        if parts.len() != 2 {
            return Err(GlobalInteractionParseError {});
        }
        if !(*parts.get(0).unwrap()).eq("overview") {
            return Err(GlobalInteractionParseError {});
        }
        match *parts.get(1).unwrap() {
            "managesignups" => Ok(Self::ManageSignups),
            _ => Err(GlobalInteractionParseError {}),
        }
    }
}

impl std::fmt::Display for OverviewMessageInteraction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ManageSignups => write!(f, "overview_managesignups"),
        }
    }
}

pub fn overview_action_row() -> CreateActionRow {
    let mut ar = CreateActionRow::default();

    let mut b = CreateButton::default();
    b.style(ButtonStyle::Primary);
    b.custom_id(OverviewMessageInteraction::ManageSignups);
    b.label(COMPONENT_MANAGE_SIGNUPS);
    b.emoji(ReactionType::from(MEMO_EMOJI));
    ar.add_button(b);

    ar
}

async fn button_general_interaction(
    ctx: &Context,
    mci: Arc<MessageComponentInteraction>,
    ovi: &OverviewMessageInteraction,
) {
    log_discord(ctx, mci.clone().as_ref(), |trace| async move {
        match ovi {
            OverviewMessageInteraction::ManageSignups => {
                manage_sign_up::interaction(ctx, mci, trace).await
            }
        }
    })
    .await
}

pub async fn button_interaction(ctx: &Context, mci: MessageComponentInteraction) {
    // Putting it in Arc to unify methods with collectors
    let mci = Arc::new(mci);
    // Check what interaction to handle
    if let Ok(bi) = mci.data.custom_id.parse::<GlobalInteraction>() {
        match &bi {
            GlobalInteraction::Overview(bgi) => button_general_interaction(ctx, mci, bgi).await,
        }
    };
}
