/**
 * User-Agent classification for `/download` rows. Shared by the write path (`telemetry/`, which stores
 * the family on each row) and the read path (`admin/funnel.ts`, which aggregates by it), so it lives
 * here rather than in either area: both would otherwise import from the other.
 */

/** The User-Agent family a download is classified into. See `classifyUaFamily`. */
export type UaFamily = 'human' | 'bot' | 'unknown'

/**
 * Classify a stored `/download` `user_agent` into a coarse install-plausibility family. Cmdr is
 * macOS-only, which is the whole basis: a `.dmg` fetched by a non-macOS client cannot be a real install.
 *
 * - `human` (a possible install): the UA names a Mac browser (`Macintosh` / `Mac OS`), Homebrew, or a CLI
 *   downloader (`curl` / `wget`, which is how casks and manual installs fetch). Checked first, so a UA
 *   that claims to be a Mac is never excluded.
 * - `bot` (provably impossible): the UA names a non-macOS OS (`Windows`, `Android`, `Linux`, or `X11`).
 *   This is the one high-confidence exclusion — such a client literally can't install the macOS build.
 * - `unknown`: anything else, including a NULL/empty UA on rows captured before migration 0010. We can't
 *   tell, so callers must NOT exclude it from anything; they only exclude `bot`.
 *
 * Deliberately conservative: the scraper spoofs Mac browser UAs (lots of `Mozilla/5.0 (Macintosh; Intel
 * Mac OS X 10_15_7)` from China), so those land in `human` too. We do NOT try to catch spoofed-Mac bots
 * by UA, and we never exclude by country: only the provably-impossible non-macOS UAs are dropped. Pure
 * (no I/O) so it's unit-testable.
 */
export function classifyUaFamily(userAgent: string | null | undefined): UaFamily {
  if (!userAgent) return 'unknown'
  const ua = userAgent.toLowerCase()
  if (
    ua.includes('macintosh') ||
    ua.includes('mac os') ||
    ua.includes('homebrew') ||
    ua.includes('curl') ||
    ua.includes('wget')
  ) {
    return 'human'
  }
  if (ua.includes('windows') || ua.includes('android') || ua.includes('linux') || ua.includes('x11')) {
    return 'bot'
  }
  return 'unknown'
}

/** The three families a download row can fall into. Guards the stored column against a bad value. */
const uaFamilies: ReadonlySet<string> = new Set<UaFamily>(['human', 'bot', 'unknown'])

/**
 * The family for one grouped download row: the value stored at write time when it's there, else the
 * classifier run over the raw UA (rows from before migration 0013). A row that has neither, because
 * the retention sweep cleared the UA of a pre-0013 row, lands in `unknown`, which is never excluded
 * from anything.
 */
export function resolveUaFamily(row: { uaFamily: string | null; userAgent: string | null }): UaFamily {
  if (row.uaFamily !== null && uaFamilies.has(row.uaFamily)) return row.uaFamily as UaFamily
  return classifyUaFamily(row.userAgent)
}
