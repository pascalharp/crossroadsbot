use serenity::{
    prelude::*,
    framework::standard::{
        macros::group
    },
    model::prelude::*,
};

use std::{
    sync::Arc,
};

use dashmap::DashSet;

pub struct Conversation<'a> {
    lock: Arc<DashSet<UserId>>,
    pub user: &'a User,
    pub chan: PrivateChannel
}

impl<'a> Conversation<'a>  {
    pub async fn start(ctx: &'a Context, user: &'a User) -> Result<Conversation<'a>, ()> {

        let lock = {
            let data_read = ctx.data.read().await;
            data_read.get::<ConversationLock>().unwrap().clone()
        };

        if lock.insert(user.id) {
            if let Ok(chan) = user.create_dm_channel(ctx).await {
                return Ok( Conversation {
                            lock: lock,
                            user: user,
                            chan: chan
                        });
            }
        }

        Err(())
    }
}

impl<'a> Drop for Conversation<'a> {

    fn drop(&mut self) {
        self.lock.remove(&self.user.id);
    }
}

pub struct ConversationLock;
impl TypeMapKey for ConversationLock {
    type Value = Arc<DashSet<UserId>>;
}

mod misc;
use misc::*;
#[group]
#[commands(ping,dudu)]
struct Misc;

mod signup;
use signup::*;
#[group]
#[commands(register)]
struct Signup;
