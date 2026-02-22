use std::process::Command;

fn main() {
    let sha = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    if let Some(sha) = sha {
        let short_sha = &sha[..7];
        println!("cargo:rustc-env=GIT_SHA={sha}");
        println!("cargo:rustc-env=GIT_SHORT_SHA={short_sha}");
    }
}
