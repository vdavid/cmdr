# Pre-launch audit — 2026-05-28

Independent adversarial review of Cmdr (Tauri 2 / Rust / Svelte 5 file manager). Audit-only; no source modified.
Findings filed individually in this directory.

## Findings table

| Sev    | Lens | File                                                   | Title                                                           |
| ------ | ---- | ------------------------------------------------------ | --------------------------------------------------------------- |
| high   | F    | `high-F-updater-applescript-injection.md`              | Updater `osascript` builds AppleScript via path interpolation   |
| medium | C    | `medium-C-string-match-permission-error.md`            | Updater string-matches localizable English error text           |
| medium | C    | `medium-C-keychain-string-match.md`                    | Keychain classifier substring-matches localizable text          |
| medium | A    | `medium-A-dest-inside-source-canonicalize-fallback.md` | `validate_destination_not_inside_source` falls back to raw path |
| medium | A    | `medium-A-copy-misses-dangling-symlink-dest.md`        | Regular-file copy misses dangling symlink at dest               |
| medium | D    | `medium-D-viewer-window-has-store-access.md`           | Viewer capability grants full `store:default`                   |
| low    | A    | `low-A-plain-file-secrets-non-atomic-write.md`         | `PlainFileStore` non-atomic write; crash drops all secrets      |
| low    | A    | `low-A-find-unique-name-toctou.md`                     | `find_unique_name` TOCTOU race with concurrent writers          |
| low    | A    | `low-A-conflict-dialog-misreports-type-mismatch.md`    | Conflict dialog hides source/dest type on type-mismatch         |
| low    | A    | `low-A-no-fsync-on-copy-completion.md`                 | No per-file fsync; "complete" precedes durability               |
| low    | F    | `low-F-mcp-origin-bypass-when-header-missing.md`       | MCP allows requests without Origin header                       |
| low    | B    | `low-B-mutex-unwrap-poison-panics-in-volume-copy.md`   | Volume-copy bypasses `IgnorePoison` for 19 mutexes              |

12 findings: 1 high, 5 medium, 6 low. No critical.

## Top 5 to fix before launch

1. **`high-F-updater-applescript-injection.md`** — The only pre-auth-to-root path in the codebase. A user moving Cmdr
   into a folder with `'` in the name turns the next update into local privilege escalation. Cheap to fix (use
   `quoted form of` + arg passing); high asymmetry between attack cost and damage.
2. **`medium-C-string-match-permission-error.md`** — Updates silently fail to escalate on non-English macOS. Users see
   "Couldn't copy …" with no admin prompt. Also defense-in-depth for finding #1.
3. **`medium-A-copy-misses-dangling-symlink-dest.md`** — Silent clobber of files via symlink indirection in the
   regular-file copy path. Cheap fix; the parallel symlink branch already does the right thing.
4. **`medium-D-viewer-window-has-store-access.md`** — Viewer windows can read `license.json`, `secrets.json` (dev/Linux
   fallback), and override any setting. Violates the documented capability split. Drop one permission line.
5. **`medium-C-keychain-string-match.md`** — On localized macOS, Keychain misses NotFound and prompts the user for
   credentials they already saved. Same shape as #2; both should be fixed together with a typed-error pass.

## Areas to revisit on a second pass

- **SMB / MTP volume backends.** Both have substantial inline test code in the audited files, but I didn't audit the
  backend implementations (`backends/smb.rs` is ~4700 lines, `mtp/` is its own subsystem). The streaming-read patterns
  are documented carefully; the cancellation propagation via `CancelToken` looks designed-for; but the write-path edge
  cases (mid-stream connection drop, partial uploads, `attempt_reconnect` single-flight race) deserve their own pass.
- **Indexing subsystem (`indexing/`).** SQLite DB with custom collation, jwalk, FSEvents. Touched by every Tauri command
  via the `LISTING_CACHE` oracle path. Mutation-test score is reportedly good per `write_operations/CLAUDE.md`; I didn't
  independently verify.
- **Crash reporter signal handler.** `crash_reporter/CLAUDE.md` notes the async-signal-safety constraints and the gap
  around ASLR base addresses. The constraints are real and the docs are honest; verifying the signal handler's actual
  code paths needs a careful read I didn't do.
- **Drag-and-drop swizzle.** `drag_image_detection.rs` and `drag_image_swap.rs` use Objective-C method swizzling. Both
  swizzles need a read for state leaks across NSDocumentController instances and re-entrancy with the system drag.
- **Auto-updater frontend half.** `lib/updates/`. The download/install flow's UI is supposed to prevent the user from
  confusingly retrying mid-install; I only audited the backend.
- **API server (`apps/api-server/`).** Cloudflare Worker that mediates licensing, telemetry, crash + error report
  uploads. Out of scope for this pass; deserves its own audit, especially the R2 presigned-URL TTL and the Paddle
  webhook validation.

## Subsystems explicitly NOT covered (and why)

- **Analytics dashboard** (`apps/analytics-dashboard/`): private deployment, not user-facing.
- **Website** (`apps/website/`): marketing site; no code path touches the desktop app's data.
- **AI provider request bodies / streaming SSE parsing** (`ai/client.rs`, `ai/suggestions.rs`): noted the structure is
  solid (uses `genai`, has wiremock + axum SSE tests, dedicated streaming-cancel registry) and moved on. The
  cancellation contract is documented and tested; the per-provider quirks are surfaced in CLAUDE.md.
- **Indexing scanner internals** (`indexing/scanner.rs`, `indexing/reconciler.rs`): saw counts of `unwrap()` (5 each);
  same `IgnorePoison` pattern as the `low-B` finding likely applies, but the subsystem is not on the data-write hot
  path.
- **Tests directories everywhere**: out of scope.

## Intentional, documented constraints (not findings)

Patterns that initially looked wrong but turn out to be documented trade-offs in the relevant `CLAUDE.md`:

- `block_on` / sync-trait → async-trait migration leftovers and their `Pin<Box<dyn Future>>` shapes in the `Volume`
  trait. Discussed at length in `file_system/volume/CLAUDE.md` § "Decision: `Volume` trait is async."
- Detached `std::thread::spawn` for background cleanup (`remove_file_in_background`, `remove_dir_all_in_background`).
  Documented as "best-effort." Comments at call sites consistently use the `.cmdr-` prefix for recoverable leftovers.
- `safe_overwrite_file` (`helpers.rs:579`) not crash-safe between steps 2 and 3 (original at `.cmdr-backup-<uuid>`, dest
  empty, temp at `.cmdr-tmp-<uuid>`). The `.cmdr-` prefix is the documented recoverability mechanism. Crash-safety here
  would require a journal; the temp+backup pattern is the documented compromise.
- MCP server defaulting to ephemeral port and binding `127.0.0.1` only. Origin validation has a hole (filed as `low-F`)
  but the bind-address choice is correct.
- `withGlobalTauri: true` in dev mode. Gated by `#[cfg(debug_assertions)]` in `lib.rs`; prod builds never see it.
  `docs/security.md` is explicit.
- File copies follow symlinks for the destination's target (the regular-file branch). The `is_same_file` check at
  `copy.rs:728` prevents the self-copy-via-symlink case. Symlink-as-dest detection is the gap (`medium-A`).
- Conditional `OverwriteSmaller` / `OverwriteOlder` reduction. The test corpus in `helpers.rs:829-1161` is exhaustive
  across edge cases. Solid.
- Verify/commit split in licensing. Activation flow correctly avoids persisting invalid keys. Crypto verification
  (Ed25519 via `ed25519-dalek`) is straight; tampering tests pin it.

## Audit methodology

Read in full: `AGENTS.md`, `docs/architecture.md`, `docs/security.md`, and the CLAUDE.md files for `commands/`,
`capabilities/`, `secrets/`, `ai/`, `network/`, `licensing/`, `mcp/`, `updater/`, `error_reporter/`, `crash_reporter/`,
the three `write_operations/*` docs, and `file_system/volume/`.

Code spot-reads: `updater/{installer.rs, signature.rs, mod.rs, manifest.rs}`,
`secrets/{mod.rs, keychain_macos.rs, plain_file.rs}`, `ai/api_keys.rs`, `licensing/verification.rs`, `mcp/server.rs`,
`file_system/write_operations/{helpers.rs, scan.rs (partial)}`,
`file_system/write_operations/transfer/{copy.rs, move_op.rs, chunked_copy.rs (partial)}`, `capabilities/*.json`,
`commands/network.rs` (excerpt), `lib.rs` (excerpt).

Greps: counts of `.unwrap()` / `.expect()` / `panic!` / `unreachable!` per file, raw `invoke(` sites outside the
bindings folder, error-string-matching patterns. Two confirmed `error-string-match` slip-throughs (findings
`medium-C-string-match-permission-error.md`, `medium-C-keychain-string-match.md`) suggest the lint has a gap worth
investigating.

Hard limit: didn't read SMB / MTP backend implementation files (`backends/smb.rs`, `mtp/*`), didn't audit the search
engine internals, didn't open the frontend Svelte components beyond capability files. Twelve findings from the scope I
did cover; an expanded pass would likely double that.
