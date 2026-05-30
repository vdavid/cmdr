# Pre-launch security & reliability audit — Cmdr — 2026-05-30

Independent adversarial review of the Cmdr desktop app (`apps/desktop/`), Rust backend + Svelte frontend. Audit-only: no source files were modified. Every finding was verified by reading the cited code; the two headline findings (critical + high) were re-verified by hand against the source, not just taken from the sweep.

**Overall posture: strong.** This is a mature, heavily-documented codebase with genuinely good engineering discipline — panic-free live code, banned-pattern enforcement that holds, a pinned-key signature-verified updater, token-gated loopback-only MCP, secrets that never hit logs, and a thorough FDA/TCC gate. The findings below are the exceptions, not the rule. **One critical data-loss bug must be fixed before launch.**

## Findings

| File | Severity | Lens | Title |
|---|---|---|---|
| `critical-A-crossfs-move-skip-deletes-source.md` | critical | A | Cross-filesystem move deletes the source of a Skipped file → permanent data loss |
| `high-A-volume-copy-not-durable-before-complete.md` | high | A | Volume copy/move to a local-FS dest reports "complete" before data is fsynced |
| `medium-A-volume-autorename-toctou-clobber.md` | medium | A | Volume-side auto-rename is non-atomic; streaming write clobbers a racing file |
| `medium-A-smb-write-partial-file-no-cleanup.md` | medium | A | SMB streaming write leaves a partial file on the server on WRITE/finish error |
| `medium-A-mtp-upload-partial-file-no-cleanup.md` | medium | A | MTP upload leaves a partial/corrupt file on the device after mid-stream failure |
| `medium-C-safe-overwrite-file-swallows-restore-failure.md` | medium | C | `safe_overwrite_file` silently swallows a failed restore-of-original |
| `medium-B-get-dir-stats-sync-sqlite-on-executor.md` | medium | B | `get_dir_stats[_batch]` run synchronous SQLite reads on the async executor |
| `medium-D-capabilities-dont-gate-app-commands.md` | medium | D | Per-window capabilities don't gate app commands; viewer can reach secrets/destructive ops |
| `low-A-crossvol-move-source-cleanup-swallows-errors.md` | low | A | Cross-volume move source cleanup swallows child errors → misleading failure |
| `low-A-autorename-placeholder-leak-on-validation-error.md` | low | A | Auto-rename placeholder leaks as a stray 0-byte file on later validation error |
| `low-A-safe-overwrite-temp-not-synced-before-swap.md` | low | A | Linux overwrite deletes the original aside before new content is proven durable |
| `low-B-updater-verify-and-write-on-executor.md` | low | B | `download_update` verifies + writes the tarball synchronously on the executor |
| `low-B-listing-cache-watcher-no-ttl-eviction.md` | low | B/G | Listing cache + watchers evicted only on explicit `list_directory_end`; no TTL backstop |
| `low-C-write-lock-unwrap-bypasses-ignorepoison.md` | low | C | Live write-path lock unwraps bypass the `IgnorePoison` convention |
| `low-D-business-logic-in-command-files.md` | low | D | Business logic leaks into rename/search/write_ops command files (thin-IPC drift) |
| `low-F-ai-connection-body-preview-panic.md` | low | F/C | `check_ai_connection` byte-slices a response body → panic on multibyte boundary |
| `low-F-smbutil-stderr-may-log-credential-url.md` | low | F | SMB listing failure logs raw `smbutil` output, which can echo a credential URL |

Totals: **1 critical, 1 high, 6 medium, 9 low.**

## Top 5 to fix before launch

1. **`critical-A` — cross-FS move Skip deletes the source.** This is unambiguous permanent data loss in a common flow (move a file across filesystems where the destination name exists, click Skip). A file manager that loses data is dead (AGENTS.md principle #4). Highest priority, and the fix is well-scoped (track skipped sources, don't delete them in Phase 4).
2. **`high-A` — volume→local copy/move isn't durable before "complete."** Directly breaks the stated "complete means you can eject now" guarantee for phone/NAS-import-to-USB. One `sync_data` in `LocalPosixVolume::write_from_stream` closes it.
3. **`medium-A-smb` and `medium-A-mtp` — partial remote files left on error.** A move preserves the source, but the destination silently holds a corrupt file the user may trust. Add the same cleanup the cancel path already does.
4. **`medium-A-volume-autorename-toctou` — volume auto-rename clobbers a racing file.** Same class of bug the local path was already fixed for; close it on the volume side before launch since SMB/MTP imports into actively-synced folders are common.
5. **`medium-D` — capability boundary doesn't cover app commands.** At minimum correct the doc so nobody trusts a boundary that isn't there; ideally gate secrets/destructive commands on the `main` window label.

## Areas to revisit in a second pass

- **Move conflict × phase-ordering matrix.** The critical bug suggests a dedicated property-test sweep across {same-FS, cross-FS, volume} × {Skip, Overwrite, Rename, OverwriteSmaller/Older} × {file, dir-with-skipped-child}: assert the source survives iff it was actually moved. The cross-FS Skip case is unlikely to be the only ordering gap.
- **Volume write durability audit.** Walk every volume write-completion path (not just the one found) for an fsync/finish-confirm before `write-complete`.
- **Index DB crash consistency.** `indexing/` (writer, reconciler, event_loop — ~13k LOC) was only touched for lock-across-await and the `get_dir_stats` command. WAL torn-write behavior and reconciler correctness under a mid-write crash weren't audited.
- **Redaction corpus.** The redactor wasn't fuzzed against real subprocess output (smbutil, smbclient, git). The `low-F-smbutil` finding hints the known-pattern catalog may miss some shapes.
- **Frontend resource hygiene (lens G on the Svelte side).** Long-session store growth, listener/watcher teardown on navigation, and snapshot-store refcounting were not deeply audited — the sweep concentrated on the Rust backend.

## Subsystems NOT covered (and why)

- **`mtp-rs` and `smb2` sibling crates (internals).** Out of scope per the brief — only the Cmdr glue (`volume/backends/`, `mtp/`, `network/`) was audited. The protocol-level partial-write/abort semantics referenced in the MTP/SMB findings live in those crates and need confirming there.
- **Frontend Svelte/TS in depth.** Verified the IPC contract (no raw-invoke survivors, bindings discipline) and pulled in CLAUDE.md context, but did not audit component-level logic, virtual-scroll memory, drag/drop, or store lifecycles for leaks. Backend was the priority given "smart backend, thin frontend."
- **`file_viewer/` backends (FullLoad/ByteSeek/LineIndex).** Not audited for huge-file handling or path-traversal on the viewer's read path. Worth a look given the viewer is the lowest-trust window (see `medium-D`).
- **`git/` virtual portal, `quick_look/`, `drag_image_*`, `menu/`, `text_size.rs`, `system_strings.rs`, `clipboard/`.** macOS glue and the git virtual filesystem were not deeply audited.
- **`crash_reporter/` / `error_reporter/` redaction completeness.** Spot-checked (bundles collect only redacted logs, anonymous IDs) but the redaction pattern catalog wasn't exhaustively tested.
- **`apps/api-server/` (Cloudflare Worker), `apps/analytics-dashboard/`, `apps/website/`.** Out of scope (desktop-app focus). Note: the api-server handles licensing, telemetry, crash/error uploads, and admin endpoints — it warrants its own dedicated audit before relying on it for revenue protection and PII handling.
- **Licensing bypass depth.** Confirmed Ed25519 verification is client-side against a compiled-in public key with no committed private key. Did not deep-dive binary-patching resistance — accepted as inherent to offline client-side verification (maintainer's own revenue protection, low-sev by the brief).

## Method notes

Six parallel lens agents (A copy/move, A delete/cross-vol/MTP/SMB, B concurrency/resources, C error discipline, D IPC, E+F macOS/security) swept the Rust backend, each instructed to read the relevant CLAUDE.md first and list documented trade-offs separately so they wouldn't be filed as findings. Candidate findings were then hand-verified against source before write-up; the critical and high findings were read end-to-end in `move_op.rs` and `local_posix.rs` to confirm reachability and the absence of a guard. A number of plausible-looking issues were checked and *not* filed because a CLAUDE.md documents them as intentional (overwrite-not-reversible, delete/trash don't fsync, Skip-All-drops-volume-subtree, cross-volume file→file safe-replace, the FDA gate's deliberate-looking multi-trigger, the 7-day Discord presigned links). Those are the system working as designed.
