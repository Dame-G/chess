//! `chess`: the runner and its command line. `chess serve` runs the API, the live event stream and the embedded UI;
//! every other command is a client of a running runner and prints JSON with `--json`.

mod analysis;
mod api;
mod cli;
mod client;
mod config;
mod engine;
mod events;
mod experiments;
mod funnel;
mod jobs;
mod lichess;
mod matches;
mod proc;
mod stats;
mod store;
mod tools;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::config::Config;

#[derive(Parser)]
#[command(name = "chess", version, about = "Development harness for a Reckless fork")]
struct Cli {
    /// Data directory: database, builds, games, nets. Defaults to the platform's local data folder.
    #[arg(long, global = true, env = "CHESS_DATA")]
    data: Option<PathBuf>,

    /// Runner URL for client commands.
    #[arg(long, global = true, env = "CHESS_URL", default_value = "http://127.0.0.1:7878")]
    url: String,

    /// Bearer token for a remote runner. A local runner's token is read from its data directory.
    #[arg(long, global = true, env = "CHESS_TOKEN", hide_env_values = true)]
    token: Option<String>,

    /// Print machine-readable JSON instead of text.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the runner: API, live events and the UI.
    Serve {
        /// Address to listen on. Anything other than a loopback address requires the token on every request.
        #[arg(long, default_value = "127.0.0.1:7878")]
        listen: String,

        /// The Reckless checkout to build from.
        #[arg(long, env = "RECKLESS_DIR", default_value = "../reckless")]
        reckless: PathBuf,

        /// The Lichess server the bot plays on.
        #[arg(long, env = "LICHESS_URL", default_value = config::LICHESS_URL)]
        lichess_url: String,
    },
    #[command(flatten)]
    Client(cli::ClientCommand),
    /// Print the token the hosted UI needs to reach this machine's runner.
    Token,
    /// Print the OpenAPI document.
    Openapi,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let data = cli.data.clone().unwrap_or_else(config::default_data_dir);

    match cli.command {
        Command::Serve { listen, reckless, lichess_url } => {
            tracing_subscriber::fmt().with_target(false).init();
            let config = Config::load(data, reckless, listen, lichess_url)?;
            api::serve(config).await
        }
        Command::Token => {
            let token = std::fs::read_to_string(config::token_path(&data)).map_err(|_| {
                anyhow::anyhow!("no token in {} yet: start the runner once with `chess serve`", data.display())
            })?;
            println!("{}", token.trim());
            Ok(())
        }
        Command::Openapi => {
            println!("{}", api::openapi().to_pretty_json()?);
            Ok(())
        }
        Command::Client(command) => cli::run(client::Client::new(&cli.url, cli.token, &data), command, cli.json).await,
    }
}
