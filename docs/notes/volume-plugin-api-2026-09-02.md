# Third-party Volume plugins: what's missing, and who'd bring one

What it would take to tell people "does Cmdr not support your weird volume type? Write it as a plugin, here are the
docs", and whether the volume types anyone would actually bring fit the abstractions we have.

Written 2026-09-02, from reading the tree rather than from the roadmap. Scoped to **Volume plugins only**: the general
plugin-system design argument (four capabilities, transports, sandboxing, consent, the Column-first vertical slice) is
`totalcmd-plugin-analysis.md`, and this note assumes rather than repeats it.

## The short answer

The `Volume` trait is in good shape for this. What's missing is everything around it: a backend today is a compile-time
thing wired by a per-protocol app-side module, per-protocol IPC commands, a hand-written volume-ID constructor, and a
closed frontend union. Nothing about a backend is discovered at runtime.

Of the five volume types a third party would plausibly bring, **three fit the current abstractions and two need a new
seam**. The two seams (OAuth, path-triggered mounting) are worth building for first-party backends anyway, which is the
useful part of the finding.

## What already exists, and it's most of the contract

- **`Volume` is object-safe, fully async, and held as `Arc<dyn Volume>` everywhere.** Every method already returns
  `Pin<Box<dyn Future>>` (`crates/cmdr-fs/src/volume/mod.rs`), so a `PluginVolume` that forwards over a wire is a
  mechanical implementation rather than a redesign. Optional methods default to `NotSupported` / `false`, so a partial
  plugin is a legal plugin.
- **`VolumeHost` already names every seam a backend needs from the app** (`crates/cmdr-fs/src/volume/host/`): listings,
  events, credentials, host keys, indexing, activity, analytics, settings, runtime. That is the other half of a plugin
  API, already designed, already `dyn`, already carrying test doubles. It is the single most valuable asset here.
- **`cmdr_fs::volume::conformance` is a runnable data-safety contract**: delete never recurses, `rename(force = false)`
  refuses an existing destination, `create_file` refuses rather than truncates, `create_directory_all` reports a
  pre-existing leaf, export matches the bytes offered. That is `cmdr plugin test` waiting to happen, and it is what
  makes "let strangers write backends" defensible at all.
- **The API docs are ~70% written**: `apps/desktop/src-tauri/src/file_system/volume/DETAILS.md` § "Building a new
  volume" is four tiers of checklist plus a per-backend capability matrix.
- **Five worked examples**: `cmdr-archive`, `cmdr-smb`, `cmdr-sftp`, `cmdr-webdav`, `cmdr-adb`, plus app-resident MTP
  and `LocalPosixVolume`. `cmdr-sftp` is ~5.4k lines excluding tests, which is the honest "what am I signing up for"
  number to put in front of a plugin author.

## What's missing

### The runtime half

1. **A process boundary and a proxy.** A `PluginVolume: Volume` forwarding ~60 trait methods over JSON-RPC. Three parts
   are not mechanical:
   - `list_directory` takes `on_progress: Option<&(dyn Fn(ListingProgress) + Sync)>`, a host callback passed _into_ the
     backend, so the wire needs a reverse notification channel. Feeding it is mandatory, not optional
     (`backends/CLAUDE.md`: a silent backend gets cut off as unresponsive).
   - `open_read_stream` hands back a `Box<dyn VolumeReadStream>` with pull-based `next_chunk`, and `write_from_stream`
     hands the plugin a host-side stream to pull from. Bytes have to move both directions on a framed side channel,
     never through JSON. The tier-2 "no full-file buffering" rule becomes a wire-protocol requirement: a plugin that
     buffers an 8 GB file allocates 8 GB.
   - Cancellation: `CancellationToken` has to become a real cancel message, or a plugin wedges the pane.
   - `FileEntry` serializes cleanly already (all `serde` + `specta`, mostly `Option`), so the data model is not a
     problem.
2. **A declarative connect path.** `network/connect_wiring.rs` now holds the half that's identical across protocols (the
   attempt table, the cancel race, the supersede order). What stays per-backend is the connection params, the IPC
   commands, and the sign-in form: `sftp_volume_wiring.rs`, `webdav_volume_wiring.rs`, `smb_upgrade.rs`,
   `adb/volume_wiring.rs`, `mtp/volume_wiring.rs`. A plugin needs the declarative version: the manifest ships a params
   schema, Cmdr renders the form, dials the plugin, and registers. This is the piece with the least precedent.
3. **A generic volume-ID constructor.** `crates/cmdr-fs/src/volume/ids.rs` has a hand-written function per scheme
   (`local_`, `path_`, `smb_`, `sftp_`, `webdav_`, `adb_`, plus `mtp_ids`), and a hard rule against building an ID any
   other way, because an ID keys the index DB, `lastUsedPaths`, and tab state. Plugins need
   `plugin_volume_id(plugin_id, parts)` with the plugin id inside the digest, so two plugins can't collide into each
   other's index.
4. **Frontend surfaces that aren't closed unions.** `VolumeKind`
   (`apps/desktop/src/lib/file-explorer/pane/volume-capabilities.ts`) and `LocationCategory` are frozen sets that grow a
   member per backend; tint, icons, and the connect dialog branch on them. A plugin volume needs a `plugin` kind
   carrying display identity as DATA (name, icon, tint) rather than as a compile-time branch.
5. **A published, permissively licensed API crate.** `cmdr-fs` is `publish = false` and `LicenseRef-BSL-1.1`, and it
   drags bundled `rusqlite`, `uzers`, and `xattr`. Nobody outside this repo can build against it. A lean
   `cmdr-volume-api` (trait, types, host seams, conformance) under a permissive license is a prerequisite, and it is a
   licensing decision as much as a refactor.
6. **Trust: manifest scopes, install consent, a sandbox profile, crash and hang containment.** Argued in
   `totalcmd-plugin-analysis.md` § 3 and not re-argued here. The volume-specific addition is credentials: the
   `CredentialStore` seam is Keychain-backed, so a plugin needs a per-plugin namespace, and for protocols where the host
   can do the auth the plugin should never see the secret at all.

### The ecosystem half, which decides whether anyone shows up

7. `cmdr plugin new` / `test` / `lint`, hot reload, and a manifest `schemaVersion` from the first commit.
8. Install, update, signing, and a revoke path.
9. The page we'd actually point people at.

Items 1-5 are the real build. **Guess, not a measurement**: six to ten weeks to something strangers could install, most
of it in 1, 2, and 6 rather than in the trait plumbing.

## Two gaps closed themselves while this was being written

Both were named as blockers in the first pass of this analysis and are now shipped, which is worth recording so nobody
re-derives them:

- **Runtime location publishing is solved for devices.** `apps/desktop/src-tauri/src/device_volumes.rs` generalized the
  MTP-only fold that `volume_listing::complete` used to hardcode: a backend registers one `DeviceVolumeProvider`, the
  listing folds over every provider, eject and path resolution ask which provider owns an id, and hotplug pushes through
  a backend-neutral `volumes-changed`. A plugin host would register exactly one provider for all its plugins. **Two
  things still block a plugin from using it as-is**: `DeviceVolumeEntry::fs_type` is `&'static str` (a runtime plugin
  namespace has no `'static` to give), and `append_from` files every entry as `LocationCategory::MobileDevice`, so a
  plugin volume that isn't a device has no category to land in.
- **The connect flow is half-generalized**, as described in item 2 above. What remains is narrower than "build a connect
  flow": it's a params schema and a form renderer.

## The two gaps that still stand

1. **OAuth and browser-based auth.** `SignInPrompt`, `NeedsCredentials`, and the whole `volume-connection-changed`
   recovery story are password-shaped, and `HostKeys` covers SSH-style trust. There is no seam for "open a browser, come
   back with a token, refresh it before it expires." This blocks every git host and every proprietary consumer cloud,
   first-party or plugin. **Worth building regardless of whether plugins happen.**
2. **Path-triggered volume routing.** "This path crosses into something I serve" exists exactly once, hardcoded to
   archive magic bytes in `file_system/volume/manager/archive_routing.rs`. `ArchiveVolume` is a proper stacked volume
   (its `lane_key` and `get_space_info` delegate to the parent volume holding the `.zip`), so the shape is proven; it
   just isn't registerable. Blocks encrypted vaults, disk images, and every "open X as a folder" idea in
   `totalcmd-plugin-analysis.md` § Packer. **Also worth building regardless.**

## Cheaper rungs on the same ladder

- **(a) Recipe, not runtime.** Publish the trait docs, the tier checklist, the capability matrix, and the conformance
  suite, and invite people to send a backend crate we compile in. Days, not weeks; zero new machinery; real
  contributions. We pay by owning the review, by contributors not being able to ship independently, and by asking them
  to put code in a BSL repo.
- **(b) The rclone bridge.** Most "weird volume types" already exist as one of rclone's ~70 backends. One "connect via
  rclone" volume driving `rclone rcd`, or pointing the SFTP/WebDAV backends at `rclone serve`, answers the long tail for
  roughly one backend of work and no plugin API at all. Not a plugin story, but a better answer to the user's actual
  question per unit of effort. Composes with (a). Open questions nobody has checked: bundling a Go binary against our
  size and notarization budget, and whether `rcd`'s API can carry streaming writes without a temp file.
- **(c) The full subprocess plugin host.** Items 1-9.

If the goal is the message, (a) plus (b) delivers it now and (c) is what happens once somebody has actually turned up
wanting it.

## Who'd actually bring one

Ranked by real 2026 demand times fit. Excludes what's ours: S3 and S3-compatible, SFTP, WebDAV, and Android over ADB are
first-party (WebDAV and ADB backends landed 2026-08/09; their UIs follow).

1. **iOS device over libimobiledevice / AFC.** The biggest genuine gap. Finder's integration is all-or-nothing and the
   commercial tools are subscription-priced, so the demand is real and unserved. **Fits**: device-anchored exactly like
   MTP (`rerooted` → `None`, `lane_key` = device serial, `max_concurrent_ops` = 1), and `DeviceVolumeProvider` now
   exists for the hotplug half.
2. **A git host as a volume** (GitHub / GitLab: repo × branch × path, browsed without cloning). Dev-centric, which is
   our audience; `totalcmd-plugin-analysis.md` § 3 notes the newest TC plugins are all this shape. **Fits read-only,
   does not fit writable**: git has no empty directories and no rename, and a write is a commit, which is a BATCH
   semantic the trait has no concept of. Read-only is a first-class option (`ArchiveVolume` shipped that way). The
   blocker is gap 1, OAuth.
3. **An encrypted vault** (Cryptomator, gocryptfs, age). Real privacy demand and a natural outside contribution. **Fits
   the trait, blocked by gap 2**: it's a volume stacked on another volume, triggered by a path, and only archives can do
   that today.
4. **Container and cluster filesystems** (`docker exec`, `kubectl cp`, distrobox). The most literal reading of "weird
   volume type", and squarely our audience. **Fits**, with the caveat that per-entry stat is a subprocess round trip, so
   it lands in MTP's "no single-file stat, list the parent instead" pattern and wants `get_metadata` implemented in
   terms of a cached listing.
5. **FTP and FTPS.** `ftp-crate-evaluation-2026-08-22.md` evaluated it and recommended NOT building it, parking it
   behind a request counter. That makes it the ideal plugin: a protocol we've said no to, with the crate question
   already settled (`suppaftp`) and four gotchas already written down. **Fits** on the SFTP rails. A parked protocol
   becoming somebody else's yes is the clearest argument for having a plugin API at all.

Runners-up: non-native filesystem and VM disk images (ext4, NTFS, APFS raw, VMDK / VDI), proprietary consumer clouds
that aren't S3-shaped (Proton Drive, pCloud, Mega, Jottacloud), and an rclone bridge if we don't ship (b) ourselves.

## What would change the answer

- Somebody actually asking. Nothing in this note is demand evidence; it's fit analysis. A request counter on
  "unsupported protocol" would be worth more than another design pass.
- Building the OAuth seam or the path-triggered routing seam for a first-party reason, which moves two of the five
  candidates from "needs a seam" to "fits" for free.
- A decision on (b): if an rclone bridge ships, the marginal value of a Volume plugin API drops a lot, because the long
  tail it was meant to serve is already served.
