/**
 * Search dialog: filter chips.
 *
 * Exercises the Size chip (the simplest configured-state shape): default
 * state → open popover → set min value → confirm via input change → chip
 * shows the configured summary → × clears it. Also pins the Escape-scoped
 * close contract (`SearchFilterChips.svelte` documents this: Esc inside the
 * popover closes only the popover, not the dialog).
 *
 * The Modified and Search-in chips share the same popover primitive, so
 * covering one chip end-to-end is enough at the E2E tier; the per-chip shape
 * differences are pinned by the Vitest unit tests in
 * `SearchFilterChips.svelte.test.ts`.
 */

import { test, expect } from './fixtures.js'
import { dismissOverlay, ensureAppReady, pollUntil } from './helpers.js'
import { ensureMcpClient } from '../e2e-shared/mcp-client.js'
import { SEARCH_OVERLAY, closeSearchDialog, openSearchDialog } from './search-helpers.js'

const SIZE_CHIP_DEFAULT = '.search-overlay .chip-filter[aria-label="Size"]'
const SIZE_CHIP_CONFIGURED = '.search-overlay .chip-filter.is-configured'
const SIZE_CHIP_CLEAR = '.search-overlay .chip-filter.is-configured .chip-clear'
const FILTER_POPOVER = '.search-overlay .ui-popover'
// The Size popover renders as a list-style grid: each comparator / preset / unit is a
// `role="radio"` button inside a labeled `role="radiogroup"` column. The "≥" cell sits
// in the Comparator column; the "100" cell sits in the "Minimum size value" column.
const SIZE_COMPARATOR_GTE = `${FILTER_POPOVER} [role="radiogroup"][aria-label="Comparator"] button[role="radio"]:nth-child(2)`
const SIZE_VALUE_100 = `${FILTER_POPOVER} [role="radiogroup"][aria-label="Minimum size value"] button[role="radio"]:nth-child(8)`

test.describe('Search dialog: filter chips', () => {
  test('Size chip: open popover, set min, confirm, clear via ×', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    await openSearchDialog(tauriPage)

    // Default state: chip exists, no `.is-configured` modifier.
    expect(await tauriPage.count(SIZE_CHIP_DEFAULT)).toBe(1)
    expect(await tauriPage.count(SIZE_CHIP_CONFIGURED)).toBe(0)

    // Click the Size chip → popover opens.
    await tauriPage.click(SIZE_CHIP_DEFAULT)
    await tauriPage.waitForSelector(FILTER_POPOVER, 2000)

    // Pick the ≥ comparator from the first list column.
    await tauriPage.waitForSelector(SIZE_COMPARATOR_GTE, 2000)
    await tauriPage.click(SIZE_COMPARATOR_GTE)

    // Pick the "100" preset from the second list column. The chip's configured
    // summary reads back from the same state, so we poll the chip class.
    await tauriPage.click(SIZE_VALUE_100)
    const configured = await pollUntil(tauriPage, async () => (await tauriPage.count(SIZE_CHIP_CONFIGURED)) === 1, 2000)
    expect(configured).toBe(true)

    // Esc closes ONLY the popover (`SearchFilterChips.svelte` capture-phase
    // guard, see lib/search/CLAUDE.md); the dialog must stay open.
    // `dismissOverlay` dispatches a synthetic Escape on the `.ui-popover`
    // element itself (first in the overlay priority list) and asserts it closed.
    // Using it instead of `pressKey('Escape')` — which targets
    // `document.activeElement` — removes a focus-position dependency that flaked
    // under Linux Xvfb when focus wasn't inside the chip subtree.
    await dismissOverlay(tauriPage)
    expect(await tauriPage.count(SEARCH_OVERLAY)).toBe(1)

    // Click × to clear. The chip drops `is-configured` and the value vanishes
    // from the popover state (re-opening would show comparator `any`).
    await tauriPage.evaluate(`(function(){
        var x = document.querySelector(${JSON.stringify(SIZE_CHIP_CLEAR)});
        if (x) x.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    })()`)
    const cleared = await pollUntil(tauriPage, async () => (await tauriPage.count(SIZE_CHIP_CONFIGURED)) === 0, 2000)
    expect(cleared).toBe(true)

    await closeSearchDialog(tauriPage)
  })
})
