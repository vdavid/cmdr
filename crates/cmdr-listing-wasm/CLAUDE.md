# cmdr-listing-wasm

The real listing engine (`cmdr-listing` over `cmdr-fs`'s `InMemoryVolume`) in a browser, for the `/demo` page (#302).
The engine's second host, beside `apps/desktop/src-tauri/src/listing_host/`. Nothing in the desktop app depends on it.

## Module map

- `tree.rs`: the page's JSON tree into an `InMemoryVolume`. `events.rs`: engine facts into Tauri-named events.
  `commands.rs`: `Demo`, the listing commands by their Tauri names. `bindings.rs` (wasm only): the `wasm-bindgen` API.
- `smoke.mjs`: loads the built module in Node and drives one listing. Build: `pnpm build:listing-wasm`.

## Must-knows

- **Same wire as the desktop, or the frontend breaks silently.** Command names, camelCase args, answers, rejections, and
  event payloads all mirror `commands/file_system/listing.rs` and `listing_host/events.rs`. A change there needs one
  here; `events_test.rs` pins every event's keys against `bindings.ts`.
- **Everything but `bindings.rs` builds natively**, so the tests run with `cargo nextest` like any crate. Keep JS types
  out of `commands.rs`, `events.rs`, and `tree.rs`.
- **The runtime traps are browser-only**: `std::time::Instant::now` and `SystemTime::now` panic, `Path::is_absolute` is
  always `false`, and nothing can spawn a thread. Native tests can't see them; `desktop-rust-listing-wasm` runs
  `smoke.mjs` against the real `.wasm` so they fail a check.
- **`JsSink` is `unsafe impl Send + Sync`**, sound only while the build has no `atomics` target feature (one thread).
  The `cfg` makes a threaded build fail to compile; ❌ never drop it.
- **The `wasm-bindgen` CLI must equal the locked crate exactly.** The build script reads `Cargo.lock` and installs the
  match; ❌ never pin the CLI anywhere else.

The decision (target, runtime), the JS API, the build, and where the demo differs from the app: `DETAILS.md`. Read it
before any non-trivial work here: editing, planning, reorganizing, or advising.
