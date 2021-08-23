use std::fmt;
use serenity::model::prelude::*;
use serenity::model::interactions::message_component::ButtonStyle;
use serenity::model::interactions::message_component::MessageComponentInteraction;
use serenity::builder::{CreateActionRow, CreateButton};
use crate::utils::{CHECK_EMOJI, X_EMOJI};

pub const COMPONENT_LABEL_CONFIRM: &str = "Confirm";
pub const COMPONENT_LABEL_ABORT: &str = "Abort";
pub const COMPONENT_ID_CONFIRM: &str = "confirm";
pub const COMPONENT_ID_ABORT: &str = "abort";

pub enum ButtonResponse {
    Confirm,
    Abort,
    Other(String)
}

impl fmt::Display for ButtonResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ButtonResponse::Confirm => write!(f, "{}", COMPONENT_LABEL_CONFIRM),
            ButtonResponse::Abort => write!(f, "{}", COMPONENT_LABEL_ABORT),
            ButtonResponse::Other(s) => write!(f, "{}", s),
        }
    }
}

pub fn resolve_button_response(response :&MessageComponentInteraction) -> ButtonResponse {
    match response.data.custom_id.as_str() {
        COMPONENT_ID_CONFIRM => ButtonResponse::Confirm,
        COMPONENT_ID_ABORT => ButtonResponse::Abort,
        s => ButtonResponse::Other(String::from(s))
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

pub fn confirm_abort_action_row() -> CreateActionRow {
    let mut ar = CreateActionRow::default();
    ar.add_button(confirm_button());
    ar.add_button(abort_button());
    ar
}
