# Friendly error system

Turns raw OS errors into warm, actionable messages (a friendly title, a plain-language explanation, and
provider-specific advice) instead of "I/O error: Operation timed out (os error 60)". Raw details (errno name, code) stay
available in a collapsible "Technical details" section.

Parent: [`volume/CLAUDE.md`](../CLAUDE.md) (trait + manager + capability matrix). Broader app-wide error conventions:
`docs/guides/error-handling.md`.

## Module map

- `mod.rs`: `FriendlyError`, `ErrorCategory`, `ErrorActionKind` data model + public re-exports.
- `errno.rs`: raw macOS errno → `FriendlyError` (37 codes), non-macOS fallback.
- `volume_error.rs`: `VolumeError` → `FriendlyError` (dispatches to `errno` for raw `IoError`s).
- `empty_root.rs`: TCC-restricted volume-root hint (single special case).
- `kinds.rs`: shared failure classification used by `volume_error`.
- `markdown.rs`: `Markdown` newtype + `md!` macro (escapes interpolated runtime strings before snarkdown).
- `provider.rs`: `Provider` enum (18 variants), `detect_provider()`, `provider_suggestion()`, `enrich_with_provider()`.

## Must-knows

- **Mapping lives in Rust, not the frontend.** The FE receives a fully-baked `FriendlyError` via `listing-error` /
  `write-error` events and renders it with category-based styling; it never sees errno codes or does OS-specific logic.
- **`explanation` / `suggestion` are typed `Markdown`, built via the `md!` macro.** Use positional `{}` args only:
  captured-identifier syntax (`md!("foo {bar}")`) bypasses the escape path and renders the literal `{bar}` in the UI.
  Positional args route through `MarkdownArg::render_arg`, which escapes plain strings (paths, OS messages, names) and
  passes `Markdown` values through unescaped. Raw OS strings contain markdown specials (`STATUS_DELETE_PENDING` would
  render with italics), so unescaped interpolation is a real bug. See DETAILS.md § Markdown escaping.
- **Layer 0 git pass-through must match first.** `VolumeError::FriendlyGit(...)` is matched before any errno mapping and
  returns its carried `FriendlyError` directly, so git-specific copy isn't clobbered by the generic I/O fallback. Don't
  reorder it below the errno arms.
- **Writing rules for messages are non-negotiable and partly test-enforced.** NEVER use "error" or "failed" (the
  `error_messages_never_contain_error_or_failed` test catches this). Active voice, contractions, no trivializing
  ("just"/"simply"), no permissive language, no em dashes, sentence case in titles, platform-native terms ("System
  Settings", "Finder", "Trash"), max two sentences per explanation. Full list + good/bad examples in DETAILS.md.
- **Keep the two provider lists in sync.** Adding a `Provider` variant also requires updating the `volumes/CLAUDE.md`
  provider table.

## Adding error messages or providers

Step-by-step recipes (new errno/`VolumeError` arm; new provider) and the provider-detection strategy table live in
DETAILS.md § Adding a new error message and § Adding a new provider.

Full details: [DETAILS.md](DETAILS.md).
