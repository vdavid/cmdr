/**
 * Pure grouping logic for the Advanced settings page.
 *
 * Advanced is the one section that auto-renders its rows: it pulls every
 * `section[0] === 'Advanced' && !hidden` setting from `getAdvancedSettings()` and
 * lays them out as bespoke rows. To match the macOS System Settings card look that
 * every other page now uses, the rows are grouped into `SectionCard`s by their
 * `card` title (resolved from each setting's `cardKey`).
 *
 * Grouping is driven by the resolved `card` string, in first-appearance
 * (registry) order, so the card order on the page is the author's registry
 * order and stable. Every Advanced setting has `section: ['Advanced']` as its
 * single home (no mirrors on feature pages).
 *
 * `card` is descriptive metadata, never the visibility source: card visibility
 * is owned by `AdvancedSection.svelte` via the same `shouldShow` predicate that
 * gates each row (the `anyVisible(...)` guard), so an all-filtered-out card
 * hides its frame too. This helper only decides membership and order.
 */
import type { SettingDefinition } from '../types'

/** A titled card group of Advanced settings, ready to render. */
export interface AdvancedCardGroup {
  /** The resolved card title (also the stable key for the `{#each}` and the `<SectionCard label>`). */
  title: string
  settings: SettingDefinition[]
}

/**
 * Group Advanced settings into cards by their resolved `card` title, preserving
 * first-appearance (registry) order for both the cards and the rows within each.
 *
 * Every Advanced setting carries a `cardKey` (resolved to `card`), so each lands
 * in exactly one titled card. A setting with no `card` (should not happen for
 * Advanced; guarded by the set-equality test) falls into a trailing untitled
 * "Other" bucket so it still renders and gets surfaced for a real home, rather
 * than silently vanishing.
 */
export function groupAdvancedByCard(settings: SettingDefinition[]): AdvancedCardGroup[] {
  const groups: AdvancedCardGroup[] = []
  const byTitle = new Map<string, AdvancedCardGroup>()
  // Settings with no `card` collect here and render last under no label.
  let other: AdvancedCardGroup | null = null

  for (const setting of settings) {
    const title = setting.card
    if (title === undefined) {
      other ??= { title: '', settings: [] }
      other.settings.push(setting)
      continue
    }
    let group = byTitle.get(title)
    if (!group) {
      group = { title, settings: [] }
      byTitle.set(title, group)
      groups.push(group)
    }
    group.settings.push(setting)
  }

  if (other) groups.push(other)
  return groups
}
