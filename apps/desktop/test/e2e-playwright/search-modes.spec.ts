/**
 * Search dialog: mode switching via ⌘1 / ⌘2 / ⌘3 and query preservation.
 *
 * The chip set the dialog renders depends on whether AI is enabled in the
 * test fixture (provider != 'off' AND index available). The dialog's
 * `modeForShortcutNumber()` keeps the digits aligned with the visible chip
 * positions:
 *   - AI on:  ⌘1 = AI, ⌘2 = Filename, ⌘3 = Regex
 *   - AI off: ⌘1 = Filename, ⌘2 = Regex, ⌘3 no-op
 *
 * We don't assume which lane the fixture is in. We read it once via
 * `hasAiChip()` and pick the matching expectation per press. The query the
 * user typed must survive every mode switch (per plan §3.1's "the input is
 * one model" contract).
 */

import { test, expect } from './fixtures.js'
import { ensureAppReady } from './helpers.js'
import { ensureMcpClient } from '../e2e-shared/mcp-client.js'
import {
  closeSearchDialog,
  getActiveMode,
  getSearchInputValue,
  hasAiChip,
  openSearchDialog,
  pollActiveMode,
  pressMetaDigit,
  setSearchInputValue,
} from './search-helpers.js'

test.describe('Search dialog: mode shortcuts', () => {
  test('⌘1/⌘2/⌘3 switch modes and preserve the typed query', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    await openSearchDialog(tauriPage)

    // Seed a query that has no chance of matching anything (we don't care about
    // results here; we only care that the value sticks across mode switches).
    await setSearchInputValue(tauriPage, 'zzz-xyz-marker')
    expect(await getSearchInputValue(tauriPage)).toBe('zzz-xyz-marker')

    const aiOn = await hasAiChip(tauriPage)
    if (aiOn) {
      await pressMetaDigit(tauriPage, 2)
      expect(await pollActiveMode(tauriPage, 'filename')).toBe(true)
      expect(await getSearchInputValue(tauriPage)).toBe('zzz-xyz-marker')

      await pressMetaDigit(tauriPage, 3)
      expect(await pollActiveMode(tauriPage, 'regex')).toBe(true)
      expect(await getSearchInputValue(tauriPage)).toBe('zzz-xyz-marker')

      await pressMetaDigit(tauriPage, 1)
      expect(await pollActiveMode(tauriPage, 'ai')).toBe(true)
      expect(await getSearchInputValue(tauriPage)).toBe('zzz-xyz-marker')
    } else {
      // AI off: ⌘1 = Filename, ⌘2 = Regex, ⌘3 no-op. The dialog opens on
      // Filename by default in this lane.
      expect(await getActiveMode(tauriPage)).toBe('filename')

      await pressMetaDigit(tauriPage, 2)
      expect(await pollActiveMode(tauriPage, 'regex')).toBe(true)
      expect(await getSearchInputValue(tauriPage)).toBe('zzz-xyz-marker')

      await pressMetaDigit(tauriPage, 1)
      expect(await pollActiveMode(tauriPage, 'filename')).toBe(true)
      expect(await getSearchInputValue(tauriPage)).toBe('zzz-xyz-marker')
    }

    await closeSearchDialog(tauriPage)
  })
})
