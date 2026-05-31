use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");

    let Ok(output) = Command::new("git").args(["rev-parse", "HEAD"]).output() else {
        return;
    };
    if !output.status.success() {
        return;
    }
    let commit = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !commit.is_empty() {
        println!("cargo:rustc-env=CODEXHUB_BUILD_GIT_HEAD={commit}");
    }
}
