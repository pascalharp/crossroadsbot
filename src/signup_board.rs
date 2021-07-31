use crate::{data, db, embeds, utils};
use chrono::prelude::*;
use serenity::{futures::StreamExt, model::prelude::*, prelude::*};
use std::{collections::HashMap, fmt, sync::Arc};
use tracing::{error, info};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub static SIGNUP_BOARD_NAME: &str = "signup_board";

pub struct SignupBoard {
    category: Option<ChannelId>,
    date_channels: HashMap<NaiveDate, GuildChannel>,
    msg_trainings: HashMap<MessageId, Arc<db::Training>>,
}

pub enum SignupBoardAction {
    Ignore,                          // if not on a SignupBoard msg
    None,                            // if invalid emoji
    JoinSignup(Arc<db::Training>),   // join
    EditSignup(Arc<db::Training>),   // edit
    RemoveSignup(Arc<db::Training>), // remove
}

impl fmt::Display for SignupBoardAction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SignupBoardAction::Ignore => {
                write!(f, "Ignored")
            }
            SignupBoardAction::None => write!(f, "None"),
            SignupBoardAction::JoinSignup(t) => {
                write!(f, "Join ({})", t.id)
            }
            SignupBoardAction::EditSignup(t) => {
                write!(f, "Edit ({})", t.id)
            }
            SignupBoardAction::RemoveSignup(t) => {
                write!(f, "Remove ({})", t.id)
            }
        }
    }
}

impl SignupBoard {
    pub fn new() -> SignupBoard {
        SignupBoard {
            category: None,
            date_channels: HashMap::new(),
            msg_trainings: HashMap::new(),
        }
    }

    pub fn set_category_channel(&mut self, id: ChannelId) {
        self.category = Some(id);
    }

    async fn post_training(
        &self,
        ctx: &Context,
        chan: ChannelId,
        training: Arc<db::Training>,
    ) -> Result<Message> {
        let tier_roles = match training.get_tier(ctx).await {
            None => None,
            Some(t) => match t {
                Ok(ok) => {
                    let tier = Arc::new(ok);
                    let roles = match tier.clone().get_discord_roles().await {
                        Ok(ok) => Arc::new(ok),
                        Err(e) => {
                            error!("Failed to load discord roles for tier {}", e);
                            return Err(e.into());
                        }
                    };
                    Some((tier, roles))
                }
                Err(e) => {
                    error!("Failed to load tier for training {}", e);
                    return Err(e.into());
                }
            },
        };

        let mut embed = embeds::training_base_embed(&training);
        embeds::training_embed_add_tier(&mut embed, &tier_roles, true);
        embeds::training_embed_add_board_footer(&mut embed, &training.state);

        // post message
        let msg = match chan
            .send_message(ctx, |m| {
                m.allowed_mentions(|a| a.empty_parse());
                m.embed(|e| {
                    e.0 = embed.0;
                    e
                })
            })
            .await
        {
            Ok(ok) => ok,
            Err(e) => {
                info!("Failed send message {}", e);
                return Err(e.into());
            }
        };

        // add reactions
        if db::TrainingState::Open == training.state {
            msg.react(ctx, utils::CHECK_EMOJI).await?;
            msg.react(ctx, utils::MEMO_EMOJI).await?;
            msg.react(ctx, utils::CROSS_EMOJI).await?;
        }

        Ok(msg)
    }

    async fn update_training(
        &self,
        ctx: &Context,
        chan: ChannelId,
        msg: MessageId,
        training: Arc<db::Training>,
    ) -> Result<()> {
        let tier_roles = match training.get_tier(ctx).await {
            None => None,
            Some(t) => match t {
                Ok(ok) => {
                    let tier = Arc::new(ok);
                    let roles = match tier.clone().get_discord_roles().await {
                        Ok(ok) => Arc::new(ok),
                        Err(e) => {
                            error!("Failed to load discord roles for tier {}", e);
                            return Err(e.into());
                        }
                    };
                    Some((tier, roles))
                }
                Err(e) => {
                    error!("Failed to load tier for training {}", e);
                    return Err(e.into());
                }
            },
        };

        let mut embed = embeds::training_base_embed(&training);
        embeds::training_embed_add_tier(&mut embed, &tier_roles, true);
        embeds::training_embed_add_board_footer(&mut embed, &training.state);

        let msg = match chan
            .edit_message(ctx, msg, |m| {
                m.embed(|e| {
                    e.0 = embed.0;
                    e
                })
            })
            .await
        {
            Ok(m) => m,
            Err(e) => {
                info!("Failed send message {}", e);
                return Err(e.into());
            }
        };

        chan.message(ctx, &msg).await?.delete_reactions(ctx).await?;
        if db::TrainingState::Open == training.state {
            // add reactions
            msg.react(ctx, utils::CHECK_EMOJI).await?;
            msg.react(ctx, utils::MEMO_EMOJI).await?;
            msg.react(ctx, utils::CROSS_EMOJI).await?;
        }

        Ok(())
    }

    async fn remove_training(
        &mut self,
        ctx: &Context,
        chan: ChannelId,
        msg: MessageId,
    ) -> Result<()> {
        // remove message
        chan.delete_message(ctx, msg).await?;
        self.msg_trainings.remove(&msg);

        // check if channel has no more messages. If so. remove
        let mut messages = chan.messages_iter(&ctx).boxed();
        let mut found: bool = false;
        while let Some(msg_res) = messages.next().await {
            match msg_res {
                Ok(msg) => {
                    if self.msg_trainings.contains_key(&msg.id) {
                        found = true;
                        break;
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }

        // if to remove. find HashMap Entry
        if !found {
            let key = self.date_channels.iter().find_map(|(k, v)| {
                if v.id == chan {
                    Some(k.clone())
                } else {
                    None
                }
            });
            if let Some(key) = key {
                self.date_channels.remove(&key);
            }
            // delete channel (either way)
            chan.delete(ctx).await?;
        }

        Ok(())
    }

    // Checks if a channel for the date already exists and returns it
    // or creates a new channel with the date
    async fn get_channel(
        &mut self,
        ctx: &Context,
        training: Arc<db::Training>,
    ) -> Result<ChannelId> {
        let category = match self.category {
            Some(ok) => ok,
            None => {
                info!("Guild category for signup board not set");
                return Err("Guild category for signup board not set".into());
            }
        };

        let guild_id = match ctx.data.read().await.get::<data::ConfigValuesData>() {
            Some(conf) => conf.main_guild_id,
            None => {
                error!("Failed to load configuration for guild id");
                return Err("Failed to load configuration for guild id".into());
            }
        };

        let guild = match PartialGuild::get(ctx, guild_id).await {
            Ok(g) => g,
            Err(e) => {
                error!("Failed to load main guild information: {}", e);
                return Err(e.into());
            }
        };

        let date = training.date.date();
        // If channel not exists create it
        if !self.date_channels.contains_key(&date) {
            let channel = match guild
                .create_channel(ctx, |c| {
                    c.name(date.format("%a, %v"));
                    c.category(category);
                    c
                })
                .await
            {
                Ok(ok) => ok,
                Err(e) => {
                    error!("Failed to create channel: {}", e);
                    return Err(e.into());
                }
            };

            self.date_channels.insert(date, channel);
        }

        // Retrieve channel
        let channel = match self.date_channels.get(&date) {
            None => {
                return Err(format!("Expected to find channel for date: {}", date)
                    .to_string()
                    .into());
            }
            Some(s) => s,
        };
        Ok(channel.id.clone())
    }

    // Fully resets all channels by deleting and recreating them not assume that
    // the current information in the SignupBoard struct is correct
    pub async fn reset(&mut self, ctx: &Context) -> Result<()> {
        let category = match self.category {
            Some(ok) => ok,
            None => {
                info!("Guild category for signup board not set");
                return Ok(());
            }
        };

        let guild_id = match ctx.data.read().await.get::<data::ConfigValuesData>() {
            Some(conf) => conf.main_guild_id,
            None => {
                error!("Failed to load configuration for guild id");
                return Ok(());
            }
        };

        let guild = match PartialGuild::get(ctx, guild_id).await {
            Ok(g) => g,
            Err(e) => {
                error!("Failed to load main guild information: {}", e);
                return Err(e.into());
            }
        };

        let all_channels = match guild.channels(ctx).await {
            Ok(chan) => chan,
            Err(e) => {
                error!("Failed to load guild channels: {}", e);
                return Err(e.into());
            }
        };

        // Delete all channels in the category
        for chan in all_channels.values() {
            if chan.category_id.map_or(false, |id| id.eq(&category)) {
                if let Err(e) = chan.delete(ctx).await {
                    error!("Failed to delete channel: {}", e);
                }
            }
        }

        // Clear internal info
        self.date_channels.clear();
        self.msg_trainings.clear();

        // Load all active trainings
        let mut trainings = match db::Training::all_active(ctx).await {
            Ok(ok) => ok,
            Err(e) => {
                error!("Failed to load active trainings for sign up board: {}", e);
                return Err(e.into());
            }
        };

        trainings.sort_by(|a, b| a.date.cmp(&b.date));

        // Create channels for the dates
        for t in trainings {
            let training = Arc::new(t);
            let channel = self.get_channel(ctx, training.clone()).await?;
            let msg = self.post_training(ctx, channel, training.clone()).await?;
            self.msg_trainings.insert(msg.id, training);
        }

        Ok(())
    }

    // Updates training information. Creates new channel if needed and deletes channels
    // with no trainings left.
    pub async fn update(&mut self, ctx: &Context, id: i32) -> Result<()> {
        // Get the latest version from the db
        let new_training = match db::Training::by_id(ctx, id).await {
            Ok(ok) => Arc::new(ok),
            Err(e) => {
                error!("Failed to load training: {}", e);
                return Err(e.into());
            }
        };

        // Look for training in current signup board
        let curr_training = self
            .msg_trainings
            .iter()
            .find_map(|(m, t)| t.id.eq(&id).then(|| m.clone()));

        match curr_training {
            None => {
                // Training not on the board yet. Consider adding
                match new_training.state {
                    db::TrainingState::Open
                    | db::TrainingState::Closed
                    | db::TrainingState::Started => {
                        //add to training board
                        let channel = self.get_channel(ctx, new_training.clone()).await?;
                        let msg = self
                            .post_training(ctx, channel, new_training.clone())
                            .await?;
                        self.msg_trainings.insert(msg.id, new_training);
                    }
                    _ => (), //Nothing to do
                }
                ();
            }
            Some(msg) => {
                // Training already on board. Update or remove
                match new_training.state {
                    db::TrainingState::Open
                    | db::TrainingState::Closed
                    | db::TrainingState::Started => {
                        // update training
                        let channel = self.get_channel(ctx, new_training.clone()).await?;
                        self.update_training(ctx, channel, msg, new_training.clone())
                            .await?;
                    }
                    _ => {
                        // remove training
                        let channel = self.get_channel(ctx, new_training.clone()).await?;
                        self.remove_training(ctx, channel, msg).await?;
                    }
                }
            }
        }
        Ok(())
    }

    // We can not do the whole logic in here or we would block the RWMutex
    // So only return what step to take
    pub fn on_reaction(&self, added_reaction: &Reaction) -> SignupBoardAction {
        let training = match self.msg_trainings.get(&added_reaction.message_id) {
            Some(t) => t.clone(),
            None => return SignupBoardAction::Ignore,
        };
        if added_reaction.emoji == ReactionType::from(utils::CHECK_EMOJI) {
            return SignupBoardAction::JoinSignup(training);
        } else if added_reaction.emoji == ReactionType::from(utils::MEMO_EMOJI) {
            return SignupBoardAction::EditSignup(training);
        } else if added_reaction.emoji == ReactionType::from(utils::CROSS_EMOJI) {
            return SignupBoardAction::RemoveSignup(training);
        } else {
            return SignupBoardAction::None;
        }
    }
}
