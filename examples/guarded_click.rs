//! Resolve a fresh observation revision before dispatching a guarded click.

use glass::{BrowserSession, SessionOptions};

#[tokio::main]
async fn main() -> glass::BrowserResult<()> {
    let options = SessionOptions::builder().build()?;
    let session = BrowserSession::start(&options).await?;
    let observation = session.observe().await?;
    let outcome = session
        .click_with_revision(
            "role=button[name=Submit]",
            observation.accessibility.revision,
        )
        .await?;
    println!("{}", serde_json::to_string_pretty(&outcome)?);
    session.close().await
}
