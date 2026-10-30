//! `chess`: the app and its command line. `chess serve` runs the API, the event stream and the web UI; every
//! other command is a client of a running app and prints one JSON document with `--json`.

use std::process::ExitCode;

use chess::{
    Options,
    cli::{Cli, Client, Command},
};
use chess_hub::Config;
use clap::Parser;

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
            match cli.data {
                Some(data) => config.data_dir = std::path::absolute(&data).unwrap_or(data),
                // The default folder, on its first start: what is yours comes over from the app's old folder.
                None => adopt_legacy_data(&config.data_dir)?,
            }
            chess::app::serve(config, Options::default()).await
        }
        Command::Openapi => {
            println!("{}", chess::api::openapi().to_pretty_json()?);
            Ok(())
        }
        command => {
            let data = cli.data.unwrap_or_else(chess_hub::config::default_data_dir);
            chess::cli::run(Client::new(&cli.url, cli.token, &data), command, cli.json).await
        }
    }
}

/// Copies the old `dameless` folder's database, tokens, games and logs into the default data folder when that folder
/// does not exist yet, and logs what came over (`chess_hub::legacy`).
fn adopt_legacy_data(data_dir: &std::path::Path) -> anyhow::Result<()> {
    if data_dir != chess_hub::config::default_data_dir() {
        return Ok(());
    }
    let old = chess_hub::legacy::legacy_data_dir();
    if let Some(adopted) = chess_hub::legacy::adopt(data_dir, &old)? {
        tracing::info!(
            "copied {} from {} into {} (the old folder is left as it was)",
            adopted.copied.join(", "),
            adopted.from.display(),
            data_dir.display()
        );
        if adopted.engines_rebased > 0 {
            tracing::info!("{} engine(s) now point into {}", adopted.engines_rebased, data_dir.join("tools").display());
        }
    }
    Ok(())
}
