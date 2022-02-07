use crate::db;
use crate::utils::{
    CHECK_EMOJI, DOCUMENT_EMOJI, LEFT_ARROW_EMOJI, MEMO_EMOJI, RIGHT_ARROW_EMOJI, X_EMOJI,
};
use serenity::builder::{CreateActionRow, CreateButton, CreateSelectMenu, CreateSelectMenuOption};
use serenity::model::interactions::message_component::ButtonStyle;
use serenity::model::interactions::message_component::MessageComponentInteraction;
use serenity::model::prelude::*;
use std::fmt;

pub const COMPONENT_LABEL_CONFIRM: &str = "Confirm";
pub const COMPONENT_LABEL_ABORT: &str = "Abort";
pub const COMPONENT_LABEL_NEXT: &str = "Next Page";
pub const COMPONENT_LABEL_PREV: &str = "Previous Page";
pub const COMPONENT_LABEL_SIGNUP_JOIN: &str = "SIGN UP";
pub const COMPONENT_LABEL_SIGNUP_EDIT: &str = "EDIT SIGNUP";
pub const COMPONENT_LABEL_SIGNUP_LEAVE: &str = "SIGN OUT";
pub const COMPONENT_LABEL_SIGNUP_COMMENT: &str = "ADD COMMENT";
pub const COMPONENT_LABEL_LIST: &str = "LIST SIGNUPS";
pub const COMPONENT_LABEL_REGISTER: &str = "REGISTER INFORMATION";
pub const COMPONENT_ID_CONFIRM: &str = "selection_confirm";
pub const COMPONENT_ID_ABORT: &str = "selection_abort";
pub const COMPONENT_ID_NEXT: &str = "selection_next";
pub const COMPONENT_ID_PREV: &str = "selection_prev";
pub const COMPONENT_ID_SIGNUP_JOIN: &str = "join";
pub const COMPONENT_ID_SIGNUP_EDIT: &str = "edit";
pub const COMPONENT_ID_SIGNUP_LEAVE: &str = "leave";
pub const COMPONENT_ID_SIGNUP_COMMENT: &str = "comment";

pub enum ButtonResponse {
    Confirm,
    Abort,
    Next,
    Prev,
    Other(String),
}

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

#[derive(Debug)]
pub enum ButtonTrainingInteraction {
    Join(i32),
    Edit(i32),
    Leave(i32),
    Comment(i32),
}

impl ButtonTrainingInteraction {
    pub fn button(&self) -> CreateButton {
        let mut b = CreateButton::default();
        match self {
            ButtonTrainingInteraction::Join(_) => {
                b.style(ButtonStyle::Success);
                b.label(COMPONENT_LABEL_SIGNUP_JOIN);
                b.emoji(ReactionType::from(CHECK_EMOJI));
            }
            ButtonTrainingInteraction::Edit(_) => {
                b.style(ButtonStyle::Primary);
                b.label(COMPONENT_LABEL_SIGNUP_EDIT);
                b.emoji(ReactionType::from(MEMO_EMOJI));
            }
            ButtonTrainingInteraction::Leave(_) => {
                b.style(ButtonStyle::Danger);
                b.label(COMPONENT_LABEL_SIGNUP_LEAVE);
                b.emoji(ReactionType::from(X_EMOJI));
            }
            ButtonTrainingInteraction::Comment(_) => {
                b.style(ButtonStyle::Primary);
                b.label(COMPONENT_LABEL_SIGNUP_COMMENT);
                b.emoji(ReactionType::from(MEMO_EMOJI));
            }
        };
        b.custom_id(self.to_string());
        b
    }
}

impl std::fmt::Display for ButtonTrainingInteraction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ButtonTrainingInteraction::Join(id) => write!(f, "training_join_{}", id),
            ButtonTrainingInteraction::Edit(id) => write!(f, "training_edit_{}", id),
            ButtonTrainingInteraction::Leave(id) => write!(f, "training_leave_{}", id),
            ButtonTrainingInteraction::Comment(id) => write!(f, "training_comment_{}", id),
        }
    }
}

impl std::str::FromStr for ButtonTrainingInteraction {
    type Err = GlobalInteractionParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<_> = s.split('_').collect();
        if parts.len() != 3 {
            return Err(GlobalInteractionParseError {});
        }
        if !(*parts.get(0).unwrap()).eq("training") {
            return Err(GlobalInteractionParseError {});
        }
        let training_id = match parts.get(2).unwrap().parse::<i32>() {
            Ok(i) => i,
            Err(_) => return Err(GlobalInteractionParseError {}),
        };
        match *parts.get(1).unwrap() {
            "join" => Ok(ButtonTrainingInteraction::Join(training_id)),
            "edit" => Ok(ButtonTrainingInteraction::Edit(training_id)),
            "leave" => Ok(ButtonTrainingInteraction::Leave(training_id)),
            "comment" => Ok(ButtonTrainingInteraction::Comment(training_id)),
            _ => Err(GlobalInteractionParseError {}),
        }
    }
}

impl fmt::Display for ButtonResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ButtonResponse::Confirm => write!(f, "{}", COMPONENT_LABEL_CONFIRM),
            ButtonResponse::Abort => write!(f, "{}", COMPONENT_LABEL_ABORT),
            ButtonResponse::Next => write!(f, "{}", COMPONENT_LABEL_NEXT),
            ButtonResponse::Prev => write!(f, "{}", COMPONENT_LABEL_PREV),
            ButtonResponse::Other(s) => write!(f, "{}", s),
        }
    }
}

#[derive(Debug, Clone)]
pub enum OverviewMessageInteraction {
    List,
    Register,
    TrainingSelect,
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
        }
    }
}

pub enum SelectionRolePriority {
    VeryHigh,
    High,
    Default,
    Low,
    VeryLow,
}

impl std::fmt::Display for SelectionRolePriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SelectionRolePriority::VeryHigh => write!(f, "selection_role_priority_very_high"),
            SelectionRolePriority::High => write!(f, "selection_role_priority_high"),
            SelectionRolePriority::Default => write!(f, "selection_role_priority_default"),
            SelectionRolePriority::Low => write!(f, "selection_role_priority_low"),
            SelectionRolePriority::VeryLow => write!(f, "selection_role_priority_very_low"),
        }
    }
}

impl std::str::FromStr for SelectionRolePriority {
    type Err = GlobalInteractionParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let split = s
            .split_once("selection_role_priority_")
            .ok_or(GlobalInteractionParseError {})?;
        match split.1 {
            "very_high" => Ok(SelectionRolePriority::VeryHigh),
            "high" => Ok(SelectionRolePriority::High),
            "default" => Ok(SelectionRolePriority::Default),
            "low" => Ok(SelectionRolePriority::Low),
            "very_low" => Ok(SelectionRolePriority::VeryLow),
            _ => Err(GlobalInteractionParseError {}),
        }
    }
}

impl SelectionRolePriority {
    pub fn to_i16(&self) -> i16 {
        match self {
            SelectionRolePriority::VeryHigh => 4,
            SelectionRolePriority::High => 3,
            SelectionRolePriority::Default => 2,
            SelectionRolePriority::Low => 1,
            SelectionRolePriority::VeryLow => 0,
        }
    }

    pub fn select_menu_option(&self) -> CreateSelectMenuOption {
        let mut opt = CreateSelectMenuOption::default();
        opt.value(self);
        let d = match self {
            SelectionRolePriority::VeryHigh => "Very High",
            SelectionRolePriority::High => "High",
            SelectionRolePriority::Default => "Default",
            SelectionRolePriority::Low => "Low",
            SelectionRolePriority::VeryLow => "Very Low",
        };
        opt.label(d);
        opt
    }

    pub fn select_menu_id_str() -> &'static str {
        "selection_role_priority"
    }

    pub fn select_menu() -> CreateSelectMenu {
        let mut menu = CreateSelectMenu::default();
        menu.custom_id(SelectionRolePriority::select_menu_id_str());
        menu.options(|o| {
            o.add_option(SelectionRolePriority::VeryHigh.select_menu_option());
            o.add_option(SelectionRolePriority::High.select_menu_option());
            o.add_option(SelectionRolePriority::Default.select_menu_option());
            o.add_option(SelectionRolePriority::Low.select_menu_option());
            o.add_option(SelectionRolePriority::VeryLow.select_menu_option());
            o
        });
        menu.placeholder("Select priority");
        menu.min_values(0);
        menu.max_values(1);
        menu
    }
}

pub fn resolve_button_response(response: &MessageComponentInteraction) -> ButtonResponse {
    match response.data.custom_id.as_str() {
        COMPONENT_ID_CONFIRM => ButtonResponse::Confirm,
        COMPONENT_ID_ABORT => ButtonResponse::Abort,
        COMPONENT_ID_NEXT => ButtonResponse::Next,
        COMPONENT_ID_PREV => ButtonResponse::Prev,
        s => ButtonResponse::Other(String::from(s)),
    }
}

pub fn confirm_button() -> CreateButton {
    let mut b = CreateButton::default();
    b.style(ButtonStyle::Success);
    b.label(COMPONENT_LABEL_CONFIRM);
    b.custom_id(COMPONENT_ID_CONFIRM);
    b.emoji(ReactionType::from(CHECK_EMOJI));
    b
}

pub fn abort_button() -> CreateButton {
    let mut b = CreateButton::default();
    b.style(ButtonStyle::Danger);
    b.label(COMPONENT_LABEL_ABORT);
    b.custom_id(COMPONENT_ID_ABORT);
    b.emoji(ReactionType::from(X_EMOJI));
    b
}

pub fn next_button() -> CreateButton {
    let mut b = CreateButton::default();
    b.style(ButtonStyle::Primary);
    b.label(COMPONENT_LABEL_NEXT);
    b.custom_id(COMPONENT_ID_NEXT);
    b.emoji(ReactionType::from(RIGHT_ARROW_EMOJI));
    b
}

pub fn prev_button() -> CreateButton {
    let mut b = CreateButton::default();
    b.style(ButtonStyle::Primary);
    b.label(COMPONENT_LABEL_PREV);
    b.custom_id(COMPONENT_ID_PREV);
    b.emoji(ReactionType::from(LEFT_ARROW_EMOJI));
    b
}

pub fn role_button(role: &db::Role) -> CreateButton {
    let mut b = CreateButton::default();
    b.style(ButtonStyle::Primary);
    b.label(role.title.clone());
    b.custom_id(role.repr.clone());
    b.emoji(ReactionType::from(EmojiId::from(role.emoji as u64)));
    b
}

pub fn confirm_abort_action_row(confirm_disabled: bool) -> CreateActionRow {
    let mut ar = CreateActionRow::default();
    ar.add_button({
        let mut b = confirm_button();
        b.disabled(confirm_disabled);
        b
    });
    ar.add_button(abort_button());
    ar
}

pub fn signup_action_row(training_id: i32) -> CreateActionRow {
    let mut ar = CreateActionRow::default();
    ar.add_button(ButtonTrainingInteraction::Join(training_id).button());
    ar.add_button(ButtonTrainingInteraction::Edit(training_id).button());
    ar.add_button(ButtonTrainingInteraction::Leave(training_id).button());
    ar
}

pub fn edit_leave_action_row(training_id: i32) -> CreateActionRow {
    let mut ar = CreateActionRow::default();
    ar.add_button(ButtonTrainingInteraction::Edit(training_id).button());
    ar.add_button(ButtonTrainingInteraction::Comment(training_id).button());
    ar.add_button(ButtonTrainingInteraction::Leave(training_id).button());
    ar
}

pub fn join_action_row(training_id: i32) -> CreateActionRow {
    let mut ar = CreateActionRow::default();
    ar.add_button(ButtonTrainingInteraction::Join(training_id).button());
    ar
}

pub fn overview_register_list_action_row() -> CreateActionRow {
    let mut ar = CreateActionRow::default();

    let mut b = CreateButton::default();
    b.style(ButtonStyle::Primary);
    b.custom_id(OverviewMessageInteraction::List);
    b.label(COMPONENT_LABEL_LIST);
    b.emoji(ReactionType::from(DOCUMENT_EMOJI));
    ar.add_button(b);

    let mut b = CreateButton::default();
    b.style(ButtonStyle::Primary);
    b.custom_id(OverviewMessageInteraction::Register);
    b.label(COMPONENT_LABEL_REGISTER);
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

pub fn role_priority_select_action_row() -> CreateActionRow {
    let mut ar = CreateActionRow::default();
    ar.add_select_menu(SelectionRolePriority::select_menu());
    ar
}

// Only 5 buttons per row possible
// This pages. One page can have up to 4 rows with 5 roles each
// I only choose 2 rows. looks neater with embeds
pub fn role_action_row(roles: &[db::Role]) -> Vec<Vec<CreateActionRow>> {
    // split into 5 roles for each action row
    let role_chunks = roles.chunks(5).collect::<Vec<_>>();
    // split into 2 action roles per page
    let row_chunks = role_chunks.chunks(2);

    // Create required amount of pages
    let mut pages: Vec<Vec<CreateActionRow>> = Vec::with_capacity(row_chunks.len());

    for rows in row_chunks {
        // add new page
        pages.push(Vec::with_capacity(4));
        let new_page = pages.last_mut().unwrap();

        for r in rows {
            // Create Action row with role buttons
            let mut ar = CreateActionRow::default();
            for role in r.iter() {
                ar.add_button(role_button(role));
            }
            new_page.push(ar);
        }
    }
    pages
}
