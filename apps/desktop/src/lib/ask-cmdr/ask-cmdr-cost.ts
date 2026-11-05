/**
 * Pure cost-formatting helpers for the per-thread footer and the settings spend display.
 *
 * Costs are integer micro-USD estimates from the backend meter (providers bill in USD, so
 * the currency is USD regardless of the display locale). The honesty rules live in
 * `costReading`: a local-only thread reads "free, on-device", an unpriced turn reads "cost
 * unknown", and only a fully-priced thread shows an estimated amount — never a silent $0.
 */

import { tString } from '#lib/intl/messages.svelte.js'
import { formatCompactCount, formatInteger, getNumberFormatter } from '#lib/intl/number-format.js'
import type { ConversationCost, NextCallPricing } from '#lib/tauri-commands/index.js'

/** Total tokens (prompt + completion) for a metered record. */
export function totalTokens(cost: { promptTokens: number; completionTokens: number }): number {
  return cost.promptTokens + cost.completionTokens
}

/** From here up, a token count reads compact ("2k", "1.3M"); below it, exact. */
const COMPACT_TOKENS_FROM = 2_000

/**
 * A token count as the rail shows it: exact below 2,000, then the UI language's own compact
 * form with one decimal at most ("2.2K", "1.3M"; en-GB "1.3m", Chinese "2.2万", German writes
 * thousands in full). CLDR picks the unit, so 999,950 reads "1M", never "1,000K", and a
 * language counting in 10⁴ gets it. Exact counts belong in a tooltip.
 */
export function formatTokenCount(tokens: number): string {
  if (tokens < COMPACT_TOKENS_FROM) return formatInteger(tokens)
  return formatCompactCount(tokens)
}

/**
 * A token count spelled out exactly, plus how many of the prompt tokens the provider's cache
 * served when any were ("48,213 tokens, 41,002 of them cached"). What the tooltips behind a
 * compact count say, and what the settings spend list shows.
 */
export function exactTokensText(tokens: number, cacheReadTokens: number): string {
  const params = { count: tokens, countText: formatInteger(tokens) }
  if (cacheReadTokens <= 0) return tString('askCmdr.cost.tokens', params)
  return tString('askCmdr.cost.tokensWithCache', { ...params, cachedText: formatInteger(cacheReadTokens) })
}

/**
 * The tooltip behind a compact token count: the exact count, with the cached share. `undefined`
 * when the visible count already says it all (exact, and nothing came from the cache).
 */
export function tokensTooltip(tokens: number, cacheReadTokens: number): string | undefined {
  if (cacheReadTokens <= 0 && formatTokenCount(tokens) === formatInteger(tokens)) return undefined
  return exactTokensText(tokens, cacheReadTokens)
}

/**
 * Format an integer micro-USD estimate as a USD amount rounded to cents ("$0.83"), symbol,
 * placement, and digits all in the UI language. The tooltip beside it says it's USD, so the
 * symbol stays narrow.
 */
export function formatUsdMicros(micros: number): string {
  return getNumberFormatter({
    style: 'currency',
    currency: 'USD',
    currencyDisplay: 'narrowSymbol',
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  }).format(micros / 1_000_000)
}

/** Half a cent in micro-USD: anything below it would round to `$0.00`. */
const HALF_CENT_MICROS = 5_000

/**
 * The estimate as a phrase: "about $0.83", or "under $0.01" for a real cost too small to show
 * in cents (a short chat often costs a fraction of a cent, and "$0.00" would read as free).
 */
export function costEstimateText(micros: number): string {
  if (micros > 0 && micros < HALF_CENT_MICROS) {
    return tString('askCmdr.cost.underAmount', { amount: formatUsdMicros(10_000) })
  }
  return tString('askCmdr.cost.estimate', { amount: formatUsdMicros(micros) })
}

/** True when every metered turn used a local/on-device model (so the thread is free). */
export function isLocalOnly(providers: string[]): boolean {
  return providers.length > 0 && providers.every((provider) => provider === 'local')
}

/** The footer's cost line as data: a token count and, when there is something to price, how. */
export type CostReading = {
  tokens: number
  /** Of the prompt tokens, how many the provider's cache served. */
  cacheReadTokens: number
  price: { kind: 'free' } | { kind: 'unknown' } | { kind: 'estimate'; micros: number } | null
}

/**
 * What the footer says about money. A thread with metered calls reads from the meter (the one
 * source of its total). A thread with none (a brand-new chat, or one whose turns all failed)
 * reads zero, priced the way its first call would be (`nextCall`, from the slot status), so it
 * says "free", "about $0", or "cost unknown" honestly from the start. `nextCall` is null while
 * AI is off, and then nothing is priced at all.
 */
export function costReading(threadCost: ConversationCost | null, nextCall: NextCallPricing | null): CostReading {
  if (threadCost && totalTokens(threadCost) > 0) {
    const metered = { tokens: totalTokens(threadCost), cacheReadTokens: threadCost.cacheReadTokens }
    if (isLocalOnly(threadCost.providers)) return { ...metered, price: { kind: 'free' } }
    if (!threadCost.fullyPriced) return { ...metered, price: { kind: 'unknown' } }
    return { ...metered, price: { kind: 'estimate', micros: threadCost.costMicros } }
  }
  const nothing = { tokens: 0, cacheReadTokens: 0 }
  if (!nextCall) return { ...nothing, price: null }
  if (nextCall.provider === 'local') return { ...nothing, price: { kind: 'free' } }
  if (!nextCall.priced) return { ...nothing, price: { kind: 'unknown' } }
  return { ...nothing, price: { kind: 'estimate', micros: 0 } }
}
