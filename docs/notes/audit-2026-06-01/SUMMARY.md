# Pre-launch security and reliability audit — 2026-06-01

Independent adversarial review of the Cmdr desktop app (Rust/Tauri 2 backend + Svelte 5 frontend) ahead of launch.
Read-only: no source files were modified. Findings below each have a standalone file in this directory.

**Headline:** the codebase is in strong shape. Across seven lenses I filed **13 findings** — one high, five medium, and
seven low — and most are defense-in-depth or maintainability rather than active bugs. The data-loss-class write paths
(copy/move/delete/trash, cross-volume, safe overwrite, durability flush) are heavily guarded and well-tested, and I
verified the documented guards actually exist in code, not just in the docs. The concurrency lens turned up **zero**
fileable issues. The security controls I could check statically (updater signature, license verification, MCP auth,
`withGlobalTauri` gating, secret redaction, FDA gate coverage) are correctly implemented.

This was a **static** audit. I did not run the app, take live memory snapshots, or fuzz anything. Confidence levels on
each finding reflect that.

## All findings

| File | Severity | Lens | Title |
| --- | --- | --- | --- |
| `high-G-git-watcher-subscription-leak.md` | high | G — Resource hygiene | Concurrent git subscribe leaks watcher subscriptions over a long session |
| `medium-C-savesettings-silent-swallow.md` | medium | C — Error handling | `saveSettings` swallows persist failures with no log, losing FDA + onboarding state |
| `medium-F-smb-password-in-process-argv.md` | medium | F — Security | SMB password leaks into the process argument list on the CLI fallback paths |
| `medium-G-listing-cache-no-backstop-eviction.md` | medium | G — Resource hygiene | `LISTING_CACHE` / `WATCHER_MANAGER` have no backstop eviction for orphaned listings |
| `medium-D-smb-upgrade-orchestration-in-command-layer.md` | medium | D — IPC boundary | SMB-upgrade orchestration lives in the command layer, breaking the thin-pass-through contract |
| `medium-D-untyped-event-payloads.md` | medium | D — IPC boundary | IPC event payloads are entirely untyped — no specta link between Rust emit and FE listen |
| `low-A-atomic-json-no-fsync.md` | low | A — Data safety | `atomic_write_json` renames a temp it never fsync'd, so power loss can leave a zero-length config file |
| `low-F-download-update-url-unvalidated.md` | low | F — Security | `download_update` fetches a frontend-supplied URL with no scheme/host check (signature still verified) |
| `low-E-entitlements-library-validation-disabled.md` | low | E — macOS platform | Hardened-runtime exceptions: unsigned-executable-memory + disabled library validation |
| `low-C-thread-join-panic-propagation.md` | low | C — Error handling | Worker-thread panic re-propagates and crashes the calling command (icons / sync-status fan-out) |
| `low-G-icon-disk-cache-unbounded.md` | low | G — Resource hygiene | On-disk icon cache grows without bound across sessions |
| `low-D-excluded-commands-table-incomplete.md` | low | D — IPC boundary | `ipc/CLAUDE.md` "Excluded commands" table omits ~13 raw-invoke survivors |
| `low-C-panic-inventory.md` | (baseline) | C — Error handling | Panic-pattern inventory: ~174 non-test `unwrap`/`expect`/`unreachable`, 0 judged risky |

## Top 5 to fix before launch

1. **SMB password in the process argument list** (`medium-F-smb-password-in-process-argv.md`). A real credential
   exposure: on the `smbutil`/`smbclient` CLI fallback paths the password rides in argv (`-U user%pass`, or embedded in
   the `smb://user:pass@host` URL), readable by any local process via `ps` / `/proc/<pid>/cmdline`. The primary NetFS
   mount path deliberately avoids this (network/CLAUDE.md says so), so the fallback silently downgrades the guarantee.
   Fix is small (authfile / env var) and it's a credential leak — worth closing before launch even though the path is
   rare. **Verified against code.**

2. **Git watcher subscription leak** (`high-G-git-watcher-subscription-leak.md`). The only high. `subscribeToRepo` does
   two `await`s before reserving its map slot, and it's driven from a fire-and-forget `$effect` with no generation
   guard, so two interleaving subscribes to the same repo leave the frontend refcount at 1 while the backend sits at 2 —
   permanently pinning an OS `.git` watcher + gix repo handle for the rest of the session. Bites a user who navigates
   fast through many repos over a multi-day session. The fix (coalesce concurrent subscribes onto one in-flight promise)
   is cheap. **Verified against code; severity reflects unbounded OS-resource growth, confidence medium because it needs
   a race to trigger.**

3. **`saveSettings` silently swallows persistence failures** (`medium-C-savesettings-silent-swallow.md`). An empty
   `catch {}` around the store write that persists `fullDiskAccessChoice` and `isOnboarded`. If the write fails, the FDA
   decision and onboarding-complete flag vanish with no log, and the next launch re-runs onboarding / re-prompts for Full
   Disk Access. At minimum log the error; better, surface it. Trivial fix, and it touches the most user-visible state in
   the app. **Verified against code.**

4. **No backstop eviction for orphaned listings** (`medium-G-listing-cache-no-backstop-eviction.md`). `LISTING_CACHE`
   (full `Vec<FileEntry>`, up to 50k+) and `WATCHER_MANAGER` (live OS watcher) only shrink via an explicit frontend
   `list_directory_end` IPC — no TTL, no cap, no backend reaper. The file-viewer subsystem already has a
   `WindowEvent::Destroyed` net for exactly this "FE close IPC not delivered" risk; the larger, longer-lived listing
   subsystem doesn't. Add a cheap age-based reaper on the existing `created_at`. Defense-in-depth for a process designed
   to run for days.

5. **`atomic_write_json` isn't power-loss-atomic** (`low-A-atomic-json-no-fsync.md`). The only data-safety finding. The
   four config stores `write` + `rename` with no `fsync` of the temp data or the parent dir, so a power loss in the
   wrong window can replace a good file with a zero-length one. Three of four self-heal, but `manual-servers.json` holds
   non-rediscoverable user-entered SMB servers. Low severity, but it's user data and the fix (fsync before/after rename,
   mirroring `write_from_stream`) is a few lines. Worth doing for the one store that matters.

(The two `medium-D` findings — command-layer orchestration and untyped event payloads — are real but are
maintainability/testability concerns, not launch blockers. The untyped-events one is a sizable migration; stage it
post-launch.)

## Intentional, documented trade-offs (considered, not filed)

These came up during the audit and are deliberate, documented decisions with stated rationale — left as-is per the
scoping rule:

- **Overwrite is not reversible** — copy/move keep no backup of a replaced original; `transfer/CLAUDE.md` reasons the
  bounded-disk cost of backups is the worse surprise. Verified the temp+rename-aside path deletes (not retains) the
  aside.
- **Delete and trash don't `fsync`** — `delete/CLAUDE.md`: annoyance-class, not data-loss-class; pinned by a test that
  forbids `libc::sync()` reappearing.
- **Indexing SQLite is a disposable cache** — `indexing/CLAUDE.md`: WAL + `synchronous=NORMAL`, corruption triggers
  delete-and-rebuild; not user data.
- **`create_file` / `create_directory` are no-clobber** via `create_new` / `create_dir` (error on existing, never
  truncate).
- **MTP/SMB partial-write cleanup** lives in the `smb2` / `mtp-rs` library layer with named regression tests; the Cmdr
  glue honors it.
- **Cross-volume file→file safe-replace**, **cross-FS move flush-before-source-delete**, **TOCTOU placeholder
  reservation**, **per-file cross-volume rollback granularity**, **dest-inside-source guard** — all documented, and all
  verified present in code as described.
- **`settings.json` / `license.json` / `shortcuts.json` atomicity** is owned by `tauri-plugin-store`; secrets live in the
  OS secret store, never these files.
- **Lock-poison `expect`/`unwrap`** across the shared-state mutexes is a deliberate "poison ⇒ abort" stance. Whether to
  migrate to the existing `lock_ignore_poison()` helper is an open question (see note below), not a documented decision
  per se — flagged in the panic inventory, not double-filed.
- **JSON-over-binary IPC**, **viewer window has no `store` capability**, **no `skip_serializing_if`** — documented
  decisions.
- **Linux encrypted-file secret fallback keyed on `machine-id`** is acknowledged obfuscation; `0600` perms are the real
  protection (`secrets/CLAUDE.md`), acceptable for alpha Linux SMB.
- **Error-report Discord 7-day presigned links** — reasoned in `docs/security.md` with a stated mitigation if access
  widens.
- **`mcp-*` bridge events stay loose JSON** (automation round-trip channel) — the untyped-events finding explicitly
  excludes them.
- **Several uncapped caches are inherently bounded** (`OWNER_CACHE`/`GROUP_CACHE` by system user count, `CREDENTIAL_CACHE`
  by shares, `EXT_CACHE` by extension and wiped on launch, in-memory `ext:`/`dir:` icons), and `RepoCache` has no idle
  TTL by documented decision.

## Security controls checked and found solid

So the summary records what's verified-good, not just what's wrong:

- **`withGlobalTauri`**: `false` in `tauri.conf.json`; the wrapper only flips it to `true` when an `instanceId` is set
  (dev/E2E/worktree), and prod builds skip wrapper composition entirely. The Tauri MCP bridge plugin is
  `#[cfg(debug_assertions)]`-gated. Prod cannot expose `__TAURI__`.
- **Updater**: minisign signature verified with a compiled-in pubkey **before** anything is written to disk; atomic-rename
  install; the admin-escalation shell-out is injection-safe (quoted, with a regression test). Not an RCE path.
- **License**: Ed25519 with compiled-in pubkey, genuinely enforced (tamper + wrong-key tests present); `CMDR_MOCK_LICENSE`
  is debug-only.
- **MCP server**: binds `127.0.0.1` only, default-disabled in prod, per-launch CSPRNG bearer token (`0600` file, fresh
  each start, cleared on stop → fails closed), constant-time compare, Origin validation, and the token gate covers
  exactly the confirmation-bypass tools (auto-confirm delete/move/copy, `dialog confirm`, all `set_setting`). No
  destructive auto-confirm path reaches dispatch without auth.
- **Secret redaction**: keychain/secret-store code logs keys and error context, never values; the AI client never logs
  the API key; SMB passwords are logged only via the `***`-masked `safe_url`; the salted redactor scrubs userinfo from
  any line reaching a bundle.
- **FDA gate coverage**: thorough — `get_icons`/`refresh_directory_icons`, `volumes` favorites + cloud drives +
  `get_icon_for_path`, indexer auto-start, MTP watcher, and the Downloads watcher are all gated on
  `is_fda_pending`/`is_fda_pending_runtime`.
- **AI SSRF/key-exfil guard**: `validate_ai_base_url` enforces HTTPS for non-loopback hosts when a key is set (loopback
  keeps `http` for Ollama/LM Studio).
- **Concurrency**: every std `Mutex`/`RwLock` guard in the async-heavy modules is dropped or `.take()`n before the
  relevant `.await`; no lock-across-await, no inconsistent lock ordering, no sync FS in an async handler without the
  `blocking_with_timeout` wrapper. Lens B produced zero findings.

## Open question worth a decision (not filed)

The shared-state mutexes use `.lock().expect("... poisoned")` (abort on poison) in some modules and the
`lock_ignore_poison()` helper in others (`volume_copy.rs`, `lib.rs`). It's inconsistent. For a file manager that must
"feel rock solid," a single panicked thread poisoning a lock and then aborting the whole app on the next acquisition is
a harsh failure mode. Worth a deliberate project-wide call: abort-on-poison everywhere, or recover-on-poison everywhere.
Not filed because it's a design stance, not a bug.

## Areas I'd revisit in a second pass

- The **concurrent (`FuturesUnordered`) volume-copy path** in `volume_copy.rs` — I read the serial path and the
  documented decisions/tests but didn't trace the concurrent `cleanup_temp` / `CreatedPaths` bookkeeping line-by-line.
- **SMB/MTP `write_from_stream`** direct read — relied on `backends/CLAUDE.md` plus named integration tests rather than
  re-reading `smb.rs`/MTP write code.
- The full **`transfer_driver.rs`** cancel/skip-accounting state machine branches.
- **`ai/` client internals** (model loading / streaming locks) and **`updater/` + `licensing/` async paths** read in
  full (grepped for guard-across-await, found none).
- **Runtime verification** of everything: MCP token enforcement live, Tauri's per-command catch-unwind behavior (does a
  command panic crash the app or just the thread?), actual memory growth, and fuzzing the redactor corpus.

## Subsystems NOT covered (and why)

- **`apps/api-server` (Cloudflare Worker), `apps/analytics-dashboard`, `apps/website`** — out of scope. The brief
  targeted the desktop app (`apps/desktop`). The server-side error-report retention and Discord deep-link posture is
  documented in `docs/security.md`; I did not audit the Worker endpoints, admin routes, or KV/R2 access control.
- **`smb2` / `mtp-rs` sibling crate internals** — the brief said "just the Cmdr glue." Partial-write recovery was
  verified at the Cmdr trait/glue level and via named tests, not by reading the crates' source.
- **Linux paths** (`volumes_linux/`, `mount_linux.rs`, inotify watch-limit handling) — spot-checked at the CLAUDE.md /
  grep level only; the launch target is macOS, so depth went to macOS paths.
- **Exhaustive frontend review** — only the lens-targeted areas (IPC, error handling, resource teardown, data-write
  wiring). Not a component-by-component Svelte audit; UI correctness, accessibility, and visual polish were out of scope.
- **`file_viewer` large-file/encoding correctness** — touched only for handle-leak purposes.
- **Drag-and-drop drop-target write wiring** — confirmed it reuses the safe `copy_files`/`move_files` IPCs; the native
  drag-drop-into-a-write path wasn't traced end-to-end.
- **Test code, coverage adequacy, and the check runner** — not part of a correctness/safety/security audit.

## Method

Six parallel sub-audits, one per lens (A data safety, B concurrency, C error handling, D IPC boundary, E+F macOS
platform + security, G resource hygiene). Each read the relevant subsystem `CLAUDE.md` files first to avoid filing
documented trade-offs, then grepped + read the actual code, citing `file:line` for every claim. The three headline
findings (high-G, medium-F, medium-C) were independently re-verified against source by the orchestrator before landing
in the top-5. Findings already explained as deliberate in a `CLAUDE.md` were excluded and listed under "intentional,
documented" above.
