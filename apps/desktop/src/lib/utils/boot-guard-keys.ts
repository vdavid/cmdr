/**
 * The catalog keys the old-WebKit boot guard shows, and the ONE place they're
 * named.
 *
 * The guard is an inline ES5 `<script>` in `src/app.html` that runs before any
 * module loads, on a WebKit too old to parse the bundle, so it can't call `t()`.
 * `svelte.config.js` resolves these keys against every shipped catalog at build
 * time and bakes the answers into the shell, which is how the guard speaks 14
 * languages without an i18n runtime.
 *
 * The generator imports this module, so a key rename moves both sides at once.
 * Keep the import list here empty of anything with a runtime dependency: plain
 * Node loads this file directly, outside Vite's alias resolution.
 *
 * How the guard works and why it can't live in the bundle: `DETAILS.md` §
 * Old-WebKit boot guard.
 */
import type { MessageKey } from '$lib/intl/keys.gen'

/** Title, body, and button label of the "this WebKit is too old" screen. */
export const BOOT_GUARD_KEYS: Record<'title' | 'body' | 'quit', MessageKey> = {
  title: 'main.oldWebkit.title',
  body: 'main.oldWebkit.body',
  quit: 'main.oldWebkit.quit',
}
