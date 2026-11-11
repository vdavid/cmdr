# cmdr-nfs

The NFSv3 backend: our own user-space client (ONC RPC over TCP, MOUNT, NFS) and a `Volume` per export, so a `secure`
export opens from an unprivileged process with no OS mount. WebDAV's lifecycle (`crates/cmdr-webdav`), minus the secret.

## Module map

- `params.rs` (`NfsServer`, `NfsIdentity`, `NfsConnectionParams`), `errors.rs` (every variant from a number the server
  sent).
- `xdr.rs` (bounded codec), `transport.rs` (`RpcConnection`), `proto/` (`rpc`, `portmap`, `mount`, `nfs3`'s
  `Nfs3Client`), `dial/` (resolve → portmap → split + `MNT`).
- `volume/`: `NfsVolume` and `connect_nfs_volume` (`mod.rs`), `handles.rs`, `streams.rs`, `writes.rs`, `mutation.rs`,
  `copy.rs`, `poll.rs`, `reconnect.rs`; `fake_nfs.rs` is the in-process server.

## Must-knows

- ❗ **v3 and `AUTH_SYS` only.** A server without v3 is `NoNfsV3`, ❌ never a fallback.
- ❌ **Never classify by message.** `nfsstat3`, `mountstat3`, and the RPC stats map by number; a variant's `String` is
  log-only.
- ❗ **Identity is `(host, canonical port, export)`**, minted only by `cmdr_fs::volume::nfs_volume_id` / `nfs_app_root`.
  The port is the one the address named, else 2049, ❌ never portmap's answer. The uid and groups are a SETTING, ❌
  never in an id or a path.
- ❗ **The app path carries the export as ONE escaped segment** (`nfs://nas:2049/volume1%2Fdata/photos`).
  `cmdr_fs::volume::remote_paths` owns the grammar; `server-path-utils.ts` mirrors it.
- ❗ **`connect_nfs_volume` resolves the export/folder split.** Register under the returned volume's `volume_id()` and
  save its `params()`, ❌ never the id of what was passed in.
- ❗ **Cancel a call by dropping it**: the XID is forgotten, ❌ never the connection. Only `transport.rs` and `rpc.rs`
  touch bytes; only a `Repeat::Safe` procedure is ever resent.
- ❗ **Every wire-touching delegator wraps itself in `noting`**, and a dial answers only `DialRefusal::Transient`: no
  secret, so no credential states.
- ❗ **A mutation goes through `dir_for_mutation`** (the parent chain proven by `GETATTR` stamps), by
  `dir handle + name`, ❌ never a leaf's cached handle.
- ❗ **Nothing is reported written that a COMMIT didn't vouch for**, and a `force = false` rename claims the name first.
  `write_is_single_shot` stays `false`: the engine stages.
- ❗ **The poller sends nothing unless `Connected` and a pane shows the folder.**
- ❌ **No `tauri`, no user-facing words** (`index-crate-isolation`); the public surface is capped at 10.

Decisions, the protocol rules, the write and rename paths, polling, and the test map: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
