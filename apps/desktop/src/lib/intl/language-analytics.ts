/**
 * Two questions about language, answered anonymously: which language does an
 * install actually run in, and does auto-selection land somewhere the user
 * wants to stay?
 *
 * The second one is the honest quality signal, and it's the only one we get.
 * Nothing in the UI asks how a translation reads (deliberately: no
 * machine-translation notice anywhere), so a user reaching for the picker and
 * walking away from their own language is the strongest evidence we have that a
 * locale is bad.
 *
 * Both events ride the existing consent gate through `trackEvent`, and both
 * carry the BASE language subtag only (`hu`, never `hu-HU`): a rare language
 * plus a region narrows a population further than the question needs, and the
 * base subtag answers it completely.
 */

import { trackEvent } from '$lib/tauri-commands'
import { getSetting } from '$lib/settings'
import { getUiLocale } from './locale'
import { pickUiLocale } from './os-locales'

/** Where a hand pick happened. The onboarding frame and the Settings picker are the only two. */
export type LanguageSurface = 'onboarding' | 'settings'

/** How the running language was arrived at. */
type LanguageSource = 'auto' | 'explicit' | 'fallback'

/** What `detected` says when nothing the user listed is a language we ship. */
const NO_LANGUAGE = 'none'

/**
 * The base subtag this window is running in, as last reported. It's the `from`
 * of the next hand pick, and it deliberately does NOT follow the live locale:
 * the picker applies each highlighted row as a preview (keyboard AND hover), so
 * reading the current locale at pick time would report the row the user skimmed
 * past rather than the language they arrived with.
 */
let activeLanguage: string | null = null

/** Whether this window has already sent its one `language_resolved`. */
let resolvedSent = false

/**
 * The base language subtag of a BCP-47 tag: `pt-BR` → `pt`, `zh-Hant-TW` →
 * `zh`. `null` (no OS answer at all) becomes the categorical {@link NO_LANGUAGE}
 * rather than a missing prop, so "we found nothing" and "the event predates the
 * prop" stay distinguishable in the data.
 */
function baseLanguage(tag: string | null): string {
  if (tag === null || tag.length === 0) return NO_LANGUAGE
  return tag.split('-')[0].toLowerCase()
}

/**
 * Records the language this window came up in, without sending anything. The
 * first call wins, so a startup seed that lands after the user has already
 * picked something can't rewrite their history.
 *
 * Secondary windows need this: the Settings window hosts the picker but never
 * sends `language_resolved` (that's the main window's one-per-launch event), so
 * without a seed its first pick would have nothing to report as `from`.
 */
export function noteStartupLanguage(): void {
  activeLanguage ??= baseLanguage(getUiLocale())
}

/**
 * Reports the language this launch resolved to, once. Called from the main
 * window's settings applier after the OS answers have landed and the stored
 * setting has been applied, so `active` is what the user is actually looking at.
 *
 * `detected` is what the Rust resolver found by walking the OS preference list
 * against the shipped catalogs, which is a different question from `active`: an
 * install can detect Hungarian and still run in German because the user said so.
 */
export function trackLanguageResolved(): void {
  if (resolvedSent) return
  resolvedSent = true

  const detected = baseLanguage(pickUiLocale('system'))
  const active = baseLanguage(getUiLocale())
  activeLanguage = active

  const source: LanguageSource =
    getSetting('appearance.language') !== 'system' ? 'explicit' : detected === NO_LANGUAGE ? 'fallback' : 'auto'

  void trackEvent('language_resolved', { detected, active, source })
}

/**
 * Reports a language the user picked by hand. Call it from the pick itself, ❌
 * never from a settings subscription: the setting is written on every
 * highlighted row (the live preview) and it's mirrored into every open window,
 * so a subscription would report the skimmed-past rows and double every change.
 *
 * A pick that lands on the language already running sends nothing. `from` means
 * "what they left", so a user pinning "System default (Magyar)" to an explicit
 * `hu` must not read as walking away from Hungarian.
 *
 * @param surface which of the two pickers the user used
 * @param picked the `appearance.language` value they chose (`'system'` included)
 */
export function trackLanguageChanged(surface: LanguageSurface, picked: string): void {
  const to = baseLanguage(pickUiLocale(picked))
  const from = activeLanguage
  if (from === null || from === to) {
    activeLanguage = to
    return
  }
  activeLanguage = to
  void trackEvent('language_changed', { from, surface })
}

/** Test seam: forget this window's language history. ❌ Not for production. */
export function _resetLanguageAnalyticsForTests(): void {
  activeLanguage = null
  resolvedSent = false
}
