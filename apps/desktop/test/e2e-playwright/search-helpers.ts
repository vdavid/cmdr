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
import { dismissOverlay, dispatchMenuCommand, escapeOverlayUntilGone, pollUntil, pressKey } from './helpers.js'

export type PageLike = TauriPage | BrowserPageAdapter

/**
 * The query dialog's overlay. `QueryDialog` is a `ModalDialog` that adds this class via
 * `overlayClass`, so the element is BOTH `.modal-overlay` and `.search-overlay`; this one
 * names "a query dialog" across all three of its dialog ids, which `data-dialog-id` can't.
 */
export const SEARCH_OVERLAY = '.search-overlay'
export const SEARCH_INPUT = '.search-overlay .query-bar input.text-field-control'
/**
 * Active mode chip in the dialog's `role="tablist"`. `ModeChips.svelte` is backed by
 * `lib/ui/ToggleGroup.svelte` (semantics='tabs'), which renders `.tg-item` cells with
 * `aria-selected="true"` for the active one and `.tg-label` for the inner label.
 */
export const ACTIVE_MODE_CHIP = '.search-overlay .tg-item[aria-selected="true"]'
/** All mode chips in the dialog. Used to confirm the chip set (and indirectly, whether AI is on). */
export const MODE_CHIPS = '.search-overlay .tg-item'
/** The "Search in" filter chip, which opens the scope popover. Matches configured and default states. */
export const SCOPE_CHIP = '.search-overlay .chip[aria-label^="Search in"]'
/** The scope popover's body (`FilterPopover` renders the section class). */
export const SCOPE_POPOVER = '.scope-popover'

/** Opens the search dialog via the `search.open` registry command and waits for it to mount. */
export async function openSearchDialog(tauriPage: PageLike): Promise<void> {
  await dispatchMenuCommand(tauriPage, 'search.open')
  await tauriPage.waitForSelector(SEARCH_OVERLAY, 3000)
}

/**
 * Presses ⌘N (the dialog's "new search") and waits until nothing is running.
 *
 * The dialog's state survives close + reopen by design, so it reopens holding an
 * earlier spec's query, scope, and mode — and re-running that query is something it
 * may do on its own. For a live-walk spec that matters twice over: the leftover run
 * would satisfy every "a walk is going" assertion that follows, and the run under
 * test would be a second one nobody looked at. Waiting for the Stop button to be
 * gone AND the list to be empty is what makes the next Enter the only run on screen.
 */
export async function resetSearchDialog(tauriPage: PageLike): Promise<void> {
  await tauriPage.evaluate(`(function(){
        var overlay = document.querySelector(${JSON.stringify(SEARCH_OVERLAY)});
        if (overlay) overlay.dispatchEvent(new KeyboardEvent('keydown', { key: 'n', metaKey: true, bubbles: true, cancelable: true }));
    })()`)
  const quiet = await pollUntil(
    tauriPage,
    async () =>
      (await tauriPage.count(`${SEARCH_OVERLAY} .status-stop`)) === 0 &&
      (await tauriPage.count(`${SEARCH_OVERLAY} .result-row`)) === 0,
    10000,
  )
  if (!quiet) throw new Error('search dialog still had a run going 10s after ⌘N')
}

/**
 * Closes the dialog with Escape (the canonical close path) and waits for it to unmount.
 *
 * ❗ Named overlay, and re-pressed: a query dialog spends its first Escape stopping a live
 * run or handing the press to an open popover, so "one press, then wait" closes it only
 * when neither is in play. `escapeOverlayUntilGone` carries the full reasoning.
 */
export async function closeSearchDialog(tauriPage: PageLike): Promise<void> {
  await escapeOverlayUntilGone(tauriPage, SEARCH_OVERLAY)
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
  // Returns whether the input was there, so a missing box fails HERE. Swallowing it
  // leaves the dialog holding an earlier spec's query — which still runs, still lands
  // rows, and still satisfies "results arrived", so the test goes green measuring a
  // search nobody asked for.
  const typed = await tauriPage.evaluate<boolean>(`(function(){
        var el = document.querySelector(${JSON.stringify(SEARCH_INPUT)});
        if (!el) return false;
        el.focus();
        el.value = ${json};
        el.dispatchEvent(new Event('input', { bubbles: true }));
        return true;
    })()`)
  if (!typed) throw new Error(`setSearchInputValue: the search input was not on screen to receive ${json}`)
}

/**
 * Widens the dialog's scope to the whole volume — the ⌥V rung, the most a single
 * search can cover.
 *
 * A search covers one volume at most, and an unset scope means the FOCUSED PANE's
 * current folder, so a spec whose fixtures live outside that folder has to say so.
 * Clicking the footer button (rather than the ⌥V key combo) keeps the test off the
 * macOS Option-key glyph remapping.
 */
export async function scopeSearchToThisVolume(tauriPage: PageLike): Promise<void> {
  await tauriPage.click(SCOPE_CHIP)
  await tauriPage.waitForSelector(SCOPE_POPOVER, 3000)
  // Footer buttons in order: "Use current folder" (⌥C), "This volume" (⌥V).
  await tauriPage.click(`${SCOPE_POPOVER} .popover-footer .footer-button:nth-of-type(2)`)
  // `dismissOverlay` finds `.ui-popover` before `.search-overlay`, so it closes the
  // popover and leaves the dialog mounted (the chip-popover focus contract).
  await dismissOverlay(tauriPage)
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
