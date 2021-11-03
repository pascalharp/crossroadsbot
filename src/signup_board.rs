use crate::{components, data, db, embeds};
use chrono::Datelike;
use chrono::NaiveDate;
use serenity::{model::prelude::*, prelude::*};
use std::convert::TryFrom;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub const SIGNUP_BOARD_NAME: &str = "signup_board_id";
const CHANNEL_TIME_FORMAT: &str = "%a-%d-%b-%Y";

// We are not holding on to any information
pub struct SignupBoard {}

// loads the signup board category and guild Id of managed server
async fn load_meta_infos(ctx: &Context) -> Result<(GuildId, ChannelId)> {
    // Load the configured guild category from the db
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

    Ok((guild_id, channel_category))
}

async fn update_signup_board_training(ctx: &Context, msg: &mut Message, training: &db::Training) -> Result<()> {

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
    Ok(())
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

impl SignupBoard {

    // Posts a channel for a day if it does not exist yet
    pub async fn post_channel(ctx: &Context, training: &db::Training) -> Result<GuildChannel> {
        let date = training.date.date();
        let channel = match db::SignupBoardChannel::by_day(ctx, date).await {
            Ok(ch) => Some(ch),
            Err(diesel::NotFound) => None,
            Err(e) => return Err(e.into())
        };
        if let Some(ch) = channel {
            if let Ok(ch) = ctx.http.get_channel(*ch.channel().as_u64()).await {
                if let Some(gch) = ch.guild() {
                        return Ok(gch);
                }
            }
            // Something is weird, remove db entry
            ch.delete(ctx).await?;
        }
        // If we get here something went wrong, either channel does not exist, no db entry, ...
        let (gid, cit) = load_meta_infos(ctx).await?;
        // create new channel
        let channel = insert_channel_ordered(ctx, gid, cit, date).await?;
        // add to db
        db::SignupBoardChannel::insert(ctx, date, channel.id).await?;
        return Ok(channel);
    }

    // Returns the channel for the training if it exists
    pub async fn get_channel(ctx: &Context, training: &db::Training) -> Result<Option<GuildChannel>> {
        let date = training.date.date();
        let sbc = match db::SignupBoardChannel::by_day(ctx, date).await {
            Ok(ch) => ch,
            Err(diesel::NotFound) => return Ok(None),
            Err(e) => return Err(e.into())
        };
        if let Ok(ch) = ctx.http.get_channel(*sbc.channel().as_u64()).await {
            if let Some(gch) = ch.guild() {
                return Ok(Some(gch));
            }
        }
        // Something is weird, remove db entry
        sbc.delete(ctx).await?;
        return Ok(None);
    }

    // This checks if there are no more trainings for a channel and deletes it
    // This will take the db data as reference and not the actual channel
    // so if a training is still on the board by accident the channel will be deleted anyway
    // If there are trainings left on that day but not on the board by accident the channel
    // will stay up (even if empty). A reset should hopefully post the trainings again
    pub async fn check_channel(ctx: &Context, day: NaiveDate) -> Result<()> {

        // check if channel even registered in db
        let sbc = match db::SignupBoardChannel::by_day(ctx, day).await {
            Ok(ch) => ch,
            Err(diesel::NotFound) => return Ok(()),
            Err(e) => return Err(e.into())
        };

        let count = db::Training::amount_active_by_day(ctx, day).await?;
        if count > 0 { return Ok(()) }

        if let Ok(ch) = ctx.http.get_channel(*sbc.channel().as_u64()).await {
            ch.delete(ctx).await?;
        }

        sbc.delete(ctx).await?;
        Ok(())
    }

    // Posts a training on the signup board
    // This first checks if a msg is already registered with that training
    // if so, it will check if the message still exists and updates it
    // if no message registered or message not found create a new one
    // we need a mutable reference to training to update msg id if needed
    pub async fn post_training(ctx: &Context, training: &mut db::Training) -> Result<Message> {

        let channel = SignupBoard::post_channel(ctx, training).await?;

        let mut msg = match training.board_message() {
            Some(msg_id) => {
                if let Ok(msg) = channel.message(ctx, msg_id).await {
                    // Message already registered and still in channel
                    msg
                } else {
                    // Message already registered but not found anymore. Create new
                    let msg = channel.send_message(ctx, |f| f.content("Loading training")).await?;
                    training.set_board_msg(ctx, Some(msg.id.0)).await?;
                    msg
                }
            }
            None => {
                // Message not registered in db
                let msg = channel.send_message(ctx, |f| f.content("Loading training")).await?;
                training.set_board_msg(ctx, Some(msg.id.0)).await?;
                msg
            }
        };

        update_signup_board_training(ctx, &mut msg, training).await?;
        Ok(msg)
    }

    pub async fn delete_training(ctx: &Context, training: &db::Training) -> Result<()> {

        let msg_id = match training.board_message() {
            Some(id) => id,
            None => return Ok(())
        };

        let channel = match SignupBoard::get_channel(ctx, training).await? {
            Some(ch) => ch,
            // if channel doesnt exist, msg cant exist either
            None => return Ok(()),
        };

        channel.message(ctx, msg_id).await?.delete(ctx).await?;
        // check if channel is empty now and has to be deleted
        Self::check_channel(ctx, training.date.date()).await?;
        Ok(())
    }

    // This updates or inserts a training to the signup board
    // Try to avoid calling this too often since it does a lot of networking
    pub async fn update_training(ctx: &Context, training_id: i32) -> Result<Option<Message>> {
        let mut training = db::Training::by_id(ctx, training_id).await?;
        // only accept correct state
        match training.state {
            db::TrainingState::Open | db::TrainingState::Closed | db::TrainingState::Started => {
                let msg = Self::post_training(ctx, &mut training).await?;
                Ok(Some(msg))
            }
            _ => {
                let _msg = Self::delete_training(ctx, &training).await?;
                Ok(None)
            }
        }
    }

    pub async fn reset(ctx: &Context) -> Result<()> {


        // load all trainings to be posted
        let trainings = db::Training::all_active(ctx).await?;
        for t in trainings {
            // rather inefficient since update_training calls the db again
            SignupBoard::update_training(ctx, t.id).await?;
        }

        Ok(())
    }

    pub async fn reset_hard(ctx: &Context) -> Result<()> {
        // Load all channels for category from the guild that are in the category
        let (gid, cit) = load_meta_infos(ctx).await?;
        // Load all channels in the signup board category
        let channels = gid
            .channels(ctx)
            .await?
            .into_iter()
            .map(|(_, ch)| ch)
            .filter(|ch| ch.category_id.eq(&Some(cit)))
            .collect::<Vec<_>>();

        // delete all channels
        for ch in channels {
            ch.delete(ctx).await?;
        }

        // post channels again
        Self::reset(ctx).await?;

        Ok(())
    }
}
