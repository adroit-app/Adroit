//! Tells a debug build which source it was made from, for its window title.

fn main() {
    // A release is named by its version; only a build being tried needs this.
    if std::env::var("PROFILE").as_deref() != Ok("debug") {
        return;
    }
    let described = std::process::Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|d| !d.is_empty());
    if let Some(d) = described {
        println!("cargo:rustc-env=ADROIT_DESCRIBE={d}");
    }
}
