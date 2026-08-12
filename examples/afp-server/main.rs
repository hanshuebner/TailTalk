use clap::Parser;
use std::path::PathBuf;
use tailtalk::{
    TalkStack,
    afp::AfpServerConfig,
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Network interface to bind to (EtherTalk)
    #[arg(short, long)]
    interface: Option<String>,

    /// Path to serve via AFP
    #[arg(short, long)]
    path: PathBuf,

    /// TashTalk serial port path (LocalTalk)
    #[arg(short, long)]
    tashtalk: Option<String>,

    /// Server name, as it appears in the Chooser
    #[arg(short, long)]
    name: Option<String>,

    /// Volume name, as it appears on the client's desktop
    #[arg(long)]
    volume_name: Option<String>,

    /// Serve the volume as a locked disk
    #[arg(short, long)]
    read_only: bool,
}

/// NBP carries the server name as a Pascal string, and AppleTalk allows an
/// object name of 32 characters.
const MAX_SERVER_NAME: usize = 32;

/// AFP 1.x and 2.x carry a Macintosh volume name, which is 27 characters.
const MAX_VOLUME_NAME: usize = 27;

/// MacRoman spends one byte per character, so counting characters counts the
/// bytes that go on the wire.
fn check_length(what: &str, value: &str, max: usize) {
    let length = value.chars().count();
    if length == 0 || length > max {
        eprintln!("error: --{what} must be 1 to {max} characters, got {length}");
        std::process::exit(1);
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    if args.interface.is_none() && args.tashtalk.is_none() {
        eprintln!("error: at least one of --interface or --tashtalk is required");
        std::process::exit(1);
    }

    if let Some(ref name) = args.name {
        check_length("name", name, MAX_SERVER_NAME);
    }
    if let Some(ref volume_name) = args.volume_name {
        check_length("volume-name", volume_name, MAX_VOLUME_NAME);
    }

    let mut builder = TalkStack::builder();
    if let Some(ref intf) = args.interface {
        builder = builder.ethernet(intf);
    }
    if let Some(ref tty) = args.tashtalk {
        builder = builder.localtalk(tty);
    }
    let stack = builder.build().await.expect("failed to build AppleTalk stack");

    let mut afp_config = AfpServerConfig {
        volume_path: args.path.clone(),
        read_only: args.read_only,
        ..AfpServerConfig::default()
    };
    if let Some(name) = args.name.clone() {
        afp_config.server_name = name;
    }
    if let Some(volume_name) = args.volume_name.clone() {
        afp_config.volume_name = volume_name;
    }

    let _afp_server = stack.spawn_afp(Some(254), afp_config)
        .await
        .expect("failed to spawn AFP server");

    let transport = args.interface.as_deref().unwrap_or("LocalTalk");
    tracing::info!("AFP server serving {:?} on {}", args.path, transport);
    tracing::info!("Press Ctrl+C to exit");

    let shutdown = stack.shutdown_handle();
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Ctrl+C received, shutting down");
        }
        _ = shutdown.transport_closed() => {
            tracing::info!("Transport closed, shutting down");
        }
    }
    shutdown.graceful_shutdown().await;
}
