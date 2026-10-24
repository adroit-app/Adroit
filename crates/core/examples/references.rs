//! Writes the references in `docs/spec/reference/` from the code; `just
//! references` runs it.

use std::path::Path;

fn main() -> std::io::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (path, text) in adroit_core::reference::files() {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("a reference lies in a folder"))?;
        std::fs::write(&path, text)?;
    }
    Ok(())
}
