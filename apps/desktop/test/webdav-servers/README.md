# The WebDAV fixture stack

Two real Apache httpd servers in Docker, one per auth scheme a WebDAV client has to answer. `crates/cmdr-webdav`'s
Docker cells and the app's WebDAV suite both talk to these.

```bash
./start.sh          # core: both servers the integration lane uses
./start.sh minimal  # the Basic-auth server alone
./stop.sh           # releases this shell's lease; downs only at zero holders
```

`pnpm check` brings the stack up on its own (`desktop-rust-integration-tests` declares it), so a manual `start.sh` is
for iterating by hand.

## What differs from the SFTP stack next door

- **Same shape, smaller.** First-party compose file in this directory, one env-driven image under `image/`, rebuilt on
  every bring-up (the lease declares `image/` as its build context, exactly like SFTP).
- **Ports are 13480+**, this stack's own range: SFTP owns 12480+, SMB's vendored consumer stack owns 11480+, and
  `smb2`'s own harness defaults to 10480+. Two stacks sharing a range made them mutually exclusive on one machine.
- **Its own lease namespace** (`/tmp/cmdr-webdav.lock`, `/tmp/cmdr-webdav-leases`), so downing one stack at zero holders
  can never touch another. The model: `scripts/check/DETAILS.md` § "Two fixture stacks, two lease namespaces".
- **No host state.** HTTP has no key pair to publish, so nothing bind-mounts a machine-wide directory and there is no
  keys-dir agreement to guard. Credentials are generated inside the container at start.

## The servers

| Service                 | Port  | What it's for                                                               |
| ----------------------- | ----- | --------------------------------------------------------------------------- |
| `webdav-fixture-apache` | 13480 | Stock `mod_dav` behind Basic auth: the default target for every cell        |
| `webdav-fixture-digest` | 13481 | `AuthType Digest` only, realm `cmdr`: proves the client refuses with a type |

Both run as `ada` / `openthedoor` and export `/srv/data` at the URL path `/dav/`. Both carry the same landmarks
(`hello.txt`, `docs/` holding a `readme.md`, `nested/deep/file.txt`, `many/` with 300 entries, `empty/`,
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

## One image, env-driven

`image/` builds both. `AUTH=basic|digest` picks the scheme; everything else is identical, so a scheme is a compose line
rather than a second Dockerfile to keep in sync. The entrypoint uncomments the DAV and Digest modules in the stock
`httpd.conf`, writes the credentials (`htpasswd` for Basic; the digest line is `user:realm:MD5(user:realm:password)`,
computed by hand because `htdigest` is interactive), and includes one `webdav-fixture.conf` for the export.

## Adding a server

1. Add the service here, prefixed `webdav-fixture-`, on the next free port, publishing it as
   `'${WEBDAV_BIND_ADDR:-127.0.0.1}:${WEBDAV_FIXTURE_<NAME>_PORT:-<port>}:80'`. ❗ Two load-bearing parts: the service
   prefix, because `desktop-fixture-lane-coverage` identifies a WebDAV cell by an `#[ignore]` reason naming
   `webdav-servers/start.sh` or `webdav-fixture`; and the bind prefix, without which Docker publishes on every interface
   (see § Where the ports bind).
2. Add it to `start.sh`'s `core` list **and** to `modeServices` in `scripts/check/stacklease/registry.go`. Those two
   lists have to agree, or a cell ends up with no server.
3. Add its port to `webdavServiceHostPorts` in `scripts/check/checks/webdav_ports.go`; the lane's wait guard in
   `scripts/check/checks/desktop-rust-integration-tests.go` derives from that table.
   `TestWebdavFixturePortsMatchComposeDefaults` fails on a service whose compose default and table entry disagree, and
   on one the table forgot entirely.
