# Cmdr pre-launch security & reliability audit — 2026-05-31

Independent adversarial review of the Cmdr desktop app (`apps/desktop/`), Rust (Tauri 2) backend + Svelte 5 frontend.
Audited across seven lenses (A data safety, B concurrency, C error handling, D IPC, E macOS platform, F security, G
resource hygiene). Every finding below was verified by reading the cited source lines; the four highest-severity ones
were independently re-read by the lead reviewer (not just the lens pass).

**Headline:** this is an unusually disciplined codebase. The write-operations machinery (temp+rename-aside, TOCTOU
placeholder reservation, cross-volume safe-replace, per-file `sync_data` + end-of-op flush, settle-guard RAII), the
FDA/TCC gating, the IPC capability scoping, the secret handling, and the redaction pipeline all hold up under scrutiny
and match their docs. Lens C (error handling) and lens E (macOS platform) produced **no fileable defects** — the panic
surface is guarded, the banned-macro / error-string-match bans are clean, and the FDA gate is airtight. The real
findings cluster on the **volume-aware write paths** (which skip the local path's validation + per-file transaction
tracking) and a few **panic-safety / cleanup-ordering** gaps.

## Findings table

| #   | Severity | Lens | File                                                      | Title                                                                                                                      |
| --- | -------- | ---- | --------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| 1   | **high** | A    | `high-A-cross-volume-rollback-deletes-merged-dest.md`     | Cross-volume copy **rollback** recursively deletes a merged destination directory, destroying pre-existing dest-only files |
| 2   | medium   | A    | `medium-A-volume-copy-no-dest-inside-source-guard.md`     | No "destination inside source" guard on the volume copy path — folder-into-its-own-descendant can recurse unboundedly      |
| 3   | medium   | B    | `medium-B-indexing-mutex-held-across-blocking-drain.md`   | `INDEXING` global mutex held across an up-to-5 s blocking shutdown drain (stalls status/verification)                      |
| 4   | low      | A    | `low-A-cross-fs-move-source-delete-before-final-fsync.md` | Cross-FS local move deletes source originals before the final dest's rename-into-place is fsynced                          |
| 5   | low      | B    | `low-B-verifier-in-flight-leak-on-panic.md`               | Verifier `in_flight` set leaks a concurrency slot on a panicking verification task                                         |
| 6   | low      | B    | `low-B-create-fallback-untimed-blocking-fs.md`            | Unwrapped synchronous `std::fs` in the `create_directory`/`create_file` unknown-volume fallback                            |
| 7   | low      | G    | `low-G-status-cache-leak-on-volume-delete-panic.md`       | Write-op status-cache entries leak on a panic inside the async volume-delete branch                                        |
| 8   | low      | G    | `low-G-per-path-icon-cache-unbounded.md`                  | Per-path directory-icon cache (`path:` keys) has no per-entry eviction — latent unbounded growth                           |
| 9   | low      | F    | `low-F-mcp-logs-resource-unredacted.md`                   | MCP `cmdr://logs` resource serves the raw, unredacted log tail without the bearer token                                    |
| 10  | low      | F    | `low-F-mcp-set-setting-ungated.md`                        | MCP `set_setting` mutates any setting without the bearer token, including diagnostic opt-ins                               |

## Top 5 to fix before launch

1. **#1 (high, A) — cross-volume rollback over-deletes a merged destination.** This is the only finding that loses
   _untouched_ user data, on the operation explicitly sold as the safe "undo." Importing into an existing share/device
   folder + Rollback is a realistic sequence. Fix the rollback granularity (record per-file dests, or never recursively
   delete a pre-existing dest dir).
2. **#2 (medium, A) — missing dest-inside-source guard on volume copy.** Same-share/same-device "copy a folder into its
   own child" can fill the volume. The local path already treats this as must-reject; bring the volume path to parity.
   Cheap, the error variant already exists.
3. **#3 (medium, B) — `INDEXING` mutex held across the 5 s drain.** Violates the module's own "reads never contend on
   `INDEXING`" contract and freezes the index-status surface right when the user toggles indexing. One-line-ish fix:
   drop the guard before `mgr.shutdown()`.
4. **#7 + #5 (low, B/G) — panic-safety RAII for the volume-delete cleanup and the verifier slot.** Same class (cleanup
   runs after an `.await` that can panic). Both are quick guard additions following the existing `WriteSettledGuard`
   pattern; worth doing together since a panicking flaky-device op is exactly the "hostile case" the principles call
   out.
5. **#9 (low, F) — redact the MCP `cmdr://logs` resource.** Off-by-default and marginal surface, but a one-call fix
   (`redact_line` per line) that closes an inconsistency with every other log consumer. Do it before MCP gets promoted
   out of "developer" status.

## Areas worth a second pass

- **Volume write-path rollback/cancel races end-to-end.** Finding #1 came out of the copy path; the move path
  (`volume_move.rs`) and the concurrent `FuturesUnordered` copy path (`volume_copy.rs:832`) record paths via a different
  code site (`recorded_path`) that this pass did not fully trace for the same merged-dir granularity bug. Re-verify both
  record the right thing for a directory source merging into an existing dir.
- **MTP backend internals** (`backends/mtp.rs` + the `mtp-rs` sibling crate): partial-write/interrupted-transfer
  recovery on the device protocol layer was out of scope (sibling crate). Worth a dedicated look given USB yank is a
  first-class hostile case.
- **AI subsystem concurrency** (`ai/`): llama-server process lifecycle, stream cancellation, and download cancellation
  were only skimmed. No red flags surfaced, but not exhaustively traced.
- **api-server side** (Cloudflare Worker): error-report retention, license validation, presigned-URL exposure — only the
  client half was audited.
- **Frontend XSS surface**: confirmed CSP + no remote `fetch`/`iframe`/remote-`img`, but did not audit Svelte `{@html}`
  usage.
- **Entitlements / `Info.plist` vs code assumptions**: not opened. A focused pass on hardened-runtime entitlements vs.
  what the code assumes (especially around the in-place updater) is the one macOS-platform gap worth a follow-up.

## Intentional / documented trade-offs (considered and NOT filed)

These are deliberate, documented decisions with a stated reason — surfacing them so they're not re-flagged:

- **Overwrite is not reversible** (no backup of replaced originals) — `transfer/CLAUDE.md` § "Overwrite isn't
  reversible". Chosen to avoid an unbounded-disk-footprint footgun; safe-overwrite keeps the original intact until the
  new content is in place.
- **Delete and trash don't fsync** — `delete/CLAUDE.md`. Annoyance-class only (a non-durable delete reappears, never
  loses data).
- **Cross-volume file→file Overwrite leaves a recoverable `.cmdr-tmp-*` if finalize's rename fails after the delete** —
  deliberate; the temp holds the only complete new copy and must NOT be cleaned.
- **MTP/SMB rename TOCTOU residual window** (non-local backends can't `O_CREAT|O_EXCL`-reserve) — documented narrow
  window; local-FS dests are atomically reserved.
- **Skip-All on a volume dir-vs-dir conflict over-skips the whole subtree** — documented UX gap (over-skips, never
  over-deletes).
- **`withGlobalTauri: true` is dev-only** — verified `false` in `tauri.conf.json`; the MCP bridge plugin is
  `#[cfg(debug_assertions)]`-gated. Correct.
- **MCP bearer token gates only destructive auto-confirm + `dialog confirm`; reads/nav/search ungated** — documented at
  length in `mcp/CLAUDE.md`. Findings #9/#10 argue the _scope_ leaks more than the rationale claims, but the core token
  design (constant-time compare, fail-closed, 0o600 files) is sound.
- **AI BYOK key sent only over HTTPS or loopback HTTP** (`validate_ai_base_url`), never logged — verified solid.
- **License/activation trust comes from the Ed25519-verified payload, not the server response** — a malicious license
  server can't forge commercial status. Verified.
- **Error/crash bundles ship a redacted log tail + a fixed settings struct, never secrets** — verified; keys are never
  logged in the first place.
- **FDA gate**: `check_full_disk_access()` deliberately probes TCC files at launch _before_ the gate to register the
  bundle (single umbrella prompt); Allow-path requires a restart; Deny-path fires per-folder prompts the user opted
  into. All documented in `onboarding/CLAUDE.md` and verified airtight — every launch-time icon/TCC call site is gated.
- **Updater mutates the `.app` in place** to preserve TCC grants, escalates via `osascript`, classifies permission
  errors by `io::ErrorKind` (not strings), forwards paths as quoted argv — verified hardened.
- **Bounded caches with documented TTL/cap/refcount**: `SCAN_PREVIEW_RESULTS` (5-min TTL), `LISTING_CACHE` (ended on
  nav/unmount), snapshot store (refcounted, capped), nav history (`MAX_HISTORY_PER_TAB=100`), owner/group/font/open_with
  caches (naturally key-bounded), llama-server child (PID-tracked, killed+reaped). All verified bounded.

## Subsystems explicitly NOT covered (and why)

- **`mtp-rs` and `smb2` sibling crates** — out of scope per brief (Cmdr glue only). The volume _backends_
  (`backends/mtp.rs`, `backends/smb.rs`) glue was read; the wire protocols were not.
- **`licensing/` internals, `font_metrics/` binary I/O, `clipboard/` ObjC FFI internals, vendored
  `crates/fsevent-stream/`** — spot-checked only; no red flags but not exhaustively traced.
- **Linux-specific paths** (`volumes_linux/`, `mount_linux.rs`, inotify watch-limit handling, `keyring_linux`) — outside
  the macOS-launch focus; read at the CLAUDE.md level only.
- **`apps/analytics-dashboard/`, `apps/api-server/`, `apps/website/`** — the brief scoped this to the desktop app.
- **Running-app dynamic testing via MCP** — this was a static read-only audit; no source files were modified and the app
  was not driven. All conclusions are from reading cited `path:line` evidence.
- **Frontend display logic** (Svelte dialog/store correctness beyond resource-leak lens G) — backend-weighted per the
  brief's "smart backend, thin frontend" emphasis.

## Method note

Six parallel lens passes (one per lens-cluster) read the relevant CLAUDE.md docs first to avoid re-filing documented
trade-offs, then verified against source with `path:line` citations. The lead reviewer independently re-read the source
for findings #1, #2, #3, #6, #7, #9, and #10 before filing. No source files were modified; the only writes are the
markdown files in this directory.
