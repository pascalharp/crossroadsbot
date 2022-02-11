use std::{
    fmt::{Display, Error as FmtError},
    str::FromStr,
    time::Duration,
};

use anyhow::{anyhow, bail, Context as ErrContext, Error, Result};
use chrono::NaiveDate;
use itertools::Itertools;
use serenity::{
    builder::{CreateButton, CreateEmbed},
    client::Context,
    model::{
        channel::{Message, ReactionType},
        id::{EmojiId, RoleId},
        interactions::{
            message_component::{ButtonStyle, MessageComponentInteraction},
            InteractionApplicationCommandCallbackDataFlags, InteractionResponseType,
        },
        misc::Mention,
    },
};
use serenity_tools::{
    builder::CreateEmbedExt,
    collectors::{PagedSelectorConfig, UpdatAbleMessage},
    interactions::MessageComponentInteractionExt,
};

use crate::{
    data, db,
    embeds::{self, CrossroadsEmbeds},
    logging::{self, LogTrace, ReplyHelper}, signup_board::title_sort_value,
};

enum Buttons {
    Join,
    Leave,
    EditRoles,
    EditPreferences,
    AddComment,
    Dismiss,
}

pub(crate) async fn interaction(
    ctx: &Context,
    mut mci: &MessageComponentInteraction,
    trace: LogTrace,
) -> Result<()> {
    trace.step("Preparing interaction");
    // Not super elegent but gives as a message to work with right away
    mci.create_quick_info(ctx, "Loading...", true).await?;
    let msg = mci.get_interaction_response(ctx).await?;

    let guild_id = {
        ctx.data
            .read()
            .await
            .get::<data::ConfigValuesData>()
            .unwrap()
            .clone()
            .main_guild_id
    };

    loop {
        trace.step("Refreshing information");
        let db_user = match db::User::by_discord_id(ctx, mci.user.id).await {
            Ok(u) => u,
            Err(diesel::NotFound) => {
                return Err(diesel::NotFound)
                    .context("Not yet registered. Please register first")
                    .map_err_reply(|_| mci.edit_original_interaction_response(ctx, |r| {
                        r.add_embed(embeds::register_instructions_embed())
                    }))
                    .await
            }
            Err(e) => bail!(e),
        };
        let trainings_all = db::Training::all_active(ctx).await?;
        let mut trainings: Vec<db::Training> = Vec::with_capacity(trainings_all.len());

        for training in trainings_all {

            // filter for trainings user can join
            let tier = training.get_tier(ctx).await.transpose()?;
            let tier_roles = match &tier {
                Some(t) => Some(t.get_discord_roles(ctx).await?),
                None => None,
            };
            // check if user can join the training
            let can_join = if let Some(tier_roles) = tier_roles {
                let mut cj = false;
                for tr in tier_roles {
                    if let Ok(b) = mci
                        .user
                        .has_role(ctx, guild_id, RoleId::from(tr.discord_role_id as u64))
                        .await
                    {
                        if b {
                            cj = true;
                            break;
                        }
                    }
                }
                cj
            } else {
                true
            };

            if !can_join { continue }

            // Add training to selection options
            trainings.push(training);
        };

        if trainings.is_empty() {
            trace.step("No training's available");
            mci.edit_quick_info(ctx, "There currently are no training options available for you =(").await?;
            return Ok(());
        }

        // Sort trainings -> splitted trainings will also be sorted
        trainings.sort_by(|a, b| title_sort_value(b).cmp(&title_sort_value(a)));
        trainings.sort_by_key(|t| t.date);

        let signups = db_user.open_signups(ctx).await?;
        let mut joined: Vec<&db::Training> = Vec::with_capacity(trainings.len());
        let mut not_joined: Vec<&db::Training> = Vec::with_capacity(trainings.len());

        for t in &trainings {
            if signups.iter().map(|s| s.training_id).contains(&t.id) {
                joined.push(t);
            } else {
                // only show trainings still open for not yet joined
                if t.state == db::TrainingState::Open {
                    not_joined.push(t);
                }
            }
        }

        // Build embed
        let mut emb = CreateEmbed::xdefault();
        emb.title("Manage your Sign-Ups");
        emb.description("**Feel free to dismiss this message once your are done**");
        emb.footer(

        let mut joined_str = String::new();
        for (d, v) in joined
            .iter()
            .group_by(|t| t.date.date())
            .into_iter()
        {
            joined_str.push_str(&format!("\n__{}__\n```\n", d.format("__**%A**, %v__")));
            for t in v {
                    joined_str.push_str(&format!("> {}\n", &t.title));
            }
            joined_str.push_str("```");
        }
        if !joined_str.is_empty() {
            emb.field("**✅ Already signed up for**", joined_str, false);
        };

        // now filter out non open training's to not offer them in the select menu
        joined = joined.into_iter().filter(|t| t.state == db::TrainingState::Open).collect();

        let mut not_joined_str = String::new();
        for (d, v) in not_joined
            .into_iter()
            .group_by(|t| t.date.date())
            .into_iter()
        {
            not_joined_str.push_str(&format!("\n__{}__\n```\n", d.format("__**%A**, %v__")));
            for t in v {
                    not_joined_str.push_str(&format!("> {}\n", &t.title));
            }
            not_joined_str.push_str("```");
        }
        if !not_joined_str.is_empty() {
            emb.field("**❌ Not yet signed up for**", not_joined_str, false);
        };

        mci.edit_original_interaction_response(ctx, |r| {
            r.add_embed(emb)
        })
        .await?;

        //tokio::select! {
        //}
        break;
    }
    Ok(())
}
