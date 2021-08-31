use crate::{data::GLOB_COMMAND_PREFIX, db, utils::*};
use chrono::{DateTime, Utc};
use chrono_tz::Europe::{London, Paris};
use serenity::{
    builder::CreateEmbed,
    model::{id::EmojiId, id::RoleId, misc::Mention},
};
use std::{collections::HashSet};

// Embed helpers
pub fn select_roles_embed(
    roles: &Vec<db::Role>, // all roles
    sel: &HashSet<String>, // selected roles
) -> CreateEmbed {
    let mut e = CreateEmbed::default();
    let field_str = roles
        .iter()
        .map(|r| {
            format!(
                "`{}` | {} | {}",
                if sel.contains(&r.repr) {
                    CHECK_EMOJI
                } else {
                    RED_SQUARE_EMOJI
                },
                Mention::from(EmojiId::from(r.emoji as u64)),
                r.title
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    e.field("Select roles", field_str, false);
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
    t: &Option<(db::Tier, Vec<db::TierMapping>)>,
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

pub fn embed_add_roles(e: &mut CreateEmbed, r: &Vec<db::Role>, inline: bool) {
    let repr_width = r
        .iter()
        .map(|r| r.repr.len())
        .fold(usize::MIN, |max, next| std::cmp::max(max, next));
    let title_width = r
        .iter()
        .map(|r| r.title.len())
        .fold(usize::MIN, |max, next| std::cmp::max(max, next));
    let roles_text = r
        .iter()
        .map(|r| {
            format!(
                "{} `| {:^rwidth$} | {:^twidth$} |`",
                Mention::from(EmojiId::from(r.emoji as u64)),
                &r.repr,
                &r.title,
                rwidth = repr_width,
                twidth = title_width
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    e.field("Roles", roles_text, inline);
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
