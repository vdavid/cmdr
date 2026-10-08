/**
 * Which setting rows a `SettingsSection` holds right now: each `SettingRow` registers its id with
 * the nearest section through context while it's mounted. The section reads the set to say
 * "Your organization manages some of these settings" at its top, so no section keeps a list of
 * which of its ids a policy could lock. A row outside any section (onboarding) registers nowhere.
 */

import { getContext, setContext } from 'svelte'
import { SvelteSet } from 'svelte/reactivity'

const SECTION_ROWS = Symbol('settings-section-rows')

/** Called by `SettingsSection`: the reactive set of setting ids its rows registered. */
export function provideSectionRows(): ReadonlySet<string> {
  const ids = new SvelteSet<string>()
  setContext(SECTION_ROWS, ids)
  return ids
}

/** Called by `SettingRow` during init: registers `id` with the enclosing section while mounted. */
export function registerSectionRow(id: string): void {
  const ids = getContext<SvelteSet<string> | undefined>(SECTION_ROWS)
  if (ids === undefined) return
  $effect(() => {
    ids.add(id)
    return () => {
      ids.delete(id)
    }
  })
}
