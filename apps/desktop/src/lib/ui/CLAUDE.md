# UI primitives

Reusable UI components used across the entire desktop app.

## Key files

| File                     | Purpose                                                                  |
| ------------------------ | ------------------------------------------------------------------------ |
| `ModalDialog.svelte`     | Central modal container: overlay, dragging, Escape, focus, MCP tracking  |
| `dialog-registry.ts`     | `SOFT_DIALOG_REGISTRY` array — single source of truth for all dialog IDs |
| `Button.svelte`          | Styled button with variant and size props                                |
| `CommandBox.svelte`      | Copyable terminal command (monospace + Copy button)                      |
| `LoadingIcon.svelte`     | Animated spinner with progressive status text                            |
| `AlertDialog.svelte`     | Single-action confirmation dialog built on `ModalDialog`                 |
| `ProgressBar.svelte`     | Reusable progress bar (just the bar, no labels or layout)                |
| `ProgressOverlay.svelte` | Floating top-right progress indicator: spinner, progress bar, ETA        |
| `toast/`                 | Centralized toast notification system — store, container, item           |

## ModalDialog

Props:

| Prop             | Type                          | Notes                                                                 |
| ---------------- | ----------------------------- | --------------------------------------------------------------------- |
| `titleId`        | `string`                      | Used for `aria-labelledby`                                            |
| `title`          | Snippet                       | Rendered as `<h2>` in the title bar                                   |
| `children`       | Snippet                       | Dialog body                                                           |
| `dialogId`       | `SoftDialogId?`               | Auto-calls `notifyDialogOpened`/`notifyDialogClosed` on mount/destroy |
| `onclose`        | `() => void`?                 | Renders × button; also called on Escape                               |
| `draggable`      | `boolean`                     | Default `true`. Title bar drag moves the dialog.                      |
| `blur`           | `boolean`                     | `true` → 0.6 opacity + `backdrop-filter: blur(4px)` overlay           |
| `containerStyle` | `string`                      | Inline style appended to the dialog element (for sizing, colors)      |
| `role`           | `'dialog'` \| `'alertdialog'` | Default `'dialog'`                                                    |

The overlay element receives `tabindex="-1"` and is focused on mount so Escape/keydown events are captured without a
visible focus ring on the scrim.

## Dialog registry

`dialog-registry.ts` exports `SOFT_DIALOG_REGISTRY` (a `const` array) and the derived `SoftDialogId` union type. Using a
`dialogId` not in the registry produces a TypeScript error. The registry is sent to the Rust backend at startup so the
MCP "available dialogs" resource stays in sync.

To add a new dialog:

1. Add an entry to `SOFT_DIALOG_REGISTRY` in `dialog-registry.ts`.
2. Pass the new id as `dialogId` to `ModalDialog` — MCP tracking is then automatic.

## Tooltip (`../tooltip/tooltip.ts`)

Global tooltip system via Svelte action. Apple-style frosted glass appearance, 400ms show delay, auto-flips above/below
viewport.

Usage:

```svelte
import { tooltip } from '$lib/tooltip/tooltip'

<!-- Plain text -->
<span use:tooltip="Simple tooltip">...</span>

<!-- Rich HTML (supports <br>, <span class="...">, etc.) -->
<span use:tooltip={{ html: 'Line one<br>Line two' }}>...</span>

<!-- Text + keyboard shortcut badge -->
<span use:tooltip={{ text: 'Save', shortcut: '⌘S' }}>...</span>

<!-- Only show when text overflows -->
<span use:tooltip={{ text: longText, overflowOnly: true }}>...</span>

<!-- No tooltip (pass undefined or '') -->
<span use:tooltip={undefined}>...</span>
```

`TooltipParam` type: `string | { text?, html?, shortcut?, overflowOnly? } | null | undefined`.

The tooltip element has `white-space: pre-line` and uses global CSS classes, so `<span class="size-mb">` etc. work
inside `{ html }` tooltips. The `html` variant renders via `innerHTML` — only use with trusted content.

## Button

Variants: `primary` | `secondary` (default) | `danger`. Sizes: `regular` (default) | `mini`. Extends
`HTMLButtonAttributes` so all native button attributes pass through.

## LoadingIcon

Progressive status text driven by props (mutually exclusive, evaluated top-down):

1. `finalizingCount` set → "All N file/files loaded. Sorting your files, preparing view..."
2. `loadedCount` set → "Loaded N file/files..."
3. `openingFolder` true → "Opening folder..."
4. Default → "Loading..."

`showCancelHint` adds "Press ESC to cancel and go back" below the spinner. The container uses a 400ms `fadeIn` animation
where the first 50% is invisible (effectively 200ms before fade begins), avoiding flash for fast loads.

## ProgressBar

Reusable progress bar component — just the bar, no labels or layout. Consumers arrange their own labels around it.

Props:

| Prop        | Type           | Notes                                                                                       |
| ----------- | -------------- | ------------------------------------------------------------------------------------------- |
| `value`     | `number`       | 0–1 fractional progress                                                                     |
| `size`      | `'sm' \| 'md'` | Bar height + radius. `sm` = 4px / `--radius-xs`, `md` = 8px / `--radius-sm`. Default `'md'` |
| `ariaLabel` | `string?`      | Accessible label for screen readers                                                         |

Uses `role="progressbar"` with `aria-valuenow` / `aria-valuemin` / `aria-valuemax`. Fill transitions via
`transition: width 0.15s ease-out`.

Consumers: `ProgressOverlay` (size `sm`), `TransferProgressDialog` (size `md`, dual bars for size + file count).

## ProgressOverlay

Floating top-right overlay for showing progress on long-running operations. Uses `pointer-events: none` so it never
blocks clicks. Two layout modes:

- **Label only** (`progress` omitted): Spinner + single-line label. Compact layout.
- **With progress** (`progress` passed, even as `null`): Spinner + column layout with label, optional detail text,
  optional progress bar + percentage + ETA. The column has `min-width: 160px` to give the progress bar enough room.

Props:

| Prop       | Type             | Notes                                                                         |
| ---------- | ---------------- | ----------------------------------------------------------------------------- |
| `visible`  | `boolean`        | Show/hide the overlay                                                         |
| `label`    | `string`         | Main text (for example, "Scanning...", "Computing directory sizes...")        |
| `detail`   | `string?`        | Secondary text (for example, "42,000 entries")                                |
| `progress` | `number \| null` | 0–1 for determinate bar, `null` for no bar. Omit entirely for compact layout. |
| `eta`      | `string \| null` | Pre-formatted ETA string (for example, "~2 min left")                         |

Used by `ScanStatusOverlay` (indexing progress). Designed to also be used for replay progress.

## Toast system (`toast/`)

Centralized toast notifications with stacking, levels, and two dismissal modes.

- **Store** (`toast-store.svelte.ts`): Module-level `$state` array. `addToast(content, options?)` accepts a `Snippet` or
  plain `string`. Optional `id` for dedup (replace in place). Max 5 visible.
- **Container** (`ToastContainer.svelte`): Mounted once in `(main)/+layout.svelte`. Fixed top-right, stacks vertically.
- **Item** (`ToastItem.svelte`): Frame, close button, auto-dismiss timer for transient toasts.

Levels: `info` (default), `success`, `warn`, `error`. Dismissal: `transient` (4s timeout + nav-dismiss, default) or
`persistent`.

Call `dismissTransientToasts()` on pane navigation to clear stale feedback.

## CommandBox

`CommandBox.svelte` — monospace terminal command with a one-click Copy button and 2-second "Copied!" feedback. Takes a
single `command` string prop. Handles clipboard internally (`copyToClipboard` with `navigator.clipboard` fallback).
Parent controls spacing via its own wrapper. Used in `PtpcameradDialog`, `MtpPermissionDialog`, and `ShareBrowser`.

## Ark UI

Uses `@ark-ui/svelte` as the headless component library for complex interactive components (Dialog, Tabs, Select,
Checkbox, Switch, Slider, Radio Group, etc.). Chosen over Bits UI and Melt UI for: 45+ components (vs ~20-25), clean
tree-shaking (1.33 MB unpacked), Zag.js FSM robustness (prevents invalid states), full focus/escape control (disable FSM
defaults with `={false}`, implement custom logic in callbacks), and scoped CSS selectors
(`[data-scope="dialog"][data-part="content"]`) that work with vanilla CSS. Team-maintained by Chakra UI team. Simple
elements like `<Button>` are our own thin wrappers (a button needs no headless library).

## Adding a component-level a11y test (tier 3)

Cmdr runs a three-tier a11y strategy — see `docs/design-system.md` § "Automated contrast checks" and
`apps/desktop/test/e2e-playwright/accessibility.spec.ts` for tiers 1 and 2. Tier 3 runs axe-core against a component
mounted in Vitest/jsdom, covering structural a11y (ARIA, labels, focusable-when-enabled) in milliseconds. Contrast is
tier 1's job; focus traps and Escape-return-focus are tier 2's.

Helper: `$lib/test-a11y` exports `expectNoA11yViolations(container)`. Same axe ruleset as E2E, minus `color-contrast`
and `region` (both misfire in jsdom — see the helper's comments).

Template — colocate `ComponentName.a11y.test.ts` next to the component:

```ts
import { describe, it } from 'vitest'
import { mount, tick } from 'svelte'
import MyComponent from './MyComponent.svelte'
import { expectNoA11yViolations } from '$lib/test-a11y'

describe('MyComponent a11y', () => {
  it('default state has no a11y violations', async () => {
    const target = document.createElement('div')
    document.body.appendChild(target)
    mount(MyComponent, {
      target,
      props: {
        /* default props */
      },
    })
    await tick()
    await expectNoA11yViolations(target)
  })
})
```

Write one test per meaningful state (default, disabled, error, open/closed, etc.). For components that hit Tauri IPC,
mock `$lib/tauri-commands` at the top of the file. Set `CMDR_A11Y_DEBUG=1` to log pass/violation counts per call when
investigating why a test passes silently.

## Key decisions

**Decision**: Custom `ModalDialog` with manual overlay + drag logic instead of the native `<dialog>` element. **Why**:
Native `<dialog>` doesn't support drag-to-reposition, and its `::backdrop` is not style-customizable enough for the blur
effect. The trade-off is manually managing focus trapping and Escape handling, but the overlay `tabindex="-1"` +
`focus()` on mount approach is simpler than a full focus-trap library.

**Decision**: Dialog registry is a `const` array with `satisfies` (not an `enum` or `Record`). **Why**:
`as const satisfies` gives a union type for `SoftDialogId` that TypeScript can narrow, while also letting the array be
iterated at runtime to register with the Rust MCP backend. An `enum` can't be iterated without extra transformation, and
a `Record` would split the ID from its metadata.

**Decision**: `containerStyle` prop for one-off sizing instead of CSS custom properties or class names. **Why**: The
project's stylelint config blocks custom properties that don't match the `(color|spacing|font)-` prefix convention.
Inline style strings bypass this restriction for layout-only overrides (width, max-width) that don't belong in the
design token system.

**Decision**: Toast content accepts both `string` and `Component<any>` (Svelte component). **Why**: Simple notifications
are strings. Interactive toasts (update restart, AI download) need buttons and state, so they're full Svelte components.
The toast item renders strings as `<span>` and components via `{@const}` + render — no wrapper needed.

**Decision**: Toast dedup uses an optional `id` key with in-place replacement rather than preventing duplicates.
**Why**: The update toast and AI toast need to update their content as state changes (e.g. download progress) while
keeping the same slot in the stack. Replacing in place avoids the visual flicker of remove-then-add.

## Key gotchas

- The Svelte 5 snippet named `title` shadows any prop also named `title`. In `AlertDialog` this is handled by
  destructuring as `title: dialogTitle`.
- `containerStyle` exists because stylelint blocks non-standard CSS custom properties (any not matching
  `(color|spacing|font)-` prefix). Use it for one-off sizing instead of CSS vars.
- `blur` prop applies `backdrop-filter` which triggers GPU compositing — use sparingly.
- When the toast stack is full (5 toasts) and all are persistent, new toasts are silently dropped. This is intentional —
  persistent toasts represent important state (update ready, AI installing) and should not be evicted by transient
  feedback.

## Dependencies

- `$lib/tauri-commands` — `notifyDialogOpened`, `notifyDialogClosed`
- `apps/desktop/src/app.css` — all CSS variables used here must be defined there
