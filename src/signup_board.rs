use crate::{components, data, db, embeds};
use chrono::Datelike;
use chrono::NaiveDate;
use serenity::{model::prelude::*, prelude::*};
use std::convert::TryFrom;
use std::sync::Arc;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub const SIGNUP_BOARD_NAME: &str = "signup_board_id";
const CHANNEL_TIME_FORMAT: &str = "%a-%d-%b-%Y";

// We are not holding on to any information
pub struct SignupBoard {}

impl SignupBoard {
    // posts a training to the signup board
    async fn post_training(ctx: &Context, training: &db::Training) -> Result<Message> {
        // Load all channels for category from the guild that are in the category
        let channel_category: ChannelId = db::Config::load(ctx, SIGNUP_BOARD_NAME.to_string())
            .await?
            .value
            .parse::<u64>()?
            .into();
        // Load guild id provided on startup
        let guild_id = ctx
            .data
            .read()
            .await
            .get::<data::ConfigValuesData>()
            .unwrap()
            .main_guild_id;

        // now check if one channel already matches the date string
        let time_fmt = training
            .date
            .format(CHANNEL_TIME_FORMAT)
            .to_string()
            .to_lowercase()
            .replace(" ", "");
        let channel = guild_id
            .channels(ctx)
            .await?
            .into_iter()
            .map(|(_, ch)| ch)
            .filter(|ch| ch.category_id.eq(&Some(channel_category)))
            .find(|ch| ch.name.eq(&time_fmt));

        // Use channel or create new one if none found
        let channel = match channel {
            Some(ch) => ch,
            None => {
                SignupBoard::insert_channel_ordered(
                    ctx,
                    guild_id,
                    channel_category,
                    training.date.date(),
                )
                .await?
            }
        };

        // check if Training is on the board yet.
        // 100 is discord limit but that should be easily enough
        // if we actually ever get more than 100 trainings on one day I am happy to rework this ;)
        let channel_msgs = channel.messages(ctx, |msg| msg.limit(100)).await?;

        let msg = channel_msgs.into_iter().find(|m| {
            m.embeds.get(0).map_or(false, |e| {
                e.description.as_ref().map_or(false, |d| {
                    // clone is good here to not change orig msg if we use it to update
                    d.clone()
                        .replace("||", "")
                        .parse::<i32>()
                        .map_or(false, |id| id.eq(&training.id))
                })
            })
        });

        // load tier and roles information
        let roles = training.active_roles(ctx).await?;
        let tiers = {
            let tier = training.get_tier(ctx).await;
            match tier {
                None => None,
                Some(t) => {
                    let t = t?;
                    let r = t.get_discord_roles(ctx).await?;
                    Some((t, r))
                }
            }
        };

        let msg = match msg {
            Some(mut msg) => {
                msg.edit(ctx, |m| {
                    m.embed(|e| {
                        e.0 = embeds::signupboard_embed(training, &roles, &tiers).0;
                        e
                    });
                    m.components(|c| {
                        if training.state.eq(&db::TrainingState::Open) {
                            c.add_action_row(components::signup_action_row(training.id));
                        }
                        c
                    })
                })
                .await?;
                msg
            }
            None => {
                channel
                    .send_message(ctx, |m| {
                        m.embed(|e| {
                            e.0 = embeds::signupboard_embed(training, &roles, &tiers).0;
                            e
                        });
                        m.components(|c| {
                            if training.state.eq(&db::TrainingState::Open) {
                                c.add_action_row(components::signup_action_row(training.id));
                            }
                            c
                        })
                    })
                    .await?
            }
        };
        Ok(msg)
    }

    async fn delete_training(ctx: &Context, training: &db::Training) -> Result<Message> {
        // Load all channels for category from the guild that are in the category
        let channel_category: ChannelId = db::Config::load(ctx, SIGNUP_BOARD_NAME.to_string())
            .await?
            .value
            .parse::<u64>()?
            .into();
        // Load guild id provided on startup
        let guild_id = ctx
            .data
            .read()
            .await
            .get::<data::ConfigValuesData>()
            .unwrap()
            .main_guild_id;

        // now check if one channel already matches the date string
        let time_fmt = training
            .date
            .format(CHANNEL_TIME_FORMAT)
            .to_string()
            .to_lowercase()
            .replace(" ", "");
        let channel = guild_id
            .channels(ctx)
            .await?
            .into_iter()
            .map(|(_, ch)| ch)
            .filter(|ch| ch.category_id.eq(&Some(channel_category)))
            .find(|ch| ch.name.eq(&time_fmt));

        let channel = match channel {
            None => return Err("The training was not on the signup board".into()),
            Some(c) => c,
        };

        // find training msg in the channel
        // 100 is discord limit but that should be easily enough
        // if we actually ever get more than 100 trainings on one day I am happy to rework this ;)
        let channel_msgs = channel.messages(ctx, |msg| msg.limit(100)).await?;
        let msgs_count = channel_msgs.len();

        let msg = channel_msgs.into_iter().find(|m| {
            m.embeds.get(0).map_or(false, |e| {
                e.description.as_ref().map_or(false, |d| {
                    // clone is good here to not change orig msg if we use it to update
                    d.clone()
                        .replace("||", "")
                        .parse::<i32>()
                        .map_or(false, |id| id.eq(&training.id))
                })
            })
        });

        let msg = match msg {
            None => {
                // msg not found, but clean up if there are no messages left
                if msgs_count == 0 {
                    channel.delete(ctx).await?;
                }
                return Err("The training was not on the signup board".into());
            }
            Some(m) => m,
        };

        // Found message. KILL it with fire....
        msg.delete(ctx).await?;
        // If this was the last message in the channel, remove whole channel
        if msgs_count <= 1 {
            channel.delete(ctx).await?;
        }
        Ok(msg)
    }

    // This updates or inserts a training to the signup board
    // Try to avoid calling this too often since it does a lot of networking
    pub async fn update_training(ctx: &Context, training_id: i32) -> Result<Option<Message>> {
        let training = db::Training::by_id(ctx, training_id).await?;
        // only accept correct state
        match training.state {
            db::TrainingState::Open | db::TrainingState::Closed | db::TrainingState::Started => {
                let msg = Self::post_training(ctx, &training).await?;
                Ok(Some(msg))
            }
            _ => {
                let _msg = Self::delete_training(ctx, &training).await?;
                Ok(None)
            }
        }
    }

    // cit is ChannelCategory Id
    async fn insert_channel_ordered(
        ctx: &Context,
        gid: GuildId,
        cit: ChannelId,
        date: NaiveDate,
    ) -> Result<GuildChannel> {
        let date_num =
            date.day0() + (date.month0() << 6u32) + (u32::try_from(date.year())? << (6u32 + 5u32));

        let time_fmt = date
            .format(CHANNEL_TIME_FORMAT)
            .to_string()
            .to_lowercase()
            .replace(" ", "");

        Ok(gid
            .create_channel(ctx, |ch| {
                ch.category(cit);
                ch.kind(ChannelType::Text);
                ch.topic("Use the buttons to join/edite/delete signups");
                ch.name(time_fmt);
                ch.position(date_num);
                ch
            })
            .await?)
    }

    pub async fn reset(ctx: &Context) -> Result<()> {
        // Load all channels for category from the guild that are in the category
        let channel_category: ChannelId = db::Config::load(ctx, SIGNUP_BOARD_NAME.to_string())
            .await?
            .value
            .parse::<u64>()?
            .into();
        // Load guild id provided on startup
        let guild_id = ctx
            .data
            .read()
            .await
            .get::<data::ConfigValuesData>()
            .unwrap()
            .main_guild_id;
        // Load all channels in the signup board category
        let channels = guild_id
            .channels(ctx)
            .await?
            .into_iter()
            .map(|(_, ch)| ch)
            .filter(|ch| ch.category_id.eq(&Some(channel_category)))
            .collect::<Vec<_>>();

        // delete all channels
        for ch in channels {
            ch.delete(ctx).await?;
        }

        // load all trainings to be posted
        let trainings = db::Training::all_active(ctx).await?;
        for t in trainings {
            // rather inefficient since update_training calls the db again
            SignupBoard::update_training(ctx, t.id).await?;
        }

        Ok(())
    }
}

pub enum SignupBoardAction {
    Ignore,                          // if not on a SignupBoard msg
    None,                            // if invalid emoji
    JoinSignup(Arc<db::Training>),   // join
    EditSignup(Arc<db::Training>),   // edit
    RemoveSignup(Arc<db::Training>), // remove
}
