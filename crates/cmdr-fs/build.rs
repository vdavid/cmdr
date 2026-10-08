//! Folds the global-allocator rule into one cfg, `cmdr_mimalloc`: set when mimalloc is the
//! process's global allocator, so every allocator-specific line asks the same question.
//!
//! The rule: Linux always runs on mimalloc, and macOS only with the `mimalloc` feature.
//! Why the platforms differ: `DETAILS.md` § "Which global allocator".

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rustc-check-cfg=cfg(cmdr_mimalloc)");

    let feature_on = std::env::var_os("CARGO_FEATURE_MIMALLOC").is_some();
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if feature_on || target_os == "linux" {
        println!("cargo::rustc-cfg=cmdr_mimalloc");
    }
}
