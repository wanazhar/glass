//! Build the MCP initialize request used by a Glass client.

use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "glass-example", "version": env!("CARGO_PKG_VERSION")}
        }
    });
    println!("{}", serde_json::to_string_pretty(&request)?);
    Ok(())
}
