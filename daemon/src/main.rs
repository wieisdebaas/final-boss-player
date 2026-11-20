pub mod db;
mod paths;
pub mod protocol;
pub mod server;
pub mod state;

use anyhow::Result;
use clap::Parser;
use paths::Paths;
use state::AppState;

#[derive(Debug, Parser)]
#[command(name = "endbossd")]
struct Args {
    #[arg(long)]
    print_paths: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let paths = Paths::from_env()?;

    if args.print_paths {
        println!("{}", serde_json::to_string_pretty(&paths)?);
        return Ok(());
    }

    paths.ensure()?;
    server::run(&paths.socket_path, AppState::new()).await
}
