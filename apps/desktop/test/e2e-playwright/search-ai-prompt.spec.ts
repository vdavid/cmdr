/**
 * Search dialog: AI mode auto-apply contract.
 *
 * Per plan §3.6 and lib/search/CLAUDE.md's "AI single-pass flow": AI mode
 * NEVER auto-applies. The user must press Enter / ⌘Enter / click the ⏎ run
 * button. Filename + Regex modes auto-apply on a 1 s debounce (gated by the
 * `search.autoApply` setting, default on).
 *
 * Mocking the AI provider end-to-end in the Playwright fixture is non-trivial
 * (the AI server lives in a separate process and the dialog calls
 * `translateSearchQuery` IPC which routes through the registry-driven cloud
 * config). So we test the explicit-trigger contract directly: type into the
 * AI input, wait past the debounce window, and assert that no AI translation
 * happened — observable as the input value still matching what we typed (an
 * AI run would overwrite `query` with the translated pattern, per the
 * "AI overwrites the bar" decision documented in lib/search/CLAUDE.md).
 *
 * When AI is off in the test fixture (the only chip is Filename / Regex), the
 * test self-skips with a clear note: there's nothing meaningful to test.
 */

import { test } from './fixtures.js'
import { ensureAppReady, sleep } from './helpers.js'
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

test.describe('Search dialog: AI mode never auto-applies', () => {
  test('typing in AI mode does not run a search until the user explicitly triggers it', async ({ tauriPage }) => {
    await ensureAppReady(tauriPage)
    await ensureMcpClient(tauriPage)
    await openSearchDialog(tauriPage)

    const aiOn = await hasAiChip(tauriPage)
    test.skip(!aiOn, 'AI provider is off in this fixture; AI-mode contract has no observable surface')

    // Switch to AI mode (⌘1 with AI on per modeForShortcutNumber). If the
    // dialog already opened on AI, the press is a no-op.
    if ((await getActiveMode(tauriPage)) !== 'ai') {
      await pressMetaDigit(tauriPage, 1)
      const switched = await pollActiveMode(tauriPage, 'ai')
      test.skip(!switched, 'failed to switch to AI mode; cannot validate the contract')
    }

    const prompt = 'find very large pdf files from last week'
    await setSearchInputValue(tauriPage, prompt)

    // Wait past the auto-apply debounce window (1 s per
    // SEARCH_AUTO_APPLY_DEBOUNCE_MS) plus a small buffer. This is the rare
    // legitimate fixed wait: we're asserting a *negative* (nothing fires),
    // and "nothing fires" has no observable signal to poll on. We pick the
    // shortest sleep that's strictly larger than the debounce window.
    // eslint-disable-next-line cmdr/no-arbitrary-sleep-in-e2e -- asserting a negative; nothing to poll for
    await sleep(1500)

    const after = await getSearchInputValue(tauriPage)
    // We don't assert anything about the result set — that needs a real or
    // stubbed AI provider. We do assert the input wasn't mutated, which is
    // the observable signature of an AI run that landed.
    if (after !== prompt) {
      throw new Error(
        `AI mode auto-applied: input was rewritten from ${JSON.stringify(prompt)} to ${JSON.stringify(after)}`,
      )
    }

    await closeSearchDialog(tauriPage)
  })
})
