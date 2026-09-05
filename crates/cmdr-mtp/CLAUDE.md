# cmdr-mtp

Everything Cmdr says to an Android phone or a PTP camera over USB, with no app around it. macOS and Linux; on Linux a
missing udev rule (`resources/99-cmdr-mtp.rules`) reads as `PermissionDenied`. The app half (hotplug policy, the
`tauri_specta` events, the registrar wiring, ptpcamerad) is `apps/desktop/src-tauri/src/mtp/`.

## Module map

- `discovery.rs` (`list_mtp_devices`, and `watch_devices` re-exported from `mtp-rs`), `types.rs` (the camelCase device
  and storage vocabulary that crosses IPC).
- `connection/`: the per-device session layer. Its `CLAUDE.md` holds the locks, caches, and wire gotchas; read it before
  touching anything under there.
- `volume/`: `MtpVolume`, one storage area behind the `Volume` trait, split as `volume_impl` / `streams` / `mapping` /
  `scan` / `cancel`, plus `testing` (the `list_directory` counter and the read-window override).
- `virtual_device.rs`: the fixture-backed fake phone, behind the `virtual-device` feature.

## Must-knows

- **❌ Nothing here may name `tauri`, `tauri_specta`, or `cmdr`.** `cargo check -p cmdr-mtp --all-targets` is the whole
  verification loop, and `index-crate-isolation` proves the tree stays app-free.
- **The public surface is capped** at what the app uses today, with no headroom: 20 root promises, two public modules,
  13 items in them. `connection` is deliberately PRIVATE, so every session-layer name arrives as a root re-export. A new
  `pub` needs David's say-so, like a `file-length` entry. The item-by-item argument is in `DETAILS.md`.
- **❌ Never wrap an `mtp-rs` call in `tokio::time::timeout`, and never abort a task holding one.** The deadline drops
  the future mid-transaction and wedges the user's phone until they replug it; a `CancelToken` bails at a safe boundary.
  Enforced by `pnpm check mtp-dropping-timeout` over this tree AND the app's.
- **The manager is a VALUE.** `MtpConnectionManager::new(host, events, registrar)`. ❌ Never add a static here: the app
  parks its one manager, and a test builds its own (`connection::testing`).
- **❌ No English a user reads.** Every sentence is rendered host-side from the typed values here, which is also why
  `MtpDeviceEvents` reports an enum rather than a message.
- **Test-gated behavior takes `any(test, feature = "testing")`, ❌ never `cfg(test)`**, which is off when the app
  compiles this crate as a dependency. `virtual-device` is the separate axis: whether a fake phone exists at all.
- **`missing_docs` is denied.** Every `pub` item says what a caller must know; a doc restating the signature is worse
  than none, and specta copies these into `bindings.ts`.

The boundary's rationale, the capped surface item by item, and which side a test lives on: `DETAILS.md`. Read it before
any non-trivial work here.
