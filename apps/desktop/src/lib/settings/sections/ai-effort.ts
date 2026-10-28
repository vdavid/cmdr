/**
 * The reasoning picker's pure half (labeled per provider: Reasoning effort, Effort, or Thinking level): which options a model gets, and how a pick is stored in
 * `ai.reasoningEfforts` (a JSON object keyed `"<provider id>/<model>"`). The backend decides what
 * the model can take (`AiEffortOptions`) and maps a stored pick it can't to the nearest one it
 * can (`agent::chat::effort::resolve_effort`), so this menu is a courtesy, never the guard.
 */

import type { MessageKey } from '#lib/intl/keys.gen.js'
import type { AiEffortLevel, AiEffortOptions } from '#lib/ipc/bindings.js'
import { getCloudProvider, type EffortTerm } from '../cloud-providers'

/** One literal key per term, so the i18n checks can see every one. */
const TERM_LABEL_KEYS: Record<EffortTerm, MessageKey> = {
  reasoningEffort: 'settings.ai.reasoningEfforts.label.reasoningEffort',
  effort: 'settings.ai.reasoningEfforts.label.effort',
  thinkingLevel: 'settings.ai.reasoningEfforts.label.thinkingLevel',
}

/**
 * The picker's label in the provider's own words (`CloudProviderPreset.effortTerm`), or the
 * Thinking level fallback for an id no preset knows.
 */
export function effortLabelKey(providerId: string): MessageKey {
  return TERM_LABEL_KEYS[getCloudProvider(providerId)?.effortTerm ?? 'thinkingLevel']
}

/** What the picker stores: `recommended`, `off`, or a level. */
export type EffortChoice = 'recommended' | 'off' | AiEffortLevel

/** Whether to show the picker at all: hidden only for a model known not to reason. */
export function showsEffortPicker(options: AiEffortOptions | null | undefined): boolean {
  return options != null && options.reasons !== false
}

/** The picker's choices, in menu order: Recommended, Off when the model can stop thinking, then its levels. */
export function effortChoices(options: AiEffortOptions): EffortChoice[] {
  return ['recommended', ...(options.canTurnOff ? (['off'] as const) : []), ...options.levels]
}

const LEVEL_ORDER: AiEffortLevel[] = ['low', 'medium', 'high', 'max']

/**
 * What a stored pick amounts to on this model, so the picker shows what the next send will
 * really ask for. Mirrors `agent::chat::effort::resolve_effort`: a level it doesn't take maps to
 * the nearest it does (ties to the cheaper), Off without an off switch to the cheapest level, and
 * either with no known levels to Recommended.
 */
export function effectiveEffortChoice(options: AiEffortOptions, choice: EffortChoice): EffortChoice {
  if (effortChoices(options).includes(choice)) return choice
  if (options.levels.length === 0) return 'recommended'
  if (choice === 'off' || choice === 'recommended') return options.levels[0]
  const wanted = LEVEL_ORDER.indexOf(choice)
  return options.levels.reduce((best, level) => {
    const distance = Math.abs(LEVEL_ORDER.indexOf(level) - wanted)
    const bestDistance = Math.abs(LEVEL_ORDER.indexOf(best) - wanted)
    return distance < bestDistance ? level : best
  })
}

/** The stored pick for one model, or `recommended` when there is none (or the blob doesn't parse). */
export function readEffortChoice(stored: string, providerId: string, model: string): EffortChoice {
  const value = parseMap(stored)[effortKey(providerId, model)]
  return isChoice(value) ? value : 'recommended'
}

/** The blob with one model's pick set. Recommended is the default, so it removes the entry. */
export function writeEffortChoice(stored: string, providerId: string, model: string, choice: EffortChoice): string {
  const key = effortKey(providerId, model)
  const kept = Object.entries(parseMap(stored)).filter(([existing]) => existing !== key)
  return JSON.stringify(Object.fromEntries(choice === 'recommended' ? kept : [...kept, [key, choice]]))
}

function effortKey(providerId: string, model: string): string {
  return `${providerId}/${model}`
}

function parseMap(stored: string): Record<string, unknown> {
  try {
    const parsed: unknown = JSON.parse(stored)
    return parsed !== null && typeof parsed === 'object' && !Array.isArray(parsed)
      ? { ...(parsed as Record<string, unknown>) }
      : {}
  } catch {
    return {}
  }
}

function isChoice(value: unknown): value is EffortChoice {
  return (
    value === 'recommended' ||
    value === 'off' ||
    value === 'low' ||
    value === 'medium' ||
    value === 'high' ||
    value === 'max'
  )
}
