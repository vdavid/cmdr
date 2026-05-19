# check-btn-restyle

Forbid scoped `.svelte` `<style>` blocks from restyling the canonical `<Button>` classes.

## Why

`<Button>` (`apps/desktop/src/lib/ui/Button.svelte`) is the single source of truth for the app's button visuals: variant
background/text colors, hover states, focus ring, disabled opacity. Every primary button shares one accent-driven color
pair, so a contrast fix made in `Button.svelte` reaches every consumer.

When a feature component drops a `.btn-primary { color: ... }` into its scoped `<style>`, it silently re-introduces
whatever problem we just paid to solve in `Button.svelte`. The feature owner isn't usually thinking about contrast under
macOS-Purple-accent dark mode, so the regression goes unnoticed until a user reports unreadable text — exactly the
failure mode the accent matrix in `check-a11y-contrast` exists to prevent.

## What it flags

Any rule inside a `<style>` block (in any `.svelte` file other than `Button.svelte` itself) whose:

- selector references one of `.btn`, `.btn-primary`, `.btn-secondary`, `.btn-danger`, AND
- declarations include `color`, `background`, or `background-color`.

Layout-only overrides (flex, width, padding, margin, font-size, etc.) are allowed and not flagged. Re-targeting
`<Button>` via `:global(button)` for layout is also fine, since the `<button>` selector alone is not on the banned list.

Similarly-named classes like `.btn-mini`, `.btn-regular` (Button's size classes), or `.toggle-button` are NOT flagged —
only the four canonical classes above.

## Allowlist

Add `/* allowed-btn-restyle: <rationale> */` immediately before the rule (only whitespace between the comment's closing
`*/` and the selector). Empty rationales are rejected — write a real reason:

```svelte
<style>
  /* allowed-btn-restyle: drag handle inside the conflict resolver needs an inverted color pair for visibility on the
     warning-tinted background; verified at 5.2:1 under all accent variants */
  .btn-primary {
    color: var(--color-warning-text);
    background: var(--color-warning-bg);
  }
</style>
```

Allowlisted rules show up in `--verbose` output at the end of a clean run so the rationales stay visible (and reviewable
later).

## Run

```bash
# Direct
go run ./scripts/check-btn-restyle

# Via check runner
./scripts/check.sh --check btn-restyle

# Verbose (list allowlisted rules)
go run ./scripts/check-btn-restyle -- --verbose
```

Exit code 0 on clean, 1 on any violation.

## Scope and limitations

- Only top-level rules inside a `<style>` block are scanned. Rules nested inside `@media` / `@supports` blocks are
  currently NOT scanned. This is acceptable for now: dark-mode overrides of Button styling have not appeared in the
  codebase, and adding `@media` traversal is straightforward later if it becomes needed (see the test
  `TestScanDescendsIntoMediaBlocks` for the pinned behavior).
- The check trusts that Svelte's class-scoping means `.btn-primary` in a feature file only matches via global selectors
  or Button's own classes flowing through. In practice both end up clobbering the shared component.
- The check skips `Button.svelte` itself by filename. If you ever rename the canonical button file, update `scanFile`'s
  caller in `main.go`.

## See also

- `scripts/check-a11y-contrast` — the static contrast checker that catches the contrast regressions this check exists to
  prevent in the first place. Run both.
