# What the SFTP backend still owes

The backend and its IPC surface are done: `crates/cmdr-sftp` connects, lists, reads, writes, copies, scans, and comes
back after a drop, and `crates/cmdr-sftp/DETAILS.md` is the canonical account of all of it. Four things are open, and
each one is written down beside the code it belongs to. This file exists so they stay schedulable rather than only
discoverable by someone already reading the crate.

❌ Nothing here restates a mechanism. Every item points at the doc that owns it.

## 1. There is no SFTP frontend

**The gap**: a connected SFTP volume is registered and navigable by `volumeId`, and every write path can reach it, but
nothing puts it on screen. `volume_listing::complete` has no SFTP arm, so the sidebar never shows one, and
`resolve_path_volume` / `resolve_location` don't answer for a remote path — an SFTP path is a plain server-side absolute
path with no scheme in front, spelled exactly like a local one, so whatever the sidebar decides about identity, path
resolution has to agree with it.

**Why it isn't specced here**: David designs and builds this, and the build guide already exists as one file —
`crates/cmdr-sftp/DETAILS.md` § "Connecting from the frontend" carries every command, the connect outcomes a sign-in UI
branches on, the three-round first connection, the two-phase host-key approval, and what the banner shows per auth rung.
The open questions are design ones (which sidebar section, what icon, what an eject means), not protocol ones.

**Cost**: the UI work, plus item 3 below, which the banner runs into immediately.

## 2. Free space and non-UTF-8 filenames both wait on one vendoring

Two unrelated-looking gaps with the same fix, which is why they are one piece of work rather than two.

- **`get_space_info` answers `NotSupported`**, so a pane never shows how full a server is. `statvfs@openssh.com` is
  unreachable from this crate stack: no request to send it, and no predicate to ask whether the server has it.
- **A filename that isn't valid UTF-8 costs the whole session**, not just the listing that hit it. That is the loud
  failure and the right one to have, but it means a server with one such name is unusable.

**The fix for both**: vendor `openssh-sftp-protocol` and `ssh_format` under `crates/` as **path** dependencies (❌ not
`git =`; `deny.toml` denies unknown git sources), then add the `statvfs` request and make `NameEntry::filename`
byte-backed. Roughly 2 750 lines of `src/` between them at the pinned versions, most of it protocol tables nobody edits
after the first read.

**Where the detail lives**: `crates/cmdr-sftp/DETAILS.md` § "4. A filename that isn't UTF-8 costs the SESSION" and §
"The `Volume` answers, and why" (the `get_space_info` bullet, including the app-side half of that contract, which is
already paid).

**Trigger**: a user hitting either. Vendoring buys a permanent maintenance obligation on two crates, so it wants a real
report behind it rather than a hypothetical.

## 3. `signIn` goes stale, and the banner is what notices

`connectSftpVolume` answers with the auth rung the session was built on and what a later "Sign in" may ask for. Both are
decided **per dial**, and a mid-life reconnect can land on a different rung — adding an agent identity moves a
`password` volume up to `agent`, removing one drops it back — while `volume-connection-changed` is payload-free by
design and no command re-reads the current rung for a live volume. So a banner built on the connect-time answer goes
stale in both directions: it can leave a volume that now wants a password with no way in, or ask for a secret the
session no longer uses.

**The two ways out**, both written up in `crates/cmdr-sftp/DETAILS.md` § "What the banner shows, per rung": widen the
event (a deliberate compile-time refusal across `events/volume_mapping.rs`'s `wire_state`), or add a command answering
the current rung and `signIn` for a `volumeId` and have the banner call it whenever the connection state flips.

❗ **Settle this before designing the banner**, not after: it decides whether the banner is event-driven or asks.

## 4. Not SFTP, but this effort surfaced it: two backends drop the path from `NotFound`

`VolumeError::NotFound` and `PermissionDenied` are defined to carry the missing PATH, and the transfer layer forwards
that payload straight into what the frontend renders as the name of the file the user is looking for. `LocalPosixVolume`
puts an errno string there and `SmbVolume` puts an NTSTATUS sentence there, so a copy that loses a file tells the user
to go hunting for a name that was never on their disk.

`cmdr_fs::volume::conformance::assert_not_found_carries_the_path` is the shared assertion, wired into every backend that
keeps the contract; its doc comment names both gaps and the reason each one has. `cmdr-smb/DETAILS.md` § "The `NotFound`
payload gap" carries SMB's site counts and the fix shape (`cmdr-sftp`'s `map_sftp_error`: give the mapper the path it is
mapping a failure for, so a pathless `NotFound` stops being constructible). LocalPosix's cause is the blanket
`impl From<std::io::Error> for VolumeError`, which fills all three path-carrying variants with `err.to_string()`; its
cell exists, `#[ignore]`d, in `local_posix_conformance_test.rs`.

**Two independent changes**, each across a shipping backend's whole error surface, which is why neither rode along with
the SFTP work. ❗ SMB's cell can't be added ahead of its fix: the SMB integration lane runs `--run-ignored only`, so an
`#[ignore]`d cell recording the gap would still run and still fail.
