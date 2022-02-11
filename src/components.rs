use crate::db;
use serenity::builder::{CreateActionRow, CreateButton};
use serenity::model::interactions::message_component::ButtonStyle;
use serenity::model::prelude::*;

const COMPONENT_LABEL_LIST: &str = "LIST SIGNUPS";
const COMPONENT_LABEL_REGISTER: &str = "REGISTER INFORMATION";
const COMPONENT_MANAGE_SIGNUPS: &str = "SIGN UP / SIGN OUT / EDIT SIGN-UP";
const DOCUMENT_EMOJI: char = '🧾';
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
    List,
    Register,
    TrainingSelect,
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
            "list" => Ok(Self::List),
            "register" => Ok(Self::Register),
            "trainingselect" => Ok(Self::TrainingSelect),
            "managesignups" => Ok(Self::ManageSignups),
            _ => Err(GlobalInteractionParseError {}),
        }
    }
}

impl std::fmt::Display for OverviewMessageInteraction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::List => write!(f, "overview_list"),
            Self::Register => write!(f, "overview_register"),
            Self::TrainingSelect => write!(f, "overview_trainingselect"),
            Self::ManageSignups => write!(f, "overview_managesignups"),
        }
    }
}

pub fn overview_register_list_action_row() -> CreateActionRow {
    let mut ar = CreateActionRow::default();

    let mut b = CreateButton::default();
    b.style(ButtonStyle::Secondary);
    b.custom_id(OverviewMessageInteraction::List);
    b.label(COMPONENT_LABEL_LIST);
    b.emoji(ReactionType::from(DOCUMENT_EMOJI));
    ar.add_button(b);

    let mut b = CreateButton::default();
    b.style(ButtonStyle::Secondary);
    b.custom_id(OverviewMessageInteraction::Register);
    b.label(COMPONENT_LABEL_REGISTER);
    b.emoji(ReactionType::from(MEMO_EMOJI));
    ar.add_button(b);

    let mut b = CreateButton::default();
    b.style(ButtonStyle::Primary);
    b.custom_id(OverviewMessageInteraction::ManageSignups);
    b.label(COMPONENT_MANAGE_SIGNUPS);
    b.emoji(ReactionType::from(MEMO_EMOJI));
    ar.add_button(b);

    ar
}

pub fn overview_training_select_action_row(trainings: &[&db::Training]) -> CreateActionRow {
    let mut ar = CreateActionRow::default();
    ar.create_select_menu(|sm| {
        sm.custom_id(OverviewMessageInteraction::TrainingSelect);
        sm.placeholder("Select a training");
        sm.options(|o| {
            o.create_option(|smo| {
                smo.label("Clear selection");
                smo.value("clear")
            });
            for t in trainings {
                if t.state != db::TrainingState::Open {
                    continue;
                }
                o.create_option(|smo| {
                    let label = format!("{} | {}", t.date.format("%a, %m, %Y"), t.title);
                    smo.label(label);
                    smo.value(t.id);
                    smo
                });
            }
            o
        })
    });
    ar
}
