//! Extracts smb2's consumer Docker Compose files to a directory.
//!
//! Used by `test/smb-servers/start.sh` to set up the SMB test containers.
//!
//! Run with:
//!   cargo run --example smb_compose --features smb-e2e -- <output_dir>

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "A standalone diagnostic run by hand from a terminal: stdout IS its output. \
The workspace lint set routes app code through `log::*` so error-report bundles capture it, \
which an example that nobody bundles has no use for."
)]

fn main() {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "test/smb-servers/.compose".to_string());
    let path = std::path::Path::new(&dir);
    smb2::testing::write_compose_files(path).expect("Failed to write compose files");
    println!("{}", path.display());
}
