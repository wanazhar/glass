use clap::{Parser, Subcommand};
use glass_marble::{
    AccessPolicy, ApprovalInbox, ControlProfile, Conversation, EventBus, HostPolicy,
    InboundMessage, MarbleError, MarbleServer, MarbleServerParts, MarbleTurn, ProductState,
    ServiceSpec, StateStore, TICK, ToolRouter, TurnOptions, default_data_dir, run_turn,
    sessions_dir, spawn_scheduler, spawn_turn_worker, to_turn_input,
};
use glass_marble::{notify, service};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(
    name = "glass-marble",
    about = "Always-on, self-hosted personal assistant built on Glass Core",
    version
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
                    profile: ControlProfile::parse(&profile).ok_or_else(|| {
                        MarbleError::Protocol(format!(
                            "unknown control profile {profile}: use observe, workspace, or unattended"
                        ))
                    })?,
                },
            ),
            Commands::InstallService {
                port,
                host,
                profile,
                token_file,
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
    let store = Arc::new(StateStore::in_data_dir(data_dir));
    let mut loaded = match store.load() {
        Ok(state) => state,
        Err(error) => {
            tracing::warn!(%error, "starting from an empty product state");
            ProductState::default()
        }
    };
    loaded.control.profile = profile;
    // An always-on assistant has to be honest about what a restart cost it: a
    // turn that was running when the process stopped is marked interrupted, not
    // left looking like it is still working.
    loaded.presence.mark_start();
    let recovered = loaded.recover_interrupted();
    if recovered > 0 {
        tracing::warn!(recovered, "marked interrupted turns from the previous run");
    }
    // A document written by an older build may already be past the bounds, so
    // the first thing a restart does is fold it back in.
    let pruned = loaded.prune(glass_marble::worker::now_millis());
    if !pruned.is_empty() {
        tracing::info!(?pruned, "trimmed durable history on startup");
    }
    let events = EventBus::default();
    let approvals = ApprovalInbox::default();
    let router = ToolRouter::new(cwd)?;
    let state = Arc::new(Mutex::new(loaded));
    let conversation = Conversation::spawn_with_router(
        runtime_command(cli),
        cwd,
        &sessions_dir(data_dir),
        HostPolicy::asking(),
        approvals.clone(),
        router,
        events.clone(),
    )?
    .with_model(cli.model.clone())
    .with_product(Arc::clone(&state))
    .with_approval_timeout(Duration::from_secs(approval_wait_secs.max(30)));
    let timeout = Duration::from_secs(timeout_secs.max(30));
    let turn_tx = spawn_turn_worker(
        conversation,
        Arc::clone(&state),
        Some(Arc::clone(&store)),
        events.clone(),
        timeout,
    );
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
    let listener = TcpListener::bind((host, port)).map_err(MarbleError::Io)?;
    let server = MarbleServer::new(MarbleServerParts {
        listener,
        state: Arc::clone(&state),
        store: Some(store),
        approvals,
        events,
        turn_tx,
        policy: AccessPolicy::new(token),
    });
    let address = server.local_addr();
    println!("glass-marble listening on http://{address}");
    println!("workspace: {}", cwd.display());
    println!("data: {}", data_dir.display());
    println!("control:  {} profile", profile.as_str());
    {
        let state = state.lock().unwrap_or_else(|error| error.into_inner());
        println!(
            "presence: {} · {} routines · {} memories",
            state.presence.status(&state),
            state.routines.routines.len(),
            state.memory.facts.len()
        );
        if recovered > 0 {
            println!("recovery: {recovered} interrupted turn(s) marked");
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

/// Inputs for the login service that keeps the assistant running.
struct ServiceOptions<'a> {
    cwd: &'a Path,
    data_dir: &'a Path,
    port: u16,
    host: &'a str,
    profile: &'a str,
    token_file: Option<PathBuf>,
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
