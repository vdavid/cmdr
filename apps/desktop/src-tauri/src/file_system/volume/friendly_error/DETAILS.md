# Friendly error classification details

`CLAUDE.md` holds the must-knows. This file holds the depth: the classification architecture, the wire shape, the
Rust/FE split, and the step-by-step recipes for adding reasons and providers.

## Philosophy

The user should never feel alone with a broken state. Every message should feel like the app putting its hand on the
user's shoulder: "Here's what happened, and here's what you can do." We detect which cloud provider or mount tool manages
the path so the suggestion can be tailored to that app (a timeout on a Dropbox folder gets different advice than a
timeout on an SSHFS mount). Power users still get the raw errno name and code in a collapsible technical-details section:
never hidden, never in your face.

That philosophy is split across two homes now: this module decides WHAT happened (the typed identity), and the frontend
(`src/lib/errors/`) decides the WORDS. This module owns none of the prose.

## Architecture: classification only

A message's IDENTITY (a typed reason + structured params: the backend's job, it needs the path, the errno, the `statfs`
type) is separated from its WORDS (the frontend's job). The backend ships a typed, word-free `ListingError`; the FE
factories render it.

`ListingError` (in `mod.rs`) carries: `category` (Transient / NeedsAction / Serious, drives styling), `reason` (a
serde-tagged `ListingErrorReason` with variant-carried params, so impossible param combinations are unrepresentable),
`provider` (optional detected provider), `action_kind` (optional, drives the "Open System Settings" button),
`retry_hint` (drives the "Try again" button), and `raw_detail` (plain text for the disclosure).

Sources that produce a `ListingError`:

- **Layer 0: typed git pass-through.** `VolumeError::FriendlyGit(FriendlyGitError)` is a dedicated variant the git
  module's volume hooks (`try_route_listing`, `try_route_metadata`, `try_open_blob_stream`) return on a git-shaped
  failure. `listing_error_from_volume_error` matches it FIRST and ships the carried `FriendlyGitErrorKind` as the `Git`
  reason (category from `kind.category()`, retry only when transient), with no errno mapping and no provider enrichment.
  Keeps git copy from being clobbered by the generic I/O fallback; end-to-end type-checked, no string parsing. The FE
  routes the `Git` reason to its parallel git factory (`git-error-messages.ts`).
- **`listing_error_from_volume_error(err, path)`** (`volume_error.rs`): the entry point. Maps typed `VolumeError`
  variants through the shared `kinds::*` constructors, and dispatches `IoError { raw_os_error: Some(_) }` to
  `errno::listing_error_from_errno`. The permission-denied arm branches on `tcc_paths::is_potentially_tcc_restricted`
  (or `is_network_volume_path`) to choose `TccRestricted` (two escape hatches) vs the generic `PermissionDenied`.
- **`errno::listing_error_from_errno`** (`errno.rs`): macOS errno → reason, one reason per distinct outcome (errnos with
  identical FE copy collapse to one reason). Non-macOS falls back to `CouldntReadUnknown`.
- **`enrich_with_provider(error, path)`** (`provider.rs`): detects 18 cloud/mount providers from path patterns +
  `statfs` filesystem type and SETS `provider`. The FE overlays the provider-specific suggestion when `provider` is
  present (reproducing the old override exactly), keyed by `(provider, category)`.
- **Empty-root path: `listing_error_for_restricted_empty_root(volume_id, path)`** (`empty_root.rs`): for an OS-returned
  successful empty listing at a volume root commonly hidden by macOS TCC (currently iCloud Drive without Full Disk
  Access). The streaming listing path (`file_system/listing/streaming.rs`) checks this after a successful empty read at
  the volume root and emits `listing-error` with the `EmptyRootICloud` reason instead of `listing-complete`. Returns
  `None` for any other volume / non-root path so genuine empty directories don't warn.

The write path is separate: `write-error` events ship only the typed `WriteOperationError`, and the FE renders its copy
and classification (`transfer-error-messages.ts`). There is no `friendly_error` involvement on the write path.

## The Rust/FE split (what lives where)

- **Rust keeps**: errno → reason mapping, the `kinds.rs` constructors, the TCC-vs-permission branch, category / retry /
  `action_kind` assignment, `enrich_with_provider` detection, the Layer-0 git pass-through and its ordering, and the
  `raw_detail` technical string.
- **FE gains** (`src/lib/errors/`): all titles / explanations / suggestions, the provider-suggestion table, provider
  display / app names, the reason / git / provider message factories, the markdown escaper (the XSS boundary), and the
  `system_strings` pane-name interpolation. See [`src/lib/errors/CLAUDE.md`](../../../../../src/lib/errors/CLAUDE.md)
  and its `DETAILS.md`.

## Adding a new error message

Rust side (the FE side is in [`src/lib/errors/DETAILS.md`](../../../../../src/lib/errors/DETAILS.md)):

1. Add a `ListingErrorReason` variant in `mod.rs` with its typed params (model params as variant fields). Keep the
   variant name in lockstep with the TS `ListingErrorReason` union member.
2. Add the map arm: a new errno arm in `errno.rs`, a new `VolumeError` arm in `volume_error.rs`, or a new `kinds::*`
   constructor if several variants share the reason.
3. Pick the `ErrorCategory` (Transient = retry might work, NeedsAction = user must act, Serious = genuinely broken),
   `retry_hint`, and `action_kind`.
4. Add a typed-mapping test in `mod.rs::tests` asserting the reason, category, retry, action, and populated params.

Then add the FE factory case and confirm the style + parity tests cover it.

## Adding a new provider

1. Add a variant to the `Provider` enum in `provider.rs`.
2. Add path detection in `detect_provider` (CloudStorage prefix, a specific path, or a `statfs` type).
3. Add the detection unit test in `provider.rs::tests`.
4. Add the provider's `(provider, category)` suggestions + display / app names to the FE
   `provider-error-messages.ts`, and add it to the FE parity + style test matrices.
5. Update the `volumes/CLAUDE.md` provider table to keep the two lists in sync.

## Provider detection strategies

- **`~/Library/CloudStorage/<Prefix>*`**: Dropbox, GoogleDrive, OneDrive, Box, pCloud, Nextcloud, SynologyDrive,
  Tresorit, ProtonDrive, Sync, Egnyte, MacDroid, plus a generic fallback for unrecognized providers.
- **`~/Library/Mobile Documents/`**: iCloud Drive.
- **`/Volumes/pCloudDrive`**: pCloud (FUSE virtual drive).
- **`/Volumes/veracrypt*`**: VeraCrypt.
- **`~/.CMVolumes/`**: CloudMounter.
- **`statfs` `f_fstypename` (macOS)**: macFUSE / SSHFS / Cryptomator / rclone (`macfuse`, `osxfuse`), pCloud
  (`pcloudfs`).

The `statfs` check runs only at error time (not on every listing), so the syscall cost is negligible.

## Key decisions

- **Classification in Rust, words on the FE.** Classification needs the full path (for provider detection) and
  platform-specific errno codes; keeping it in Rust keeps the FE from duplicating errno knowledge. The words live on the
  FE so all displayable text has a single home (the "smart backend, thin frontend" principle, and the seed for an i18n
  catalog). The behavior-preservation net for the move is the frozen FE golden + parity test; do not regenerate it.
- **Two conceptual layers (reason, then provider enrichment).** Not every provider+reason combination needs custom copy;
  the base reason message is always useful and provider enrichment is additive. Keeping them separate avoids a
  combinatorial explosion.
- **Git keeps its own typed enum** (`FriendlyGitErrorKind`) rather than folding into `ListingErrorReason`: git copy is
  git-domain and already cleanly typed, and the Layer-0 pass-through stays a distinct path.
