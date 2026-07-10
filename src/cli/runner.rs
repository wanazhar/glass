use super::args::{Cli, Commands, ProfileCmd};
use crate::browser::{
    cdp::CdpClient, chrome::ChromeManager, dom::DomEngine, mouse::MouseEngine, profile::ProfileManager,
};
use crate::mcp;
use crate::tui;
use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use tracing::info;

pub async fn dispatch(cli: Cli) -> Result<()> {
    if cli.mcp {
        return mcp::server::run(cli).await;
    }

    if let Some(Commands::InstallChromium) = &cli.command {
        let path = ChromeManager::install_chromium().await?;
        println!("Chromium installed at {}", path.display());
        return Ok(());
    }

    if let Some(Commands::Profiles { action }) = &cli.command {
        let pm = ProfileManager::new()?;
        match action {
            None | Some(ProfileCmd::List) => {
                for p in pm.list()? {
                    println!(
                        "{:<16} last_used={} created={}",
                        p.name, p.last_used, p.created_at
                    );
                }
            }
            Some(ProfileCmd::Create { name }) => {
                let dir = pm.create(name)?;
                println!("created profile {name} at {}", dir.display());
            }
            Some(ProfileCmd::Delete { name }) => {
                pm.delete(name)?;
                println!("deleted profile {name}");
            }
        }
        return Ok(());
    }

    // TUI: no prompt and no subcommand, or explicit tui
    let want_tui = matches!(cli.command, Some(Commands::Tui) | None) && cli.prompt.is_none();
    if want_tui {
        return tui::app::run(cli).await;
    }

    // One-shot CLI
    let mut session = Session::start(&cli).await?;
    if let Some(prompt) = &cli.prompt {
        run_prompt(&mut session, prompt).await?;
    } else if let Some(cmd) = &cli.command {
        run_command(&mut session, cmd).await?;
    }
    session.finish().await?;
    Ok(())
}

pub struct Session {
    pub chrome: ChromeManager,
    pub cdp: CdpClient,
    pub mouse: MouseEngine,
    pub profiles: ProfileManager,
}

impl Session {
    pub async fn start(cli: &Cli) -> Result<Self> {
        let mut profiles = ProfileManager::new()?;
        profiles.incognito = cli.incognito;
        if let Some(name) = &cli.profile {
            if !cli.incognito {
                profiles.select(name)?;
            }
        }

        let mut chrome = ChromeManager::new(cli.port);
        chrome.headless = !cli.headed;
        if let Some(path) = &cli.chrome {
            chrome.binary = Some(PathBuf::from(path));
        }
        chrome.user_data_dir = profiles.user_data_dir();
        chrome.ensure_running().await?;

        let cdp = CdpClient::connect(cli.port).await?;
        profiles.load_cookies(&cdp).await?;

        Ok(Self {
            chrome,
            cdp,
            mouse: MouseEngine::new(),
            profiles,
        })
    }

    pub async fn finish(&self) -> Result<()> {
        self.profiles.save_cookies(&self.cdp).await?;
        Ok(())
    }

    pub async fn navigate(&self, url: &str) -> Result<()> {
        let url = normalize_url(url);
        info!(%url, "navigate");
        self.cdp.navigate(&url).await
    }

    pub async fn click_target(&mut self, target: &str) -> Result<()> {
        let snap = DomEngine::snapshot(&self.cdp).await?;
        let el = DomEngine::find_by_ref(&snap, target)
            .ok_or_else(|| anyhow::anyhow!("element not found: {target}"))?;
        let (x, y) = DomEngine::center_of(el);
        info!(%target, x, y, role = %el.role, name = %el.name, "click");
        self.mouse.click(&self.cdp, x, y).await
    }

    pub async fn type_text(&mut self, text: &str, target: Option<&str>) -> Result<()> {
        if let Some(t) = target {
            self.click_target(t).await?;
        }
        MouseEngine::type_text(&self.cdp, text).await
    }

    pub async fn screenshot(&self, output: &str) -> Result<()> {
        let png = self.cdp.screenshot_png().await?;
        std::fs::write(output, png).with_context(|| format!("write {output}"))?;
        println!("wrote {output}");
        Ok(())
    }
}

async fn run_command(session: &mut Session, cmd: &Commands) -> Result<()> {
    match cmd {
        Commands::Navigate { url } => session.navigate(url).await?,
        Commands::Click { target } => session.click_target(target).await?,
        Commands::Type { text, target } => {
            session
                .type_text(text, target.as_deref())
                .await?
        }
        Commands::Screenshot { output } => session.screenshot(output).await?,
        Commands::Text => {
            let t = DomEngine::page_text(&session.cdp).await?;
            println!("{t}");
        }
        Commands::Dom => {
            let snap = DomEngine::snapshot(&session.cdp).await?;
            println!("url: {}\ntitle: {}\n", snap.url, snap.title);
            println!("{}", snap.ax_summary);
        }
        Commands::Scroll { dx, dy } => {
            session.cdp.scroll_by(*dx, *dy).await?;
        }
        Commands::Tui
        | Commands::InstallChromium
        | Commands::Profiles { .. } => unreachable!("handled earlier"),
    }
    Ok(())
}

/// Minimal command interpreter for free-form CLI prompts.
async fn run_prompt(session: &mut Session, prompt: &str) -> Result<()> {
    let p = prompt.trim();
    let lower = p.to_lowercase();

    // navigate …
    if let Some(rest) = lower
        .strip_prefix("navigate to ")
        .or_else(|| lower.strip_prefix("go to "))
        .or_else(|| lower.strip_prefix("open "))
    {
        // recover original casing URL from prompt
        let idx = p.len() - rest.len();
        let url = p[idx..].trim();
        session.navigate(url).await?;
        let snap = DomEngine::snapshot(&session.cdp).await?;
        println!("OK {} — {}", snap.title, snap.url);
        return Ok(());
    }

    if let Some(rest) = lower.strip_prefix("click ") {
        let idx = p.len() - rest.len();
        let target = p[idx..].trim().trim_matches('"');
        session.click_target(target).await?;
        println!("OK clicked {target}");
        return Ok(());
    }

    if let Some(rest) = lower.strip_prefix("type ") {
        let idx = p.len() - rest.len();
        let text = p[idx..].trim().trim_matches('"');
        session.type_text(text, None).await?;
        println!("OK typed");
        return Ok(());
    }

    if lower.starts_with("screenshot") {
        let out = p
            .split_whitespace()
            .nth(1)
            .unwrap_or("screenshot.png");
        session.screenshot(out).await?;
        return Ok(());
    }

    if lower == "text" || lower == "get text" || lower == "page text" {
        let t = DomEngine::page_text(&session.cdp).await?;
        println!("{t}");
        return Ok(());
    }

    if lower == "dom" || lower == "snapshot" {
        let snap = DomEngine::snapshot(&session.cdp).await?;
        println!("url: {}\ntitle: {}\n{}", snap.url, snap.title, snap.ax_summary);
        return Ok(());
    }

    // Multi-step: "navigate to X and …" — only first navigate for MVP
    if lower.contains("navigate to ") || lower.contains("go to ") {
        if let Some(start) = lower.find("navigate to ").or_else(|| lower.find("go to ")) {
            let marker_len = if lower[start..].starts_with("navigate to ") {
                "navigate to ".len()
            } else {
                "go to ".len()
            };
            let after = &p[start + marker_len..];
            let url = after
                .split_whitespace()
                .next()
                .unwrap_or("about:blank")
                .trim_matches(|c: char| c == '"' || c == '\'' || c == ',');
            session.navigate(url).await?;
            let snap = DomEngine::snapshot(&session.cdp).await?;
            println!("OK navigated to {} — {}", snap.url, snap.title);
            println!("{}", snap.ax_summary);
            return Ok(());
        }
    }

    bail!(
        "unrecognized prompt: {prompt:?}\n\
         Try: navigate to URL | click TARGET | type TEXT | screenshot [file] | text | dom"
    )
}

fn normalize_url(url: &str) -> String {
    let u = url.trim();
    if u.starts_with("http://") || u.starts_with("https://") || u.starts_with("about:") || u.starts_with("file:")
    {
        u.to_string()
    } else {
        format!("https://{u}")
    }
}
