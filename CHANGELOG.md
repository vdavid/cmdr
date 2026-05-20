# Changelog

All notable changes to Cmdr will be documented in this file.

The format is based on [keep a changelog](https://keepachangelog.com/en/1.1.0/), and we use
[Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).

## [0.21.0] - 2026-05-21

### Added

- Add Quick Look (⇧Space) ([6778494b](https://github.com/vdavid/cmdr/commit/6778494b))
- Add ⌘← / ⌘→ to copy the cursor path between panes ([a3e15f45](https://github.com/vdavid/cmdr/commit/a3e15f45))
- Add red binary-file warning in the file viewer ([74e7b0cd](https://github.com/vdavid/cmdr/commit/74e7b0cd))
- Redesign Settings window to look like System Settings ([69480931](https://github.com/vdavid/cmdr/commit/69480931),
  [76be4f8a](https://github.com/vdavid/cmdr/commit/76be4f8a),
  [9668a078](https://github.com/vdavid/cmdr/commit/9668a078),
  [91c31f35](https://github.com/vdavid/cmdr/commit/91c31f35))
- Redesign tab bar, flatten panes for a more native-macOS look, fix UI glitches
  ([dc7d6500](https://github.com/vdavid/cmdr/commit/dc7d6500),
  [9668a078](https://github.com/vdavid/cmdr/commit/9668a078),
  [3771570a](https://github.com/vdavid/cmdr/commit/3771570a),
  [79ed3b6c](https://github.com/vdavid/cmdr/commit/79ed3b6c))

### Fixed

- Fix transfer dialog showing "✓ 0 files" when pre-flight scan beat the FE listeners
  ([8525835c](https://github.com/vdavid/cmdr/commit/8525835c))
- Fix stale path events corrupting the breadcrumb after switching a pane to Network
  ([a3e15f45](https://github.com/vdavid/cmdr/commit/a3e15f45))
- Fix Quick Look toast/content import cycle ([b3d67fe6](https://github.com/vdavid/cmdr/commit/b3d67fe6))

### Non-app

- Move `rust-toolchain.toml` to the workspace root so every crate pins one toolchain (fixes v0.20.0's
  `rustup target add` drift) ([41e999ab](https://github.com/vdavid/cmdr/commit/41e999ab))
- Add `workflows-rustup` check forbidding `rustup target/component add` in workflows
  ([c68630ee](https://github.com/vdavid/cmdr/commit/c68630ee))

## [0.20.0] - 2026-05-20

### Added

- Cmd+click toggles selection ([c6adee74](https://github.com/vdavid/cmdr/commit/c6adee74))
- Bind `Insert` to toggle selection in Total Commander style
  ([719e4f9b](https://github.com/vdavid/cmdr/commit/719e4f9b))
- Modify Shift+Arrow/Page/Home/End behavior to align more with other file managers
  ([47932132](https://github.com/vdavid/cmdr/commit/47932132))
- Switch selection to red. Clears WCAG AA across all backgrounds!
  ([9028722c](https://github.com/vdavid/cmdr/commit/9028722c),
  [02b295da](https://github.com/vdavid/cmdr/commit/02b295da),
  [069bc400](https://github.com/vdavid/cmdr/commit/069bc400),
  [14a36dd8](https://github.com/vdavid/cmdr/commit/14a36dd8))
- Tint each pane's background by volume type (local/SMB/MTP)
  ([3f5629d3](https://github.com/vdavid/cmdr/commit/3f5629d3))
- Improve MCP: replace fire-and-forgets with wound-trips ([48a9701c](https://github.com/vdavid/cmdr/commit/48a9701c),
  [3c1b0dc9](https://github.com/vdavid/cmdr/commit/3c1b0dc9),
  [e12285d1](https://github.com/vdavid/cmdr/commit/e12285d1),
  [df11caef](https://github.com/vdavid/cmdr/commit/df11caef))
- New MCP resources: `cmdr://logs` + filters, `cmdr://state` filters, `recentErrors`, `upgrade_smb_to_direct`, SMB
  connection state ([e597d24d](https://github.com/vdavid/cmdr/commit/e597d24d),
  [640c3330](https://github.com/vdavid/cmdr/commit/640c3330))
- SMB volumes auto-upgrade from OS-mount to direct smb2 sessions
  ([640c3330](https://github.com/vdavid/cmdr/commit/640c3330))
- Propagate MTP cancel all the way to the USB layer; dialog stays at "Cancelling…" until the device settles. No more
  30-second wedges! ([0de4c6b7](https://github.com/vdavid/cmdr/commit/0de4c6b7),
  [1696355d](https://github.com/vdavid/cmdr/commit/1696355d),
  [f894e60e](https://github.com/vdavid/cmdr/commit/f894e60e),
  [b4018891](https://github.com/vdavid/cmdr/commit/b4018891))
- Copy/move/delete pre-flight scans reuse watcher-backed listings. Skip a 17s MTP re-list when the folder is already
  open in another pane! ([9d434638](https://github.com/vdavid/cmdr/commit/9d434638),
  [ba20ca3e](https://github.com/vdavid/cmdr/commit/ba20ca3e),
  [49187230](https://github.com/vdavid/cmdr/commit/49187230),
  [fdebd329](https://github.com/vdavid/cmdr/commit/fdebd329),
  [b90b9003](https://github.com/vdavid/cmdr/commit/b90b9003))
- SMB streaming writes no longer hold the client mutex (smb2 0.9). Concurrent writes pipeline over one session
  ([3d0d5db7](https://github.com/vdavid/cmdr/commit/3d0d5db7),
  [06bc5da7](https://github.com/vdavid/cmdr/commit/06bc5da7),
  [ed4b6886](https://github.com/vdavid/cmdr/commit/ed4b6886))
- Bump SMB watcher to smb2 0.10 to stop losing events between polls
  ([432d13ff](https://github.com/vdavid/cmdr/commit/432d13ff))
- Localize macOS pane names in onboarding and error dialogs (points at what System Settings actually shows)
  ([bad5d926](https://github.com/vdavid/cmdr/commit/bad5d926))
- Coalesce `directory-diff` events with soft-refresh in the FE. No more empty-pane flicker on bulk ops!
  ([54674854](https://github.com/vdavid/cmdr/commit/54674854),
  [13b486a8](https://github.com/vdavid/cmdr/commit/13b486a8))
- Honest transfer-complete toasts: report copied vs skipped separately
  ([5cdf989e](https://github.com/vdavid/cmdr/commit/5cdf989e))
- Friendly message for SMB `STATUS_DELETE_PENDING` (was misleading "disk needs attention")
  ([a560243b](https://github.com/vdavid/cmdr/commit/a560243b))
- Polish the license nudge: clearer copy and layout ([95007952](https://github.com/vdavid/cmdr/commit/95007952))
- Add fallback UI colors on macOS Monterey, achieving macOS 12.x compat!
  ([5792b10e](https://github.com/vdavid/cmdr/commit/5792b10e))
- Improve accent-fg to match WCAG AA+ against all colors, and add cursor outline
  ([d00ba5b4](https://github.com/vdavid/cmdr/commit/d00ba5b4))
- Properly pluralize all words ("1 file"/"10 files") everywhere
  ([eb360370](https://github.com/vdavid/cmdr/commit/eb360370))

### Fixed

- Fix MTP destination pane staying stale after cross-volume writes
  ([873f1102](https://github.com/vdavid/cmdr/commit/873f1102))
- Fix SMB/MTP listing cache going stale when the watcher misses an event
  ([1dea24e1](https://github.com/vdavid/cmdr/commit/1dea24e1),
  [ab98ee88](https://github.com/vdavid/cmdr/commit/ab98ee88))
- Fix MTP delete not emitting `write-cancelled` when cancel landed mid-iteration
  ([e21ca6d3](https://github.com/vdavid/cmdr/commit/e21ca6d3))
- Fix transfer dialog wedging at "Cancelling…" when Cancel raced ahead of the `operationId` IPC
  ([2b2a5ec6](https://github.com/vdavid/cmdr/commit/2b2a5ec6))
- Fix MCP `open_under_cursor` on the Network view ([0aec8fbd](https://github.com/vdavid/cmdr/commit/0aec8fbd))
- Fix Linux startup hanging on a half-configured D-Bus (probes now bounded by a 500 ms timeout)
  ([91afacbf](https://github.com/vdavid/cmdr/commit/91afacbf),
  [85580df9](https://github.com/vdavid/cmdr/commit/85580df9))
- Fix `refresh_listing` short-circuiting on local volumes during the FSEvents symlink race
  ([57ef1034](https://github.com/vdavid/cmdr/commit/57ef1034))
- Fix two SMB shares with the same case-folded name on different servers colliding on the same volume ID
  ([f2414556](https://github.com/vdavid/cmdr/commit/f2414556))
- Fix opening a guest SMB share popping the kernel `smbfs` credential dialog
  ([92119464](https://github.com/vdavid/cmdr/commit/92119464))
- Fix `TransferErrorDialog` being see-through in the transient branch
  ([f01af359](https://github.com/vdavid/cmdr/commit/f01af359))
- Fix error dialogs rendering OS strings with markdown bleed-through (`STATUS<em>DELETE</em>PENDING`)
  ([dbd7a2ac](https://github.com/vdavid/cmdr/commit/dbd7a2ac))
- Fix Brief mode cursor stripe briefly spanning the entire pane while column widths load
  ([d676efa5](https://github.com/vdavid/cmdr/commit/d676efa5))
- Fix Move dialog hiding the Size progress bar (`bytes_total` was 0)
  ([8856e012](https://github.com/vdavid/cmdr/commit/8856e012))
- Fix conflict-resolution radios reading "Skip all" / "Ask for each" when only one conflict exists
  ([4eac76b4](https://github.com/vdavid/cmdr/commit/4eac76b4))
- Fix focused-button Enter firing the dialog's default action instead of the focused button
  ([079a0ce1](https://github.com/vdavid/cmdr/commit/079a0ce1))
- Fix free-space numbers tier-coloring as red on healthy disks
  ([8219a06c](https://github.com/vdavid/cmdr/commit/8219a06c))
- Fix the AI offer prompting Intel Macs for a local-model download they can't run
  ([52f3cd81](https://github.com/vdavid/cmdr/commit/52f3cd81))
- Fix every tokio task crashing when stderr becomes a broken pipe
  ([31d97e06](https://github.com/vdavid/cmdr/commit/31d97e06))
- Fix Linux compile errors in `errno.rs` and `mcp/resources.rs`
  ([90b0afee](https://github.com/vdavid/cmdr/commit/90b0afee))
- Fix Linux compile errors in `system_strings.rs` (macOS-only loctable items)
  ([e852f04a](https://github.com/vdavid/cmdr/commit/e852f04a))
- Fix `clippy::unnecessary_sort_by` on Linux volume sorting (1.95 picked it up)
  ([03faf480](https://github.com/vdavid/cmdr/commit/03faf480))

### Non-app

- Cap every Rust test at 8 s (matches the Playwright convention), with documented exceptions
  ([eb67f389](https://github.com/vdavid/cmdr/commit/eb67f389))
- Stop gating `desktop-e2e-linux` on `desktop-rust` in CI ([66a2e501](https://github.com/vdavid/cmdr/commit/66a2e501))
- Harden the checker against supply-chain attacks: `--locked` everywhere, pinned tool versions, new
  `workflows-hardening` + `govulncheck` checks ([7d771ca8](https://github.com/vdavid/cmdr/commit/7d771ca8))
- Declare `rustfmt` and `clippy` as required `rust-toolchain.toml` components
  ([a23222eb](https://github.com/vdavid/cmdr/commit/a23222eb))
- Trigger Rust CI on `rust-toolchain.toml` changes ([0f8c9ffb](https://github.com/vdavid/cmdr/commit/0f8c9ffb))
- Dev override `VITE_CMDR_FORCE_OLD_WEBKIT=1 pnpm dev` to test the old-WebKit fallback on modern Macs
  ([17537510](https://github.com/vdavid/cmdr/commit/17537510))
- 14-day release-age gate via Renovate (3-day override for security advisories)
  ([8bd5af1e](https://github.com/vdavid/cmdr/commit/8bd5af1e))
- Shared `pluralize` helper for log/error/UI strings, plus a `pluralize-noun` check
  ([0ae2ee92](https://github.com/vdavid/cmdr/commit/0ae2ee92),
  [ec277ba8](https://github.com/vdavid/cmdr/commit/ec277ba8),
  [e070fc34](https://github.com/vdavid/cmdr/commit/e070fc34))
- Force file-backed secret store under `CMDR_E2E_MODE=1` (no more Keychain prompts in unattended E2E)
  ([ecb495fc](https://github.com/vdavid/cmdr/commit/ecb495fc))
- New `btn-restyle` check (forbids `.btn-*` overrides); accent-matrix in the contrast check
  ([51f31939](https://github.com/vdavid/cmdr/commit/51f31939),
  [0e885f5d](https://github.com/vdavid/cmdr/commit/0e885f5d))
- Codify 100-char Rust comment width; reflow existing comments
  ([b76b9277](https://github.com/vdavid/cmdr/commit/b76b9277),
  [610f66f6](https://github.com/vdavid/cmdr/commit/610f66f6))
- Vendor `smb-consumer-maxreadsize` and pin the SMB streaming-write no-deadlock invariant (200 × 1 MB at concurrency 8)
  ([1ae6eec7](https://github.com/vdavid/cmdr/commit/1ae6eec7),
  [e8259eef](https://github.com/vdavid/cmdr/commit/e8259eef),
  [e750920b](https://github.com/vdavid/cmdr/commit/e750920b))
- Ticketed acquire/release logs on the `SmbVolume` client mutex
  ([2e4aeb9d](https://github.com/vdavid/cmdr/commit/2e4aeb9d))
- E2E focus hygiene: viewer/settings windows skip OS focus, Escape-binding tests use synthetic dispatch
  ([be21bebe](https://github.com/vdavid/cmdr/commit/be21bebe),
  [0dfdcb2a](https://github.com/vdavid/cmdr/commit/0dfdcb2a))
- Defensive disk-poll + refresh in the MTP→local copy E2E ([9693b283](https://github.com/vdavid/cmdr/commit/9693b283))
- Stamp the running E2E test name into the main window's OS title
  ([1181e0c1](https://github.com/vdavid/cmdr/commit/1181e0c1))
- Document the UTM Ubuntu VM loop for iterating Linux-only tests
  ([917938ee](https://github.com/vdavid/cmdr/commit/917938ee))
- Switch `mtp-rs` to crates.io 0.15.0 (off the path dep) ([f98313f0](https://github.com/vdavid/cmdr/commit/f98313f0))

## [0.19.0] - 2026-05-16

### Added

- Reorganize Settings into Appearance, Behavior, File systems, Updates, AI, Network, Privacy, and Advanced
  ([c3003a05](https://github.com/vdavid/cmdr/commit/c3003a05))
- Add "Overwrite all smaller" and "Overwrite all older" conflict actions
  ([2dfd17b8](https://github.com/vdavid/cmdr/commit/2dfd17b8))
- ⌘⇧T reopens closed tabs; double-click the tab bar opens a new tab
  ([65417fbe](https://github.com/vdavid/cmdr/commit/65417fbe),
  [d7a85a33](https://github.com/vdavid/cmdr/commit/d7a85a33))
- Move AI API keys to the OS keychain, with 300 ms debounced save
  ([42bc5eaf](https://github.com/vdavid/cmdr/commit/42bc5eaf),
  [10f8525b](https://github.com/vdavid/cmdr/commit/10f8525b))
- Command palette recents on empty query (last 10, LRU, grouped, self-heals stale IDs)
  ([d3406299](https://github.com/vdavid/cmdr/commit/d3406299),
  [a2971aba](https://github.com/vdavid/cmdr/commit/a2971aba))
- Type to jump to a file in the explorer ([0b9f943f](https://github.com/vdavid/cmdr/commit/0b9f943f))
- Sort-column shortcuts ⌘3–6 (Brief) and ⌘F3–F6 (Full) ([74e827e5](https://github.com/vdavid/cmdr/commit/74e827e5))
- Brief mode: backend-computed per-column widths, plus a max-column-width slider
  ([d84d5c2a](https://github.com/vdavid/cmdr/commit/d84d5c2a),
  [f7907107](https://github.com/vdavid/cmdr/commit/f7907107),
  [f9e40fc4](https://github.com/vdavid/cmdr/commit/f9e40fc4),
  [e18bdbf4](https://github.com/vdavid/cmdr/commit/e18bdbf4))
- Volume picker wraps cursor at top and bottom ([206ec7d9](https://github.com/vdavid/cmdr/commit/206ec7d9))
- USB link-speed indicator in the volume switcher ([637b152e](https://github.com/vdavid/cmdr/commit/637b152e))
- Stream MTP source-scan progress in the copy dialog (no more 0/0/0 freeze)
  ([fef1aafd](https://github.com/vdavid/cmdr/commit/fef1aafd))
- Bulk-skip pre-known conflicts under Skip-all for copy and move
  ([b365076d](https://github.com/vdavid/cmdr/commit/b365076d))
- MTP→SMB copy: kill the 2-min stall, faster source scan ([1ae5c198](https://github.com/vdavid/cmdr/commit/1ae5c198))
- Honest copy ETA on long single-file streams: stop decaying files_rate
  ([4737acbc](https://github.com/vdavid/cmdr/commit/4737acbc))
- Format sub-1 files/s readouts instead of rounding to 0 ([ff7a72f9](https://github.com/vdavid/cmdr/commit/ff7a72f9))
- Strip em-dashes from user copy and docs; rephrase microcopy to sound more human
  ([971e35c4](https://github.com/vdavid/cmdr/commit/971e35c4),
  [c39ecdc7](https://github.com/vdavid/cmdr/commit/c39ecdc7),
  [a16afb0c](https://github.com/vdavid/cmdr/commit/a16afb0c),
  [adab08fa](https://github.com/vdavid/cmdr/commit/adab08fa))

### Fixed

- Fix MTP delete freezing instead of showing live scan progress
  ([4e005f95](https://github.com/vdavid/cmdr/commit/4e005f95))
- Fix Cancel-copy losing the rollback on the APFS clonefile fast path
  ([9c2e6244](https://github.com/vdavid/cmdr/commit/9c2e6244))
- SMB upgrade no longer races mDNS in dev ([be1350d7](https://github.com/vdavid/cmdr/commit/be1350d7))
- "Connect directly" SMB login dialog now shows the actual server name
  ([0d84e4e7](https://github.com/vdavid/cmdr/commit/0d84e4e7))
- Bulk-skip no longer pollutes the throughput estimator, and only fires for top-level file conflicts
  ([55d3ca46](https://github.com/vdavid/cmdr/commit/55d3ca46),
  [c3be95c1](https://github.com/vdavid/cmdr/commit/c3be95c1))
- Per-iter Skip on volume copy credits byte progress ([e7f657df](https://github.com/vdavid/cmdr/commit/e7f657df))
- Show duration settings in their declared unit ([66571349](https://github.com/vdavid/cmdr/commit/66571349))
- Brief column-width slider enables inside the Settings window
  ([591e090b](https://github.com/vdavid/cmdr/commit/591e090b))
- Brief mode horizontal scrollbar drag no longer vibrates at 60 Hz
  ([b80789e1](https://github.com/vdavid/cmdr/commit/b80789e1))
- Restore focus when a ModalDialog closes and when the command palette closes
  ([35413fa3](https://github.com/vdavid/cmdr/commit/35413fa3),
  [6c45e12d](https://github.com/vdavid/cmdr/commit/6c45e12d))
- File viewer surfaces `SearchStatus::Cancelled` to the FE on cancel
  ([14ba2735](https://github.com/vdavid/cmdr/commit/14ba2735))
- Separate MCP ports for prod (19224) and dev (19225) so dev no longer collides with the installed app
  ([f0524658](https://github.com/vdavid/cmdr/commit/f0524658))
- `setSetting` is idempotent on unchanged values ([c49636d8](https://github.com/vdavid/cmdr/commit/c49636d8))
- Pane state: clear network host on leaving the network volume; skip FilePane MCP sync on Network
  ([602fcb94](https://github.com/vdavid/cmdr/commit/602fcb94),
  [a1d19947](https://github.com/vdavid/cmdr/commit/a1d19947))

### Non-app

- Refactor write ops behind a shared transfer driver: per-source loop for copy/move, sink-based inner functions across
  local and volume code paths, drop one unsafe transmute ([b6833e26](https://github.com/vdavid/cmdr/commit/b6833e26),
  [1d9f2ca4](https://github.com/vdavid/cmdr/commit/1d9f2ca4),
  [63b6728e](https://github.com/vdavid/cmdr/commit/63b6728e),
  [0218a645](https://github.com/vdavid/cmdr/commit/0218a645),
  [01c8614e](https://github.com/vdavid/cmdr/commit/01c8614e),
  [101e8385](https://github.com/vdavid/cmdr/commit/101e8385),
  [118ac6b1](https://github.com/vdavid/cmdr/commit/118ac6b1),
  [bc957471](https://github.com/vdavid/cmdr/commit/bc957471),
  [9d7c69e8](https://github.com/vdavid/cmdr/commit/9d7c69e8),
  [a056eb58](https://github.com/vdavid/cmdr/commit/a056eb58),
  [5cf1173a](https://github.com/vdavid/cmdr/commit/5cf1173a),
  [1280056b](https://github.com/vdavid/cmdr/commit/1280056b),
  [0a7c257c](https://github.com/vdavid/cmdr/commit/0a7c257c),
  [643e7cb2](https://github.com/vdavid/cmdr/commit/643e7cb2),
  [afb70901](https://github.com/vdavid/cmdr/commit/afb70901))
- Parallel-shard the E2E suite across three Tauri instances (MTP isolated, two non-MTP shards balanced); wall-clock 5m
  49s → 2m 48s ([7802fca3](https://github.com/vdavid/cmdr/commit/7802fca3),
  [1841e0c5](https://github.com/vdavid/cmdr/commit/1841e0c5),
  [6e8971a0](https://github.com/vdavid/cmdr/commit/6e8971a0))
- Cut Playwright wall-clock 10m 12s → 5m 6s via condition polling, MCP-driven cursor moves, beforeEach short-circuits,
  and a per-keystroke → menu-dispatch migration ([507afb0e](https://github.com/vdavid/cmdr/commit/507afb0e),
  [3b04806e](https://github.com/vdavid/cmdr/commit/3b04806e),
  [f907adc2](https://github.com/vdavid/cmdr/commit/f907adc2),
  [df89b217](https://github.com/vdavid/cmdr/commit/df89b217))
- Add proptest-based property tests for `platform_case_compare`, search scope parsing, `glob_to_regex`, and
  `topological_sort_bottom_up` ([2e747bf8](https://github.com/vdavid/cmdr/commit/2e747bf8),
  [ffd799c8](https://github.com/vdavid/cmdr/commit/ffd799c8),
  [1813e3dc](https://github.com/vdavid/cmdr/commit/1813e3dc),
  [2cf586d1](https://github.com/vdavid/cmdr/commit/2cf586d1),
  [e69e45aa](https://github.com/vdavid/cmdr/commit/e69e45aa))
- Add state-transition tests for `IndexPhase`, `ActivityPhase`, `DiscoveryState`, and `SearchStatus`
  ([c0aed651](https://github.com/vdavid/cmdr/commit/c0aed651),
  [9a9899e9](https://github.com/vdavid/cmdr/commit/9a9899e9),
  [9dd32504](https://github.com/vdavid/cmdr/commit/9dd32504),
  [4ae15120](https://github.com/vdavid/cmdr/commit/4ae15120))
- Add vitest mockIPC harness plus IPC contract tests for SMB connection, file viewer, and write operations
  ([04c26e4d](https://github.com/vdavid/cmdr/commit/04c26e4d),
  [3a538b44](https://github.com/vdavid/cmdr/commit/3a538b44),
  [baa977ed](https://github.com/vdavid/cmdr/commit/baa977ed),
  [967d93be](https://github.com/vdavid/cmdr/commit/967d93be))
- Add mutation-testing-driven unit tests across `indexing/store`, `chunked_copy`, `watcher`, `copy_strategy`, and
  `state` ([ef91cfb8](https://github.com/vdavid/cmdr/commit/ef91cfb8),
  [a812cd9a](https://github.com/vdavid/cmdr/commit/a812cd9a),
  [e9a3a9fd](https://github.com/vdavid/cmdr/commit/e9a3a9fd),
  [b026f43d](https://github.com/vdavid/cmdr/commit/b026f43d),
  [4f04d03c](https://github.com/vdavid/cmdr/commit/4f04d03c),
  [41a3a831](https://github.com/vdavid/cmdr/commit/41a3a831))
- Codify the testing playbook and tools inventory ([9515adde](https://github.com/vdavid/cmdr/commit/9515adde))
- Add ESLint rule `cmdr/no-arbitrary-sleep-in-e2e` ([a9aea301](https://github.com/vdavid/cmdr/commit/a9aea301))
- File-length check: 10% growth buffer with growth % shown; split long files into focused modules
  ([1c1bdeb0](https://github.com/vdavid/cmdr/commit/1c1bdeb0),
  [2d7c27a3](https://github.com/vdavid/cmdr/commit/2d7c27a3))
- Pre-commit `--fast` lane in the check runner ([33f77ca5](https://github.com/vdavid/cmdr/commit/33f77ca5))
- E2E windows get a blue title stripe and `E2E -` prefix so they can't be mistaken for the installed app
  ([b1f707b7](https://github.com/vdavid/cmdr/commit/b1f707b7))

## [0.18.0] - 2026-05-12

### Added

- Suppress the 5–10 native macOS TCC popups that stacked behind the FDA onboarding prompt, deep-link to the Full Disk
  Access pane, version-aware copy, Tahoe `+`-button tip, and a multi-trigger probe that catches kernel short-circuits
  ([3c708d35](https://github.com/vdavid/cmdr/commit/3c708d35),
  [16918218](https://github.com/vdavid/cmdr/commit/16918218),
  [f32dfc55](https://github.com/vdavid/cmdr/commit/f32dfc55),
  [791edff0](https://github.com/vdavid/cmdr/commit/791edff0))
- Flag TCC-restricted folders live in the sidebar and file list: italic + (i) icon, `<no perms>` Size, generic folder
  icon for FDA-gated favorites, failed listings stay in nav history
  ([7baa9317](https://github.com/vdavid/cmdr/commit/7baa9317),
  [6581f5ad](https://github.com/vdavid/cmdr/commit/6581f5ad),
  [df6cd794](https://github.com/vdavid/cmdr/commit/df6cd794),
  [762d7b9a](https://github.com/vdavid/cmdr/commit/762d7b9a))
- Defer the AI offer toast until onboarding ends so it stops piling on the FDA prompt
  ([265c72d9](https://github.com/vdavid/cmdr/commit/265c72d9))
- Color modified dates by age with per-segment tiers (year, month, day, time each get their own color); App palette is
  now the default ([c73fcf54](https://github.com/vdavid/cmdr/commit/c73fcf54),
  [d98459b6](https://github.com/vdavid/cmdr/commit/d98459b6),
  [be2333c2](https://github.com/vdavid/cmdr/commit/be2333c2))
- Color sizes at every previously-plain site (tooltips, breadcrumb, transfer/delete dialogs, viewer footer, AI progress,
  search results); light-mode palette retuned to clear WCAG AA against every background
  ([265c5a0e](https://github.com/vdavid/cmdr/commit/265c5a0e),
  [31128012](https://github.com/vdavid/cmdr/commit/31128012))
- Show real scan progress in copy/delete dialogs with running tallies, throughput, current directory, and a real
  progress bar; hardlinks deduped by inode so totals match the indexer
  ([03215d25](https://github.com/vdavid/cmdr/commit/03215d25))
- Honest ETA when files outnumber bytes: tracks both axes, picks the slower; no more "~0 s remaining" while the
  small-file tail drains ([16b49a04](https://github.com/vdavid/cmdr/commit/16b49a04))
- Stream folder-name suggestions in the New folder dialog: first option in under 500 ms instead of after the full reply
  ([d681c8de](https://github.com/vdavid/cmdr/commit/d681c8de))
- Add multi-provider AI via the `genai` crate (GPT-5, o-series, Anthropic, Gemini, xAI, Groq, DeepSeek, OpenRouter,
  Ollama); fixes GPT-5 400 on `temperature` and `*-pro`/`*-codex` 404 on chat completions
  ([0c45a469](https://github.com/vdavid/cmdr/commit/0c45a469))
- Cap updater hangs at 30 s and surface the real cause (DNS error, TCP deadline) instead of generic "error sending
  request" ([e5be1467](https://github.com/vdavid/cmdr/commit/e5be1467))
- Per-row crash email with build mode and short ID, schema migrations, newest-first sort
  ([e89a63a3](https://github.com/vdavid/cmdr/commit/e89a63a3))
- Stable client-side ID for error reports across dialog, toast, and Discord; env-prefixed R2 keys; `[DEV]`/`[PROD]`
  Discord prefix ([77260827](https://github.com/vdavid/cmdr/commit/77260827),
  [e1810361](https://github.com/vdavid/cmdr/commit/e1810361))
- Guard read-only volumes up front for F7/F8/F2 so MTP read-only SD cards warn before you type anything
  ([d9212b83](https://github.com/vdavid/cmdr/commit/d9212b83))
- Provider-enriched friendly errors on the write path: MacDroid folders get "Managed by **MacDroid**…" on move failures;
  dialog renders markdown + category icon, retry shown only when meaningful
  ([e9452032](https://github.com/vdavid/cmdr/commit/e9452032),
  [51dff4c1](https://github.com/vdavid/cmdr/commit/51dff4c1),
  [5bcacfef](https://github.com/vdavid/cmdr/commit/5bcacfef))
- Route every dir-into-dir cross-volume conflict through the resolver so Stop/Skip/Overwrite/Rename works for folders
  too; pin Overwrite-means-merge as an architectural guarantee
  ([7ecf9d37](https://github.com/vdavid/cmdr/commit/7ecf9d37),
  [2f4e377d](https://github.com/vdavid/cmdr/commit/2f4e377d))
- Bump smb2 to 0.8.0 with typed `STATUS_OBJECT_NAME_COLLISION` and `FILE_IS_A_DIRECTORY` so merging into an existing SMB
  directory works after a partial copy; fast-path WARNs demoted to debug
  ([7dd9cfc8](https://github.com/vdavid/cmdr/commit/7dd9cfc8),
  [623f8c17](https://github.com/vdavid/cmdr/commit/623f8c17))
- Move MCP defaults to ports 19224 (prod) and 19225 (dev) so a dev build no longer collides with the installed app
  ([c9fad17e](https://github.com/vdavid/cmdr/commit/c9fad17e))
- Polish getcmdr.com hero: "Download for macOS" button, viewport-responsive illustration mask, muted link style,
  tightened copy ([606c724e](https://github.com/vdavid/cmdr/commit/606c724e))

### Fixed

- Fix F8 (and other dialogs) dying after a volume switch: Option fields started serializing as JSON `null` instead of
  being omitted after the typed-IPC migration; swept `=== undefined` checks across 11 sites
  ([f2019aff](https://github.com/vdavid/cmdr/commit/f2019aff),
  [46bd6d0e](https://github.com/vdavid/cmdr/commit/46bd6d0e),
  [eef042d3](https://github.com/vdavid/cmdr/commit/eef042d3))
- Fix one or two rows ellipsizing the Modified column under non-100% text size (sub-pixel glyph-advance drift)
  ([a7a7915e](https://github.com/vdavid/cmdr/commit/a7a7915e))
- Fix light/dark theme briefly flipping at startup when the persisted choice differed from the system preference
  ([f689da01](https://github.com/vdavid/cmdr/commit/f689da01))
- Stop the dev runtime silently overwriting committed `bindings.ts` on every `pnpm dev` launch
  ([6e39d68d](https://github.com/vdavid/cmdr/commit/6e39d68d))
- Silence the `get_file_at` FE/BE drift warning that fired legitimately during async listing refreshes
  ([0b51a331](https://github.com/vdavid/cmdr/commit/0b51a331))
- Accept `null` for optional crash-report fields so reports written by older app versions still upload after upgrade
  ([3c12ff2f](https://github.com/vdavid/cmdr/commit/3c12ff2f))
- Stop re-anchoring focus inside `selectVolumeByIndex`/`navigateToPath`, which dropped keystrokes mid-sequence in fast
  multi-select ([6074cd21](https://github.com/vdavid/cmdr/commit/6074cd21))

### Non-app

- Migrate the full IPC surface to typed bindings via tauri-specta; an ESLint rule and a Go check block raw `invoke()`
  and lockfile drift ([f1e58011](https://github.com/vdavid/cmdr/commit/f1e58011),
  [dc5f0b47](https://github.com/vdavid/cmdr/commit/dc5f0b47))
- Ban classifying errors by string-matching `message`/`stderr`/`title` with a Go check and ESLint rule; sweep across
  SMB, git, friendly errors, and updater ([c764962a](https://github.com/vdavid/cmdr/commit/c764962a))
- Pin pnpm 11.0.9 in `mise.toml` and move overrides to `pnpm-workspace.yaml`; unblocks CI's E2E-Linux
  ([cee0aa08](https://github.com/vdavid/cmdr/commit/cee0aa08),
  [c41d2e0d](https://github.com/vdavid/cmdr/commit/c41d2e0d))
- Track recurring upkeep in `docs/maintenance.md` with a log going back to 2025-12-25
  ([49a119bd](https://github.com/vdavid/cmdr/commit/49a119bd))

## [0.17.0] - 2026-05-06

### Added

- Add dynamic text size slider in Settings (75–150%, ⌘+/⌘-/⌘0 shortcuts)
  ([a326bca6](https://github.com/vdavid/cmdr/commit/a326bca6),
  [ca78382d](https://github.com/vdavid/cmdr/commit/ca78382d),
  [e207effb](https://github.com/vdavid/cmdr/commit/e207effb))
- Add "Open with" and system Services to menus ([71e6061b](https://github.com/vdavid/cmdr/commit/71e6061b))
- Add iCloud Drive cloud actions to context menu ([01bc0dae](https://github.com/vdavid/cmdr/commit/01bc0dae))
- Split Brief/Full menu items to per-pane View > Left/Right submenus
  ([7f4d123d](https://github.com/vdavid/cmdr/commit/7f4d123d))
- Add networking toggle, lazy mDNS, no more local-network prompt at launch
  ([d2ae5170](https://github.com/vdavid/cmdr/commit/d2ae5170))
- Faster external drive detection, fixes USB-C dock invisibility
  ([6527d850](https://github.com/vdavid/cmdr/commit/6527d850))
- Drag & drop matches Finder (same-volume Move, cross-volume Copy, modifier overrides)
  ([64db140f](https://github.com/vdavid/cmdr/commit/64db140f))
- Drag & drop "+" badge tracks the actual op, no flicker ([cf8e3818](https://github.com/vdavid/cmdr/commit/cf8e3818),
  [dcfe439e](https://github.com/vdavid/cmdr/commit/dcfe439e))
- Drag files into terminals (Warp etc.) ([97d10675](https://github.com/vdavid/cmdr/commit/97d10675))
- Add Trash/Delete toggle to delete dialog ([778296dd](https://github.com/vdavid/cmdr/commit/778296dd))
- Always show Copy/Move toggle in transfer dialog ([450363e6](https://github.com/vdavid/cmdr/commit/450363e6))
- Default to Full mode on fresh installs ([57ba47c1](https://github.com/vdavid/cmdr/commit/57ba47c1))
- File list typography polish: aligned dates, aligned headers, fade selection, clamped Ext
  ([474f7414](https://github.com/vdavid/cmdr/commit/474f7414),
  [e9aec7bd](https://github.com/vdavid/cmdr/commit/e9aec7bd),
  [88f56367](https://github.com/vdavid/cmdr/commit/88f56367),
  [c5698998](https://github.com/vdavid/cmdr/commit/c5698998))
- Add size-color palette setting (Rainbow / Accent / None) ([5fe0d77e](https://github.com/vdavid/cmdr/commit/5fe0d77e))
- Restore double-click-to-zoom on macOS title bar ([f95441dc](https://github.com/vdavid/cmdr/commit/f95441dc))
- Focus search when Settings opens ([cb88685d](https://github.com/vdavid/cmdr/commit/cb88685d))
- Hand cursor on License dialog support and Buy links ([554b3801](https://github.com/vdavid/cmdr/commit/554b3801))
- Show real .git/\* files alongside virtual categories in git portal
  ([33219321](https://github.com/vdavid/cmdr/commit/33219321))
- Per-file Modified dates inside git portal snapshots ([3cead878](https://github.com/vdavid/cmdr/commit/3cead878))
- Cache git status per index change, near-instant repeat navs
  ([19f0e98e](https://github.com/vdavid/cmdr/commit/19f0e98e))
- Error-report preview now lands under 200 ms on big log dirs (was 30+ s)
  ([f24f255c](https://github.com/vdavid/cmdr/commit/f24f255c))
- Send error reports in dev too, tagged \[DEV\] ([63ebabf6](https://github.com/vdavid/cmdr/commit/63ebabf6))
- Persistent "Save bundle to disk" toast with Reveal in Finder
  ([0debff1c](https://github.com/vdavid/cmdr/commit/0debff1c))
- getcmdr.com comments follow live theme changes ([7333b13c](https://github.com/vdavid/cmdr/commit/7333b13c))

### Fixed

- Fix Intel DMG download 404 ([19f797da](https://github.com/vdavid/cmdr/commit/19f797da))
- Fix crash on virtual git portal toggle; empty git roots no longer render as 1970-01-01
  ([b266737e](https://github.com/vdavid/cmdr/commit/b266737e))
- Fix folder size column losing value after rename ([b1d032c1](https://github.com/vdavid/cmdr/commit/b1d032c1),
  [d7e08e16](https://github.com/vdavid/cmdr/commit/d7e08e16))

### Non-app

- Big dead-code cleanup, 355 lines across 22 files ([a6b46131](https://github.com/vdavid/cmdr/commit/a6b46131))
- Bump GitHub Actions to Node 24 ([2f02fa7e](https://github.com/vdavid/cmdr/commit/2f02fa7e))
- Replace claude-md-staleness with claude-md-reminder (fires in-loop, not weeks later)
  ([60e30be5](https://github.com/vdavid/cmdr/commit/60e30be5))
- Big CHANGELOG cleanup: shorten long items and document style guidelines.
  ([8f3daa0a](https://github.com/vdavid/cmdr/commit/8f3daa0a))

## [0.16.0] - 2026-05-01

### Added

- Add SMB live reconnect, 5-attempt backoff right in the pane, no re-auth
  ([d96bc4b4](https://github.com/vdavid/cmdr/commit/d96bc4b4),
  [0c1d3680](https://github.com/vdavid/cmdr/commit/0c1d3680))
- Disconnect button now actually unmounts (toast if Finder's holding the volume)
  ([c5a410aa](https://github.com/vdavid/cmdr/commit/c5a410aa))
- Add Check for updates from inside app ([00470b96](https://github.com/vdavid/cmdr/commit/00470b96))
- Add human-friendly size units toggle ([c8cc1008](https://github.com/vdavid/cmdr/commit/c8cc1008))
- Add symlink-aware size hint, info icon explains exclusion (matches du and Finder)
  ([0d83a7b2](https://github.com/vdavid/cmdr/commit/0d83a7b2))
- AI download toast X stays closed for the rest of the download
  ([97f1cee3](https://github.com/vdavid/cmdr/commit/97f1cee3))
- Skip rename warning for equivalent extensions (jpg/jpeg, htm/html, yml/yaml, tif/tiff, etc.)
  ([55592ba4](https://github.com/vdavid/cmdr/commit/55592ba4))

### Fixed

- Fix temp network issues kicking users out of folders ([48ac9bf8](https://github.com/vdavid/cmdr/commit/48ac9bf8))
- Suppress "Restart to update" toast during first-launch onboarding
  ([ffeb7d96](https://github.com/vdavid/cmdr/commit/ffeb7d96))
- Fix indexer triggering macOS perm popups while onboarding: now waits for FDA
  ([59aca717](https://github.com/vdavid/cmdr/commit/59aca717))
- Fix SMB reconnect runaway subscribe loop after hot reload ([91bc2e46](https://github.com/vdavid/cmdr/commit/91bc2e46))
- Fix SMB reconnect double-triggering loadDirectory ([3f6b1b0d](https://github.com/vdavid/cmdr/commit/3f6b1b0d))

## [0.15.0] - 2026-04-29

### Added

- Add git browser: live branch/dirty pill in breadcrumb, browse `.git/branches/`, `tags/`, `commits/`, `stash/`,
  `worktrees/`, `submodules/` as folders, drag any file out of any branch or commit into working tree (preserves bytes
  and exec bit, no `git checkout`), optional per-file status column with M/A/D/?/! glyphs
  ([314e9ae2](https://github.com/vdavid/cmdr/commit/314e9ae2),
  [897df2c7](https://github.com/vdavid/cmdr/commit/897df2c7),
  [1ebcfa1c](https://github.com/vdavid/cmdr/commit/1ebcfa1c))
- Meaningful Modified and Size columns in git portal (`+12 / -3` for branches, `5 files` for commits, `on main` for
  stashes, short SHAs for tags) ([31aec35c](https://github.com/vdavid/cmdr/commit/31aec35c))
- Add friendly errors for git browser ([19d5b075](https://github.com/vdavid/cmdr/commit/19d5b075),
  [af64689f](https://github.com/vdavid/cmdr/commit/af64689f))
- Add Git toggles in Settings (repo chip, status column, virtual portal)
  ([19d5b075](https://github.com/vdavid/cmdr/commit/19d5b075),
  [af64689f](https://github.com/vdavid/cmdr/commit/af64689f))

### Fixed

- Fix virtual `.git/<category>/...` paths kicking pane back to parent
  ([bfcbfa48](https://github.com/vdavid/cmdr/commit/bfcbfa48))

## [0.14.0] - 2026-04-26

### Added

- Add error reports: one-click redacted diagnostic bundle via Help menu or error toast, with optional auto-send and a
  short ERR-XXXXX correlation ID ([6d904aa6](https://github.com/vdavid/cmdr/commit/6d904aa6),
  [51b6102a](https://github.com/vdavid/cmdr/commit/51b6102a))
- Add log storage cap setting (default 200 MB, 0 disables log storage and error reports)
  ([f3dbf514](https://github.com/vdavid/cmdr/commit/f3dbf514))
- Add per-output log filtering, with a verbose-logging toggle in Settings
  ([319d5d37](https://github.com/vdavid/cmdr/commit/319d5d37))

### Fixed

- Fix auto-sent error reports dropping when fired before the Tauri handle exists
  ([f069a712](https://github.com/vdavid/cmdr/commit/f069a712))
- Align Size column icons flush right ([1d5f661a](https://github.com/vdavid/cmdr/commit/1d5f661a))

### Non-app

- Add error-report endpoint on api server with R2 presigned-URL handoff
  ([1a2ea1c0](https://github.com/vdavid/cmdr/commit/1a2ea1c0),
  [f78f76af](https://github.com/vdavid/cmdr/commit/f78f76af))
- Add shared PII redactor for crash files and error-report bundles
  ([1d719f36](https://github.com/vdavid/cmdr/commit/1d719f36),
  [b64e2c2c](https://github.com/vdavid/cmdr/commit/b64e2c2c))

## [0.13.0] - 2026-04-22

### Added

- SMB copies ~30× faster on high-latency links (100×10 KB over ~60 ms RTT: ~28 s to ~1 s)
  ([94090555](https://github.com/vdavid/cmdr/commit/94090555),
  [9d6df0e9](https://github.com/vdavid/cmdr/commit/9d6df0e9),
  [4009b9ba](https://github.com/vdavid/cmdr/commit/4009b9ba),
  [77ea6e81](https://github.com/vdavid/cmdr/commit/77ea6e81))
- Add SMB concurrency setting (default 10, range 1–32, live)
  ([7fdd85e3](https://github.com/vdavid/cmdr/commit/7fdd85e3),
  [aa331c4e](https://github.com/vdavid/cmdr/commit/aa331c4e),
  [f46d45e4](https://github.com/vdavid/cmdr/commit/f46d45e4))
- `..` row shows current folder's totals, not parent's ([36212ede](https://github.com/vdavid/cmdr/commit/36212ede))
- Full mode shrink-wraps Ext/Size/Modified to give Name every spare pixel
  ([7325c8f8](https://github.com/vdavid/cmdr/commit/7325c8f8))
- Brief mode shrink-wraps each column to its widest filename
  ([c336dbba](https://github.com/vdavid/cmdr/commit/c336dbba))
- Filename tooltip on truncation in Brief and Full ([f37d7e51](https://github.com/vdavid/cmdr/commit/f37d7e51))
- Volume tooltip on tabs ([b6663988](https://github.com/vdavid/cmdr/commit/b6663988))

### Fixed

- Security: bump smb2 to 0.7.2, fixes a crafted DFS referral crashing Cmdr
  ([7e7eaf76](https://github.com/vdavid/cmdr/commit/7e7eaf76))
- Fix small SMB uploads ignoring cancel ([f948731c](https://github.com/vdavid/cmdr/commit/f948731c))
- Fix click-on-cursor eating the next drag ([cccf0095](https://github.com/vdavid/cmdr/commit/cccf0095))
- ⌘C now copies selected text when there's a text selection ([47f03b20](https://github.com/vdavid/cmdr/commit/47f03b20))
- Block dropping a folder onto itself or its descendants ("Can't drop here" feedback); `..` accepts drops
  ([b7c3d960](https://github.com/vdavid/cmdr/commit/b7c3d960))
- Fix frontend hot reload (swap UnoCSS for unplugin-icons) ([00906566](https://github.com/vdavid/cmdr/commit/00906566))

### Changed

- Internal: cross-volume copies flow through stream API (plus APFS clonefile fast path); batch copies run in parallel
  per-backend ([eb99c37c](https://github.com/vdavid/cmdr/commit/eb99c37c),
  [508a0fe1](https://github.com/vdavid/cmdr/commit/508a0fe1),
  [50b7221e](https://github.com/vdavid/cmdr/commit/50b7221e),
  [39c71eed](https://github.com/vdavid/cmdr/commit/39c71eed))
- Move smb2 from git to crates.io, bump through 0.7.1 and 0.7.2
  ([96f4bbd3](https://github.com/vdavid/cmdr/commit/96f4bbd3),
  [0ec95a79](https://github.com/vdavid/cmdr/commit/0ec95a79),
  [7e7eaf76](https://github.com/vdavid/cmdr/commit/7e7eaf76))

### Non-app

- Run Docker SMB integration tests on every push (26 tests against real servers)
  ([257269bb](https://github.com/vdavid/cmdr/commit/257269bb))
- Byte-level blake3 hash verification on every SMB copy test
  ([fd5a2d84](https://github.com/vdavid/cmdr/commit/fd5a2d84))
- SMB copy soak harness: 30-min Docker run, 41,984 iterations, zero drift
  ([3a9b58f2](https://github.com/vdavid/cmdr/commit/3a9b58f2),
  [6a9e046d](https://github.com/vdavid/cmdr/commit/6a9e046d))
- Add changelog-commit-links check (surfaced and fixed 8 bad links)
  ([4e28130](https://github.com/vdavid/cmdr/commit/4e28130))

## [0.12.0] - 2026-04-18

### Added

- Add friendly error pane for listing failures (provider-aware suggestions for Dropbox, Drive, OneDrive, iCloud,
  MacDroid, VeraCrypt, etc.) ([eec50ff](https://github.com/vdavid/cmdr/commit/eec50ff),
  [cc7bb3](https://github.com/vdavid/cmdr/commit/cc7bb3))
- Live disk-space updates in status bar (configurable threshold, 3 s timeout)
  ([d67dd3](https://github.com/vdavid/cmdr/commit/d67dd3))
- Add "Copy path" to breadcrumb context menu ([eb4d3c](https://github.com/vdavid/cmdr/commit/eb4d3c))
- Add SMB streaming reads/writes (MTP↔SMB and SMB↔SMB copies skip temp files, ~1 MiB peak RAM)
  ([ac71bd](https://github.com/vdavid/cmdr/commit/ac71bd), [a82709](https://github.com/vdavid/cmdr/commit/a82709),
  [35120d](https://github.com/vdavid/cmdr/commit/35120d), [043597f](https://github.com/vdavid/cmdr/commit/043597f))
- Disambiguate same-named SMB shares per server ([76671b](https://github.com/vdavid/cmdr/commit/76671b))
- Inline SMB login form on direct-connection upgrade ([b315b4](https://github.com/vdavid/cmdr/commit/b315b4))
- Instant dialog open for large selections (50k-file Copy/Move: ~10 s to ~1 ms)
  ([48ea60](https://github.com/vdavid/cmdr/commit/48ea60))
- Add MTP Samsung support (phones reporting 0 storages at connect time now appear)
  ([14b3ac](https://github.com/vdavid/cmdr/commit/14b3ac))
- Batch MTP scan for copy (one USB call per parent dir, not per file)
  ([70978c](https://github.com/vdavid/cmdr/commit/70978c))
- Skip rename extension warning on case-only changes (photo.JPG to photo.jpg)
  ([1401017](https://github.com/vdavid/cmdr/commit/1401017))
- Split filename + extension in Full view ([275d091](https://github.com/vdavid/cmdr/commit/275d091))
- Volume selector polish (clickable spacebar area, no clipping over F-key bar)
  ([700eac](https://github.com/vdavid/cmdr/commit/700eac))
- File-op dialog polish (thousand separators, mid-text truncation, fixed 500 px width)
  ([d67dd3](https://github.com/vdavid/cmdr/commit/d67dd3))
- Add debug-window error-pane preview with all 47 error states ([cc7bb3](https://github.com/vdavid/cmdr/commit/cc7bb3))

### Fixed

- User cancels no longer log as ERROR ([6f79392](https://github.com/vdavid/cmdr/commit/6f79392))
- Fix copy/move crash from a reactivity race ([0cdd7d](https://github.com/vdavid/cmdr/commit/0cdd7d))
- Fix stuck "Scanning 0 files" transfer dialog ([dd06d68](https://github.com/vdavid/cmdr/commit/dd06d68))
- Fix double-dispatched MCP autoConfirm copies ([4af22ab](https://github.com/vdavid/cmdr/commit/4af22ab))
- Fix file watcher panic on 500+ external changes ([4087e30](https://github.com/vdavid/cmdr/commit/4087e30))
- Match Finder for copy space checks (count APFS purgeable space)
  ([3454656](https://github.com/vdavid/cmdr/commit/3454656))
- Fix SMB paths with spaces, serialize concurrent manual-server writes, fix viewer search after emoji/CJK
  ([97c0481](https://github.com/vdavid/cmdr/commit/97c0481))
- Fix SMB port handling and human host display for mDNS names ([c26f7e8](https://github.com/vdavid/cmdr/commit/c26f7e8),
  [017b7043](https://github.com/vdavid/cmdr/commit/017b7043))
- Fix "Connect directly" on QNAP ([2666db8](https://github.com/vdavid/cmdr/commit/2666db8))
- Hide Clear-index button when there's no index (fixes AA contrast)
  ([b1915d9](https://github.com/vdavid/cmdr/commit/b1915d9))
- Network pane no longer sticks on old host after mount ([41c1860](https://github.com/vdavid/cmdr/commit/41c1860))
- Fix llama-server startup on Linux with locked keyring (encrypted-file fallback)
  ([55ccde3](https://github.com/vdavid/cmdr/commit/55ccde3))

### Improved

- Async Volume trait end-to-end, no more nested-runtime panics on MTP/SMB
  ([531bb9b](https://github.com/vdavid/cmdr/commit/531bb9b), [9d4982a](https://github.com/vdavid/cmdr/commit/9d4982a),
  [694ddc1](https://github.com/vdavid/cmdr/commit/694ddc1))
- MTP read stream now safe from any runtime context ([1598f8c](https://github.com/vdavid/cmdr/commit/1598f8c))
- Cancelled SMB uploads skip server FLUSH (~100 ms to 1 s saved per cancel)
  ([6fa0780](https://github.com/vdavid/cmdr/commit/6fa0780))

### Non-app

- Add design-time WCAG contrast checker (resolves CSS vars and color-mix chains, replaces flaky axe rule)
  ([db25f0d](https://github.com/vdavid/cmdr/commit/db25f0d), [55af258](https://github.com/vdavid/cmdr/commit/55af258))
- Fix 18 real WCAG AA contrast failures ([747507f](https://github.com/vdavid/cmdr/commit/747507f),
  [67d42ba](https://github.com/vdavid/cmdr/commit/67d42ba), [4a15a53](https://github.com/vdavid/cmdr/commit/4a15a53))
- Add tier-3 component-level a11y tests (61 files, 146 tests, ~6.3 s) and a11y-coverage check
  ([33300a4](https://github.com/vdavid/cmdr/commit/33300a4), [d56c1df](https://github.com/vdavid/cmdr/commit/d56c1df),
  [398bf7a](https://github.com/vdavid/cmdr/commit/398bf7a))
- Switch Lucide to UnoCSS pure-CSS icons ([93548fa](https://github.com/vdavid/cmdr/commit/93548fa))
- Add file-length check; split 20+ long files into sub-800-line modules
  ([7514cb4](https://github.com/vdavid/cmdr/commit/7514cb4), [2939bfe](https://github.com/vdavid/cmdr/commit/2939bfe),
  [4514a83](https://github.com/vdavid/cmdr/commit/4514a83), [315609a](https://github.com/vdavid/cmdr/commit/315609a))
- Run Linux E2E in Docker ([8803c3c](https://github.com/vdavid/cmdr/commit/8803c3c),
  [f39177c](https://github.com/vdavid/cmdr/commit/f39177c))
- Drop CrabNebula/WebDriverIO macOS E2E suite (Playwright covers all 15)
  ([4cecfb9](https://github.com/vdavid/cmdr/commit/4cecfb9))
- Upgrade rustls-webpki 0.103.12 (RUSTSEC-2026-0098/0099) and bitstream-io 4.10.0
  ([3734502](https://github.com/vdavid/cmdr/commit/3734502))
- Add docs/error-handling.md contributor guide ([a4a5fdb](https://github.com/vdavid/cmdr/commit/a4a5fdb))

## [0.11.1] - 2026-04-10

### Added

- Add striped-rows setting (alternating row shading in Full and Brief)
  ([faa2534](https://github.com/vdavid/cmdr/commit/faa2534))
- Add MTP per-file copy progress and instant mid-file cancel (~300 ms via USB SIC abort)
  ([ac5ec4d](https://github.com/vdavid/cmdr/commit/ac5ec4d), [a66adf6](https://github.com/vdavid/cmdr/commit/a66adf6))

### Fixed

- Sync View menu Full/Brief checkmarks across panes ([6e36a49](https://github.com/vdavid/cmdr/commit/6e36a49))
- Stop MTP `ObjectNotFound` log spam on every copy ([0cc675a](https://github.com/vdavid/cmdr/commit/0cc675a))
- Fix MTP mid-stream cancel corrupting USB session (mtp-rs 0.11.0)
  ([a66adf6](https://github.com/vdavid/cmdr/commit/a66adf6))
- A11y: darken accent-text for WCAG AA, fix search placeholder opacity
  ([b7744dd](https://github.com/vdavid/cmdr/commit/b7744dd))
- Fix Linux compilation (cross-platform SMB types, get_smb_mount_info)
  ([00c5f18](https://github.com/vdavid/cmdr/commit/00c5f18))

## [0.11.0] - 2026-04-10

### Added

- Add SMB direct connections via smb2 (~4× faster, OS mount stays for Finder/Terminal)
  ([dea46ec](https://github.com/vdavid/cmdr/commit/dea46ec))
- Auto-upgrade existing and new SMB mounts to direct connections in the background
  ([a6ab2ca](https://github.com/vdavid/cmdr/commit/a6ab2ca))
- Add "Connect to server" for SMB by hostname, IP, or `smb://` URL (persisted, context-menu Disconnect/Forget)
  ([2df24ac](https://github.com/vdavid/cmdr/commit/2df24ac))
- Add SMB connection status indicators with one-click upgrade ([0473250](https://github.com/vdavid/cmdr/commit/0473250))
- Real-time SMB transfer progress with end-to-end cancel ([f530355](https://github.com/vdavid/cmdr/commit/f530355))
- All SMB write ops (create, delete, rename, copy, move) through direct connections with full conflict handling
  ([e72c082](https://github.com/vdavid/cmdr/commit/e72c082), [4f030d7](https://github.com/vdavid/cmdr/commit/4f030d7))
- Unified SMB/MTP change notifications with incremental cache patches
  ([2d0bc98](https://github.com/vdavid/cmdr/commit/2d0bc98))
- Warn in transfer dialog when using slower OS mount ([d25de48](https://github.com/vdavid/cmdr/commit/d25de48))
- Auto-suppress ptpcamerad on macOS for MTP ([d161f9b](https://github.com/vdavid/cmdr/commit/d161f9b))
- Add MTP settings (disable toggle, "Don't show again" toast, dedicated section)
  ([2467ece](https://github.com/vdavid/cmdr/commit/2467ece), [70d8d40](https://github.com/vdavid/cmdr/commit/70d8d40))
- Brief mode shows real recursive directory sizes in selection info
  ([53ee5ef](https://github.com/vdavid/cmdr/commit/53ee5ef))
- Cursor jumps to newly created directories ([eff84d1](https://github.com/vdavid/cmdr/commit/eff84d1))

### Fixed

- Fix per-file copy progress (counts files, not top-level items)
  ([d10d9cc](https://github.com/vdavid/cmdr/commit/d10d9cc))
- Faster SMB deletes (skip stat round-trip) ([0e7f072](https://github.com/vdavid/cmdr/commit/0e7f072))
- Copy cancellation checks between every file in tree copies ([a7d401a](https://github.com/vdavid/cmdr/commit/a7d401a))
- Fix cross-volume copy misclassifying SmbVolume as local ([4a86a85](https://github.com/vdavid/cmdr/commit/4a86a85))
- Fix SMB paths with accented characters (NFC normalization) ([baaccc8](https://github.com/vdavid/cmdr/commit/baaccc8))
- Resolve SMB IPs to hostnames via mDNS so Keychain finds saved credentials
  ([b1addfd](https://github.com/vdavid/cmdr/commit/b1addfd))
- Show login form on stale Keychain credentials instead of empty share list
  ([46609f1](https://github.com/vdavid/cmdr/commit/46609f1))
- Block navigating above SMB mount root, fall back to home when unreachable
  ([d25de48](https://github.com/vdavid/cmdr/commit/d25de48))
- Fix stale cursor index after file ops ([945093b](https://github.com/vdavid/cmdr/commit/945093b))
- Fix drag & drop after wry upgrade ([a816c77](https://github.com/vdavid/cmdr/commit/a816c77))
- Fix stale dir sizes after copy/create ([1479108](https://github.com/vdavid/cmdr/commit/1479108))
- Fix scan-preview race in progress dialog ([5d9b91b](https://github.com/vdavid/cmdr/commit/5d9b91b))
- Fix dir_stats count drift on file/dir type changes ([364ddf1](https://github.com/vdavid/cmdr/commit/364ddf1))
- Fix index entry ID race via shared atomic counter ([6e173e4](https://github.com/vdavid/cmdr/commit/6e173e4))
- Fix MTP move not refreshing UI on Linux (mtp-rs 0.9.1) ([5b27ead](https://github.com/vdavid/cmdr/commit/5b27ead))

### Non-app

- Replace smb/smb-rpc crates with our own smb2 ([2d7904f](https://github.com/vdavid/cmdr/commit/2d7904f))

## [0.10.0] - 2026-04-08

### Added

- Visible copy rollback (progress bars count back, Cancel stops the rollback)
  ([0ac5d0](https://github.com/vdavid/cmdr/commit/0ac5d0))
- Dual progress bars in transfer dialogs (size + file count) ([ced9d2](https://github.com/vdavid/cmdr/commit/ced9d2))
- MCP: cmdr://settings resource and set_setting tool ([c71115](https://github.com/vdavid/cmdr/commit/c71115))
- MCP: move_cursor awaits frontend confirmation ([6341c25](https://github.com/vdavid/cmdr/commit/6341c25))

### Fixed

- Fix MTP move conflicts silently overwriting ([27f2ff](https://github.com/vdavid/cmdr/commit/27f2ff))
- Fix MTP watcher missing external file changes ([266026](https://github.com/vdavid/cmdr/commit/266026))
- Fix MTP event debouncer dropping suppressed events ([21b3bc](https://github.com/vdavid/cmdr/commit/21b3bc))
- Fix MTP pane falling back to local root after copy ([9deba7](https://github.com/vdavid/cmdr/commit/9deba7))
- Fix MTP volumes missing from copy/move dialog ([cd6603](https://github.com/vdavid/cmdr/commit/cd6603))
- Fix MTP event-loop lock contention blocking copy/move/scan ([0461e33](https://github.com/vdavid/cmdr/commit/0461e33),
  [547a41](https://github.com/vdavid/cmdr/commit/547a41))
- Fix MTP scan preview showing 0/0/0 in confirmation dialog ([4e1efa](https://github.com/vdavid/cmdr/commit/4e1efa))
- Fix MTP rename conflicts not showing dialog on non-local volumes
  ([25f2b2](https://github.com/vdavid/cmdr/commit/25f2b2))
- Fix copy "Cancel" (keep partial files) triggering unintended rollback
  ([3042f2](https://github.com/vdavid/cmdr/commit/3042f2))
- Fix copy cancel hanging 30+ s on network mounts ([816e9e](https://github.com/vdavid/cmdr/commit/816e9e))
- Fix UI blocking on network filesystem ops ([bed59d](https://github.com/vdavid/cmdr/commit/bed59d))
- Fix indexing replay progress showing "Scanning..." instead of replay overlay
  ([32c053](https://github.com/vdavid/cmdr/commit/32c053))
- Push-based volume selector, fixes mount/unmount races ([b09665](https://github.com/vdavid/cmdr/commit/b09665))
- Fix volume path resolution to <1 ms regardless of mount health, handle APFS firmlinks
  ([5a1f78](https://github.com/vdavid/cmdr/commit/5a1f78))
- Harden unsafe Rust (main-thread markers, scoped Send impls, SAFETY comments)
  ([541804](https://github.com/vdavid/cmdr/commit/541804))

### Improved

- Typed write-op errors (9 variants) replace string parsing ([c10e06](https://github.com/vdavid/cmdr/commit/c10e06))
- Typed MTP volume errors ([8f2296](https://github.com/vdavid/cmdr/commit/8f2296))
- Backend owns MTP move strategy, frontend no longer orchestrates
  ([547a41](https://github.com/vdavid/cmdr/commit/547a41))
- Demote noisy per-file copy/move/MTP logs from INFO to DEBUG ([357fef](https://github.com/vdavid/cmdr/commit/357fef))

### Non-app

- Fix all WCAG violations found by axe-core ([d29a7c](https://github.com/vdavid/cmdr/commit/d29a7c),
  [438046](https://github.com/vdavid/cmdr/commit/438046), [6e6230](https://github.com/vdavid/cmdr/commit/6e6230))
- Port E2E tests from WebDriverIO to Playwright; add 80+ tests (MTP, SMB, conflicts, a11y, indexing)
  ([77d05937](https://github.com/vdavid/cmdr/commit/77d05937),
  [7d58bd6c](https://github.com/vdavid/cmdr/commit/7d58bd6c),
  [4f83aeb8](https://github.com/vdavid/cmdr/commit/4f83aeb8))
- Replace Prettier with oxfmt (10–20× faster) ([995f8c](https://github.com/vdavid/cmdr/commit/995f8c))
- Split indexing module (1951 lines) into focused files ([390864](https://github.com/vdavid/cmdr/commit/390864))
- Add light/dark website theme, features page, OG images, blog Like buttons
  ([49dbe782](https://github.com/vdavid/cmdr/commit/49dbe782),
  [98bdcc35](https://github.com/vdavid/cmdr/commit/98bdcc35),
  [56a9e764](https://github.com/vdavid/cmdr/commit/56a9e764),
  [5cff7c35](https://github.com/vdavid/cmdr/commit/5cff7c35))
- Dashboard: color-coded charts, GitHub star tracking, error reporting
  ([4b7c9e1e](https://github.com/vdavid/cmdr/commit/4b7c9e1e),
  [67efc4ae](https://github.com/vdavid/cmdr/commit/67efc4ae),
  [2e26b956](https://github.com/vdavid/cmdr/commit/2e26b956))

## [0.9.1] - 2026-03-24

### Fixed

- Fix orphaned llama-server processes after rapid AI provider switching
  ([b3382e](https://github.com/vdavid/cmdr/commit/b3382e))
- Fix vendor-specific MTP detection (Kindle, USB class 0xFF) via mtp-rs 0.4.1
  ([1a170d](https://github.com/vdavid/cmdr/commit/1a170d))

### Non-app

- API server: migrate telemetry to D1, add crash email notifications via Resend, rename license-server to api-server
  ([7dc0da](https://github.com/vdavid/cmdr/commit/7dc0da))
- Split search.rs (2361 lines) and SearchDialog.svelte (1552 lines) into focused modules
  ([c17c21](https://github.com/vdavid/cmdr/commit/c17c21))
- Deduplicate repeated patterns across Rust, Svelte, TS, and Go ([52afe3](https://github.com/vdavid/cmdr/commit/52afe3))
- Bump 9 Rust deps (reqwest 0.13, rusqlite 0.39, notify-debouncer-full 0.7, etc.)
  ([929556](https://github.com/vdavid/cmdr/commit/929556))
- Skip pnpm install when lockfile unchanged (~20 s saved per run)
  ([8d2b39](https://github.com/vdavid/cmdr/commit/8d2b39))
- Blog: add Kindle support article ([5c9d5b](https://github.com/vdavid/cmdr/commit/5c9d5b))

## [0.9.0] - 2026-03-23

### Added

- Add whole-drive file search (⌘F): glob/regex, size/date filters, scope, AI mode, MCP search and ai_search tools
  ([058136](https://github.com/vdavid/cmdr/commit/058136), [15110c](https://github.com/vdavid/cmdr/commit/15110c),
  [8c3546](https://github.com/vdavid/cmdr/commit/8c3546), [cf5827](https://github.com/vdavid/cmdr/commit/cf5827),
  [415db3](https://github.com/vdavid/cmdr/commit/415db3), [21d32e](https://github.com/vdavid/cmdr/commit/21d32e),
  [26d682](https://github.com/vdavid/cmdr/commit/26d682))
- Add opt-in crash reporting (panic hook + signal handler, inspect-and-send dialog, no PII)
  ([016ee3](https://github.com/vdavid/cmdr/commit/016ee3), [be29af](https://github.com/vdavid/cmdr/commit/be29af))
- Add Shift+F4 (Total Commander style): create new file, open in default editor
  ([da8ca9](https://github.com/vdavid/cmdr/commit/da8ca9))
- Add smart size display (min logical/physical, dual-size tooltips, hardlink dedup, mismatch icons)
  ([1d666a](https://github.com/vdavid/cmdr/commit/1d666a), [b302d0](https://github.com/vdavid/cmdr/commit/b302d0),
  [065820](https://github.com/vdavid/cmdr/commit/065820), [1d588f](https://github.com/vdavid/cmdr/commit/1d588f),
  [a93a8b](https://github.com/vdavid/cmdr/commit/a93a8b), [9c450c](https://github.com/vdavid/cmdr/commit/9c450c))
- Add sortable Ext column in Full mode ([e834b4](https://github.com/vdavid/cmdr/commit/e834b4))
- Add replay progress overlay during cold-start ([f166b0](https://github.com/vdavid/cmdr/commit/f166b0))
- Show live MTP disk space in volume dropdown and status bar ([b155f1](https://github.com/vdavid/cmdr/commit/b155f1),
  [c4cc26](https://github.com/vdavid/cmdr/commit/c4cc26))
- Show MTP loading progress on large folders ([77ebaa](https://github.com/vdavid/cmdr/commit/77ebaa))
- Add focus indicators on search and command palette inputs ([179221](https://github.com/vdavid/cmdr/commit/179221))
- Selection summary includes directory sizes ([3928c1c](https://github.com/vdavid/cmdr/commit/3928c1c))
- MCP: show directory sizes in state resource ([9cb775](https://github.com/vdavid/cmdr/commit/9cb775))

### Fixed

- Fix multi-GB macOS memory leak (ObjC calls on background threads now run inside autoreleasepool)
  ([777f9e](https://github.com/vdavid/cmdr/commit/777f9e))
- Fix stack overflow in sync status (8 MB OS threads instead of rayon for NSURL/XPC calls)
  ([fa28cd](https://github.com/vdavid/cmdr/commit/fa28cd))
- Fix size overcounting (hardlink dedup, exclude cloud-only files, smart-size for dataless)
  ([fe5eff](https://github.com/vdavid/cmdr/commit/fe5eff))
- Fix file watcher: instant updates in large dirs via incremental diffs
  ([df558e](https://github.com/vdavid/cmdr/commit/df558e))
- Fix selection clearing after file ops; gradual deselection per source item
  ([538ec5](https://github.com/vdavid/cmdr/commit/538ec5))
- Fix selection indices drifting after external file changes ([453ec0](https://github.com/vdavid/cmdr/commit/453ec0))
- Fix cursor lost after deleting all files ([17808d](https://github.com/vdavid/cmdr/commit/17808d))
- Fix stale dir sizes on rename ([10213d](https://github.com/vdavid/cmdr/commit/10213d))
- Fix indexing not starting on fresh DB ([a61376d](https://github.com/vdavid/cmdr/commit/a61376d))
- Fix "Scanning..." stuck after replay ([4a44d7](https://github.com/vdavid/cmdr/commit/4a44d7),
  [fb796e](https://github.com/vdavid/cmdr/commit/fb796e))
- Fix verifier + replay transaction conflict via named savepoints
  ([72ca9f](https://github.com/vdavid/cmdr/commit/72ca9f))
- Fix MTP browsing panic; show device name on single-storage devices
  ([d37b8a](https://github.com/vdavid/cmdr/commit/d37b8a))
- Fix MTP duplicate directory listing on connect ([17efe8](https://github.com/vdavid/cmdr/commit/17efe8))
- Fix MCP stale state after server crash; auto-probe port when configured port is in use
  ([0369d2](https://github.com/vdavid/cmdr/commit/0369d2), [d69f87](https://github.com/vdavid/cmdr/commit/d69f87))
- Fix OpenAI compatibility ([795a67](https://github.com/vdavid/cmdr/commit/795a67))
- Hide misleading rollback button for move ops ([fbdba5](https://github.com/vdavid/cmdr/commit/fbdba5))
- Raise replay/journal gap thresholds to reduce unnecessary full rescans
  ([377919](https://github.com/vdavid/cmdr/commit/377919), [af2bf7](https://github.com/vdavid/cmdr/commit/af2bf7))

### Non-app

- Add full-stack analytics dashboard (6 data sources, agent-readable report)
  ([b4f740](https://github.com/vdavid/cmdr/commit/b4f740), [0766c4](https://github.com/vdavid/cmdr/commit/0766c4),
  [b97028](https://github.com/vdavid/cmdr/commit/b97028))
- Enforce CSS design tokens via Stylelint ([50f2b4](https://github.com/vdavid/cmdr/commit/50f2b4),
  [e3259b](https://github.com/vdavid/cmdr/commit/e3259b), [36b340](https://github.com/vdavid/cmdr/commit/36b340))
- Drop desktop smoke tests, speed up store tests by ~20 s ([c6210a](https://github.com/vdavid/cmdr/commit/c6210a),
  [dab071](https://github.com/vdavid/cmdr/commit/dab071))
- Reduce code duplication across write ops, listing, events, search dialog
  ([33ec2f](https://github.com/vdavid/cmdr/commit/33ec2f))
- Website: story + testimonials sections, landing page polish, Docker healthcheck, Remark42 CSP
  ([d5a7f4](https://github.com/vdavid/cmdr/commit/d5a7f4), [51acd8](https://github.com/vdavid/cmdr/commit/51acd8),
  [424a80](https://github.com/vdavid/cmdr/commit/424a80), [dd5e34](https://github.com/vdavid/cmdr/commit/dd5e34))
- Bump mtp-rs to 0.2.0 ([634255](https://github.com/vdavid/cmdr/commit/634255))

## [0.8.2] - 2026-03-15

### Fixed

- Fix crash on launch after auto-update (kernel code-signing cache SIGKILL: temp + rename for a fresh inode)
  ([d2923af](https://github.com/vdavid/cmdr/commit/d2923af))
- Fix indexing drift: per-navigation verifier with 30 s debounce; skip /System and /dev as empty stubs
  ([0f28b51](https://github.com/vdavid/cmdr/commit/0f28b51), [b0b1730](https://github.com/vdavid/cmdr/commit/b0b1730))
- Fix dir size display during indexing (refresh on aggregation-complete, not scan-complete)
  ([d0746fb](https://github.com/vdavid/cmdr/commit/d0746fb))
- Fix navigation latency: fire-and-forget verification, parallelize 6 listen() calls
  ([a4e87f1](https://github.com/vdavid/cmdr/commit/a4e87f1))
- Fix indexing perf (integer-only index: 25 min to seconds on 5.1M entries; 99% replay-event dedup)
  ([a5b5beb](https://github.com/vdavid/cmdr/commit/a5b5beb), [44fecd6](https://github.com/vdavid/cmdr/commit/44fecd6),
  [d9877c1](https://github.com/vdavid/cmdr/commit/d9877c1))

### Non-app

- Separate dev and prod log dirs, fix Linux test output capture, fix smoke test timeout
  ([e8762be](https://github.com/vdavid/cmdr/commit/e8762be), [83d2365](https://github.com/vdavid/cmdr/commit/83d2365),
  [88901f9](https://github.com/vdavid/cmdr/commit/88901f9))
- Improve agent instructions ([dec19cf](https://github.com/vdavid/cmdr/commit/dec19cf))

## [0.8.1] - 2026-03-14

### Fixed

- Fix indexing (lock-free dir-stats reads, drop stale PathResolver cache, fix "DB is locked", fix overlay race, lost
  scan metadata, dir→file replacement orphans) ([50bd4fa](https://github.com/vdavid/cmdr/commit/50bd4fa),
  [44abfd1](https://github.com/vdavid/cmdr/commit/44abfd1), [7319c5c](https://github.com/vdavid/cmdr/commit/7319c5c),
  [26785fc](https://github.com/vdavid/cmdr/commit/26785fc), [795e48b](https://github.com/vdavid/cmdr/commit/795e48b),
  [424eedb](https://github.com/vdavid/cmdr/commit/424eedb), [dbccec1](https://github.com/vdavid/cmdr/commit/dbccec1),
  [8f87a4f](https://github.com/vdavid/cmdr/commit/8f87a4f))
- Fix traffic light position in production builds ([7551df2](https://github.com/vdavid/cmdr/commit/7551df2))

### Non-app

- Add indexing concurrency stress tests, event loop tests, reconciler tests
  ([3ad3adc](https://github.com/vdavid/cmdr/commit/3ad3adc), [8a084cd](https://github.com/vdavid/cmdr/commit/8a084cd),
  [dbccec1](https://github.com/vdavid/cmdr/commit/dbccec1))

## [0.8.0] - 2026-03-13

### Added

- Add custom macOS updater that preserves Full Disk Access (syncs into existing .app bundle, privilege escalation)
  ([190a637](https://github.com/vdavid/cmdr/commit/190a637))
- Add MTP delete, rename, move (full progress, cancel, dry-run)
  ([812ad07](https://github.com/vdavid/cmdr/commit/812ad07))
- Add breadcrumb polish ("/" prefix, "~" for home) ([44b7105](https://github.com/vdavid/cmdr/commit/44b7105))
- Add auto-rescan on FSEvents channel overflow ([ca7cece](https://github.com/vdavid/cmdr/commit/ca7cece))
- Add index debug dashboard (DB stats, watcher status, event-rate sparkline)
  ([7510ec3](https://github.com/vdavid/cmdr/commit/7510ec3))

### Fixed

- Fix indexing (interrupt-safe reconciler, stop micro-scans, faster bulk inserts, false FSEvents deletes, missing dir
  sizes after replay, periodic DB vacuum) ([31df59e](https://github.com/vdavid/cmdr/commit/31df59e),
  [981b311](https://github.com/vdavid/cmdr/commit/981b311), [da74290](https://github.com/vdavid/cmdr/commit/da74290),
  [f0c225f](https://github.com/vdavid/cmdr/commit/f0c225f), [bf0b47f](https://github.com/vdavid/cmdr/commit/bf0b47f),
  [d125a24](https://github.com/vdavid/cmdr/commit/d125a24), [67684bb](https://github.com/vdavid/cmdr/commit/67684bb))
- Fix drag swizzle failing on wry 0.54+ ([2680bae](https://github.com/vdavid/cmdr/commit/2680bae))
- Fix MCP live start/stop UX (backend state as ground truth, port auto-check)
  ([f4c107a](https://github.com/vdavid/cmdr/commit/f4c107a))
- Fix MCP server not stopping on app quit ([61fe290](https://github.com/vdavid/cmdr/commit/61fe290))
- Fix traffic light position in production builds ([b74ed39](https://github.com/vdavid/cmdr/commit/b74ed39))
- Fix scan overlay showing stale state ([218bcb9](https://github.com/vdavid/cmdr/commit/218bcb9))

### Non-app

- Vendor cmdr-fsevent-stream fork as workspace crate ([8b937a6](https://github.com/vdavid/cmdr/commit/8b937a6))
- Fix two FOUC flickers on website page load ([8c21ac7](https://github.com/vdavid/cmdr/commit/8c21ac7))
- Set up self-hosted macOS GitHub Actions runner; add index DB query tool, website deploy workflow extracted
  ([665f63a](https://github.com/vdavid/cmdr/commit/665f63a), [37f1062](https://github.com/vdavid/cmdr/commit/37f1062),
  [5744636](https://github.com/vdavid/cmdr/commit/5744636))
- Pink title bar in dev to distinguish from prod ([d2c9ae4](https://github.com/vdavid/cmdr/commit/d2c9ae4))

## [0.7.1] - 2026-03-12

### Fixed

- Fix scan overlay stuck at 100% after directory size aggregation
  ([424eedb](https://github.com/vdavid/cmdr/commit/424eedb))

## [0.7.0] - 2026-03-12

### Added

- Add AI settings: three providers (off / cloud / local LLM), 15 cloud presets, per-provider keys, model combobox, RAM
  gauge, context size ([b41365b](https://github.com/vdavid/cmdr/commit/b41365b),
  [abfc248](https://github.com/vdavid/cmdr/commit/abfc248), [423e669](https://github.com/vdavid/cmdr/commit/423e669))
- Live MCP server start/stop in Settings (no app restart) ([e0c55e7](https://github.com/vdavid/cmdr/commit/e0c55e7))
- Add stale index detection with toast + auto-rescan ([b590a54](https://github.com/vdavid/cmdr/commit/b590a54))
- Add device tracking for license abuse, fair-use terms in ToS
  ([cf4f913](https://github.com/vdavid/cmdr/commit/cf4f913))
- Add license section to Settings (status display, action buttons, dynamic labels)
  ([39cf7b4](https://github.com/vdavid/cmdr/commit/39cf7b4))
- Improve app icon for macOS Sequoia ([cc80d28](https://github.com/vdavid/cmdr/commit/cc80d28))

### Changed

- Drop supporter license tier (legacy keys map to Personal) ([c0a63f5](https://github.com/vdavid/cmdr/commit/c0a63f5))
- Split Settings UI horizontally 50/50 ([9493f88](https://github.com/vdavid/cmdr/commit/9493f88))
- Rename settings-v2.json to settings.json ([d987cc8](https://github.com/vdavid/cmdr/commit/d987cc8))

### Fixed

- Fix startup panic from blocking_lock in async context ([f9855ca](https://github.com/vdavid/cmdr/commit/f9855ca))
- Fix SQLite write pragmas on read-only connections (panic in subtree scans)
  ([a53a275](https://github.com/vdavid/cmdr/commit/a53a275))
- Fix llama-server not stopping on quit, stale PIDs, excess memory (256k to 4k default context)
  ([eae70f1](https://github.com/vdavid/cmdr/commit/eae70f1), [ffcbc81](https://github.com/vdavid/cmdr/commit/ffcbc81),
  [e45c742](https://github.com/vdavid/cmdr/commit/e45c742))
- Fix Settings UI freezing ~5 s when stopping AI server (instant SIGKILL for stateless llama-server)
  ([2af7ee8](https://github.com/vdavid/cmdr/commit/2af7ee8))
- Separate dev/prod data dir and MCP port ([b8b058a](https://github.com/vdavid/cmdr/commit/b8b058a))
- Fix fallback path resolution falling to / instead of ~ ([8d7c644](https://github.com/vdavid/cmdr/commit/8d7c644))
- Fix indexing (100× faster aggregation, DB auto-vacuum, truncate before full scan)
  ([47a2e8e](https://github.com/vdavid/cmdr/commit/47a2e8e), [cad1af5](https://github.com/vdavid/cmdr/commit/cad1af5),
  [aff2046](https://github.com/vdavid/cmdr/commit/aff2046), [96323e9](https://github.com/vdavid/cmdr/commit/96323e9))
- Fix FSEvents storms causing memory pressure (mimalloc, 1 s dedup window)
  ([207ddee](https://github.com/vdavid/cmdr/commit/207ddee))

### Non-app

- Replace 19 ADRs with colocated Decision/Why entries in 11 CLAUDE.md files; slim AGENTS.md from 245 to 93 lines
  ([ccf5cc7](https://github.com/vdavid/cmdr/commit/ccf5cc7), [d297a1a](https://github.com/vdavid/cmdr/commit/d297a1a),
  [0595796](https://github.com/vdavid/cmdr/commit/0595796))
- Website: version + file size on download buttons, fix Intel/Apple detection flicker
  ([bd17056](https://github.com/vdavid/cmdr/commit/bd17056), [ec35b1f](https://github.com/vdavid/cmdr/commit/ec35b1f))
- Add html-validate and circular-dep checks ([3dbd5af](https://github.com/vdavid/cmdr/commit/3dbd5af),
  [4bead2b](https://github.com/vdavid/cmdr/commit/4bead2b))
- Eliminate all circular deps via refactor (volume grouping, menu platform code, viewer scroll/search)
  ([7740fbc](https://github.com/vdavid/cmdr/commit/7740fbc), [8522e71](https://github.com/vdavid/cmdr/commit/8522e71),
  [e16bd91](https://github.com/vdavid/cmdr/commit/e16bd91), [7ed1cea](https://github.com/vdavid/cmdr/commit/7ed1cea))

## [0.6.1] - 2026-03-10

### Added

- Add top menu icons ([1a2621a](https://github.com/vdavid/cmdr/commit/1a2621a))
- Add View, Copy, Move, New folder, and Delete actions to context menu
  ([a966f17](https://github.com/vdavid/cmdr/commit/a966f17))

### Fixed

- Fix OOM crash from unbounded indexing buffers; toggling Full Disk Access could replay millions of FSEvents with zero
  backpressure, consuming 500+ GB RAM. All buffers are now bounded (~350 MB peak), with a memory watchdog that stops
  indexing at 16 GB ([f1501ec](https://github.com/vdavid/cmdr/commit/f1501ec))

### Non-app

- Website: add llms.txt, Schema.org JSON-LD, and auto-generated sitemap for agent accessibility
  ([ba64c36](https://github.com/vdavid/cmdr/commit/ba64c36))
- Website: update roadmap ([5197120](https://github.com/vdavid/cmdr/commit/5197120))
- CI: simplify release pipeline, download sigs directly from release, generate `latest.json` with `jq`, validate all 3
  sigs before proceeding ([d3095cb](https://github.com/vdavid/cmdr/commit/d3095cb),
  [5b82cd0](https://github.com/vdavid/cmdr/commit/5b82cd0))
- CI: fix Backspace E2E test on WebKitGTK, fix CI failures, fix 3 flaky tests
  ([7c22951](https://github.com/vdavid/cmdr/commit/7c22951), [79f593c](https://github.com/vdavid/cmdr/commit/79f593c),
  [8f4ea82](https://github.com/vdavid/cmdr/commit/8f4ea82))
- Docs: add troubleshooting section to releasing guide ([1768b29](https://github.com/vdavid/cmdr/commit/1768b29))

## [0.6.0] - 2026-03-08

### Added

- Add Linux support (alpha): volumes via /proc/mounts, file ops with reflink support, trash via FreeDesktop spec,
  inotify file watching, MTP ungated, SMB via mDNS + smbclient fallback, GVFS-mounted shares as volumes, native file
  icons via freedesktop-icons, accent color via XDG Desktop Portal, encrypted credential fallback when no system
  keyring, distro-specific install hints, USB permission handling
  ([b6e80f6](https://github.com/vdavid/cmdr/commit/b6e80f6), [20be0c3](https://github.com/vdavid/cmdr/commit/20be0c3),
  [9c51fa9](https://github.com/vdavid/cmdr/commit/9c51fa9), [64e41f9](https://github.com/vdavid/cmdr/commit/64e41f9),
  [40cc1a9](https://github.com/vdavid/cmdr/commit/40cc1a9), [c3ad1ed](https://github.com/vdavid/cmdr/commit/c3ad1ed),
  [d40ea25](https://github.com/vdavid/cmdr/commit/d40ea25), [60063ec](https://github.com/vdavid/cmdr/commit/60063ec),
  [e65d993](https://github.com/vdavid/cmdr/commit/e65d993), [22e2ea7](https://github.com/vdavid/cmdr/commit/22e2ea7),
  [afe2609](https://github.com/vdavid/cmdr/commit/afe2609), [4bbcbb0](https://github.com/vdavid/cmdr/commit/4bbcbb0),
  [48af543](https://github.com/vdavid/cmdr/commit/48af543))
- Add per-pane tab support: ⌘T/⌘W, ⌃Tab cycling, pin/unpin, context menu, persistence with migration, per-tab sort
  ([791a29a](https://github.com/vdavid/cmdr/commit/791a29a))
- Add delete/trash feature (F8): trash by default, ⇧F8 for permanent delete, confirmation dialog with scan preview,
  batch progress with cancellation, volume trash support detection
  ([e3560a3](https://github.com/vdavid/cmdr/commit/e3560a3))
- Add clipboard for files: ⌘C/⌘V/⌘X with Finder interop, ⌥⌘V for "Move here", cut state tracking, text clipboard in all
  windows via NSPasteboard ([0dc2953](https://github.com/vdavid/cmdr/commit/0dc2953),
  [60baeba](https://github.com/vdavid/cmdr/commit/60baeba))
- Add toast notification system with centralized store, dedup, stacking, three levels, transient/persistent modes
  ([6c5c452](https://github.com/vdavid/cmdr/commit/6c5c452), [2329f2f](https://github.com/vdavid/cmdr/commit/2329f2f))
- Add per-pane disk space display: 2px usage bar, free-space text in status bar, mini bars in volume dropdown
  ([9b6d057](https://github.com/vdavid/cmdr/commit/9b6d057))
- Add custom tooltips with glass material effect, shortcut badges, smart positioning, accessibility support, replacing
  all native tooltips ([3c7f965](https://github.com/vdavid/cmdr/commit/3c7f965))
- Add drive indexing with integer-keyed DB schema (7.4x size reduction, 3.8 GB → 0.54 GB), LRU path cache,
  platform-aware collation, recursive CTE aggregation ([7c5d3ce](https://github.com/vdavid/cmdr/commit/7c5d3ce),
  [daee97b](https://github.com/vdavid/cmdr/commit/daee97b), [5e10fa9](https://github.com/vdavid/cmdr/commit/5e10fa9),
  [68be3ab](https://github.com/vdavid/cmdr/commit/68be3ab))
- Add IPC hardening: timeout-protect all filesystem commands, transparent timeout UI with retry/fallback for volumes,
  tabs, file ops, and viewer ([6a58278](https://github.com/vdavid/cmdr/commit/6a58278),
  [71de96e](https://github.com/vdavid/cmdr/commit/71de96e))
- Add accent color option in Settings: macOS theme or Cmdr gold, "Recolor to gold" for folder icons
  ([330e824](https://github.com/vdavid/cmdr/commit/330e824), [ef9de79](https://github.com/vdavid/cmdr/commit/ef9de79))
- Add directory sorting by size with toggle in Settings ([a7dd8ca](https://github.com/vdavid/cmdr/commit/a7dd8ca))
- Add "Forget saved password" UI for SMB network shares ([7d751d5](https://github.com/vdavid/cmdr/commit/7d751d5))
- Add path validation in copy/move and mkdir dialogs with platform-correct limits
  ([6b295ec](https://github.com/vdavid/cmdr/commit/6b295ec))
- Add centralized keyboard shortcut dispatch with runtime custom bindings
  ([e40bcc2](https://github.com/vdavid/cmdr/commit/e40bcc2))
- Add macOS entitlements and TCC usage descriptions for proper permission prompts
  ([ff0c27e](https://github.com/vdavid/cmdr/commit/ff0c27e))
- Add Apple code signing, notarization, and arch-specific downloads (aarch64, x86_64, universal)
  ([b03f91e](https://github.com/vdavid/cmdr/commit/b03f91e), [944085f](https://github.com/vdavid/cmdr/commit/944085f))
- Add licensing UI improvements: verify/commit split, typed errors, short code in signed payload, Paddle live setup
  ([0abc704](https://github.com/vdavid/cmdr/commit/0abc704), [1f2308b](https://github.com/vdavid/cmdr/commit/1f2308b))

### Fixed

- Fix file viewer: search progress bar with spinner and stop button, incremental match delivery, 10k match cap,
  byte-seek navigation, loading very long files ([9c0a3c3](https://github.com/vdavid/cmdr/commit/9c0a3c3),
  [a3b9d0e](https://github.com/vdavid/cmdr/commit/a3b9d0e), [31cf5fd](https://github.com/vdavid/cmdr/commit/31cf5fd),
  [d15ecde](https://github.com/vdavid/cmdr/commit/d15ecde), [86ef2a5](https://github.com/vdavid/cmdr/commit/86ef2a5),
  [0fcdb13](https://github.com/vdavid/cmdr/commit/0fcdb13), [8b57bbe](https://github.com/vdavid/cmdr/commit/8b57bbe))
- Fix 3–10s startup block from index enrichment holding the mutex
  ([267e02b](https://github.com/vdavid/cmdr/commit/267e02b))
- Fix mDNS host resolution arriving before discovery, causing SMB auth failures
  ([2dda99b](https://github.com/vdavid/cmdr/commit/2dda99b))
- Fix focus escaping panes with focus guard, removing ~50 redundant refocus calls
  ([4c9aadc](https://github.com/vdavid/cmdr/commit/4c9aadc))
- Fix clipboard shortcuts in text fields on macOS ([20f3de0](https://github.com/vdavid/cmdr/commit/20f3de0))
- Fix non-blocking navigation on slow/dead SMB shares with timeouts and optimistic UI
  ([c85c8c2](https://github.com/vdavid/cmdr/commit/c85c8c2))
- Fix copy feature: auto-rollback on panic, deadlock prevention, cancel race condition
  ([2b17ab5](https://github.com/vdavid/cmdr/commit/2b17ab5))
- Fix status bar not refreshing after file watcher diffs ([e880f9f](https://github.com/vdavid/cmdr/commit/e880f9f))
- Fix pinned tab volume change now opens new tab instead of navigating in-place
  ([ff4c8f2](https://github.com/vdavid/cmdr/commit/ff4c8f2))
- Fix cancel-loading to return to previous folder instead of home
  ([8ff2379](https://github.com/vdavid/cmdr/commit/8ff2379))
- Fix ⌘, to refocus Settings window if already open ([71b3e61](https://github.com/vdavid/cmdr/commit/71b3e61))
- Fix Settings: ⌥+key shortcuts showing "Dead" on macOS, key filter subset matching, ESC clears filter
  ([1fd540a](https://github.com/vdavid/cmdr/commit/1fd540a), [5056bb6](https://github.com/vdavid/cmdr/commit/5056bb6),
  [47050e0](https://github.com/vdavid/cmdr/commit/47050e0))
- Fix settings not initialized warning at startup ([b540fcc](https://github.com/vdavid/cmdr/commit/b540fcc))
- Fix SMB share showing 0 bytes free on network filesystems ([f791153](https://github.com/vdavid/cmdr/commit/f791153))
- Fix volumes cached to prevent timeout at startup ([024e48f](https://github.com/vdavid/cmdr/commit/024e48f))
- Fix top menu items staying enabled on non-main windows ([7572d13](https://github.com/vdavid/cmdr/commit/7572d13))
- Fix live file count during large folder loading ([7815d0f](https://github.com/vdavid/cmdr/commit/7815d0f))
- Fix window content height for production builds ([0cbd0fd](https://github.com/vdavid/cmdr/commit/0cbd0fd))
- Fix folder icons updating on OS theme change ([6b02445](https://github.com/vdavid/cmdr/commit/6b02445))
- Fix focus lost after rename cancellation ([edace18](https://github.com/vdavid/cmdr/commit/edace18))
- Fix file viewer not loading settings ([acfef93](https://github.com/vdavid/cmdr/commit/acfef93))
- Fix drive indexing: orphaned entries, missing dir sizes, background scan failures, DB transaction issues
  ([323ae86](https://github.com/vdavid/cmdr/commit/323ae86), [004f302](https://github.com/vdavid/cmdr/commit/004f302),
  [c331143](https://github.com/vdavid/cmdr/commit/c331143))
- Fix MCP protocol version mismatch warnings at startup ([2af0b90](https://github.com/vdavid/cmdr/commit/2af0b90))
- Fix arrow up/down performance in large folders ([e6f268c](https://github.com/vdavid/cmdr/commit/e6f268c))
- Fix PostHog CSP and make it cookieless ([1700d99](https://github.com/vdavid/cmdr/commit/1700d99),
  [9cea85a](https://github.com/vdavid/cmdr/commit/9cea85a))
- Fix app loading slowly due to startup optimizations: license cache, async validation
  ([3835866](https://github.com/vdavid/cmdr/commit/3835866), [87de136](https://github.com/vdavid/cmdr/commit/87de136))

### Non-app

- Overhaul native menus on macOS and Linux: build from scratch, strip macOS system-injected items, unify dispatch via
  single event, context-aware graying, full accelerator sync ([b38c552](https://github.com/vdavid/cmdr/commit/b38c552))
- Unify frontend + backend logging via tauri-plugin-log, demote noisy log levels, suppress smb/sspi noise
  ([22f4ab5](https://github.com/vdavid/cmdr/commit/22f4ab5), [dbbcc55](https://github.com/vdavid/cmdr/commit/dbbcc55),
  [1e59a56](https://github.com/vdavid/cmdr/commit/1e59a56))
- Design system: unified button styles, consistent loading states, improved text readability, redesigned network screens
  ([8dc2e33](https://github.com/vdavid/cmdr/commit/8dc2e33), [4d07ad0](https://github.com/vdavid/cmdr/commit/4d07ad0),
  [71dbe0b](https://github.com/vdavid/cmdr/commit/71dbe0b), [b5d8b28](https://github.com/vdavid/cmdr/commit/b5d8b28),
  [a018a3e](https://github.com/vdavid/cmdr/commit/a018a3e), [90e2010](https://github.com/vdavid/cmdr/commit/90e2010))
- Docs overhaul: CLAUDE.md staleness checker in CI, enriched 25 CLAUDE.md files with Decision/Why entries, cross-cutting
  patterns in architecture.md, split infrastructure.md into per-service files
  ([ff8b3be](https://github.com/vdavid/cmdr/commit/ff8b3be), [347ae9b](https://github.com/vdavid/cmdr/commit/347ae9b),
  [f961f19](https://github.com/vdavid/cmdr/commit/f961f19), [2f7bff1](https://github.com/vdavid/cmdr/commit/2f7bff1))
- Website: add blog with first post, PostHog and Umami analytics, arch-specific download buttons, Docker build check,
  newsletter improvements ([01681c1](https://github.com/vdavid/cmdr/commit/01681c1),
  [75d5228](https://github.com/vdavid/cmdr/commit/75d5228), [78de573](https://github.com/vdavid/cmdr/commit/78de573),
  [ae8f6cb](https://github.com/vdavid/cmdr/commit/ae8f6cb), [34ecc70](https://github.com/vdavid/cmdr/commit/34ecc70))
- Check runner: CSV stats logging, cfg-gate enclosing block scope detection, file length check, flag combining fix
  ([9ac4b54](https://github.com/vdavid/cmdr/commit/9ac4b54), [539db62](https://github.com/vdavid/cmdr/commit/539db62),
  [4a24562](https://github.com/vdavid/cmdr/commit/4a24562), [6fe48a9](https://github.com/vdavid/cmdr/commit/6fe48a9))
- Refactors: split DualPaneExplorer and FilePane, extract dialog state, deduplicate templates and Settings CSS, split
  tauri-commands ([337f620](https://github.com/vdavid/cmdr/commit/337f620),
  [cfae0db](https://github.com/vdavid/cmdr/commit/cfae0db), [dad8790](https://github.com/vdavid/cmdr/commit/dad8790),
  [35a4239](https://github.com/vdavid/cmdr/commit/35a4239), [ba86d87](https://github.com/vdavid/cmdr/commit/ba86d87))
- License server: download tracking via Cloudflare Analytics Engine
  ([ef0f049](https://github.com/vdavid/cmdr/commit/ef0f049))
- Add Renovate for automated dependency updates ([00880a0](https://github.com/vdavid/cmdr/commit/00880a0))
- Add macOS Playwright E2E tests and CrabNebula E2E tests ([ec900ee](https://github.com/vdavid/cmdr/commit/ec900ee),
  [a768c03](https://github.com/vdavid/cmdr/commit/a768c03))
- Infra: uptime monitoring with UptimeRobot + Pushover, hardened deploy script
  ([19baefd](https://github.com/vdavid/cmdr/commit/19baefd))
- Add cfg-gate lint check for macOS-only Rust crates ([075c1d4](https://github.com/vdavid/cmdr/commit/075c1d4))

## [0.5.0] - 2026-02-15

### Added

- Add file viewer (F3) with three-backend architecture for files of any size, virtual scrolling, search with multibyte
  support, word wrap, horizontal scrolling, and keyboard shortcuts
  ([79268a4](https://github.com/vdavid/cmdr/commit/79268a4), [9f91bce](https://github.com/vdavid/cmdr/commit/9f91bce),
  [b10002a](https://github.com/vdavid/cmdr/commit/b10002a), [2ad2521](https://github.com/vdavid/cmdr/commit/2ad2521),
  [b65c422](https://github.com/vdavid/cmdr/commit/b65c422), [43adc86](https://github.com/vdavid/cmdr/commit/43adc86))
- Add drag-and-drop into Cmdr: pane and folder-level targeting, canvas overlay with file names and icons, Alt to switch
  copy/move, smart overlay suppression for large source images
  ([1ad1493](https://github.com/vdavid/cmdr/commit/1ad1493), [6207d8e](https://github.com/vdavid/cmdr/commit/6207d8e),
  [a89f18f](https://github.com/vdavid/cmdr/commit/a89f18f), [371746b](https://github.com/vdavid/cmdr/commit/371746b),
  [a3eae1c](https://github.com/vdavid/cmdr/commit/a3eae1c), [c776eed](https://github.com/vdavid/cmdr/commit/c776eed),
  [e97d3db](https://github.com/vdavid/cmdr/commit/e97d3db))
- Add settings window (⌘,) with declarative registry, fuzzy search, persistence, keyboard shortcut customization with
  conflict detection, and cross-window sync ([db121f6](https://github.com/vdavid/cmdr/commit/db121f6),
  [418f790](https://github.com/vdavid/cmdr/commit/418f790), [8f78596](https://github.com/vdavid/cmdr/commit/8f78596),
  [218b79b](https://github.com/vdavid/cmdr/commit/218b79b), [9c39db3](https://github.com/vdavid/cmdr/commit/9c39db3),
  [4e90137](https://github.com/vdavid/cmdr/commit/4e90137))
- Add MTP (Android device) support: browsing, file operations (copy, delete, rename, new folder), USB hotplug,
  multi-storage, MTP-to-MTP transfers ([938e87c](https://github.com/vdavid/cmdr/commit/938e87c),
  [672fa6e](https://github.com/vdavid/cmdr/commit/672fa6e), [d1e9f80](https://github.com/vdavid/cmdr/commit/d1e9f80),
  [7ac1528](https://github.com/vdavid/cmdr/commit/7ac1528), [b08af36](https://github.com/vdavid/cmdr/commit/b08af36),
  [ea845a6](https://github.com/vdavid/cmdr/commit/ea845a6), [fd8dad6](https://github.com/vdavid/cmdr/commit/fd8dad6))
- Add move feature (F6) reusing the copy UI as a unified transfer abstraction
  ([682d33a](https://github.com/vdavid/cmdr/commit/682d33a), [cb9e047](https://github.com/vdavid/cmdr/commit/cb9e047))
- Add rename feature with edge-case handling ([62799c6](https://github.com/vdavid/cmdr/commit/62799c6))
- Add swap panes feature with ⌘U shortcut ([2a1b329](https://github.com/vdavid/cmdr/commit/2a1b329))
- Add local AI for folder name suggestions in New Folder dialog, optional download
  ([b9a112e](https://github.com/vdavid/cmdr/commit/b9a112e), [3dc19c0](https://github.com/vdavid/cmdr/commit/3dc19c0))
- Add chunked copy with cancellation and pause support on network drives
  ([ba5409e](https://github.com/vdavid/cmdr/commit/ba5409e))
- Add 6 copy/move safety checks: path canonicalization, writability, disk space, inode identity, name length, special
  file filtering ([9548022](https://github.com/vdavid/cmdr/commit/9548022))
- Add sync status polling so iCloud/Dropbox icons update in real time
  ([ed36158](https://github.com/vdavid/cmdr/commit/ed36158), [6296412](https://github.com/vdavid/cmdr/commit/6296412))
- Add CSP to Tauri webview for XSS protection ([68bd510](https://github.com/vdavid/cmdr/commit/68bd510))
- Add copy/move folder-into-subfolder warning with clear error message
  ([521ab5e](https://github.com/vdavid/cmdr/commit/521ab5e))

### Fixed

- Fix panes getting stale when current directory or its parents are deleted
  ([1b5ad52](https://github.com/vdavid/cmdr/commit/1b5ad52))
- Fix multi-window race conditions that could crash the app ([9a33e24](https://github.com/vdavid/cmdr/commit/9a33e24))
- Fix recovering from poisoned mutexes instead of crashing (56 lock sites)
  ([62fd685](https://github.com/vdavid/cmdr/commit/62fd685))
- Fix wrong cursor position after show/hide hidden files ([223b041](https://github.com/vdavid/cmdr/commit/223b041))
- Fix selection and cursor position breaking on sort change ([36d61d0](https://github.com/vdavid/cmdr/commit/36d61d0))
- Fix panel unresponsive after Brief/Full view change ([2b6d513](https://github.com/vdavid/cmdr/commit/2b6d513))
- Fix copy operationId capture race condition ([9b5c57c](https://github.com/vdavid/cmdr/commit/9b5c57c))
- Fix $effect listener cleanup race in FilePane ([e2c6ee1](https://github.com/vdavid/cmdr/commit/e2c6ee1))
- Fix condvar hang on unresolved conflict dialog ([2975c45](https://github.com/vdavid/cmdr/commit/2975c45))
- Fix first click on main window not changing file focus ([59c5da4](https://github.com/vdavid/cmdr/commit/59c5da4))
- Fix AppleScript injection in get_info command ([e3378c3](https://github.com/vdavid/cmdr/commit/e3378c3))
- Fix URL-encoding of SMB username in smbutil URLs ([f908a74](https://github.com/vdavid/cmdr/commit/f908a74))
- Fix mouse/keyboard interaction bug for volume picker ([8afd0de](https://github.com/vdavid/cmdr/commit/8afd0de))
- Fix drop coordinates when DevTools is docked ([a9a041f](https://github.com/vdavid/cmdr/commit/a9a041f))
- Fix MCP server always returning left pane as selected ([2f9160a](https://github.com/vdavid/cmdr/commit/2f9160a))
- Redact PII from production log statements ([fe31316](https://github.com/vdavid/cmdr/commit/fe31316))

### Non-app

- Migrate network discovery from NSNetServiceBrowser to mdns-sd: 68% code reduction, no unsafe code
  ([3d44cf1](https://github.com/vdavid/cmdr/commit/3d44cf1))
- Rewrite MCP server with fewer tools but more capabilities, auto-reconnect, and instructions field
  ([1061fad](https://github.com/vdavid/cmdr/commit/1061fad), [ede6463](https://github.com/vdavid/cmdr/commit/ede6463),
  [82345d1](https://github.com/vdavid/cmdr/commit/82345d1))
- Introduce ModalDialog component for all soft modals with drag support
  ([ffbf14a](https://github.com/vdavid/cmdr/commit/ffbf14a))
- Major refactors: split DualPaneExplorer, FilePane, volume_copy, listing/operations, connection modules
  ([04dc3de](https://github.com/vdavid/cmdr/commit/04dc3de), [e14c289](https://github.com/vdavid/cmdr/commit/e14c289),
  [2da8e6d](https://github.com/vdavid/cmdr/commit/2da8e6d), [c0bd500](https://github.com/vdavid/cmdr/commit/c0bd500),
  [707a96a](https://github.com/vdavid/cmdr/commit/707a96a))
- Security: pin GitHub Actions to commit SHAs, fix Paddle webhook timing attack, use crypto.getRandomValues for license
  codes, HTML-escape license emails, add webhook idempotency, constant-time admin auth
  ([c0d8cc3](https://github.com/vdavid/cmdr/commit/c0d8cc3), [70bc594](https://github.com/vdavid/cmdr/commit/70bc594),
  [51cd0b5](https://github.com/vdavid/cmdr/commit/51cd0b5), [bea3b2a](https://github.com/vdavid/cmdr/commit/bea3b2a),
  [9db450b](https://github.com/vdavid/cmdr/commit/9db450b), [b82f857](https://github.com/vdavid/cmdr/commit/b82f857))
- Docs overhaul: add colocated CLAUDE.md files throughout repo, architecture.md, branding guide
  ([eac9e61](https://github.com/vdavid/cmdr/commit/eac9e61), [dd91c78](https://github.com/vdavid/cmdr/commit/dd91c78))
- Website: add changelog, roadmap, newsletter signup with Listmonk + AWS SES, mobile responsiveness fixes, 512px logo
  ([643de6a](https://github.com/vdavid/cmdr/commit/643de6a), [07936d1](https://github.com/vdavid/cmdr/commit/07936d1),
  [ba4812d](https://github.com/vdavid/cmdr/commit/ba4812d), [aa661cf](https://github.com/vdavid/cmdr/commit/aa661cf))
- Add dead code check, manual CI trigger, pnpm security audit, LoC counter, summary job for branch protection
  ([9876600](https://github.com/vdavid/cmdr/commit/9876600), [3b20e66](https://github.com/vdavid/cmdr/commit/3b20e66),
  [ad22eba](https://github.com/vdavid/cmdr/commit/ad22eba))
- Tooling: extract shared Go check helpers, add VNC mode for Linux testing, fix Linux E2E environment
  ([550c353](https://github.com/vdavid/cmdr/commit/550c353), [6aa5ff7](https://github.com/vdavid/cmdr/commit/6aa5ff7),
  [fa907b6](https://github.com/vdavid/cmdr/commit/fa907b6))
- License server: add input validation, webhook idempotency, and security hardening
  ([4363a32](https://github.com/vdavid/cmdr/commit/4363a32), [9db450b](https://github.com/vdavid/cmdr/commit/9db450b),
  [7398965](https://github.com/vdavid/cmdr/commit/7398965))

## [0.4.0] - 2026-01-27

### Added

- Add file selection: Space toggles, Shift+arrows for range, Cmd+A for select all, selection info in status bar
  ([4d44cda](https://github.com/vdavid/cmdr/commit/4d44cda), [1cac4b3](https://github.com/vdavid/cmdr/commit/1cac4b3))
- Add copy feature with F5: copy dialog, destination picker with free space display, conflict handling
  ([281f45e](https://github.com/vdavid/cmdr/commit/281f45e), [fb5f027](https://github.com/vdavid/cmdr/commit/fb5f027),
  [a6d148d](https://github.com/vdavid/cmdr/commit/a6d148d), [6c661f2](https://github.com/vdavid/cmdr/commit/6c661f2))
- Add new folder feature with F7 shortcut and conflict handling
  ([80ec297](https://github.com/vdavid/cmdr/commit/80ec297))
- Add "Open in editor" feature with F4 shortcut ([7eb66ac](https://github.com/vdavid/cmdr/commit/7eb66ac))
- Add function key bar at bottom of UI for mouse-initiated actions
  ([537e040](https://github.com/vdavid/cmdr/commit/537e040))
- Add pane resizing: drag to resize between 25–75%, double-click to reset to 50%
  ([542b491](https://github.com/vdavid/cmdr/commit/542b491))
- Add multifile external drag and drop ([7426334](https://github.com/vdavid/cmdr/commit/7426334))
- Add keyboard navigation to network panes: PgUp/PgDn, Home/End, arrow keys
  ([70aa341](https://github.com/vdavid/cmdr/commit/70aa341))
- Add "Opening folder..." loading phase for network folders with distinct status messages
  ([9eb1185](https://github.com/vdavid/cmdr/commit/9eb1185))
- Add license key entry dialog with organization address and tax ID collection
  ([52480ce](https://github.com/vdavid/cmdr/commit/52480ce), [29eb6fe](https://github.com/vdavid/cmdr/commit/29eb6fe))

### Fixed

- Fix UI not updating on external file renames ([5de9346](https://github.com/vdavid/cmdr/commit/5de9346))
- Fix light mode colors ([42888c7](https://github.com/vdavid/cmdr/commit/42888c7))
- Fix cursor going out of Full view bounds ([7edcac8](https://github.com/vdavid/cmdr/commit/7edcac8))
- Fix ESC during loading navigating to wrong location ([b8c12e7](https://github.com/vdavid/cmdr/commit/b8c12e7))
- Fix focus after dragging window ([8488de6](https://github.com/vdavid/cmdr/commit/8488de6))
- Fix multiple volume selectors opening at once ([f4c4c21](https://github.com/vdavid/cmdr/commit/f4c4c21))
- Fix frontend race condition from refactor ([646c7af](https://github.com/vdavid/cmdr/commit/646c7af))

### Non-app

- Add E2E tests with tauri-driver on Linux using WebDriverIO in Docker
  ([1b0cbac](https://github.com/vdavid/cmdr/commit/1b0cbac))
- Revamp checker script: parallel execution, dependency graph, aligned output, colored durations
  ([7835b4c](https://github.com/vdavid/cmdr/commit/7835b4c))
- Add type drift detection between Rust and Svelte types ([b3ae1c3](https://github.com/vdavid/cmdr/commit/b3ae1c3))
- Add jscpd for Rust code duplication detection, CSS health checks, Go checks
  ([67e6c15](https://github.com/vdavid/cmdr/commit/67e6c15), [d177eb3](https://github.com/vdavid/cmdr/commit/d177eb3),
  [2540752](https://github.com/vdavid/cmdr/commit/2540752))
- Add Claude hooks for pre-session context and post-edit autoformat
  ([3d59dde](https://github.com/vdavid/cmdr/commit/3d59dde), [122182d](https://github.com/vdavid/cmdr/commit/122182d))
- Add LogTape logging for Svelte and debug pane for dev mode ([affa548](https://github.com/vdavid/cmdr/commit/affa548),
  [f494e15](https://github.com/vdavid/cmdr/commit/f494e15))
- Require reasoning in clippy lint exceptions ([d327cf4](https://github.com/vdavid/cmdr/commit/d327cf4))
- Website: fix hero image animation and sizing, fix broken Paddle references
  ([40faeee](https://github.com/vdavid/cmdr/commit/40faeee), [278ad4c](https://github.com/vdavid/cmdr/commit/278ad4c),
  [5eb5a52](https://github.com/vdavid/cmdr/commit/5eb5a52))
- License server: wire up Paddle checkout, fix webhook email fetching, support quantity > 1
  ([3c40929](https://github.com/vdavid/cmdr/commit/3c40929))

## [0.3.2] - 2026-01-14

### Fixed

- Fix auto-updater to download updates and restart the app after updating
  ([c0bff9a](https://github.com/vdavid/cmdr/commit/c0bff9a))

### Non-app

- Website: redesign with mustard yellow theme, view transitions, hero animation, and reduced motion support
  ([0296379](https://github.com/vdavid/cmdr/commit/0296379), [18b729f](https://github.com/vdavid/cmdr/commit/18b729f),
  [689a151](https://github.com/vdavid/cmdr/commit/689a151))
- Website: avoid aggressive caching, rearrange T&C ([8ca0539](https://github.com/vdavid/cmdr/commit/8ca0539),
  [c92dff8](https://github.com/vdavid/cmdr/commit/c92dff8))
- Tooling: turn off MCP stdio sidecar, fix Rust-Linux check, reduce CI frequency, fix latest.json formatting
  ([5dda608](https://github.com/vdavid/cmdr/commit/5dda608), [2ec3f7e](https://github.com/vdavid/cmdr/commit/2ec3f7e),
  [42d81ab](https://github.com/vdavid/cmdr/commit/42d81ab), [52980ae](https://github.com/vdavid/cmdr/commit/52980ae))
- Docs: release process and auto-updater documentation ([c7c36f6](https://github.com/vdavid/cmdr/commit/c7c36f6),
  [765f5ad](https://github.com/vdavid/cmdr/commit/765f5ad), [f3785da](https://github.com/vdavid/cmdr/commit/f3785da),
  [10e43de](https://github.com/vdavid/cmdr/commit/10e43de))

## [0.3.1] - 2026-01-14

### Added

- Add custom title bar, 4 px narrower for more content space ([33e90c8](https://github.com/vdavid/cmdr/commit/33e90c8))

### Changed

- Replace rusty icon with yellow one ([79777e3](https://github.com/vdavid/cmdr/commit/79777e3))

### Fixed

- Fix app name in task switcher: shows "Cmdr" instead of "cmdr"
  ([8117300](https://github.com/vdavid/cmdr/commit/8117300))

## [0.3.0] - 2026-01-13

### Added

- Add MCP server with file exploring tools ([f6dcf27](https://github.com/vdavid/cmdr/commit/f6dcf27))
- Add stdio MCP interface for broader client compatibility ([3b193f7](https://github.com/vdavid/cmdr/commit/3b193f7))
- Add Streamable HTTP support to MCP server ([1d0549b](https://github.com/vdavid/cmdr/commit/1d0549b))
- Stream folder contents for blazing fast experience ([1d82ec9](https://github.com/vdavid/cmdr/commit/1d82ec9))
- Add "listing complete" state showing file count ([5059e00](https://github.com/vdavid/cmdr/commit/5059e00))
- Add Linux checks to checker script ([02ab0ab](https://github.com/vdavid/cmdr/commit/02ab0ab))

### Fixed

- Fix MCP server port and tool naming ([c2ae7de](https://github.com/vdavid/cmdr/commit/c2ae7de))
- Fix race condition when loading files ([38865e6](https://github.com/vdavid/cmdr/commit/38865e6))

## [0.2.0] - 2026-01-10

Initial public release. Free forever for personal use (BSL license).

### Added

- Dual-pane file explorer with keyboard and mouse navigation ([c945f18](https://github.com/vdavid/cmdr/commit/c945f18))
- Full mode (vertical scroll with size/date columns) and Brief mode (horizontal multi-column), switchable via ⌘1/⌘2
  ([c779a6d](https://github.com/vdavid/cmdr/commit/c779a6d))
- Virtual scrolling for 100k+ files ([cf6c35d](https://github.com/vdavid/cmdr/commit/cf6c35d))
- Chunked directory loading (50k files: 350 ms to first files)
  ([869cdfb](https://github.com/vdavid/cmdr/commit/869cdfb))
- File icons from OS with caching ([b8c588e](https://github.com/vdavid/cmdr/commit/b8c588e))
- File metadata panel with size color coding and date tooltips
  ([bc3dc85](https://github.com/vdavid/cmdr/commit/bc3dc85))
- Native context menu (Open, Show in Finder, Copy path, Quick Look)
  ([7d977a1](https://github.com/vdavid/cmdr/commit/7d977a1))
- Live file watching with incremental diffs ([cf12372](https://github.com/vdavid/cmdr/commit/cf12372))
- Dropbox and iCloud sync status icons ([46f1770](https://github.com/vdavid/cmdr/commit/46f1770))
- Volume switching with keyboard navigation ([ba3e770](https://github.com/vdavid/cmdr/commit/ba3e770))
- Network drives (SMB): host discovery via Bonjour, share listing, authentication, and mounting
  ([54ee04f](https://github.com/vdavid/cmdr/commit/54ee04f))
- Sorting by name, size, date, extension with alphanumeric sort
  ([e7b7206](https://github.com/vdavid/cmdr/commit/e7b7206))
- Back/Forward navigation ([56a5bf6](https://github.com/vdavid/cmdr/commit/56a5bf6))
- Drag and drop from the app ([8e1d53b](https://github.com/vdavid/cmdr/commit/8e1d53b))
- Command palette with fuzzy search ([7b0ea13](https://github.com/vdavid/cmdr/commit/7b0ea13))
- Window state persistence (position and size remembered) ([b8d93c5](https://github.com/vdavid/cmdr/commit/b8d93c5))
- Dark mode support ([7deb986](https://github.com/vdavid/cmdr/commit/7deb986))
- Show hidden files menu item ([4af855d](https://github.com/vdavid/cmdr/commit/4af855d))
- Full disk access permission handling ([9f433d8](https://github.com/vdavid/cmdr/commit/9f433d8))
- Licensing features (validation, about screen, expiry modal) ([dc68eeb](https://github.com/vdavid/cmdr/commit/dc68eeb))
- Keyboard shortcuts: Backspace/⌘↑ (go up), ⌥↑/↓ (home/end), Fn arrows (page up/down)
  ([fc899d4](https://github.com/vdavid/cmdr/commit/fc899d4))
- getcmdr.com website ([0f9eb21](https://github.com/vdavid/cmdr/commit/0f9eb21))
- License server (Cloudflare Worker) with Ed25519-signed keys ([bff3e8a](https://github.com/vdavid/cmdr/commit/bff3e8a))

---

### Development history

<details>
<summary>Click to expand full development history</summary>

#### 2026-01-10 - Initial public release

- Add licensing features to app (validation, about screen, expiry modal)
  ([dc68eeb](https://github.com/vdavid/cmdr/commit/dc68eeb))
- Add command palette with fuzzy search ([7b0ea13](https://github.com/vdavid/cmdr/commit/7b0ea13))
- Switch to BSL license (free for individuals) ([06c49cb](https://github.com/vdavid/cmdr/commit/06c49cb))

#### 2026-01-09 - License server improvements

- Add checkout tester tool for license server ([38774fe](https://github.com/vdavid/cmdr/commit/38774fe))
- Add sandbox/live environment duality for license tests ([15b3957](https://github.com/vdavid/cmdr/commit/15b3957))
- Unify trial period to 14 days ([7e68c27](https://github.com/vdavid/cmdr/commit/7e68c27))

#### 2026-01-08 - Cmdr, website, licensing

- Rename to Cmdr ([016a3e3](https://github.com/vdavid/cmdr/commit/016a3e3))
- Restructure as monorepo with desktop app in apps/desktop ([c0e764a](https://github.com/vdavid/cmdr/commit/c0e764a))
- Add getcmdr.com website ([0f9eb21](https://github.com/vdavid/cmdr/commit/0f9eb21))
- Add license server (Cloudflare Worker) with Ed25519-signed keys
  ([bff3e8a](https://github.com/vdavid/cmdr/commit/bff3e8a))
- Add legal pages (privacy policy, terms, refund policy, pricing)
  ([4f32a29](https://github.com/vdavid/cmdr/commit/4f32a29))
- Streamline CI (website-only PRs: 22 min → 2 min) ([4894003](https://github.com/vdavid/cmdr/commit/4894003))

#### 2026-01-07 - Network fixes

- Fix network share unnecessary login prompts ([dbeebaf](https://github.com/vdavid/cmdr/commit/dbeebaf))
- Fix Back/Forward navigation across network screens ([bf462e9](https://github.com/vdavid/cmdr/commit/bf462e9))
- Sort network hosts and shares alphabetically ([9de5f2b](https://github.com/vdavid/cmdr/commit/9de5f2b))

#### 2026-01-05-06 - Network drives (SMB)

- Add network host discovery via Bonjour ([54ee04f](https://github.com/vdavid/cmdr/commit/54ee04f))
- Add SMB share listing ([693e926](https://github.com/vdavid/cmdr/commit/693e926))
- Add network share authentication ([283e5fd](https://github.com/vdavid/cmdr/commit/283e5fd))
- Add network share mounting ([308d55c](https://github.com/vdavid/cmdr/commit/308d55c))
- Add volume mount/unmount watching ([76bbf22](https://github.com/vdavid/cmdr/commit/76bbf22))

#### 2026-01-04 - Sorting

- Add sorting feature (name, size, date, extension) with alphanumeric sort
  ([e7b7206](https://github.com/vdavid/cmdr/commit/e7b7206))
- Add Stylelint for CSS quality ([a778dcc](https://github.com/vdavid/cmdr/commit/a778dcc))

#### 2026-01-02-03 - Navigation and permissions

- Add ⌘↑ shortcut to go up a folder ([848e2f1](https://github.com/vdavid/cmdr/commit/848e2f1))
- Add full disk access permission handling ([9f433d8](https://github.com/vdavid/cmdr/commit/9f433d8))
- Add Back/Forward navigation with menu items ([56a5bf6](https://github.com/vdavid/cmdr/commit/56a5bf6))
- Add keyboard navigation to volume selector ([46c3023](https://github.com/vdavid/cmdr/commit/46c3023))
- Save last directory per volume ([9886fcd](https://github.com/vdavid/cmdr/commit/9886fcd))
- Set minimum window size ([237c5a9](https://github.com/vdavid/cmdr/commit/237c5a9))
- Fix opening files ([714dc5a](https://github.com/vdavid/cmdr/commit/714dc5a))

#### 2026-01-01 - Drag and drop, volumes

- Add drag and drop FROM the app ([8e1d53b](https://github.com/vdavid/cmdr/commit/8e1d53b))
- Add volume switching feature ([ba3e770](https://github.com/vdavid/cmdr/commit/ba3e770))
- Remove Tailwind (was slowing down app startup) ([5354a48](https://github.com/vdavid/cmdr/commit/5354a48))

#### 2025-12-31 - Polish

- Add font width measuring for precise Brief mode layout ([848f68f](https://github.com/vdavid/cmdr/commit/848f68f))
- Abstract file system access for better testing ([eb9dd72](https://github.com/vdavid/cmdr/commit/eb9dd72))
- Fix Dropbox sync icon false positives ([64007f0](https://github.com/vdavid/cmdr/commit/64007f0))
- Fix file watching reliability ([aefe3e7](https://github.com/vdavid/cmdr/commit/aefe3e7))

#### 2025-12-30 - Speed optimizations

- Add keyboard shortcuts: ⌥↑/↓ for home/end, Fn arrows for page up/down
  ([6298990](https://github.com/vdavid/cmdr/commit/6298990))
- Move file cache to backend for major speed improvements ([a42eda5](https://github.com/vdavid/cmdr/commit/a42eda5))
- Optimize directory loading (phase 1 and 2) ([7efd61a](https://github.com/vdavid/cmdr/commit/7efd61a))

#### 2025-12-29 - View modes and cloud sync

- Add Full mode (vertical scroll with size/date columns) and Brief mode (horizontal multi-column)
  ([c779a6d](https://github.com/vdavid/cmdr/commit/c779a6d))
- Add Dropbox and iCloud sync status icons ([46f1770](https://github.com/vdavid/cmdr/commit/46f1770))
- Add loading screen animation ([234f0a7](https://github.com/vdavid/cmdr/commit/234f0a7))

#### 2025-12-28 - Performance and file operations

- Add chunked directory loading (50k files: 350 ms to first files)
  ([869cdfb](https://github.com/vdavid/cmdr/commit/869cdfb))
- Add file metadata panel with size color coding and date tooltips
  ([bc3dc85](https://github.com/vdavid/cmdr/commit/bc3dc85))
- Add native context menu (Open, Show in Finder, Copy path, Quick Look)
  ([7d977a1](https://github.com/vdavid/cmdr/commit/7d977a1))
- Add live file watching with incremental diffs ([cf12372](https://github.com/vdavid/cmdr/commit/cf12372))
- Add virtual scrolling for 100k+ files ([cf6c35d](https://github.com/vdavid/cmdr/commit/cf6c35d))
- Add Backspace shortcut to go up a folder ([fc899d4](https://github.com/vdavid/cmdr/commit/fc899d4))
- Scroll to last folder when navigating up ([8ccd8bd](https://github.com/vdavid/cmdr/commit/8ccd8bd))

#### 2025-12-27 - File metadata and icons

- Add file metadata display (owner, size, dates) ([d9994bc](https://github.com/vdavid/cmdr/commit/d9994bc))
- Add file icons from OS with caching ([b8c588e](https://github.com/vdavid/cmdr/commit/b8c588e))
- Add per-folder custom icons support ([210f23b](https://github.com/vdavid/cmdr/commit/210f23b))
- Add Tauri MCP server for AI tooling integration ([0a64eb3](https://github.com/vdavid/cmdr/commit/0a64eb3))
- Fix symlinked directory handling ([5a134ac](https://github.com/vdavid/cmdr/commit/5a134ac))

#### 2025-12-26 - Dual-pane explorer

- Add dual-pane file explorer with home directory listing ([c945f18](https://github.com/vdavid/cmdr/commit/c945f18))
- Add window state persistence (position and size remembered) ([b8d93c5](https://github.com/vdavid/cmdr/commit/b8d93c5))
- Add file navigation with keyboard and mouse ([20424e0](https://github.com/vdavid/cmdr/commit/20424e0))
- Add "Show hidden files" menu item ([4af855d](https://github.com/vdavid/cmdr/commit/4af855d))
- Add dark mode support ([7deb986](https://github.com/vdavid/cmdr/commit/7deb986))

#### 2025-12-25 - Project init

- Initialize Rust + Tauri 2 + Svelte 5 project ([b410bd9](https://github.com/vdavid/cmdr/commit/b410bd9))
- Add GitHub Actions workflow ([6dbf265](https://github.com/vdavid/cmdr/commit/6dbf265))

</details>
