//! glass — a minimal, fast coding harness.
//!
//! Two tools, one loop, native terminal. Everything a mod can touch is in
//! `glass-mod-api`; everything the model can call is in `provider::builtin_tools`.

mod agent;
mod agents_md;
mod btw;
mod compact;
mod concurrency;
mod config;
mod ext;
mod input;
mod queue;
mod mods;
mod provider;
mod readguard;
mod session;
mod skills;
mod sse;
mod tools;
mod verify;

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
  /btw <q>    ask a side question without recording it
  /extensions loaded extensions, their tools and commands
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

    // Project instructions are pure prompt text and cost no code path, but they
    // are the one thing that can be large, so they are capped hard.
    let instructions = agents_md::load(&cfg.workspace);
    for i in &instructions {
        if !i.truncated {
            println!("\x1b[90m  reading {}\x1b[0m", i.path.display());
        }
    }
    let project_instructions = agents_md::render_all(&instructions);

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
    let mut extensions = extensions;
    let result = repl(cfg, provider, loaded_mods, &mut extensions, catalogue, project_instructions, session);
    // Extensions are child processes: they must be reaped on every exit path,
    // or they outlive the session holding the workspace open.
    extensions.shutdown();
    term::leave().ok();
    result
}

fn repl(
    cfg: config::Config,
    provider: Box<dyn provider::Provider>,
    loaded_mods: Vec<Box<dyn glass_mod_api::Mod_>>,
    extensions: &mut ext::Registry,
    catalogue: Option<String>,
    project_instructions: Option<String>,
    mut session: Session,
) -> Result<(), String> {
    let mut state = tools::State::new();
    let mut editor = Editor::new();
    let mut back = glass_tui::term::BackBuffer::new();

    print_banner(&cfg, &loaded_mods, extensions, &session);

    let keys = input::KeyReader::spawn();
    let mut pending = queue::Queue::new();

    let (mut w, h) = glass_tui::crossterm::terminal::size().unwrap_or((80, 24));
    let mut prompt_row = h.saturating_sub(2);

    loop {
        editor.render(prompt_row, w, true);

        let ev = match keys.try_next().or_else(|| keys.next_blocking()) {
            Some(e) => e,
            None => {
                term::leave().ok();
                return Err("input stream closed".into());
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

        // `/btw <question>` asks the model a side question against the current
        // context and prints the answer without recording either. The main
        // conversation is untouched, so the next turn costs what it would have
        // anyway.
        if let Some(question) = text.strip_prefix("/btw") {
            let question = question.trim();
            if question.is_empty() {
                println!("\x1b[33musage: /btw <question>\x1b[0m");
                continue;
            }
            print!("\x1b[90masking...\x1b[0m\r");
            let _ = std::io::stdout().flush();
            match btw::ask(
                provider.as_ref(),
                &cfg.system_prompt,
                &session.messages(),
                question,
                provider.id(),
            ) {
                Ok(answer) if answer.is_empty() => println!("\x1b[33mbtw: no answer\x1b[0m"),
                Ok(answer) => print!("{}", btw::render(question, &answer)),
                Err(e) => println!("\x1b[31mbtw failed: {e}\x1b[0m"),
            }
            continue;
        }

        if text == "/extensions" {
            if extensions.is_empty() {
                println!("no extensions loaded. add them under `extensions` in config.toml.");
            } else {
                for line in extensions.describe() {
                    println!("  {line}");
                }
                let cmds = extensions.commands();
                if !cmds.is_empty() {
                    println!("  commands:");
                    for (name, desc) in cmds {
                        println!("    /{name:<14} {desc}");
                    }
                }
            }
            continue;
        }

        // Anything else beginning with `/` may be an extension command. If no
        // extension claims it, say so rather than sending it to the model as a
        // prompt, where it would read as confused text.
        if let Some(rest) = text.strip_prefix('/') {
            let (cmd, args) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            if extensions.has_command(cmd) {
                match extensions.run_command(cmd, args.trim()) {
                    Some(out) => println!("{}", out.trim_end()),
                    None => println!("\x1b[33m/{cmd} did not return a result\x1b[0m"),
                }
                continue;
            }
            println!("\x1b[31munknown command /{cmd}\x1b[0m — try /help");
            continue;
        }

        session.append(Record::User(text.clone()));
        println!("\n\x1b[90m❯\x1b[0m {text}");

        // Dispatch this message, then anything the user queued while it ran.
        // The queue is drained here so a follow-up is a single Enter away from
        // being asked, which is the entire point of queuing it.
        let mut next = Some(text);
        while next.is_some() {
            let mut last_error: Option<String> = None;
            let outcome = {
                let mut agent = Agent::new(
                    provider.as_ref(),
                    &mut session,
                    &mut state,
                    &loaded_mods,
                    extensions,
                    &cfg.workspace,
                )
                .prime_extensions()
                .with_prompt(cfg.system_prompt.clone(), catalogue.clone(), project_instructions.clone())
                .with_limits(cfg.max_tokens, cfg.reasoning_budget, 24)
                .with_progress(report);

                println!("\x1b[38;5;245m");
                let mut draft = glass_tui::Editor::new();
                let (_, nh) = glass_tui::crossterm::terminal::size().unwrap_or((80, 24));
                let draft_row = nh.saturating_sub(2);
                let result = agent.run_turn(&mut |steer_slot: &mut Option<String>| {
                    if let Some(s) = drain_input(&keys, &mut draft, &mut pending, draft_row, 80)
                        && !s.is_empty()
                    {
                        *steer_slot = Some(s);
                    }
                });
                let _ = std::io::stdout().flush();
                println!("\x1b[0m");
                result
            };

            match outcome {
                Ok((out, stop)) => {
                    match stop {
                        Stop::ToolBudget => println!(
                            "\x1b[33m⚠ stopped: reached the tool-call budget for this turn\x1b[0m"
                        ),
                        Stop::Steered => println!("\x1b[33m⚡ stopped to apply your correction\x1b[0m"),
                        Stop::Finished => {}
                    }
                    for w in &out.warnings {
                        println!("\x1b[33m⚠ {w}\x1b[0m");
                    }
                    if let Some(u) = out.usage {
                        println!("{}", usage_line(&u));
                    }
                }
                Err(e) => {
                    println!("\x1b[31mprovider error: {e}\x1b[0m");
                    last_error = Some(e.to_string());
                }
            }

            if let Some(msg) = last_error {
                session.append(Record::System(format!("error: {msg}")));
                // A failed turn means the connection is probably gone; draining
                // the queue would just burn the same failure repeatedly.
                break;
            }

            next = pending.take_follow_up();
            if let Some(q) = &next {
                println!("\n\x1b[90m❯\x1b[0m {q}");
                session.append(Record::User(q.clone()));
            }
        }
        println!();

        // Scroll the prompt back to just under the last output line.
        let (nw, nh) = glass_tui::crossterm::terminal::size().unwrap_or((80, 24));
        let _ = nw;
        prompt_row = nh.saturating_sub(2);
        back.push(String::new());
    }
}

/// Consume key events while the agent is busy.
///
/// Mid-turn input needs its own line editor: the REPL's editor has already been
/// consumed by the turn that is running, and replaying into it would show the
/// user characters in a buffer they are no longer looking at. So the drain path
/// keeps its own editor and renders it below the streaming output.
///
/// Only the two submission intents are honoured. Navigation and editing keys go
/// to this editor, so a message can be typed and corrected normally while the
/// model works.
fn drain_input(
    keys: &input::KeyReader,
    editor: &mut glass_tui::Editor,
    pending: &mut queue::Queue,
    row: u16,
    width: u16,
) -> Option<String> {
    use glass_tui::crossterm::event::{Event, KeyCode, KeyModifiers};

    let mut dirty = false;

    while let Some(ev) = keys.try_next() {
        let Event::Key(k) = ev else { continue };

        if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c') {
            // Ctrl-C is an escape hatch: abandon the turn and drop the draft.
            editor.clear();
            println!("\r\x1b[33m⚡ interrupt\x1b[0m");
            return Some(String::new());
        }

        let alt = k.modifiers.contains(KeyModifiers::ALT);
        match editor.key(k) {
            glass_tui::Edit::Continue => dirty = true,
            glass_tui::Edit::Submit(text) => {
                let Some(intent) = queue::classify(&text, alt, true) else {
                    continue;
                };
                match intent {
                    queue::Intent::Steer(s) => {
                        println!("\r\x1b[33m⚡ steering: {s}\x1b[0m");
                        // Returned so the caller owns the slot; taking it here
                        // would borrow the same state the loop is holding.
                        return Some(s);
                    }
                    queue::Intent::FollowUp(s) => {
                        pending.push(queue::Intent::FollowUp(s));
                        println!("\r\x1b[90m· queued\x1b[0m");
                    }
                }
            }
        }
    }

    if dirty {
        // Repaint the draft line in place so a queued message is visible.
        editor.render(row, width, true);
        let _ = std::io::stdout().flush();
    }
    None
}

/// One line of token accounting, including how much of the prompt was cached.
///
/// Without this line a broken breakpoint layout is invisible: the request still
/// succeeds, it just costs 1.25x more per turn forever. The hit rate is the
/// only externally visible signal that the cache is doing anything.
fn usage_line(u: &provider::Usage) -> String {
    let total = u.prompt_total();
    let pct = (u.hit_rate() * 100.0).round() as u32;
    let cached = if u.cache_read > 0 || u.cache_write > 0 {
        format!(
            " · cache {pct}% ({} read / {} written)",
            thousands(u.cache_read),
            thousands(u.cache_write)
        )
    } else {
        // Zero in both buckets means nothing was cached at all: either the
        // prefix is under the model's minimum, or there is no breakpoint.
        " · \x1b[33mno cache\x1b[90m".to_string()
    };
    format!(
        "\x1b[90m  {total} in / {} out{cached}\x1b[0m",
        thousands(u.output)
    )
}

/// Group digits so long token counts stay readable at a glance.
fn thousands(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
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
        Progress::BlockedByGuard(paths) => {
            let _ = writeln!(io, "[31m⛔ refused to write unread file(s): {paths}[0m");
            let _ = io.flush();
        }
        Progress::Steered(text) => {
            let _ = writeln!(io, "\x1b[33m⚡ applying your correction: {text}\x1b[0m");
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

fn print_banner(
    cfg: &config::Config,
    loaded: &[Box<dyn glass_mod_api::Mod_>],
    exts: &ext::Registry,
    session: &Session,
) {
    let mods = if loaded.is_empty() {
        String::new()
    } else {
        format!(" · mod {}", loaded.iter().map(|m| m.name()).collect::<Vec<_>>().join(", "))
    };
    let extensions = if exts.is_empty() {
        String::new()
    } else {
        format!(" · ext {}", exts.names().join(", "))
    };
    println!(
        "\x1b[1mglass\x1b[0m \x1b[90m{} · {}{mods}{extensions}\x1b[0m",
        cfg.model, session.id
    );
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
