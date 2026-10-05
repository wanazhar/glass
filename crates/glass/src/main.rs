//! glass — a minimal, fast coding harness.
//!
//! Two tools, one loop, native terminal. Everything a mod can touch is in
//! `glass-mod-api`; everything the model can call is in `provider::builtin_tools`.

mod agent;
mod compact;
mod config;
mod ext;
mod mods;
mod provider;
mod session;
mod skills;
mod sse;
mod tools;

use std::io::Write;

use glass_tui::{Edit, Editor, term};

use crate::agent::{Agent, Progress, Stop};
use crate::session::{Record, Session};

const HELP: &str = "\
glass — a minimal coding harness

  glass                 start in the current directory
  glass resume          reopen the most recent session
  glass resume <id>     reopen a specific session
  glass list            list saved sessions

In the prompt:
  ctrl-g      fullscreen transcript (scroll, collapse, follow)
  ctrl-c      interrupt the current turn
  ctrl-d      quit
  up/down     prompt history
  /help       show this text

glass ships no permission system: bash runs what the model asks for. Run it
somewhere you are willing to lose.
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("--help") | Some("-h") | Some("help") => {
            print!("{HELP}");
            return;
        }
        Some("--version") | Some("-V") => {
            println!("glass {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        Some("list") => {
            list_sessions();
            return;
        }
        _ => {}
    }

    if let Err(e) = run(&args) {
        eprintln!("glass: {e}");
        std::process::exit(1);
    }
}

fn list_sessions() {
    let dir = config::data_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        println!("no sessions in {}", dir.display());
        return;
    };
    let mut rows: Vec<(String, u64)> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
        .filter_map(|e| {
            let stem = e.path().file_stem()?.to_string_lossy().into_owned();
            let bytes = e.metadata().ok()?.len();
            Some((stem, bytes))
        })
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));

    if rows.is_empty() {
        println!("no sessions in {}", dir.display());
        return;
    }
    println!("{:<28} {:>10}", "session", "bytes");
    for (id, bytes) in rows {
        println!("{id:<28} {bytes:>10}");
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let cfg_path = config::default_path();
    let cfg = config::load(&cfg_path).map_err(|e| e.to_string())?;

    let provider = provider::build(&cfg.provider, &cfg.base_url, cfg.api_key(), &cfg.model)
        .map_err(|e| e.to_string())?;

    let loaded_mods = mods::load(&cfg.mods)?;

    // Extensions are opt-in: nothing is spawned unless config names it, so a
    // default install pays nothing for the mechanism.
    let extensions = if cfg.extensions.is_empty() {
        ext::Registry::empty()
    } else {
        ext::Registry::load(&cfg.extensions, &cfg.workspace, &cfg.model)
    };

    let skills = cfg.skills_dir.as_deref().map(skills::load_dir).unwrap_or_default();
    let catalogue = skills::catalogue(&skills);

    // Resume or create the session before touching the terminal so a bad path
    // produces a clean error instead of a mangled screen.
    let dir = config::data_dir();
    let resume = args.first().map(String::as_str).filter(|a| *a != "resume");
    let session = match args.first().map(String::as_str) {
        Some("resume") => match resume {
            Some(id) => Session::open(&dir, id).map_err(|e| format!("cannot resume {id}: {e}"))?,
            None => match Session::last(&dir) {
                Some(p) => {
                    let id = p.file_stem().unwrap_or_default().to_string_lossy().into_owned();
                    Session::open(&dir, &id).map_err(|e| format!("cannot resume: {e}"))?
                }
                None => Session::create(&dir).map_err(|e| e.to_string())?,
            },
        },
        _ => Session::create(&dir).map_err(|e| e.to_string())?,
    };

    if !term::stdin_is_tty() {
        return Err("glass needs a terminal. pipe input is not supported.".into());
    }

    term::enter().map_err(|e| format!("cannot set up terminal: {e}"))?;
    let result = repl(cfg, provider, loaded_mods, extensions, catalogue, session);
    term::leave().ok();
    result
}

fn repl(
    cfg: config::Config,
    provider: Box<dyn provider::Provider>,
    loaded_mods: Vec<Box<dyn glass_mod_api::Mod_>>,
    mut extensions: ext::Registry,
    catalogue: Option<String>,
    mut session: Session,
) -> Result<(), String> {
    let mut state = tools::State::new();
    let mut editor = Editor::new();
    let mut back = glass_tui::term::BackBuffer::new();

    print_banner(&cfg, &loaded_mods, &session);

    let (mut w, h) = glass_tui::crossterm::terminal::size().unwrap_or((80, 24));
    let mut prompt_row = h.saturating_sub(2);

    loop {
        editor.render(prompt_row, w, true);

        let ev = match glass_tui::crossterm::event::read() {
            Ok(e) => e,
            Err(e) => {
                term::leave().ok();
                return Err(format!("input error: {e}"));
            }
        };

        // Global bindings are handled here and never reach the editor, so a
        // ctrl-key can never leak into the prompt as a literal character.
        let key = match ev {
            glass_tui::crossterm::event::Event::Key(k) => k,
            glass_tui::crossterm::event::Event::Resize(nw, nh) => {
                w = nw;
                prompt_row = nh.saturating_sub(2);
                continue;
            }
            _ => continue,
        };

        let ctrl = key.modifiers.contains(glass_tui::crossterm::event::KeyModifiers::CONTROL);
        if ctrl {
            match key.code {
                glass_tui::crossterm::event::KeyCode::Char('d') => {
                    println!("\nbye");
                    return Ok(());
                }
                glass_tui::crossterm::event::KeyCode::Char('g') => {
                    view_transcript(&session, &loaded_mods)?;
                    continue;
                }
                glass_tui::crossterm::event::KeyCode::Char('c') => {
                    editor.clear();
                    println!("^C");
                    continue;
                }
                _ => {}
            }
        }

        let text = match editor.key(key) {
            Edit::Continue => continue,
            Edit::Submit(t) => t,
        };

        let text = text.trim().to_string();
        if text.is_empty() {
            continue;
        }
        if text == "/help" {
            print!("{HELP}");
            continue;
        }

        session.append(Record::User(text.clone()));
        println!("\n\x1b[90m❯\x1b[0m {text}");

        let mut last_error: Option<String> = None;
        let outcome = {
            let mut agent = Agent::new(
                provider.as_ref(),
                &mut session,
                &mut state,
                &loaded_mods,
                &mut extensions,
                &cfg.workspace,
            )
            .prime_extensions()
            .with_prompt(cfg.system_prompt.clone(), catalogue.clone())
            .with_limits(cfg.max_tokens, cfg.reasoning_budget, 24)
            .with_progress(report);

            println!("\x1b[38;5;245m");
            let result = agent.run_turn();
            let _ = std::io::stdout().flush();
            println!("\x1b[0m");
            result
        };

        match outcome {
            Ok((out, stop)) => {
                if let Stop::ToolBudget = stop {
                    println!("\x1b[33m⚠ stopped: reached the tool-call budget for this turn\x1b[0m");
                }
                for w in &out.warnings {
                    println!("\x1b[33m⚠ {w}\x1b[0m");
                }
                if let Some((i, o)) = out.usage {
                    println!("\x1b[90m  {i} in / {o} out\x1b[0m");
                }
            }
            Err(e) => {
                println!("\x1b[31mprovider error: {e}\x1b[0m");
                // `agent` still borrows the session, so the error is recorded
                // after it goes out of scope below.
                last_error = Some(e.to_string());
            }
        }
        if let Some(msg) = last_error {
            session.append(Record::System(format!("error: {msg}")));
        }
        println!();

        // Scroll the prompt back to just under the last output line.
        let (nw, nh) = glass_tui::crossterm::terminal::size().unwrap_or((80, 24));
        let _ = nw;
        prompt_row = nh.saturating_sub(2);
        back.push(String::new());
    }
}

/// Paint one progress event.
///
/// Streaming text is written bare so the turn reads as prose; tool activity is
/// prefixed and dimmed so it is visibly not prose. Everything here is written
/// into the terminal's own scrollback, which is why this needs no viewport
/// bookkeeping at all.
fn report(p: Progress<'_>) {
    let mut io = std::io::stdout();
    match p {
        Progress::Text(t) => {
            let _ = write!(io, "{t}");
            let _ = io.flush();
        }
        Progress::ToolStart { name, command } => {
            // A newline only when text is mid-line, so a turn that starts with
            // a tool call does not open with a blank row.
            let _ = write!(io, "\n\x1b[38;5;179m◆ {name}\x1b[0m ");
            if let Some(c) = command {
                let _ = write!(io, "\x1b[90m{}\x1b[0m", first_line(c, 120));
            }
            let _ = io.flush();
        }
        Progress::ToolEnd { name, summary } => {
            let _ = writeln!(io, "\x1b[90m  {} → {}\x1b[0m", name, summary);
            let _ = io.flush();
        }
        Progress::Compacted { kept, dropped } => {
            let _ = writeln!(
                io,
                "\x1b[38;5;179m◆ context compacted\x1b[0m \x1b[90mdropped {dropped}, kept {kept}\x1b[0m"
            );
            let _ = io.flush();
        }
    }
}

fn first_line(s: &str, max: usize) -> String {
    let line = s.lines().next().unwrap_or("").trim();
    let mut out: String = line.chars().take(max).collect();
    if line.chars().count() > max {
        out.push('…');
    }
    out
}

fn print_banner(cfg: &config::Config, loaded: &[Box<dyn glass_mod_api::Mod_>], session: &Session) {
    let mods = if loaded.is_empty() {
        String::new()
    } else {
        format!(" · {}", loaded.iter().map(|m| m.name()).collect::<Vec<_>>().join(", "))
    };
    println!("\x1b[1mglass\x1b[0m \x1b[90m{} · {}{mods}\x1b[0m", cfg.model, session.id);
    println!("\x1b[90m  bash + state · ctrl-g transcript · ctrl-d quit\x1b[0m");
    println!("\x1b[90m  no permission gate: commands run as asked\x1b[0m\n");
}

/// Fullscreen transcript viewer.
fn view_transcript(session: &Session, loaded: &[Box<dyn glass_mod_api::Mod_>]) -> Result<(), String> {
    use glass_tui::crossterm::execute;
    use glass_tui::ratatui::Terminal;

    let mut transcript = glass_tui::Transcript::new(session.entries());
    let mut tui = Terminal::new(glass_tui::ratatui::backend::CrosstermBackend::new(std::io::stdout()))
        .map_err(|e| format!("cannot start viewer: {e}"))?;

    // Alternate screen only here. The default view never touches it, which is
    // what keeps history in the terminal instead of the heap.
    execute!(std::io::stdout(), glass_tui::crossterm::terminal::EnterAlternateScreen, glass_tui::crossterm::cursor::Hide)
        .map_err(|e| e.to_string())?;
    tui.hide_cursor().ok();

    let res = (|| -> Result<(), String> {
        use glass_tui::crossterm::event::{Event, KeyCode, KeyModifiers};
        loop {
            // Viewport height is needed to clamp paging against the real
            // window, which can change between frames.
            let height = tui.size().map(|r| r.height).unwrap_or(24);
            tui.draw(|f| transcript.draw(f, loaded, &session.id))
                .map_err(|e| e.to_string())?;

            match glass_tui::crossterm::event::read().map_err(|e| e.to_string())? {
                Event::Key(k) => {
                    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
                    match k.code {
                        KeyCode::Esc => break,
                        KeyCode::Char('q') => break,
                        KeyCode::Char('c') if ctrl => break,
                        KeyCode::Char('g') if ctrl => break,
                        KeyCode::Up => transcript.step(-1, height),
                        KeyCode::Down => transcript.step(1, height),
                        KeyCode::PageUp => transcript.page(-1, height),
                        KeyCode::PageDown => transcript.page(1, height),
                        KeyCode::Home => transcript.top(),
                        KeyCode::End => transcript.follow_tail(height),
                        KeyCode::Enter => transcript.toggle_collapse(),
                        KeyCode::Char('f') => transcript.follow_tail(height),
                        KeyCode::Char('t') => transcript.jump_to_turn(1, height),
                        _ => {}
                    }
                }
                Event::Resize(_, _) => tui.autoresize().map_err(|e| e.to_string())?,
                _ => {}
            }
        }
        Ok(())
    })();

    tui.show_cursor().ok();
    execute!(std::io::stdout(), glass_tui::crossterm::cursor::Show, glass_tui::crossterm::terminal::LeaveAlternateScreen)
        .map_err(|e| e.to_string())?;
    println!();
    res
}
