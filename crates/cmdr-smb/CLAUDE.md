# `cmdr-smb`

The SMB backend's protocol layer: everything Cmdr says to an SMB server that needs no application around it. Address
building, error classification, and the share-listing vocabulary.

**Mid-extraction.** The `SmbVolume` backend itself still lives in the app (`file_system/volume/backends/smb/`); it moves
down here in stages. The plan and what's left: `docs/specs/backend-as-a-crate.md`. Writing a NEW backend? Copy
`crates/cmdr-archive/`, which is finished; this crate isn't the model yet.

## Module map

- `src/types.rs`: `ShareInfo` / `AuthMode` / `ShareListResult` / `ShareListError`, plus `convert_shares`. These cross
  IPC, so they carry serde and `specta::Type`.
- `src/errors.rs`: `is_auth_error` (would credentials help?) and the `classify_*` pair (`smb2::Error` →
  `ShareListError`).
- `src/connection.rs`: `build_smb_addr`, and the guest / authenticated share-listing calls.

## Must-knows

- **The boundary test is "can the protocol and its own types answer this?"** Yes → here. No (mDNS, keychain, kernel
  mounts, upgrade passes, anything the frontend sees) → the app's `network/`. Rationale and the full split:
  `DETAILS.md`.
- **`cargo check -p cmdr-smb` is the whole verification loop.** Nothing here may name the app; `index-crate-isolation`
  forbids `tauri` / `tauri-specta` / `cmdr` anywhere in this crate's tree.
- **`specta` is pinned to the exact version the app uses.** Two `specta` crates in one graph make these `Type` impls
  stop satisfying `tauri-specta`, and the app's command signatures collect them transitively.
- **The `testing` feature exists only to forward `smb2/testing`**, which the app's `smb-e2e` turns on through this
  crate. Cargo unifies features across the graph, so the app's own direct `smb2` calls see it too. ❌ Never gate
  behavior on `cfg(test)`; use `any(test, feature = "testing")`.
- **`#![deny(missing_docs)]` holds.** A new `pub` item, field, or enum variant needs a doc comment.
- **No user-facing prose.** A `message: String` on `ShareListError` is a diagnostic for logs; the host renders every
  word a human reads.
