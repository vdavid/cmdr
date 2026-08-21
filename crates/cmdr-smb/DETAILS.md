# `cmdr-smb` details

## Where the boundary runs, and why

The app's `network/` module grew as one pile: mDNS discovery, share listing, mounting, the keychain, the auto-upgrade
passes, and the Tauri events, plus a handful of pure functions over `smb2`'s own types that ended up there for no reason
beyond proximity. This crate is those pure functions, plus the vocabulary they speak.

The test is a single question: **can the protocol and its own types answer this?**

Here, because the answer is yes:

- `build_smb_addr` — an `smb2` address string. Strips a `.local` suffix, because `smb2` puts the addr's host component
  into UNC paths (`\\server\IPC$`) and some servers reject `.local` there.
- `is_auth_error` / `classify_error` / `classify_authenticated_error` — reading an `smb2::Error`. Every retry, fallback,
  and sign-in path branches on the first one, and it's the reason the `UpgradeFailure` log can name a rejected password
  even though its own enum has no auth variant.
- `try_list_shares_as_guest` / `try_list_shares_authenticated` — one connect and one `list_shares`, nothing else.
- `ShareInfo` / `AuthMode` / `ShareListResult` / `ShareListError` — what a listing attempt produced.

In the app, because the answer is no:

- **Discovery** (`network/mdns_discovery.rs`, `manual_servers.rs`, `virtual_smb_hosts.rs`) — mDNS is a different
  protocol, and the discovered-host registry is app state the frontend subscribes to.
- **Mounts** (`network/mount.rs`, `mount_linux.rs`) — `NetFSMountURLSync` and `gio mount` are OS APIs, not SMB.
- **The keychain** (`network/keychain.rs`, `credential_store.rs`) — the backend asks for credentials through the
  `CredentialStore` seam; where they're stored is the host's business.
- **The CLI fallback** (`network/smb_smbutil.rs`, `smb_smbclient.rs`) — subprocesses under the app's deadline wrapper,
  reading app settings.
- **The upgrade passes** (`network/smb_upgrade.rs`) — they decide when to replace a kernel mount with a direct session,
  which needs the volume registry, the index, and analytics.
- **Every event and every word** (`network/mod.rs`'s `VolumeConnectionChanged`, `SmbFellBackToOsMount`,
  `os_mount_notice.rs`) — `tauri_specta` payloads and the once-per-server ledger behind them.

The full app-side story stays in `apps/desktop/src-tauri/src/network/DETAILS.md`; this document doesn't restate it.

## Why `convert_shares` sits in `types.rs` rather than beside the classifiers

It builds `ShareInfo` out of `smb2::ShareInfo`, so it belongs with the type it produces. `errors.rs` answers one
question — what went wrong, and would credentials help — and keeping a successful-path mapper there blurred it.

`smb2::list_shares()` already filters to disk shares and strips `$` shares, which is why `is_disk` is unconditionally
true: the field exists for the frontend's benefit, not as a filter this crate applies.

## Why the `testing` feature does nothing of its own

It forwards `smb2/testing` and stops there. The app's `smb-e2e` feature turns it on, which reaches `smb2` through
`cmdr-smb` rather than naming `smb2/testing` directly.

That works because cargo unifies features per package across the whole resolved graph: there is one `smb2` node, and a
feature any dependent turns on is on for every dependent. So `network/virtual_smb_hosts.rs`, which calls
`smb2::testing::guest_port()` through the app's OWN direct `smb2` dependency, keeps compiling with no second forward.
(Verified with `cargo tree -p cmdr -e features -i smb2 --features smb-e2e` and a real
`cargo check -p cmdr --lib --examples --features smb-e2e` on cargo 1.9x, 2026-08-21: `smb2 feature "testing"` appears
only under the `smb-e2e` resolution.)

Two consequences worth knowing before the remaining extraction stages land:

- **A backend test that needs the Docker fixture ports can live on either side of the boundary.** Whichever crate's test
  target enables the feature, the one `smb2` gets it.
- **A `smb2/testing`-gated item is NOT reachable from a plain `cargo test -p cmdr-smb`.** The feature is off by default
  and this crate has no self dev-dependency turning it on, unlike `cmdr-archive`. Add one when a test here needs it, the
  way `cmdr-archive/Cargo.toml` does.

## What this crate deliberately doesn't have yet

- **No `cmdr-fs` dependency.** Nothing here implements `Volume` or touches a host seam, so the dependency would be
  unused and `unused_crate_dependencies` would say so. It arrives with `SmbVolume`.
- **No public-surface ceiling in `index-crate-isolation`.** `cmdr-archive` has one because its extraction is finished
  and its surface is the audited answer. Setting one here would mean raising it at every remaining stage, which trains
  the reflex the ceiling exists to prevent. The dependency guard applies from day one; the ceiling gets measured when
  the last stage lands. Reasoning in `scripts/check/checks/index-crate-isolation.go`.
