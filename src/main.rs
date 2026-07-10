use clap::Parser;
use tracing::info;

mod browser;
mod cli;
mod mcp;
mod tui;

use cli::main::Cli;
use cli::main::Commands;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::InstallChromium) => {
            info!("Installing Chromium...");
            let path = browser::chrome::download_chromium().await?;
            println!("Chromium installed at: {}", path.display());
            return Ok(());
        }
        Some(Commands::Profiles) => {
            let manager = browser::profile::ProfileManager::new();
            let profiles = manager.list_profiles()?;
            if profiles.is_empty() {
                println!("No profiles found.");
            } else {
                println!("Saved profiles:");
                for name in &profiles {
                    println!("  - {name}");
                }
            }
            return Ok(());
        }
        Some(Commands::DeleteProfile { name }) => {
            let manager = browser::profile::ProfileManager::new();
            manager.delete_profile(name)?;
            println!("Profile '{name}' deleted.");
            return Ok(());
        }
        None => {}
    }

    // MCP server mode
    if cli.mcp {
        info!("Starting MCP server...");
        mcp::server::run_mcp_server().await?;
        return Ok(());
    }

    // CLI mode: one-shot command
    if let Some(prompt) = &cli.prompt {
        info!("CLI mode: {prompt}");
        run_cli_mode(&cli, prompt).await?;
        return Ok(());
    }

    // TUI mode (default)
    info!("Starting TUI...");
    tui::app::run_tui(&cli).await?;

    Ok(())
}

/// Run a one-shot CLI command.
async fn run_cli_mode(cli: &Cli, prompt: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Find or launch Chrome
    let chrome_path = if let Some(path) = &cli.chrome_path {
        path.clone()
    } else {
        browser::chrome::detect_chrome()
            .ok_or("Chrome not found. Run 'glass install-chromium' or install Chrome.")?
    };

    // Check if Chrome is already running
    if !browser::chrome::check_chrome_health(cli.port).await {
        info!("Launching Chrome...");
        let profile_dir = if cli.incognito {
            None
        } else {
            let manager = browser::profile::ProfileManager::new();
            Some(manager.chrome_data_dir(&cli.profile))
        };
        browser::chrome::launch_chrome(&chrome_path, cli.port, profile_dir.as_deref()).await?;
    }

    // Connect via CDP
    let ws_url = browser::chrome::get_ws_url(cli.port).await?;
    let mut cdp = browser::cdp::CdpClient::connect(&ws_url).await?;

    // Enable domains
    cdp.enable_page().await?;
    cdp.enable_runtime().await?;
    cdp.enable_network().await?;

    // Parse the prompt and execute
    execute_prompt(&mut cdp, prompt).await?;

    cdp.close().await;
    Ok(())
}

/// Execute a natural language prompt by parsing simple commands.
async fn execute_prompt(
    cdp: &mut browser::cdp::CdpClient,
    prompt: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let prompt_lower = prompt.to_lowercase();

    if prompt_lower.starts_with("navigate to ") || prompt_lower.starts_with("go to ") {
        let url = if prompt_lower.starts_with("navigate to ") {
            &prompt[12..]
        } else {
            &prompt[6..]
        };
        let url = url.trim();
        // Add https:// if no scheme
        let url = if !url.starts_with("http://") && !url.starts_with("https://") {
            format!("https://{url}")
        } else {
            url.to_string()
        };
        println!("Navigating to: {url}");
        cdp.navigate(&url).await?;
        println!("Navigation complete.");
    } else if prompt_lower.starts_with("screenshot") {
        println!("Taking screenshot...");
        let data = cdp.screenshot("png").await?;
        // Save screenshot to file
        let decoded = base64_decode(&data)?;
        let path = "screenshot.png";
        std::fs::write(path, &decoded)?;
        println!("Screenshot saved to {path}");
    } else if prompt_lower.starts_with("get text") || prompt_lower.starts_with("get content") {
        println!("Fetching page content...");
        let result = cdp.evaluate("document.body.innerText").await?;
        if let Some(text) = result["result"]["value"].as_str() {
            println!("{text}");
        }
    } else if prompt_lower.starts_with("click ") {
        let selector = &prompt[6..].trim();
        println!("Clicking: {selector}");
        let node = cdp.query_selector(selector).await?;
        let node_id = node["nodeId"].as_i64().ok_or("Element not found")?;
        let box_model = cdp.get_box_model(node_id).await?;
        if let Some(content) = box_model["model"]["content"].as_array() {
            let x = content[0].as_f64().unwrap_or(0.0);
            let y = content[1].as_f64().unwrap_or(0.0);
            let w = content[2].as_f64().unwrap_or(0.0) - x;
            let h = content[5].as_f64().unwrap_or(0.0) - y;
            let click_x = x + w / 2.0;
            let click_y = y + h / 2.0;

            let mouse = browser::mouse::MouseEngine::new();
            let events = mouse.generate_click_events(browser::mouse::Point { x: click_x, y: click_y });
            for event in &events {
                cdp.dispatch_mouse_event(
                    &event.event_type,
                    event.x,
                    event.y,
                    Some(&event.button),
                    Some(event.click_count),
                ).await?;
            }
            println!("Clicked at ({click_x:.0}, {click_y:.0})");
        }
    } else if prompt_lower.starts_with("type ") || prompt_lower.starts_with("type in ") {
        let text = if prompt_lower.starts_with("type in ") {
            &prompt[8..]
        } else {
            &prompt[5..]
        };
        println!("Typing: {text}");
        for ch in text.chars() {
            cdp.dispatch_key_event("keyDown", &ch.to_string(), "").await?;
            cdp.dispatch_key_event("keyUp", &ch.to_string(), "").await?;
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    } else {
        // Default: try to evaluate as JavaScript
        println!("Executing: {prompt}");
        let result = cdp.evaluate(prompt).await?;
        println!("Result: {}", serde_json::to_string_pretty(&result)?);
    }

    Ok(())
}

/// Simple base64 decode for screenshots.
fn base64_decode(data: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // Use a simple approach: decode base64 manually
    use std::collections::HashMap;

    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut map: HashMap<u8, u8> = CHARS.iter().enumerate().map(|(i, &c)| (c, i as u8)).collect();
    map.insert(b'=', 0);

    let data = data.trim_end_matches('=');
    let mut result = Vec::with_capacity(data.len() * 3 / 4);

    for chunk in data.as_bytes().chunks(4) {
        let mut buf = [0u8; 4];
        for (i, &b) in chunk.iter().enumerate() {
            buf[i] = map.get(&b).copied().ok_or("Invalid base64 character")?;
        }

        let b0 = buf[0] as u32;
        let b1 = buf[1] as u32;
        let b2 = buf[2] as u32;

        result.push(((b0 << 2) | (b1 >> 4)) as u8);
        if chunk.len() > 2 {
            result.push(((b1 << 4) | (b2 >> 2)) as u8);
        }
        if chunk.len() > 3 {
            result.push(((b2 << 6) | (buf[3] as u32)) as u8);
        }
    }

    Ok(result)
}
