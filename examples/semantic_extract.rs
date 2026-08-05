//! Extract bounded typed records from one fresh semantic observation.

use glass::browser::session::{ExtractionField, ExtractionKind, StructuredExtractionRequest};
use glass::{BrowserSession, SessionOptions};

#[tokio::main]
async fn main() -> glass::BrowserResult<()> {
    let options = SessionOptions::builder().build()?;
    let session = BrowserSession::start(&options).await?;
    let request = StructuredExtractionRequest {
        fields: vec![ExtractionField {
            name: "title".into(),
            path: "title".into(),
            kind: ExtractionKind::Scalar,
        }],
        region_id: None,
        start_index: 0,
        continuation: None,
        max_items: 16,
        max_bytes: 16 * 1024,
    };
    let result = session.extract_structured(&request).await?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    session.close().await
}
