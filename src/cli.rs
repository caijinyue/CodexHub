use crate::{config, doctor, process, profile, shared, shell, size, tui};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::io::{self, Write};

#[derive(Debug, Parser)]
#[command(
    name = "codexhub",
    version,
    about = "Multi CODEX_HOME profile manager for OpenAI Codex CLI"
)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Init,
    Create {
        name: String,
        #[arg(long)]
        copy_config: bool,
    },
    ImportDefault {
        name: Option<String>,
    },
    Login {
        name: String,
    },
    Run {
        name: String,
        #[arg(last = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    Exec {
        name: String,
        #[arg(last = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    Shell {
        name: String,
    },
    Path {
        name: String,
    },
    List,
    Doctor {
        #[arg(long)]
        allow_auth_symlink: bool,
    },
    ShareCache {
        name: String,
    },
    UnshareCache {
        name: String,
        #[arg(long)]
        restore_backup: bool,
        #[arg(long)]
        keep_empty: bool,
    },
    Delete {
        name: String,
    },
    Tui,
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Commands::Tui) {
        Commands::Init => {
            let paths = config::init()?;
            println!("Initialized {}", paths.root.display());
        }
        Commands::Create { name, copy_config } => {
            let path = profile::create(&name, copy_config)?;
            println!("Created profile {name}: {}", path.display());
        }
        Commands::ImportDefault { name } => {
            let (name, path) = profile::import_default(name.as_deref())?;
            println!("Imported ~/.codex as profile {name}: {}", path.display());
        }
        Commands::Login { name } => std::process::exit(process::codex_login(&name)?),
        Commands::Run { name, args } => std::process::exit(process::codex_run(&name, &args)?),
        Commands::Exec { name, args } => std::process::exit(process::codex_exec(&name, &args)?),
        Commands::Shell { name } => std::process::exit(shell::open(&name)?),
        Commands::Path { name } => println!("{}", profile::profile_path(&name)?.display()),
        Commands::List => print_list()?,
        Commands::Doctor { allow_auth_symlink } => print_doctor(allow_auth_symlink)?,
        Commands::ShareCache { name } => {
            shared::share_cache(&name)?;
            println!("Shared cache enabled for {name}");
        }
        Commands::UnshareCache {
            name,
            restore_backup,
            keep_empty,
        } => {
            shared::unshare_cache(&name, restore_backup, keep_empty)?;
            println!("Shared cache disabled for {name}");
        }
        Commands::Delete { name } => {
            confirm_delete(&name)?;
            profile::delete(&name)?;
            println!("Deleted profile {name}");
        }
        Commands::Tui => tui::run()?,
    }
    Ok(())
}

fn print_list() -> Result<()> {
    let profiles = profile::list()?;
    println!(
        "{:<16} {:<6} {:<19} {:>10} {:>10} {:>10} {:<6} PATH",
        "NAME", "LOGIN", "AUTH MTIME", "SESSIONS", "LOGS", "TOTAL", "SHARED"
    );
    for p in profiles {
        let auth = p
            .auth_mtime
            .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "-".into());
        println!(
            "{:<16} {:<6} {:<19} {:>10} {:>10} {:>10} {:<6} {}",
            p.name,
            if p.logged_in { "yes" } else { "no" },
            auth,
            size::human(p.sessions_size),
            size::human(p.logs_size),
            size::human(p.total_size),
            if p.shared_cache { "yes" } else { "no" },
            p.path.display()
        );
    }
    Ok(())
}

fn print_doctor(allow_auth_symlink: bool) -> Result<()> {
    let checks = doctor::run(allow_auth_symlink)?;
    let mut exit_error = false;
    for check in checks {
        if check.level == doctor::Level::Error {
            exit_error = true;
        }
        println!(
            "[{}] {}: {}",
            check.level.as_str(),
            check.subject,
            check.message
        );
    }
    if exit_error {
        std::process::exit(2);
    }
    Ok(())
}

fn confirm_delete(name: &str) -> Result<()> {
    print!("Type profile name \"{name}\" to confirm deletion: ");
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("Reading confirmation")?;
    if input.trim() != name {
        anyhow::bail!("Deletion cancelled");
    }
    Ok(())
}
