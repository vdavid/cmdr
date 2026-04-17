# check-a11y-contrast

Design-time WCAG 2.2 contrast checker for the Cmdr desktop app.

## Why

Our E2E axe-core tests flake on `color-contrast` rules because axe + webkit2gtk + chained `color-mix(var(...))`
sometimes disagree on the effective pixel color. The design tokens themselves are deterministic, though — we can verify
contrast at build time without a browser.

This tool is tier 1 of a three-tier a11y strategy:

1. **tier 1 (this tool)**: Static analysis of design tokens + scoped CSS. Millisecond runtime, no browser.
2. **tier 2**: Visual regression snapshots (future).
3. **tier 3**: E2E axe-core for structural a11y (ARIA, focus, labels).

## Run

```bash
# Direct
go run ./scripts/check-a11y-contrast

# Via check runner
./scripts/check.sh --check a11y-contrast

# Verbose (show warnings from unresolvable values)
go run ./scripts/check-a11y-contrast -- --verbose
```

Exit code 0 on clean, 1 on any violation.

## What it checks

For every element selector in every `.svelte` `<style>` block (and global rules in `app.css`), when the selector sets
both a text color and a background, the tool:

1. Resolves `var(--foo)` chains (including fallbacks `var(--x, #fff)`).
2. Evaluates `color-mix(in srgb, ...)` and `color-mix(in oklch, ...)` exactly.
3. Composites translucent colors over `--color-bg-primary`.
4. Computes WCAG 2.2 contrast ratio for light and dark mode separately.
5. Flags pairs below 4.5:1 (normal text) or 3:1 (large text: ≥24px, or ≥18.66px with weight ≥700).

## Output

```
apps/desktop/src/lib/ui/Button.svelte:97  .btn-danger:hover:not(:disabled)  mode=light  fg=#d32f2f  bg=#fbeaea  ratio=4.28  need=4.5  delta=-0.22
```

Each line is: `file:line`, selector, mode, resolved fg hex, resolved bg hex, actual ratio, required threshold, and the
delta (how much contrast is missing).

## Scope and limitations

- **Translucent backgrounds**: composited over `--color-bg-primary`. If the real runtime ancestor is something else, the
  reported bg is wrong by a known-small amount. For most color-mix-with-transparent cases the ancestor IS the page
  background, so this works.
- **OKLCH mixing**: implemented (via OKLab round-trip). The sibling spaces `hsl`, `oklab`, `lch`, `lab`, `xyz` are
  approximated as OKLCH with a warning.
- **Cascade inheritance**: each unique compound-class set is a distinct state. `.foo.bar.baz` inherits from all subset
  entries (`.foo`, `.foo.bar`, `.foo.baz`, `.bar.baz`, etc.) in source order, but ONLY their direct contributions —
  sibling compound rules don't leak each other's inherited defaults.
- **Pseudo-elements**: `::placeholder` is handled. `::before`, `::after`, `::selection` are skipped.
- **Pseudo-classes** (`:hover`, `:focus`, `:not(...)`) share state with the base class — hover is a transient state, not
  a parallel configuration.
- **`currentColor` / `inherit` / `unset` / `initial` / `revert`**: skipped with a warning (no fixed value to check).
- **Specificity across files**: not modeled. We trust source order within one `<style>` block. Global rules in `app.css`
  are evaluated separately.
- **Modes**: rules inside `@media (prefers-color-scheme: dark)` are tagged so they only contribute in dark-mode
  evaluation.

## Architecture

```
main.go       Entry, walks `apps/desktop/src/**/*.svelte`, orchestrates.
parser.go     Parses app.css (light + dark var tables) and Svelte <style>
              blocks into Rule structs.
resolver.go   Resolves a CSS value string (literal / var() / color-mix()) to
              RGBA per mode.
contrast.go   sRGB parsing, hex/rgb()/named literals, sRGB + OKLCH mixing
              (premultiplied alpha), WCAG 2.2 contrast math.
analyzer.go   Walks parsed rules per mode, tracks cascade state by compound
              class set, emits Finding per (selector, mode) pair.
reporter.go   Pretty prints violations and optional warnings.
```

Tests:

- `contrast_test.go` — WCAG math, color-mix, OKLCH round-trip, compositing.
- `resolver_test.go` — var() fallbacks, nested color-mix, dark overrides.
- `parser_test.go` — app.css + Svelte parsing, selector extraction.
- `analyzer_test.go` — cascade inheritance, known false-positive cases.

## Extending

### Add support for a new CSS color function

Edit `resolver.go` → `Resolver.Resolve`. Add a prefix check (for example `rgb-to-hsl(...)`), then implement the
evaluator.

### Add a new CSS named color

Edit `namedColors` map in `contrast.go`.

### Tune thresholds

WCAG AA (current) uses 4.5:1 / 3:1. For AAA, change the constants in `analyzer.evaluate` (7:1 / 4.5:1).

### Add an allowlist for intentional violations

Not built yet. If the team decides certain findings are acceptable (marketing badges, subtle hover tints), add a JSON
allowlist alongside this README and filter in `main.go` before calling `Report`.

## Known trade-offs

- No support for `light-dark()` — if Cmdr adopts that token pattern later, add parsing in resolver.go.
- Alpha compositing uses straight RGB (spec-correct for opaque-on-opaque). For chained translucent layers (transparent
  on transparent on solid), we composite once against `--color-bg-primary`. Sufficient for current patterns.
