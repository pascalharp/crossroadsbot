use dotenv::dotenv;
use crossroadsbot::db;
use crossroadsbot::bot;

#[tokio::main]
async fn main() {

    dotenv().ok();
    println!("Hello Crossroads!");

    // Make a quick check to the database
    {
        db::connect();
    }

    bot::start().await;

}
