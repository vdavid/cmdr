/**
 * TC's per-field history (⌥⇧↓): one menu, shown under whichever text field asked, listing
 * what that field held when renames ran, newest first. A pick fills the field. The list is the
 * backend's (`src-tauri/src/multi_rename/history.rs`), recorded when a rename starts.
 */

import { getMultiRenameHistory, type FieldHistoryEntry, type HistoryField } from '$lib/tauri-commands'
import { createMenu, type MenuController } from '$lib/ui/menu-controller.svelte'
import { tString } from '$lib/intl/messages.svelte'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('multiRename')

/** How many history entries a field's menu offers. */
export const HISTORY_SHOWN = 30

export const HISTORY_FIELDS: readonly HistoryField[] = ['nameMask', 'extensionMask', 'search', 'replace']

export interface FieldHistoryDeps {
  /** The field's input element, once mounted. */
  inputOf: (field: HistoryField) => HTMLInputElement | undefined
  fill: (field: HistoryField, value: string) => void
}

export interface FieldHistoryMenu {
  readonly menu: MenuController<string>
  /** Reads the history; until it lands, every field's list is empty. */
  load: () => Promise<void>
  /** What `field` held in earlier renames, newest first. */
  historyOf: (field: HistoryField) => string[]
  open: (field: HistoryField) => void
  /** Which history field `target` is, or `null` for any other element. */
  fieldOf: (target: EventTarget | null) => HistoryField | null
  destroy: () => void
}

export function createFieldHistoryMenu(deps: FieldHistoryDeps): FieldHistoryMenu {
  let field = $state<HistoryField>('nameMask')
  let entries = $state.raw<FieldHistoryEntry[]>([])

  function historyOf(of: HistoryField): string[] {
    return entries.filter((entry) => entry.field === of).map((entry) => entry.value)
  }

  const menu = createMenu<string>({
    getSections: () => [
      {
        id: 'history',
        items: historyOf(field)
          .slice(0, HISTORY_SHOWN)
          .map((value, i) => ({ value: String(i), label: value, data: value })),
        emptyLabel: tString('multiRename.historyEmpty'),
      },
    ],
    onSelect: (item) => {
      if (item.data !== undefined) deps.fill(field, item.data)
    },
    restoreFocus: () => {
      deps.inputOf(field)?.focus()
    },
  })

  return {
    menu,
    async load() {
      try {
        entries = await getMultiRenameHistory()
      } catch (e) {
        // No history is a sheet without its lists, never a broken sheet.
        log.warn("couldn't read the multi-rename field history: {reason}", { reason: String(e) })
      }
    },
    historyOf,
    open(next) {
      const input = deps.inputOf(next)
      if (!input) return
      field = next
      menu.openUnder(input)
    },
    fieldOf(target) {
      return HISTORY_FIELDS.find((f) => deps.inputOf(f) === target) ?? null
    },
    destroy() {
      menu.destroy()
    },
  }
}
