//! Start an isolated Glass session and inspect the current page.

use glass_browser::{BrowserSession, SessionOptions};

#[tokio::main]
async fn main() -> glass_browser::BrowserResult<()> {
    let options = SessionOptions::builder().build()?;
    let session = BrowserSession::start(&options).await?;
    let observation = session.observe().await?;
    println!("{}", serde_json::to_string_pretty(&observation)?);
    session.close().await
}
