use crate::{components, data, data::SignupBoardData, db, embeds};
use chrono::Datelike;
use chrono::NaiveDate;
use itertools::Itertools;
use serenity::{model::prelude::*, prelude::*};
use std::{convert::TryFrom, mem, sync::Arc};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
type StdResult<T, E> = std::result::Result<T, E>;

pub const SIGNUP_BOARD_NAME: &str = "signup_board_id";
pub const OVERVIEW_CHANNEL_ID: &str = "overview_channel_id";
pub const OVERVIEW_MESSAGE_ID: &str = "overview_message_id";
const CHANNEL_TIME_FORMAT: &str = "%a-%d-%b-%Y";

// Hold on to often used values
pub struct SignupBoard {
    pub discord_category_id: Option<ChannelId>,
    pub overview_channel_id: Option<ChannelId>,
    pub overview_message_id: Option<MessageId>,
}

#[derive(Debug)]
pub enum SignupBoardError {
    CategoryNotSet,
    OverviewMessageNotSet,
    OverviewChannelNotSet,
    ChannelNotFound(ChannelId),
}

impl std::fmt::Display for SignupBoardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CategoryNotSet => write!(f, "Signupboard category not set"),
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

async fn update_signup_board_training(
    ctx: &Context,
    msg: &mut Message,
    training: &db::Training,
) -> Result<()> {
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

fn title_sort_value(t: &db::Training) -> u64 {
    if t.title.contains("Beginner") {
        return 10;
    }
    if t.title.contains("Intermediate") {
        return 8;
    }
    if t.title.contains("Pracitce") {
        return 6;
    }
    return 0;
}

impl SignupBoard {
    fn get_category_channel(&self) -> StdResult<ChannelId, SignupBoardError> {
        match self.discord_category_id {
            Some(id) => Ok(id),
            None => Err(SignupBoardError::CategoryNotSet),
        }
    }

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
            discord_category_id: match db::Config::load(ctx, SIGNUP_BOARD_NAME.to_string()).await {
                Ok(conf) => Some(conf.value.parse::<ChannelId>()?),
                Err(diesel::NotFound) => None,
                Err(e) => return Err(e.into()),
            },
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
        if let Some(dci) = self.discord_category_id {
            db::Config {
                name: SIGNUP_BOARD_NAME.to_string(),
                value: dci.to_string(),
            }
            .save(ctx)
            .await?;
        }

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

    // this creates the channel for the overview message and the
    // overview message itself.
    // It first tries to remove the old channel and then creates the new one
    // as well as updating the db entry.
    pub async fn set_up_overview(&mut self, ctx: &Context) -> Result<()> {
        match self.overview_channel_id {
            Some(id) => {
                id.delete(ctx).await.ok();
            }
            None => (),
        }
        self.overview_channel_id = None;
        self.overview_message_id = None;
        self.save_to_db(ctx).await?;

        // Try to create channel
        let gid = load_guild_id(ctx).await?;
        let cat = self.get_category_channel()?;
        let chan = gid
            .create_channel(ctx, |ch| {
                ch.category(cat);
                ch.kind(ChannelType::Text);
                ch.topic("This channel contains an overview of all available trainings");
                ch.name("overview");
                ch.position(0)
            })
            .await?;

        self.overview_channel_id = Some(chan.id);
        self.save_to_db(ctx).await?;

        let msg = chan
            .send_message(ctx, |m| m.content("Loading overview ..."))
            .await?;

        self.overview_message_id = Some(msg.id);
        self.save_to_db(ctx).await?;

        Ok(())
    }

    // Updates the overview message if available
    pub async fn update_overview(&self, ctx: &Context) -> Result<()> {
        let msg = match self.overview_message_id {
            Some(m) => m,
            None => return Err(SignupBoardError::OverviewMessageNotSet.into()),
        };
        let chan = match self.overview_channel_id {
            Some(c) => c,
            None => return Err(SignupBoardError::OverviewChannelNotSet.into()),
        };
        let gid = load_guild_id(ctx).await?;

        let active_trainings = db::Training::all_active(ctx).await?;
        let mut trainings: Vec<(db::Training, i64)> = Vec::new();
        for at in active_trainings {
            let count = at.get_signup_count(ctx).await?;
            trainings.push((at, count));
        }

        // Sort by custom names and dates
        trainings.sort_by(|a, b| title_sort_value(&b.0).cmp(&title_sort_value(&a.0)));
        trainings.sort_by(|a, b| a.0.date.date().cmp(&b.0.date.date()));

        // Group by dates
        let mut groups: Vec<(NaiveDate, Vec<(db::Training, i64)>)> = Vec::new();
        for (g, t) in trainings
            .into_iter()
            .group_by(|a| a.0.date.date())
            .into_iter()
        {
            groups.push((g, t.collect::<Vec<_>>()));
        }

        // Get the signup board channels for the dates
        let mut ready: Vec<(NaiveDate, Option<ChannelId>, Vec<(db::Training, i64)>)> = Vec::new();
        for (g, t) in groups {
            let chan = db::SignupBoardChannel::by_day(ctx, g)
                .await
                .ok()
                .map(|sbc| sbc.channel());
            ready.push((g, chan, t));
        }

        chan.edit_message(ctx, msg, |m| {
            m.content("");
            m.set_embed(embeds::signup_board_overview(ready, gid));
            m.components(|c| c.add_action_row(components::register_list_action_row()))
        })
        .await?;

        Ok(())
    }

    // Posts a channel for a day if it does not exist yet
    pub async fn post_channel(
        &self,
        ctx: &Context,
        training: &db::Training,
    ) -> Result<GuildChannel> {
        let date = training.date.date();
        let channel = match db::SignupBoardChannel::by_day(ctx, date).await {
            Ok(ch) => Some(ch),
            Err(diesel::NotFound) => None,
            Err(e) => return Err(e.into()),
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
        // create new channel
        let channel = insert_channel_ordered(
            ctx,
            load_guild_id(ctx).await?,
            self.get_category_channel()?,
            date,
        )
        .await?;
        // add to db
        db::SignupBoardChannel::insert(ctx, date, channel.id).await?;
        return Ok(channel);
    }

    // Returns the channel for the training if it exists
    pub async fn get_channel(
        ctx: &Context,
        training: &db::Training,
    ) -> Result<Option<GuildChannel>> {
        let date = training.date.date();
        let sbc = match db::SignupBoardChannel::by_day(ctx, date).await {
            Ok(ch) => ch,
            Err(diesel::NotFound) => return Ok(None),
            Err(e) => return Err(e.into()),
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
            Err(e) => return Err(e.into()),
        };

        let count = db::Training::amount_active_by_day(ctx, day).await?;
        if count > 0 {
            return Ok(());
        }

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
        let channel = {
            let lock = SignupBoard::get(ctx).await.clone();
            let sb = lock.read().await;
            sb.post_channel(ctx, training).await?
        };

        let mut msg = match training.board_message() {
            Some(msg_id) => {
                if let Ok(msg) = channel.message(ctx, msg_id).await {
                    // Message already registered and still in channel
                    msg
                } else {
                    // Message already registered but not found anymore. Create new
                    let msg = channel
                        .send_message(ctx, |f| f.content("Loading training"))
                        .await?;
                    training.set_board_msg(ctx, Some(msg.id.0)).await?;
                    msg
                }
            }
            None => {
                // Message not registered in db
                let msg = channel
                    .send_message(ctx, |f| f.content("Loading training"))
                    .await?;
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
            None => return Ok(()),
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
        SignupBoard::get(ctx)
            .await
            .read()
            .await
            .update_overview(ctx)
            .await?;

        Ok(())
    }

    pub async fn reset_hard(ctx: &Context) -> Result<()> {
        // Load all channels for category from the guild that are in the category
        let cit = {
            let lock = SignupBoard::get(ctx).await.clone();
            let sb = lock.read().await;
            sb.get_category_channel()?
        };

        let gid = load_guild_id(ctx).await?;

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
