# Changelog

All notable changes to Cmdr will be documented in this file.

The format is based on [keep a changelog](https://keepachangelog.com/en/1.1.0/),
and we use [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.7.0] - 2026-03-12

### Added

- Add AI settings with three providers (off / cloud API / local LLM), 15 cloud presets with per-provider key storage, connection check, model combobox, RAM gauge, and context size control ([b41365b](https://github.com/vdavid/cmdr/commit/b41365b), [abfc248](https://github.com/vdavid/cmdr/commit/abfc248), [423e669](https://github.com/vdavid/cmdr/commit/423e669))
- Add live MCP server start/stop in Settings — no app restart needed ([e0c55e7](https://github.com/vdavid/cmdr/commit/e0c55e7))
- Add stale index detection with user notification toast and automatic rescan ([b590a54](https://github.com/vdavid/cmdr/commit/b590a54))
- Add device tracking for license abuse detection with fair use terms in ToS ([cf4f913](https://github.com/vdavid/cmdr/commit/cf4f913))
- Add license section to Settings with status display, action buttons, and dynamic labels across the app ([39cf7b4](https://github.com/vdavid/cmdr/commit/39cf7b4))
- Improve app icon for macOS Sequoia ([cc80d28](https://github.com/vdavid/cmdr/commit/cc80d28))

### Changed

- Remove supporter license tier — legacy keys gracefully map to Personal ([c0a63f5](https://github.com/vdavid/cmdr/commit/c0a63f5))
- Split Settings UI horizontally 50-50% ([9493f88](https://github.com/vdavid/cmdr/commit/9493f88))
- Rename settings file from `settings-v2.json` to `settings.json` ([d987cc8](https://github.com/vdavid/cmdr/commit/d987cc8))

### Fixed

- Fix startup panic from `blocking_lock` in async context ([f9855ca](https://github.com/vdavid/cmdr/commit/f9855ca))
- Fix SQLite write pragmas running on read-only connections, causing panic in subtree scans ([a53a275](https://github.com/vdavid/cmdr/commit/a53a275))
- Fix llama-server not stopping on app quit, keeping stale PIDs alive, and using excessive memory (256k → 4k default context) ([eae70f1](https://github.com/vdavid/cmdr/commit/eae70f1), [ffcbc81](https://github.com/vdavid/cmdr/commit/ffcbc81), [e45c742](https://github.com/vdavid/cmdr/commit/e45c742))
- Fix Settings UI freezing for ~5s when stopping AI server — instant SIGKILL for stateless llama-server ([2af7ee8](https://github.com/vdavid/cmdr/commit/2af7ee8))
- Fix dev and prod app data clashing on same machine — dev now uses separate data directory and MCP port ([b8b058a](https://github.com/vdavid/cmdr/commit/b8b058a))
- Fix fallback path resolution falling to `/` instead of `~` ([8d7c644](https://github.com/vdavid/cmdr/commit/8d7c644))
- Fix indexing: 100x faster aggregation via in-memory accumulation, DB auto vacuum, truncate before full scan, live index size in Settings ([47a2e8e](https://github.com/vdavid/cmdr/commit/47a2e8e), [cad1af5](https://github.com/vdavid/cmdr/commit/cad1af5), [aff2046](https://github.com/vdavid/cmdr/commit/aff2046), [96323e9](https://github.com/vdavid/cmdr/commit/96323e9))
- Fix FSEvents storms causing high memory pressure — mimalloc allocator, 1s dedup window, reduced SQLite cache and channel buffers ([207ddee](https://github.com/vdavid/cmdr/commit/207ddee))

### Non-app

- Docs: replace 19 ADRs with colocated Decision/Why entries in 11 CLAUDE.md files, slim down AGENTS.md from 245 to 93 lines, add `@wrap-up` and `@plan` commands ([ccf5cc7](https://github.com/vdavid/cmdr/commit/ccf5cc7), [d297a1a](https://github.com/vdavid/cmdr/commit/d297a1a), [0595796](https://github.com/vdavid/cmdr/commit/0595796))
- Website: show version + file size on all download buttons, fix Intel/Apple detection flicker, fix a11y warning, fix Umami script collision ([bd17056](https://github.com/vdavid/cmdr/commit/bd17056), [ec35b1f](https://github.com/vdavid/cmdr/commit/ec35b1f), [55c950e](https://github.com/vdavid/cmdr/commit/55c950e), [0ad03f4](https://github.com/vdavid/cmdr/commit/0ad03f4))
- Tooling: add html-validate and circular dep checks, pass kill signals in checker script, remove pnpm audit check ([3dbd5af](https://github.com/vdavid/cmdr/commit/3dbd5af), [4bead2b](https://github.com/vdavid/cmdr/commit/4bead2b), [ce3eae1](https://github.com/vdavid/cmdr/commit/ce3eae1), [2c588bf](https://github.com/vdavid/cmdr/commit/2c588bf))
- Refactors: extract volume grouping, menu platform code, viewer scroll/search; eliminate all circular deps ([7740fbc](https://github.com/vdavid/cmdr/commit/7740fbc), [8522e71](https://github.com/vdavid/cmdr/commit/8522e71), [e16bd91](https://github.com/vdavid/cmdr/commit/e16bd91), [7ed1cea](https://github.com/vdavid/cmdr/commit/7ed1cea))
- Add missing tests across multiple modules ([b53ce59](https://github.com/vdavid/cmdr/commit/b53ce59))

## [0.6.1] - 2026-03-10

### Added

- Add top menu icons ([1a2621a](https://github.com/vdavid/cmdr/commit/1a2621a))
- Add View, Copy, Move, New folder, and Delete actions to context menu ([a966f17](https://github.com/vdavid/cmdr/commit/a966f17))

### Fixed

- Fix OOM crash from unbounded indexing buffers — toggling Full Disk Access could replay millions of FSEvents with zero backpressure, consuming 500+ GB RAM. All buffers are now bounded (~350 MB peak), with a memory watchdog that stops indexing at 16 GB ([f1501ec](https://github.com/vdavid/cmdr/commit/f1501ec))

### Non-app

- Website: add llms.txt, Schema.org JSON-LD, and auto-generated sitemap for agent accessibility ([ba64c36](https://github.com/vdavid/cmdr/commit/ba64c36))
- Website: update roadmap ([5197120](https://github.com/vdavid/cmdr/commit/5197120))
- CI: simplify release pipeline, download sigs directly from release, generate `latest.json` with `jq`, validate all 3 sigs before proceeding ([d3095cb](https://github.com/vdavid/cmdr/commit/d3095cb), [5b82cd0](https://github.com/vdavid/cmdr/commit/5b82cd0))
- CI: fix Backspace E2E test on WebKitGTK, fix CI failures, fix 3 flaky tests ([7c22951](https://github.com/vdavid/cmdr/commit/7c22951), [79f593c](https://github.com/vdavid/cmdr/commit/79f593c), [8f4ea82](https://github.com/vdavid/cmdr/commit/8f4ea82))
- Docs: add troubleshooting section to releasing guide ([1768b29](https://github.com/vdavid/cmdr/commit/1768b29))

## [0.6.0] - 2026-03-08

### Added

- Add Linux support (alpha): volumes via /proc/mounts, file ops with reflink support, trash via FreeDesktop spec, inotify file watching, MTP ungated, SMB via mDNS + smbclient fallback, GVFS-mounted shares as volumes, native file icons via freedesktop-icons, accent color via XDG Desktop Portal, encrypted credential fallback when no system keyring, distro-specific install hints, USB permission handling ([b6e80f6](https://github.com/vdavid/cmdr/commit/b6e80f6), [20be0c3](https://github.com/vdavid/cmdr/commit/20be0c3), [9c51fa9](https://github.com/vdavid/cmdr/commit/9c51fa9), [64e41f9](https://github.com/vdavid/cmdr/commit/64e41f9), [40cc1a9](https://github.com/vdavid/cmdr/commit/40cc1a9), [c3ad1ed](https://github.com/vdavid/cmdr/commit/c3ad1ed), [d40ea25](https://github.com/vdavid/cmdr/commit/d40ea25), [60063ec](https://github.com/vdavid/cmdr/commit/60063ec), [e65d993](https://github.com/vdavid/cmdr/commit/e65d993), [22e2ea7](https://github.com/vdavid/cmdr/commit/22e2ea7), [afe2609](https://github.com/vdavid/cmdr/commit/afe2609), [4bbcbb0](https://github.com/vdavid/cmdr/commit/4bbcbb0), [48af543](https://github.com/vdavid/cmdr/commit/48af543))
- Add per-pane tab support: ⌘T/⌘W, ⌃Tab cycling, pin/unpin, context menu, persistence with migration, per-tab sort ([791a29a](https://github.com/vdavid/cmdr/commit/791a29a))
- Add delete/trash feature (F8): trash by default, ⇧F8 for permanent delete, confirmation dialog with scan preview, batch progress with cancellation, volume trash support detection ([e3560a3](https://github.com/vdavid/cmdr/commit/e3560a3))
- Add clipboard for files: ⌘C/⌘V/⌘X with Finder interop, ⌥⌘V for "Move here", cut state tracking, text clipboard in all windows via NSPasteboard ([0dc2953](https://github.com/vdavid/cmdr/commit/0dc2953), [60baeba](https://github.com/vdavid/cmdr/commit/60baeba))
- Add toast notification system with centralized store, dedup, stacking, three levels, transient/persistent modes ([6c5c452](https://github.com/vdavid/cmdr/commit/6c5c452), [2329f2f](https://github.com/vdavid/cmdr/commit/2329f2f))
- Add per-pane disk space display: 2px usage bar, free-space text in status bar, mini bars in volume dropdown ([9b6d057](https://github.com/vdavid/cmdr/commit/9b6d057))
- Add custom tooltips with glass material effect, shortcut badges, smart positioning, accessibility support, replacing all native tooltips ([3c7f965](https://github.com/vdavid/cmdr/commit/3c7f965))
- Add drive indexing with integer-keyed DB schema (7.4x size reduction, 3.8 GB → 0.54 GB), LRU path cache, platform-aware collation, recursive CTE aggregation ([7c5d3ce](https://github.com/vdavid/cmdr/commit/7c5d3ce), [daee97b](https://github.com/vdavid/cmdr/commit/daee97b), [5e10fa9](https://github.com/vdavid/cmdr/commit/5e10fa9), [68be3ab](https://github.com/vdavid/cmdr/commit/68be3ab))
- Add IPC hardening: timeout-protect all filesystem commands, transparent timeout UI with retry/fallback for volumes, tabs, file ops, and viewer ([6a58278](https://github.com/vdavid/cmdr/commit/6a58278), [71de96e](https://github.com/vdavid/cmdr/commit/71de96e))
- Add accent color option in Settings: macOS theme or Cmdr gold, "Recolor to gold" for folder icons ([330e824](https://github.com/vdavid/cmdr/commit/330e824), [ef9de79](https://github.com/vdavid/cmdr/commit/ef9de79))
- Add directory sorting by size with toggle in Settings ([a7dd8ca](https://github.com/vdavid/cmdr/commit/a7dd8ca))
- Add "Forget saved password" UI for SMB network shares ([7d751d5](https://github.com/vdavid/cmdr/commit/7d751d5))
- Add path validation in copy/move and mkdir dialogs with platform-correct limits ([6b295ec](https://github.com/vdavid/cmdr/commit/6b295ec))
- Add centralized keyboard shortcut dispatch with runtime custom bindings ([e40bcc2](https://github.com/vdavid/cmdr/commit/e40bcc2))
- Add macOS entitlements and TCC usage descriptions for proper permission prompts ([ff0c27e](https://github.com/vdavid/cmdr/commit/ff0c27e))
- Add Apple code signing, notarization, and arch-specific downloads (aarch64, x86_64, universal) ([b03f91e](https://github.com/vdavid/cmdr/commit/b03f91e), [944085f](https://github.com/vdavid/cmdr/commit/944085f))
- Add licensing UI improvements: verify/commit split, typed errors, short code in signed payload, Paddle live setup ([0abc704](https://github.com/vdavid/cmdr/commit/0abc704), [1f2308b](https://github.com/vdavid/cmdr/commit/1f2308b))

### Fixed

- Fix file viewer: search progress bar with spinner and stop button, incremental match delivery, 10k match cap, byte-seek navigation, loading very long files ([9c0a3c3](https://github.com/vdavid/cmdr/commit/9c0a3c3), [a3b9d0e](https://github.com/vdavid/cmdr/commit/a3b9d0e), [31cf5fd](https://github.com/vdavid/cmdr/commit/31cf5fd), [d15ecde](https://github.com/vdavid/cmdr/commit/d15ecde), [86ef2a5](https://github.com/vdavid/cmdr/commit/86ef2a5), [0fcdb13](https://github.com/vdavid/cmdr/commit/0fcdb13), [8b57bbe](https://github.com/vdavid/cmdr/commit/8b57bbe))
- Fix 3–10s startup block from index enrichment holding the mutex ([267e02b](https://github.com/vdavid/cmdr/commit/267e02b))
- Fix mDNS host resolution arriving before discovery, causing SMB auth failures ([2dda99b](https://github.com/vdavid/cmdr/commit/2dda99b))
- Fix focus escaping panes with focus guard, removing ~50 redundant refocus calls ([4c9aadc](https://github.com/vdavid/cmdr/commit/4c9aadc))
- Fix clipboard shortcuts in text fields on macOS ([20f3de0](https://github.com/vdavid/cmdr/commit/20f3de0))
- Fix non-blocking navigation on slow/dead SMB shares with timeouts and optimistic UI ([c85c8c2](https://github.com/vdavid/cmdr/commit/c85c8c2))
- Fix copy feature: auto-rollback on panic, deadlock prevention, cancel race condition ([2b17ab5](https://github.com/vdavid/cmdr/commit/2b17ab5))
- Fix status bar not refreshing after file watcher diffs ([e880f9f](https://github.com/vdavid/cmdr/commit/e880f9f))
- Fix pinned tab volume change now opens new tab instead of navigating in-place ([ff4c8f2](https://github.com/vdavid/cmdr/commit/ff4c8f2))
- Fix cancel-loading to return to previous folder instead of home ([8ff2379](https://github.com/vdavid/cmdr/commit/8ff2379))
- Fix ⌘, to refocus Settings window if already open ([71b3e61](https://github.com/vdavid/cmdr/commit/71b3e61))
- Fix Settings: ⌥+key shortcuts showing "Dead" on macOS, key filter subset matching, ESC clears filter ([1fd540a](https://github.com/vdavid/cmdr/commit/1fd540a), [5056bb6](https://github.com/vdavid/cmdr/commit/5056bb6), [47050e0](https://github.com/vdavid/cmdr/commit/47050e0))
- Fix settings not initialized warning at startup ([b540fcc](https://github.com/vdavid/cmdr/commit/b540fcc))
- Fix SMB share showing 0 bytes free on network filesystems ([f791153](https://github.com/vdavid/cmdr/commit/f791153))
- Fix volumes cached to prevent timeout at startup ([024e48f](https://github.com/vdavid/cmdr/commit/024e48f))
- Fix top menu items staying enabled on non-main windows ([7572d13](https://github.com/vdavid/cmdr/commit/7572d13))
- Fix live file count during large folder loading ([7815d0f](https://github.com/vdavid/cmdr/commit/7815d0f))
- Fix window content height for production builds ([0cbd0fd](https://github.com/vdavid/cmdr/commit/0cbd0fd))
- Fix folder icons updating on OS theme change ([6b02445](https://github.com/vdavid/cmdr/commit/6b02445))
- Fix focus lost after rename cancellation ([edace18](https://github.com/vdavid/cmdr/commit/edace18))
- Fix file viewer not loading settings ([acfef93](https://github.com/vdavid/cmdr/commit/acfef93))
- Fix drive indexing: orphaned entries, missing dir sizes, background scan failures, DB transaction issues ([323ae86](https://github.com/vdavid/cmdr/commit/323ae86), [004f302](https://github.com/vdavid/cmdr/commit/004f302), [c331143](https://github.com/vdavid/cmdr/commit/c331143))
- Fix MCP protocol version mismatch warnings at startup ([2af0b90](https://github.com/vdavid/cmdr/commit/2af0b90))
- Fix arrow up/down performance in large folders ([e6f268c](https://github.com/vdavid/cmdr/commit/e6f268c))
- Fix PostHog CSP and make it cookieless ([1700d99](https://github.com/vdavid/cmdr/commit/1700d99), [9cea85a](https://github.com/vdavid/cmdr/commit/9cea85a))
- Fix app loading slowly due to startup optimizations: license cache, async validation ([3835866](https://github.com/vdavid/cmdr/commit/3835866), [87de136](https://github.com/vdavid/cmdr/commit/87de136))

### Non-app

- Overhaul native menus on macOS and Linux: build from scratch, strip macOS system-injected items, unify dispatch via single event, context-aware graying, full accelerator sync ([b38c552](https://github.com/vdavid/cmdr/commit/b38c552))
- Unify frontend + backend logging via tauri-plugin-log, demote noisy log levels, suppress smb/sspi noise ([22f4ab5](https://github.com/vdavid/cmdr/commit/22f4ab5), [dbbcc55](https://github.com/vdavid/cmdr/commit/dbbcc55), [1e59a56](https://github.com/vdavid/cmdr/commit/1e59a56))
- Design system: unified button styles, consistent loading states, improved text readability, redesigned network screens ([8dc2e33](https://github.com/vdavid/cmdr/commit/8dc2e33), [4d07ad0](https://github.com/vdavid/cmdr/commit/4d07ad0), [71dbe0b](https://github.com/vdavid/cmdr/commit/71dbe0b), [b5d8b28](https://github.com/vdavid/cmdr/commit/b5d8b28), [a018a3e](https://github.com/vdavid/cmdr/commit/a018a3e), [90e2010](https://github.com/vdavid/cmdr/commit/90e2010))
- Docs overhaul: CLAUDE.md staleness checker in CI, enriched 25 CLAUDE.md files with Decision/Why entries, cross-cutting patterns in architecture.md, split infrastructure.md into per-service files ([ff8b3be](https://github.com/vdavid/cmdr/commit/ff8b3be), [347ae9b](https://github.com/vdavid/cmdr/commit/347ae9b), [f961f19](https://github.com/vdavid/cmdr/commit/f961f19), [2f7bff1](https://github.com/vdavid/cmdr/commit/2f7bff1))
- Website: add blog with first post, PostHog and Umami analytics, arch-specific download buttons, Docker build check, newsletter improvements ([01681c1](https://github.com/vdavid/cmdr/commit/01681c1), [75d5228](https://github.com/vdavid/cmdr/commit/75d5228), [78de573](https://github.com/vdavid/cmdr/commit/78de573), [ae8f6cb](https://github.com/vdavid/cmdr/commit/ae8f6cb), [34ecc70](https://github.com/vdavid/cmdr/commit/34ecc70))
- Check runner: CSV stats logging, cfg-gate enclosing block scope detection, file length check, flag combining fix ([9ac4b54](https://github.com/vdavid/cmdr/commit/9ac4b54), [539db62](https://github.com/vdavid/cmdr/commit/539db62), [4a24562](https://github.com/vdavid/cmdr/commit/4a24562), [6fe48a9](https://github.com/vdavid/cmdr/commit/6fe48a9))
- Refactors: split DualPaneExplorer and FilePane, extract dialog state, deduplicate templates and Settings CSS, split tauri-commands ([337f620](https://github.com/vdavid/cmdr/commit/337f620), [cfae0db](https://github.com/vdavid/cmdr/commit/cfae0db), [dad8790](https://github.com/vdavid/cmdr/commit/dad8790), [35a4239](https://github.com/vdavid/cmdr/commit/35a4239), [ba86d87](https://github.com/vdavid/cmdr/commit/ba86d87))
- License server: download tracking via Cloudflare Analytics Engine ([ef0f049](https://github.com/vdavid/cmdr/commit/ef0f049))
- Add Renovate for automated dependency updates ([00880a0](https://github.com/vdavid/cmdr/commit/00880a0))
- Add macOS Playwright E2E tests and CrabNebula E2E tests ([ec900ee](https://github.com/vdavid/cmdr/commit/ec900ee), [a768c03](https://github.com/vdavid/cmdr/commit/a768c03))
- Infra: uptime monitoring with UptimeRobot + Pushover, hardened deploy script ([19baefd](https://github.com/vdavid/cmdr/commit/19baefd))
- Add cfg-gate lint check for macOS-only Rust crates ([075c1d4](https://github.com/vdavid/cmdr/commit/075c1d4))

## [0.5.0] - 2026-02-15

### Added

- Add file viewer (F3) with three-backend architecture for files of any size, virtual scrolling, search with multibyte support, word wrap, horizontal scrolling, and keyboard shortcuts ([79268a4](https://github.com/vdavid/cmdr/commit/79268a4), [9f91bce](https://github.com/vdavid/cmdr/commit/9f91bce), [b10002a](https://github.com/vdavid/cmdr/commit/b10002a), [2ad2521](https://github.com/vdavid/cmdr/commit/2ad2521), [b65c422](https://github.com/vdavid/cmdr/commit/b65c422), [43adc86](https://github.com/vdavid/cmdr/commit/43adc86))
- Add drag-and-drop into Cmdr: pane and folder-level targeting, canvas overlay with file names and icons, Alt to switch copy/move, smart overlay suppression for large source images ([1ad1493](https://github.com/vdavid/cmdr/commit/1ad1493), [6207d8e](https://github.com/vdavid/cmdr/commit/6207d8e), [a89f18f](https://github.com/vdavid/cmdr/commit/a89f18f), [371746b](https://github.com/vdavid/cmdr/commit/371746b), [a3eae1c](https://github.com/vdavid/cmdr/commit/a3eae1c), [c776eed](https://github.com/vdavid/cmdr/commit/c776eed), [e97d3db](https://github.com/vdavid/cmdr/commit/e97d3db))
- Add settings window (⌘,) with declarative registry, fuzzy search, persistence, keyboard shortcut customization with conflict detection, and cross-window sync ([db121f6](https://github.com/vdavid/cmdr/commit/db121f6), [418f790](https://github.com/vdavid/cmdr/commit/418f790), [8f78596](https://github.com/vdavid/cmdr/commit/8f78596), [218b79b](https://github.com/vdavid/cmdr/commit/218b79b), [9c39db3](https://github.com/vdavid/cmdr/commit/9c39db3), [4e90137](https://github.com/vdavid/cmdr/commit/4e90137))
- Add MTP (Android device) support: browsing, file operations (copy, delete, rename, new folder), USB hotplug, multi-storage, MTP-to-MTP transfers ([938e87c](https://github.com/vdavid/cmdr/commit/938e87c), [672fa6e](https://github.com/vdavid/cmdr/commit/672fa6e), [d1e9f80](https://github.com/vdavid/cmdr/commit/d1e9f80), [7ac1528](https://github.com/vdavid/cmdr/commit/7ac1528), [b08af36](https://github.com/vdavid/cmdr/commit/b08af36), [ea845a6](https://github.com/vdavid/cmdr/commit/ea845a6), [fd8dad6](https://github.com/vdavid/cmdr/commit/fd8dad6))
- Add move feature (F6) reusing the copy UI as a unified transfer abstraction ([682d33a](https://github.com/vdavid/cmdr/commit/682d33a), [cb9e047](https://github.com/vdavid/cmdr/commit/cb9e047))
- Add rename feature with edge-case handling ([62799c6](https://github.com/vdavid/cmdr/commit/62799c6))
- Add swap panes feature with ⌘U shortcut ([2a1b329](https://github.com/vdavid/cmdr/commit/2a1b329))
- Add local AI for folder name suggestions in New Folder dialog, optional download ([b9a112e](https://github.com/vdavid/cmdr/commit/b9a112e), [3dc19c0](https://github.com/vdavid/cmdr/commit/3dc19c0))
- Add chunked copy with cancellation and pause support on network drives ([ba5409e](https://github.com/vdavid/cmdr/commit/ba5409e))
- Add 6 copy/move safety checks: path canonicalization, writability, disk space, inode identity, name length, special file filtering ([9548022](https://github.com/vdavid/cmdr/commit/9548022))
- Add sync status polling so iCloud/Dropbox icons update in real time ([ed36158](https://github.com/vdavid/cmdr/commit/ed36158), [6296412](https://github.com/vdavid/cmdr/commit/6296412))
- Add CSP to Tauri webview for XSS protection ([68bd510](https://github.com/vdavid/cmdr/commit/68bd510))
- Add copy/move folder-into-subfolder warning with clear error message ([521ab5e](https://github.com/vdavid/cmdr/commit/521ab5e))

### Fixed

- Fix panes getting stale when current directory or its parents are deleted ([1b5ad52](https://github.com/vdavid/cmdr/commit/1b5ad52))
- Fix multi-window race conditions that could crash the app ([9a33e24](https://github.com/vdavid/cmdr/commit/9a33e24))
- Fix recovering from poisoned mutexes instead of crashing (56 lock sites) ([62fd685](https://github.com/vdavid/cmdr/commit/62fd685))
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

- Migrate network discovery from NSNetServiceBrowser to mdns-sd: 68% code reduction, no unsafe code ([3d44cf1](https://github.com/vdavid/cmdr/commit/3d44cf1))
- Rewrite MCP server with fewer tools but more capabilities, auto-reconnect, and instructions field ([1061fad](https://github.com/vdavid/cmdr/commit/1061fad), [ede6463](https://github.com/vdavid/cmdr/commit/ede6463), [82345d1](https://github.com/vdavid/cmdr/commit/82345d1))
- Introduce ModalDialog component for all soft modals with drag support ([ffbf14a](https://github.com/vdavid/cmdr/commit/ffbf14a))
- Major refactors: split DualPaneExplorer, FilePane, volume_copy, listing/operations, connection modules ([04dc3de](https://github.com/vdavid/cmdr/commit/04dc3de), [e14c289](https://github.com/vdavid/cmdr/commit/e14c289), [2da8e6d](https://github.com/vdavid/cmdr/commit/2da8e6d), [c0bd500](https://github.com/vdavid/cmdr/commit/c0bd500), [707a96a](https://github.com/vdavid/cmdr/commit/707a96a))
- Security: pin GitHub Actions to commit SHAs, fix Paddle webhook timing attack, use crypto.getRandomValues for license codes, HTML-escape license emails, add webhook idempotency, constant-time admin auth ([c0d8cc3](https://github.com/vdavid/cmdr/commit/c0d8cc3), [70bc594](https://github.com/vdavid/cmdr/commit/70bc594), [51cd0b5](https://github.com/vdavid/cmdr/commit/51cd0b5), [bea3b2a](https://github.com/vdavid/cmdr/commit/bea3b2a), [9db450b](https://github.com/vdavid/cmdr/commit/9db450b), [b82f857](https://github.com/vdavid/cmdr/commit/b82f857))
- Docs overhaul: add colocated CLAUDE.md files throughout repo, architecture.md, branding guide ([eac9e61](https://github.com/vdavid/cmdr/commit/eac9e61), [dd91c78](https://github.com/vdavid/cmdr/commit/dd91c78))
- Website: add changelog, roadmap, newsletter signup with Listmonk + AWS SES, mobile responsiveness fixes, 512px logo ([643de6a](https://github.com/vdavid/cmdr/commit/643de6a), [07936d1](https://github.com/vdavid/cmdr/commit/07936d1), [ba4812d](https://github.com/vdavid/cmdr/commit/ba4812d), [aa661cf](https://github.com/vdavid/cmdr/commit/aa661cf))
- Add dead code check, manual CI trigger, pnpm security audit, LoC counter, summary job for branch protection ([9876600](https://github.com/vdavid/cmdr/commit/9876600), [3b20e66](https://github.com/vdavid/cmdr/commit/3b20e66), [ad22eba](https://github.com/vdavid/cmdr/commit/ad22eba))
- Tooling: extract shared Go check helpers, add VNC mode for Linux testing, fix Linux E2E environment ([550c353](https://github.com/vdavid/cmdr/commit/550c353), [6aa5ff7](https://github.com/vdavid/cmdr/commit/6aa5ff7), [fa907b6](https://github.com/vdavid/cmdr/commit/fa907b6))
- License server: add input validation, webhook idempotency, and security hardening ([4363a32](https://github.com/vdavid/cmdr/commit/4363a32), [9db450b](https://github.com/vdavid/cmdr/commit/9db450b), [7398965](https://github.com/vdavid/cmdr/commit/7398965))

## [0.4.0] - 2026-01-27

### Added

- Add file selection: Space toggles, Shift+arrows for range, Cmd+A for select all, selection info in status bar ([4d44cda](https://github.com/vdavid/cmdr/commit/4d44cda), [1cac4b3](https://github.com/vdavid/cmdr/commit/1cac4b3))
- Add copy feature with F5: copy dialog, destination picker with free space display, conflict handling ([281f45e](https://github.com/vdavid/cmdr/commit/281f45e), [fb5f027](https://github.com/vdavid/cmdr/commit/fb5f027), [a6d148d](https://github.com/vdavid/cmdr/commit/a6d148d), [6c661f2](https://github.com/vdavid/cmdr/commit/6c661f2))
- Add new folder feature with F7 shortcut and conflict handling ([80ec297](https://github.com/vdavid/cmdr/commit/80ec297))
- Add "Open in editor" feature with F4 shortcut ([7eb66ac](https://github.com/vdavid/cmdr/commit/7eb66ac))
- Add function key bar at bottom of UI for mouse-initiated actions ([537e040](https://github.com/vdavid/cmdr/commit/537e040))
- Add pane resizing: drag to resize between 25–75%, double-click to reset to 50% ([542b491](https://github.com/vdavid/cmdr/commit/542b491))
- Add multifile external drag and drop ([7426334](https://github.com/vdavid/cmdr/commit/7426334))
- Add keyboard navigation to network panes: PgUp/PgDn, Home/End, arrow keys ([70aa341](https://github.com/vdavid/cmdr/commit/70aa341))
- Add "Opening folder..." loading phase for network folders with distinct status messages ([9eb1185](https://github.com/vdavid/cmdr/commit/9eb1185))
- Add license key entry dialog with organization address and tax ID collection ([52480ce](https://github.com/vdavid/cmdr/commit/52480ce), [29eb6fe](https://github.com/vdavid/cmdr/commit/29eb6fe))

### Fixed

- Fix UI not updating on external file renames ([5de9346](https://github.com/vdavid/cmdr/commit/5de9346))
- Fix light mode colors ([42888c7](https://github.com/vdavid/cmdr/commit/42888c7))
- Fix cursor going out of Full view bounds ([7edcac8](https://github.com/vdavid/cmdr/commit/7edcac8))
- Fix ESC during loading navigating to wrong location ([b8c12e7](https://github.com/vdavid/cmdr/commit/b8c12e7))
- Fix focus after dragging window ([8488de6](https://github.com/vdavid/cmdr/commit/8488de6))
- Fix multiple volume selectors opening at once ([f4c4c21](https://github.com/vdavid/cmdr/commit/f4c4c21))
- Fix frontend race condition from refactor ([646c7af](https://github.com/vdavid/cmdr/commit/646c7af))

### Non-app

- Add E2E tests with tauri-driver on Linux using WebDriverIO in Docker ([1b0cbac](https://github.com/vdavid/cmdr/commit/1b0cbac))
- Revamp checker script: parallel execution, dependency graph, aligned output, colored durations ([7835b4c](https://github.com/vdavid/cmdr/commit/7835b4c))
- Add type drift detection between Rust and Svelte types ([b3ae1c3](https://github.com/vdavid/cmdr/commit/b3ae1c3))
- Add jscpd for Rust code duplication detection, CSS health checks, Go checks ([67e6c15](https://github.com/vdavid/cmdr/commit/67e6c15), [d177eb3](https://github.com/vdavid/cmdr/commit/d177eb3), [254075a](https://github.com/vdavid/cmdr/commit/254075a))
- Add Claude hooks for pre-session context and post-edit autoformat ([3d59dde](https://github.com/vdavid/cmdr/commit/3d59dde), [122182d](https://github.com/vdavid/cmdr/commit/122182d))
- Add LogTape logging for Svelte and debug pane for dev mode ([affa548](https://github.com/vdavid/cmdr/commit/affa548), [f494e15](https://github.com/vdavid/cmdr/commit/f494e15))
- Require reasoning in clippy lint exceptions ([d327cf4](https://github.com/vdavid/cmdr/commit/d327cf4))
- Website: fix hero image animation and sizing, fix broken Paddle references ([40faeee](https://github.com/vdavid/cmdr/commit/40faeee), [278ad4c](https://github.com/vdavid/cmdr/commit/278ad4c), [5eb5a52](https://github.com/vdavid/cmdr/commit/5eb5a52))
- License server: wire up Paddle checkout, fix webhook email fetching, support quantity > 1 ([3c40929](https://github.com/vdavid/cmdr/commit/3c40929))

## [0.3.2] - 2026-01-14

### Fixed

- Fix auto-updater to download updates and restart the app after updating ([c0bff9a](https://github.com/vdavid/cmdr/commit/c0bff9a))

### Non-app

- Website: redesign with mustard yellow theme, view transitions, hero animation, and reduced motion support ([0296379](https://github.com/vdavid/cmdr/commit/0296379), [18b729f](https://github.com/vdavid/cmdr/commit/18b729f), [689a151](https://github.com/vdavid/cmdr/commit/689a151))
- Website: avoid aggressive caching, rearrange T&C ([8ca0539](https://github.com/vdavid/cmdr/commit/8ca0539), [c92dff8](https://github.com/vdavid/cmdr/commit/c92dff8))
- Tooling: turn off MCP stdio sidecar, fix Rust-Linux check, reduce CI frequency, fix latest.json formatting ([5dda608](https://github.com/vdavid/cmdr/commit/5dda608), [2ec3f7e](https://github.com/vdavid/cmdr/commit/2ec3f7e), [42d81ab](https://github.com/vdavid/cmdr/commit/42d81ab), [52980ae](https://github.com/vdavid/cmdr/commit/52980ae))
- Docs: release process and auto-updater documentation ([c7c36f6](https://github.com/vdavid/cmdr/commit/c7c36f6), [765f5ad](https://github.com/vdavid/cmdr/commit/765f5ad), [f3785da](https://github.com/vdavid/cmdr/commit/f3785da), [10e43de](https://github.com/vdavid/cmdr/commit/10e43de))

## [0.3.1] - 2026-01-14

### Added

- Add custom title bar, 4 px narrower for more content space ([33e90c8](https://github.com/vdavid/cmdr/commit/33e90c8))

### Changed

- Replace rusty icon with yellow one ([79777e3](https://github.com/vdavid/cmdr/commit/79777e3))

### Fixed

- Fix app name in task switcher: shows "Cmdr" instead of "cmdr" ([8117300](https://github.com/vdavid/cmdr/commit/8117300))

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
- Full mode (vertical scroll with size/date columns) and Brief mode (horizontal multi-column), switchable via ⌘1/⌘2 ([c779a6d](https://github.com/vdavid/cmdr/commit/c779a6d))
- Virtual scrolling for 100k+ files ([cf6c35d](https://github.com/vdavid/cmdr/commit/cf6c35d))
- Chunked directory loading (50k files: 350 ms to first files) ([869cdfb](https://github.com/vdavid/cmdr/commit/869cdfb))
- File icons from OS with caching ([b8c588e](https://github.com/vdavid/cmdr/commit/b8c588e))
- File metadata panel with size color coding and date tooltips ([bc3dc85](https://github.com/vdavid/cmdr/commit/bc3dc85))
- Native context menu (Open, Show in Finder, Copy path, Quick Look) ([7d977a1](https://github.com/vdavid/cmdr/commit/7d977a1))
- Live file watching with incremental diffs ([cf12372](https://github.com/vdavid/cmdr/commit/cf12372))
- Dropbox and iCloud sync status icons ([46f1770](https://github.com/vdavid/cmdr/commit/46f1770))
- Volume switching with keyboard navigation ([ba3e770](https://github.com/vdavid/cmdr/commit/ba3e770))
- Network drives (SMB): host discovery via Bonjour, share listing, authentication, and mounting ([54ee04f](https://github.com/vdavid/cmdr/commit/54ee04f))
- Sorting by name, size, date, extension with alphanumeric sort ([e7b7206](https://github.com/vdavid/cmdr/commit/e7b7206))
- Back/Forward navigation ([56a5bf6](https://github.com/vdavid/cmdr/commit/56a5bf6))
- Drag and drop from the app ([8e1d53b](https://github.com/vdavid/cmdr/commit/8e1d53b))
- Command palette with fuzzy search ([7b0ea13](https://github.com/vdavid/cmdr/commit/7b0ea13))
- Window state persistence (position and size remembered) ([b8d93c5](https://github.com/vdavid/cmdr/commit/b8d93c5))
- Dark mode support ([7deb986](https://github.com/vdavid/cmdr/commit/7deb986))
- Show hidden files menu item ([4af855d](https://github.com/vdavid/cmdr/commit/4af855d))
- Full disk access permission handling ([9f433d8](https://github.com/vdavid/cmdr/commit/9f433d8))
- Licensing features (validation, about screen, expiry modal) ([dc68eeb](https://github.com/vdavid/cmdr/commit/dc68eeb))
- Keyboard shortcuts: Backspace/⌘↑ (go up), ⌥↑/↓ (home/end), Fn arrows (page up/down) ([fc899d4](https://github.com/vdavid/cmdr/commit/fc899d4))
- getcmdr.com website ([0f9eb21](https://github.com/vdavid/cmdr/commit/0f9eb21))
- License server (Cloudflare Worker) with Ed25519-signed keys ([bff3e8a](https://github.com/vdavid/cmdr/commit/bff3e8a))

---

### Development history

<details>
<summary>Click to expand full development history</summary>

#### 2026-01-10 - Initial public release

- Add licensing features to app (validation, about screen, expiry modal) ([dc68eeb](https://github.com/vdavid/cmdr/commit/dc68eeb))
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
- Add license server (Cloudflare Worker) with Ed25519-signed keys ([bff3e8a](https://github.com/vdavid/cmdr/commit/bff3e8a))
- Add legal pages (privacy policy, terms, refund policy, pricing) ([4f32a29](https://github.com/vdavid/cmdr/commit/4f32a29))
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

- Add sorting feature (name, size, date, extension) with alphanumeric sort ([e7b7206](https://github.com/vdavid/cmdr/commit/e7b7206))
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

- Add keyboard shortcuts: ⌥↑/↓ for home/end, Fn arrows for page up/down ([6298990](https://github.com/vdavid/cmdr/commit/6298990))
- Move file cache to backend for major speed improvements ([a42eda5](https://github.com/vdavid/cmdr/commit/a42eda5))
- Optimize directory loading (phase 1 and 2) ([7efd61a](https://github.com/vdavid/cmdr/commit/7efd61a))

#### 2025-12-29 - View modes and cloud sync

- Add Full mode (vertical scroll with size/date columns) and Brief mode (horizontal multi-column) ([c779a6d](https://github.com/vdavid/cmdr/commit/c779a6d))
- Add Dropbox and iCloud sync status icons ([46f1770](https://github.com/vdavid/cmdr/commit/46f1770))
- Add loading screen animation ([234f0a7](https://github.com/vdavid/cmdr/commit/234f0a7))

#### 2025-12-28 - Performance and file operations

- Add chunked directory loading (50k files: 350 ms to first files) ([869cdfb](https://github.com/vdavid/cmdr/commit/869cdfb))
- Add file metadata panel with size color coding and date tooltips ([bc3dc85](https://github.com/vdavid/cmdr/commit/bc3dc85))
- Add native context menu (Open, Show in Finder, Copy path, Quick Look) ([7d977a1](https://github.com/vdavid/cmdr/commit/7d977a1))
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
