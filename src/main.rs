mod activation;
mod cli;
mod config;
mod doctor;
mod process;
mod profile;
mod remote;
mod shared;
mod shared_account;
mod shell;
mod size;
mod tui;
mod update;

#[cfg(test)]
mod test_support {
    use std::sync::{Mutex, MutexGuard};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    pub fn env_lock() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner())
    }
}

fn main() -> anyhow::Result<()> {
    cli::run()
}
