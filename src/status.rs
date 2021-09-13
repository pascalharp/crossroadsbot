use serenity::model::gateway::Activity;
use serenity::prelude::*;

use crate::db;
use crate::utils;

pub async fn update_status(ctx: &Context) {
    let trainings_count = db::Training::amount_by_state(ctx, db::TrainingState::Open).await;
    let activity = match trainings_count {
        Ok(0) => Activity::playing(format!("{} zero open trainings", utils::RED_CIRCLE_EMOJI)),
        Ok(n) => Activity::playing(format!(
            "{} {} open trainings",
            utils::GREEN_CIRCLE_EMOJI,
            n
        )),
        Err(_) => Activity::playing(format!("{} figuring out some issues", utils::DIZZY_EMOJI)),
    };

    ctx.set_activity(activity).await;
}
