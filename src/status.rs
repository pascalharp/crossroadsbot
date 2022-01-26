use serenity::model::gateway::Activity;
use serenity::prelude::*;

use crate::{db, logging, utils};

pub async fn update_status(ctx: &Context) {
    logging::log_discord_err_only(
        ctx,
        logging::LogInfo::automatic("Updating status"),
        |trace| async move {
            trace.step("Loading training(s)");
            let trainings_count = db::Training::amount_by_state(ctx, db::TrainingState::Open).await;
            let activity = match trainings_count {
                Ok(0) => {
                    Activity::playing(format!("{} No training available", utils::RED_CIRCLE_EMOJI))
                }
                Ok(1) => Activity::playing(format!(
                    "{} 1 training available",
                    utils::GREEN_CIRCLE_EMOJI,
                )),
                Ok(n) => Activity::playing(format!(
                    "{} {} trainings available",
                    utils::GREEN_CIRCLE_EMOJI,
                    n
                )),
                Err(_) => {
                    Activity::playing(format!("{} figuring out some issues", utils::DIZZY_EMOJI))
                }
            };

            trace.step("Setting activity");
            ctx.set_activity(activity).await;
            Ok(())
        },
    )
    .await;
}
