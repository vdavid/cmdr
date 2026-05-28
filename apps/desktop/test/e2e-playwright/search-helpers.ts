/**
 * Shared helpers for the `search-*.spec.ts` Playwright specs.
 *
 * Extracted so multiple specs can reuse the open / close / type / mode-detect
 * primitives without copy-paste drift. The Open-in-pane spec is older and
 * keeps its inline helpers since they're tightly coupled to the snapshot /
 * pane state model unique to that test; the chips / filters / recent / AI /
 * dialog-open specs all share the helpers below.
 */

import type { TauriPage, BrowserPageAdapter } from '@srsholmes/tauri-playwright'
import { dispatchMenuCommand, pollUntil, pressKey } from './helpers.js'

export type PageLike = TauriPage | BrowserPageAdapter

export const SEARCH_OVERLAY = '.search-overlay'
export const SEARCH_INPUT = '.search-overlay input.query-input'
/**
 * Active mode chip in the dialog's `role="tablist"`. `ModeChips.svelte` is backed by
 * `lib/ui/ToggleGroup.svelte` (semantics='tabs'), which renders `.tg-item` cells with
 * `aria-selected="true"` for the active one and `.tg-label` for the inner label.
 */
export const ACTIVE_MODE_CHIP = '.search-overlay .tg-item[aria-selected="true"]'
/** All mode chips in the dialog. Used to confirm the chip set (and indirectly, whether AI is on). */
export const MODE_CHIPS = '.search-overlay .tg-item'

/** Opens the search dialog via the `search.open` registry command and waits for it to mount. */
export async function openSearchDialog(tauriPage: PageLike): Promise<void> {
  await dispatchMenuCommand(tauriPage, 'search.open')
  await tauriPage.waitForSelector(SEARCH_OVERLAY, 3000)
}

/** Closes the dialog with Escape (the canonical close path) and waits for it to unmount. */
export async function closeSearchDialog(tauriPage: PageLike): Promise<void> {
  await pressKey(tauriPage, 'Escape')
  const gone = await pollUntil(tauriPage, async () => (await tauriPage.count(SEARCH_OVERLAY)) === 0, 3000)
  if (!gone) throw new Error('search overlay still mounted 3s after Escape')
}

/**
 * Sets the search input's value via direct DOM mutation + `input` event so the
 * bound `query` state updates. Use this for tests that need a deterministic
 * "this is what's in the input" without typing one character at a time
 * (the dialog's 1 s debounce makes synthetic char-by-char typing both slow
 * and flaky).
 */
export async function setSearchInputValue(tauriPage: PageLike, value: string): Promise<void> {
  const json = JSON.stringify(value)
  await tauriPage.evaluate(`(function(){
        var el = document.querySelector(${JSON.stringify(SEARCH_INPUT)});
        if (!el) return;
        el.focus();
        el.value = ${json};
        el.dispatchEvent(new Event('input', { bubbles: true }));
    })()`)
}

/** Returns the current value of the dialog's search input. Empty string if absent. */
export async function getSearchInputValue(tauriPage: PageLike): Promise<string> {
  return tauriPage.evaluate<string>(`(function(){
        var el = document.querySelector(${JSON.stringify(SEARCH_INPUT)});
        return el ? el.value : '';
    })()`)
}

/**
 * Returns the active mode chip's label as one of `'ai' | 'filename' | 'regex' | null`.
 *
 * Infers from the chip's label text (`.tg-label`, rendered by
 * `lib/ui/ToggleGroup.svelte` via `ModeChips.svelte`). `'ai'` corresponds to
 * "Ask anything" (AI chip's label); `'filename'` / `'regex'` match the chip
 * labels verbatim. Returns null when no chip is active (shouldn't happen for
 * an open dialog; treat as a test bug).
 */
export async function getActiveMode(tauriPage: PageLike): Promise<'ai' | 'filename' | 'regex' | null> {
  const label = await tauriPage.evaluate<string>(`(function(){
        var chip = document.querySelector(${JSON.stringify(ACTIVE_MODE_CHIP)});
        if (!chip) return '';
        var labelEl = chip.querySelector('.tg-label');
        return (labelEl ? labelEl.textContent : '').trim();
    })()`)
  if (label === 'Ask anything') return 'ai'
  if (label === 'Filename') return 'filename'
  if (label === 'Regex') return 'regex'
  return null
}

/**
 * Returns true when the dialog's mode-chip row includes the AI chip
 * ("Ask anything"). Used to decide whether `⌘1` lands on AI or on Filename
 * in the test fixture, since the dialog reorders chips based on whether AI
 * is enabled.
 */
export async function hasAiChip(tauriPage: PageLike): Promise<boolean> {
  return tauriPage.evaluate<boolean>(`(function(){
        var chips = document.querySelectorAll(${JSON.stringify(MODE_CHIPS)});
        for (var i = 0; i < chips.length; i++) {
            var labelEl = chips[i].querySelector('.tg-label');
            if (labelEl && (labelEl.textContent || '').trim() === 'Ask anything') return true;
        }
        return false;
    })()`)
}

/**
 * Dispatches a ⌘<digit> key combo at the focused element (the search input,
 * after `openSearchDialog`). `pressKey` already handles modifier flags; we
 * call it with the explicit `Meta+<digit>` form so the dialog's
 * `handleModeShortcut` reads `e.metaKey && e.key === '<digit>'`.
 */
export async function pressMetaDigit(tauriPage: PageLike, digit: 1 | 2 | 3): Promise<void> {
  await pressKey(tauriPage, `Meta+${String(digit)}`)
}

/**
 * Polls until the active mode equals `expected`. Useful right after a
 * `⌘<digit>` press since the chip-class flip happens on the next render tick.
 */
export async function pollActiveMode(
  tauriPage: PageLike,
  expected: 'ai' | 'filename' | 'regex',
  timeoutMs = 1500,
): Promise<boolean> {
  return pollUntil(tauriPage, async () => (await getActiveMode(tauriPage)) === expected, timeoutMs)
}
