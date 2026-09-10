# check-a11y-contrast

Design-time contrast checker for the Cmdr desktop app: WCAG 2.2 AA (the primary gate) plus an enforced APCA Lc-45
perceptual floor.

## Why

Our E2E axe-core tests flake on `color-contrast` rules because axe + webkit2gtk + chained `color-mix(var(...))`
sometimes disagree on the effective pixel color. The design tokens themselves are deterministic, though, so we can
verify contrast at build time without a browser.

This tool is tier 1 of a three-tier a11y strategy:

1. **tier 1 (this tool)**: Static analysis of design tokens + scoped CSS. Millisecond runtime, no browser.
2. **tier 2**: Visual regression snapshots (future).
3. **tier 3**: E2E axe-core for structural a11y (ARIA, focus, labels).

## Run

```bash
# Direct
go run ./scripts/check-a11y-contrast

# Via check runner
pnpm check a11y-contrast

# Verbose (warnings from unresolvable values + the full APCA advisory detail)
go run ./scripts/check-a11y-contrast -- --verbose
```

Exit code 0 on clean, 1 on any violation (a WCAG pair below threshold OR an APCA pair below the Lc-45 floor). Unmodeled
`opacity` dimming (see "Opacity (detect, don't compute)" below) is a third, advisory category: always printed, never
part of the exit code.

## What it checks

For every element selector in every `.svelte` `<style>` block (and global rules in `app.css`), when the selector sets
both a text color and a background, the tool:

1. Resolves `var(--foo)` chains (including fallbacks `var(--x, #fff)`).
2. Evaluates `color-mix(in srgb, ...)` and `color-mix(in oklch, ...)` exactly.
3. Composites translucent colors over `--color-bg-primary`.
4. Computes WCAG 2.2 contrast ratio for light and dark mode separately.
5. Flags pairs below 4.5:1 (normal text) or 3:1 (large text: ≥24px, or ≥18.66px with weight ≥700).
6. **Accent matrix**: for any pair whose resolution depends on `--color-accent` (directly or via a derived token like
   `--color-accent-hover`, `--color-accent-text`, `--color-age-*`, `--color-size-*`), re-evaluates under every runtime
   accent variant `accent-color.ts` can produce (the 8 macOS system accents + Cmdr gold), and reports the worst case.
   The variant name is shown in the violation line as `accent=<name>`. Without this pass, the check would only validate
   the static `app.css` fallback (`#d4a006`) and silently miss issues that surface when the user's macOS accent is Apple
   Blue, Purple, Graphite, etc. See `accent_matrix.go` for the variant list.

The rule walker pairs `color` and `background`, full stop: it has no notion of CSS `opacity`, so a translucent text
color composited via `opacity` is invisible to steps 1-6 above. A separate pass, `AnalyzeOpacity`, flags any such rule
it can't otherwise account for — see "Opacity (detect, don't compute)" below.

In addition to the rule walker, four scenario synthesizers cover cases where the text color and the background are set
on different selectors — cases the walker can't pair on its own:

- **Row state matrix** (`row_state_matrix.go`): the file-list's selected-row text colors (`--color-selection-fg` and the
  `--color-size-*-selected` mixes, on `.is-selected` descendants) and its unselected-row text colors (today:
  `--color-text-quiet`, shared by the hidden-entry dim and the TCC-restricted row treatment) are both set on a
  descendant selector while the row bg is set elsewhere (the pane, the stripe rule, or the cursor rules). The
  synthesizer composites the bg the row will actually render with — pane tint × stripe × cursor state × accent variant —
  and pairs each text role in `rowRoleGroups` against it. About 1000 pairs evaluated; worst-case-per-(role, mode, tint,
  variant) reported. Mirrors the three-tier selection-fg cascade in `app.css` (primary → cursor → fallback) for selected
  roles, so the synthesizer's verdict matches what the runtime actually paints.
- **Dropdown ancestor-bg matrix** (`dropdown_states.go`): hand-listed `(descendant-text-var, ancestor-bg-var)` tuples
  for cases where bg comes from an ancestor selector and `[data-highlighted]` (the dropdown options today; designed for
  easy extension). Runs every entry against the accent matrix + both modes. Each tuple supports a `FgExpr` / `BgExpr`
  escape hatch (an arbitrary CSS expression instead of a single var) so a pair can model an `opacity` composite (via a
  `color-mix(..., transparent N%)` term) or a tint-over-surface bg.
- **Query dialog states** (`query_dialog_states.go`): the Search / Select dialogs' fg-on-bg pairs that span selectors or
  fold through `opacity` and the rule walker can't see: the `ToggleGroup` "AI" badge + shortcut hint, the under-cursor
  result row's muted columns on the accent-tinted cursor bg, and the footer shortcut hints (on the dialog surface and on
  the primary button). Reuses the `dropdown_states.go` scenario type and accent-matrix sweep.
- **Toast states** (`toast_states.go`): the toast frame's text roles (the message on `--color-text-primary`, the age
  label on `--color-text-tertiary`) on each level's tinted surface, which `ToastItem.svelte` sets on `.toast.<level>`
  while the text colors live on their own selectors. Same scenario type and sweep.

## Opacity (detect, don't compute)

The rule walker pairs a `color` and a `background` declared on the SAME selector; it has no notion of CSS `opacity` at
all, so a rule like `.foo { color: var(--color-text-primary); opacity: 0.6; }` composites a translucent glyph against
whatever is behind it at runtime, and the walker never evaluates the result. This is a real hole, not a theoretical one:
a hidden/restricted file-list row composited to about Lc 44 in dark mode (under the enforced APCA floor) via exactly
this pattern, and nothing caught it for as long as it shipped (fixed in `4cdc00c0a`, converted to the
`--color-text-quiet` token).

Rather than resolve the CSS cascade to compute what an `opacity` rule actually composites against (which needs a
browser, the thing tier 1 exists to avoid), `opacity_check.go` detects the gap instead: `AnalyzeOpacity` reports any
rule with a static `opacity: N < 1` UNLESS it's one of:

- **A disabled or inactive UI component** (`opacityInactiveSelector` in `opacity_check.go`): `:disabled`, `[disabled]`,
  `aria-disabled`, `data-disabled`, `data-gated`, or a `.disabled` / `.is-disabled*` / `*-disabled` class. WCAG 1.4.3
  explicitly exempts inactive components, and this is where most of the codebase's `opacity` dimming lives.
- **A hand-verified non-text element** (`opacityDecorativeAllowlist`): an `<Icon>` wrapper, an empty CSS-shape status
  indicator (a colored dot/bar/swatch with no child content), or an aria-hidden punctuation divider with no
  informational content. None of these render a text glyph, so this checker's text-contrast scope doesn't apply. Each
  entry was verified by reading the component's markup, not guessed from the class name — see "Add a new opacity
  exemption" below.
- **Already hand-modeled** by a scenario synthesizer (`opacityModeledElsewhere`), so it isn't double-reported.
- **`opacity: 0`**: renders nothing, so there's no visible glyph to have a contrast ratio against anything (the CSS
  idiom for "hidden until `:hover`/`:focus`/a state class reveals it," not a dimmed-but-readable case). The revealed
  state is a separate rule, evaluated on its own merits.

A reported finding isn't a computed WCAG/APCA verdict like `Finding` (there's no FG/BG/ratio to show): it's a structural
"the walker can't see this" flag. The fix is one of the two the message states: express the dimming as a color token (so
the walker's normal color/background pairing sees it) or hand-model the composited pair in a synthesizer. Converting a
survivor to a color token is what promotes it into the set the WCAG/APCA gate actually verifies — it doesn't just quiet
this check, it makes the pair provably fine (or catches it if it isn't).

**Advisory, not a hard gate** — unlike the WCAG violations and the APCA Lc-45 floor, which stay hard failures.
`AnalyzeOpacity` reports what the walker _can't verify_, not a computed failure: a reported rule might be a real
contrast bug (the file-list row above genuinely was) or might be perfectly fine once you look at what's actually behind
it — the tool has no way to tell the two apart without a browser. Blocking every build on that would be
indistinguishable from blocking on noise, so opacity findings never fail the exit code (for a direct `go run` or through
the check runner). They're always printed, though — every run, in full, with no allowlist or baseline that can quiet an
entry — so they stay visible until someone looks and either converts the rule or hand-models it. See
`scripts/check/checks/desktop-svelte-a11y-contrast.go` for how the check runner surfaces this as a `warn` (yellow, not
red; doesn't fail `pnpm check`) via a `CMDR_A11Y_OPACITY_STATUS_FILE` side channel — `go run` collapses any non-zero
exit to 1, so the exit code alone can't tell "clean" apart from "clean of hard failures, but opacity findings remain."

Only literal numeric `opacity` values resolve (`parseOpacity` in `parser.go`); `var(...)`, `calc(...)`, and other
non-literal values are left unparsed rather than guessed (none exist in the codebase today).

**File coverage**: this pass also walks every `.css` file under `apps/desktop/src` (not just `.svelte` components and
`app.css`), since a global stylesheet like `app-field.css`, `app-file-list.css`, or a component-local
`filter-popover.css` is imported directly by `+layout.svelte` rather than scoped to a component and the WCAG rule walker
below doesn't reach it. The WCAG/APCA gate's own file scope is unchanged; only the opacity check sees these extra files.

The parser also normalizes `app.css` before extracting vars: every `@supports not (color: color-mix(...))` block is
stripped (those carry old-WebKit hex fallbacks that would otherwise overwrite the modern `color-mix` formulas), and
**every** `@media (prefers-color-scheme: dark)` block is descended into rather than only the first (the codebase has
more than one — for example a small block carrying the selection-fg fallback rule sits before the main dark token
table).

## APCA floor (perceptual second opinion)

On top of the WCAG gate, the tool runs every evaluated pair through APCA (the Accessible Perceptual Contrast Algorithm,
the method explored for WCAG 3), using the canonical `apca-w3` 0.1.9 (W3) constants. APCA predicts real readability
better than WCAG 2: it's polarity-aware (dark-on-light and light-on-dark differ, which WCAG 2 treats as identical) and
accounts for font size and weight. Output is `Lc` ("lightness contrast"), signed, |Lc| ~0..108. See `apca.go`.

- **Enforced**: any pair below **APCA Lc 45** (APCA's "absolute minimum for any text") fails the check, alongside WCAG.
- **Advisory** (printed only with `-verbose`): the full `|Lc|` distribution, a "blast radius if bar X were the gate"
  table, a zoom sweep (zoom relaxes the size-based target; it does not change `Lc`), and every pair below its
  font-size/weight-aware target. Non-verbose runs print a one-line summary plus the floor verdict.
- **Why a floor, not the full APCA ladder**: APCA's preferred body level (Lc 75–90) would flag most of the app's
  14px/400 text — that 45–60 band is design intent (de-emphasized placeholders, hints, disabled), not a bug. We gate
  only the hard floor and keep the muted band advisory.
- **Status** (verified 2026-06-30): APCA was removed from the WCAG 3 draft (2023) and now develops independently (ARC);
  its core math has been stable since 2022, which is why we enforce one conservative bar and keep the rest advisory.
  WCAG 2.2 AA stays the primary, legally-recognized gate.

## Output

```
apps/desktop/src/lib/ui/Button.svelte:97  .btn-danger:hover:not(:disabled)  mode=light  fg=#d32f2f  bg=#fbeaea  ratio=4.28  need=4.5  delta=-0.22
apps/desktop/src/lib/ui/Button.svelte:68  .btn-primary                      mode=light  accent=purple  fg=#1a1a1a  bg=#a54fa7  ratio=3.54  need=4.5  delta=-0.96
```

Each line is: `file:line`, selector, mode, optional `accent=<name>` (set when the worst case came from the accent
matrix), resolved fg hex, resolved bg hex, actual ratio, required threshold, and the delta.

## Scope and limitations

- **Translucent backgrounds**: composited over `--color-bg-primary`. If the real runtime ancestor is something else, the
  reported bg is wrong by a known-small amount. For most color-mix-with-transparent cases the ancestor IS the page
  background, so this works.
- **OKLCH mixing**: implemented (via OKLab round-trip). The sibling spaces `hsl`, `oklab`, `lch`, `lab`, `xyz` are
  approximated as OKLCH with a warning.
- **Cascade inheritance**: each unique compound-class set is a distinct state. `.foo.bar.baz` inherits from all subset
  entries (`.foo`, `.foo.bar`, `.foo.baz`, `.bar.baz`, etc.) in source order, but ONLY their direct contributions.
  sibling compound rules don't leak each other's inherited defaults.
- **Pseudo-elements**: `::placeholder` is handled. `::before`, `::after`, `::selection` are skipped.
- **Pseudo-classes** (`:hover`, `:focus`, `:not(...)`) share state with the base class. Hover is a transient state, not
  a parallel configuration.
- **`currentColor` / `inherit` / `unset` / `initial` / `revert`**: skipped with a warning (no fixed value to check).
- **Specificity across files**: not modeled. We trust source order within one `<style>` block. Global rules in `app.css`
  are evaluated separately.
- **Modes**: rules inside `@media (prefers-color-scheme: dark)` are tagged so they only contribute in dark-mode
  evaluation.

## Architecture

```
main.go              Entry, walks `apps/desktop/src/**/*.svelte`, orchestrates
                     the rule walker + scenario synthesizers.
parser.go            Parses app.css (light + dark var tables) and Svelte <style>
                     blocks into Rule structs. Strips `@supports not (...)` and
                     descends into every `@media (prefers-color-scheme: dark)`.
resolver.go          Resolves a CSS value string (literal / var() / color-mix())
                     to RGBA per mode. Tracks visited var names in `Deps` so
                     callers can decide whether to sweep the accent matrix.
contrast.go          sRGB parsing, hex/rgb()/named literals, sRGB + OKLCH mixing
                     (premultiplied alpha), WCAG 2.2 contrast math.
analyzer.go          Walks parsed rules per mode, tracks cascade state by
                     compound class set, emits Finding per (selector, mode) pair.
                     Uses `evaluateAt` to run worst-case across accent variants
                     when the pair is accent-sensitive.
reporter.go          Pretty prints WCAG violations and optional warnings.
apca.go              APCA 0.1.9 (W3) Lc math, the font-size/weight target
                     ladder, the enforced Lc-45 floor, and the advisory report.
accent_matrix.go     Runtime accent variants (the 8 macOS system accents +
                     Cmdr gold) and the per-variant VarTable override.
size_tiers.go        `.size-*` utility classes × known container bgs, since
                     they're only `color:` rules with no paired bg.
row_state_matrix.go  Selected-row text × (pane tint × stripe × cursor state ×
                     accent variant) bg composition. Models the three-tier
                     `--color-selection-fg` cascade.
dropdown_states.go   Hand-listed (descendant-text-var, ancestor-bg-var) tuples
                     for "secondary text on accent-bg" cases. Supports FgExpr /
                     BgExpr for opacity composites and tint-over-surface bgs.
query_dialog_states.go  Search / Select dialog pairs the walker can't pair:
                     ToggleGroup badge + hint, under-cursor result row, footer
                     shortcut hints. Reuses the dropdown_states scenario type.
toast_states.go      Toast frame text (message, age label) on every
                     level-tinted toast surface. Same scenario type.
opacity_check.go     Detects (doesn't compute) a static `opacity: N < 1` the
                     rule walker can't fold into its color/background
                     pairing. Exempts disabled/inactive components, a
                     hand-verified non-text allowlist, and anything already
                     hand-modeled; reports everything else. See "Opacity
                     (detect, don't compute)" above.
```

Tests:

- `contrast_test.go`: WCAG math, color-mix, OKLCH round-trip, compositing.
- `resolver_test.go`: var() fallbacks, nested color-mix, dark overrides.
- `parser_test.go`: app.css + Svelte parsing, selector extraction.
- `analyzer_test.go`: cascade inheritance, known false-positive cases.
- `accent_matrix_test.go`: variant sweep + per-variant resolution.
- `apca_test.go`: APCA reference values (black-on-white ≈ Lc 106, white-on-black ≈ −108), polarity asymmetry, target
  ladder.
- `opacity_check_test.go`: disabled/inactive exemption, decorative allowlist exemption, `opacity: 0` and `opacity: 1`
  are non-findings, a plain-text opacity dim is reported, a re-declared selector (for example a `prefers-reduced-motion`
  override) dedupes to one finding, and `parseOpacity`'s literal-only contract.

Diagnostic helpers (skipped by default; gated on env vars):

- `before_after_test.go` (`CMDR_PRINT_BEFORE_AFTER=1`): prints a curated before/after comparison table for the
  selected-row text-role × bg matrix. Useful when iterating on selection colors to see how a change affects every
  meaningful combination.
- `diff_test.go` (`CMDR_PRINT_SELECTION_DIFF=1`, `CMDR_PRINT_RED_CANDIDATES=1`): prints the contrast ratio between the
  selected-row text color and the unselected-row text color across modes + accents (differentiation, not the AA-vs-bg
  question), plus a sweep of candidate red hexes against the worst- case bgs for picking a new selection color.

## Extending

### Add support for a new CSS color function

Edit `resolver.go` → `Resolver.Resolve`. Add a prefix check (for example `rgb-to-hsl(...)`), then implement the
evaluator.

### Add a new CSS named color

Edit `namedColors` map in `contrast.go`.

### Tune thresholds

WCAG AA (current) uses 4.5:1 / 3:1. For AAA, change the constants in `analyzer.evaluate` (7:1 / 4.5:1). The enforced
APCA floor is `apcaFloor` in `apca.go` (Lc 45); the advisory size/weight target ladder is `apcaTiers` there.

### Add an allowlist for intentional violations

Not built yet. If the team decides certain findings are acceptable (marketing badges, subtle hover tints), add a JSON
allowlist alongside this README and filter in `main.go` before calling `Report`.

### Add a new "text from one selector, bg from another" scenario

When the rule walker can't pair a text role with its actual bg (the bg lives on an ancestor or sibling selector that the
walker doesn't traverse), reach for one of the synthesizers:

- For file-list selected-row text colors, add a new entry to `rowSelectedTextRoles` in `row_state_matrix.go`.
- For "secondary text on accent-bg" cases like dropdown options, append a new entry to `dropdownScenarios` in
  `dropdown_states.go`.
- For a Search / Select dialog pair, append a new entry to `queryDialogScenarios` in `query_dialog_states.go`.
- To model an `opacity: N` on the text, set `FgExpr` to `color-mix(in srgb, var(--token), transparent (1-N)%)`; to model
  a bg that's a tint composited over a surface, set `BgExpr` to the equivalent opaque `color-mix(...)`. The synthesizer
  composites translucent fg/bg the same way the rule walker does.
- For a one-off pair (specific descendant + specific ancestor bg) that's not in any bucket, mirror the
  `dropdown_states.go` pattern in a new `<thing>_states.go` and wire it into `main.go` alongside the other
  `Analyzer.AnalyzeXxx` calls.

If the scenario depends on the active accent, you don't need to do anything special — the synthesizers iterate
`AccentVariants` automatically.

If the pair composites through an `opacity: N` on the fg or bg side (rather than only `color-mix(...)`), mirror it in
the scenario's `FgExpr` / `BgExpr` with a `color-mix(in srgb, var(--token), transparent (1-N)%)` term, then add the same
`(file-suffix, selector)` pair to `opacityModeledElsewhere` in `opacity_check.go` so the opacity check doesn't also
report what the synthesizer now verifies.

### Add a new opacity exemption

When `AnalyzeOpacity` (`opacity_check.go`) reports a rule that's genuinely out of scope:

- **Disabled/inactive component** (a new attribute or class shape beyond `:disabled` / `[data-disabled]` /
  `aria-disabled` / `data-gated` / `.disabled` / `.is-disabled*` / `*-disabled`): extend `opacityInactiveSelector`'s
  pattern list. This is a general rule, not a per-component allowlist — prefer widening the pattern over adding a
  one-off entry.
- **Non-text/decorative element**: add an entry to `opacityDecorativeAllowlist`, but only after reading the component's
  markup (never infer from the selector or class name alone). The bar is: does this selector's element render an
  `<Icon>`, an empty CSS-shape indicator with no child content, or an aria-hidden punctuation divider with no
  informational content? If you can't tell, don't add it — leave the rule reported instead; a false exemption hides a
  real contrast bug the way `opacity` itself did before this check existed.
- **Already hand-modeled**: see the synthesizer note just above.

Never widen an exemption just to make a survivor go quiet. A real informational-text case that opacity-dims without
being disabled or decorative is a finding: either convert it to a color token (see `--color-text-quiet`) or hand-model
it in a synthesizer.

## Known trade-offs

- No support for `light-dark()`. If Cmdr adopts that token pattern later, add parsing in resolver.go.
- Alpha compositing uses straight RGB (spec-correct for opaque-on-opaque). For chained translucent layers (transparent
  on transparent on solid), we composite once against `--color-bg-primary`. Sufficient for current patterns.
