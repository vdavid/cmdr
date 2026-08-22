# The SFTP fixture stack

Eleven real OpenSSH servers in Docker, one per thing that breaks an SFTP client, plus a twelfth nothing in CI touches.
`crates/cmdr-sftp`'s Docker cells and the app's SFTP suites both talk to these.

```bash
./start.sh          # core: every server the integration lane uses
./start.sh minimal  # the stock server and the key-only one
./start.sh bench    # the local-only measurement server
./stop.sh           # releases this shell's lease; downs only at zero holders
```

`pnpm check` brings the stack up on its own (`desktop-rust-integration-tests` declares it), so a manual `start.sh` is
for iterating by hand.

## What differs from the SMB stack next door

- **First-party.** SMB's compose is vendored from the `smb2` crate and lives under a `.compose/` marker dir with a
  cmdr-owned override layered on top. There's nothing to re-vendor here, so the compose file sits in this directory and
  there is exactly one of it.
- **Ports are 12480+**, this stack's own range: SMB's vendored consumer stack owns 11480+ and `smb2`'s own harness
  defaults to 10480+. Two stacks sharing a range made them mutually exclusive on one machine.
- **Its own lease namespace** (`/tmp/cmdr-sftp.lock`, `/tmp/cmdr-sftp-leases`), so downing one stack at zero holders can
  never touch the other. The model: `scripts/check/DETAILS.md` § "Two fixture stacks, two lease namespaces".

## The servers

| Service                      | Port  | What it's for                                                        |
| ---------------------------- | ----- | -------------------------------------------------------------------- |
| `sftp-fixture-openssh`       | 12480 | Stock OpenSSH: password + key, and it HAS `posix-rename@openssh.com` |
| `sftp-fixture-keyonly`       | 12481 | `PasswordAuthentication no`, so the ladder must reach its key rung   |
| `sftp-fixture-passphrase`    | 12482 | Key-only, and its key is passphrase-protected (`letmein`)            |
| `sftp-fixture-kbdint`        | 12483 | `KbdInteractiveAuthentication yes` over PAM: one hidden prompt       |
| `sftp-fixture-twokeys`       | 12484 | Two host key types on one server, which is a healthy thing to be     |
| `sftp-fixture-changedkey`    | 12485 | A second, deliberately different identity                            |
| `sftp-fixture-noposixrename` | 12486 | No `posix-rename@openssh.com`, no `copy-data`                        |
| `sftp-fixture-shortreads`    | 12487 | Truncates every `SSH_FXP_DATA` to 4 KiB                              |
| `sftp-fixture-smalllimits`   | 12488 | `limits@openssh.com` far stingier than OpenSSH's own                 |
| `sftp-fixture-bigdir`        | 12489 | 5 000 entries in one directory, and a 40-level nest                  |
| `sftp-fixture-oddnames`      | 12490 | Filenames that aren't valid UTF-8, plus awkward ones that are        |
| `sftp-fixture-bench`         | 12491 | 128 MiB export and `NET_ADMIN`, for measuring. ❗ Not in `core`      |

⚠️ **`QUIRK_DROP_EXTENSIONS` matches the name the server actually sends, and `copy-data` has NO `@openssh.com` suffix**
where every other extension in this stack does (OpenSSH `sftp-server.c` 9.9p2, read 2026-08-22). A name that matches
nothing drops nothing, silently, and the fixture then quietly HAS the extension it is named for lacking. That happened;
`crates/cmdr-sftp/src/volume/integration_test.rs`'s `a_server_with_the_extensions_dropped_advertises_neither` is what
caught it and what keeps it caught.

Every server runs as `ada` / `openthedoor` and exports `/srv/data`. Every export carries the same landmarks
(`hello.txt`, `photos/`, `ten-bytes.txt`, `five-bytes.txt`, `empty-dir/`, `full-dir/child.txt`, `large.bin`), so a cell
can assert on them whichever server it's pointed at.

`large.bin` is the byte path's file: 4 MiB by default (`LARGE_MB`), and every 16-byte line in it holds its own line
number, so each position says where it belongs. That's what lets a cell assert byte-exactness without shipping a copy of
the file — a reader that holes or duplicates a span lands bytes whose contents no longer match their offsets.
`cmdr_sftp::volume::testing::fixture_large_bytes` regenerates the expectation.

**The bench server is local only.** It is deliberately outside `core` and outside `sftpServiceHostPorts`: the
integration lane waits on every service in that table, and a throughput number measured under runner contention is a
flake rather than a gate. It carries a 128 MiB `large.bin` and `NET_ADMIN`, so a run can shape its link:

```bash
docker exec sftp-fixture-sftp-fixture-bench-1 tc qdisc replace dev eth0 root netem delay 50ms limit 50000
```

The measurements, the method, and what they set: `crates/cmdr-sftp/DETAILS.md` § "The read window".

## One image, env-driven

`image/` builds all eleven. What differs is environment, so a quirk is a compose line rather than a second Dockerfile to
keep in sync.

- `AUTH`, `HOST_KEYS`, `SEED` shape the server.
- `QUIRK_*` route the SFTP subsystem through `image/sftp-quirk.py`, a byte-level proxy in front of the **real**
  `sftp-server`.

**Why a proxy rather than a different SFTP implementation.** OpenSSH's `sftp-server` has no switches for any of this: it
always advertises `posix-rename`, always answers `limits@openssh.com` with its own numbers, and never short-reads.
Swapping in a third-party server to get those behaviours would mean testing against something users don't run. The proxy
leaves every other byte identical to stock OpenSSH.

## Two settings that look like noise and aren't

Both live in `image/entrypoint.sh`, and both cost hours to rediscover.

- **`PerSourcePenalties no`.** OpenSSH 9.8 blocks a source address for a while after a failed authentication. The cell
  that deliberately signs in with the wrong password would otherwise take every other cell on that server down with it,
  surfacing as an unrelated "Disconnected" in a different test seconds later.
- **`MaxStartups 200:30:400`.** The stock `10:30:100` starts refusing the eleventh unauthenticated connection, and
  nextest runs a whole binary's cells in parallel against one server.

## Keys

The key-auth servers generate a pair at start and write the private half to `.keys/<service>/id_ed25519`, a bind mount
this directory gitignores. ❌ Nothing here is checked in: a private key in a repo is a private key on the internet.

## Host keys are fresh per container

Each server generates its own host keys at start, so identities differ between services and nothing is committed. Suites
approve on first contact the way a user does (`cmdr_sftp::volume::testing::connect_fixture`), and the changed-key cell
reads one server's real fingerprint and offers it against another's address.

## Adding a server

1. Add the service here, prefixed `sftp-fixture-`, on the next free port. ❗ The prefix is load-bearing:
   `desktop-fixture-lane-coverage` identifies an SFTP cell by an `#[ignore]` reason naming `sftp-servers/start.sh` or
   `sftp-fixture`.
2. Add it to `start.sh`'s `core` list **and** to `modeServices` in `scripts/check/stacklease/registry.go`. Those two
   lists have to agree, or a cell ends up with no server.
3. Add its port to `smbServiceHostPorts`' sibling, `sftpServiceHostPorts` in `scripts/check/checks/sftp_ports.go`, and
   to the lane's wait guard in `scripts/check/checks/desktop-rust-integration-tests.go`.
