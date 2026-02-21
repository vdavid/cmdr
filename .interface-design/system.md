# Cmdr design system

Design language for the Cmdr desktop app and getcmdr.com website.

## Principles

1. **The tool recedes, the content leads.** The file list is 90% of the app. Chrome (toolbars, dialogs, headers) should
   be quiet so file names, sizes, and icons can breathe. Rich data, calm surroundings.
2. **Personal, not branded.** In the app, the user's macOS accent color drives all interactive UI — selection, focus,
   buttons. The Cmdr brand (mustard yellow) lives only on marketing surfaces. The app feels like _their_ tool, not ours.
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
--color-accent-hover: color-mix(in oklch, var(--color-accent), white 15%);       /* light mode */
--color-accent-hover: color-mix(in oklch, var(--color-accent), white 10%);       /* dark mode */
--color-accent-subtle: color-mix(in oklch, var(--color-accent), transparent 85%); /* tinted bg */
```

The folder color setting (System Settings > Appearance > Folder color) is separate from the accent color. We use the
accent (theme) color for interactive UI chrome. This matches macOS intent: accent is for controls, folder tint is
cosmetic.

**Migration note:** The existing `--color-button-hover` and `--color-bg-hover` tokens (hardcoded blue-tinted rgba
values) will be removed. Interactive hover states migrate to `--color-accent-subtle` (accent-derived). Neutral hover
states (non-interactive surfaces) migrate to `--color-bg-tertiary`. Affected components: ModalDialog, Notification,
DualPaneExplorer, SortableHeader, ShareBrowser, NetworkBrowser, viewer page, CommandPalette, and settings components
(9 files, ~17 occurrences).

**Website:** Mustard yellow `#ffc206` is the brand accent. Used for CTAs, links, and emphasis. Hover: `#ffd23f`. Glow:
`rgba(255, 194, 6, 0.4)`.

### Neutrals

The app and website use different color temperatures by design:

- **App neutrals are pure gray.** A file manager displays user content (icons, text, images) that shouldn't be
  color-biased. Pure gray is the most neutral canvas, like a photographer's gray card.
- **Website neutrals are warm.** Marketing surfaces benefit from personality. Warm tones feel approachable and
  intentional — cold grays feel generic.

**App (light):**

| Token | Value | Role |
|-------|-------|------|
| `--color-bg-primary` | `#ffffff` | Main canvas |
| `--color-bg-secondary` | `#f5f5f5` | Headers, sidebars |
| `--color-bg-tertiary` | `#e8e8e8` | Hover fills, grouped sections |
| `--color-text-primary` | `#1a1a1a` | Body text (not pure black — easier on eyes) |
| `--color-text-secondary` | `#666666` | Labels, descriptions |
| `--color-text-tertiary` | `#888888` | Timestamps, metadata |
| `--color-border` | `#ddd` | Default borders |
| `--color-border-strong` | `#bbb` | Panel dividers, emphasized boundaries |
| `--color-border-subtle` | `#e8e8e8` | Internal separators |

**App (dark):**

| Token | Value | Role |
|-------|-------|------|
| `--color-bg-primary` | `#1e1e1e` | Main canvas |
| `--color-bg-secondary` | `#2a2a2a` | Headers, sidebars |
| `--color-bg-tertiary` | `#333333` | Hover fills, grouped sections |
| `--color-text-primary` | `#e8e8e8` | Body text (not pure white — reduces glare) |
| `--color-text-secondary` | `#aaaaaa` | Labels, descriptions |
| `--color-text-tertiary` | `#888888` | Timestamps, metadata |
| `--color-border` | `#444444` | Default borders |
| `--color-border-strong` | `#555555` | Panel dividers |
| `--color-border-subtle` | `#333333` | Internal separators |

**Website (dark):**

| Token | Value | Role |
|-------|-------|------|
| `--color-background` | `#14130f` | Page background |
| `--color-surface` | `#1a1917` | Cards, code blocks |
| `--color-surface-elevated` | `#222120` | Hover, raised elements |
| `--color-text-primary` | `#fafafa` | Headings |
| `--color-text-secondary` | `#a1a1aa` | Body |
| `--color-border` | `#2e2d2a` | Borders |

**Website (light):** Used on sub-pages (blog, legal, pricing, changelog, roadmap). Landing page stays dark.

| Token | Value | Role |
|-------|-------|------|
| `--color-background` | `#fafaf8` | Page background |
| `--color-surface` | `#f0efec` | Cards, code blocks |
| `--color-surface-elevated` | `#e8e7e3` | Hover, raised elements |
| `--color-text-primary` | `#1a1917` | Headings |
| `--color-text-secondary` | `#5c5c66` | Body |
| `--color-border` | `#d8d7d3` | Borders |

Note: blog code blocks use a dark syntax theme regardless of page mode — they keep their dark surface/border tokens.

### Semantic colors

| Token | Light | Dark | Role |
|-------|-------|------|------|
| `--color-allow` | `#2e7d32` | `#66bb6a` | Success, granted |
| `--color-error` | `#d32f2f` | `#f44336` | Error, destructive |
| `--color-error-bg` | `#fef2f2` | `#450a0a` | Error background fill |
| `--color-error-border` | `#fecaca` | `#7f1d1d` | Error container border |
| `--color-warning` | `#e65100` | `#f5a623` | Caution |
| `--color-warning-bg` | `rgba(230, 81, 0, 0.1)` | `rgba(245, 166, 35, 0.15)` | Warning background fill |
| `--color-selection-fg` | `#c9a227` | `#d4a82a` | Selected file names (gold, distinct from accent) |

The selection gold is intentional: it must be distinct from the accent color (which can be _any_ hue the user picks).
Gold was chosen because it reads as "marked" rather than "active," and contrasts with every macOS accent option.

### Search highlight colors

| Token | Light | Dark | Role |
|-------|-------|------|------|
| `--color-highlight` | `rgba(255, 213, 0, 0.4)` | `rgba(255, 213, 100, 0.9)` | Find-in-file and settings search match |
| `--color-highlight-active` | `rgba(255, 150, 50, 0.6)` | `rgba(255, 150, 100, 0.9)` | Currently focused search match |

Dark mode uses near-opaque highlights because the dark background absorbs more color — translucent yellow becomes
invisible.

### Size-tier colors

File sizes are subtly color-coded by magnitude — a distinctive Cmdr feature. Colors are derived from
`--color-text-secondary` via `color-mix` so they stay readable in both modes without separate light/dark values.

```css
--color-size-kb: color-mix(in srgb, var(--color-text-secondary) 70%, #f0c000); /* warm yellow */
--color-size-mb: color-mix(in srgb, var(--color-text-secondary) 70%, #ff8c00); /* orange */
--color-size-gb: color-mix(in srgb, var(--color-text-secondary) 70%, #ff4444); /* red */
--color-size-tb: color-mix(in srgb, var(--color-text-secondary) 70%, #aa44ff); /* purple */
```

Byte-sized files use `--color-text-secondary` (no tint). The 70/30 mix keeps the tint subtle — it's a hint, not a
traffic light.

### Header background

| Token | Light | Dark | Role |
|-------|-------|------|------|
| `--color-bg-header` | `#f0f0f0` | `#252525` | Column headers in file list |

Intentionally slightly darker than `--color-bg-secondary` (`#f5f5f5` / `#2a2a2a`). Column headers need visual weight
to anchor the file list below them. Kept as a separate token rather than reusing `--color-bg-secondary`.

## Typography

### App

```css
--font-system: -apple-system, BlinkMacSystemFont, system-ui, sans-serif;
--font-mono: ui-monospace, 'SF Mono', SFMono-Regular, Menlo, Monaco, Consolas, monospace;
```

**Type scale.** Fixed `px`, not `rem` — the app scales with macOS display settings, not browser font preferences.
`html { font-size: 16px }` is hardcoded to establish `1rem = 16px`. The body inherits this. The tokens below are for
explicit use on specific elements, not as a global cascade.

| Token | Size | Role | Why this step exists |
|-------|------|------|---------------------|
| `--font-size-xs` | 10px | Tiny badges, shortcut hints | Minimum legible size for auxiliary info |
| `--font-size-sm` | 12px | File list body, most UI text | Workhorse size for dense information display |
| `--font-size-md` | 14px | Dialog body, button labels, settings | Comfortable reading size for focused content |
| `--font-size-lg` | 16px | Dialog/section titles | Clear heading that separates from body |
| `--font-size-xl` | 20px | Page-level titles (settings, about) | Large enough to anchor a full screen |

Five steps, each perceptibly different from its neighbors. The 11px step from the existing code should be consolidated
to 12px. The 18px step to 16px or 20px.

**Future: system font size.** The intent is for chrome (settings, dialogs, buttons) to eventually follow the user's
macOS text size preference (Accessibility > Display > Text size), while pane text stays density-controlled and compact.
This requires research into how WKWebView responds to macOS Dynamic Type and is tracked as a future milestone, not part
of the initial design system migration.

**Line-height** (critical for a file manager):

| Context | Line-height | Why |
|---------|-------------|-----|
| File list rows | `1.0` | Maximum density. Row height controls vertical rhythm, not line-height. |
| Dialog/settings body | `1.4` | Comfortable reading for multi-line text. |
| Buttons, labels | `1.0` | Tight. Padding controls button height, not line-height. |
| Headings | `1.2` | Standard heading tightness. |

**Weight:**

| Usage | Weight |
|-------|--------|
| Body text, file names | 400 (regular) |
| Labels, button text | 500 (medium) |
| Headings, emphasis | 600 (semibold) |

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

| Token | Value | Role |
|-------|-------|------|
| `--spacing-xs` | 4px | Inline gaps, icon-to-text |
| `--spacing-sm` | 8px | Standard element gap, list item padding |
| `--spacing-md` | 12px | Component internal padding, button horizontal padding |
| `--spacing-lg` | 16px | Section padding, card internal margins |
| `--spacing-xl` | 24px | Dialog body padding, large gaps between sections |
| `--spacing-2xl` | 32px | Page-level padding, large dialog body |

The existing 2px (`--spacing-xxs`) is used in tight file-list contexts. Keep it available but prefer `--spacing-xs`
(4px) when possible. The existing 6px (25 uses) is off-scale — migrate to `--spacing-xs` (4px) or `--spacing-sm` (8px)
case by case.

### Website

Tailwind's spacing scale (base 4px). No custom tokens needed.

## Border-radius

### App

| Token | Value | Role |
|-------|-------|------|
| `--radius-sm` | 4px | Chips, inline tags, compact inputs |
| `--radius-md` | 6px | Buttons, standard inputs |
| `--radius-lg` | 8px | Dialogs, cards, larger containers |
| `--radius-full` | 9999px | Circles, pills |

Consolidate the existing 3px (10 uses) to `--radius-sm`. The scale intentionally uses small values — large radii look
web-native, not macOS-native.

### Website

Larger radii for marketing expressiveness:

| Class | Value | Role |
|-------|-------|------|
| `rounded-lg` | 8px | Buttons, inputs, small cards |
| `rounded-xl` | 12px | Medium cards, badges |
| `rounded-2xl` | 16px | Feature cards, hero elements |
| `rounded-full` | 9999px | Pills, dots |

## Depth

Both surfaces use **borders as the primary depth cue.** Shadows are reserved for floating layers only.

### App shadows

| Token | Light value | Dark value | Role |
|-------|-------------|------------|------|
| `--shadow-sm` | `0 1px 3px rgba(0,0,0,0.12)` | `0 1px 3px rgba(0,0,0,0.3)` | Subtle lift (tooltips) |
| `--shadow-md` | `0 4px 12px rgba(0,0,0,0.1)` | `0 4px 12px rgba(0,0,0,0.35)` | Dropdowns, popovers |
| `--shadow-lg` | `0 16px 48px rgba(0,0,0,0.15)` | `0 16px 48px rgba(0,0,0,0.5)` | Modals |
| `--shadow-focus` | `0 0 0 3px var(--color-accent-subtle)` | same | Focus rings |

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

### Website

Shadows reserved for the accent glow effect only (`0 0 40px var(--color-accent-glow)`). All other depth via borders.

## Motion

### App

| Token | Value | Role |
|-------|-------|------|
| `--transition-fast` | `100ms ease` | Icon color changes, tiny state flips |
| `--transition-base` | `150ms ease` | Standard hover, focus, button press |
| `--transition-slow` | `200ms ease` | Opacity fades, width/height changes |

150ms is the standard. Fast enough to feel instant, slow enough to be perceived. Anything slower feels sluggish in a
keyboard-driven tool.

All non-essential animation must be wrapped in `@media (prefers-reduced-motion: no-preference)`.

### Website

| Token | Value | Role |
|-------|-------|------|
| `--duration-normal` | `300ms` | Standard transitions |
| `--duration-slow` | `500ms` | Reveal animations |
| `--ease-out-expo` | `cubic-bezier(0.16, 1, 0.3, 1)` | Entrance easing (signature feel) |

Hero uses a 7-step stagger blur-in, 100ms delay increments. Respect `prefers-reduced-motion`: degrade to a simple
opacity fade.

## Component patterns

### File list (app — the primary surface)

The file list is what users see 90% of the time. Every other component is secondary.

**Density** is a user setting (`appearance.uiDensity`): Compact | Comfortable (default) | Spacious. The setting applies
three runtime CSS variables via JS in `settings-applier.ts`. These are NOT defined in `app.css` — they're set
dynamically on `document.documentElement`.

| Density | `--row-height` | `--icon-size` | `--density-spacing` |
|---------|---------------|--------------|-------------------|
| Compact | 16px | 24px | 2px |
| Comfortable (default) | 20px | 32px | 4px |
| Spacious | 28px | 40px | 8px |

Row padding: `2px 8px` (vertical, horizontal). Line-height: `1.0` — the row height variable, not line-height, controls
vertical rhythm.

**Cursor highlight:**
- Focused pane: `background: var(--color-accent-subtle)` (derived from system accent at ~15% opacity)
- Unfocused pane: `background: var(--color-bg-tertiary)` (neutral, no accent)

**Selection:** File names turn `var(--color-selection-fg)` (gold). No background change — color alone signals
selection, keeping the list scannable.

**Scrollbars:** Native macOS. No custom styling. This is deliberate — custom scrollbars immediately feel web-native.

### Buttons (app)

Target heights follow macOS conventions: **mini** (22px), **regular** (32px).

**Primary** (regular):

```css
background: var(--color-accent);
color: white;
padding: 7px 20px;               /* Yields ~32px total height at 14px/1.0 line-height */
font-size: var(--font-size-md);   /* 14px */
font-weight: 500;
line-height: 1.0;
border: none;
border-radius: var(--radius-md);  /* 6px */
transition: background var(--transition-base);
```

Hover: `background: var(--color-accent-hover)` (derived via `color-mix`, never `brightness()` filter).
Disabled: `opacity: 0.4; cursor: not-allowed; pointer-events: none;`
Focus-visible: uses the two-layer focus ring from the depth section.

**Secondary** (regular):

```css
background: transparent;
color: var(--color-text-secondary);
padding: 7px 20px;
font-size: var(--font-size-md);
font-weight: 500;
line-height: 1.0;
border: 1px solid var(--color-border);
border-radius: var(--radius-md);
transition: all var(--transition-base);
```

Hover: `background: var(--color-bg-tertiary); color: var(--color-text-primary);`

**Danger:**

Same as secondary but: `color: var(--color-error); border-color: var(--color-error);`
Hover: `background: color-mix(in srgb, var(--color-error), transparent 90%);`

**Mini** (for inline/toolbar use):

```css
padding: 3px 12px;               /* Yields ~22px at 12px/1.0 */
font-size: var(--font-size-sm);   /* 12px */
border-radius: var(--radius-sm);  /* 4px */
```

**All buttons:** `cursor: pointer`. No `outline: none` on focus — rely on focus-visible ring.

### Buttons (website)

**CTA:** `bg-accent text-background rounded-xl px-8 py-4 font-semibold text-lg`
Hover: `bg-accent-hover scale-[1.02] shadow-[0_0_40px_var(--color-accent-glow)]`

Scale is 1.02, not 1.05. 1.05 is noticeable enough to feel like a web gimmick; 1.02 adds life without feeling flashy.

**Secondary:** `border border-border bg-surface rounded-xl px-8 py-4 font-semibold`
Hover: `border-text-tertiary bg-surface-elevated`

### Dialogs (app)

All dialogs use `ModalDialog.svelte`.

| Property | Value | Why |
|----------|-------|-----|
| Body padding | `0 24px 24px` | Wide enough for comfortable reading at 480px max width |
| Title | 16px, weight 600, centered | Clear hierarchy, centered for symmetry in floating dialogs |
| Button row | `flex, gap 12px, justify-content: flex-end` | Right-aligned matches macOS convention (primary action right) |
| Border-radius | 8px (`--radius-lg`) | Matches macOS window chrome radius |
| Max content width | 480px | Optimal line length (~60 chars at 14px body) |

Overlay: `background: rgba(0,0,0, 0.4)` in light mode, `rgba(0,0,0, 0.6)` in dark mode (higher opacity needed for
contrast against dark chrome).

### Inputs (app)

```css
padding: 8px 12px;
border: 1px solid var(--color-border);
border-radius: var(--radius-sm);    /* 4px — tighter than buttons, matches macOS text fields */
font-size: var(--font-size-md);     /* 14px */
line-height: 1.4;
background: var(--color-bg-primary);
transition: border-color var(--transition-base);
```

Focus: `border-color: var(--color-accent); box-shadow: var(--shadow-focus);`
Error: `border-color: var(--color-error); box-shadow: 0 0 0 3px color-mix(in srgb, var(--color-error), transparent 85%);`

### Tooltips (app)

Native `title` attribute. No custom tooltip component. This is intentional — native tooltips match OS behavior and don't
require hover-timing logic. If custom tooltips become necessary later, they should use `--shadow-sm`, `--radius-sm`,
`--font-size-sm`, and `--color-bg-secondary` background.

### Empty states (app)

Currently missing — no visual feedback when a folder is empty. Should show a centered, single-line message in
`--color-text-tertiary` at `--font-size-sm`: "Empty folder". No icon, no illustration — keep it as quiet as the rest of
the chrome.

### Notifications/toasts (app)

Slide in from top-right. Background: `--color-bg-secondary`. Border: `1px solid --color-border`. Shadow: `--shadow-md`.
Auto-dismiss after 4 seconds. Close button on hover.

## Applying the system

### App rules

1. System font only. Never import a web font.
2. All colors via CSS custom properties from `app.css`. Never hardcode hex in component styles.
3. All spacing via `--spacing-*` tokens. Never use arbitrary px values.
4. Accent color is dynamic — never assume it's blue. Derive hover/subtle variants with `color-mix()`.
5. Transitions default to `var(--transition-base)`. Only `--transition-fast` or `--transition-slow` with justification.
6. Focus ring on every interactive element via `:focus-visible`. No bare `outline: none`.
7. `prefers-reduced-motion` wraps all non-essential animation.
8. Scrollbars stay native. Never style `::-webkit-scrollbar`.

### Website rules

1. Geist Sans, self-hosted, variable weight.
2. All colors via Tailwind theme tokens from `global.css`.
3. Sub-pages (blog, legal, pricing, changelog, roadmap) support both light and dark via `prefers-color-scheme`. Landing
   page is dark-only.
4. Blog code blocks keep dark surface tokens regardless of page color scheme.
5. CTA glow effect only on primary buttons. Secondary buttons shift surface color only.
6. Hero animation degrades to opacity-only under `prefers-reduced-motion`.

### What makes it feel like Cmdr on both surfaces

Not shared tokens — shared _decisions_:
- Borders, not shadows, define structure. You can remove every shadow and still perceive the layout.
- One accent hue at a time. Everything else is neutral. No gratuitous color.
- Fast enough to feel mechanical. Transitions serve confirmation ("I heard your click"), not decoration.
- Dark mode is the assumed default, not a bolt-on. Light mode is tuned independently, not auto-inverted.
