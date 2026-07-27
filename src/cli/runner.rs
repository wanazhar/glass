//! CLI command dispatch and session orchestration.
//!
//! Routes parsed CLI arguments to the appropriate runner: one-shot browser
//! commands, interactive TUI, or the MCP stdio server.

use super::args::{CheckpointCommand, Cli, Commands, ProfileCommand};
use crate::browser::policy::{BrowserPolicy, PolicyCapability};
use crate::browser::profile::ProfileManager;
use crate::browser::session::{
    ActionKind, BatchStep, BrowserResult, BrowserSession, CheckpointV1, Cookie, Locator,
    PdfOptions, ReconciliationOptions, SessionOptions, VerificationPredicate, VisualCaptureOptions,
    WaitCondition,
};
use base64::Engine;
use serde::Serialize;
use std::io::Read;
use std::time::Duration;

/// Top-level command-line entry point: parses CLI arguments and dispatches
/// to the appropriate runner (one-shot, TUI, or MCP server).
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
        audit: cli.audit,
        policy: None,
    };
    let session = BrowserSession::start_with_policy(&options, policy).await?;
    let result = if let Some(prompt) = &cli.prompt {
        run_prompt(&session, prompt).await
    } else if let Some(command) = &cli.command {
        run_command(&session, command).await
    } else {
        Ok(())
    };
    if let Err(error) = &result
        && cli.trace_on_error
    {
        let trace = session
            .failure_trace_for(
                cli_trace_action(cli.command.as_ref(), cli.prompt.as_deref()),
                error.to_string(),
            )
            .await;
        eprintln!("{}", serde_json::to_string(&trace)?);
    }
    let close_result = session.close().await;
    result?;
    close_result
}

fn cli_trace_action(command: Option<&Commands>, prompt: Option<&str>) -> ActionKind {
    if let Some(prompt) = prompt {
        let lower = prompt.trim().to_ascii_lowercase();
        return if lower.starts_with("double click ") {
            ActionKind::DoubleClick
        } else if lower.starts_with("click ") {
            ActionKind::Click
        } else if lower.starts_with("type ") {
            ActionKind::Type
        } else {
            ActionKind::Click
        };
    }
    match command {
        Some(Commands::DoubleClick { .. }) => ActionKind::DoubleClick,
        Some(Commands::ClickExpectPopup { .. }) => ActionKind::ClickExpectPopup,
        Some(Commands::Click { .. })
        | Some(Commands::Preflight { .. })
        | Some(Commands::ClickAt { .. }) => ActionKind::Click,
        Some(Commands::Hover { .. }) => ActionKind::Hover,
        Some(Commands::Drag { .. }) => ActionKind::Drag,
        Some(Commands::Type { .. }) => ActionKind::Type,
        Some(Commands::Key { .. }) => ActionKind::KeyPress,
        Some(Commands::KeyDown { .. }) => ActionKind::KeyDown,
        Some(Commands::KeyUp { .. }) => ActionKind::KeyUp,
        Some(Commands::Shortcut { .. }) => ActionKind::Shortcut,
        Some(Commands::Clear { .. }) => ActionKind::Clear,
        Some(Commands::Check { .. }) => ActionKind::Check,
        Some(Commands::Uncheck { .. }) => ActionKind::Uncheck,
        Some(Commands::Select { .. }) => ActionKind::Select,
        Some(Commands::Upload { .. }) => ActionKind::Upload,
        Some(Commands::Scroll { .. }) => ActionKind::Scroll,
        _ => ActionKind::Click,
    }
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
        Commands::Navigate {
            url,
            timeout_ms,
            expected_revision,
        } => {
            if let Some(expected_revision) = expected_revision {
                print_json(
                    &session
                        .navigate_with_revision(
                            url,
                            Duration::from_millis(*timeout_ms),
                            *expected_revision,
                        )
                        .await?,
                )?;
            } else {
                let page = session
                    .navigate_with_deadline(url, Duration::from_millis(*timeout_ms))
                    .await?;
                print_json(&page)?;
            }
        }
        Commands::Click {
            target,
            expected_revision,
        } => {
            if let Some(expected_revision) = expected_revision {
                print_json(
                    &session
                        .click_with_revision(target, *expected_revision)
                        .await?,
                )?;
            } else {
                print_json(&session.click(target).await?)?;
            }
        }
        Commands::Preflight { target, action } => {
            print_json(&session.preflight_with_action(target, *action).await)?;
        }
        Commands::ClickAt { x, y } => {
            print_json(&session.click_at(*x, *y).await?)?;
        }
        Commands::ClickExpectPopup {
            target,
            expected_revision,
        } => {
            print_json(
                &session
                    .click_expect_popup_with_revision(target, *expected_revision)
                    .await?,
            )?;
        }
        Commands::DoubleClick {
            target,
            expected_revision,
        } => {
            print_json(
                &session
                    .double_click_with_revision(target, *expected_revision)
                    .await?,
            )?;
        }
        Commands::Hover { target } => print_json(&session.hover(target).await?)?,
        Commands::Drag {
            source,
            destination,
            expected_revision,
        } => {
            print_json(
                &session
                    .drag_with_revision(source, destination, *expected_revision)
                    .await?,
            )?;
        }
        Commands::Type {
            text,
            target,
            expected_revision,
        } => {
            print_json(
                &session
                    .type_text_with_expected_revision(text, target.as_deref(), *expected_revision)
                    .await?,
            )?;
        }
        Commands::Key {
            key,
            expected_revision,
        } => print_json(
            &session
                .key_press_with_revision(key, *expected_revision)
                .await?,
        )?,
        Commands::KeyDown {
            key,
            expected_revision,
        } => print_json(
            &session
                .key_down_with_revision(key, *expected_revision)
                .await?,
        )?,
        Commands::KeyUp {
            key,
            expected_revision,
        } => print_json(
            &session
                .key_up_with_revision(key, *expected_revision)
                .await?,
        )?,
        Commands::Shortcut {
            shortcut,
            expected_revision,
        } => print_json(
            &session
                .shortcut_with_revision(shortcut, *expected_revision)
                .await?,
        )?,
        Commands::Clear {
            target,
            expected_revision,
        } => print_json(
            &session
                .clear_with_revision(target, *expected_revision)
                .await?,
        )?,
        Commands::Check {
            target,
            expected_revision,
        } => print_json(
            &session
                .check_with_revision(target, *expected_revision)
                .await?,
        )?,
        Commands::Uncheck {
            target,
            expected_revision,
        } => print_json(
            &session
                .uncheck_with_revision(target, *expected_revision)
                .await?,
        )?,
        Commands::Select {
            target,
            value,
            expected_revision,
        } => {
            print_json(
                &session
                    .select_option_with_revision(target, value, *expected_revision)
                    .await?,
            )?;
        }
        Commands::Upload {
            target,
            files,
            expected_revision,
        } => {
            print_json(
                &session
                    .upload_files_with_revision(target, files, *expected_revision)
                    .await?,
            )?;
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
            form_values,
        } => {
            let context = match (*deep_dom, *screenshot, *form_values) {
                (false, false, false) => session.observe().await?,
                (true, false, false) => session.observe_with_dom().await?,
                (false, true, false) => session.observe_with_screenshot().await?,
                (true, true, false) => session.observe_with_dom_and_screenshot().await?,
                (false, false, true) => session.observe_with_form_values().await?,
                _ => return Err("form values can only be combined with compact observe".into()),
            };
            print_json(&context)?;
        }
        Commands::Scroll {
            dx,
            dy,
            expected_revision,
        } => {
            print_json(
                &session
                    .scroll_with_revision(*dx, *dy, *expected_revision)
                    .await?,
            )?;
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
        Commands::DismissConsent => print_json(&session.dismiss_consent().await?)?,
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
        Commands::Cookies => print_json(&session.cookies().await?)?,
        Commands::ExportCookies { output } => {
            let cookies = session.cookies().await?;
            let bytes = serde_json::to_vec_pretty(&cookies)?;
            tokio::fs::write(output, bytes).await?;
            println!("cookies exported to {}", output.display());
        }
        Commands::ImportCookies { input } => {
            const MAX_COOKIE_FILE_BYTES: u64 = 512 * 1024;
            let metadata = tokio::fs::metadata(input).await?;
            if metadata.len() > MAX_COOKIE_FILE_BYTES {
                return Err(format!(
                    "cookie file exceeds the {}-byte limit",
                    MAX_COOKIE_FILE_BYTES
                )
                .into());
            }
            let bytes = tokio::fs::read(input).await?;
            let cookies: Vec<Cookie> = serde_json::from_slice(&bytes)?;
            session.set_cookies(&cookies).await?;
            println!("{} cookies imported", cookies.len());
        }
        Commands::Pdf { output, background } => {
            let mut opts = PdfOptions::letter();
            if *background {
                opts.print_background = Some(true);
            }
            let data = session.print_to_pdf(&opts).await?;
            let bytes = base64::engine::general_purpose::STANDARD.decode(&data)?;
            tokio::fs::write(&output, &bytes).await?;
            println!("PDF saved to {output} ({} bytes)", bytes.len());
        }
        Commands::FillForm {
            fields,
            expected_revision,
        } => {
            let parsed: Vec<serde_json::Value> = serde_json::from_str(fields)?;
            let field_refs: Vec<(String, String)> = parsed
                .iter()
                .map(|v| {
                    (
                        v["target"].as_str().unwrap_or("").to_string(),
                        v["value"].as_str().unwrap_or("").to_string(),
                    )
                })
                .collect();
            let field_slices: Vec<(&str, &str)> = field_refs
                .iter()
                .map(|(t, v)| (t.as_str(), v.as_str()))
                .collect();
            print_json(
                &session
                    .fill_form_with_expected_revision(&field_slices, *expected_revision)
                    .await?,
            )?;
        }
        Commands::Batch {
            input,
            atomic,
            mode,
            expected_revision,
        } => {
            let payload = read_json_input(input.as_ref())?;
            let steps_value = payload.get("steps").cloned().unwrap_or(payload);
            let steps: Vec<BatchStep> = serde_json::from_value(steps_value)
                .map_err(|error| format!("invalid batch document: {error}"))?;
            print_json(
                &session
                    .run_batch_with_mode(&steps, *atomic, *mode, *expected_revision)
                    .await?,
            )?;
        }
        Commands::Verify {
            predicate,
            timeout_ms,
        } => {
            let predicate: VerificationPredicate = serde_json::from_str(predicate)
                .map_err(|error| format!("invalid verification predicate: {error}"))?;
            print_json(
                &session
                    .verify(predicate, Duration::from_millis(*timeout_ms))
                    .await?,
            )?;
        }
        Commands::ReconcileRefs {
            from_revision,
            hints,
            scope,
            refs,
        } => {
            let options = ReconciliationOptions {
                hints: hints
                    .iter()
                    .map(|hint| Locator::parse(hint))
                    .collect::<BrowserResult<Vec<_>>>()?,
                scope_ref: scope.clone(),
            };
            print_json(
                &session
                    .reconcile_references_with_options(*from_revision, refs, &options)
                    .await?,
            )?;
        }
        Commands::ObserveDelta => {
            print_json(&session.observe_delta().await?)?;
        }
        Commands::Checkpoint { action } => match action {
            CheckpointCommand::Export => print_json(&session.export_checkpoint().await?)?,
            CheckpointCommand::Import { input } => {
                let checkpoint: CheckpointV1 =
                    serde_json::from_value(read_json_input(input.as_ref())?)
                        .map_err(|error| format!("invalid checkpoint: {error}"))?;
                session.import_checkpoint(&checkpoint).await?;
                print_json(&serde_json::json!({"status": "checkpoint_imported"}))?;
            }
        },
        Commands::ClipboardRead => {
            let text = session.clipboard_read().await?;
            println!("{text}");
        }
        Commands::ClipboardWrite { text } => {
            session.clipboard_write(text).await?;
            println!("Text written to clipboard");
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

fn read_json_input(path: Option<&std::path::PathBuf>) -> BrowserResult<serde_json::Value> {
    let mut input = String::new();
    match path {
        Some(path) => std::fs::File::open(path)?.read_to_string(&mut input)?,
        None => std::io::stdin().read_to_string(&mut input)?,
    };
    Ok(serde_json::from_str(&input)?)
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
    let mut value = serde_json::to_value(value)?;
    let payload = serde_json::to_vec(&value)?;
    let payload_bytes = payload.len();
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "contextCost".to_string(),
            serde_json::json!({
                "payloadBytes": payload_bytes,
                "estimatedTokens": payload_bytes.div_ceil(4)
            }),
        );
    }
    Ok(serde_json::to_string(&value)?)
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

        let parsed = serde_json::from_str::<serde_json::Value>(&output).unwrap();
        assert!(!output.contains('\n'));
        assert_eq!(parsed["items"], json!([1, 2]));
        assert!(parsed["contextCost"]["payloadBytes"].as_u64().unwrap() > 0);
        assert!(parsed["contextCost"]["estimatedTokens"].as_u64().unwrap() > 0);
    }
}
