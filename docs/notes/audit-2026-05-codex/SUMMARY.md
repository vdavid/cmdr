# Cmdr Pre-Launch Security and Reliability Audit, May 2026

## Findings

| File                                                                                             | Severity | Lens                                                   | Title                                                                 |
| ------------------------------------------------------------------------------------------------ | -------- | ------------------------------------------------------ | --------------------------------------------------------------------- |
| [high-A-volume-create-file-clobbers-existing.md](high-A-volume-create-file-clobbers-existing.md) | high     | A — Data safety                                        | Volume create-file can clobber an existing file                       |
| [high-G-mtp-upload-buffers-entire-file.md](high-G-mtp-upload-buffers-entire-file.md)             | high     | G — Resource hygiene                                   | MTP upload buffers the entire file and bypasses cancellation progress |
| [medium-C-write-error-string-classification.md](medium-C-write-error-string-classification.md)   | medium   | C — Error handling discipline                          | Write-operation errors are classified by message text                 |
| [medium-B-secret-store-ipc-blocks.md](medium-B-secret-store-ipc-blocks.md)                       | medium   | B — Concurrency, races, and main-thread responsiveness | Secret-store IPC calls run blocking Keychain work inline              |

## Top Findings To Fix Before Launch

1. `high-A-volume-create-file-clobbers-existing.md` — this is the only finding in this pass with a direct user-data
   clobber path.
2. `high-G-mtp-upload-buffers-entire-file.md` — large MTP writes can exhaust memory and do not honor the documented
   per-chunk cancellation contract.
3. `medium-B-secret-store-ipc-blocks.md` — a blocking Keychain or Secret Service call can make credential flows appear
   hung.
4. `medium-C-write-error-string-classification.md` — error-string classification can mislead recovery UX in write
   operations.
5. No fifth finding was filed in this pass. I found other rough edges, but they were either documented trade-offs,
   test-only code, or did not clear the launch-risk bar.

## Intentional, Documented Trade-Offs Not Filed

- Production `withGlobalTauri` is disabled in `apps/desktop/src-tauri/tauri.conf.json`; `docs/security.md:15-21`
  documents that the wrapper enables it for dev/MCP and warns to gate remote-loading functionality in dev.
- The MTP/Linux USB permission string matching is documented in `apps/desktop/src-tauri/src/mtp/CLAUDE.md`; I did not
  file that as an error-string finding.
- FDA gating for protected paths and icon APIs is documented in the onboarding and volume docs and implemented around
  the inspected icon/volume call sites; I did not find an ungated launch-time `NSWorkspace.iconForFile:` path in the
  sampled code.
- Local write-operation overwrite semantics, conflict handling, and temp/backup/rename behavior are extensively
  documented in `apps/desktop/src-tauri/src/file_system/write_operations/CLAUDE.md`; I did not re-file known behavior
  that matched those docs.
- The panic-hook `eprintln!` path is explicitly justified in `apps/desktop/src-tauri/src/crash_reporter/mod.rs` for
  crash-time fallback logging, and benchmark-only `eprintln!` usage is isolated from normal app code.

## Second-Pass Areas

- Full transfer pipeline race audit: I sampled volume copy/move and noted truncating opens in backend stream writers,
  but did not fully prove or disprove every conflict-resolution time-of-check/time-of-use path.
- SMB backend semantics: the create/write behavior depends on the sibling `smb2` crate's create disposition helpers; I
  inspected Cmdr glue but did not audit that crate end to end.
- AI model download and updater networking: I did not complete a timeout/cancellation/content-validation pass over every
  remote download path.
- Frontend store lifetime audit: I focused on backend data safety and IPC; long-lived Svelte stores, listeners, and
  unmount cleanup deserve a dedicated pass.
- Capability minimization: I spot-checked the capability files and raw invoke policy, but did not produce a full
  command-by-window permission matrix.

## Subsystems Not Covered

- `apps/analytics-dashboard/`, `apps/api-server/`, and `apps/website/`: out of scope; the request targeted the desktop
  app.
- End-to-end Playwright/WebDriver test suites: I read relevant docs and selected tests by search, but did not execute or
  audit the suites comprehensively.
- Full `mtp-rs` and `smb2` sibling crates: only Cmdr integration points were sampled because the request prioritized
  `apps/desktop/src-tauri/` and `apps/desktop/src/`; deeper backend crate review would be a separate audit.
- Licensing, telemetry, crash-report upload, and error-report upload: I skimmed the security docs and redaction context,
  but did not trace every network request and server response path.
- Index database internals and search ranking: I read the architecture notes but did not inspect every indexer/search
  code path for resource growth or correctness.

## Coverage Note

I completed the requested orientation pass first: `AGENTS.md`, `docs/architecture.md`, every `CLAUDE.md` found under
`apps/desktop/`, plus `docs/style-guide.md` and `docs/security.md`. The code audit then focused on high-risk write
paths, backend volume implementations, IPC command surfaces, banned error/print patterns, capability posture, FDA-gated
macOS APIs, secret handling, and selected long-running transfer paths.
