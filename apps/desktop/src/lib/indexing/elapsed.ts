/**
 * Format an elapsed duration as a clock (`m:ss`, e.g. `0:42`, `12:05`), or
 * `null` when there's under a second to show (so the caller falls back to a
 * count-only phrasing and the clock never flashes a misleading `0:00`).
 *
 * A clock, not a localized number, so it's built here rather than routed
 * through `$lib/intl` number formatting. Shared by the breadcrumb drive-index
 * badge tooltip and the top-right indexing indicator's drive rows, so both
 * surfaces show the same "· M:SS" elapsed clock from one source.
 */
export function formatElapsedClock(elapsedMs: number): string | null {
  if (!Number.isFinite(elapsedMs) || elapsedMs < 1000) return null
  const totalSeconds = Math.floor(elapsedMs / 1000)
  const minutes = Math.floor(totalSeconds / 60)
  const seconds = totalSeconds % 60
  return `${String(minutes)}:${String(seconds).padStart(2, '0')}`
}
