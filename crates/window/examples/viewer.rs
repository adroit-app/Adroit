//! Open a file in its viewer, in a window of its own:
//! `cargo run -p adroit --example viewer -- <file>`.

fn main() {
    let Some(path) = std::env::args_os().nth(1) else {
        eprintln!("usage: viewer <file>");
        std::process::exit(2);
    };
    adroit_window::viewers::try_in_window(path.into());
}
