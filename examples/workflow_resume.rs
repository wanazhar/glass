//! Validate a persisted workflow checkpoint before attempting reconciliation.

use glass::BrowserSession;
use std::env;
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args()
        .nth(1)
        .ok_or("usage: workflow_resume CHECKPOINT.json")?;
    let source = fs::read_to_string(path)?;
    let checkpoint = BrowserSession::parse_workflow_checkpoint(&source)?;
    println!("{}", serde_json::to_string_pretty(&checkpoint)?);
    Ok(())
}
