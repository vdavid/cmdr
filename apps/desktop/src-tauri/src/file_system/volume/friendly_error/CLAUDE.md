# Friendly error CLASSIFICATION

Turns a raw OS error + path into a TYPED, word-free `ListingError` the frontend renders. The split: CLASSIFICATION in
Rust (errno → reason, provider detection, category/retry/action), WORDS on the frontend
([`src/lib/errors/`](../../../../../src/lib/errors/CLAUDE.md)). This module emits zero user-facing prose.

Parent: [`volume/CLAUDE.md`](../CLAUDE.md) (trait + manager + capability matrix). App-wide error conventions:
[`docs/guides/error-handling.md`](../../../../../../../docs/guides/error-handling.md).

## Module map

- `mod.rs`: data model (`ListingError`, `ListingErrorReason`, `ErrorCategory`, `ErrorActionKind`) + public re-exports +
  the typed-mapping tests.
- `volume_error.rs`: `VolumeError` → `ListingError` (the entry point; dispatches to `errno` for raw `IoError`s).
- `errno.rs`: raw macOS errno → `ListingError` (one reason per distinct outcome), non-macOS fallback.
- `kinds.rs`: shared `ListingError` constructors for `VolumeError` variants that map to the same conceptual reason.
- `empty_root.rs`: TCC-restricted volume-root hint (the iCloud-looks-empty special case).
- `provider.rs`: `Provider` enum (18 variants), `detect_provider`, `enrich_with_provider` (sets the typed `provider`).

## Must-knows

- **This layer emits NO prose.** `ListingErrorReason` carries a semantic reason + typed params; the FE switches on the
  reason to pick the words. Don't reintroduce `title`/`explanation`/`suggestion` strings here. Reason and `Provider`
  variant names are the IPC contract with `src/lib/errors/`; renaming one without the other breaks the FE parity test.
- **The frontend NEVER sees raw errno numbers.** Rust maps errno → semantic reason; the FE switches on the reason.
  Errnos that produce identical FE copy collapse to one reason; nothing else merges (the 1:1 mapping is what makes the
  parity check meaningful).
- **Layer 0 git pass-through must match first.** `VolumeError::FriendlyGit(...)` is matched before any errno mapping and
  rides as the `Git` reason carrying its typed `FriendlyGitErrorKind`, so git copy isn't clobbered by the generic I/O
  fallback and is never provider-enriched. Don't reorder it below the errno arms.
- **`enrich_with_provider` SETS `provider`, never overwrites prose.** Detection stays in Rust (needs path patterns +
  `statfs`); the FE overlays the provider-specific suggestion. Adding a `Provider` variant also requires updating the
  FE `provider-error-messages.ts` table AND the `volumes/CLAUDE.md` provider table.
- **`raw_detail` is plain text, never markdown** (errno name + code, or the git kind token). It's rendered verbatim in
  the technical-details disclosure, not through snarkdown.

## Adding a new error message

Recipe (Rust side; FE side is in [`src/lib/errors/CLAUDE.md`](../../../../../src/lib/errors/CLAUDE.md)): add the
`ListingErrorReason` variant (with its typed params), add the map arm in `errno.rs` / `volume_error.rs` / `kinds.rs`
choosing the `category`/`retry_hint`/`action_kind`, and add a typed-mapping test in `mod.rs`. Full recipe + the
provider-detection strategy table: [DETAILS.md](DETAILS.md).

Full details: [DETAILS.md](DETAILS.md).
