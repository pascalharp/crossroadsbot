// automatic background tasks
use crate::signup_board::SignupBoard;
use serenity::client::Context;
use std::time::Duration;
use tracing::{error, info};

pub async fn signup_board_task(ctx: Context) {
    loop {
        let res = SignupBoard::get(&ctx)
            .await
            .read()
            .await
            .update_overview(&ctx)
            .await;
        if let Err(res) = res {
            error!("Signup board update error: {}", res);
        } else {
            info!("Signup board updated");
        }
        tokio::time::sleep(Duration::from_secs(60 * 5)).await;
    }
}
