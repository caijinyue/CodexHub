mod cli;
mod config;
mod doctor;
mod process;
mod profile;
mod shared;
mod shell;
mod size;
mod tui;

fn main() -> anyhow::Result<()> {
    cli::run()
}
