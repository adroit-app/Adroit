//! Tells the build which source it was made from, for the daemon's build
//! stamp and a debug build's window title.

fn main() {
    describe();
}

fn describe() {
    // Release builds are described too: one installed from a checkout keeps
    // the version of the last release, which cannot tell it from the daemon
    // an earlier install left running.
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
