// automatic task trigger by specific events or constantly running in the background
use crate::{logging::*, signup_board::SignupBoard};
use serenity::client::Context;
use std::time::Duration;

pub async fn signup_board_task(ctx: Context) {
    let ctx = &ctx;
    loop {
        log_discord_err_only(
            ctx,
            LogInfo::automatic("Update Signup Board"),
            |trace| async move {
                trace.step("Updating board");
                SignupBoard::get(ctx)
                    .await
                    .read()
                    .await
                    .update_overview(ctx, trace)
                    .await?;
                Ok(())
            },
        )
        .await;
        tokio::time::sleep(Duration::from_secs(60 * 5)).await;
    }
}
