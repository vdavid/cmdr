import { afterEach, describe, expect, it, vi } from 'vitest'
import { installColorScheme, pinColorScheme } from './color-scheme.js'

describe('pinColorScheme', () => {
  it('answers the scheme feature for an explicit theme, and leaves System to the browser', () => {
    expect(pinColorScheme('(prefers-color-scheme: dark)', 'dark')).toBe('(min-width: 0px)')
    expect(pinColorScheme('(prefers-color-scheme: dark)', 'light')).toBe('((max-width: 0px) and (min-width: 1px))')
    expect(pinColorScheme('screen and (prefers-color-scheme:light)', 'light')).toBe('screen and (min-width: 0px)')
    expect(pinColorScheme('(prefers-color-scheme: dark)', null)).toBe('(prefers-color-scheme: dark)')
    expect(pinColorScheme('(prefers-reduced-motion: reduce)', 'dark')).toBe('(prefers-reduced-motion: reduce)')
  })
})

/** A `matchMedia` for a visitor whose OS is in `os` mode, answering the two pinned conditions as a browser does. */
function fakeMatchMedia(os: { dark: boolean }) {
  const osListeners: (() => void)[] = []
  const matchMedia = (query: string) => {
    const matches =
      query === '(min-width: 0px)'
        ? true
        : query.startsWith('((max-width: 0px)')
          ? false
          : query === '(prefers-color-scheme: dark)'
            ? os.dark
            : !os.dark
    return {
      matches,
      media: query,
      addEventListener: (_: string, cb: () => void) => osListeners.push(cb),
    } as unknown as MediaQueryList
  }
  const flipOs = (dark: boolean) => {
    os.dark = dark
    for (const cb of osListeners) cb()
  }
  return { matchMedia, flipOs }
}

describe('installColorScheme', () => {
  const realMatchMedia = window.matchMedia.bind(window)

  afterEach(() => {
    window.matchMedia = realMatchMedia
    document.head.querySelectorAll('style').forEach((style) => {
      style.remove()
    })
  })

  it('answers matchMedia for the pinned theme and tells listeners when it flips', () => {
    const fake = fakeMatchMedia({ dark: false })
    window.matchMedia = fake.matchMedia
    const scheme = installColorScheme(window)
    const dark = window.matchMedia('(prefers-color-scheme: dark)')
    const heard: boolean[] = []
    dark.addEventListener('change', (event) => heard.push(event.matches))
    expect(dark.matches).toBe(false)

    scheme.set('dark')
    expect(dark.matches).toBe(true)
    // An explicit pick holds whatever the OS does.
    fake.flipOs(true)
    fake.flipOs(false)
    scheme.set(null)
    // On System, the OS leads again.
    fake.flipOs(true)
    expect(heard).toEqual([true, false, true])
  })

  it('leaves every other query to the browser', () => {
    const plain = vi.fn(
      (_query: string) => ({ matches: true, addEventListener: () => undefined }) as unknown as MediaQueryList,
    )
    window.matchMedia = plain
    installColorScheme(window)
    window.matchMedia('(prefers-reduced-motion: reduce)')
    expect(plain).toHaveBeenCalledWith('(prefers-reduced-motion: reduce)')
  })

  it('pins the stylesheets’ scheme rules, and puts them back on System', () => {
    window.matchMedia = fakeMatchMedia({ dark: false }).matchMedia
    const style = document.createElement('style')
    style.textContent = '@media (prefers-color-scheme: dark) { body { color: white } }'
    document.head.append(style)
    const scheme = installColorScheme(window)
    const rule = () => (style.sheet?.cssRules[0] as CSSMediaRule).media.mediaText

    scheme.set('dark')
    expect(rule()).toBe('(min-width: 0px)')
    expect(document.documentElement.style.colorScheme).toBe('dark')
    scheme.set(null)
    expect(rule()).toBe('(prefers-color-scheme: dark)')
    expect(document.documentElement.style.colorScheme).toBe('')
  })
})
