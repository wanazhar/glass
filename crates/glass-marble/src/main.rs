use clap::{Parser, Subcommand};
use glass_marble::{
    ApprovalInbox, Conversation, InboundMessage, MarbleError, MarbleTurn, ProductState,
    to_turn_input,
};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex, mpsc};

#[derive(Debug, Parser)]
struct Cli {
    #[arg(value_name = "MESSAGE")]
    message: String,
    #[arg(long, value_name = "CONVERSATION_ID")]
    conversation: Option<String>,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Serve the local Glass marble product UI.
    Serve {
        #[arg(long, default_value_t = 4783)]
        port: u16,
    },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("glass-marble: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), MarbleError> {
    let cli = Cli::parse();
    if let Some(command) = cli.command {
        return serve_command(command);
    }
    let message = InboundMessage::new(cli.message);
    let turn = MarbleTurn {
        conversation_id: cli.conversation,
        request_id: format!("marble-{}", std::process::id()),
        input: to_turn_input(&message),
    };
    let cwd = std::env::current_dir().map_err(MarbleError::Io)?;
    let storage_dir = std::env::temp_dir().join("glass-marble-sessions");
    let binary = std::env::var_os("GLASS_CORE_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("glass-core"));
    let mut command = Command::new(binary);
    command.arg("runtime");
    let mut conversation =
        Conversation::spawn(command, &cwd, &storage_dir, ApprovalInbox::default())?;
    let reply = glass_marble::run_turn(&turn, &mut conversation)?;
    println!("{}", reply.text);
    Ok(())
}

fn serve_command(command: Commands) -> Result<(), MarbleError> {
    let Commands::Serve { port } = command;
    let cwd = std::env::current_dir().map_err(MarbleError::Io)?;
    let storage_dir = std::env::temp_dir().join("glass-marble-sessions");
    let binary = std::env::var_os("GLASS_CORE_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("glass-core"));
    let mut runtime = Command::new(binary);
    runtime.arg("runtime");
    let approvals = ApprovalInbox::default();
    let mut conversation = Conversation::spawn(runtime, &cwd, &storage_dir, approvals.clone())?;
    let state = Arc::new(Mutex::new(ProductState::default()));
    let (turn_tx, turn_rx) = mpsc::channel::<glass_marble::TurnCommand>();
    std::thread::spawn(move || {
        loop {
            let Ok(command) = turn_rx.recv() else {
                break;
            };
            if let Ok(reply) = glass_marble::run_turn(&command.turn, &mut conversation) {
                let _ = command.reply.send(Ok(reply));
            }
        }
    });
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(MarbleError::Io)?;
    let server = glass_marble::server::MarbleServer::bind(listener, state, approvals, turn_tx);
    server.serve().map_err(MarbleError::Io)
}
