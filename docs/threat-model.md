# Threat model

The whole-app threat model for Cmdr: what we protect, from whom, where the trust boundaries run, what stops each threat
today, and what risk we knowingly carry. It's written for two readers: an enterprise security reviewer, and an agent
changing code that sits on a boundary below.

This is a map. Each mechanism lives in exactly one doc (usually the `DETAILS.md` beside the code), and this file points
there instead of restating it. Per-area privacy and hardening decisions are in `docs/security.md`; the reviewer-facing
summary is the website's `/trust` page (`apps/website/src/lib/trust.ts`); the disclosure policy is the repo-root
`SECURITY.md`.

Verified against the code at `6b4bd4a2c` on 2026-10-05, by reading the code paths named below (not the docs alone); the
security fixes, fuzzing, release environment, and managed-preferences mitigations re-checked at `218426862` the same
day.

❗ This repo is public. Gaps an attacker could use before they're fixed are tracked in a private tracker, not here. This
file names the boundary and says "tracked privately"; don't add exploit detail. A newly found gap goes to the private
tracker first.

## How to use this

- **Changing code at a boundary below?** Read that boundary's section first. If your change adds an entry point, widens
  what crosses the boundary, or removes a mitigation listed here, update the section in the same commit.
- **Adding a new boundary** (a new network backend, a new IPC surface, a new outbound host, a new parser of untrusted
  bytes): add a section with the same four parts (entry points, threats, mitigations, residual risk), and re-rank the
  list at the end.
- **Closing a residual risk?** Move it to mitigations, and update `/trust` (`trust.ts`'s `notInPlaceYet`) if it's listed
  there.

## Security posture in one paragraph

Cmdr is a native macOS app (Tauri 2: a Rust core plus a WKWebView UI) that runs as the logged-in user. It isn't App
Sandboxed, because a file manager's job is to reach every file the user can, and most users grant it Full Disk Access.
Release builds are Developer ID signed, notarized, and run under the hardened runtime with an empty entitlements file
(`docs/security.md` § Entitlements). So the realistic attacker is someone who controls input Cmdr reads (a file, an
archive, a server, a device, a web response, an AI reply) or a link in the delivery chain (dependencies, CI, the update
channel, the maintainer's accounts). An attacker already running code as the user is out of scope: they can already do
whatever the user can (the Scope section of the repo-root `SECURITY.md`).

## Assets

Ranked by what losing them costs a user.

1. **The user's files**, local and remote: integrity (no silent loss or corruption, no writes outside where the user
   aimed) and confidentiality (nothing leaves the Mac unless the user sends it). With Full Disk Access this is the whole
   home folder.
2. **Credentials**: SMB, SFTP, WebDAV, and S3 passwords and keys, and the user's cloud AI API key. All live in the macOS
   Keychain (`apps/desktop/src-tauri/src/secrets/CLAUDE.md`); SFTP can also use the user's SSH agent and key files.
3. **The code that runs**: the signed app bundle, the update chain that replaces it (minisign key, Apple Developer ID
   certificate, notarization credentials, the release workflow, `latest.json`), and helper binaries the app starts
   (`llama-server`, `adb`).
4. **Derived data on disk**: the drive index, media index (OCR text and image tags), folder-importance visits, the
   operation log (a map of the user's file activity), Ask Cmdr chats and memory, and logs. All local, none encrypted by
   Cmdr (FileVault is the layer).
5. **Data that leaves the Mac**: crash reports, error-report bundles, usage stats, feedback, and what cloud AI sends to
   the user's provider. Every outbound connection is listed on `/trust` (`networkConnections` in `trust.ts`).
6. **The license system**: the production Ed25519 license-signing key and the license ledger (`docs/security.md` §
   License signing keys).
7. **The backend and its data**: `api.getcmdr.com` (Cloudflare Worker, D1, R2, KV), its secrets, and the subprocessors
   it forwards to (`trust.ts`'s `dataLocations`).

## Actors

- **The local user.** Trusted. Cmdr protects their data from mistakes (confirmations, temp+rename writes, rollback) and
  never from themselves.
- **Other processes as the same user.** Out of scope as an attacker (they already hold the user's power), but we still
  avoid handing them anything for free: no unauthenticated control channels, no plaintext secrets on disk.
- **Other local users and processes on the same Mac.** Can reach loopback ports. In scope for loopback servers.
- **A malicious file, archive, git repo, or file name.** Arrives by download, USB drive, email, or a shared folder. In
  scope: Cmdr lists, previews, indexes, OCRs, and extracts these, often in the background.
- **A hostile server or device**: an SMB, SFTP, WebDAV, or S3 endpoint, an Android phone over ADB or MTP, or a spoofed
  Bonjour (mDNS) answer on the LAN. In scope: everything such a peer sends is untrusted.
- **A network attacker**: coffee-shop Wi-Fi, a hostile LAN, or a corporate TLS-inspecting proxy (trusted by the
  organization, but still a party that sees traffic).
- **A cloud AI provider, or text that steers it.** The provider is chosen and trusted by the user for what they send;
  prompt injection through file names and contents is in scope.
- **A compromised update channel or maintainer account**: the GitHub account, the release workflow, the website host
  serving `latest.json`, or a signing key.
- **The backend and its subprocessors** (Cloudflare, PostHog, GitHub, Discord, Resend, Google, AWS SES, Paddle,
  Hetzner): hold report and telemetry data, never file contents.
- **A malicious dependency or contributor.** One maintainer, AI-assisted development, outside PRs accepted
  (`docs/guides/handling-prs.md`).

## Trust boundaries

Each section: entry points, threats (STRIDE as a lens where it helps), current mitigations with pointers, and residual
risk.

### 1. The webview and the Rust core (IPC)

- **Entry points**: every Tauri command in `generate_handler![]`, callable from every window; the `cmdr-media:` URI
  scheme; the window's CSP.
- **Threats**: script injection into a webview (a file name, an error message, or an AI reply rendered as HTML) that
  then calls privileged commands (elevation of privilege); a command that leaks a secret into the webview (information
  disclosure).
- **Mitigations**:
  - The CSP allows scripts only from the app bundle (`script-src 'self'`, no `unsafe-inline`), allows no frames, and
    lets the page fetch only itself and `getcmdr.com` (`apps/desktop/src-tauri/tauri.conf.json`). No window loads a
    remote URL.
  - `withGlobalTauri` is off in release, and the dev-only MCP bridge plugin is compiled out (`docs/security.md` §
    withGlobalTauri).
  - Svelte escapes text by default. The few `{@html}` sinks escape their inputs first, and the escaper is the named XSS
    boundary (`apps/desktop/src/lib/error-messages/CLAUDE.md`, `apps/desktop/src/lib/ask-cmdr/CLAUDE.md`).
  - `cmdr-media:` serves only files the viewer or thumbnail code registered, behind a random token. No path appears in
    the URL (`apps/desktop/src-tauri/src/file_viewer/CLAUDE.md`).
  - No command returns a stored API key or password (`docs/security.md` § AI API keys).
  - Why there's no per-window command allowlist, and what would overturn that: `docs/security.md` § Why there's no
    caller-window authorization guard.
- **Residual risk**: a missed escape at a new `{@html}` site in the main window would hand script full IPC. Low
  likelihood (a small, named set of sinks), high impact.

### 2. Other local processes

- **Entry points**:
  - **The Cmdr MCP server** (for AI assistants driving the app): HTTP on `127.0.0.1`.
  - **`llama-server`** (local AI): a child process on a loopback port.
  - **Apple Events**: "open documents" and "Reveal in Cmdr" (`NSFileViewer`).
  - **The Services menu, drag and drop in, and the Dock menu.**
  - **The helper binaries Cmdr starts**: `adb` when no ADB server runs, `llama-server`, `open`, and `osascript`.
- **Threats**: another process (or a web page through DNS rebinding) driving Cmdr to copy, delete, or exfiltrate files;
  command injection through a crafted path; a planted helper binary.
- **Mitigations**:
  - **MCP server**: off by default in release builds and turned on by a developer setting. Once on, it binds loopback
    only, requires a per-start random bearer token on every request (compared in constant time), stores that token in an
    owner-only file, and refuses a non-localhost `Origin`. The token is what stops DNS rebinding. Mechanism:
    `apps/desktop/src-tauri/src/mcp/DETAILS.md` § Authentication.
  - **Apple Events** only navigate: an incoming file URL raises the window and moves the cursor, nothing more
    (`apps/desktop/src-tauri/src/reveal/CLAUDE.md`). Cmdr registers no URL scheme and no Service of its own.
  - **Drag and drop in** opens the normal transfer confirmation.
  - **Process launches pass paths as argv**, never through a shell string. AppleScript takes paths through `on run argv`
    or `quoted form of`.
- **Residual risk**:
  - With MCP turned on, a holder of the token can run destructive tools without a dialog; that's the feature, for the
    developer who turned it on.
  - `llama-server` has no API key, so any local process or user can run inference on it. It holds no user data beyond
    the current request. Low.
  - Keychain items use the default access control, so the user's other processes are trusted the way macOS trusts them.
    Accepted (same-user scope).

### 3. Untrusted file content

- **Entry points**: listings (file names), the file viewer, Quick Look, `inspect_file` (text, PDF, archive listings,
  EXIF), archive browse and extract (zip, tar, 7z), image dimension and EXIF reads, the media index (ImageIO decode,
  Vision OCR and tags), the git status columns and the `.git` portal, and the drive indexer walking everything.
- **Threats**: memory-safety bugs in parsers (tampering, elevation); path traversal or symlink tricks writing outside
  the target folder on extract or copy; decompression bombs and pathological inputs (denial of service); terminal or log
  injection through control characters in names; repository content that makes a git tool run code.
- **Mitigations**:
  - **Memory safety by language**: parsers in Cmdr's process are Rust (`rc-zip`, `zip`, `tar`, `sevenz-rust2`,
    `pdf-extract`/`lopdf`, `image` headers only, `kamadak-exif`, `gix`). Heavy decoding of images and previews runs in
    Apple frameworks, and Quick Look and Vision run out of process.
  - **Archive paths**: one choke point sanitizes every entry name when the archive is indexed (no `..`, absolute paths
    clamped, depth capped), entry counts are capped, symlinks and hardlinks are never recreated as links, setuid bits
    are stripped, and decompression streams in small chunks (`crates/cmdr-archive/CLAUDE.md`).
  - **PDF parsing** in `inspect_file` runs under panic containment with size and time caps (`docs/security.md` § Cloud
    AI egress, the `inspect_file` item).
  - **Copy and delete** don't descend into symlinks, copy a link as a link, and write through temp+rename
    (`apps/desktop/src-tauri/src/file_system/write_operations/DETAILS.md`).
  - **Git**: read-only by construction. Cmdr never fetches, pushes, or runs hooks, and strips a repo's `filter` config
    before reading status, so a repo's own commands never run (`crates/cmdr-git/CLAUDE.md`).
  - **Size caps before allocating**: the xz dictionary an archive asks for is capped, and a PDF page whose `Parent`
    chain loops reads as unparseable instead of recursing (`crates/cmdr-archive/src/read/DETAILS.md` § Resource caps,
    `apps/desktop/src-tauri/src/agent/tools/DETAILS.md`).
  - **Fuzzing**: fuzz targets cover archive names and indexes, PDF, image headers, and the S3, WebDAV, and ADB wire
    parsers, on demand (`pnpm check fuzz`) and in the slow CI lane every six days (`fuzz/DETAILS.md`, which also lists
    the findings and the one open upstream issue).
- **Residual risk**:
  - **Parsers run in Cmdr's process, unsandboxed, with Full Disk Access.** A memory-safety bug in a native dependency or
    Apple framework (ImageIO is a classic target) reached through a downloaded file would run with the user's full file
    access. Low likelihood, high impact; the main mitigation is staying current on macOS and dependencies.
  - **Decompression bombs** are bounded by disk space and cancelation, not by a ratio cap. Low impact.
  - **File names aren't stripped of control and bidi characters**, so a name can look like something else in a log line
    or a confirmation. Low.
  - Some gaps in hostile-content handling are tracked privately.

### 4. Remote servers and devices

- **Entry points**: SMB (the in-house `smb2` crate, plus the OS mount), SFTP (`russh`), WebDAV (`reqwest`, `quick-xml`),
  S3 (in-house SigV4 over `reqwest`), ADB (the ADB server on loopback, which relays the phone), MTP (the in-house
  `mtp-rs` over USB), and Bonjour discovery of SMB servers.
- **Threats**: credential theft (a spoofed server or a plaintext protocol); malformed protocol messages (memory
  exhaustion, parser bugs); remote file names that escape the destination folder on copy; a server that lies about sizes
  or contents; a hostile device on a USB port.
- **Mitigations**:
  - **SMB**: NTLMv2 auth, signing whenever the session is authenticated or the server asks, unsigned replies on a signed
    session rejected, SMB 3.1.1 pre-auth integrity and a signed final session setup against downgrade, a guest session
    refused when a named login was asked for, and a frame size cap. Credentials in the Keychain, keyed by server
    (`crates/cmdr-smb/CLAUDE.md`, `apps/desktop/src-tauri/src/network/CLAUDE.md`).
  - **SFTP**: trust on first use with a prompt, a changed host key stops the connection, a revoked key is a hard stop,
    and `~/.ssh/known_hosts` is read but never written. No agent forwarding (`crates/cmdr-sftp/DETAILS.md` § Host-key
    trust, § The auth ladder).
  - **WebDAV and S3**: TLS against the system trust store, redirects off so credentials never follow a cross-origin hop,
    and S3 requests SigV4-signed (`crates/cmdr-webdav/CLAUDE.md`, `crates/cmdr-s3/CLAUDE.md`).
  - **ADB and MTP**: each can be turned off in Settings > File systems; ADB talks only to a loopback server.
  - **Peer-chosen lengths are capped before allocating**: ADB sync payloads and shell frames, WebDAV PROPFIND bodies,
    and S3 answer bodies (`crates/cmdr-adb/DETAILS.md`, `crates/cmdr-webdav/DETAILS.md` § Bounded bodies,
    `crates/cmdr-s3/DETAILS.md` § Responses).
  - **A listed name can't escape the destination**: every cross-volume copy, move, and drag-out joins a name only as a
    `ChildName` (one plain path component), whatever the backend
    (`apps/desktop/src-tauri/src/file_system/write_operations/transfer/volume/DETAILS.md` § "Listed names are
    untrusted").
  - **Typed error policy per backend**, so a server's error text never decides control flow (`AGENTS.md` § Hard rules).
- **Residual risk**:
  - **Plaintext is the user's call**: WebDAV and custom S3 endpoints accept `http://`, which sends WebDAV's Basic auth
    in the clear. SMB encryption is used when the server or share asks, and not demanded otherwise; SMB 2.0.2 and 2.1
    are still offered for old NAS boxes. On a hostile LAN this exposes credentials or file traffic. Medium.
  - **A spoofed Bonjour answer** can impersonate a saved SMB server name and receive an NTLMv2 response that can be
    cracked offline. Medium likelihood on untrusted networks, impact bounded by password strength.
  - **No Kerberos**, so domain-joined shares use NTLMv2 with stored passwords.
  - Some gaps in hostile-server and hostile-device input handling are tracked privately.

### 5. The network path

- **Entry points**: every HTTPS call (`trust.ts`'s `networkConnections`), plus the protocols in boundary 4.
- **Threats**: interception or tampering on the wire; a TLS-inspecting proxy seeing traffic; a proxy-only network
  blocking updates.
- **Mitigations**:
  - Every HTTP client is `reqwest` on `rustls` with `rustls-platform-verifier`, so certificates are checked against the
    macOS trust store, including a CA an organization deploys by MDM. Nothing disables certificate checks.
  - Update archives and licenses are signed, so a network attacker who breaks TLS still can't forge them (boundaries 6
    and 7).
- **Residual risk**:
  - **A TLS-inspecting proxy sees everything** that crosses it, by design of the organization: report bundles, usage
    stats, and cloud AI requests including file names and requested file contents. Organizations that inspect TLS should
    treat cloud AI as visible to the proxy.
  - **macOS system proxy settings and PAC files aren't read**; only the `HTTP(S)_PROXY` environment variables are. On a
    proxy-only network, Cmdr's own calls fail rather than leak. Availability, not confidentiality. Already listed on
    `/trust`.

### 6. The update channel and the release pipeline

- **Entry points**: `api.getcmdr.com/update-check` redirecting to `getcmdr.com/latest.json`; the archive download from
  GitHub Releases; the in-place bundle sync on macOS; the release workflow on GitHub Actions; the maintainer's GitHub
  account and laptop.
- **Threats**: a forged or replayed update (tampering, elevation to every install); a stolen signing key; a compromised
  CI run or third-party action; a compromised maintainer account.
- **Mitigations**:
  - **Every update archive is verified against a minisign public key compiled into the app**, on the downloaded bytes,
    before anything is written into the bundle. The manifest must name a newer version, the verified archive's own
    `Info.plist` must too (so an older signed release can't be replayed as an update), and the release workflow refuses
    to move the published manifest backwards (`apps/desktop/src-tauri/src/updater/CLAUDE.md`,
    `apps/desktop/src/lib/updates/DETAILS.md`).
  - **Release builds are Developer ID signed and notarized** (`docs/guides/apple-signing-and-notarization.md`).
  - **The release pipeline**: release tags are SSH-signed and verified against `.github/release-signers` before a build
    starts; only admins can create `v*` tags (a repository ruleset); the only job that signs runs in a `release`
    environment that only `v*` tags can deploy to (all eight signing secrets are in it; their repo-level copies stay
    until the first release through the environment succeeds, so until then a branch workflow could still read them:
    `docs/guides/releasing.md` § Signing secrets); a release waits for a full, green CI run of its commit; every
    third-party action is pinned to a commit SHA; each release publishes SHA-256 checksums, SLSA build provenance, and
    signed SBOMs (`docs/guides/releasing.md`).
  - **The updater key signs on the maintainer's laptop, not in CI**, from the first release in `local` signing mode
    (`docs/guides/releasing.md` § Who signs the update archives). CI builds, notarizes, and attests into a draft
    release; the laptop verifies each update archive's build provenance (the release workflow, at the signed tag's
    commit, on a GitHub-hosted runner), refuses anything that doesn't verify, signs with the key from its encrypted
    store, checks each signature against the app's public key, and only then lets CI publish. The key's GitHub copies
    are deleted once that first release ships, which is when a GitHub or CI compromise stops being able to read it.
  - **Account and key custody**: hardware-key 2FA on the GitHub account, and encrypted copies of every signing key
    outside GitHub and Cloudflare.
- **Residual risk**:
  - **One maintainer account can ship to every install**, and updates install automatically (unless an organization
    turns updates off or sets a version ceiling through managed preferences). The controls above raise the bar, but
    there's no second-person approval. High impact, low likelihood. Already listed on `/trust`.
  - **The tag-signature check catches mistakes, not someone who already holds tag rights.** A tag push runs the workflow
    and reads `.github/release-signers` from the tagged commit itself, so whoever can push a `v*` tag controls both.
    What stops an attacker is who holds that right (admins only, behind hardware-key 2FA). Stronger options (a required
    approval on the `release` environment, or a deployment-protection rule that verifies the signature outside the
    tagged commit) are tracked privately.
  - **No reproducible builds**, so a reviewer can't rebuild and compare a release.
  - **A malicious build from inside the release pipeline still gets signed.** Signing locally checks where an archive
    was built, not what went into it, so a compromised third-party action or dependency in the build job still ships to
    every install. What local signing removes is the key itself leaking from GitHub, and an archive swapped on the
    release.
  - **The minisign updater key can't be rotated** without a release signed by the old key; losing it strands installs on
    manual reinstall. With local signing it lives on one laptop (encrypted, with Bitwarden copies), so a compromise of
    that laptop could sign anything.
  - Some release-chain hardening is tracked privately.

### 7. Licensing

- **Entry points**: the license key the user enters; `api.getcmdr.com/activate` and `/validate`; the cached license
  status on disk.
- **Threats**: forged licenses (the production signing key leaking), a forged or replayed "revoked" answer (a
  TLS-intercepting proxy, or whoever holds a lapsed domain), and tampering with local status.
- **Mitigations**: Ed25519 signatures checked offline against a public key per build mode; the production private key
  exists only as a Cloudflare Worker secret (`docs/security.md` § License signing keys,
  `apps/desktop/src-tauri/src/licensing/DETAILS.md`). `/validate` answers are signed with the same key over the app's
  fresh nonce and the transaction id, and the app ignores anything else, so only our server can revoke a license
  (`apps/desktop/src-tauri/src/licensing/DETAILS.md` § Signed validation answers). `/activate` and `/validate` are
  rate-limited per IP, so short codes can't be guessed at speed (`apps/api-server/src/licensing/DETAILS.md`).
- **Residual risk**: a leaked production key mints licenses every shipped build accepts, and revocation needs a new
  binary. Because unreachability never downgrades a license, a revoked or refunded key stays Commercial on any Mac that
  can't (or is blocked from) reaching `api.getcmdr.com`. The license gates no feature (it decides which reminder a user
  sees), so both that and local tampering are accepted as having no security impact.

### 8. Cloud AI providers and prompt injection

- **Entry points**: the user's configured provider and base URL; every untrusted string that reaches the model (file
  names, `inspect_file` text, PDF pages, archive entry names, EXIF, OCR text); the model's replies, tool calls, and
  memory writes.
- **Threats**: data sent to a third party (disclosure); a key harvested by a malicious base URL; injected instructions
  steering the agent to propose harmful operations, to read and send sensitive content, or to plant persistent
  instructions in its memory.
- **Mitigations**: one fail-closed, versioned consent gate in the backend before any cloud request; the key never
  returns to a webview; plaintext `http://` refused for a non-loopback base URL; the agent can read and propose but
  never write files, and only the user approves a proposal; the memory folder is jailed, capped, and fenced in the
  prompt; no tool returns raw file bytes (`docs/security.md` § Cloud AI egress, § AI API keys).
- **Residual risk**:
  - **Prompt injection is mitigated, not solved.** Injected text can make the agent read more than the user meant (and
    so send it to the provider), propose a harmful operation (which the user still has to approve), or save a note that
    rides along on every later turn. Medium likelihood, medium impact. The approval step is the hard boundary.
  - **What reaches the provider is governed by the provider's terms**, not Cmdr's. An organization can turn AI off,
    allow only on-device AI, or allow only listed cloud hosts through managed preferences, enforced in the backend at
    every request and redirect hop (`apps/desktop/src-tauri/src/managed_policy/DETAILS.md`).

### 9. The backend, report data, and subprocessors

- **Entry points**: public endpoints on `api.getcmdr.com` (heartbeat, crash, error report, feedback, beta signup, update
  check, license activate and validate, S3 prices, downloads); the Paddle and GitHub webhooks; the admin endpoints the
  private dashboard reads.
- **Threats**: personal data in reports (disclosure); abuse of public endpoints (spam, storage exhaustion); webhook
  forgery; admin endpoint compromise; a subprocessor breach.
- **Mitigations**:
  - **On the Mac**: report bundles are redacted before they leave, and usage stats carry a random install id that can't
    be joined to diagnostics (`docs/security.md` § Error reports, `apps/desktop/src-tauri/src/analytics/CLAUDE.md`).
  - **On the server**: per-IP rate limits and body caps on the report endpoints; webhooks are HMAC-verified, and a
    Paddle webhook signed more than five minutes off is refused, so a captured one can't be replayed later; admin
    endpoints need a bearer token compared in constant time; error-report storage is size-capped; retention sweeps run
    daily (`apps/api-server/DETAILS.md`).
  - **Toward subprocessors**: Discord never gets an email address, and error-report links expire after 24 hours
    (`docs/security.md` § Discord deep links).
- **Residual risk**:
  - **The redactor is path-shaped**: it covers every path and a name repeated on its path's line, but a name logged with
    no path on its line can pass (`apps/desktop/src-tauri/src/redact/DETAILS.md` § "Names in prose"). Listed on
    `/trust`.
  - **Usage stats are opt-out during the beta**, and their settings list is kept clean by review, not a filter.
  - **Data leaves the EU** for several subprocessors (`trust.ts`'s `dataLocations`).
  - Some server-side hardening is tracked privately.

### 10. Dependencies and contributors

- **Entry points**: crates.io, npm, Go modules, GitHub Actions, and outside pull requests.
- **Threats**: a typosquatted or hijacked package; a malicious update to a real one; a malicious PR.
- **Mitigations**: Renovate and pnpm hold every new release for three days; `cargo deny` allows only crates.io as a
  source and fails on new advisories; actions are pinned by SHA; GitGuardian scans for secrets (`docs/security.md` §
  Secret scanning); the check suite runs on every change (`docs/guides/update-dependencies.md`,
  `docs/guides/add-rust-dependency.md`).
- **Residual risk**:
  - **No second-person review**: a malicious change has to get past the maintainer and the automated checks only.
  - **The maintainer's own crates** (`smb2`, `mtp-rs`) skip the three-day hold, so they're as safe as the maintainer's
    crates.io account.
  - **A dependency that's malicious from its first release** gets past the age window; review on adding is the guard
    (`AGENTS.md` § Dependencies).

### 11. Local data at rest

- **What's there**: the indexes, media index (OCR text can be a passport number), the operation log, Ask Cmdr chats and
  memory, logs, and settings, all under the user's app data and log folders.
- **Threats**: disclosure on a stolen or shared Mac, or through backups.
- **Mitigations**: all files sit in the user's own folders with normal user permissions; secrets go to the Keychain,
  never a settings file; none of this is transmitted (`docs/security.md` § Folder-importance visit signal, § Operation
  log).
- **Residual risk**: Cmdr doesn't encrypt them, and they're included in Time Machine backups. FileVault is the
  protection. Listed on `/trust`.

## Residual risks, ranked

Impact × likelihood, highest first. Gaps tracked privately aren't ranked here.

1. **Maintainer account or release chain compromise ships code to every install** (boundary 6). Impact: critical.
   Likelihood: low, after hardware keys, the release environment, signed and protected tags, the CI gate, and (once the
   first `local`-mode release ships) an updater key that no longer lives on GitHub.
2. **A parser bug in a dependency or Apple framework, reached by a downloaded file or a hostile server, runs with Full
   Disk Access** (boundaries 3 and 4). Impact: high. Likelihood: low. Cmdr isn't sandboxed and parses in process.
3. **Prompt injection through file names or contents steers Ask Cmdr** into reading and sending more than intended,
   proposing a harmful operation, or saving a persistent note (boundary 8). Impact: medium (writes still need approval).
   Likelihood: medium, and rising as the agent does more.
4. **Credentials or traffic exposed on a hostile LAN**: user-chosen plaintext WebDAV or S3, optional SMB encryption,
   NTLMv2 to a spoofed Bonjour name (boundary 4). Impact: medium. Likelihood: medium on untrusted networks.
5. **Personal data in reports**: free-text file names in error reports, opt-out usage stats, data leaving the EU
   (boundary 9). Impact: medium (privacy, compliance). Likelihood: medium.
6. **Enterprise network policy isn't honored**: system proxy and PAC settings aren't read (boundary 5). Impact: medium
   (compliance, availability). IT can turn off cloud AI, telemetry, and updates through managed preferences
   (`apps/desktop/src-tauri/src/managed_policy/DETAILS.md`).
7. **A malicious dependency** past the age window and review (boundary 10). Impact: high. Likelihood: low.
8. **Local data at rest isn't encrypted by Cmdr** (boundary 11). Impact: medium. Likelihood: low with FileVault on.
9. **A missed escape at a new `{@html}` site** gives script full IPC (boundary 1). Impact: high. Likelihood: low.
10. **Loopback services are reachable by other local users**: `llama-server` with no key, and MCP when a developer turns
    it on, behind its token (boundary 2). Impact: low. Likelihood: low.
