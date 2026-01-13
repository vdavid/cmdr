---
description: How to add a Rust dependency to the project
---

1. `cd src-tauri && cargo add <package-name>` to add the dep to `Cargo.toml`
2. Verify the license is acceptable with `cargo deny check licenses`
3. Run `cargo build` to update `Cargo.lock`
4. Check with `./scripts/check.sh --rust` and `./scripts/check.sh --check e2e-tests`
5. Do not commit unless asked to
