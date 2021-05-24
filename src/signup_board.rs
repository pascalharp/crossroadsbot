use crate::{data, db, embeds};
use chrono::prelude::*;
use serenity::{model::prelude::*, prelude::*};
use std::collections::{HashMap, HashSet};
use tracing::{error, info};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub static SIGNUP_BOARD_NAME: &str = "signup_board";

pub struct SignupBoard {
    category: Option<ChannelId>,
    date_channels: HashMap<NaiveDate, GuildChannel>,
    msg_trainings: HashMap<MessageId, db::Training>,
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

    // Fully resets all channels by deleting and recreating them not assume that
    // the current information in the SignupBoard struct is correct
    pub async fn reset(&mut self, ctx: &Context) -> Result<()> {
        let category = match &self.category {
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
            if chan.category_id.map_or(false, |id| id.eq(category)) {
                if let Err(e) = chan.delete(ctx).await {
                    error!("Failed to delete channel: {}", e);
                }
            }
        }

        // Clear internal info
        self.date_channels.clear();
        self.msg_trainings.clear();

        // Load all active trainings
        let mut trainings = match db::Training::load_active().await {
            Ok(ok) => ok,
            Err(e) => {
                error!("Failed to load active trainings for sign up board: {}", e);
                return Err(e.into());
            }
        };

        trainings.sort_by(|a, b| a.date.cmp(&b.date));

        // Create channels for the dates
        for t in trainings {
            let date = t.date.date();
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
            // Send training msg to channel
            let channel = match self.date_channels.get(&date) {
                None => continue, // We just checked if we have this channel
                Some(s) => s,
            };

            let embed = embeds::training_base_embed(&t);
            let msg = match channel
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

            self.msg_trainings.insert(msg.id, t);
        }

        Ok(())
    }

    // Updates training information. Creates new channel if needed and deletes channels
    // with no trainings left.
    pub async fn update(&self) -> Result<()> {
        let category = match &self.category {
            Some(ok) => ok,
            None => return Ok(()),
        };

        // TODO

        Ok(())
    }
}
