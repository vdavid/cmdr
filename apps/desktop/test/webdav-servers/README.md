# The WebDAV fixture stack

Three servers in Docker: two Apache httpd, one per auth scheme a WebDAV client has to answer, and one real Nextcloud for
the questions no `mod_dav` can settle. `crates/cmdr-webdav`'s Docker cells and the app's WebDAV suite both talk to
these.

```bash
./start.sh            # core: both httpd servers the integration lane uses
./start.sh minimal    # the Basic-auth server alone
./start.sh nextcloud  # the sabre/dav server alone (slow: it installs itself on first boot)
./stop.sh             # releases this shell's lease; downs only at zero holders
```

`pnpm check` brings the stack up on its own (`desktop-rust-integration-tests` declares `core`,
`desktop-rust-webdav-nextcloud` declares `nextcloud`), so a manual `start.sh` is for iterating by hand.

## What differs from the SFTP stack next door

- **Same shape, smaller.** First-party compose file in this directory, one env-driven image under `image/` for the two
  httpd servers plus `image-nextcloud/` for the sabre/dav one, both rebuilt on every bring-up (the lease declares both
  as build contexts, the way SFTP declares its one).
- **Ports are 13480+**, this stack's own range: SFTP owns 12480+, SMB's vendored consumer stack owns 11480+, and
  `smb2`'s own harness defaults to 10480+. Two stacks sharing a range made them mutually exclusive on one machine.
- **Its own lease namespace** (`/tmp/cmdr-webdav.lock`, `/tmp/cmdr-webdav-leases`), so downing one stack at zero holders
  can never touch another. The model: `scripts/check/DETAILS.md` § "Two fixture stacks, two lease namespaces".
- **No host state.** HTTP has no key pair to publish, so nothing bind-mounts a machine-wide directory and there is no
  keys-dir agreement to guard. Credentials are generated inside the container at start.

## The servers

| Service                    | Port  | What it's for                                                               |
| -------------------------- | ----- | --------------------------------------------------------------------------- |
| `webdav-fixture-apache`    | 13480 | Stock `mod_dav` behind Basic auth: the default target for every cell        |
| `webdav-fixture-digest`    | 13481 | `AuthType Digest` only, realm `cmdr`: proves the client refuses with a type |
| `webdav-fixture-nextcloud` | 13482 | A real sabre/dav server: `Range`, a chunked PUT, and RFC 4331 quota         |

The two httpd servers run as `ada` / `openthedoor` and export `/srv/data` at the URL path `/dav/`. Both carry the same
landmarks (`hello.txt`, `docs/` holding a `readme.md`, `nested/deep/file.txt`, `many/` with 300 entries, `empty/`,
`naïve name.txt`, `photos/2024 summer/`, `large.bin`), so a cell can assert on them whichever server it's pointed at.

`large.bin` is the byte path's file: 4 MiB by default (`LARGE_MB`), and every 16-byte line in it holds its own line
number, so each position says where it belongs. That's what lets a cell assert byte-exactness without shipping a copy of
the file. `cmdr_webdav::volume::testing::fixture_large_bytes` regenerates the expectation.

The export is writable by the httpd user, and Apache answers GET with `Content-Length` and honours `Range` natively,
which is what the streaming and `read_range` cells lean on. `DavDepthInfinity On` allows a whole-tree PROPFIND;
`DavMinTimeout 600` keeps a lock a cell takes alive past the cell.

## Where the ports bind

Every mapping carries a `${WEBDAV_BIND_ADDR:-127.0.0.1}` prefix, so the stack answers on loopback and nowhere else.
Docker's default is `0.0.0.0`, which would put a writable DAV export on the LAN and the tailnet of whoever runs the
suite, and the credentials above are in a public repo. The lease model makes these containers outlive the run that
started them, which is what turns a few seconds of exposure into hours of it.

Set `WEBDAV_BIND_ADDR=0.0.0.0` to reach the fixtures from a NAT'd VM or a second machine, which hit the host by gateway
IP and can't see loopback. Nothing in the check runner sets the variable; `TestWebdavFixturePortsBindToLoopback` fails
the run if a `ports:` entry loses the prefix.

## The Nextcloud server

`webdav-fixture-nextcloud` is the stack's only non-Apache service, and the only one outside `core`. It exists because
three claims in `crates/cmdr-webdav/DETAILS.md` are about REAL servers, and no `mod_dav` can settle any of them: it
honours `Range` natively and omits the quota properties entirely. `crates/cmdr-webdav/src/volume/nextcloud_test.rs`
holds the cells; the answers they observed are anchored in that `DETAILS.md`.

- **Not in `core`, on purpose.** The image is ~1 GB against httpd's ~60 MB, and first boot runs the whole Nextcloud
  install before it binds a port (~25 s on a warm laptop). A default `pnpm check` never pays for it: the lane is
  `desktop-rust-webdav-nextcloud`, which is slow-lane, so `--include-slow`, `pnpm check webdav-nextcloud`, or CI's own
  step is what runs it. The port staying unbound IS the install still running, which is why `start.sh` gives this one
  service a 300 s budget where httpd gets 120 s.
- **Two accounts, and both are load-bearing.** `ada` / `openthedoor` carries a 5 GiB quota; `grace` / `openthedoor`
  keeps the stock unlimited one. Nextcloud answers `quota-available-bytes` with a real number for the first and with the
  negative sentinel `-3` for the second, which the backend reads as a free-space figure and as `NotSupported`
  respectively. One account each is what makes both observable.
- **`image-nextcloud/post-install.sh` provisions them**, from inside the official image's post-installation hook. ❗
  That timing is the whole reason it is a hook and not a script anyone runs afterwards: a quota set through `occ`
  against a LIVE server stays invisible to WebDAV until the process restarts, so provisioning after the fact would leave
  the quota cell reading `-3` forever. The hook also disables `password_policy`, which otherwise refuses `openthedoor`
  ("present in compromised password list") and the fixture password is public on purpose.
- **The base URL is the legacy `/remote.php/webdav/`**, not `/remote.php/dav/files/<user>/`, so one URL serves both
  accounts. The three properties under test answer identically on both endpoints (verified on `nextcloud:34.0.2-apache`,
  by hand with `curl`, 2026-09-02).
- **SQLite, so the fixture is one container.** Nothing these cells read reaches the storage engine.

## Against a server of your own

Set `CMDR_WEBDAV_TEST_URL` and the whole crate suite talks to a server you name instead of Docker: a Nextcloud, a
Synology, a Fastmail account. Nothing else changes, and nothing is set in CI or by the check runner, so leaving them
unset is exactly today's behaviour.

```bash
export CMDR_WEBDAV_TEST_URL=https://cloud.example.com/remote.php/dav/files/you/
export CMDR_WEBDAV_TEST_USERNAME=you
read -rs CMDR_WEBDAV_TEST_PASSWORD && export CMDR_WEBDAV_TEST_PASSWORD   # never on a command line
export CMDR_WEBDAV_TEST_ROOT=/cmdr-scratch                               # optional; defaults to the whole account
cargo nextest run -p cmdr-webdav --run-ignored only
```

- ❗ **The write cells write.** They create `cmdr-test-<pid>-<n>/` at the root you name, fill it, and remove it again.
  Point `CMDR_WEBDAV_TEST_ROOT` at a directory you would not miss.
- ❗ **The password is read from the environment and handed to the credential store.** Use the `read -rs` form above
  rather than an inline assignment, which lands in shell history.
- `CMDR_WEBDAV_TEST_URL` overrides the service argument entirely, so every cell lands on the one server: pointing the
  suite at your Nextcloud does NOT reach the fixture's Digest or Apache server.

**Cells that opt out, and why.** Each says so on stderr and returns; run nextest with `--success-output immediate` to
watch them go by. They need something only the seeded fixture has:

- `the_root_listing_tells_files_from_directories_and_knows_sizes` — `hello.txt`, `large.bin`, `many/`, `empty/`.
- `a_name_with_spaces_and_utf8_round_trips_through_every_verb` — `naïve name.txt`, `photos/2024 summer/`.
- `a_whole_file_stream_is_byte_exact_and_knows_its_size_up_front` and
  `a_bounded_range_comes_back_exactly_and_never_over_long` — the seeded `large.bin` and its 4 MiB size.
- `a_reconnect_against_a_live_server_succeeds_and_keeps_listing` — `hello.txt` and `docs/`.
- `a_digest_only_server_is_a_typed_refusal` — a server that offers no Basic scheme, which is `webdav-fixture-digest`
  and nothing else.
- `quota_reports_the_accounts_own_numbers_not_the_servers_disk` and
  `an_account_with_no_quota_reports_no_free_space_at_all` — the Nextcloud fixture's two accounts and its exact 5 GiB
  quota.

Everything else is honest anywhere: the seven conformance cells, the write and rename and copy and delete cells, the
cancellation cells, and the two credential refusals all build their own scratch directory or assert something
client-side. So do the two sabre/dav cells worth aiming at someone else's server, which is rather the point of aiming
it: `a_ranged_get_is_answered_with_a_window_rather_than_the_whole_file` and
`a_put_with_no_content_length_is_accepted_rather_than_refused` ask a Synology or a proxied Nextcloud exactly what this
fixture asked its own.

## One image, env-driven

`image/` builds both httpd servers. `AUTH=basic|digest` picks the scheme; everything else is identical, so a scheme is a
compose line rather than a second Dockerfile to keep in sync. The entrypoint uncomments the DAV and Digest modules in
the stock `httpd.conf`, writes the credentials (`htpasswd` for Basic; the digest line is
`user:realm:MD5(user:realm:password)`, computed by hand because `htdigest` is interactive), and includes one
`webdav-fixture.conf` for the export.

## Adding a server

1. Add the service here, prefixed `webdav-fixture-`, on the next free port, publishing it as
   `'${WEBDAV_BIND_ADDR:-127.0.0.1}:${WEBDAV_FIXTURE_<NAME>_PORT:-<port>}:80'`. ❗ Two load-bearing parts: the service
   prefix, because `desktop-fixture-lane-coverage` identifies a WebDAV cell by an `#[ignore]` reason naming
   `webdav-servers/start.sh` or `webdav-fixture`; and the bind prefix, without which Docker publishes on every interface
   (see § Where the ports bind).
2. Add it to the right mode in `start.sh`'s case table **and** to `modeServices` in
   `scripts/check/stacklease/registry.go`. Those two lists have to agree, or a cell ends up with no server;
   `TestWebdavModeServicesAgree` is what says so.
3. Add its port to `webdavServiceHostPorts` in `scripts/check/checks/webdav_ports.go`, and its key to the mode list
   there that matches the mode you put it in (`webdavCoreServices` or `webdavNextcloudServices`); each lane's wait guard
   derives from those. `TestWebdavFixturePortsMatchComposeDefaults` fails on a service whose compose default and table
   entry disagree, and on one the table forgot entirely.
4. If it builds a first-party image of its own, add that context to `buildContextsRel` on the `WEBDAV` stack. Without
   it, `up` never rebuilds the image and an edit to the Dockerfile or its scripts never reaches a running container.
