// automatic background tasks
use crate::{logging::*, signup_board::SignupBoard};
use serenity::client::Context;
use std::time::Duration;

pub async fn signup_board_task(ctx: Context) {
    let ctx = &ctx;
    loop {
        log_discord(
            ctx,
            LogInfo::automatic("Update Signup Board"),
            |trace| async move {
                trace.step("Updating board");
                SignupBoard::get(ctx)
                    .await
                    .read()
                    .await
                    .update_overview(ctx)
                    .await?;
                Ok(())
            },
        )
        .await;
        tokio::time::sleep(Duration::from_secs(60 * 5)).await;
    }
}
