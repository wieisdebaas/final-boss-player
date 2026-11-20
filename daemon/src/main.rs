mod paths;

use anyhow::Result;
use clap::Parser;
use paths::Paths;

#[derive(Debug, Parser)]
#[command(name = "endbossd")]
struct Args {
    #[arg(long)]
    print_paths: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let paths = Paths::from_env()?;

    if args.print_paths {
        println!("{}", serde_json::to_string_pretty(&paths)?);
        return Ok(());
    }

    paths.ensure()
}
