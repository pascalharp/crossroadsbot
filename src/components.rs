use crate::db;
use crate::utils::{CHECK_EMOJI, LEFT_ARROW_EMOJI, MEMO_EMOJI, RIGHT_ARROW_EMOJI, X_EMOJI};
use serenity::builder::{CreateActionRow, CreateButton};
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
pub const COMPONENT_ID_CONFIRM: &str = "selection_confirm";
pub const COMPONENT_ID_ABORT: &str = "selection_abort";
pub const COMPONENT_ID_NEXT: &str = "selection_next";
pub const COMPONENT_ID_PREV: &str = "selection_prev";
pub const COMPONENT_ID_SIGNUP_JOIN: &str = "join";
pub const COMPONENT_ID_SIGNUP_EDIT: &str = "edit";
pub const COMPONENT_ID_SIGNUP_LEAVE: &str = "leave";

pub enum ButtonResponse {
    Confirm,
    Abort,
    Next,
    Prev,
    Other(String),
}

#[derive(Debug)]
pub struct ButtonTrainingInteractionParseError {}

impl std::fmt::Display for ButtonTrainingInteractionParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid format")
    }
}

impl std::error::Error for ButtonTrainingInteractionParseError {}

#[derive(Debug)]
pub enum ButtonTrainingInteraction {
    Join(i32),
    Edit(i32),
    Leave(i32),
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
        }
    }
}

impl std::str::FromStr for ButtonTrainingInteraction {
    type Err = ButtonTrainingInteractionParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<_> = s.split('_').collect();
        if parts.len() != 3 {
            return Err(ButtonTrainingInteractionParseError {});
        }
        if !(*parts.get(0).unwrap()).eq("training") {
            return Err(ButtonTrainingInteractionParseError {});
        }
        let training_id = match parts.get(2).unwrap().parse::<i32>() {
            Ok(i) => i,
            Err(_) => return Err(ButtonTrainingInteractionParseError {}),
        };
        match *parts.get(1).unwrap() {
            "join" => Ok(ButtonTrainingInteraction::Join(training_id)),
            "edit" => Ok(ButtonTrainingInteraction::Edit(training_id)),
            "leave" => Ok(ButtonTrainingInteraction::Leave(training_id)),
            _ => Err(ButtonTrainingInteractionParseError {}),
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

pub fn confirm_abort_action_row() -> CreateActionRow {
    let mut ar = CreateActionRow::default();
    ar.add_button(confirm_button());
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
    ar.add_button(ButtonTrainingInteraction::Leave(training_id).button());
    ar
}

pub fn join_action_row(training_id: i32) -> CreateActionRow {
    let mut ar = CreateActionRow::default();
    ar.add_button(ButtonTrainingInteraction::Join(training_id).button());
    ar
}

// Only 5 buttons per row possible
// This pages. One page can have up to 4 rows with 5 roles each
// I only choose 2 rows. looks neater with embeds
pub fn role_action_row(roles: &Vec<db::Role>) -> Vec<Vec<CreateActionRow>> {
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
