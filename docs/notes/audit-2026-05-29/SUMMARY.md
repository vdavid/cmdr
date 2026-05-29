# Cmdr pre-launch audit — 2026-05-28

Independent security and reliability audit across seven lenses (A–G) on commit `e429aa14`. Audit-only; no source was
modified. Findings live in this directory as one markdown file each, named `<severity>-<lens>-<slug>.md`.

## Findings table

37 findings total: **6 high**, **18 medium**, **13 low**. Zero critical.

| File                                                                  | Sev    | Lens | Title                                                                                                 |
| --------------------------------------------------------------------- | ------ | ---- | ----------------------------------------------------------------------------------------------------- |
| high-A-volume-overwrite-deletes-dest-before-stream-success.md         | high   | A    | Cross-volume copy/move deletes destination before confirming source stream succeeds                   |
| high-A-chunked-copy-truncates-existing-dest-on-overwrite.md           | high   | A    | Chunked copy (USB / network) truncates destination before knowing source can be read end-to-end       |
| high-A-rename-conflict-resolution-fails-on-apfs-and-linux-local-copy.md | high | A    | Rename conflict-resolution race: `find_unique_name` + `COPYFILE_EXCL`/`O_EXCL` collide silently       |
| high-D-fs-plugin-unscoped-allow-write-and-remove.md                   | high   | D    | `default.json` grants `fs:allow-temp-write` and `fs:allow-remove` with no path scope                  |
| high-D-debug-window-inherits-full-main-capability.md                  | high   | D    | Debug window inherits main's full capability set (process, mcp-bridge, updater, fs:allow-remove)      |
| high-F-mcp-destructive-ops-no-origin.md                               | high   | F    | MCP `validate_origin` allows no-Origin requests; any local process can `delete` / `move` with `autoConfirm` |
| medium-A-overwrite-rollback-leaves-no-original.md                     | medium | A    | Overwrite rollback destroys the backup before failure path, so multi-file rollback restores nothing   |
| medium-A-move-with-rename-overwrite-loses-dest.md                     | medium | A    | Same-FS rename-overwrite path loses the destination on failure                                        |
| medium-A-local-write-from-stream-no-fsync-no-error-cleanup.md         | medium | A    | Local `write_from_stream` does not fsync and leaves partial file on error                             |
| medium-A-batch-trash-partial-failure-silently-emits-complete.md       | medium | A    | Batch trash partial failure surfaces as "complete"; user thinks files are in trash                    |
| medium-A-local-delete-aborts-midbatch-no-cancelled-event.md           | medium | A    | Local delete aborts mid-batch without emitting a cancelled/error event                                |
| medium-B-cloud-actions-no-timeout.md                                  | medium | B    | `cloud_make_available_offline` / `cloud_remove_download` lack the mandated timeout wrapper            |
| medium-B-space-poller-sync-statfs-on-runtime.md                       | medium | B    | Space poller runs sync `statfs` / NSURL FFI inline on tokio workers                                   |
| medium-B-listing-sort-enrich-on-runtime.md                            | medium | B    | Post-read finalize (sort/enrich/cache write) runs inline on async worker for 100k+ listings           |
| medium-B-verifier-sync-readdir-on-runtime.md                          | medium | B    | Per-navigation index verifier's `read_dir` + `symlink_metadata` runs without `spawn_blocking`         |
| medium-D-unstructured-event-payloads-bypass-specta.md                 | medium | D    | ~30 `app.emit(name, serde_json::json!(...))` event call sites bypass typed bindings                   |
| medium-D-fat-rename-command-file.md                                   | medium | D    | `commands/rename.rs` carries ~270 LOC of business logic                                               |
| medium-D-show-file-context-menu-builds-context-and-state.md           | medium | D    | `show_file_context_menu` orchestrates iCloud probing, sync-status, LaunchServices, menu popup inline  |
| medium-D-raw-invoke-call-sites-outside-documented-exclusions.md       | medium | D    | 16 raw `invoke('…')` sites with "tracked for follow-up" disables; configure_ai passes API key untyped |
| medium-E-updater-tcc-perm-loss-mid-crash.md                           | medium | E    | Custom updater sync isn't crash-safe across phases; mid-install crash → signature-mismatch SIGKILL    |
| medium-F-updater-download-url-unvalidated.md                          | medium | F    | `download_update(url, signature)` takes FE-supplied URL with no scheme/host check and no size cap     |
| medium-F-ai-base-url-no-scheme-validation.md                          | medium | F    | `configure_ai` accepts any `cloud_base_url`; BYOK key can be sent over plaintext `http://attacker`    |
| medium-G-viewer-sessions-orphan-on-os-close.md                        | medium | G    | Viewer `SESSIONS` orphan on macOS titlebar-X close (no Rust-side WindowEvent cleanup)                 |
| medium-G-scan-preview-results-leak-on-dialog-dismiss.md               | medium | G    | `SCAN_PREVIEW_RESULTS` leaks on dialog dismiss after scan completes (no TTL, no cap)                  |
| low-A-cross-fs-move-staging-cleanup-best-effort.md                    | low    | A    | No recovery sweeper for `.cmdr-tmp-` / `.cmdr-staging-` artefacts on next launch                      |
| low-A-macos-and-linux-native-copies-skip-fsync.md                     | low    | A    | macOS / Linux native copies skip explicit fsync; user may lose data on power loss                     |
| low-B-watcher-debouncer-fire-and-forget.md                            | low    | B    | Git watcher's notify callback chains into `tokio::spawn` on the notify-rs internal thread             |
| low-B-cancel-write-op-race-with-conflict.md                           | low    | B    | TOCTOU between cancel's `take()` and worker's conflict-resolution oneshot install                     |
| low-B-find-first-fuzzy-async-no-spawn.md                              | low    | B    | `find_first_fuzzy_match` async without `spawn_blocking`                                               |
| low-C-panics-in-backend.md                                            | low    | C    | ~130 `unwrap`/`expect`/`panic!`/`unreachable!` sites triaged; almost all justified, two soft cleanups |
| low-C-result-string-stragglers.md                                     | low    | C    | 117 commands across 20 files still return `Result<T, String>` instead of typed `IpcError`             |
| low-E-swizzle-process-wide-scope.md                                   | low    | E    | `drag_image_detection::install_swizzles` lacks function-level idempotency gate                        |
| low-E-app-not-sandboxed-no-explicit-assert.md                         | low    | E    | Cmdr ships unsandboxed; assumption is implicit across many subsystems                                 |
| low-E-text-size-system-strings-future-macos.md                        | low    | E    | Undocumented Apple APIs (UIKit content-size key, `.loctable` paths); calling out for maintenance log  |
| low-F-csp-style-unsafe-inline.md                                      | low    | F    | Prod CSP has `style-src 'unsafe-inline'`; defense-in-depth concern                                    |
| low-F-port-files-default-permissions.md                               | low    | F    | `mcp.port` / `tauri-mcp.port` use default `0o644`; harden to `0o600`                                  |
| low-G-icon-cache-unbounded-path-keys.md                               | low    | G    | `ICON_CACHE` `path:<full path>` entries accumulate per directory visited; no eviction policy          |

## Top 5 to fix before launch

These are the findings where the cost-to-fix is moderate but the user-facing failure mode is severe (data loss or
unconsented destructive action). All are independent fixes.

1. **high-A-volume-overwrite-deletes-dest-before-stream-success.md** — cross-volume copy/move deletes the destination
   before knowing the source read will succeed. SMB / MTP / removable-media disconnect mid-stream → user loses both
   source and dest. The "Overwrite" UX promise is broken outside the same-APFS-volume happy path.
2. **high-A-chunked-copy-truncates-existing-dest-on-overwrite.md** — same family, on the local chunked path used for
   USB and network mounts. Same data-loss class.
3. **high-F-mcp-destructive-ops-no-origin.md** — `validate_origin` allows requests with no Origin header at all. Any
   local process can read `<data_dir>/mcp.port` and POST a delete / move with `autoConfirm: true`, bypassing the
   confirmation dialog real users see. Add a token file (`0o600`) and refuse `autoConfirm` on destructive tools.
4. **high-D-fs-plugin-unscoped-allow-write-and-remove.md** — `default.json` capability grants `fs:allow-temp-write`
   and `fs:allow-remove` with no path scope, so a future webview compromise could `remove()` arbitrary user files.
   The frontend only uses two known paths; scope the perms to them.
5. **high-A-rename-conflict-resolution-fails-on-apfs-and-linux-local-copy.md** — `find_unique_name` + the kernel's
   `COPYFILE_EXCL` / `O_EXCL` race. A concurrent write between the name probe and the create can silently merge or
   overwrite content. Same "assumed-truncate semantics that aren't actually truncate" pattern as the two A-highs above.

The two **high-D capability** findings are listed at 4 (fs scope) and structurally relevant at the secondary spot
**high-D-debug-window-inherits-full-main-capability.md** — currently gated by `import.meta.env.DEV` and
`#[cfg(debug_assertions)]` so runtime risk is zero, but the documented per-window capability split is broken at the
file level. Either trim the `windows` list in `default.json` or move debug-only items into a `debug.json` capability.
Worth fixing before launch because a future frontend gate slip would expose it.

## Areas to revisit in a second pass

The audit covered everything under `apps/desktop/src-tauri/src/` and `apps/desktop/src/lib/` at varying depths.
Highest-value follow-ups:

- **MTP and SMB backend internals of `write_from_stream` and `open_read_stream`.** Lens A confirmed they exist and
  stream, but didn't drill into per-backend partial-write semantics, session-loss races, or temp-cleanup contracts.
  Same applies to the SMB watcher's `next_events` → `attempt_reconnect` handoff for race-with-disconnect, and MTP
  session-lock interaction during concurrent reads + writes (Lens B noted the split-lock design but couldn't deep-dive).
- **No crash-mid-operation recovery scanner exists** for `.cmdr-tmp-` / `.cmdr-staging-` artefacts. Captured as a low
  but really worth a dedicated design pass — the "protect user data" principle implies a sweeper.
- **macOS `copyfile` COPYFILE_QUIT-via-callback after partial cancel.** Existing code defends it but the cleanup
  completeness in the cancel arm wasn't audited.
- **Index DB writes during file operations.** The indexer's perspective on mutations wasn't audited; could race with
  in-flight transfers.
- **Indexing writer-thread message ordering under cancel.** Lens B flagged the verifier; the writer side is
  unexplored.
- **FSEvents debouncer behavior under disconnect-mid-batch.** Volume unmount during a high-FSEvents-rate operation.
- **Mutation-test pass on backend `unwrap` sites.** Lens C triaged by inspection; an automated mutation pass would
  raise confidence that the "justified" calls really are.
- **Capability inheritance map.** The debug-window finding hinted at structural drift. A pass that enumerates every
  capability × every window would catch the next instance.

## Intentional, documented trade-offs (considered, NOT filed)

These were inspected and confirmed as deliberate decisions with stated rationale in the colocated CLAUDE.md files.
Listed so the maintainer can verify the call:

- `LocalPosixVolume::open_read_stream` channel-backed streaming, `SmbVolume` split lock pattern, MTP `backend_cancel`
  `Arc<AtomicBool>` plumbing — all documented as deliberate (Lens B).
- `SmbVolume::on_unmount`'s `blocking_lock` / `blocking_write` — documented gotcha; callers wrap in `spawn_blocking`.
- Indexing `block_in_place + block_on` at shutdown — documented and correct.
- Listing's known network-mount blocking gap on `read_dir` — explicitly acknowledged in `architecture-patterns.md`,
  mitigated via separate-thread + 100ms poll.
- "Background task runs to completion even if cancelled on frontend" gotcha (listing) — explicitly known and mitigated
  via `AtomicBool`.
- `Volume::delete`'s "file or empty directory" contract enforcing merge semantics for dir Overwrite (Lens A).
- Hardlink dedup not straddling the oracle/walk boundary (Lens A).
- Volume disconnect mid-walk race — documented as "future investigation" in delete CLAUDE.md (Lens A).
- Local delete being non-rollbackable (Lens A); only the partial-progress signaling gap was filed.
- 20 Rust `allowed-error-string-match` opt-outs + 2 TS `cmdr/no-error-string-match` opt-outs — all justified (CLI
  subprocess stderr with `LC_ALL=C` pinning, Display-impl test assertions, etc.) (Lens C).
- All `eprintln!` / `println!` survivors in `src-tauri/src/` have explicit `#[allow(clippy::print_stderr, reason=...)]`
  justifications (Lens C).
- `cargo-deny` advisories disabled — team uses `cargo audit` instead (Lens E).
- AI base URL — `configure_ai` accepts the URL by design for BYOK custom endpoints, but Lens F filed the lack of
  scheme validation as a separate hardening item.

## Subsystems explicitly NOT covered

By scope:

- **`mtp-rs` and `smb2` sibling crates** at `~/projects-git/vdavid/{mtp-rs,smb2}` — out of `apps/desktop/` scope. The
  Cmdr glue was audited; the crates themselves were not.
- **`apps/api-server/`** (Cloudflare Worker, licensing / telemetry / crash-report / download endpoints) — a separate
  attack surface that warrants its own audit. Not in scope here.
- **`apps/analytics-dashboard/`** — private dashboard, not user-facing.
- **`apps/website/`** — marketing site, separate concern.
- **`scripts/check/`** Go check runner — tooling, low user-facing-bug surface.

By depth:

- **`volumes_linux/` and `stubs/`** were only shallowly scanned. Cmdr is macOS-first; Linux is best-effort.
- **E2E test framework correctness** (Playwright, Linux Docker WebDriverIO) — fixture races, fake-fs glitches, etc.
  were out of scope.
- **Accessibility / a11y** lens — the user's audit prompt did not include accessibility as a lens.
- **Performance benchmarks** — Lens G surfaces resource hygiene (leaks, unbounded growth) but doesn't quantify
  throughput. The `docs/notes/json-ipc-benchmarks.md` referenced in the architecture is the right ongoing source for
  that.
- **License + activation backend logic** — `licensing/CLAUDE.md` was consulted by Lens F for secret handling, but the
  Ed25519 verification flow itself was not audited end-to-end (server side is out of scope).

## Notes on the audit process

- Run on the `audit/2026-05-28` branch in worktree `.claude/worktrees/audit-2026-05-28/`. No source modified.
- Seven parallel sub-agents, one per lens, each instructed to read the lens-relevant `CLAUDE.md` files before flagging
  anything so documented trade-offs would not be re-surfaced.
- Output template was uniform across lenses; severity / confidence are agent judgements.
- If a finding cites a line range that has shifted since the audit, re-grep on the slug in the finding's title — the
  semantic location is more durable than the absolute line number.
