use crate::embeds::CrossroadsEmbeds;
use crate::{data, data::SignupBoardData, db, interactions, logging::LogTrace};
use anyhow::Result;
use chrono::NaiveDate;
use itertools::Itertools;
use serenity::builder::CreateEmbed;
use serenity::{model::prelude::*, prelude::*};
use serenity_tools::builder::CreateEmbedExt;
use std::{mem, sync::Arc};

const OVERVIEW_CHANNEL_ID: &str = "overview_channel_id";
const OVERVIEW_MESSAGE_ID: &str = "overview_message_id";
const CROSS_EMOJI: char = '❌';
const RUNNING_EMOJI: char = '🏃';
const GREEN_CIRCLE_EMOJI: char = '🟢';
const CONSTRUCTION_SITE_EMOJI: char = '🚧';
const LOCK_EMOJI: char = '🔒';

// Hold on to often used values
pub struct SignupBoard {
    pub overview_channel_id: Option<ChannelId>,
    pub overview_message_id: Option<MessageId>,
}

#[derive(Debug)]
pub enum SignupBoardError {
    OverviewMessageNotSet,
    OverviewChannelNotSet,
    ChannelNotFound(ChannelId),
}

impl std::fmt::Display for SignupBoardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ChannelNotFound(id) => {
                write!(f, "Channel with id: {} not found on Signupboard", id)
            }
            Self::OverviewMessageNotSet => write!(f, "Overview message not set"),
            Self::OverviewChannelNotSet => write!(f, "Overview channel not set"),
        }
    }
}

impl std::error::Error for SignupBoardError {}

// loads the main guild id, that is always set
async fn load_guild_id(ctx: &Context) -> Result<GuildId> {
    // Load guild id provided on startup
    let guild_id = ctx
        .data
        .read()
        .await
        .get::<data::ConfigValuesData>()
        .unwrap()
        .main_guild_id;

    Ok(guild_id)
}

pub(crate) fn title_sort_value(t: &db::Training) -> u64 {
    if t.title.contains("Beginner") {
        return 10;
    }
    if t.title.contains("Intermediate") {
        return 8;
    }
    if t.title.contains("Practice") {
        return 6;
    }
    0
}

impl SignupBoard {
    // get a lock on the SignupBoardConfig
    pub async fn get(ctx: &Context) -> Arc<RwLock<SignupBoard>> {
        ctx.data
            .read()
            .await
            .get::<SignupBoardData>()
            .unwrap()
            .clone()
    }

    pub async fn load_from_db(&mut self, ctx: &Context) -> Result<()> {
        let new_board = SignupBoard {
            overview_channel_id: match db::Config::load(ctx, OVERVIEW_CHANNEL_ID.to_string()).await
            {
                Ok(conf) => Some(conf.value.parse::<ChannelId>()?),
                Err(diesel::NotFound) => None,
                Err(e) => return Err(e.into()),
            },
            overview_message_id: match db::Config::load(ctx, OVERVIEW_MESSAGE_ID.to_string()).await
            {
                Ok(conf) => Some(conf.value.parse::<u64>()?.into()),
                Err(diesel::NotFound) => None,
                Err(e) => return Err(e.into()),
            },
        };
        // overwrite at once and not value by value
        let _ = mem::replace(self, new_board);
        Ok(())
    }

    pub async fn save_to_db(&self, ctx: &Context) -> Result<()> {
        if let Some(oci) = self.overview_channel_id {
            db::Config {
                name: OVERVIEW_CHANNEL_ID.to_string(),
                value: oci.to_string(),
            }
            .save(ctx)
            .await?;
        }

        if let Some(omi) = self.overview_message_id {
            db::Config {
                name: OVERVIEW_MESSAGE_ID.to_string(),
                value: omi.to_string(),
            }
            .save(ctx)
            .await?;
        }

        Ok(())
    }

    /// Saves the channel to be used for the overview message
    pub async fn set_channel(
        &mut self,
        ctx: &Context,
        chan: ChannelId,
        trace: LogTrace,
    ) -> Result<()> {
        trace.step("Looking for channel in guild");
        let gid = load_guild_id(ctx).await?;
        let channels = gid.channels(ctx).await?;
        if let Some(channel) = channels.get(&chan) {
            trace.step("Found. Setting new channel internally");
            self.overview_channel_id = Some(channel.id);
        } else {
            return Err(SignupBoardError::ChannelNotFound(chan).into());
        }
        Ok(())
    }

    /// Creates the message for the overview and saves the message id internally
    pub async fn create_overview(&mut self, ctx: &Context, trace: LogTrace) -> Result<()> {
        trace.step("Loading channel for overview");
        let chan = match self.overview_channel_id {
            Some(c) => c,
            None => return Err(SignupBoardError::OverviewChannelNotSet.into()),
        };

        trace.step("Writing initial message to overview");
        let msg = chan
            .send_message(ctx, |m| {
                m.set_embed(CreateEmbed::info_box("Setting up overview message"))
            })
            .await?;

        trace.step("Setting new message internally");
        self.overview_message_id = Some(msg.id);

        Ok(())
    }

    /// Loads all relevant training(s) from the db and updates the overview message
    pub async fn update_overview(&self, ctx: &Context, trace: LogTrace) -> Result<()> {
        trace.step("Loading overview information");
        let msg = match self.overview_message_id {
            Some(m) => m,
            None => return Err(SignupBoardError::OverviewMessageNotSet.into()),
        };
        let chan = match self.overview_channel_id {
            Some(c) => c,
            None => return Err(SignupBoardError::OverviewChannelNotSet.into()),
        };

        trace.step("Loading training(s)");
        let active_trainings = db::Training::all_active(ctx).await?;

        struct TierInfo {
            _tier: db::Tier,
            discord: Vec<RoleId>,
        }

        struct TrainingInfo {
            training: db::Training,
            signup_count: i64,
            tier_info: Option<TierInfo>,
            bosses: Vec<db::TrainingBoss>,
        }

        trace.step("Loading additional traning info");
        let mut trainings: Vec<TrainingInfo> = Vec::new();
        for training in active_trainings {
            let signup_count = training.get_signup_count(ctx).await?;
            let tier = training.get_tier(ctx).await.transpose()?;
            let tier_info = if let Some(_tier) = tier {
                let discord = _tier
                    .get_discord_roles(ctx)
                    .await?
                    .into_iter()
                    .map(|t| RoleId::from(t.discord_role_id as u64))
                    .collect::<Vec<_>>();

                Some(TierInfo { _tier, discord })
            } else {
                None
            };
            let mut bosses = training.all_training_bosses(ctx).await?;
            bosses.sort_by_key(|b| b.position);
            bosses.sort_by_key(|b| b.wing);

            trainings.push(TrainingInfo {
                training,
                signup_count,
                tier_info,
                bosses,
            });
        }

        // Sort by custom names and dates
        trainings.sort_by(|a, b| title_sort_value(&b.training).cmp(&title_sort_value(&a.training)));
        trainings.sort_by(|a, b| a.training.date.date().cmp(&b.training.date.date()));

        let mut _groups: Vec<(NaiveDate, Vec<&TrainingInfo>)> = Vec::new();
        for (d, v) in trainings
            .iter()
            .group_by(|t| t.training.date.date())
            .into_iter()
        {
            _groups.push((d, v.collect()));
        }

        let mut groups: Vec<(NaiveDate, Vec<&TrainingInfo>, usize)> =
            Vec::with_capacity(_groups.len());
        for (d, v) in _groups {
            // FIXME do this without extra db access
            let mut total_users = db::User::by_signed_up_and_date(ctx, d).await?;
            total_users.sort_by_key(|u| u.id);
            total_users.dedup_by_key(|u| u.id);
            groups.push((d, v, total_users.len()));
        }

        let base_emb = CreateEmbed::xdefault();

        trace.step("Updating overview message");
        chan.edit_message(ctx, msg, |m| {
            m.add_embed(|e| {
                e.0 = base_emb.0.clone();
                e.title("Sign up for a training");
                e.field(
                    "How to",
                    "\
Before you can sign up you have to be __registered__. \
To do so simply use the `/register` command in any channel you have write permissions in.\n\n\
To **sign up**, **sign out** or to **edit** your sign-up click the button at the end of the message",
                    false);
                e.field(
                    "Legend",
                    format!(
                        "{} => {}\n{} => {}\n{} => {}",
                        GREEN_CIRCLE_EMOJI, "You can join this training or edit/remove your sign-up",
                        LOCK_EMOJI, "The training is locked. Most likely squadmaking is in progress",
                        RUNNING_EMOJI, "The training is currently ongoing"
                        ),
                    false);
                e.footer(|f| f.text("Last update"));
                e.timestamp(chrono::Utc::now())
            });
            for (date, trainings, total) in groups {
                m.add_embed(|e| {
                    e.0 = base_emb.0.clone();
                    e.title(date.format("__**%A**, %v__"));
                    e.description(&format!("Total sign-up count: {}", total));
                    for t in trainings {
                        let mut details = format!("`     Time    `   <t:{}:t>", t.training.date.timestamp());
                        if let Some(tier) = &t.tier_info {
                            details.push_str(&format!("\n`Tier required`   {}", tier.discord.iter().map(|d| Mention::from(*d)).join(" ")));
                        } else {
                            details.push_str("\n`Tier required`   None");
                        }
                        details.push_str(&format!("\n`Sign-up count`   {}", t.signup_count));
                        match t.bosses.len() {
                            0 => (),
                            1 => details.push_str("\n`     Boss    `   "),
                            _ => details.push_str("\n`  Boss Pool  `   "),
                        }
                        let boss_emojis = t.bosses
                            .iter()
                            .map(|b| Mention::from(EmojiId::from(b.emoji as u64)).to_string())
                            .collect::<Vec<_>>()
                            .join(" ");
                        details.push_str(&boss_emojis);

                        e.field(
                            format!(
                                "{}    **{}**",
                                match t.training.state {
                                    db::TrainingState::Created => CONSTRUCTION_SITE_EMOJI,
                                    db::TrainingState::Open => GREEN_CIRCLE_EMOJI,
                                    db::TrainingState::Closed => LOCK_EMOJI,
                                    db::TrainingState::Started => RUNNING_EMOJI,
                                    db::TrainingState::Finished => CROSS_EMOJI,
                                },
                                &t.training.title),
                            details,
                            false
                        );
                    }
                    e
                });
            }
            m.components(|c| {
                if !trainings.is_empty() {
                    c.add_action_row(interactions::overview_action_row());
                }
                c
            });
            m
        }).await?;

        Ok(())
    }
}
