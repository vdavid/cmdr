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
import { ensureAppReady, pressKey } from './helpers.js'
import { ensureMcpClient } from '../e2e-shared/mcp-client.js'
import {
  closeSearchDialog,
  getActiveMode,
  getSearchInputValue,
  hasAiChip,
  openSearchDialog,
  pollActiveMode,
  pressMetaDigit,
  SEARCH_INPUT,
  setSearchInputValue,
} from './search-helpers.js'

test.describe('Search dialog: mode shortcuts', () => {
  // Per-mode hand-typed buffers (`handTyped.ai` / `handTyped.filename` / `handTyped.regex`):
  // switching modes restores the target mode's buffer. Two rules combine (M5): a NON-empty
  // target buffer is preserved (it's the user's own prior text for that mode), and an EMPTY
  // target buffer is seeded with the outgoing term so the user's words follow them across the
  // switch instead of vanishing. This pins both, plus the round-trip ("buffer for mode X
  // survives a detour through modes Y and Z").
  test('⌘1/⌘2/⌘3 switch modes: carry the term into an empty mode, preserve a non-empty one', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    await openSearchDialog(tauriPage)

    // Focus the search input deterministically: `openSearchDialog` only waits
    // for the overlay to mount, and the dialog's own `focusInput` runs after an
    // async `tick`. If we race that with key presses (the OS keystroke targets
    // `document.activeElement`, which falls back to the previous focus owner),
    // ⌘N / ⌘1 / etc. land outside the dialog and never reach `handleKeyDown`.
    await tauriPage.evaluate(`(function(){
        var el = document.querySelector(${JSON.stringify(SEARCH_INPUT)});
        if (el && typeof el.focus === 'function') el.focus();
    })()`)
    await tauriPage.waitForFunction(
      `(function(){ var i = document.querySelector(${JSON.stringify(SEARCH_INPUT)}); return i !== null && document.activeElement === i; })()`,
      3000,
    )

    // The dialog's module-level state survives close+reopen by design (see
    // `search/CLAUDE.md` § State preservation), so prior tests in the shard
    // can leave the dialog in any mode + any per-mode buffer. ⌘N clears
    // everything (query, mode -> filename, all `handTyped` buffers, AI
    // remembered fields) so the rest of this test starts from a known state.
    await pressKey(tauriPage, 'Meta+n')
    expect(await pollActiveMode(tauriPage, 'filename')).toBe(true)

    const aiOn = await hasAiChip(tauriPage)
    if (aiOn) {
      // Switch to AI mode explicitly. ⌘N resets mode to filename regardless of
      // whether AI is enabled, so this hop is the only way to seed an AI prompt.
      await pressMetaDigit(tauriPage, 1)
      expect(await pollActiveMode(tauriPage, 'ai')).toBe(true)
      await setSearchInputValue(tauriPage, 'ai-prompt-marker')
      expect(await getSearchInputValue(tauriPage)).toBe('ai-prompt-marker')

      // ⌘2 → filename. The filename buffer is empty, so the outgoing term carries
      // over (M5: switching to an empty mode seeds it with the current text rather
      // than losing the user's words).
      await pressMetaDigit(tauriPage, 2)
      expect(await pollActiveMode(tauriPage, 'filename')).toBe(true)
      expect(await getSearchInputValue(tauriPage)).toBe('ai-prompt-marker')

      // Replace it with a real filename buffer.
      await setSearchInputValue(tauriPage, 'filename-marker')
      expect(await getSearchInputValue(tauriPage)).toBe('filename-marker')

      // ⌘3 → regex. Empty regex buffer, so `filename-marker` carries over.
      await pressMetaDigit(tauriPage, 3)
      expect(await pollActiveMode(tauriPage, 'regex')).toBe(true)
      expect(await getSearchInputValue(tauriPage)).toBe('filename-marker')

      // ⌘1 → AI. The AI buffer is non-empty (`ai-prompt-marker`), so it's preserved,
      // NOT overwritten by the carried-over text.
      await pressMetaDigit(tauriPage, 1)
      expect(await pollActiveMode(tauriPage, 'ai')).toBe(true)
      expect(await getSearchInputValue(tauriPage)).toBe('ai-prompt-marker')

      // ⌘2 → filename again. The filename buffer (`filename-marker`) survived the detour.
      await pressMetaDigit(tauriPage, 2)
      expect(await pollActiveMode(tauriPage, 'filename')).toBe(true)
      expect(await getSearchInputValue(tauriPage)).toBe('filename-marker')
    } else {
      // AI off: ⌘1 = Filename, ⌘2 = Regex, ⌘3 no-op.
      expect(await getActiveMode(tauriPage)).toBe('filename')

      await setSearchInputValue(tauriPage, 'filename-marker')
      expect(await getSearchInputValue(tauriPage)).toBe('filename-marker')

      // ⌘2 → regex. Empty regex buffer, so `filename-marker` carries over (M5).
      await pressMetaDigit(tauriPage, 2)
      expect(await pollActiveMode(tauriPage, 'regex')).toBe(true)
      expect(await getSearchInputValue(tauriPage)).toBe('filename-marker')

      // Replace it with a real regex buffer.
      await setSearchInputValue(tauriPage, 'regex-marker')
      expect(await getSearchInputValue(tauriPage)).toBe('regex-marker')

      // ⌘1 → filename. The filename buffer (`filename-marker`) survived the detour
      // (non-empty target buffers are never overwritten by carry-over).
      await pressMetaDigit(tauriPage, 1)
      expect(await pollActiveMode(tauriPage, 'filename')).toBe(true)
      expect(await getSearchInputValue(tauriPage)).toBe('filename-marker')
    }

    await closeSearchDialog(tauriPage)
  })
})
