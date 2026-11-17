use clap::{Parser, Subcommand};
use glass_marble::{
    AccessPolicy, ApprovalInbox, Conversation, EventBus, HostPolicy, InboundMessage, MarbleError,
    MarbleServer, MarbleServerParts, MarbleTurn, ProductState, StateStore, ToolRouter, TurnOptions,
    default_data_dir, run_turn, sessions_dir, spawn_turn_worker, to_turn_input,
};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(
    name = "glass-marble",
    about = "Self-hosted durable Glass collaborator",
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
            } => serve(&cli, &cwd, &data_dir, port, &host, token_file, timeout_secs),
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

fn serve(
    cli: &Cli,
    cwd: &Path,
    data_dir: &Path,
    port: u16,
    host: &str,
    token_file: Option<PathBuf>,
    timeout_secs: u64,
) -> Result<(), MarbleError> {
    let token = read_token(token_file.as_deref())?;
    if !is_loopback(host) && token.is_none() {
        return Err(MarbleError::Protocol(
            "refusing to serve off loopback without a token: pass --token-file or set \
             GLASS_MARBLE_TOKEN"
                .into(),
        ));
    }
    let store = Arc::new(StateStore::in_data_dir(data_dir));
    let state = match store.load() {
        Ok(state) => state,
        Err(error) => {
            tracing::warn!(%error, "starting from an empty product state");
            ProductState::default()
        }
    };
    let events = EventBus::default();
    let approvals = ApprovalInbox::default();
    let router = ToolRouter::new(cwd)?;
    let conversation = Conversation::spawn_with_router(
        runtime_command(cli),
        cwd,
        &sessions_dir(data_dir),
        HostPolicy::asking(),
        approvals.clone(),
        router,
        events.clone(),
    )?
    .with_model(cli.model.clone());
    let timeout = Duration::from_secs(timeout_secs.max(30));
    let state = Arc::new(Mutex::new(state));
    let turn_tx = spawn_turn_worker(
        conversation,
        Arc::clone(&state),
        Some(Arc::clone(&store)),
        events.clone(),
        timeout,
    );
    let listener = TcpListener::bind((host, port)).map_err(MarbleError::Io)?;
    let server = MarbleServer::new(MarbleServerParts {
        listener,
        state,
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
    if let Some(tailnet) = tailscale_ipv4() {
        println!("tailnet:  http://{tailnet}:{}", address.port());
    } else {
        println!(
            "tailnet:  bind --host to this machine's Tailscale address to share it over Tailscale"
        );
    }
    server.serve().map_err(MarbleError::Io)
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
