use baras_cli::commands;
use baras_cli::dir_watcher;
use baras_cli::readline;
use baras_cli::CliContext;
use clap::{Parser, Subcommand};
use std::io::Write;

#[tokio::main]
async fn main() -> Result<(), String> {
    let ctx = CliContext::new();

    // Initialize file index and start directory watcher
    if let Some(handle) = dir_watcher::init_watcher(&ctx).await {
        ctx.tasks.lock().await.watcher = Some(handle);
    }

    loop {
        let line = readline()?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        match respond(line, &ctx).await {
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
    Exit,
    Config,
    ListFiles,
    DeleteOld {
        #[arg(short, long)]
        days: u32,
    },
    SetDirectory {
        #[arg(short, long)]
        path: String,
    },
    Stats,
    CleanEmpty,
}

async fn respond(line: &str, ctx: &CliContext) -> Result<bool, String> {
    let mut args = shlex::split(line).ok_or("error: Invalid quoting")?;
    args.insert(0, "baras".to_string());
    let cli = Cli::try_parse_from(args).map_err(|e| e.to_string())?;

    match &cli.command {
        Some(Commands::ParseFile { path }) => commands::parse_file(path, ctx).await,
        Some(Commands::Config) => commands::show_settings(ctx).await,
        Some(Commands::ListFiles) => commands::list_files(ctx).await,
        Some(Commands::DeleteOld { days }) => commands::delete_old_files(ctx, *days).await,
        Some(Commands::CleanEmpty) => commands::clean_empty_files(ctx).await,
        Some(Commands::SetDirectory { path }) => commands::set_directory(path, ctx).await,
        Some(Commands::Stats) => commands::show_stats(ctx).await,
        Some(Commands::Exit) => {
            commands::exit();
            return Ok(true);
        }
        None => {}
    }
    Ok(false)
}
