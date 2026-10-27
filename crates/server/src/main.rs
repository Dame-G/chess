//! `dameless`: the app and its command line. `dameless serve` runs the API, the event stream and the web UI; every
//! other command is a client of a running app and prints one JSON document with `--json`.

use std::process::ExitCode;

use clap::Parser;
use dameless::{
    Options,
    cli::{Cli, Client, Command},
};
use dameless_hub::Config;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Command::Serve => {
            // `RUST_LOG=debug` (or warn, error, trace) changes how much is logged; info by default.
            let level = std::env::var("RUST_LOG").ok().and_then(|l| l.parse().ok()).unwrap_or(tracing::Level::INFO);
            tracing_subscriber::fmt().with_target(false).with_max_level(level).init();
            let mut config = Config::from_env()?;
            if let Some(data) = cli.data {
                config.data_dir = std::path::absolute(&data).unwrap_or(data);
            }
            dameless::app::serve(config, Options::default()).await
        }
        Command::Openapi => {
            println!("{}", dameless::api::openapi().to_pretty_json()?);
            Ok(())
        }
        command => {
            let data = cli.data.unwrap_or_else(dameless_hub::config::default_data_dir);
            dameless::cli::run(Client::new(&cli.url, cli.token, &data), command, cli.json).await
        }
    }
}
