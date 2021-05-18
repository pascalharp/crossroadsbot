use crate::{db, utils::*};
use chrono::{DateTime, Utc};
use chrono_tz::Europe::{London, Moscow, Paris};
use serenity::{
    builder::CreateEmbed,
    model::{guild::Emoji, misc::Mention},
};
use std::collections::{HashMap, HashSet};

// Embed helpers
pub fn select_roles_embed(
    re_map: &HashMap<&db::Role, &Emoji>, // map with all relevant emojis
    sel: &HashSet<&db::Role>,            // already selected roles
    initial: bool,
) -> CreateEmbed {
    let mut e = CreateEmbed::default();

    e.description("Select roles");
    e.fields(re_map.iter().map(|(r, e)| {
        let des = String::from(format!(
            "{} | {}",
            if sel.contains(r) {
                CHECK_EMOJI
            } else {
                CROSS_EMOJI
            },
            r.repr
        ));
        let cont = String::from(format!("{} | {}", Mention::from(*e), r.title));
        (des, cont, true)
    }));
    e.footer(|f| {
        if initial {
            f.text("Loading emojis. Please wait...")
        } else {
            f.text(format!("React with the corresponding emoji to select/unselect a role. Use {} to confirm. Use {} to abort", CHECK_EMOJI, CROSS_EMOJI))
        }
    });
    e
}

const TRAINING_TIME_FMT: &str = "%a, %B %Y at %H:%M %Z";
// Does not display roles
pub fn training_base_embed(training: &db::Training) -> CreateEmbed {
    let mut e = CreateEmbed::default();
    let utc = DateTime::<Utc>::from_utc(training.date, Utc);
    e.description(format!(
        "{} {}",
        match training.state {
            db::TrainingState::Created => CONSTRUCTION_SITE_EMOJI,
            db::TrainingState::Open => GREEN_CIRCLE_EMOJI,
            db::TrainingState::Closed => RED_CIRCLE_EMOJI,
            db::TrainingState::Started => RUNNING_EMOJI,
            db::TrainingState::Finished => CROSS_EMOJI,
        },
        training.title
    ));
    e.field(
        "**Date**",
        format!(
            "{}\n{}\n{}\n{}",
            utc.format(TRAINING_TIME_FMT),
            utc.with_timezone(&London).format(TRAINING_TIME_FMT),
            utc.with_timezone(&Paris).format(TRAINING_TIME_FMT),
            utc.with_timezone(&Moscow).format(TRAINING_TIME_FMT),
        ),
        false,
    );
    e.field("**State**", &training.state, true);
    e.field("**Training Id**", &training.id, true);
    e
}
