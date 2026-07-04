/**
 * Cursor and pane-focus helpers for the Cmdr Playwright E2E tests.
 *
 * Move the cursor to a named file (`moveCursorToFile`), focus a pane
 * (`focusPane`), skip the ".." parent entry (`skipParentEntry`), and re-anchor
 * keyboard focus on the explorer after MCP-driven actions (`ensureExplorerFocused`).
 */

import { expect } from '@playwright/test'
import { ensureMcpClient, mcpCall } from '../../e2e-shared/mcp-client.js'
import { type PageLike, findFileIndex, pollUntil } from './core.js'

// ── Cursor helpers ──────────────────────────────────────────────────────────

/** If the cursor is on the ".." parent entry, moves it down one position. */
export async function skipParentEntry(tauriPage: PageLike): Promise<void> {
  const cursorText = await tauriPage.evaluate<string>(`(function() {
        var entry = document.querySelector('.file-entry.is-under-cursor');
        if (!entry) return '';
        return entry.getAttribute('data-filename') || '';
    })()`)
  if (cursorText === '..') {
    await tauriPage.keyboard.press('ArrowDown')
    const moved = await pollUntil(
      tauriPage,
      async () => {
        const name = await tauriPage.evaluate<string>(`(function() {
                    var entry = document.querySelector('.file-entry.is-under-cursor');
                    if (!entry) return '';
                    return entry.getAttribute('data-filename') || '';
                })()`)
        return name !== '..'
      },
      8000,
    )
    if (!moved) {
      throw new Error('skipParentEntry: cursor did not leave the ".." entry within 8s after ArrowDown')
    }
  }
}

/**
 * Restores focus to `.dual-pane-explorer` (the keydown-handler host) before
 * an OS-keyboard test press. After MCP-driven actions like `move_cursor` the
 * focused element can drift to `<body>`, in which case `tauriPage.keyboard.press`
 * never reaches the explorer's handler. Mirrors the focus-recovery loop in
 * `ensureAppReady`, scoped to a single explicit call from a test mid-flow.
 *
 * Throws via `expect.poll` if focus can't be returned to the explorer in 2s
 * (an unexpected modal overlay or a missing explorer would do it).
 */
export async function ensureExplorerFocused(tauriPage: PageLike): Promise<void> {
  await expect
    .poll(
      async () =>
        tauriPage.evaluate<boolean>(`(function(){
          var ae = document.activeElement;
          if (!ae || !ae.closest || ae.closest('.dual-pane-explorer') === null) {
            var explorer = document.querySelector('.dual-pane-explorer');
            if (explorer) explorer.focus();
            ae = document.activeElement;
          }
          return !!(ae && ae.closest && ae.closest('.dual-pane-explorer') !== null);
        })()`),
      { timeout: 2000 },
    )
    .toBeTruthy()
}

/**
 * Focuses the requested pane (0 = left, 1 = right) so subsequent keyboard
 * input reaches the pane's handler. After MCP-driven actions like `move_cursor`,
 * `mcp-volume-select`, or `mcp-nav-to-path`, the DOM `.file-pane.is-focused`
 * marker can be absent (no pane is the "active" one), which means
 * `DualPaneExplorer.handleKeyDown`'s `getPaneRef(focusedPane)` returns undefined
 * and the keydown handler no-ops. We click the pane directly — the same gesture
 * a real user would use — to set focus deterministically, then poll the
 * `.is-focused` marker to confirm the click landed.
 *
 * Replaces the "toggle twice" idiom that depended on a specific starting
 * focus state (two `switch_pane` toggles only return to the originally-focused
 * pane; if no pane is focused, two toggles do nothing).
 */
export async function focusPane(tauriPage: PageLike, paneIndex: 0 | 1): Promise<void> {
  // Click the pane to set focus. Click coordinates come from the pane's bounding
  // rect (some inset to avoid hitting edge handles). The click is dispatched as
  // a synthetic MouseEvent so we don't depend on the OS pointer position.
  await tauriPage.evaluate(`(function(){
    var pane = document.querySelectorAll('.file-pane')[${String(paneIndex)}];
    if (!pane) throw new Error('pane ' + ${String(paneIndex)} + ' not found');
    var rect = pane.getBoundingClientRect();
    var x = rect.left + Math.min(40, rect.width / 2);
    var y = rect.top + Math.min(40, rect.height / 2);
    var opts = { bubbles: true, cancelable: true, view: window, clientX: x, clientY: y, button: 0 };
    pane.dispatchEvent(new MouseEvent('mousedown', opts));
    pane.dispatchEvent(new MouseEvent('mouseup', opts));
    pane.dispatchEvent(new MouseEvent('click', opts));
  })()`)
  await expect
    .poll(
      async () =>
        tauriPage.evaluate<boolean>(
          `document.querySelectorAll('.file-pane')[${String(paneIndex)}]?.classList.contains('is-focused') === true`,
        ),
      { timeout: 3000 },
    )
    .toBeTruthy()
}

/**
 * Moves the cursor to a specific file by name in the focused pane.
 *
 * Uses the MCP `move_cursor` tool, which jumps directly to the target file
 * instead of pressing ArrowDown N times. Falls back to `false` if the file
 * isn't in the focused pane's listing (matching the prior behavior).
 *
 * The focused-pane detection reads `.file-pane.is-focused` from the DOM, so
 * the signature stays compatible with the old keyboard-based version. Tests
 * that explicitly exercise arrow-key cursor movement (`app.spec.ts`) keep
 * their own keyboard-driven helper and don't use this function.
 */
export async function moveCursorToFile(tauriPage: PageLike, targetName: string): Promise<boolean> {
  // Wait for the target to appear in the focused pane's listing instead of
  // reading once. A file-op spec's `recreateFixtures` (beforeEach) deletes then
  // recreates `left/` on disk; the file watcher's debounced remove/create diffs
  // can drain just AFTER `ensureAppReady`'s files-present poll, briefly emptying
  // the pane (observed: left pane focused, `pane0Count` 0). A one-shot read
  // caught that reload window and returned false. Polling waits out the transient
  // empty; a file that's genuinely absent still fails after the deadline (returns
  // false, no masking). 5 s is failure headroom under parallel-shard load, well
  // within the 15 s per-test budget.
  const listed = await pollUntil(
    tauriPage,
    async () => {
      const probe = await findFileIndex(tauriPage, targetName)
      return !('error' in probe) && probe.targetIndex >= 0
    },
    5000,
  )
  if (!listed) return false

  // Determine which pane is focused so we can target the right one via MCP.
  const focusedPane = await tauriPage.evaluate<'left' | 'right' | null>(`(function() {
        var panes = document.querySelectorAll('.file-pane');
        for (var i = 0; i < panes.length; i++) {
            if (panes[i].classList.contains('is-focused')) {
                return i === 0 ? 'left' : 'right';
            }
        }
        return null;
    })()`)
  const pane: 'left' | 'right' = focusedPane ?? 'left'

  await ensureMcpClient(tauriPage)
  await mcpCall('move_cursor', { pane, filename: targetName })

  // Confirm the cursor landed on the target file. `move_cursor` round-trips AND
  // flushes the backend PaneStateStore before replying (DualPaneExplorer.moveCursor
  // → syncStateToMcpNow), so the backend cursor is fresh by the time this returns —
  // a follow-up copy/move/delete won't read a stale cursor-on-`..`. This poll only
  // covers the DOM render tick on a green run — it resolves the moment the cursor
  // lands and never reaches the budget. 8 s is failure headroom for the shared
  // Docker VM under load, where the render tick stretches.
  // Safe to keep at 8 s: this is the only bumped budget inside moveCursorToFile,
  // and the helper isn't called from ensureAppReady, so it never stacks.
  return pollUntil(
    tauriPage,
    async () => {
      return tauriPage.evaluate<boolean>(`(function() {
                var pane = document.querySelector('.file-pane.is-focused');
                if (!pane) return false;
                var entry = pane.querySelector('.file-entry.is-under-cursor');
                if (!entry) return false;
                return entry.getAttribute('data-filename') === ${JSON.stringify(targetName)};
            })()`)
    },
    8000,
  )
}
