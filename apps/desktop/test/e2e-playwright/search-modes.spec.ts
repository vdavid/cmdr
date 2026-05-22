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
  // Round 2 part B introduced per-mode hand-typed buffers (`handTyped.ai`,
  // `handTyped.filename`, `handTyped.regex`). Switching modes now SWAPS the
  // bar to the target mode's buffer, not the same shared string. The original
  // test asserted the shared-buffer contract; updated below to pin the
  // per-mode behavior plus the round-trip ("buffer for mode X survives a
  // detour through modes Y and Z").
  test('⌘1/⌘2/⌘3 switch modes and preserve each mode\'s own typed query', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    await openSearchDialog(tauriPage)

    const aiOn = await hasAiChip(tauriPage)
    if (aiOn) {
      // Open lands on AI. Seed an AI-mode query.
      expect(await getActiveMode(tauriPage)).toBe('ai')
      await setSearchInputValue(tauriPage, 'ai-prompt-marker')
      expect(await getSearchInputValue(tauriPage)).toBe('ai-prompt-marker')

      // ⌘2 → filename. Buffer is empty (never typed in filename mode here).
      await pressMetaDigit(tauriPage, 2)
      expect(await pollActiveMode(tauriPage, 'filename')).toBe(true)
      expect(await getSearchInputValue(tauriPage)).toBe('')

      // Seed a filename buffer.
      await setSearchInputValue(tauriPage, 'filename-marker')
      expect(await getSearchInputValue(tauriPage)).toBe('filename-marker')

      // ⌘3 → regex. Empty again.
      await pressMetaDigit(tauriPage, 3)
      expect(await pollActiveMode(tauriPage, 'regex')).toBe(true)
      expect(await getSearchInputValue(tauriPage)).toBe('')

      // ⌘1 → AI. The original AI prompt is back.
      await pressMetaDigit(tauriPage, 1)
      expect(await pollActiveMode(tauriPage, 'ai')).toBe(true)
      expect(await getSearchInputValue(tauriPage)).toBe('ai-prompt-marker')

      // ⌘2 → filename again. The filename buffer survived the detour.
      await pressMetaDigit(tauriPage, 2)
      expect(await pollActiveMode(tauriPage, 'filename')).toBe(true)
      expect(await getSearchInputValue(tauriPage)).toBe('filename-marker')
    } else {
      // AI off: ⌘1 = Filename, ⌘2 = Regex, ⌘3 no-op.
      expect(await getActiveMode(tauriPage)).toBe('filename')

      await setSearchInputValue(tauriPage, 'filename-marker')
      expect(await getSearchInputValue(tauriPage)).toBe('filename-marker')

      // ⌘2 → regex. Empty buffer.
      await pressMetaDigit(tauriPage, 2)
      expect(await pollActiveMode(tauriPage, 'regex')).toBe(true)
      expect(await getSearchInputValue(tauriPage)).toBe('')

      await setSearchInputValue(tauriPage, 'regex-marker')
      expect(await getSearchInputValue(tauriPage)).toBe('regex-marker')

      // ⌘1 → filename. The filename buffer survived the detour.
      await pressMetaDigit(tauriPage, 1)
      expect(await pollActiveMode(tauriPage, 'filename')).toBe(true)
      expect(await getSearchInputValue(tauriPage)).toBe('filename-marker')
    }

    await closeSearchDialog(tauriPage)
  })
})
