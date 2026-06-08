# Rust rules (`src-tauri/`)

- ❌ No `eprintln!` / `println!` / `dbg!`: they bypass the fern logger (no level filter, no file output, not in
  error-report bundles) and clippy denies them. Use `log::{debug,info,warn,error}!` with a scoped `target:`. See
  `src-tauri/src/logging/CLAUDE.md`.
- ❌ No bare `.lock()/.read()/.write().unwrap()` on a std `Mutex`/`RwLock`: a poisoned lock aborts the whole app. Use
  `*_ignore_poison()` (recover) or `.expect("…poison…<why aborting is correct>")` (abort). Enforced by `lock-poison`.
  See `src-tauri/src/ignore_poison.rs`.
- ❌ Never build the app with raw `cargo build` (white screen, no embedded frontend). Use `pnpm tauri build` or the
  `tauri-wrapper.js build` wrapper, which runs `beforeBuildCommand`. See `apps/desktop/scripts/CLAUDE.md`.
