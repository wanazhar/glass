use super::args::{Cli, Commands, ProfileCommand};
use crate::browser::profile::ProfileManager;
use crate::browser::session::{BrowserResult, BrowserSession, SessionOptions};

pub async fn dispatch(cli: Cli) -> BrowserResult<()> {
    if cli.mcp {
        return crate::mcp::server::run_mcp_server(&cli).await;
    }

    match &cli.command {
        Some(Commands::InstallChromium) => {
            let path = crate::browser::chrome::download_chromium().await?;
            println!("Chrome for Testing installed at {}", path.display());
            return Ok(());
        }
        Some(Commands::Profiles { action }) => {
            dispatch_profiles(action.as_ref())?;
            return Ok(());
        }
        Some(Commands::DeleteProfile { name }) => {
            ProfileManager::new().delete_profile(name)?;
            println!("deleted profile {name}");
            return Ok(());
        }
        Some(Commands::Tui) | None if cli.prompt.is_none() => {
            return crate::tui::app::run_tui(&cli).await;
        }
        _ => {}
    }

    let options = SessionOptions {
        port: cli.port,
        chrome_path: cli.chrome_path.clone(),
        profile: cli.profile.clone(),
        incognito: cli.incognito,
        headed: cli.headed,
        interaction_mode: cli.interaction,
    };
    let session = BrowserSession::start(&options).await?;
    let result = if let Some(prompt) = &cli.prompt {
        run_prompt(&session, prompt).await
    } else if let Some(command) = &cli.command {
        run_command(&session, command).await
    } else {
        Ok(())
    };
    let close_result = session.close().await;
    result?;
    close_result
}

fn dispatch_profiles(action: Option<&ProfileCommand>) -> BrowserResult<()> {
    let manager = ProfileManager::new();
    match action {
        None | Some(ProfileCommand::List) => {
            let profiles = manager.list_profiles()?;
            if profiles.is_empty() {
                println!("no saved profiles");
            } else {
                for profile in profiles {
                    println!("{profile}");
                }
            }
        }
        Some(ProfileCommand::Create { name }) => {
            manager.create_profile(name)?;
            println!("created profile {name}");
        }
        Some(ProfileCommand::Delete { name }) => {
            manager.delete_profile(name)?;
            println!("deleted profile {name}");
        }
    }
    Ok(())
}

async fn run_command(session: &BrowserSession, command: &Commands) -> BrowserResult<()> {
    match command {
        Commands::Navigate { url } => {
            let page = session.navigate(url).await?;
            println!("navigated to {} — {}", page.title, page.url);
        }
        Commands::Click { target } => {
            println!("clicked {}", session.click(target).await?);
        }
        Commands::Type { text, target } => {
            session.type_text(text, target.as_deref()).await?;
            println!("typed {} characters", text.chars().count());
        }
        Commands::Screenshot { output } => {
            std::fs::write(output, session.screenshot_png().await?)?;
            println!("wrote {output}");
        }
        Commands::Text => println!("{}", session.text().await?),
        Commands::Dom => println!("{}", session.snapshot().await?.format()),
        Commands::Observe { screenshot } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&session.observe(*screenshot).await?)?
            );
        }
        Commands::Scroll { dx, dy } => {
            session.scroll(*dx, *dy).await?;
            println!("scrolled by ({dx}, {dy})");
        }
        Commands::Evaluate { expression } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&session.evaluate(expression).await?)?
            );
        }
        Commands::Tui
        | Commands::InstallChromium
        | Commands::Profiles { .. }
        | Commands::DeleteProfile { .. } => {
            unreachable!("handled before starting a browser session")
        }
    }
    Ok(())
}

async fn run_prompt(session: &BrowserSession, prompt: &str) -> BrowserResult<()> {
    let trimmed = prompt.trim();
    let lower = trimmed.to_lowercase();

    for prefix in ["navigate to ", "go to ", "open "] {
        if lower.starts_with(prefix) {
            let page = session.navigate(trimmed[prefix.len()..].trim()).await?;
            println!("navigated to {} — {}", page.title, page.url);
            return Ok(());
        }
    }
    if let Some(rest) = lower.strip_prefix("click ") {
        let target = &trimmed[trimmed.len() - rest.len()..];
        println!("clicked {}", session.click(target.trim_matches('"')).await?);
        return Ok(());
    }
    if let Some(rest) = lower.strip_prefix("type ") {
        let text = &trimmed[trimmed.len() - rest.len()..];
        session.type_text(text.trim_matches('"'), None).await?;
        println!("typed {} characters", text.chars().count());
        return Ok(());
    }
    if lower.starts_with("screenshot") {
        let output = trimmed
            .split_once(char::is_whitespace)
            .map(|(_, value)| value.trim())
            .filter(|value| !value.is_empty())
            .unwrap_or("screenshot.png");
        std::fs::write(output, session.screenshot_png().await?)?;
        println!("wrote {output}");
        return Ok(());
    }
    if matches!(
        lower.as_str(),
        "text" | "get text" | "page text" | "get content"
    ) {
        println!("{}", session.text().await?);
        return Ok(());
    }
    if matches!(lower.as_str(), "dom" | "snapshot" | "get dom") {
        println!("{}", session.snapshot().await?.format());
        return Ok(());
    }
    if matches!(lower.as_str(), "observe" | "context") {
        println!(
            "{}",
            serde_json::to_string_pretty(&session.observe(false).await?)?
        );
        return Ok(());
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&session.evaluate(trimmed).await?)?
    );
    Ok(())
}
