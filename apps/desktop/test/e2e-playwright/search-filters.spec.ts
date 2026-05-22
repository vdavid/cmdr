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
import { ensureAppReady, pollUntil, pressKey } from './helpers.js'
import { ensureMcpClient } from '../e2e-shared/mcp-client.js'
import { SEARCH_OVERLAY, closeSearchDialog, openSearchDialog } from './search-helpers.js'

const SIZE_CHIP_DEFAULT = '.search-overlay .filter-chip[aria-label="Size"]'
const SIZE_CHIP_CONFIGURED = '.search-overlay .filter-chip.is-configured'
const SIZE_CHIP_CLEAR = '.search-overlay .filter-chip.is-configured .chip-clear'
const FILTER_POPOVER = '.search-overlay .filter-chip-popover'
// Round 3 D10 replaced the `<select>` + number input chain with a list-style
// grid. Each comparator / preset / unit is a `role="radio"` button inside a
// labeled `role="radiogroup"` column. The "≥" cell sits in the Comparator
// column; the "100" cell sits in the "Minimum size value" column.
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
    // guard, see lib/search/CLAUDE.md). The dialog must stay open.
    await pressKey(tauriPage, 'Escape')
    const popoverGone = await pollUntil(tauriPage, async () => (await tauriPage.count(FILTER_POPOVER)) === 0, 2000)
    expect(popoverGone).toBe(true)
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
