//! The workspace's named platform cfgs, set from each member's build script so every crate
//! asks the platform question in the same words. Call [`emit`] from `build.rs`:
//!
//! ```ignore
//! fn main() {
//!     cmdr_platform_cfg::emit();
//! }
//! ```
//!
//! The set, and why it's this small: `docs/guides/platform-seams.md` § "Named cfgs".

/// The host operating systems Cmdr builds for, as `CARGO_CFG_TARGET_OS` spells them. A target
/// outside this list (and outside the browser build) is an unknown target: a crate root that asks
/// `cmdr_host` refuses it with a `compile_error!`, so a new OS is a deliberate decision rather
/// than a build that quietly picks up whichever `cfg` arm happens to match.
const HOST_OSES: [&str; 3] = ["macos", "linux", "windows"];

/// The cfg set on every [`HOST_OSES`] target: "this build runs on a desktop OS", the positive
/// counterpart of the browser build (`target_family = "wasm"`).
const HOST_CFG: &str = "cmdr_host";

/// Declares every named cfg for `check-cfg` and sets the ones that hold for the target being
/// built. Build-script only: it reads cargo's `CARGO_CFG_TARGET_OS` and prints directives for
/// cargo to read.
#[expect(
    clippy::print_stdout,
    reason = "a build script talks to cargo through stdout; these lines are directives, not output"
)]
pub fn emit() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    for directive in directives(&target_os) {
        println!("{directive}");
    }
}

/// The directives [`emit`] prints for a target, in order: the `check-cfg` declaration first (so
/// a crate that tests a cfg under some other target still compiles warning-free), then the cfgs
/// that hold.
fn directives(target_os: &str) -> Vec<String> {
    let mut out = vec![format!("cargo::rustc-check-cfg=cfg({HOST_CFG})")];
    if HOST_OSES.contains(&target_os) {
        out.push(format!("cargo::rustc-cfg={HOST_CFG}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_host_os_sets_the_host_cfg() {
        for os in HOST_OSES {
            assert_eq!(
                directives(os),
                vec!["cargo::rustc-check-cfg=cfg(cmdr_host)", "cargo::rustc-cfg=cmdr_host"],
                "{os}"
            );
        }
    }

    #[test]
    fn the_browser_build_and_unknown_targets_only_declare_it() {
        // `wasm32-unknown-unknown` reports `target_os = "unknown"`; the rest are real OSes Cmdr
        // doesn't build for, which the crate roots refuse.
        for os in ["unknown", "freebsd", "ios", "android", ""] {
            assert_eq!(directives(os), vec!["cargo::rustc-check-cfg=cfg(cmdr_host)"], "{os:?}");
        }
    }
}
