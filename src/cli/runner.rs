use super::args::{Cli, Commands, ProfileCommand};
use crate::browser::policy::{BrowserPolicy, PolicyCapability};
use crate::browser::profile::ProfileManager;
use crate::browser::session::{
    BrowserResult, BrowserSession, SessionOptions, VisualCaptureOptions, WaitCondition,
};
use serde::Serialize;
use std::time::Duration;

pub async fn dispatch(cli: Cli) -> BrowserResult<()> {
    let policy = policy_from_cli(&cli)?;
    if cli.mcp {
        return crate::mcp::server::run_mcp_server(&cli).await;
    }

    match &cli.command {
        Some(Commands::InstallChromium { update }) => {
            let path = crate::browser::chrome::download_chromium(*update).await?;
            println!("Chrome for Testing installed at {}", path.display());
            return Ok(());
        }
        Some(Commands::Profiles { action }) => {
            policy.require(PolicyCapability::PersistentProfile)?;
            dispatch_profiles(action.as_ref())?;
            return Ok(());
        }
        Some(Commands::DeleteProfile { name }) => {
            policy.require(PolicyCapability::PersistentProfile)?;
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
        attach: cli.attach,
        target_id: cli.target_id.clone(),
        frame_id: cli.frame_id.clone(),
        headed: cli.headed,
        interaction_mode: cli.interaction,
    };
    let session = BrowserSession::start_with_policy(&options, policy).await?;
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
        Commands::Navigate { url, timeout_ms } => {
            let page = session
                .navigate_with_deadline(url, Duration::from_millis(*timeout_ms))
                .await?;
            print_json(&page)?;
        }
        Commands::Click { target } => {
            print_json(&session.click(target).await?)?;
        }
        Commands::DoubleClick { target } => {
            print_json(&session.double_click(target).await?)?;
        }
        Commands::Hover { target } => print_json(&session.hover(target).await?)?,
        Commands::Drag {
            source,
            destination,
        } => {
            print_json(&session.drag(source, destination).await?)?;
        }
        Commands::Type { text, target } => {
            print_json(&session.type_text(text, target.as_deref()).await?)?;
        }
        Commands::Key { key } => print_json(&session.key_press(key).await?)?,
        Commands::KeyDown { key } => print_json(&session.key_down(key).await?)?,
        Commands::KeyUp { key } => print_json(&session.key_up(key).await?)?,
        Commands::Shortcut { shortcut } => print_json(&session.shortcut(shortcut).await?)?,
        Commands::Clear { target } => print_json(&session.clear(target).await?)?,
        Commands::Check { target } => print_json(&session.check(target).await?)?,
        Commands::Uncheck { target } => print_json(&session.uncheck(target).await?)?,
        Commands::Select { target, value } => {
            print_json(&session.select_option(target, value).await?)?;
        }
        Commands::Upload { target, files } => {
            print_json(&session.upload_files(target, files).await?)?;
        }
        Commands::Screenshot {
            output,
            format,
            quality,
            scale,
            full_page,
            clip,
            target,
        } => {
            let output = session
                .policy()
                .require_output_path(std::path::Path::new(output))?;
            let capture = session
                .capture_visual(&VisualCaptureOptions {
                    format: *format,
                    quality: *quality,
                    scale: *scale,
                    clip: *clip,
                    full_page: *full_page,
                    target: target.clone(),
                })
                .await?;
            let mut source = base64::read::DecoderReader::new(
                capture.data.as_bytes(),
                &base64::engine::general_purpose::STANDARD,
            );
            let mut file = std::fs::File::create(&output)?;
            std::io::copy(&mut source, &mut file)?;
            println!("wrote {}", output.display());
            print_json(&capture.metadata)?;
        }
        Commands::Text => println!("{}", session.text().await?),
        Commands::Dom => print_json(&session.deep_dom().await?)?,
        Commands::Observe {
            deep_dom,
            screenshot,
        } => {
            let context = match (*deep_dom, *screenshot) {
                (false, false) => session.observe().await?,
                (true, false) => session.observe_with_dom().await?,
                (false, true) => session.observe_with_screenshot().await?,
                (true, true) => session.observe_with_dom_and_screenshot().await?,
            };
            print_json(&context)?;
        }
        Commands::Scroll { dx, dy } => {
            print_json(&session.scroll(*dx, *dy).await?)?;
        }
        Commands::Wait {
            condition,
            timeout_ms,
        } => {
            print_json(
                &session
                    .wait(
                        WaitCondition::parse(condition)?,
                        Duration::from_millis(*timeout_ms),
                    )
                    .await?,
            )?;
        }
        Commands::Diagnostics { duration_ms } => print_json(
            &session
                .diagnostics(Duration::from_millis(*duration_ms))
                .await?,
        )?,
        Commands::AcceptDialog => {
            session.accept_dialog().await?;
            print_json(&serde_json::json!({"dialog": "accepted"}))?;
        }
        Commands::DismissDialog => {
            session.dismiss_dialog().await?;
            print_json(&serde_json::json!({"dialog": "dismissed"}))?;
        }
        Commands::Download {
            destination,
            timeout_ms,
        } => print_json(
            &session
                .wait_for_download(destination, Duration::from_millis(*timeout_ms))
                .await?,
        )?,
        Commands::Targets => print_json(&session.list_targets().await?)?,
        Commands::NewTarget { url } => print_json(&session.create_target(url).await?)?,
        Commands::SelectTarget { id } => print_json(&session.select_target(id).await?)?,
        Commands::CloseTarget { id } => {
            session.close_target(id).await?;
            print_json(&serde_json::json!({"closed": id}))?;
        }
        Commands::Frames => print_json(&session.list_frames().await?)?,
        Commands::SelectFrame { id } => print_json(&session.select_frame(id).await?)?,
        Commands::Evaluate { expression } => {
            print_json(&session.evaluate(expression).await?)?;
        }
        Commands::Tui
        | Commands::InstallChromium { .. }
        | Commands::Profiles { .. }
        | Commands::DeleteProfile { .. } => {
            unreachable!("handled before starting a browser session")
        }
    }
    Ok(())
}

pub(crate) fn policy_from_cli(cli: &Cli) -> BrowserResult<BrowserPolicy> {
    Ok(BrowserPolicy::new(
        cli.policy,
        std::env::current_dir()?,
        cli.policy_allow.iter().copied(),
        cli.policy_confirm.iter().copied(),
    )?
    .with_host_rules(
        cli.policy_allow_host.iter().cloned(),
        cli.policy_deny_host.iter().cloned(),
    )?
    .with_confirmation_tokens(cli.policy_confirm_once.iter().copied())?)
}

async fn run_prompt(session: &BrowserSession, prompt: &str) -> BrowserResult<()> {
    let trimmed = prompt.trim();
    let lower = trimmed.to_lowercase();

    for prefix in ["navigate to ", "go to ", "open "] {
        if lower.starts_with(prefix) {
            let page = session.navigate(trimmed[prefix.len()..].trim()).await?;
            print_json(&page)?;
            return Ok(());
        }
    }
    if let Some(rest) = lower.strip_prefix("click ") {
        let target = &trimmed[trimmed.len() - rest.len()..];
        print_json(&session.click(target.trim_matches('"')).await?)?;
        return Ok(());
    }
    if let Some(rest) = lower.strip_prefix("double click ") {
        let target = &trimmed[trimmed.len() - rest.len()..];
        print_json(&session.double_click(target.trim_matches('"')).await?)?;
        return Ok(());
    }
    if let Some(rest) = lower.strip_prefix("type ") {
        let text = &trimmed[trimmed.len() - rest.len()..];
        print_json(&session.type_text(text.trim_matches('"'), None).await?)?;
        return Ok(());
    }
    if lower.starts_with("screenshot") {
        let output = trimmed
            .split_once(char::is_whitespace)
            .map(|(_, value)| value.trim())
            .filter(|value| !value.is_empty())
            .unwrap_or("screenshot.png");
        let output = session
            .policy()
            .require_output_path(std::path::Path::new(output))?;
        std::fs::write(&output, session.screenshot_png().await?)?;
        println!("wrote {}", output.display());
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
        print_json(&session.deep_dom().await?)?;
        return Ok(());
    }
    if matches!(lower.as_str(), "observe" | "context") {
        print_json(&session.observe().await?)?;
        return Ok(());
    }

    print_json(&session.evaluate(trimmed).await?)?;
    Ok(())
}

fn print_json<T: Serialize + ?Sized>(value: &T) -> BrowserResult<()> {
    println!("{}", compact_json(value)?);
    Ok(())
}

fn compact_json<T: Serialize + ?Sized>(value: &T) -> BrowserResult<String> {
    Ok(serde_json::to_string(value)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn structured_cli_output_is_compact_json() {
        let output = compact_json(&json!({
            "page": {"title": "Glass", "url": "https://example.com"},
            "items": [1, 2]
        }))
        .unwrap();

        assert!(!output.contains('\n'));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&output).unwrap()["items"],
            json!([1, 2])
        );
    }
}
