# Toast notifications

Consolidate the three independent notification implementations into a single toast system that supports stacking,
rich content, and two dismissal behaviors.

## Current state

Three separate, independently styled components all `position: fixed` to the same top-right corner:

| Component | Location | Used by | Dismissal | Content |
|-----------|----------|---------|-----------|---------|
| `Notification.svelte` | `$lib/ui/` | `FilePane.svelte` (rename) | Click [x], click anywhere, or navigate | Plain text |
| `UpdateNotification.svelte` | `$lib/updates/` | `(main)/+layout.svelte` | "Later" button (session flag) | Fixed template |
| `AiNotification.svelte` | `$lib/ai/` | `(main)/+layout.svelte` | State machine + action buttons | Multi-step workflow |

Problems:

1. **Overlap.** All three use the same `top/right` position. If two appear at once, they stack on top of each other
   with no offset. This already happens: update notification + AI notification can both render simultaneously.
2. **No queue or stacking.** Each pane has its own `renameNotification` state. Two rapid renames in different panes
   show two overlapping toasts.
3. **Duplicated styles.** All three duplicate the same base styles (bg, border, radius, shadow, z-index).
4. **Plain text only.** The generic `Notification.svelte` takes a `message: string`. No way to include links, icons,
   images, progress bars, or action buttons without creating a new one-off component.
5. **No warning level.** Only `info` and `error`.
6. **Aspirational gaps.** The file-operations CLAUDE.md mentions "brief success toast" for same-FS move but no
   `Notification` import exists in that module. `settings-store.ts` has a `// Could show a toast here` comment.

## Two dismissal types

Every toast falls into one of two categories. The split comes from a single question: **can the user safely ignore
this?** If yes, the system cleans it up. If no, the user must act. There's no middle ground — a toast that sometimes
auto-dismisses important info trains users to ignore all toasts.

### Transient

Disappears automatically. The trigger is **any pane navigation** (folder change in either pane) OR a configurable
auto-dismiss timeout (default: 4 seconds), whichever comes first. The user can also dismiss early by clicking [x].

**Why navigation-dismiss?** In a dual-pane file manager, navigating to a new folder is a hard context switch. Feedback
about a rename in the previous folder becomes irrelevant the moment the user moves on. Pane navigation is the natural
"I'm done with that context" signal — more reliable than a timer alone, because a user might navigate in 0.5 seconds
(fast keyboard workflow) or sit for 30 seconds reading the folder contents.

**Why 4 seconds as the fallback?** The user might not navigate at all (for example, renaming multiple files in the
same folder). 4 seconds is long enough to read a one-line message (~12 words at average reading speed) but short
enough to not feel stuck. This is configurable per toast for cases where the content is longer.

Use for ephemeral feedback that doesn't block any workflow:

- Rename succeeded (info)
- "File disappeared from view because hidden files aren't shown" (info)
- Same-FS move completed instantly (info, aspirational)
- Settings save failed (error, aspirational)

### Persistent

Stays on screen until the user takes an explicit action (close button or an action button). Does NOT auto-dismiss on
navigation or timeout.

Use for things that require a decision or acknowledgment:

- Update available — "Restart" / "Later" buttons
- AI model offer — "Download" / "Not now" / "I don't want AI" buttons
- AI download progress — "Cancel" button
- AI ready — "Got it" button

### Coverage check

All existing notification use cases fit cleanly into these two types:

| Existing use case | Dismissal type | Level |
|-------------------|----------------|-------|
| Rename validation error | Transient | error |
| Rename permission error | Transient | error |
| Hidden file info message | Transient | info |
| Update ready | Persistent | info |
| AI offer | Persistent | info |
| AI downloading | Persistent | info |
| AI installing | Persistent | info |
| AI ready | Persistent | info |
| AI starting | Persistent | info |

No current use case requires a "warning" level today, but adding it is trivial (one more CSS variant), and we should
have it from the start so that upcoming features (copy safety warnings, low disk space) don't need to touch the toast
system itself — they just pick `level: 'warn'`.

## Design

### Levels

Three severity levels, following the existing semantic color system from `app.css` and `system.md`:

| Level | Border | Background | Icon | Use for |
|-------|--------|------------|------|---------|
| info | `--color-border` | `--color-bg-secondary` | None (or optional custom) | Success, neutral feedback |
| warn | `--color-warning` at ~30% opacity | `--color-warning-bg` | Warning triangle | Caution, non-blocking issues |
| error | `--color-error-border` | `--color-error-bg` | Error circle | Failures, blocked actions |

No `success` level — `info` covers that. A dedicated success level would add a green toast for "file renamed" which
feels excessive in a file manager. If a future feature genuinely needs green-bg success, we add it then.

### Content model

Toast content is a **Svelte snippet**, not a plain string.

**Why snippet, not a string or a component?** A string can't hold the AI notification's progress bar and three
action buttons. A component prop (like `component: AiToastContent`) would work but forces every toast caller to
create a separate `.svelte` file even for one-liners. Snippets are Svelte 5's native solution for passing renderable
content — they work inline at the call site for simple cases and can reference a separate component for complex ones.
The toast system doesn't need to know or care what's inside.

```svelte
<Toast level="info" dismissal="transient">
  {#snippet content()}
    <span>File disappeared from view because hidden files aren't shown.</span>
  {/snippet}
</Toast>
```

For the complex AI notification, the snippet contains the full multi-state UI (title, description, progress bar,
buttons) — the toast system only provides the container, positioning, and dismissal behavior.

The `content` snippet can include anything: text, links (`<a>`), images, progress bars, buttons, whatever the caller
needs. The toast component provides the frame; the caller provides the content.

### Stacking

Toasts stack vertically from the top-right corner, each offset by the height of the previous toast plus a gap.
New toasts appear at the top, pushing existing ones down.

**Why newest on top?** The user's eye is already at the top-right corner where they expect new information. Appending
at the bottom would force the eye to track a growing list. Newest-on-top also means the most relevant toast is always
in the same spot, which matters for keyboard users navigating to the close button.

```
┌──────────────────────────┐
│ [Newest toast]        [x]│  ← top: var(--spacing-lg)
└──────────────────────────┘
        ↕ var(--spacing-sm) gap
┌──────────────────────────┐
│ [Older toast]         [x]│
└──────────────────────────┘
```

Maximum visible toasts: **5**. This is generous — in practice we rarely expect more than two or three (for example,
AI downloading + rename error). The cap exists as a safety valve so a bug in a caller can't flood the screen. If a
sixth arrives, the oldest transient toast is dismissed. If all five are persistent, the sixth queues and appears when
one is dismissed.

Animation: slide in from the right (existing `slide-in` keyframe), slide out to the right on dismiss. When a toast
in the middle is dismissed, the ones below it slide up to close the gap (use CSS `transition` on `top`/`transform`).
Wrap all animation in `@media (prefers-reduced-motion: no-preference)`.

### Positioning

- `position: fixed`, `top: var(--spacing-lg)`, `right: var(--spacing-lg)`
- `z-index: var(--z-notification)` (400, per existing scale — above modals so toasts remain visible even during
  dialog interaction)
- `max-width: 360px` — inherited from the current `Notification.svelte`. Wide enough for a two-line message with a
  close button; narrow enough to not obscure file list content in the pane underneath. The AI notification currently
  uses 320px and fits fine, so 360px gives slightly more breathing room for rich content without changing the feel.

### Accessibility

- Container: `role="alert"` for error/warning toasts, `role="status"` for info. **Why the split?** `role="alert"`
  triggers assertive announcements — screen readers interrupt what they're reading to announce the toast. That's
  correct for errors ("rename failed") but rude for info ("file hidden from view"). `role="status"` waits for a
  natural pause before announcing, which matches the non-urgent nature of info toasts.
- `aria-live="polite"` on the toast container region
- Close button: `aria-label="Dismiss notification"`
- Transient toasts must remain visible long enough to be read (minimum 4s, consider scaling with content length)
- Focus doesn't move to the toast on appearance (non-modal) — moving focus would disrupt keyboard workflows, which
  is the opposite of helpful for a keyboard-driven app
- Keyboard: Tab can reach the close button and any interactive content within the toast

## Architecture

### Files

```
src/lib/ui/toast/
├── toast-store.svelte.ts   — Reactive store: toast array, add/dismiss/clear functions
├── ToastContainer.svelte   — Renders the stack, handles positioning and animation
├── ToastItem.svelte        — Single toast: frame, close button, auto-dismiss timer
└── index.ts                — Public API re-exports
```

### Store (`toast-store.svelte.ts`)

**Why a centralized store?** The root problem is overlap — three components independently deciding to render at the
same position. A centralized store is the only way to coordinate stacking offsets and enforce the max-5 cap. It also
means any module in the app can show a toast without importing a component or wiring up parent-child props — just
call `addToast()`. This is why `settings-store.ts` couldn't easily show a toast today (it's a plain `.ts` module
with no component access).

```typescript
type ToastLevel = 'info' | 'warn' | 'error'
type ToastDismissal = 'transient' | 'persistent'

interface ToastOptions {
  level?: ToastLevel         // default: 'info'
  dismissal?: ToastDismissal // default: 'transient'
  timeoutMs?: number         // default: 4000 for transient, ignored for persistent
  id?: string                // optional dedup key — if a toast with this ID exists, replace it
}

// Core API
function addToast(content: Snippet, options?: ToastOptions): string  // returns toast ID
function dismissToast(id: string): void
function dismissTransientToasts(): void  // called on pane navigation
function clearAllToasts(): void
```

The store is a module-level `$state` array (in a `.svelte.ts` file per Svelte 5 rules). The `addToast` function is
the main entry point — call it from anywhere in the app.

The `id` option enables dedup: if you call `addToast(content, { id: 'ai-download' })` while a toast with that ID
already exists, it replaces the content in place instead of adding a new toast. **Why this matters:** the AI
notification transitions through five states (offer → downloading → installing → ready → starting). Without dedup,
each state change would add a new toast to the stack. With dedup, calling `addToast(newContent, { id: 'ai' })` swaps
the content in the same slot — the user sees one toast that updates, not five that pile up.

### Container (`ToastContainer.svelte`)

Mounted once in `(main)/+layout.svelte`. Iterates the store array and renders `ToastItem` instances with calculated
vertical offsets. Handles the stacking layout.

### Integration with pane navigation

`DualPaneExplorer.svelte` (or wherever pane navigation is handled) calls `dismissTransientToasts()` when either pane
navigates to a new directory. This replaces the current `renameNotification = null` pattern in `FilePane.svelte`.

### Replacing existing components

| Current | Replacement |
|---------|-------------|
| `Notification.svelte` | `addToast(snippet, { level, dismissal: 'transient' })` in FilePane |
| `UpdateNotification.svelte` | `addToast(snippet, { id: 'update', dismissal: 'persistent' })` in updater |
| `AiNotification.svelte` | `addToast(snippet, { id: 'ai', dismissal: 'persistent' })` in ai-state |

The existing components are deleted after migration. The content snippets (buttons, progress bars, etc.) move to the
call sites or to small helper snippets in their respective modules.

## Styling

Follows the design system (`system.md`):

- Background: `--color-bg-secondary`
- Border: `1px solid --color-border` (overridden per level for warn/error)
- Radius: `--radius-lg` (8px)
- Shadow: `--shadow-md`
- Padding: `--spacing-sm` vertical, `--spacing-md` horizontal
- Font: `--font-size-sm` (12px) body, `--font-size-sm` weight 600 for title if present
- Close button: `--color-text-tertiary`, hover `--color-bg-tertiary`
- Transition: `--transition-slow` (200ms) for slide in/out

These match the existing `Notification.svelte` styles exactly — no visual change for current toasts.

## Out of scope

- **Sound.** No audio feedback on toast appearance.
- **Toast history / notification center.** Not needed now. If we add it later, the store already has the data.
- **Click-to-action on the entire toast.** Toasts are not clickable as a whole. Actions live inside the content
  snippet as explicit buttons/links.

## Migration order

The order is smallest-to-largest by complexity. Each step validates the system with a real use case before taking on
the next, harder migration. If the store API or component design needs adjustment, we want to discover that during
step 2 (simple string toast), not step 4 (multi-state AI workflow).

1. Build the store and components with tests. Wire up `ToastContainer` in the layout.
2. Migrate `Notification.svelte` usage (rename toasts in FilePane) — simplest case, plain text, validates the core
   add/dismiss/transient flow.
3. Migrate `UpdateNotification.svelte` — first persistent toast with action buttons, validates snippet content and
   the dedup `id` pattern.
4. Migrate `AiNotification.svelte` — most complex: five states, progress bar, three button variants. Validates that
   the snippet approach can fully replace a dedicated component.
5. Delete the three old components. Verify with `--check knip` that nothing references them.
6. Add the "brief success toast" for same-FS move (the aspirational case from file-operations CLAUDE.md). This is
   the first *new* use of the system, proving it's easy to adopt from a fresh call site.
7. Update `$lib/ui/CLAUDE.md`, `$lib/updates/CLAUDE.md`, and `$lib/ai/CLAUDE.md` to document the new system and
   remove references to the old components.
