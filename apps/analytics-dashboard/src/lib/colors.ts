/**
 * The dashboard's shared color tokens. Kept consistent across metric dots, chart strokes, and fills.
 * See the "Consistent color coding" decision in CLAUDE.md.
 */

/** getcmdr.com / getcmdr/cmdr (the primary product). */
export const COLOR_GOLD = '#ffc206'
/** vdavid/mtp-rs (the library repo). */
export const COLOR_PURPLE = '#a78bfa'
/** veszelovszki.com (David's personal site). */
export const COLOR_GREEN = '#8faa3b'
/** getprvw.com (Prvw product site). */
export const COLOR_CYAN = '#22d3ee'

/** Homebrew as an install channel (new installs stacked by channel). */
export const COLOR_HOMEBREW = '#4a90e2'
/** An unknown value in a breakdown: deliberately gray, so it reads as "no identity". */
export const COLOR_UNKNOWN = '#6b6b73'

/**
 * The rate charts' background bands (bad / fair / good): the app's status tokens from `app.css`. Drawn
 * faint, behind the data, and named in the chart's legend, so the meaning never rides on color alone.
 */
export const COLOR_BAND = { bad: '#ef4444', fair: '#eab308', good: '#22c55e' } as const
