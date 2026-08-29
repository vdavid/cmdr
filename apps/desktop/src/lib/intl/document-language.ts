/**
 * The `<html lang>` attribute: the one place the DOM is told which language the
 * window is speaking.
 *
 * `app.html` ships a static `lang="en"`, and one template serves every window,
 * so without this every locale served English's language tag. That's a WCAG
 * 3.1.1 (Level A) failure, and the practical cost is bigger than the letter of
 * it: a screen reader picks its voice, pronunciation, and prosody from this
 * attribute, so a Swedish UI announced as English is read with an English voice
 * spelling out Swedish words.
 *
 * ❌ Nothing else may write `documentElement.lang`. The single caller is
 * `setLocale()` in `messages.svelte.ts`, which is the seam every window's
 * language init and every live language change already funnel through (the main
 * window via `settings-applier.ts`, the rest via `initWindowLanguageSync()`).
 * Hanging a second writer off a window's own mount would drift the moment one
 * window forgot, which is exactly the shape of the bug this fixes.
 *
 * Layout DIRECTION deliberately isn't set here: Cmdr ships no RTL catalog yet,
 * and a `dir` written from a language tag would be an untested guess. When an
 * RTL locale lands, this is the module it lands in.
 */

/**
 * Announces `tag` as the document's language.
 *
 * A no-op with no document. `setLocale()` runs under the SvelteKit static
 * adapter's Node pass and inside plain-Node unit tests, and neither has a DOM;
 * throwing there would take down a locale switch over an accessibility hint.
 */
export function setDocumentLanguage(tag: string): void {
  if (typeof document === 'undefined') return
  document.documentElement.lang = tag
}
