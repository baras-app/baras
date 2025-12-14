use baras::commands;
use clap::{Parser, Subcommand};
use std::io::Write;

use baras::app_state::AppState;
use baras::repl::readline;
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() -> Result<(), String> {
    let state = Arc::new(RwLock::new(AppState::new()));

    loop {
        let line = readline()?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        match respond(line, Arc::clone(&state)).await {
            Ok(quit) => {
                if quit {
                    break;
                }
            }
            Err(err) => {
                write!(std::io::stdout(), "{err}").map_err(|e| e.to_string())?;
                std::io::stdout().flush().map_err(|e| e.to_string())?;
            }
        }
    }

    Ok(())
}

#[derive(Parser)]
#[command(version, about = "cli")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    ParseFile {
        #[arg(short, long)]
        path: String,
    },
    FileInfo,
    Exit,
    Config,
}

async fn respond(line: &str, state: Arc<RwLock<AppState>>) -> Result<bool, String> {
    let mut args = shlex::split(line).ok_or("error: Invalid quoting")?;
    args.insert(0, "baras".to_string());
    let cli = Cli::try_parse_from(args).map_err(|e| e.to_string())?;

    match &cli.command {
        Some(Commands::ParseFile { path }) => commands::parse_file(path, Arc::clone(&state)).await,
        Some(Commands::FileInfo) => commands::file_info(Arc::clone(&state)).await,
        Some(Commands::Config) => commands::show_settings(Arc::clone(&state)).await,
        Some(Commands::Exit) => {
            commands::exit();
            return Ok(true);
        }
        None => {}
    }
    Ok(false)
}
