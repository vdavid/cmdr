/**
 * Pure cost-formatting helpers for the per-thread footer and the settings spend display.
 *
 * Costs are integer micro-USD estimates from the backend meter (providers bill in USD, so
 * the currency is USD regardless of the display locale). The honesty rules live in the
 * components: a local-only thread reads "free, on-device", an unpriced turn reads "cost
 * unknown", and only a fully-priced thread shows an estimated amount — never a silent $0.
 */

import { getNumberFormatter } from '$lib/intl/number-format'

/** Total tokens (prompt + completion) for a metered record. */
export function totalTokens(cost: { promptTokens: number; completionTokens: number }): number {
  return cost.promptTokens + cost.completionTokens
}

/**
 * Format an integer micro-USD estimate as a locale-aware USD amount. A sub-dollar amount
 * keeps up to four fraction digits (a chat often costs a few tenths of a cent, and rounding
 * that to `$0.00` would read as free); a dollar or more rounds to cents.
 */
export function formatUsdMicros(micros: number): string {
  const dollars = micros / 1_000_000
  const maximumFractionDigits = Math.abs(dollars) >= 1 ? 2 : 4
  return getNumberFormatter({ style: 'currency', currency: 'USD', maximumFractionDigits }).format(dollars)
}

/** True when every metered turn used a local/on-device model (so the thread is free). */
export function isLocalOnly(providers: string[]): boolean {
  return providers.length > 0 && providers.every((provider) => provider === 'local')
}
