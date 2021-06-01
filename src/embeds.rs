use crate::{db, utils::*, data::GLOB_COMMAND_PREFIX};
use chrono::{DateTime, Utc};
use chrono_tz::Europe::{London, Paris};
use serenity::{
    builder::CreateEmbed,
    model::{guild::Emoji, id::RoleId, misc::Mention},
};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

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

const TRAINING_TIME_FMT: &str = "%a, %v at %H:%M %Z";
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
            "{}\n{}\n{}",
            utc.format(TRAINING_TIME_FMT),
            utc.with_timezone(&London).format(TRAINING_TIME_FMT),
            utc.with_timezone(&Paris).format(TRAINING_TIME_FMT),
        ),
        false,
    );
    e.field("**State**", &training.state, true);
    e.field("**Training Id**", &training.id, true);
    e
}

// adds the required tiers to the embed field. Should only be used in
// guild channels since pinging wont work in DM's
pub fn training_embed_add_tier(
    e: &mut CreateEmbed,
    t: &Option<(Arc<db::Tier>, Arc<Vec<db::TierMapping>>)>,
    inline: bool,
) {
    match t {
        None => {
            e.field("Tier: none", "Open for everyone", inline);
        }
        Some((tier, roles)) => {
            e.field(
                format!("Tier: {}", tier.name),
                roles
                    .iter()
                    .map(|r| Mention::from(RoleId::from(r.discord_role_id as u64)).to_string())
                    .collect::<Vec<_>>()
                    .join("\n"),
                inline,
            );
        }
    }
}

pub fn training_embed_add_board_footer(e: &mut CreateEmbed, ts: &db::TrainingState) {
    match ts {
        db::TrainingState::Open => {
            e.footer(|f| {
                f.text(format!(
                    "{}\n{}\n{}",
                    format!("{} to signup", CHECK_EMOJI),
                    format!("{} to edit your signup", MEMO_EMOJI),
                    format!("{} to remove your signup", CROSS_EMOJI)
                ))
            });
        }
        _ => {
            e.footer(|f| f.text("Not open for signup"));
        }
    }
}

pub fn not_registered_embed() -> CreateEmbed {
    let mut e = CreateEmbed::default();
    e.description("Not yet registerd");
    e.field(
        "User not found. Use the register command first",
        format!(
            "For more information type: __{}help register__",
            GLOB_COMMAND_PREFIX
        ),
        false,
    );
    e
}
