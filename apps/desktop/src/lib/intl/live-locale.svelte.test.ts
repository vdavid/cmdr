/**
 * Following a live OS language change: the user switches their macOS language
 * while Cmdr is running, and the app follows without a restart.
 *
 * `.svelte.` infix: the point of the feature is that open markup re-renders, so
 * the test mounts a real component reading `t()` and drives the event end to
 * end. The mirror property matters just as much and is invisible in the DOM: an
 * event that doesn't move the answer must not bump the version rune, or every
 * open `t()` in every window re-runs for nothing.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, unmount, flushSync } from 'svelte'

/** The handler `watchSystemUiLocale` registered, so a test can post an event. */
let onEvent: ((payload: { locale: string }) => void) | undefined
const unlisten = vi.fn()

vi.mock('$lib/tauri-commands', () => ({
  getUiLocale: () => Promise.resolve('en'),
  onUiLocaleChanged: (handler: (payload: { locale: string }) => void) => {
    onEvent = handler
    return Promise.resolve(unlisten)
  },
}))

import { pickUiLocale, watchSystemUiLocale, _setSystemUiLocaleForTests } from './ui-locale'
import { setLocale, _setCatalogForTests, _clearCompiledCacheForTests } from './messages.svelte'
import { _setLocaleForTests } from './locale'
import Fixture from './messages-reactivity-fixture.svelte'

/** A test-only catalog, so a switch actually CHANGES the rendered text. */
const TEST_LANG = 'zz'

/**
 * Mounts a component that reads `t()` in markup and starts following the OS,
 * wired exactly the way `settings-applier.ts` and `window-settings.ts` wire it:
 * re-apply the `appearance.language` setting whenever the OS answer moves.
 */
async function mountFollowing(language: string) {
  const target = document.createElement('div')
  document.body.appendChild(target)
  const component = mount(Fixture, { target, props: { messageKey: 'transfer.trash' } })
  flushSync()

  const applied = vi.fn()
  await watchSystemUiLocale(() => {
    applied()
    setLocale(pickUiLocale(language))
  })

  const text = (): string | null | undefined => target.querySelector('[data-test="trans-text"]')?.textContent
  return { component, target, applied, text }
}

beforeEach(() => {
  onEvent = undefined
  unlisten.mockClear()
  _setSystemUiLocaleForTests('en')
  _setCatalogForTests(TEST_LANG, { 'transfer.trash': 'SWITCHED {countText}' })
})

afterEach(() => {
  setLocale(null)
  _setLocaleForTests(null)
  _setSystemUiLocaleForTests(null)
  _setCatalogForTests(TEST_LANG, null)
  _clearCompiledCacheForTests()
})

describe('a live OS language change', () => {
  it('re-resolves and re-renders open markup when the answer moves', async () => {
    const { component, applied, text } = await mountFollowing('system')
    expect(text()).toBe('Moved 1 file to trash')

    onEvent?.({ locale: TEST_LANG })
    flushSync()

    expect(applied).toHaveBeenCalledTimes(1)
    expect(pickUiLocale('system')).toBe(TEST_LANG)
    expect(text()).toBe('SWITCHED 1')
    await unmount(component)
  })

  it('leaves the app alone when the answer is the one it is already running on', async () => {
    // The backend already drops these; the second guard is what keeps a stray
    // or replayed event from re-rendering every open `t()` for nothing.
    const { component, applied, text } = await mountFollowing('system')

    onEvent?.({ locale: 'en' })
    flushSync()

    expect(applied).not.toHaveBeenCalled()
    expect(text()).toBe('Moved 1 file to trash')
    await unmount(component)
  })

  it('still re-applies under an explicit language, so formatters re-key on the new OS locale', async () => {
    // The UI language is pinned, so the copy must not move; the rune bump is
    // for the formatters, which follow the OS whatever the setting says.
    const { component, applied, text } = await mountFollowing('en')

    onEvent?.({ locale: TEST_LANG })
    flushSync()

    expect(applied).toHaveBeenCalledTimes(1)
    expect(text()).toBe('Moved 1 file to trash')
    await unmount(component)
  })

  it('hands back an unlisten, so a closing window stops following', async () => {
    const stop = await watchSystemUiLocale(() => {})
    expect(stop).toBe(unlisten)
  })
})
