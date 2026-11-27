pub mod art;
pub mod db;
pub mod fingerprint;
pub mod lookup;
pub mod loudness;
pub mod meta;
pub mod mpris;
mod paths;
pub mod player;
pub mod protocol;
pub mod scan;
pub mod server;
pub mod state;

use anyhow::Result;
use clap::Parser;
use paths::Paths;
use state::AppState;

#[derive(Debug, Parser)]
#[command(name = "finalbossd")]
struct Args {
    #[arg(long)]
    print_paths: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_logging();
    let args = Args::parse();
    let paths = Paths::from_env()?;

    if args.print_paths {
        println!("{}", serde_json::to_string_pretty(&paths)?);
        return Ok(());
    }

    paths.ensure()?;
    server::run_with_paths(
        &paths.socket_path,
        paths.db_path,
        paths.art_dir,
        AppState::new(),
    )
    .await
}

/// Logs to stderr (captured by journald under systemd). Filter with `RUST_LOG`,
/// defaulting to `finalbossd=info`. Colors only when stderr is a terminal.
fn init_logging() {
    use std::io::IsTerminal;
    use tracing_subscriber::EnvFilter;

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("finalbossd=info"));
    let ansi = std::io::stderr().is_terminal();
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(ansi)
        .init();
}
