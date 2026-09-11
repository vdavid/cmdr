/**
 * Pure option-building for the "Edit files in" row, kept beside
 * `TextEditorSelect.svelte` so the list rules are testable without a DOM.
 *
 * The backend answers "which editors macOS lists, what the system default is
 * called, and which one is chosen" (`list_text_editors`); everything here is
 * presentation of that answer, the order included: LaunchServices' own order
 * shifts between calls, so the rows sort by name.
 */

import type { TextEditorList } from '$lib/ipc/bindings'
import { SYSTEM_DEFAULT_EDITOR_CHOICE } from '$lib/text-editor/text-editor-choice'
import { CHOOSE_APP_VALUE, type AppChoiceOption } from './app-choice-options'

/** The words the rows need, resolved by the caller so this module stays free of the catalog. */
export interface TextEditorRowLabels {
  /** The first row's label, given the system default's name (`null` when nothing on this Mac claims plain text). */
  systemDefault: (appName: string | null) => string
  /** The resolved "Choose an app…" label. */
  chooseApp: string
  /** The UI language, which orders the editor names. */
  locale: string
}

/**
 * The dropdown rows: the system default first, then every other editor sorted by
 * name in the app's language, then "Choose an app…".
 * @param list - What `list_text_editors` answered.
 * @param labels - The resolved labels and the UI language.
 */
export function textEditorItems(list: TextEditorList, labels: TextEditorRowLabels): AppChoiceOption[] {
  const collator = new Intl.Collator(labels.locale)
  // The id breaks a tie between two apps with the same name, so the order never shifts between answers.
  const apps = [...list.apps].sort(
    (a, b) => collator.compare(a.displayName, b.displayName) || collator.compare(a.id, b.id),
  )
  return [
    {
      value: SYSTEM_DEFAULT_EDITOR_CHOICE,
      label: labels.systemDefault(list.defaultAppName),
      iconUrl: list.defaultAppIcon ?? undefined,
    },
    ...apps.map((app) => ({ value: app.id, label: app.displayName, iconUrl: app.icon ?? undefined })),
    { value: CHOOSE_APP_VALUE, label: labels.chooseApp },
  ]
}

/**
 * Which row reads as selected.
 *
 * A `chosenId` of `null` means the stored app isn't on this Mac right now. F4
 * opens the system default in that case, so the row shows the system default
 * too. It only DISPLAYS that: resetting the setting is F4's job, at the moment it
 * actually falls back, so browsing Settings never changes a choice.
 * @param list - The backend's answer.
 */
export function selectedTextEditorId(list: TextEditorList): string {
  return list.chosenId ?? SYSTEM_DEFAULT_EDITOR_CHOICE
}
