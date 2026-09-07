# What the SFTP backend still owes

The backend and its IPC surface are done: `crates/cmdr-sftp` connects, lists, reads, writes, copies, scans, and comes
back after a drop, and `crates/cmdr-sftp/DETAILS.md` is the canonical account of all of it. So is the frontend (§ 1).
Three things are open, and each one is written down beside the code it belongs to. This file exists so they stay
schedulable rather than only discoverable by someone already reading the crate.

❌ Nothing here restates a mechanism. Every item points at the doc that owns it.

## 1. The frontend is built (kept as a pointer, not an open item)

A saved SFTP server gets a row in the volume switcher, a place in the servers hub, and the one sign-in sheet every
credential ask in the app opens. `volume_listing::complete` has its servers arm, paths carry an `sftp://` scheme so
`resolve_path_to_volume` answers for one, and a disconnected place comes back as a greyed `saved` row that dials on
activation.

**Where it is written down**: `apps/desktop/src/lib/servers/DETAILS.md` (the path grammar, the three connect arms, the
sheet contract, the renderer table, and the refusal table) and
`apps/desktop/src/lib/file-explorer/navigation/DETAILS.md` (which rows the switcher's Network group holds, and why a
server row says Disconnect). The protocol side it builds against is still
`crates/cmdr-sftp/DETAILS.md` § "Connecting from the frontend".

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

## 3. Not SFTP, but this effort surfaced it: two backends drop the path from `NotFound`

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

## 4. `~/.ssh/config` host aliases as autocomplete in the address field

**The gap**: someone who reaches a server as `ssh naspi` has to retype `ada@nas.local:2222` into the add form, because
Cmdr never reads `~/.ssh/config`. The alias is the name they know the machine by, and it already carries the host, the
port, the user, and often the identity file.

**The shape**: a backend command that parses `~/.ssh/config` (including `Include`) and answers a list of
`{ alias, hostname, port, user, identityFile }`, which the sheet's address field offers as completions; picking one
fills the endpoint fields and leaves them editable, exactly as the address parser's own answer does. ❌ Read-only, and
❗ never write to `~/.ssh/config` or `~/.ssh/known_hosts` (the second is already a standing rule in
`apps/desktop/src-tauri/src/network/CLAUDE.md`).

**Why it is not in the servers effort**: it is backend work with its own parser and its own edge cases (`Match` blocks,
tokens like `%h`, a config that names a `ProxyJump` Cmdr can't honor), and the add form is usable without it. The
frontend side is one more source of completions behind the field the address parser already fills.

**Cost**: a day for a config parser that handles `Host`, `HostName`, `Port`, `User`, `IdentityFile`, and `Include`,
plus the decision about what to do with an alias whose directives Cmdr can't act on (offer it and let the connect
refuse, or hide it).
