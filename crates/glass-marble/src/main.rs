use clap::{Parser, Subcommand};
use glass_marble::{
    AccessPolicy, ApprovalInbox, CancelRegistry, ControlProfile, Conversation, DataDirLock,
    EventBus, HostPolicy, InboundMessage, LaneFactory, LiveStore, MarbleError, MarbleServer,
    MarbleServerParts, MarbleTurn, PrismPool, ServiceSpec, StateStore, TICK, ToolRouter,
    TurnOptions, default_data_dir, run_turn, sessions_dir, spawn_scheduler, spawn_turn_pool,
    to_turn_input,
};
use glass_marble::{notify, service};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

/// How many chats can be worked on at the same time. Each lane is an engine
/// process, so the number is a product choice rather than a pool size: three
/// chats in parallel covers a person working a question, a fix, and a review
/// without letting one host spawn engines without bound.
const CHAT_LANES: usize = 3;

#[derive(Debug, Parser)]
#[command(
    name = "glass-marble",
    about = "Always-on, self-hosted personal assistant built on Glass Core",
    version = glass_marble::build_info::CLI_VERSION
)]
struct Cli {
    /// One-shot message. Omit when using a subcommand.
    #[arg(value_name = "MESSAGE")]
    message: Option<String>,
    #[arg(long, value_name = "CONVERSATION_ID")]
    conversation: Option<String>,
    /// Workspace the collaborator works in.
    #[arg(long, value_name = "DIR", default_value = ".")]
    cwd: PathBuf,
    /// Product data directory; sessions and state live here.
    #[arg(long, value_name = "DIR")]
    data_dir: Option<PathBuf>,
    #[arg(long, value_name = "MODEL")]
    model: Option<String>,
    /// Approve every governed host tool without prompting (one-shot mode).
    #[arg(long)]
    approve: bool,
    #[arg(long, value_name = "PATH")]
    core_binary: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Serve the self-hosted product UI and JSON API.
    Serve {
        #[arg(long, default_value_t = 4783)]
        port: u16,
        /// Bind address. Use the machine's Tailscale address to share it.
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// File containing the access token; required off loopback.
        #[arg(long, value_name = "PATH")]
        token_file: Option<PathBuf>,
        /// Per-turn wall clock budget in seconds.
        #[arg(long, default_value_t = 900)]
        timeout_secs: u64,
        /// Marble's control posture: observe, workspace, or unattended.
        #[arg(long, default_value = "workspace")]
        profile: String,
        /// How long a governed tool waits for a human decision before failing closed.
        #[arg(long, default_value_t = 600)]
        approval_wait_secs: u64,
        /// PEM certificate chain; with --tls-key the port also serves HTTPS.
        #[arg(long, value_name = "PATH", requires = "tls_key")]
        tls_cert: Option<PathBuf>,
        /// PEM private key for --tls-cert.
        #[arg(long, value_name = "PATH", requires = "tls_cert")]
        tls_key: Option<PathBuf>,
    },
    /// Provision a TLS certificate for `serve --tls-cert` — a real Tailscale
    /// cert when `tailscale cert` can make one, else a self-signed fallback.
    /// Tailscale certs expire in ~90 days: rerun and restart to renew.
    TlsCert {
        /// Where cert.pem and key.pem land. Defaults to <data-dir>/tls.
        #[arg(long, value_name = "DIR")]
        dir: Option<PathBuf>,
    },
    /// Check this host's data, engine, and configuration.
    Doctor,
    /// Restore the product state from a backup or an export file.
    /// Stop the running service first.
    Restore {
        /// Backup or `/api/export` file. Defaults to the newest daily backup.
        #[arg(long, value_name = "PATH")]
        from: Option<PathBuf>,
    },
    /// Install a login service so the assistant keeps running after logout.
    InstallService {
        #[arg(long, default_value_t = 4783)]
        port: u16,
        /// Address the service binds. Use the Tailscale address to share it.
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value = "workspace")]
        profile: String,
        #[arg(long, value_name = "PATH")]
        token_file: Option<PathBuf>,
        /// PEM certificate chain the service serves HTTPS with.
        #[arg(long, value_name = "PATH", requires = "tls_key")]
        tls_cert: Option<PathBuf>,
        /// PEM private key for --tls-cert.
        #[arg(long, value_name = "PATH", requires = "tls_cert")]
        tls_key: Option<PathBuf>,
        /// Write the unit file instead of printing it for review.
        #[arg(long)]
        write: bool,
    },
}

fn main() {
    init_tracing();
    if let Err(error) = run() {
        eprintln!("glass-marble: {error}");
        std::process::exit(1);
    }
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_env("GLASS_MARBLE_LOG")
        .or_else(|_| tracing_subscriber::EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("glass_marble=info,warn"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();
}

fn core_binary(cli: &Cli) -> PathBuf {
    cli.core_binary
        .clone()
        .or_else(|| std::env::var_os("GLASS_CORE_BINARY").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("glass-core"))
}

fn runtime_command(cli: &Cli) -> Command {
    let mut command = Command::new(core_binary(cli));
    command.arg("runtime");
    command
}

/// The recipe the served product uses to replace a dead engine.
fn runtime_factory(cli: &Cli) -> impl Fn() -> Command + Send + 'static {
    let binary = core_binary(cli);
    move || {
        let mut command = Command::new(&binary);
        command.arg("runtime");
        command
    }
}

fn run() -> Result<(), MarbleError> {
    let mut cli = Cli::parse();
    let data_dir = cli.data_dir.clone().unwrap_or_else(default_data_dir);
    std::fs::create_dir_all(&data_dir).map_err(MarbleError::Io)?;
    let cwd = cli.cwd.canonicalize().map_err(MarbleError::Io)?;
    if let Some(command) = cli.command.take() {
        return match command {
            Commands::Serve {
                port,
                host,
                token_file,
                timeout_secs,
                profile,
                approval_wait_secs,
                tls_cert,
                tls_key,
            } => serve(
                &cli,
                ServeOptions {
                    cwd: &cwd,
                    data_dir: &data_dir,
                    port,
                    host: &host,
                    token_file,
                    timeout_secs,
                    approval_wait_secs,
                    tls_cert,
                    tls_key,
                    profile: ControlProfile::parse(&profile).ok_or_else(|| {
                        MarbleError::Protocol(format!(
                            "unknown control profile {profile}: use observe, workspace, or unattended"
                        ))
                    })?,
                },
            ),
            Commands::TlsCert { dir } => {
                let dir = dir.unwrap_or_else(|| data_dir.join("tls"));
                let made = glass_marble::tls::make_identity(&dir)
                    .map_err(MarbleError::Protocol)?;
                glass_marble::tls::describe(&made, &mut std::io::stdout())
                    .map_err(MarbleError::Io)?;
                Ok(())
            }
            Commands::Doctor => doctor(&cli, &cwd, &data_dir),
            Commands::Restore { from } => restore(&data_dir, from.as_deref()),
            Commands::InstallService {
                port,
                host,
                profile,
                token_file,
                tls_cert,
                tls_key,
                write,
            } => install_service(
                &cli,
                ServiceOptions {
                    cwd: &cwd,
                    data_dir: &data_dir,
                    port,
                    host: &host,
                    profile: &profile,
                    token_file,
                    tls_cert,
                    tls_key,
                    write,
                },
            ),
        };
    }
    let Some(message) = cli.message.clone() else {
        return Err(MarbleError::Protocol(
            "provide a message or run `glass-marble serve`".into(),
        ));
    };
    one_shot(&cli, &cwd, &data_dir, &message)
}

fn restore(data_dir: &Path, from: Option<&Path>) -> Result<(), MarbleError> {
    // The same lock serving takes, so a restore cannot overwrite the document
    // underneath an instance that still holds the old revision in memory.
    let _lock = DataDirLock::acquire(data_dir)?;
    let store = StateStore::in_data_dir(data_dir);
    let (state, source) = store.restore(from)?;
    println!(
        "restored {} into {}",
        source.display(),
        store.path().display()
    );
    println!(
        "{} conversation(s), {} task(s), {} transcript entr{}, revision {}",
        state.conversations.len(),
        state.tasks.len(),
        state.transcript.len(),
        if state.transcript.len() == 1 {
            "y"
        } else {
            "ies"
        },
        state.revision
    );
    println!("start the service again to serve the restored state");
    Ok(())
}

fn one_shot(cli: &Cli, cwd: &Path, data_dir: &Path, message: &str) -> Result<(), MarbleError> {
    let host = if cli.approve {
        HostPolicy::approving()
    } else {
        HostPolicy::denying()
    };
    let approvals = ApprovalInbox::default();
    let router = ToolRouter::new(cwd)?;
    let mut conversation = Conversation::spawn_with_router(
        runtime_command(cli),
        cwd,
        &sessions_dir(data_dir),
        host,
        approvals,
        router,
        EventBus::default(),
    )?
    .with_model(cli.model.clone());
    let turn = MarbleTurn {
        device_time_zone: None,
        conversation_id: cli.conversation.clone(),
        request_id: glass_marble::next_request_id(),
        input: to_turn_input(&InboundMessage::new(message)),
    };
    let reply = run_turn(&turn, &mut conversation, TurnOptions::default());
    let _ = conversation.shutdown();
    let reply = reply?;
    println!("{}", reply.text);
    Ok(())
}

/// Inputs for the served product.
struct ServeOptions<'a> {
    cwd: &'a Path,
    data_dir: &'a Path,
    port: u16,
    host: &'a str,
    token_file: Option<PathBuf>,
    timeout_secs: u64,
    approval_wait_secs: u64,
    tls_cert: Option<PathBuf>,
    tls_key: Option<PathBuf>,
    profile: ControlProfile,
}

fn serve(cli: &Cli, options: ServeOptions<'_>) -> Result<(), MarbleError> {
    let ServeOptions {
        cwd,
        data_dir,
        port,
        host,
        token_file,
        timeout_secs,
        approval_wait_secs,
        tls_cert,
        tls_key,
        profile,
    } = options;
    let token = read_token(token_file.as_deref())?;
    if !is_loopback(host) && token.is_none() {
        return Err(MarbleError::Protocol(
            "refusing to serve off loopback without a token: pass --token-file or set \
             GLASS_MARBLE_TOKEN"
                .into(),
        ));
    }
    // One instance per data directory: two servers would each write the whole
    // document from their own memory and silently overwrite each other's turns.
    let lock = DataDirLock::acquire(data_dir)?;
    tracing::info!(lock = %lock.path().display(), pid = lock.pid(), "holding the data directory");
    let store = Arc::new(StateStore::in_data_dir(data_dir));
    // An unreadable document is refused here, not replaced: load() restores the
    // newest backup when it can, and otherwise leaves the file for the operator.
    let mut loaded = store.load()?;
    store.retry_everyone_cleanup(&mut loaded);
    loaded.control.profile = profile;
    if let Some(model) = cli.model.clone() {
        // An explicit flag wins; otherwise a model chosen from a surface stays.
        loaded.model = model;
    }
    // An always-on assistant has to be honest about what a restart cost it: a
    // turn that was running when the process stopped is marked interrupted, not
    // left looking like it is still working.
    loaded.presence.mark_start();
    // Turns that were running when the previous process stopped left a live
    // journal entry. Fold those in first: the partial reply belongs in the
    // conversation, and recovering it marks its task interrupted, so the generic
    // pass below does not explain the same turn a second time.
    let live = LiveStore::in_data_dir(data_dir);
    let live_recovery = live.recover(&mut loaded);
    if !live_recovery.is_empty() {
        tracing::warn!(
            turns = live_recovery.turns,
            partials = live_recovery.partials,
            unreadable = live_recovery.unreadable,
            "recovered live turns from the previous run"
        );
    }
    let recovered = loaded.recover_interrupted();
    if recovered > 0 {
        tracing::warn!(recovered, "marked interrupted turns from the previous run");
    }
    // Tasks recorded for conversations that no longer exist are orphans —
    // nothing can open them, and a surface would show them as work anyway.
    let orphaned = loaded.prune_orphaned_tasks();
    let orphaned_everyone = loaded.prune_everyone_entries();
    if orphaned_everyone > 0 {
        tracing::info!(
            orphaned_everyone,
            "dropped Everyone entries quoting deleted turns"
        );
    }
    if orphaned > 0 {
        tracing::info!(orphaned, "pruned tasks whose conversations are gone");
    }
    // A document written by an older build may already be past the bounds, so
    // the first thing a restart does is fold it back in.
    let pruned = loaded.prune(glass_marble::worker::now_millis());
    if !pruned.is_empty() {
        tracing::info!(?pruned, "trimmed durable history on startup");
    }
    let events = EventBus::default();
    let approvals = ApprovalInbox::default();
    let state = Arc::new(Mutex::new(loaded));
    let timeout = Duration::from_secs(timeout_secs.max(30));
    let cancels = CancelRegistry::default();
    // Every chat lane owns an engine and the session of the conversation it
    // admitted, so two chats work at the same time instead of queueing. Lane
    // zero keeps the existing `sessions` directory, so a conversation written
    // before lanes existed still opens; later lanes keep their own stores.
    let binary = core_binary(cli);
    let lane_model = cli.model.clone();
    let lane_cwd = cwd.to_path_buf();
    let lane_root = sessions_dir(data_dir).to_path_buf();
    let lane_approvals = approvals.clone();
    let lane_events = events.clone();
    let lane_state = Arc::clone(&state);
    let approval_wait = approval_wait_secs.max(30);
    // The Prism roster gets its own engine per marble, started on first use: a
    // host that never swarms pays nothing for it, and each marble keeps a
    // workspace and session of its own.
    let prism_factory: Arc<dyn Fn() -> Command + Send + Sync> = Arc::new(runtime_factory(cli));
    let prism = Arc::new(PrismPool::new(glass_marble::prism::PrismPoolParts {
        factory: prism_factory,
        cwd: cwd.to_path_buf(),
        sessions: sessions_dir(data_dir).join("prism"),
        state: Arc::clone(&state),
        store: Some(Arc::clone(&store)),
        live: live.clone(),
        events: events.clone(),
        cancels: cancels.clone(),
        approvals: approvals.clone(),
        timeout,
    }));
    let lane_prism = Arc::downgrade(&prism);
    let lane_factory: LaneFactory = Arc::new(move |index| {
        let sessions = if index == 0 {
            lane_root.clone()
        } else {
            lane_root.join("lanes").join(index.to_string())
        };
        std::fs::create_dir_all(&sessions).map_err(MarbleError::Io)?;
        let mut command = Command::new(&binary);
        command.arg("runtime");
        let respawn_binary = binary.clone();
        let conversation = Conversation::spawn_with_router(
            command,
            &lane_cwd,
            &sessions,
            HostPolicy::asking(),
            lane_approvals.clone(),
            ToolRouter::new(&lane_cwd)?.with_ask({
                let pool = lane_prism.clone();
                move |scope, arguments| {
                    let Some(pool) = pool.upgrade() else {
                        return glass_marble::tools::ToolOutcome::error("the marble pool is gone");
                    };
                    pool.ask(scope, arguments)
                }
            }),
            lane_events.clone(),
        )?;
        let instructions_state = Arc::clone(&lane_state);
        Ok(conversation
            .with_model(lane_model.clone())
            .with_product(Arc::clone(&lane_state))
            .with_instructions_fn(move |cwd| {
                let persona = {
                    let guard = instructions_state
                        .lock()
                        .unwrap_or_else(|error| error.into_inner());
                    guard.prism.default_persona.clone()
                };
                glass_marble::default_assistant_instructions(&persona, cwd)
            })
            .with_respawn(move || {
                let mut command = Command::new(&respawn_binary);
                command.arg("runtime");
                command
            })
            .with_approval_timeout(Duration::from_secs(approval_wait)))
    });
    let pool = spawn_turn_pool(
        CHAT_LANES,
        lane_factory,
        Arc::clone(&state),
        Some(Arc::clone(&store)),
        live.clone(),
        events.clone(),
        timeout,
        cancels.clone(),
    )?;
    let turn_tx = pool.sender.clone();
    // Routines fire on their own clock, and approvals reach the person who is
    // not looking at a screen.
    spawn_scheduler(
        Arc::clone(&state),
        Some(Arc::clone(&store)),
        events.clone(),
        turn_tx.clone(),
        TICK,
    );
    notify::spawn_event_watcher(events.clone(), Arc::clone(&state));
    // The workspace's own crates, read once at startup: which crate owns a
    // behaviour is a question about the manifest, not about a file path.
    let crate_map = glass_marble::CrateMap::load(cwd);
    let listener = TcpListener::bind((host, port)).map_err(MarbleError::Io)?;
    let server = MarbleServer::new(MarbleServerParts {
        listener,
        state: Arc::clone(&state),
        store: Some(store),
        approvals,
        events,
        turn_tx,
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(token),
        cancels,
    })
    .with_steer_sink(pool.steer)
    .with_prism(Arc::clone(&prism))
    .with_crates(crate_map);
    prism.resume_held();
    let server = match (tls_cert, tls_key) {
        (Some(cert), Some(key)) => {
            let identity =
                glass_marble::tls::load_identity(&cert, &key).map_err(MarbleError::Protocol)?;
            tracing::info!(
                cert_sha256 = %identity.leaf_sha256_hex(),
                cert = %cert.display(),
                "serving HTTPS on the same port"
            );
            server.with_tls(Arc::new(identity))
        }
        _ => server,
    };
    let address = server.local_addr();
    println!("glass-marble listening on http://{address}");
    println!("pid: {}", std::process::id());
    println!("workspace: {}", cwd.display());
    println!("data: {}", data_dir.display());
    println!("control:  {} profile", profile.as_str());
    {
        let state = state.lock().unwrap_or_else(|error| error.into_inner());
        println!(
            "presence: {} · {} routines · {} memories · {} marble(s) on the prism roster",
            state.presence.status(&state),
            state.routines.routines.len(),
            state.memory.facts.len(),
            state.prism.marbles.len()
        );
        if recovered > 0 {
            println!("recovery: {recovered} interrupted turn(s) marked");
        }
        if live_recovery.turns > 0 {
            println!(
                "recovery: {} live turn(s) restored ({} with a partial reply)",
                live_recovery.turns, live_recovery.partials
            );
        }
        let channels = match (
            !state.notifications.webhook_url.is_empty(),
            state
                .notifications
                .apns
                .as_ref()
                .map(|apns| !apns.topic.is_empty())
                .unwrap_or(false),
        ) {
            (true, true) => "webhook + apns",
            (true, false) => "webhook",
            (false, true) => "apns",
            (false, false) => "none yet — set one in Control",
        };
        println!("notify:   {channels}");
    }
    if let Some(tailnet) = tailscale_ipv4() {
        println!("tailnet:  http://{tailnet}:{}", address.port());
    } else {
        println!(
            "tailnet:  bind --host to this machine's Tailscale address to share it over Tailscale"
        );
    }
    server.serve().map_err(MarbleError::Io)
}

/// Findings collected by `glass-marble doctor`.
#[derive(Default)]
struct DoctorReport {
    findings: Vec<(&'static str, String)>,
    problems: usize,
}

impl DoctorReport {
    fn ok(&mut self, message: impl Into<String>) {
        self.findings.push(("ok", message.into()));
    }

    fn warn(&mut self, message: impl Into<String>) {
        self.findings.push(("warn", message.into()));
    }

    fn info(&mut self, message: impl Into<String>) {
        self.findings.push(("info", message.into()));
    }

    fn fail(&mut self, message: impl Into<String>) {
        self.findings.push(("fail", message.into()));
        self.problems += 1;
    }
}

/// Check what an always-on instance needs, without touching anything.
///
/// Marble is meant to run unattended for months, so the questions an operator
/// actually has — is the document readable, is a backup recent, is the engine
/// installed, is another copy already running — should be answerable in one
/// command instead of by reading the data directory by hand.
fn doctor(cli: &Cli, cwd: &Path, data_dir: &Path) -> Result<(), MarbleError> {
    let mut report = DoctorReport::default();

    match std::fs::create_dir_all(data_dir) {
        Ok(()) => {
            let probe = data_dir.join(".doctor-probe");
            match std::fs::write(&probe, b"ok").and_then(|()| std::fs::remove_file(&probe)) {
                Ok(()) => report.ok(format!("{} is writable", data_dir.display())),
                Err(error) => {
                    report.fail(format!("cannot write to {}: {error}", data_dir.display()))
                }
            }
        }
        Err(error) => report.fail(format!("cannot create {}: {error}", data_dir.display())),
    }

    let store = StateStore::in_data_dir(data_dir);
    match store.load() {
        Ok(state) => report.ok(format!(
            "document revision {} · {} conversation(s) · {} task(s) · {} transcript entr{}",
            state.revision,
            state.conversations.len(),
            state.tasks.len(),
            state.transcript.len(),
            if state.transcript.len() == 1 {
                "y"
            } else {
                "ies"
            }
        )),
        Err(error) => report.fail(format!("the product document is unreadable: {error}")),
    }

    let backup = store.backup_status();
    match backup["ageSeconds"].as_u64() {
        Some(seconds) if seconds > 8 * 86_400 => report.warn(format!(
            "{} backup(s); the newest is {} days old",
            backup["count"],
            seconds / 86_400
        )),
        Some(seconds) => report.ok(format!(
            "{} backup(s); the newest is {} hours old",
            backup["count"],
            seconds / 3_600
        )),
        None => report
            .warn("no daily backup yet; one is written within five minutes of serving".to_string()),
    }
    let quarantined = store.quarantined_count();
    if quarantined > 0 {
        report.warn(format!(
            "{quarantined} unreadable document(s) kept aside as product-state.corrupt-*; inspect them"
        ));
    }

    match DataDirLock::acquire(data_dir) {
        Ok(lock) => {
            let held = lock.held();
            drop(lock);
            if held {
                report.ok("no other instance is serving this data directory".to_string());
            }
        }
        Err(_) => report.info(
            "another instance is serving this data directory; expected while the service runs",
        ),
    }

    let binary = core_binary(cli);
    match which(&binary) {
        Some(path) => report.ok(format!("engine {}", path.display())),
        None => report.fail(format!(
            "engine binary {} was not found; pass --core-binary or set GLASS_CORE_BINARY",
            binary.display()
        )),
    }

    match std::fs::create_dir_all(sessions_dir(data_dir)) {
        Ok(()) => report.ok(format!(
            "engine sessions live in {}",
            sessions_dir(data_dir).display()
        )),
        Err(error) => report.fail(format!("cannot create the sessions directory: {error}")),
    }

    if std::env::var("GLASS_MARBLE_TOKEN")
        .map(|token| !token.trim().is_empty())
        .unwrap_or(false)
    {
        report.ok("GLASS_MARBLE_TOKEN is set, so off-loopback serving can authenticate");
    } else {
        report.info("no token in the environment; serving off loopback needs --token-file");
    }
    report.info(format!("workspace {}", cwd.display()));

    println!("glass-marble doctor");
    println!("version: {}", env!("CARGO_PKG_VERSION"));
    for (level, message) in &report.findings {
        println!("  {level:<4} {message}");
    }
    if report.problems == 0 {
        println!("\nall checks passed");
        return Ok(());
    }
    Err(MarbleError::Protocol(format!(
        "{} check(s) need attention",
        report.problems
    )))
}

/// Resolve one binary the way a shell would.
fn which(binary: &Path) -> Option<PathBuf> {
    if binary.components().count() > 1 {
        return binary.is_file().then(|| binary.to_path_buf());
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(binary))
        .find(|candidate| candidate.is_file())
}

/// Inputs for the login service that keeps the assistant running.
struct ServiceOptions<'a> {
    cwd: &'a Path,
    data_dir: &'a Path,
    port: u16,
    host: &'a str,
    profile: &'a str,
    token_file: Option<PathBuf>,
    tls_cert: Option<PathBuf>,
    tls_key: Option<PathBuf>,
    write: bool,
}

/// Render or write the login service that keeps the assistant running.
fn install_service(cli: &Cli, options: ServiceOptions<'_>) -> Result<(), MarbleError> {
    let ServiceOptions {
        cwd,
        data_dir,
        port,
        host,
        profile,
        token_file,
        tls_cert,
        tls_key,
        write,
    } = options;
    let profile = ControlProfile::parse(profile).ok_or_else(|| {
        MarbleError::Protocol(format!(
            "unknown control profile {profile}: use observe, workspace, or unattended"
        ))
    })?;
    let binary = std::env::current_exe().unwrap_or_else(|_| core_binary(cli));
    let spec = ServiceSpec {
        binary,
        cwd: cwd.to_path_buf(),
        data_dir: data_dir.to_path_buf(),
        host: host.to_string(),
        port,
        profile: profile.as_str().to_string(),
        token_file,
        tls_cert,
        tls_key,
        log_file: service::default_log_file(data_dir),
    };
    let outcome = service::install(service::current_platform(), &spec, write)
        .map_err(MarbleError::Protocol)?;
    println!("{}", outcome.rendered);
    println!();
    if outcome.written {
        println!("wrote {}", outcome.path.display());
    } else {
        println!(
            "review the definition above, then rerun with --write to install it at {}",
            outcome.path.display()
        );
    }
    println!();
    println!("next:");
    for line in &outcome.instructions {
        println!("  {line}");
    }
    Ok(())
}

fn read_token(token_file: Option<&Path>) -> Result<Option<String>, MarbleError> {
    if let Some(path) = token_file {
        let token = std::fs::read_to_string(path).map_err(MarbleError::Io)?;
        let token = token.trim().to_string();
        if token.is_empty() {
            return Err(MarbleError::Protocol(format!(
                "token file {} is empty",
                path.display()
            )));
        }
        return Ok(Some(token));
    }
    let token = std::env::var("GLASS_MARBLE_TOKEN")
        .ok()
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty());
    Ok(token)
}

fn is_loopback(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]")
}

fn tailscale_ipv4() -> Option<String> {
    let output = Command::new("tailscale")
        .arg("ip")
        .arg("-4")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()?
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}
