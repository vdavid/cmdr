# Cmdr design system

Design language for the Cmdr desktop app and getcmdr.com website.

## Principles

1. **The tool recedes, the content leads.** The file list is 90% of the app. Chrome (toolbars, dialogs, headers) should
   be quiet so file names, sizes, and icons can breathe. Rich data, calm surroundings.
2. **Personal, not branded.** In the app, the user's macOS accent color drives all interactive UI (selection, focus,
   buttons). The Cmdr brand (mustard yellow) lives only on marketing surfaces. The app feels like _their_ tool, not
   ours.
3. **Native, not web.** System font. Platform scrollbars. Fast transitions. No hover animations that would feel alien in
   a macOS window. The app should be indistinguishable from an AppKit app at a glance.
4. **Respect the OS.** Light/dark via `prefers-color-scheme`. Accent color via `NSColor.controlAccentColor()`. Reduced
   motion via `prefers-reduced-motion`. The user already made these choices; honor them.

## Color

### Accent strategy

**App:** Reads `NSColor.controlAccentColor()` at startup and on `NSSystemColorsDidChangeNotification`. Injected as
`--color-accent` via `document.documentElement.style.setProperty()`. Fallback: macOS default blue (`#007aff` light,
`#0a84ff` dark).

Hover and focus variants are derived, not hardcoded, because the base color is dynamic:

```css
--color-accent-hover: color-mix(in oklch, var(--color-accent), white 15%); /* light mode */
--color-accent-hover: color-mix(in oklch, var(--color-accent), white 10%); /* dark mode */
--color-accent-subtle: color-mix(in oklch, var(--color-accent), transparent 85%); /* tinted bg */
--color-accent-pop: color-mix(in oklch, var(--color-accent), black 40%); /* light: darker than accent */
--color-accent-pop: color-mix(in oklch, var(--color-accent), white 40%); /* dark: lighter than accent */
```

`--color-accent-pop` is a higher-contrast accent for small marks sitting ON accent-tinted surfaces (the symlink badge
over a gold folder icon), where plain `--color-accent` would blend in. It flips direction by scheme: darker than the
accent in light mode, lighter in dark mode.

**Accent-colored TEXT always uses `--color-accent-text`, never `--color-accent`.** That's the one accent variant
darkened far enough to clear 4.5:1 on a light background (a pale user accent, say Apple Yellow, fails AA as plain text),
and it's what `LinkButton` and every accent label, link, and disclosure toggle uses. `--color-accent` is for fills,
borders, and focus rings, where the 3:1 non-text threshold applies. Hover doesn't lighten accent text: the lighter
`--color-accent-hover` drops below AA, so state comes from the underline or the surface, not the color.

The folder color setting (System Settings > Appearance > Folder color) is separate from the accent color. We use the
accent (theme) color for interactive UI chrome. This matches macOS intent: accent is for controls, folder tint is
cosmetic.

**Website:** Mustard yellow `#ffc206` is the brand accent. Used for CTAs, links, and emphasis. Hover: `#ffd23f`. Glow:
`rgba(255, 194, 6, 0.4)`.

### Neutrals

The app and website use different color temperatures by design:

- **App neutrals are pure gray.** A file manager displays user content (icons, text, images) that shouldn't be
  color-biased. Pure gray is the most neutral canvas, like a photographer's gray card.
- **Website neutrals are warm.** Marketing surfaces benefit from personality. Warm tones feel approachable and
  intentional. Cold grays feel generic.

**App (light):**

| Token                    | Value     | Role                                       |
| ------------------------ | --------- | ------------------------------------------ |
| `--color-bg-primary`     | `#ffffff` | Main canvas                                |
| `--color-bg-secondary`   | `#f5f5f5` | Headers, sidebars                          |
| `--color-bg-tertiary`    | `#e8e8e8` | Hover fills, grouped sections              |
| `--color-bg-dialog`      | `#ebebeb` | Every `ModalDialog` panel                  |
| `--color-text-primary`   | `#1a1a1a` | Body text (not pure black, easier on eyes) |
| `--color-text-secondary` | `#666666` | Labels, descriptions                       |
| `--color-text-tertiary`  | `#888888` | Timestamps, metadata                       |
| `--color-border`         | `#ddd`    | Default borders                            |
| `--color-border-strong`  | `#bbb`    | Panel dividers, emphasized boundaries      |
| `--color-border-subtle`  | `#e8e8e8` | Internal separators                        |

**App (dark):**

| Token                    | Value     | Role                                      |
| ------------------------ | --------- | ----------------------------------------- |
| `--color-bg-primary`     | `#1e1e1e` | Main canvas                               |
| `--color-bg-secondary`   | `#2a2a2a` | Headers, sidebars                         |
| `--color-bg-tertiary`    | `#333333` | Hover fills, grouped sections             |
| `--color-bg-dialog`      | `#232323` | Every `ModalDialog` panel                 |
| `--color-text-primary`   | `#e8e8e8` | Body text (not pure white, reduces glare) |
| `--color-text-secondary` | `#aaaaaa` | Labels, descriptions                      |
| `--color-text-tertiary`  | `#888888` | Timestamps, metadata                      |
| `--color-border`         | `#444444` | Default borders                           |
| `--color-border-strong`  | `#555555` | Panel dividers                            |
| `--color-border-subtle`  | `#333333` | Internal separators                       |

**Website (dark):**

| Token                      | Value     | Role                   |
| -------------------------- | --------- | ---------------------- |
| `--color-background`       | `#14130f` | Page background        |
| `--color-surface`          | `#1a1917` | Cards, code blocks     |
| `--color-surface-elevated` | `#222120` | Hover, raised elements |
| `--color-text-primary`     | `#fafafa` | Headings               |
| `--color-text-secondary`   | `#a1a1aa` | Body                   |
| `--color-border`           | `#2e2d2a` | Borders                |

**Website (light):** Used on sub-pages (blog, legal, pricing, changelog, roadmap). Landing page stays dark.

| Token                      | Value     | Role                   |
| -------------------------- | --------- | ---------------------- |
| `--color-background`       | `#fafaf8` | Page background        |
| `--color-surface`          | `#f0efec` | Cards, code blocks     |
| `--color-surface-elevated` | `#e8e7e3` | Hover, raised elements |
| `--color-text-primary`     | `#1a1917` | Headings               |
| `--color-text-secondary`   | `#5c5c66` | Body                   |
| `--color-border`           | `#d8d7d3` | Borders                |

Note: blog code blocks use a dark syntax theme regardless of page mode; they keep their dark surface/border tokens.

### Semantic colors

| Token                  | Light                   | Dark                       | Role                                                                                |
| ---------------------- | ----------------------- | -------------------------- | ----------------------------------------------------------------------------------- |
| `--color-allow`        | `#2e7d32`               | `#66bb6a`                  | Success, granted                                                                    |
| `--color-error`        | `#d32f2f`               | `#f44336`                  | Error, destructive (for borders, badges, icons)                                     |
| `--color-error-text`   | `#b91c1c`               | `#fca5a5`                  | Error text, dark/light enough to meet 4.5:1 on `--color-error-bg` and similar tints |
| `--color-error-bg`     | `#fef2f2`               | `#450a0a`                  | Error background fill                                                               |
| `--color-error-border` | `#fecaca`               | `#7f1d1d`                  | Error container border                                                              |
| `--color-warning`      | `#e65100`               | `#f5a623`                  | Caution (for borders, badges, icons)                                                |
| `--color-warning-text` | `#9a3412`               | `#fdba74`                  | Warning text (see `--color-error-text` rationale)                                   |
| `--color-warning-bg`   | `rgba(230, 81, 0, 0.1)` | `rgba(245, 166, 35, 0.15)` | Warning background fill                                                             |
| `--color-selection-fg` | `#c9a227`               | `#d4a82a`                  | Selected file names (gold, distinct from accent)                                    |

**When to use the `-text` variants:** use `--color-error-text` / `--color-warning-text` for `color:` on text rendered on
a same-hue tinted bg (or any bg where 4.5:1 isn't guaranteed). Use `--color-error` / `--color-warning` for
`border-color`, `background-color` of badges, and icon fills where no text sits directly on the brand color. If you need
white text on a warning-colored badge, use `--color-accent-fg` (always dark in both themes) rather than `white`.

The selection gold is intentional: it must be distinct from the accent color (which can be _any_ hue the user picks).
Gold was chosen because it reads as "marked" rather than "active," and contrasts with every macOS accent option.

**Contrast note:** `#c9a227` on `#ffffff` gives ~3.5:1, which passes WCAG AA for large text but not normal text. At 12px
(`--font-size-sm`) this is borderline. Acceptable because selected file names are always accompanied by a secondary cue
(the gold is applied only to names, never to standalone labels), but worth revisiting if we get accessibility
complaints.

**Automated contrast checks:** `scripts/check-a11y-contrast/` runs at build time via `pnpm check a11y-contrast`. It
parses `app.css` design tokens and every scoped `<style>` block, resolves `var()` chains + `color-mix(in srgb, ...)` +
`color-mix(in oklch, ...)`, and flags fg/bg pairs that fail WCAG 2.2 in light OR dark mode. This is intentionally
deterministic (no browser, no axe-core), so it doesn't flake on color-mix rendering quirks. See
`scripts/check-a11y-contrast/README.md` for scope and limitations.

**Three-tier a11y testing strategy:**

| Tier | Where                                                            | What it catches                                                                             | How fast |
| ---- | ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------- | -------- |
| 1    | `scripts/check-a11y-contrast/` (Go)                              | Contrast failures on every token + component style combo                                    | ~300 ms  |
| 2    | `apps/desktop/test/e2e-playwright/accessibility.spec.ts`         | Full-page structural audit (focus traps, Escape, keyboard nav) against a real Tauri webview | ~minutes |
| 3    | `apps/desktop/src/**/*.a11y.test.ts` (Vitest + axe-core + jsdom) | Per-component structural audit (ARIA, labels, roles)                                        | ~ms      |

Tiers 2 and 3 intentionally overlap for the structural rules. Tier 3 is the fast feedback loop during development; tier
2 still catches things jsdom can't model (focus trap across siblings, Escape returning focus to the trigger). See
`apps/desktop/src/lib/ui/DETAILS.md` § "Adding a component-level a11y test" for the tier-3 quickstart.

### Search highlight colors

| Token                      | Light                     | Dark                       | Role                                   |
| -------------------------- | ------------------------- | -------------------------- | -------------------------------------- |
| `--color-highlight`        | `rgba(255, 213, 0, 0.4)`  | `rgba(255, 213, 100, 0.9)` | Find-in-file and settings search match |
| `--color-highlight-active` | `rgba(255, 150, 50, 0.6)` | `rgba(255, 150, 100, 0.9)` | Currently focused search match         |

Dark mode uses near-opaque highlights because the dark background absorbs more color: translucent yellow becomes
invisible.

### Size-tier colors

File sizes are subtly color-coded by magnitude, a distinctive Cmdr feature. Colors are derived from
`--color-text-secondary` via `color-mix` so they stay readable in both modes without separate light/dark values.

The progression uses a single blue hue at increasing saturation: a "depth" metaphor where bigger = deeper. This avoids
the yellow → red "heat" connotation (size isn't danger, it's volume) and complements the pure-gray neutral palette.

```css
--color-size-bytes: color-mix(in srgb, var(--color-text-secondary) 85%, #88aacc); /* hint of blue */
--color-size-kb: color-mix(in srgb, var(--color-text-secondary) 60%, #88aacc); /* soft steel blue */
--color-size-mb: color-mix(in srgb, var(--color-text-secondary) 60%, #5599dd); /* medium blue */
--color-size-gb: color-mix(in srgb, var(--color-text-secondary) 60%, #2288ee); /* rich blue */
--color-size-tb: color-mix(in srgb, var(--color-text-secondary) 60%, #0066ff); /* vivid blue */
```

Bytes get a 15% blue tint (not plain gray) so even the smallest tier participates in the progression. The 60/40 mix for
KB+ gives clear differentiation between adjacent tiers.

When a file is **selected**, size tiers switch to a gold depth progression mirroring the blue one: bytes get a subtle
gold tint, higher tiers get richer gold, TB uses the full `--color-selection-fg`. The date column also turns gold. This
keeps the "selected = gold" language consistent across all columns (name, size, date).

### Header background

| Token               | Light     | Dark      | Role                        |
| ------------------- | --------- | --------- | --------------------------- |
| `--color-bg-header` | `#f0f0f0` | `#252525` | Column headers in file list |

Intentionally slightly darker than `--color-bg-secondary` (`#f5f5f5` / `#2a2a2a`). Column headers need visual weight to
anchor the file list below them. Kept as a separate token rather than reusing `--color-bg-secondary`.

## Typography

### App

```css
--font-system: -apple-system, BlinkMacSystemFont, system-ui, sans-serif;
--font-mono: ui-monospace, 'SF Mono', SFMono-Regular, Menlo, Monaco, Consolas, monospace;
```

**Type scale.** Fixed `px`, not `rem`: the app scales with macOS display settings, not browser font preferences.
`html { font-size: 16px }` is hardcoded to establish `1rem = 16px`. The body inherits this. The tokens below are for
explicit use on specific elements, not as a global cascade.

| Token            | Size | Role                                 | Why this step exists                         |
| ---------------- | ---- | ------------------------------------ | -------------------------------------------- |
| `--font-size-xs` | 10px | Tiny badges, shortcut hints          | Minimum legible size for auxiliary info      |
| `--font-size-sm` | 12px | File list body, most UI text         | Workhorse size for dense information display |
| `--font-size-md` | 14px | Dialog body, button labels, settings | Comfortable reading size for focused content |
| `--font-size-lg` | 16px | Dialog/section titles                | Clear heading that separates from body       |
| `--font-size-xl` | 20px | Page-level titles (settings, about)  | Large enough to anchor a full screen         |

Five steps, each perceptibly different from its neighbors. The 11px step from the existing code should be consolidated
to 12px. The 18px step to 16px or 20px.

**Future: system font size.** The intent is for chrome (settings, dialogs, buttons) to eventually follow the user's
macOS text size preference (Accessibility > Display > Text size), while pane text stays density-controlled and compact.
This requires research into how WKWebView responds to macOS Dynamic Type and is tracked as a future milestone, not part
of the initial design system migration.

### Leading (line height)

Leading is a property of the type style, not a per-component decision. A component never invents a number: stylelint
allows only these four tokens (a raw `0` stays legal; it collapses a box rather than setting a rhythm).

| Token                       | Value  | Where                                                                                 |
| --------------------------- | ------ | ------------------------------------------------------------------------------------- |
| `--font-line-height-flat`   | `1`    | Glyph boxes, badges, one-line chips: the height comes from padding, not the text      |
| `--font-line-height-tight`  | `1.2`  | Form controls (the `app-field.css` contract) and display type                         |
| `--font-line-height-normal` | `1.4`  | Every reading text. Inherited, so components rarely write it                          |
| `--font-line-height-prose`  | `1.55` | Long-measure blocks only: past ~70 characters (onboarding, consent, legal, changelog) |

**Why four and not a per-size ladder.** Leading is a function of type size (inversely) and measure (directly). The
size-inverse half only bites across big jumps, and our text range is 10–16px, narrow enough that one ratio serves it;
`tight` covers the display sizes where it does bite. The measure half is the one real judgment call, and it's decided by
looking at the container width, not by taste.

**Set once per surface, inherited from there.** A unitless line height inherits as a RATIO, so one declaration gives
every descendant the right leading for its own font size. `app.css` sets `normal` on the text surfaces: `.modal-dialog`,
`.wizard-panel`, `.toast`, and the settings / shortcuts / queue / debug window roots. Inside those, a component writes
`line-height` only when it needs something structurally different.

**The main window is deliberately absent from that list.** Its file lists and chrome are fixed-height, OS-dense surfaces
where the font's own metrics are the right answer, so they keep the browser default and any inherited ratio would be a
regression. Dialogs opened from the main window are on the list, so they still read as text. The file viewer's
`.file-content` is the one documented exception: its `1.5` is a measured contract shared with
`viewer-line-heights.svelte.ts` and `.line`'s pixel height, carrying a `stylelint-disable` that says so.

**Weight:**

- **Body text, file names**: 400 (regular)
- **Labels, button text**: 500 (medium)
- **Headings, emphasis**: 600 (semibold)

### Website

```css
--font-sans: 'Geist Sans', system-ui, -apple-system, sans-serif;
--font-mono: ui-monospace, 'SFMono-Regular', Menlo, Monaco, Consolas, monospace;
```

Geist Sans, self-hosted, variable weight. Replaces Inter. Use Tailwind's default type scale (`text-sm` through
`text-6xl`).

## Spacing

### App

Base unit: **4px**. Every spacing value must be a multiple of 4, no exceptions.

| Token           | Value | Role                                                  |
| --------------- | ----- | ----------------------------------------------------- |
| `--spacing-xs`  | 4px   | Inline gaps, icon-to-text                             |
| `--spacing-sm`  | 8px   | Standard element gap, list item padding               |
| `--spacing-md`  | 12px  | Component internal padding, button horizontal padding |
| `--spacing-lg`  | 16px  | Section padding, card internal margins                |
| `--spacing-xl`  | 24px  | Dialog body padding, large gaps between sections      |
| `--spacing-2xl` | 32px  | Page-level padding, large dialog body                 |

The existing 2px (`--spacing-xxs`) is used in tight file-list contexts. Keep it available but prefer `--spacing-xs`
(4px) when possible. The existing 6px (25 uses) is off-scale; migrate to `--spacing-xs` (4px) or `--spacing-sm` (8px)
case by case.

### Website

Tailwind's spacing scale (base 4px). No custom tokens needed.

## Border-radius

### App

| Token           | Value  | Role                                            |
| --------------- | ------ | ----------------------------------------------- |
| `--radius-xs`   | 2px    | Progress bars, search highlights, tiny swatches |
| `--radius-sm`   | 4px    | Chips, inline tags, compact inputs              |
| `--radius-md`   | 6px    | Standard inputs                                 |
| `--radius-lg`   | 8px    | Cards, larger containers                        |
| `--radius-full` | 9999px | Circles, pills, every `Button`, `ToggleGroup`   |

The scale intentionally uses small values: large radii look web-native, not macOS-native. Two deliberate exceptions
follow macOS itself: buttons and segmented controls are capsules (`--radius-full`), and a modal's corner is its own
`--radius-dialog` (27px), just under the window-level `--radius-xxl`.

### Website

Larger radii for marketing expressiveness:

| Class          | Value  | Role                         |
| -------------- | ------ | ---------------------------- |
| `rounded-lg`   | 8px    | Buttons, inputs, small cards |
| `rounded-xl`   | 12px   | Medium cards, badges         |
| `rounded-2xl`  | 16px   | Feature cards, hero elements |
| `rounded-full` | 9999px | Pills, dots                  |

## Depth

Both surfaces use **borders as the primary depth cue.** Shadows are reserved for floating layers only.

### App shadows

| Token            | Light value                            | Dark value                    | Role                   |
| ---------------- | -------------------------------------- | ----------------------------- | ---------------------- |
| `--shadow-sm`    | `0 1px 3px rgba(0,0,0,0.12)`           | `0 1px 3px rgba(0,0,0,0.3)`   | Subtle lift (tooltips) |
| `--shadow-md`    | `0 4px 12px rgba(0,0,0,0.1)`           | `0 4px 12px rgba(0,0,0,0.35)` | Dropdowns, popovers    |
| `--shadow-lg`    | `0 16px 48px rgba(0,0,0,0.15)`         | `0 16px 48px rgba(0,0,0,0.5)` | Modals                 |
| `--shadow-focus` | `0 0 0 3px var(--color-accent-subtle)` | same                          | Focus rings            |

Dark mode shadows use higher opacity because dark-on-dark has less perceived contrast.

**Focus ring accessibility.** When the accent color is close to the background (for example, a light-blue accent on a
light background), the focus ring can become hard to see. Use a two-layer strategy:

```css
/* Inner dark ring for guaranteed contrast, outer accent ring for color */
outline: 2px solid var(--color-accent);
outline-offset: 1px;
box-shadow: 0 0 0 4px rgba(0, 0, 0, 0.1); /* contrast backup in light mode */
```

In dark mode, the backup shadow uses `rgba(255, 255, 255, 0.08)` instead.

### Z-index scale (app)

| Token              | Value | Role                                  |
| ------------------ | ----- | ------------------------------------- |
| `--z-base`         | `0`   | Default stacking context              |
| `--z-sticky`       | `10`  | Sticky headers, column headers        |
| `--z-dropdown`     | `100` | Dropdowns, popovers, command palette  |
| `--z-overlay`      | `200` | Modal overlays (dim background)       |
| `--z-modal`        | `300` | Modal dialogs                         |
| `--z-notification` | `400` | Toasts, notifications (always on top) |
| `--z-tooltip`      | `500` | Tooltips (above all other layers)     |

Keep gaps between tiers so intermediate values are available without reshuffling. Never use raw numbers; always
reference the token.

### Website

Shadows reserved for the accent glow effect only (`0 0 40px var(--color-accent-glow)`). All other depth via borders.

## Motion

### App

| Token               | Value        | Role                                 |
| ------------------- | ------------ | ------------------------------------ |
| `--transition-fast` | `100ms ease` | Icon color changes, tiny state flips |
| `--transition-base` | `150ms ease` | Standard hover, focus, button press  |
| `--transition-slow` | `200ms ease` | Opacity fades, width/height changes  |

150ms is the standard. Fast enough to feel instant, slow enough to be perceived. Anything slower feels sluggish in a
keyboard-driven tool.

All non-essential animation must be wrapped in `@media (prefers-reduced-motion: no-preference)`. `ProgressBar`'s shimmer
is the worked example: the whole `::after` sits inside that query, and a second switch (`animated={false}`) stops it on
a bar that isn't moving, like a paused operation's. Prop table in `apps/desktop/src/lib/ui/DETAILS.md` § ProgressBar.

### Website

| Token               | Value                           | Role                             |
| ------------------- | ------------------------------- | -------------------------------- |
| `--duration-normal` | `300ms`                         | Standard transitions             |
| `--duration-slow`   | `500ms`                         | Reveal animations                |
| `--ease-out-expo`   | `cubic-bezier(0.16, 1, 0.3, 1)` | Entrance easing (signature feel) |

Hero uses a 7-step stagger blur-in, 100ms delay increments. Respect `prefers-reduced-motion`: degrade to a simple
opacity fade.

## Component patterns

For a live view of every primitive in `lib/ui/`, with all variants and states rendered flat, open the in-app **component
catalog**: Debug window (`⌘D`) → "Components", or `http://localhost:<port>/dev/components` in a browser tab. The
canonical "grouped card" wrapper used there (and intended for Settings refactors) is `SectionCard.svelte`. Its `tone`
prop (`neutral` | `info` | `warning` | `error`) is how a block says "pay attention here": it tints the card's fill and
border and leaves the text inside its normal color, so the signal rides the surface rather than turning a paragraph into
colored type. Reach for a toned card instead of hand-rolling a tinted `<p>` banner; prop table in
`apps/desktop/src/lib/ui/DETAILS.md` § SectionCard.

### Focus indicators (app)

Every interactive element needs a visible focus indicator. This is a keyboard-driven app, so all focus must be visible
(use `:focus`, not `:focus-visible`, for inputs).

- **Buttons**: Global `button:focus-visible` rule in `app.css` (automatic, no per-component work needed)
- **Inputs, selects, textareas**: Each component provides its own `:focus` rule:
  `border-color: var(--color-accent); box-shadow: var(--shadow-focus);`
- **File list containers (BriefList, FullList)**: Cursor/selection highlight serves as the focus indicator.
  `outline: none` is intentional
- **Non-interactive containers (`<div>` without tabindex, or tabindex=-1)**: May use `outline: none` freely (no focus
  indicator needed)

**Standard pattern for inputs:** Set `outline: none` on the base selector, then pair it with a `:focus` rule that
applies `border-color: var(--color-accent)` and `box-shadow: var(--shadow-focus)`. If the element has `border: none`,
add `border: 1px solid transparent` to the base so the border-color change is visible on focus.

**Why `:focus` instead of `:focus-visible`:** Browsers only apply `:focus-visible` to keyboard-initiated focus, hiding
the ring on click. In a keyboard-first file manager, users constantly switch between mouse and keyboard mid-action,
hiding the indicator on click would make the currently focused element invisible when they reach for the keyboard.

### File list (app, the primary surface)

The file list is what users see 90% of the time. Every other component is secondary.

**Density** is a user setting (`appearance.uiDensity`): Compact | Comfortable (default) | Spacious. The setting applies
three runtime CSS variables via JS in `settings-applier.ts`. These are NOT defined in `app.css`; they're set dynamically
on `document.documentElement`.

| Density               | `--row-height` | `--icon-size` | `--density-spacing` |
| --------------------- | -------------- | ------------- | ------------------- |
| Compact               | 16px           | 24px          | 2px                 |
| Comfortable (default) | 20px           | 32px          | 4px                 |
| Spacious              | 28px           | 40px          | 8px                 |

Row padding: `2px 8px` (vertical, horizontal). Line-height: `1.0`; the row height variable, not line-height, controls
vertical rhythm.

**Cursor highlight:**

- Focused pane: `background: var(--color-accent-subtle)` (derived from system accent at ~15% opacity)
- Unfocused pane: `background: var(--color-bg-tertiary)` (neutral, no accent)

**Selection:** File names turn `var(--color-selection-fg)` (gold). No background change; color alone signals selection,
keeping the list scannable.

**Scrollbars:** Native macOS. No custom styling. This is deliberate: custom scrollbars immediately feel web-native.

### Buttons (app)

Target heights follow macOS conventions: **mini** (22px), **regular** (32px).

**Primary** (regular):

```css
background: var(--color-accent);
color: white;
padding: 7px 20px; /* Yields ~32px total height at 14px/1.0 line-height */
font-size: var(--font-size-md); /* 14px */
font-weight: 500;
line-height: 1;
border: none;
border-radius: var(--radius-full); /* capsule ends, like a macOS alert button */
transition: background var(--transition-base);
```

Hover: `background: var(--color-accent-hover)` (derived via `color-mix`, never `brightness()` filter). Disabled:
`opacity: 0.4; cursor: not-allowed; pointer-events: none;` Focus-visible: uses the two-layer focus ring from the depth
section.

**Secondary** (regular):

```css
background: transparent;
color: var(--color-text-secondary);
padding: 7px 20px;
font-size: var(--font-size-md);
font-weight: 500;
line-height: 1;
border: 1px solid var(--color-border);
border-radius: var(--radius-md);
transition: all var(--transition-base);
```

Hover: `background: var(--color-bg-tertiary); color: var(--color-text-primary);`

**Danger:**

Same as secondary but: `color: var(--color-error); border-color: var(--color-error);` Hover:
`background: color-mix(in srgb, var(--color-error), transparent 90%);`

**Mini** (for inline/toolbar use):

```css
padding: 3px 12px; /* Yields ~22px at 12px/1.0 */
font-size: var(--font-size-sm); /* 12px */
border-radius: var(--radius-sm); /* 4px */
```

**All buttons:** `cursor: default`. No `outline: none` on focus; rely on focus-visible ring.

### Buttons (website)

**CTA:** `bg-accent text-background rounded-xl px-8 py-4 font-semibold text-lg` Hover:
`bg-accent-hover scale-[1.02] shadow-[0_0_40px_var(--color-accent-glow)]`

Scale is 1.02, not 1.05. 1.05 is noticeable enough to feel like a web gimmick; 1.02 adds life without feeling flashy.

**Secondary:** `border border-border bg-surface rounded-xl px-8 py-4 font-semibold` Hover:
`border-text-tertiary bg-surface-elevated`

### Dialogs (app)

All dialogs use `ModalDialog.svelte`.

| Property          | Value                                                                       | Why                                                                                                  |
| ----------------- | --------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| Panel surface     | `--color-bg-dialog` (`#ebebeb` / `#232323`)                                 | The settings window's backdrop, made opaque, so grouped blocks inside a dialog read as raised        |
| Body padding      | `0 var(--spacing-dialog)` (20px)                                            | Owned by `ModalDialog`; bottom comes from the footer or, when footerless, the same inset on the body |
| Title             | 16px, weight 600, left-aligned, flex row                                    | Clear hierarchy; the row lets a `StatusBadge` sit beside the words without hanging off the baseline  |
| Button row        | `flex, gap 12px, justify-content: flex-end`                                 | Right-aligned matches macOS convention (primary action right)                                        |
| Border-radius     | 27px (`--radius-dialog`)                                                    | Matches the macOS alert-panel corner                                                                 |
| Edge              | 1px `--color-dialog-border-outer` + inset 1px `--color-dialog-border-inner` | macOS draws a panel edge as two hairlines: darker outside, lighter inside                            |
| Drop shadow       | `--shadow-dialog` (three layers, down-only)                                 | Lifts the panel off the app the way a floating macOS panel casts                                     |
| Max content width | 480px                                                                       | Optimal line length (~60 chars at 14px body)                                                         |
| Alert width       | 420px, or 630px when it carries a `path`                                    | A sentence or two needs no more; a path is one long unbreakable string, and width buys back lines    |

**Three surfaces, one relationship.** The panel is `--color-bg-dialog`; a grouped block on it (`SectionCard`, a details
box) is `--color-bg-secondary` and reads as RAISED; a content well (the query dialogs' result list) is
`--color-bg-primary` and reads as RECESSED. That's the settings window's own stack, minus the vibrancy. A dialog's
strips paint no background of their own; separate them with spacing, or at most a hairline.

`ModalDialog` owns the standard body padding, so dialogs don't set their own, and there is **no opt-out**: the
horizontal inset (`--spacing-dialog`) matches the title bar and footer on every dialog, so a new body section can't
forget it and hang off the panel edge. The title bar's bottom padding supplies the gap above the body; the footer
supplies the gap below, and a footerless dialog gets the same inset as bottom padding on the body instead. ❌ A body
section must never re-add a horizontal inset of its own, in ANY token — that's a double inset, and it reads as text that
starts further in than the title above it and the buttons below it. A block that genuinely has to reach the panel edge
cancels the inset locally with a negative inline margin (`margin-inline: calc(var(--spacing-dialog) * -1)`), so the
exception is visible where it applies. `dialog-inset.spec.ts` walks the dialog gallery and measures it.

The body also carries `overflow-wrap: anywhere`, so a path, a URL, or a long filename wraps instead of running past the
inset to the panel edge. A path a dialog is ABOUT (rather than one mentioned in passing) belongs in a `CopyBox`, where
it's monospace, selectable, and copyable; `AlertDialog`'s `path` prop does that and widens the panel to 1.5× for it. The
opt-ins:

- `resizable`: lets the user drag any edge or corner to resize the dialog (default off), the way a macOS window resizes
  — no visible grip, just the arrow the pointer picks up on the edge. **Any dialog that renders a path or a filename
  turns this on**, because a fixed width is what forces the shortening in the first place; the body region grows and
  scrolls, and the caller still passes the initial size via `containerStyle`. Pass `"horizontal"` (rather than `true`)
  unless something inside actually absorbs extra height, which in practice means `fillBody` with a scrolling child: it
  then offers the left and right edges only, so a vertical drag can't open a band of dead space above the footer. The
  dialog can't grow past the viewport, and its own content is the floor it can't shrink below. A dialog never remembers
  a dragged size; it reopens at its `containerStyle` one. Note a `max-width` in `containerStyle` caps how far the drag
  can widen it, so state an opening `width` instead.
- `fillBody`: a fixed-height frame instead of one that grows with its content. The panel becomes a flex column, the body
  absorbs the vertical slack (as a column, so its own child can take `flex: 1 1 auto` and scroll), and the content clips
  to the panel's radius. Cap the height via `containerStyle`. Combines with `resizable`, and then the inner scroll
  region keeps ownership of the scrolling.
- `align="top"`: drops the dialog 10vh from the top instead of centering it, the Spotlight placement for a dialog the
  user types into and reads a long list from. The query dialogs are the only users.
- `ownsKeyboard`: hands the whole keydown contract to the consumer. `ModalDialog` still stops propagation, then forwards
  every key — including Escape and Enter on a focused button, which it otherwise handles itself. For dialogs with
  dynamic Enter semantics or an Escape that must defer to a nested popover.
- `closeOnOverlayClick`: clicking the scrim dismisses. Off by default, because macOS panels don't.
- `overlayClass`: an extra class on the overlay, for a SHARED dialog that needs one stable structural hook across
  several `dialogId`s (`QueryDialog` renders as `.search-overlay` for all three of its ids). Not a styling hook.

Overlay: `background: rgba(0,0,0, 0.4)` in light mode, `rgba(0,0,0, 0.6)` in dark mode (higher opacity needed for
contrast against dark chrome).

### Soft sheets (app)

Soft sheets cover ~90% of the viewport over the running app and host multi-step flows the user must commit to (consent,
setup, onboarding). The canonical implementation is `OnboardingWizard.svelte`. Unlike `ModalDialog`, sheets have no
title bar, no drag, no Escape close, no × button: the body owns the close gesture (Next / Finish / Allow / Deny). The
sheet is centered, lifted off the canvas with a frosted backdrop, and sized via the `--sheet-*` tokens below.

| Token                     | Value                        | Role                                                                                         |
| ------------------------- | ---------------------------- | -------------------------------------------------------------------------------------------- |
| `--sheet-width-fraction`  | `90vw`                       | Sheet width target. Pair with `min(var(--sheet-max-width), var(--sheet-width-fraction))`.    |
| `--sheet-height-fraction` | `90vh`                       | Sheet height target. Pair with `min(var(--sheet-max-height), var(--sheet-height-fraction))`. |
| `--sheet-max-width`       | `1200px`                     | Hard cap so the sheet stays readable on ultra-wide displays.                                 |
| `--sheet-max-height`      | `900px`                      | Hard cap so the sheet stays compact on 4K+ vertical setups.                                  |
| `--sheet-radius`          | `var(--radius-lg)` (8px)     | Matches macOS sheet convention.                                                              |
| `--sheet-backdrop-blur`   | `10px`                       | Frosted-glass amount. GPU-composited; the sheet is the only consumer today.                  |
| `--sheet-backdrop-color`  | `var(--color-overlay-heavy)` | Dim layer behind the sheet. Resolves to `rgba(0,0,0,0.6)` in both themes.                    |

`--sheet-backdrop-color` is heavier than `ModalDialog`'s scrim because sheets sit over the full app, not a centered
cluster.

**When to use a sheet vs `ModalDialog`:** each line pairs the sheet case with the matching `ModalDialog` case.

- **Multi-step flows the user must commit to (onboarding, paid licensing later)**: `ModalDialog` for single-decision
  confirmations (delete, discard, overwrite)
- **Content that needs > 480px width (provider picker grids, comparison tables)**: `ModalDialog` for short prose + a
  two-button choice
- **Flows where Escape-to-dismiss would lose meaningful state**: `ModalDialog` for flows where Escape-to-dismiss is safe
  (re-openable, idempotent)
- **First-launch consent (user must choose, can't cancel)**: `ModalDialog` for any everyday dialog (drag, blur, focus
  trap all come from the prim.)

Sheets are heavier: they own their own backdrop, focus trap, MCP dialog-registry entry (`'onboarding'`), and footer
chrome. Only use one when the contract genuinely needs it.

### Inputs (app)

Every text-entry surface renders through `TextInput` / `TextArea` (`lib/ui/`), so this is a description of what those
primitives already do, not a recipe to copy into a component. The chrome itself lives in exactly one place: `app.css` §
"Text fields" (the `.text-field*` classes), keyed off four tokens.

| Token                  | Value                     | Role                                                            |
| ---------------------- | ------------------------- | --------------------------------------------------------------- |
| `--radius-input`       | `var(--radius-lg)` (8px)  | Resting corner, ~25% of the field height: the macOS text field  |
| `--spacing-input`      | `var(--spacing-sm)` (8px) | Equal on all four sides, so the field reads evenly weighted     |
| `--font-size-input`    | `var(--font-size-md)`     | 14px, with `line-height: 1.2` (the frame's padding sets height) |
| `--shadow-focus-solid` | `0 0 0 2px accent`        | The focus ring: thicker and solid, not the translucent glow     |

Resting: a 1px `--color-border` hairline on `--color-bg-primary`. Focus (`:focus-within` on the frame, so a leading icon
or trailing button is inside the ring): `border-color: var(--color-accent)` plus `--shadow-focus-solid`, a 3px solid
accent edge in total. Invalid swaps both for `--color-error`; the caution state swaps both for `--color-warning`.

Two conventions the fields establish app-wide, both global rules in `app.css`:

- **Accent caret**: `caret-color: var(--color-accent)` on every `input` / `textarea`, including the Ark-rendered ones.
- **Accent selection**: `::selection` fills with `--color-accent-selection` (the accent at 30%, heavier than
  `--color-accent-subtle`, which is a surface tint) and keeps `--color-text-primary` on top.

`Combobox` and `NumberInput` render Ark UI's own input, so they can't delegate to `TextInput` without losing Ark's
wiring. They read the same four tokens instead, which is what keeps every field in the app identical.

### Text input and text area (app)

`TextInput` and `TextArea` (`lib/ui/`) are the house text fields: every raw
`<input type="text|password|email|search|url">` and `<textarea>` in the app renders through one of them, which is what
makes the § Inputs look above restyleable in a single edit. Both are presentational: no validation, debouncing, or
submit logic, so the caller keeps its own state and reaches the element through `bind:inputElement` /
`bind:textareaElement` for imperative focus and select.

**`TextInput`** props: `value` (bindable, or one-way alongside `oninput` for a controlled field), `type` (`text` |
`password` | `email` | `search` | `url` | `tel` | `number`), `radius` (`sm` | `md` | `lg` (default) | `full`), `variant`
(`default` | `chromeless`), `invalid`, `warning`, `mono`, `leadingIcon` (an `IconName`, decorative), a `trailing`
snippet (clear buttons, reveal toggles), `ariaLabel`, `containerStyle` (one-off layout sizing only), and `inputElement`.
Everything else (`id`, `placeholder`, `disabled`, `readonly`, `autocomplete`, `spellcheck`, `maxlength`, every `on*` and
`aria-*`) passes straight through to the `<input>`.

**`TextArea`** is the multi-line sibling, sharing the exact same chrome. Same props minus `type` / `leadingIcon`, plus
`rows` and `resizable` (the vertical grip, on by default). It's a sibling rather than a `multiline` prop because the two
have genuinely different contracts, and `bind:this` has to resolve to the right element type for the call sites that
focus and select.

**When to reach for what:**

- `radius="full"` + `leadingIcon="search"` is the search pill (the Settings sidebar, the Ask Cmdr session list). The
  primitive owns the icon's geometry as a real flex sibling, so there are no hand-tuned offsets to keep in sync.
- `variant="chromeless"` is for a field whose host already draws a surface (the command palette's full-bleed query
  line). It drops the frame, padding, and focus ring, and inherits the host's type scale, so the host can size the text.
- `mono` for paths, date-format strings, and keys. The placeholder stays in the system face: it's prose, not data.
- `readonly` quiets the fill without dimming the text, so the value stays copyable.

Two inline editors stay raw on purpose, each with an `eslint-disable-next-line cmdr/prefer-ui-primitive` and a reason:
`InlineRenameEditor` (row-height, severity-colored 2px border, glow and shake animations, and a `.rename-input` class
that's a load-bearing hit-test hook) and `VolumeBreadcrumb`'s favorite-rename field (dense dropdown row, inherits the
row's font, resting accent border).

### Checkbox and radio group (app)

`Checkbox` and `RadioGroup` (`lib/ui/`) are thin wrappers over Ark UI's `Checkbox` and `RadioGroup`. Unlike raw
`<input type="checkbox">` / `<input type="radio">`, they don't gray out when the window loses focus and they theme
through the design tokens (the box and dot fill with `--color-accent` when active), so they stay legible and on-brand in
a background window. Ark owns the keyboard and ARIA contract; the wrappers own the styling. Both are enforced as the
house controls by `cmdr/prefer-ui-primitive` (no raw native checkbox / radio).

**`Checkbox`** is a single on/off box. Props: `checked` (bindable, default `false`), `disabled`, `indeterminate` (mixed
dash state, overrides `checked` visually), `id`, `ariaLabel` (accessible name when there's no visible label),
`onCheckedChange`, and `children` (an inline label to the right of the box; omit for a bare box in list rows or dense
grids that own their own label). Bind with `<Checkbox bind:checked={value} />`.

**`RadioGroup`** is an items-driven single-select. Props: `value` (bindable, `''` means nothing selected), `items`
(`RadioItem[]`, each `{ value, label, description?, disabled? }`; `description` renders as quieter text below the
label), `onValueChange`, `disabled` (group-level), `orientation` (`'vertical'` stacks, `'horizontal'` wraps in a row),
`columns` (N equal full-width grid columns filling row by row, so five options over three columns read as 3 + 2;
overrides `orientation`, and beats `'horizontal'` whenever a wrapping row would break somewhere arbitrary), `ariaLabel`,
a `footer` snippet rendered after the items with the current `value` (for custom content when a specific option is
selected), and an `itemTrailing` snippet rendered on one option's own line (Brief mode's "Limit to" carries its width
field that way). `itemTrailing` renders BESIDE the option, never inside it: a focusable control nested in a
`role="radio"` element trips axe's nested-interactive rule. An option's dot CENTERS on its label; only an option that
carries a `description` top-aligns (it emits `data-described`), because there the dot belongs beside the label line
rather than the block's middle.

**`Switch`** (`lib/ui/Switch.svelte`) is the track-and-thumb on/off control: a thin wrapper over Ark UI's `Switch`, with
the same prop shape as `Checkbox` minus `indeterminate` (`checked` bindable, `disabled`, `id`, `ariaLabel`,
`onCheckedChange`, `children`), plus a `data-*` pass-through onto the hidden input for test hooks. The track fills with
`--color-accent` when on; the thumb stays white in both themes. There is ONE size (36×20 track, 16 px thumb) and one
implementation: every switch in the app renders through this file, so nothing hand-rolls Ark's `Switch` or re-declares
`.switch-control` / `.switch-thumb`. `SettingSwitch` wraps it with the settings-registry wiring, so settings rows and
feature code share it.

**When to use which:**

- `Checkbox` for a single independent on/off toggle: an option the user selects, then confirms with a button.
- `Switch` when the control reads as "this is on or off right now", which is why settings rows use it.
- `RadioGroup` for one choice from several mutually exclusive options, especially when the options need per-option
  descriptions or read better stacked vertically.
- `ToggleGroup` for a segmented control: short options that benefit from sitting side by side, or tabs that drive a UI
  mode. See § Component patterns and the Debug > Components catalog for its shape.

`Checkbox`'s root is a flex row (`align-items: center` + `--spacing-sm` gap), so an inline label sits on the box's
optical middle. Ark's `Root` is a `<label>` that would otherwise baseline-align the two and leave the box sitting low;
the gap belongs on the root because the label span is the box's SIBLING, not its container.

An empty `Checkbox` outlines with `--color-control-border`, a dedicated token sized for the WCAG 3:1 non-text minimum on
every app surface. The decorative `--color-border*` tokens sit at ~1.3–1.8:1 and are too faint to carry an affordance on
their own.

The OFF `Switch` track has no such outline: it's a bare `--color-bg-tertiary` fill, which lands at 1.1–1.3:1 against the
surfaces it sits on (computed from the tokens in `app.css`, 2026-07-23), so it doesn't clear 3:1 on its own the way the
checkbox does. Giving it one is a deliberate visual change to make with David, not a drive-by fix.

### Slider and number input (app)

`Slider` and `NumberInput` (`lib/ui/`) wrap Ark UI's `Slider` and `NumberInput`, and each is the only one of its kind in
the app. The slider is a 4 px track with a 16 px white thumb ringed in `--color-accent`; the range fill uses the accent
and animates on `--transition-base` (dropped under `prefers-reduced-motion`). The number input is a framed box with −/+
steppers around a centered field and an optional quiet unit label after it.

**`Slider`** takes plain numbers (`value` / `onChange`, not Ark's arrays) plus four optional decorations: `ticks` (marks
lit on an exact match), `snapTargets` (magnetic snapping within two steps), `endLabels` (quiet captions under the two
ends), and `valueLabel` + `valueLabelPlacement` (`'trailing'` or `'above'`). `ariaValueText` names the value when the
raw number wouldn't mean anything ("Sometimes used" rather than "2"). It never renders a hidden input, and every
decoration is `aria-hidden`, since the slider announces its own value.

**`NumberInput`** takes `value` / `onChange` as numbers, clamps into `[min, max]` itself, and never commits an emptied
field. The steppers' accessible names interpolate the field's `ariaLabel`.

**When to use which:** a slider for a coarse choice where the exact number doesn't matter (zoom, compression level,
coverage) — it has no paired number field, so the readout is a label and the value is drag-only. A number input when the
user wants to type an exact value (Brief mode's column-width limit). Registry-backed settings go through `SettingSlider`
/ `SettingNumberInput`, which add the settings wiring.

### Tooltips (app)

Custom frosted-glass tooltips via Svelte action (`use:tooltip`). A singleton `<div>` is appended to `<body>` and
repositioned per hover; only one tooltip exists at a time.

**Glass material:** the blur and hairline are the shared glass tokens (`--color-bg-glass` / `--color-border-glass`, also
used by filter-chip popovers and the volume dropdown), but the FILL is tooltip-specific.

- Fill: `--color-bg-tooltip` — the shared glass nudged 10% toward black (light) / white (dark), so a tooltip separates
  from whatever surface it floats over. Derived from `--color-bg-glass`, so it follows the reduce-transparency flip to
  an opaque fill for free. Verified ≥4.5:1 for `--color-text-primary` on every app backdrop and on the translucent worst
  case.
- Blur: `backdrop-filter: saturate(180%) blur(20px)`, dropped under `html.reduce-transparency`
- Hairline border: `0.5px solid rgba(0, 0, 0, 0.12)` (light) / `rgba(255, 255, 255, 0.1)` (dark)
- Shadow: `--shadow-sm`
- Radius: `--radius-sm` (4px)

**Typography:** `--font-size-sm` (12px), weight 400, line-height 1.3. Keyboard shortcuts render in a `<kbd>` badge with
accent-colored text on `--color-accent-subtle` background, using `--font-mono` at `--font-size-xs`.

**Timing:** 400ms show delay. Immediate hide. Entrance: 100ms opacity + 2px translateY (disabled under
`prefers-reduced-motion`).

**Positioning:** Below element by default, 6px offset. Flips above if near viewport bottom. Clamped to viewport edges
with 8px margin.

**Content types:**

- Simple string: `use:tooltip={"Close tab"}`
- With shortcut: `use:tooltip={{ text: "New tab", shortcut: "⌘T" }}`
- Rich HTML: `use:tooltip={{ html: dirSizeHtml }}`
- Overflow-only: `use:tooltip={{ text: fullPath, overflowOnly: true }}`

**Shortened text always has one.** Anything that ellipsizes (CSS `text-overflow`, the `useShortenMiddle` action, or a JS
`slice(…) + '…'`) carries `use:tooltip` with the full string, `overflowOnly: true` so a row that fits stays quiet. Where
the visible text is abbreviated rather than cut (a home directory shown as `~`), drop `overflowOnly`: it's incomplete at
any width. ❌ Never a native `title` for this: its delay and chrome are the OS's, so one surface would hover unlike
every other. `useShortenMiddle` routes through this action for the same reason.

**Accessibility:** Shows on focus (keyboard navigation), hides on blur/Escape. Trigger element gets `aria-describedby`
pointing to the tooltip's unique `id`. Tooltip has `role="tooltip"`.

### Keyboard shortcut hints (app)

Shortcut hints appear in custom tooltips (via `use:tooltip={{ text: "Label", shortcut: "⌘K" }}`) and in the command
palette. When displayed inline (for example, beside a menu item or in a settings label):

```css
font-size: var(--font-size-xs); /* 10px */
font-family: var(--font-mono);
color: var(--color-text-tertiary);
background: var(--color-bg-tertiary);
padding: 1px 4px;
border-radius: var(--radius-sm);
line-height: 1;
```

Keep them visually quiet; they're reference, not action. Monospace so key combinations (`Cmd+Shift+N`) align neatly.

### Loading states (app)

For operations under ~1 second, no indicator needed. For longer operations, follow the progressive disclosure from the
design guidelines in AGENTS.md:

1. **Spinner**: immediately on start. Use the existing LoadingIcon component.
2. **Progress bar + counter**: when the total is known (for example, "Loading 42 of 318 files"). Use `--color-accent`
   for the filled portion, `--color-bg-tertiary` for the track.
3. **Time estimate**: when we can extrapolate. Show in `--color-text-tertiary` below the progress bar.

All long operations must be cancelable. The cancel action should be a secondary button or `Escape` key, and must stop
both the UI indicator and the background process.

### Empty states (app)

Centered, single-line message in `--color-text-tertiary` at `--font-size-sm`: "Empty folder". No icon, no illustration;
keep it as quiet as the rest of the chrome. Shown when a directory exists and has loaded successfully but contains no
entries.

### Text overflow and number display (app)

The UI must never flicker or jump due to changing text content. Text should fill its available space, and when it
doesn't fit, show the most important parts.

**Mid-text truncation**: Use `useShortenMiddle` from `$lib/utils/shorten-middle-action` instead of CSS
`text-overflow: ellipsis` (which only clips from the end). The action uses `@chenglou/pretext` for pixel-accurate
canvas-based measurement, handles async loading with CSS fallback, and re-truncates on resize via `ResizeObserver`.

```svelte
<!-- File paths: snap to '/' so root context + filename stay visible -->
<div use:useShortenMiddle={{ text: filePath, preferBreakAt: '/' }}></div>

<!-- Filenames: snap to '.' to preserve the extension -->
<span use:useShortenMiddle={{ text: fileName, preferBreakAt: '.', startRatio: 0.7 }}></span>

<!-- General text: plain mid-split -->
<span use:useShortenMiddle={{ text: longText }}></span>
```

The pure function `shortenMiddle()` from `$lib/utils/shorten-middle` is also available for non-DOM contexts (accepts an
injectable `measureWidth` function).

**Stable container widths**: Dialogs and panels that display changing text (progress paths, scan stats) must use fixed
widths (`width: 500px`, not `min-width/max-width`) to prevent layout jitter.

**Number formatting**: Use `formatNumber()` from `selection-info-utils` for all user-facing counts (file counts, dir
counts, item counts). Raw numbers like `194667` are hard to read; always display as `194,667`. Byte values use
`$lib/units`'s `formatByteSize()`, which already handles this. Use `font-variant-numeric: tabular-nums` on numeric
displays so digits don't shift as values update.

### Notifications/toasts (app)

Slide in from top-right. Background: `--color-bg-secondary`. Border: `1px solid --color-border`. Shadow: `--shadow-md`.
Auto-dismiss after 4 seconds. Close button on hover.

## Applying the system

### App rules

1. System font only. Never import a web font.
2. All colors via CSS custom properties from `app.css`. Never hardcode hex in component styles.
3. All spacing via `--spacing-*` tokens. Never use arbitrary px values.
4. Accent color is dynamic; never assume it's blue. Derive hover/subtle variants with `color-mix()`.
5. Transitions default to `var(--transition-base)`. Only `--transition-fast` or `--transition-slow` with justification.
6. Focus indicator on every interactive element. No bare `outline: none`; see "Focus indicators" under Component
   patterns.
7. `prefers-reduced-motion` wraps all non-essential animation.
8. Scrollbars stay native. Never style `::-webkit-scrollbar`.

### Properties you don't need to set (app)

The reset and global styles in `app.css` already establish these. Re-declaring them in components is redundant.

**Already set on every element (`*` or `html, body`):**

| Property              | Global value | Set by                                                                  |
| --------------------- | ------------ | ----------------------------------------------------------------------- |
| `margin`              | `0`          | Reset (`* { margin: 0 }`)                                               |
| `padding`             | `0`          | Reset (`* { padding: 0 }`)                                              |
| `box-sizing`          | `border-box` | Reset (`html { box-sizing: border-box }` + `* { box-sizing: inherit }`) |
| `cursor`              | `default`    | `html, body { cursor: default }` (inherited by all descendants)         |
| `user-select`         | `none`       | `html, body { user-select: none }`                                      |
| `overscroll-behavior` | `none`       | `* { overscroll-behavior: none }`                                       |

**Inherited from `body` (don't repeat on child elements unless overriding):**

- **`color`**: `var(--color-text-primary)`
- **`font-size`**: `16px` (via `html`)
- **`font-family`**: Inherited from browser default (system font); set `var(--font-system)` only when needed

**CSS defaults you don't need to write:**

- **`display: block` on `<div>`**: That's the default
- **`flex-direction: row`**: Default for `display: flex`
- **`align-items: stretch`**: Default for `display: flex`
- **`position: static`**: That's the default
- **`opacity: 1`**: Default (except when transitioning from `opacity: 0`, where it's needed)
- **`visibility: visible`**: That's the default
- **`font-style: normal`**: Default (unless overriding an italic parent)
- **`text-decoration: none` on non-links**: That's the default
- **`border: none` on `<div>`**: Divs have no border by default
- **`background: transparent` on `<div>`**: Divs are transparent by default

When in doubt, check whether removing the declaration changes anything. If it doesn't, delete it.

### Website rules

1. Geist Sans, self-hosted, variable weight.
2. All colors via Tailwind theme tokens from `global.css`.
3. Sub-pages (blog, legal, pricing, changelog, roadmap) support both light and dark via `prefers-color-scheme`. Landing
   page is dark-only.
4. Blog code blocks keep dark surface tokens regardless of page color scheme.
5. CTA glow effect only on primary buttons. Secondary buttons shift surface color only.
6. Hero animation degrades to opacity-only under `prefers-reduced-motion`.

### What makes it feel like Cmdr on both surfaces

Not shared tokens, but shared _decisions_:

- Borders, not shadows, define structure. You can remove every shadow and still perceive the layout.
- One accent hue at a time. Everything else is neutral. No gratuitous color.
- Fast enough to feel mechanical. Transitions serve confirmation ("I heard your click"), not decoration.
- Dark mode is the assumed default, not a bolt-on. Light mode is tuned independently, not auto-inverted.
