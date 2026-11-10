//! The release gate: refuses any update archive the macOS updater would refuse to install.
//!
//! ```text
//! cargo run -p cmdr-update-trust --bin cmdr-vet-update -- --version 0.53.0 Cmdr_0.53.0_*.app.tar.gz
//! ```
//!
//! Each archive is unpacked with the installer's own extraction and checked with the app's own
//! code-signature check, then for the release's version. Exits 1 if any archive is refused, 2 on
//! bad usage. `scripts/release-finish` runs it before signing; the release pipeline's
//! `vet-update-archives` job runs it in whichever run publishes.

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a command-line gate: its verdicts are its output"
)]

#[cfg(target_os = "macos")]
fn main() {
    use std::path::PathBuf;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let (version, archives) = match args.split_first() {
        Some((flag, rest)) if flag == "--version" && rest.len() >= 2 => (&rest[0], &rest[1..]),
        _ => {
            eprintln!("Usage: cmdr-vet-update --version <release version> <archive.app.tar.gz>...");
            std::process::exit(2);
        }
    };

    let mut refused = 0;
    for archive in archives {
        let path = PathBuf::from(archive);
        let name = path
            .file_name()
            .map_or_else(|| archive.clone(), |n| n.to_string_lossy().into_owned());
        match cmdr_update_trust::vet_update_archive(&path, version) {
            Ok(()) => println!("    {name}: Cmdr's Developer ID build of {version}, as the updater requires"),
            Err(refusal) => {
                println!("    {name}: REFUSED, {refusal}");
                refused += 1;
            }
        }
    }
    if refused > 0 {
        eprintln!(
            "{refused} of {} update archives would be refused by every install.",
            archives.len()
        );
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("cmdr-vet-update runs Apple's code-signature check, so it needs macOS.");
    std::process::exit(2);
}
