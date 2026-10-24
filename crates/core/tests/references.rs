//! The references checked in say what the code says.

use std::path::Path;

#[test]
fn the_references_checked_in_are_what_the_code_writes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (path, text) in adroit_core::reference::files() {
        let checked_in = std::fs::read_to_string(root.join(path)).unwrap_or_default();
        if checked_in != text {
            let line = checked_in
                .lines()
                .zip(text.lines())
                .position(|(a, b)| a != b)
                .unwrap_or_else(|| checked_in.lines().count().min(text.lines().count()));
            panic!(
                "{path} differs from what the code writes, from line {}; \
                 run `just references` and check the file in",
                line + 1
            );
        }
    }
}
