/**
 * The demo menu bar's look: a macOS 27 menu bar and its Liquid Glass dropdowns, from the app's own
 * glass tokens (`app.css` § Frosted-glass material), so the bar follows the Liquid Glass slider and
 * reduce-transparency like every other glass surface. Measured against the real bar and an open
 * menu on macOS 27 (`screencapture`, 2026-11-04): a 30 px bar, 13 px text, capsule title
 * highlights, 12 px menu corners, 24 px rows inset by 5 px, the accent pill under the cursor row,
 * and dimmed shortcut glyphs right-aligned with a little air between them. Injected once.
 */
export const MENU_BAR_HEIGHT_PX = 30

export const MENU_BAR_STYLE = `
.cmdr-demo-menubar {
  flex: none;
  position: relative;
  z-index: calc(var(--z-modal-top) + 1);
  display: flex;
  align-items: center;
  gap: 2px;
  height: ${String(MENU_BAR_HEIGHT_PX)}px;
  padding: 0 10px;
  box-sizing: border-box;
  background: var(--color-bg-glass);
  -webkit-backdrop-filter: var(--glass-backdrop);
  backdrop-filter: var(--glass-backdrop);
  border-bottom: 0.5px solid var(--color-border-glass);
  color: var(--color-text-primary);
  font-family: var(--font-system);
  font-size: 13px;
  user-select: none;
  -webkit-user-select: none;
  cursor: default;
}
.cmdr-demo-menubar-title {
  display: flex;
  align-items: center;
  height: 24px;
  padding: 0 10px;
  border-radius: 999px;
  white-space: nowrap;
  outline: none;
}
.cmdr-demo-menubar-title.is-app { font-weight: 700; }
.cmdr-demo-menubar-title.is-open,
.cmdr-demo-menubar-title:focus-visible {
  background: color-mix(in srgb, var(--color-text-primary) 12%, transparent);
}
/* The app's two corners that sit against the page's top edge (the status corner resolves against
   the initial containing block, the toast stack is fixed): on the desktop that edge is the
   window's, here it's the bar's, so both move down by the bar or the bar covers them and eats
   their clicks. The same offsets as their own rules, plus the bar. \`html:root\` outranks the
   components' scoped classes. */
html:root .status-corner {
  top: calc(var(--cmdr-demo-window-top, 0px) + var(--spacing-sm) - var(--spacing-xxs));
}
html:root .toast-container:not(.at-bottom) {
  top: calc(var(--cmdr-demo-window-top, 0px) + var(--spacing-lg));
}
`

/**
 * An open menu's look, the bar's dropdowns and the right-click menus alike (`menu-surfaces.ts`):
 * macOS 27's Liquid Glass menu, plus the dimmed small header line a right-click menu leads with
 * (the desktop restyles that line the same way, `context_menu_header.rs`).
 */
export const MENU_SURFACE_STYLE = `
.cmdr-demo-menu {
  position: fixed;
  z-index: calc(var(--z-modal-top) + 2);
  min-width: 220px;
  max-height: calc(100vh - ${String(MENU_BAR_HEIGHT_PX + 8)}px);
  overflow-y: auto;
  box-sizing: border-box;
  padding: 5px;
  background: var(--color-bg-glass);
  -webkit-backdrop-filter: var(--glass-backdrop);
  backdrop-filter: var(--glass-backdrop);
  border: 0.5px solid var(--color-border-glass);
  border-radius: var(--radius-menu);
  box-shadow: var(--shadow-glass), var(--shadow-glass-rim);
  color: var(--color-text-primary);
  font-family: var(--font-system);
  font-size: 13px;
  user-select: none;
  -webkit-user-select: none;
  cursor: default;
  outline: none;
}
.cmdr-demo-menu-row {
  display: flex;
  align-items: center;
  height: 24px;
  padding: 0 10px 0 4px;
  border-radius: 7px;
  white-space: nowrap;
  outline: none;
}
.cmdr-demo-menu-row.is-highlighted {
  background: var(--color-system-accent);
  color: #fff;
}
.cmdr-demo-menu-row.is-disabled { color: var(--color-text-tertiary); opacity: 0.55; }
.cmdr-demo-menu-check {
  display: inline-flex;
  justify-content: center;
  width: 14px;
  margin-right: 4px;
  flex: none;
}
.cmdr-demo-menu-check svg { width: 11px; height: 11px; }
.cmdr-demo-menu-label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
.cmdr-demo-menu-shortcut {
  display: inline-flex;
  gap: 1px;
  margin-left: 28px;
  flex: none;
  color: var(--color-text-tertiary);
  font-variant-numeric: tabular-nums;
}
.cmdr-demo-menu-shortcut span { min-width: 0.9em; text-align: center; }
.cmdr-demo-menu-row.is-highlighted .cmdr-demo-menu-shortcut { color: rgb(255 255 255 / 0.85); }
.cmdr-demo-menu-arrow { display: inline-flex; margin-left: 28px; flex: none; color: var(--color-text-secondary); }
.cmdr-demo-menu-row.is-highlighted .cmdr-demo-menu-arrow { color: #fff; }
.cmdr-demo-menu-arrow svg { width: 7px; height: 11px; }
.cmdr-demo-menu-separator {
  height: 1px;
  margin: 5px 10px;
  background: var(--color-border-glass);
}
.cmdr-demo-menu-header {
  display: flex;
  align-items: center;
  height: 22px;
  max-width: 320px;
  padding: 0 10px 0 22px;
  overflow: hidden;
  color: var(--color-text-secondary);
  font-size: 11px;
  white-space: nowrap;
  text-overflow: ellipsis;
}
`
