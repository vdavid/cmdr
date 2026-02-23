# UI primitives

Reusable UI components used across the entire desktop app.

## Key files

| File                  | Purpose                                                                  |
| --------------------- | ------------------------------------------------------------------------ |
| `ModalDialog.svelte`  | Central modal container: overlay, dragging, Escape, focus, MCP tracking  |
| `dialog-registry.ts`  | `SOFT_DIALOG_REGISTRY` array — single source of truth for all dialog IDs |
| `Button.svelte`       | Styled button with variant and size props                                |
| `LoadingIcon.svelte`  | Animated spinner with progressive status text                            |
| `AlertDialog.svelte`  | Single-action confirmation dialog built on `ModalDialog`                 |
| `Notification.svelte` | Fixed-position slide-in toast (top-right), `info`/`error` styles         |

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

## Button

Variants: `primary` | `secondary` (default) | `danger`. Sizes: `regular` (default) | `mini`. Extends
`HTMLButtonAttributes` so all native button attributes pass through.

## LoadingIcon

Progressive status text driven by props (mutually exclusive, evaluated top-down):

1. `finalizingCount` set → "All N files loaded, just a moment now."
2. `loadedCount` set → "Loaded N files..."
3. `openingFolder` true → "Opening folder..."
4. Default → "Loading..."

`showCancelHint` adds "Press ESC to cancel and go back" below the spinner. The container uses a 400ms `fadeIn` animation
where the first 50% is invisible (effectively 200ms before fade begins), avoiding flash for fast loads.

## Key gotchas

- The Svelte 5 snippet named `title` shadows any prop also named `title`. In `AlertDialog` this is handled by
  destructuring as `title: dialogTitle`.
- `containerStyle` exists because stylelint blocks non-standard CSS custom properties (any not matching
  `(color|spacing|font)-` prefix). Use it for one-off sizing instead of CSS vars.
- `blur` prop applies `backdrop-filter` which triggers GPU compositing — use sparingly.
- `Notification.svelte` is a separate component from `UpdateNotification.svelte` (in the updates module). Use
  `Notification` for general app toasts.

## Dependencies

- `$lib/tauri-commands` — `notifyDialogOpened`, `notifyDialogClosed`
- `apps/desktop/src/app.css` — all CSS variables used here must be defined there
