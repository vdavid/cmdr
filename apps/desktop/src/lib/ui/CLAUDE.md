# UI primitives

Reusable UI components used across the entire desktop app. Because nearly every frontend session touches this directory,
this file stays strict: only the rules that prevent silent breakage live here. Component usage catalogs, prop tables,
the toast-level guide, and decisions are in [DETAILS.md](DETAILS.md).

## Module map

- `ModalDialog.svelte` (overlay + drag + Escape + focus + MCP tracking), `focus-trap.ts` (`use:trapFocus`),
  `dialog-registry.ts` (`SOFT_DIALOG_REGISTRY`), `AlertDialog.svelte`.
- Primitives: `Button`, `LinkButton`, `CommandBox`, `LoadingIcon`, `ProgressBar`, `Size`, `DateLabel`, `ShortcutChip`,
  `StatusBadge`, `SectionCard`, `ToggleGroup`, `Dropdown`, `FilterDropdown`, `Chip`, plus the `toast/` system. Tooltip
  lives in `../tooltip/tooltip.ts`.
- Ark UI (`@ark-ui/svelte`) is the headless library for complex interactive components; simple ones are our own thin
  wrappers.

## Must-knows

- **Every `role="dialog"` / `role="alertdialog"` element MUST carry `use:trapFocus` on the same element** (enforced by
  `cmdr/dialog-needs-focus-trap`). Without it, Tab leaks focus into the suppressed-shortcut background = a full keyboard
  lockout. `ModalDialog` owns the directive, so components passing `role` as a prop don't repeat it. Opt out only with
  an `eslint-disable-next-line cmdr/dialog-needs-focus-trap -- <reason>` (today only `NetworkLoginForm`, a non-modal
  in-pane form). Omit `onEscape` only for dialogs that must swallow Escape (the onboarding wizard).
- **Adding a dialog: add its id to `SOFT_DIALOG_REGISTRY` and pass it as `ModalDialog`'s `dialogId`.** A `dialogId` not
  in the registry is a TypeScript error; the registry is sent to the Rust MCP backend at startup, so MCP "available
  dialogs" silently drifts if you skip it. Soft sheets (`OnboardingWizard`) are NOT `ModalDialog` but still register
  (`'onboarding'`) for MCP tracking.
- **The `ModalDialog` overlay starts at `inset: var(--titlebar-height) 0 0 0`, not `inset: 0`**, so the scrim never
  covers the macOS overlay title bar (the OS window-drag region stays live). Any new full-window backdrop outside
  `ModalDialog` (command palette, query dialog) must do the same.
- **Don't restyle `.btn-*` colors from a scoped feature component** (`scripts/check-btn-restyle` flags it). The accent
  contrast checker mirrors the runtime variants; a one-off needs a `/* allowed-btn-restyle: <reason> */`. `LinkButton`
  is the ONLY place that opts back into `cursor: pointer` (app-wide `cursor: default`; stylelint blocks it elsewhere).
  Don't roll your own link-styled button.
- **`ShortcutChip` must NOT statically import `openShortcutCustomization`** (load-bearing). That helper pulls in
  `@tauri-apps/api/webviewWindow`, which must stay out of the chip's module-eval surface so it's importable in the
  capability-restricted viewer window (and would reject at runtime there). Load it via dynamic `import()` in the click
  handler only. Exactly one of `commandId` / `key` must be set (dev-time error otherwise); a `commandId` chip renders
  NOTHING when the command has no binding, so conditionalize the surrounding prose.
- **Tooltip detached-trigger gotcha (corner tooltip):** a virtual-scroll row recycled while hovered is removed without
  firing `mouseleave`, so the 400ms show timer can fire against a detached node (all-zero rect → tooltip in the top-left
  corner). Two guards must both stay: the action's `destroy()` cancels its pending timer, and
  `showTooltip`/`positionTooltip` bail on `isTriggerDetached(el)` (`!el.isConnected`). Don't swap the `isConnected`
  check for a zero-rect heuristic (happy-dom reports zero rects for every connected element).
- **Toast `default` level is rare on purpose; pick the lowest-intensity fitting level.** Five levels carry meaning by
  feedback kind, not wording. Common mistakes and the full per-level guide are in DETAILS.md § Toast system.
- **A full toast stack of all-persistent toasts silently drops new toasts.** Intentional: persistent toasts hold
  important state and shouldn't be evicted by transient feedback.
- **Toast action buttons use `Button` mini in a right-aligned `.actions` row** (default action far right, macOS
  convention). Don't hand-roll bespoke `<button>`s; `DownloadToastContent` is the reference. Filled `variant="primary"`
  is reserved for the one genuinely affirmative action; dismiss/cancel alternatives and lone soft opt-outs stay
  `secondary` (a filled opt-out reads as a loud "do this!"). Full variant guide in DETAILS.md § Toast system.
- **`StatusBadge` class is `feature-status-badge`, NOT `status-badge`** (the Debug window has a `:global(.status-badge)`
  that would leak onto it). Derive the status via `getBadgeStatus(featureId)`, never hardcode it.
- **`containerStyle` exists because stylelint blocks non-token CSS custom properties.** Use it for one-off layout sizing
  (width/max-width), not for anything that belongs in the design-token system.
- **When adding a primitive,** add it to the Components catalog (`routes/dev/components/`) and a tier-3 a11y test. Full
  checklist in DETAILS.md.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it in whole before structural changes here.
