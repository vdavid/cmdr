/**
 * The app's Light, Dark, and System appearance, for one frame. On desktop, `setTheme` (`plugin:app|set_app_theme`)
 * sets the window's appearance, and WebKit's `prefers-color-scheme` follows it, so every `@media` rule and
 * `matchMedia` query in the app switches. A page can't set `prefers-color-scheme`: the browser answers it from the
 * visitor's OS. So this pins the media query in the frame instead, the two places the app reads it:
 *
 * - CSS: every `@media` rule naming `prefers-color-scheme` gets the feature swapped for an always-true or always-false
 *   one, in every same-origin stylesheet, including ones a route adds later. `null` (System) puts the original back,
 *   so the browser follows the OS live again.
 * - JS: `matchMedia` answers a `prefers-color-scheme` query with a list that reads the pinned scheme and fires
 *   `change` when it flips, from a pick or (on System) from the OS.
 *
 * Also sets `color-scheme` on `:root`, so scrollbars and form controls match. See `DETAILS.md` § Light and dark.
 */

/** What `setTheme` sends: an explicit appearance, or `null` to follow the OS. */
export type AppTheme = 'light' | 'dark' | null

const SCHEME_FEATURE = /\(\s*prefers-color-scheme\s*:\s*(light|dark)\s*\)/gi
/** A media condition that always holds, and one that never does, each valid inside an `and` chain. */
const ALWAYS = '(min-width: 0px)'
const NEVER = '((max-width: 0px) and (min-width: 1px))'

/** Whether a media text reads the color scheme at all. */
function readsScheme(mediaText: string): boolean {
  return new RegExp(SCHEME_FEATURE.source, 'i').test(mediaText)
}

/**
 * `mediaText` with each `prefers-color-scheme` feature answered for `theme`, or as is for `null`. Exported for its
 * test.
 */
export function pinColorScheme(mediaText: string, theme: AppTheme): string {
  if (theme === null) return mediaText
  return mediaText.replace(SCHEME_FEATURE, (_, scheme: string) => (scheme.toLowerCase() === theme ? ALWAYS : NEVER))
}

export interface ColorSchemeHandle {
  set(theme: AppTheme): void
}

/** A `MediaQueryList` for a scheme query, answering for the pinned theme. */
class PinnedQueryList extends EventTarget implements MediaQueryList {
  onchange: ((this: MediaQueryList, event: MediaQueryListEvent) => unknown) | null = null
  private last: boolean

  constructor(
    readonly media: string,
    private readonly evaluate: (query: string) => boolean,
    private readonly onListened: (list: PinnedQueryList) => void,
  ) {
    super()
    this.last = this.matches
  }

  get matches(): boolean {
    return this.evaluate(this.media)
  }

  override addEventListener(...args: Parameters<EventTarget['addEventListener']>): void {
    this.onListened(this)
    super.addEventListener(...args)
  }

  addListener(callback: ((this: MediaQueryList, event: MediaQueryListEvent) => unknown) | null): void {
    if (callback) this.addEventListener('change', callback as EventListener)
  }

  removeListener(callback: ((this: MediaQueryList, event: MediaQueryListEvent) => unknown) | null): void {
    if (callback) this.removeEventListener('change', callback as EventListener)
  }

  /** Fires `change` if the answer moved since the last one. */
  recheck(): void {
    const matches = this.matches
    if (matches === this.last) return
    this.last = matches
    const event = Object.assign(new Event('change'), { matches, media: this.media }) as MediaQueryListEvent
    this.onchange?.call(this, event)
    this.dispatchEvent(event)
  }
}

/** Installs the pinning in `win`'s document and `matchMedia`. Starts on System (`null`). */
export function installColorScheme(win: Window & typeof globalThis): ColorSchemeHandle {
  const doc = win.document
  const realMatchMedia = win.matchMedia.bind(win)
  let theme: AppTheme = null
  /** Each rewritten rule's own text, to put back on System. */
  const originals = new WeakMap<CSSMediaRule, string>()
  /** The lists someone listens on: they hear a flip. A list nobody listens on is read fresh on each `matches`. */
  const listened = new Set<PinnedQueryList>()

  const evaluate = (query: string) => realMatchMedia(pinColorScheme(query, theme)).matches

  function pinRules(rules: CSSRuleList): void {
    for (const rule of Array.from(rules)) {
      if (rule instanceof win.CSSMediaRule) {
        const original = originals.get(rule) ?? rule.media.mediaText
        if (readsScheme(original)) {
          originals.set(rule, original)
          const pinned = pinColorScheme(original, theme)
          if (rule.media.mediaText !== pinned) rule.media.mediaText = pinned
        }
      }
      // `@media` inside `@layer`, `@supports`, or another `@media`.
      if ('cssRules' in rule) pinRules((rule as CSSGroupingRule).cssRules)
    }
  }

  function pinSheet(sheet: CSSStyleSheet | null): void {
    if (!sheet) return
    try {
      pinRules(sheet.cssRules)
    } catch {
      // A cross-origin sheet hides its rules; the demo loads none of its own from elsewhere.
    }
  }

  function pinAll(): void {
    for (const sheet of Array.from(doc.styleSheets)) pinSheet(sheet)
    doc.documentElement.style.colorScheme = theme ?? ''
  }

  // A route's stylesheet lands later: a `<style>` has its sheet at once, a `<link>` once it loads.
  new win.MutationObserver((records) => {
    for (const record of records) {
      for (const node of Array.from(record.addedNodes)) {
        if (node instanceof win.HTMLStyleElement) pinSheet(node.sheet)
        else if (node instanceof win.HTMLLinkElement) {
          pinSheet(node.sheet)
          node.addEventListener('load', () => {
            pinSheet(node.sheet)
          })
        }
      }
    }
  }).observe(doc, { childList: true, subtree: true })

  win.matchMedia = (query: string): MediaQueryList =>
    readsScheme(query) ? new PinnedQueryList(query, evaluate, (list) => listened.add(list)) : realMatchMedia(query)

  function recheckAll(): void {
    for (const list of listened) list.recheck()
  }
  // On System, the OS flipping its appearance reaches the app's listeners through the real query.
  realMatchMedia('(prefers-color-scheme: dark)').addEventListener('change', recheckAll)

  return {
    set(next) {
      if (next === theme) return
      theme = next
      pinAll()
      recheckAll()
    },
  }
}
