# Branding

Cmdr's visual identity across the website, the desktop app, and marketing materials (newsletter, etc.).

We don't have a formal design system yet. This doc captures what we've settled on so far.

## Logo

A golden/mustard folder icon with AI-inspired sparkle/glow effects behind it. White horizontal bars on the folder
suggest a file list. The overall feel is "familiar file manager meets futuristic AI."

- Source icon: `_ignored/app-icon-no-background.png` (transparent bg, used for regeneration) - NOT IN THE REPO!
- Desktop app icons: `apps/desktop/src-tauri/icons/` (all sizes, generated via `pnpm tauri icon`)
- Website: `apps/website/public/logo-512.png` (512x512), `logo-32.png` (32x32)
- Favicons: `apps/website/public/favicon.png`, `favicon.ico`, `apple-touch-icon.png`
- See `docs/guides/regenerating-app-icon.md` for how to regenerate

## Colors

### Website (dark-first)

Defined in `apps/website/src/styles/global.css` via Tailwind v4 `@theme`.

| Token                      | Hex                      | Usage                                           |
|----------------------------|--------------------------|-------------------------------------------------|
| `--color-background`       | `#0a0a0b`                | Page background                                 |
| `--color-surface`          | `#111113`                | Cards, panels                                   |
| `--color-surface-elevated` | `#18181b`                | Elevated surfaces, content areas                |
| `--color-border`           | `#27272a`                | Primary borders                                 |
| `--color-border-subtle`    | `#1c1c1f`                | Subtle dividers                                 |
| `--color-text-primary`     | `#fafafa`                | Headings, primary text                          |
| `--color-text-secondary`   | `#a1a1aa`                | Body text, descriptions                         |
| `--color-text-tertiary`    | `#9e9ea8`                | Muted text, captions                            |
| `--color-accent`           | `#ffc206`                | **The mustard yellow.** CTAs, links, highlights |
| `--color-accent-hover`     | `#ffd23f`                | Hover state for accent                          |
| `--color-accent-glow`      | `rgba(255, 194, 6, 0.4)` | Box shadow glow on buttons                      |
| `--color-warning`          | `#ff4d6d`                | Error/warning states                            |

The website is dark by default with no light mode toggle. The mustard accent on near-black is the most recognizable
part of the brand.

### Desktop app

Defined in `apps/desktop/src/app.css` via CSS custom properties. Supports both light and dark mode via
`prefers-color-scheme`.

**Light mode:**

| Token                    | Hex                      | Usage                            |
|--------------------------|--------------------------|----------------------------------|
| `--color-bg-primary`     | `#ffffff`                | Main background                  |
| `--color-bg-secondary`   | `#f5f5f5`                | Panels, sidebars                 |
| `--color-text-primary`   | `#000000`                | Primary text                     |
| `--color-text-secondary` | `#666666`                | Secondary text                   |
| `--color-accent`         | `#0078d4`                | Interactive accent (Fluent Blue) |
| `--color-selection-fg`   | `#c9a227`                | Selected item text (golden)      |
| `--color-highlight`      | `rgba(255, 213, 0, 0.4)` | Search highlight (golden)        |

**Dark mode:**

| Token                    | Hex       | Usage                             |
|--------------------------|-----------|-----------------------------------|
| `--color-bg-primary`     | `#1e1e1e` | Main background                   |
| `--color-bg-secondary`   | `#2a2a2a` | Panels, sidebars                  |
| `--color-text-primary`   | `#ffffff` | Primary text                      |
| `--color-text-secondary` | `#aaaaaa` | Secondary text                    |
| `--color-accent`         | `#4da3ff` | Interactive accent (lighter blue) |

Note: the desktop app uses a blue accent (`#0078d4` / `#4da3ff`) for interactive elements, not the mustard yellow.
The golden/mustard tones show up in selection and search highlights. See `app.css` for the full set of tokens.

### Color personality

The two signature colors are **mustard yellow** (`#ffc206`) and **near-black** (`#0a0a0b`). When you need to convey
"this is Cmdr" in a new context (email, social, docs), reach for these two first:

- Mustard on dark for CTAs, accent bars, link color
- Dark text on mustard for buttons (like the website's "Download" button)

## Typography

### Website

- **Sans-serif:** Inter (self-hosted, variable weight 100-900)
    - Font file: `apps/website/public/fonts/inter-latin-variable.woff2`
    - Fallback stack: `'Inter', system-ui, -apple-system, sans-serif`
- **Monospace:** `ui-monospace, 'SFMono-Regular', Menlo, Monaco, Consolas, monospace`

### Desktop app

- **Sans-serif:** System font stack: `-apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif`
- **Monospace:** `ui-monospace, 'SF Mono', SFMono-Regular, Menlo, Monaco, Consolas, monospace`
- Base font size: 16px (`1rem = 16px`)

## Icons

### Website feature icons

Location: `apps/website/public/icons/`

Six SVG icons used on the features section. All share the same style:

- 24x24 viewBox, 2px stroke width, stroke color `#ffc206` (mustard)
- `brain.svg`, `search.svg`, `zap.svg`, `keyboard.svg`, `rocket.svg`, `folder.svg`

### Desktop app icons

Location: `apps/desktop/static/icons/`

Functional icons for sync states and other UI elements. Different style from the website icons.

## Voice and tone

See the [style guide](../style-guide.md) for the full writing guide. The short version:

- Friendly, informal, encouraging
- Abbreviate ("don't", "we're", "it's")
- Sentence case everywhere
- Active voice
- No jargon, no ableist language, no latinisms

The tagline is **"The AI-native file manager"**. The legal entity is **Rymdskottkärra AB**, based in Sweden.

## Newsletter

The Listmonk campaign template lives
at [campaign-template.html](../../infra/listmonk/campaign-template.html). It uses the website's dark
palette:

- `#0a0a0b` outer background, `#18181b` content card, `#27272a` borders
- `#ffc206` mustard accent bar at the top and for links/CTAs
- `#fafafa` headings, `#d4d4d8` body text
- Inter font with system fallbacks
- Logo + "Cmdr" wordmark header, centered
- Rounded content card (degrades gracefully in Outlook)

## Quick reference for new surfaces

When creating something new that should feel like Cmdr (landing page, email, social image, docs theme, whatever):

1. Start with the near-black background (`#0a0a0b`)
2. Use `#18181b` for content surfaces with `#27272a` borders
3. Use mustard `#ffc206` for the primary accent (links, buttons, highlights)
4. Dark text (`#0a0a0b`) on mustard buttons
5. Inter for text, system mono for code
6. Keep it clean and minimal. No gradients, no drop shadows, no decorative fluff. Let the content breathe.
