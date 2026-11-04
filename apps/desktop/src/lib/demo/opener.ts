/**
 * The demo's "default browser": a link the app opens with the opener plugin (`openExternalUrl`, so
 * About, the feedback dialog, and "Star the repo on GitHub") opens in a new browser tab, cut off
 * from this page. Web and email links only; anything else is refused, as the plugin refuses a URL
 * outside its scope. See `DETAILS.md` § Links.
 */
import { refuseWith } from './answer-types.js'

const OPENABLE_SCHEMES = new Set(['http:', 'https:', 'mailto:'])

/** Opens `url` in a new tab, or refuses it (a plain string, as Tauri's plugin refusals are). */
export function openInBrowserTab(url: string): null {
  let scheme: string
  try {
    scheme = new URL(url).protocol
  } catch {
    return refuseWith(`The demo can't open "${url}": it isn't a link`)
  }
  if (!OPENABLE_SCHEMES.has(scheme)) return refuseWith(`The demo opens only web and email links, not a ${scheme} link`)
  window.open(url, '_blank', 'noopener,noreferrer')
  return null
}
