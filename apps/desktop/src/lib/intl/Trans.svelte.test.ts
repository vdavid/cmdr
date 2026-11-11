/**
 * Proof tests for `<Trans>`: a catalog
 * sentence with an inline `<tag>` renders as text nodes plus a REAL interactive
 * Svelte component, in the locale's word order, with no `{@html}` (XSS-safe by
 * construction).
 */
import { describe, it, expect, afterEach, onTestFinished, vi } from 'vitest'
import { withPlatform } from '#lib/platform/test-platform.js'
import { mount, unmount, flushSync } from 'svelte'
import { _setCatalogForTests, _clearCompiledCacheForTests } from './messages.svelte'
import { _setLocaleForTests } from './locale'
import TransFixture from './trans-fixture.svelte'

afterEach(() => {
  _setLocaleForTests(null)
  _setCatalogForTests('en', null)
  _clearCompiledCacheForTests()
})

describe('<Trans>', () => {
  it('renders text + an inline interactive component in order', () => {
    _setLocaleForTests('en-US')

    const target = document.createElement('div')
    document.body.appendChild(target)
    const onLinkClick = vi.fn()
    const component = mount(TransFixture, {
      target,
      props: { messageKey: 'common.downloadsFdaHint', onLinkClick },
    })
    flushSync()

    const host = target.querySelector('[data-test="trans-host"]')
    expect(host?.textContent).toBe('Cmdr needs Full Disk Access to watch your Downloads folder. Open System Settings')

    // The tag rendered a REAL <button> (LinkButton in onclick mode), not text.
    const button = host?.querySelector('button.link-button')
    expect(button).not.toBeNull()
    expect(button?.textContent).toBe('Open System Settings')

    // And it's interactive.
    ;(button as HTMLButtonElement).click()
    expect(onLinkClick).toHaveBeenCalledOnce()

    void unmount(component)
  })

  it('is XSS-safe: script-looking message text renders as literal text, not markup', () => {
    // A test-only message whose text (both plain and the tag's inner chunk)
    // looks like an injection. <Trans> renders text as text nodes and tags as
    // real components (no `{@html}`), so no executable node can be created.
    _setCatalogForTests('en', {
      'common.downloadsFdaHint': '1 < 2 alert(1); <settingsLink>img onerror=alert(2)</settingsLink> done',
    })
    _setLocaleForTests('en-US')

    const target = document.createElement('div')
    document.body.appendChild(target)
    const component = mount(TransFixture, {
      target,
      props: { messageKey: 'common.downloadsFdaHint' },
    })
    flushSync()

    const host = target.querySelector('[data-test="trans-host"]')
    // No injected element: the only element is the tag's real LinkButton.
    expect(host?.querySelector('script')).toBeNull()
    expect(host?.querySelector('img')).toBeNull()
    // The literal angle-bracket text survives as text content.
    expect(host?.textContent).toContain('1 < 2 alert(1);')
    // The tag's inner content rendered as the button's literal text, not markup.
    const button = host?.querySelector('button.link-button')
    expect(button?.textContent).toBe('img onerror=alert(2)')

    void unmount(component)
  })
})

describe('<Trans> with a platform block', () => {
  it('renders the branch of the platform it runs on, its tag as a real component', () => {
    _setCatalogForTests('en', {
      'common.downloadsFdaHint':
        '{platform, select, macos {Allow it in <settingsLink>System Settings</settingsLink>.} windows {Allow it in <settingsLink>Settings</settingsLink>.} other {Check your file permissions.}}',
    })
    _setLocaleForTests('en-US')
    onTestFinished(withPlatform('windows'))

    const target = document.createElement('div')
    document.body.appendChild(target)
    const windows = mount(TransFixture, { target, props: { messageKey: 'common.downloadsFdaHint' } })
    flushSync()
    const host = target.querySelector('[data-test="trans-host"]')
    expect(host?.textContent).toBe('Allow it in Settings.')
    expect(host?.querySelector('button.link-button')?.textContent).toBe('Settings')
    void unmount(windows)

    withPlatform('linux')
    const linux = mount(TransFixture, { target, props: { messageKey: 'common.downloadsFdaHint' } })
    flushSync()
    expect(target.querySelector('[data-test="trans-host"]')?.textContent).toBe('Check your file permissions.')
    void unmount(linux)
  })
})
