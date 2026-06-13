/**
 * Pure diff between a command's default shortcuts and its current (effective)
 * shortcuts, for the read-only Keyboard shortcuts help window.
 *
 * Each key becomes one chip with a status:
 *   - `active`   — bound in both default and effective: the shortcut works and
 *                  matches the shipped default.
 *   - `added`    — present in effective but NOT in the default: a user-added or
 *                  user-replaced binding. The help window shows these bold/green.
 *   - `disabled` — present in the default but NOT in effective: the shipped
 *                  binding the user turned off (by removing or replacing it).
 *                  The help window shows these dimmed/struck.
 *
 * One set-diff covers every case (extra, replaced, removed): a replaced key is
 * just an `added` plus a `disabled`, a removed key is a lone `disabled`.
 *
 * Order: effective keys first (in their effective order), then the disabled
 * defaults (in their default order). Reads as "what's live now, then what's
 * off." Within each source, the first occurrence of a duplicate key wins.
 */

export type ShortcutChipStatus = 'active' | 'added' | 'disabled'

export interface ShortcutDiffChip {
  key: string
  status: ShortcutChipStatus
}

export function diffShortcuts(defaults: readonly string[], effective: readonly string[]): ShortcutDiffChip[] {
  const defaultSet = new Set(defaults)
  const chips: ShortcutDiffChip[] = []
  const seen = new Set<string>()

  // Effective keys first, in effective order. A key in the defaults is `active`,
  // one that isn't is `added`.
  for (const key of effective) {
    if (seen.has(key)) continue
    seen.add(key)
    chips.push({ key, status: defaultSet.has(key) ? 'active' : 'added' })
  }

  // Then the defaults the user turned off (not in effective), in default order.
  const effectiveSet = new Set(effective)
  for (const key of defaults) {
    if (effectiveSet.has(key) || seen.has(key)) continue
    seen.add(key)
    chips.push({ key, status: 'disabled' })
  }

  return chips
}

/**
 * Whether a command's bindings differ from the shipped defaults, derived from
 * the diff: any `added` or `disabled` chip means the user changed something.
 */
export function isModifiedDiff(chips: readonly ShortcutDiffChip[]): boolean {
  return chips.some((c) => c.status !== 'active')
}
