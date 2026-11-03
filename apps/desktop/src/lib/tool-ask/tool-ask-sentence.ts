/**
 * The words of a question, from typed facts. Exhaustive, so a new kind of question doesn't compile
 * until it can be said. What goes in the blanks can be anything (a file name, a model's chosen value),
 * and it's already plain and bounded by `fact_text` in `tools/ask.rs`. The dialog renders text nodes.
 */

import type { AskFacts, IndexingAction, SettingValueFact } from '#lib/ipc/bindings.js'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import { tString } from '#lib/intl/messages.svelte.js'

export interface AskSentence {
  title: string
  body: string
  /** Where it is, in smaller type, when the body names only what. */
  detail?: string
  /** A setting question's two values, each a row under the body (which names the setting). */
  values?: SettingValueRow[]
}

/** One "Old value:" / "New value:" row. `name` is what a person calls the value; `code` is the
 * stored value, shown as inline code. Either can be missing, never both. */
export interface SettingValueRow {
  caption: string
  name: string | null
  code: string | null
}

const INDEXING_WORDS: Record<IndexingAction, { title: MessageKey; body: MessageKey }> = {
  enable: { title: 'ui.toolAsk.indexing.enable.title', body: 'ui.toolAsk.indexing.enable.body' },
  disable: { title: 'ui.toolAsk.indexing.disable.title', body: 'ui.toolAsk.indexing.disable.body' },
  rescan: { title: 'ui.toolAsk.indexing.rescan.title', body: 'ui.toolAsk.indexing.rescan.body' },
  forget: { title: 'ui.toolAsk.indexing.forget.title', body: 'ui.toolAsk.indexing.forget.body' },
}

function openSentence(facts: Extract<AskFacts, { kind: 'openFile' }>): AskSentence {
  const { name, opener, isBundle, shownPath } = facts
  if (isBundle) {
    return {
      title: tString('ui.toolAsk.openBundle.title'),
      body: tString('ui.toolAsk.openBundle.body', { name }),
      detail: shownPath,
    }
  }
  return {
    title: tString('ui.toolAsk.openFile.title'),
    body:
      opener === null
        ? tString('ui.toolAsk.openFile.bodyUnknownApp', { name })
        : tString('ui.toolAsk.openFile.body', { name, app: opener }),
    detail: shownPath,
  }
}

/** A friendly name plus the raw id, the raw id alone, or "nothing" for an empty value (a bare
 * blank would read as a missing word). */
function settingValueRow(caption: string, value: SettingValueFact): SettingValueRow {
  if (value.raw === '') return { caption, name: tString('ui.toolAsk.setSetting.emptyValue'), code: null }
  const name =
    value.name === null
      ? null
      : value.name.kind === 'systemDefault'
        ? tString('ui.toolAsk.setSetting.systemDefault')
        : value.name.text
  return { caption, name, code: value.raw }
}

export function askSentence(facts: AskFacts): AskSentence {
  switch (facts.kind) {
    case 'openFile':
      return openSentence(facts)
    case 'setSetting':
      return {
        title: tString('ui.toolAsk.setSetting.title'),
        body: facts.label,
        values: [
          settingValueRow(tString('ui.toolAsk.setSetting.oldValue'), facts.oldValue),
          settingValueRow(tString('ui.toolAsk.setSetting.newValue'), facts.newValue),
        ],
      }
    case 'quit':
      return { title: tString('ui.toolAsk.quit.title'), body: tString('ui.toolAsk.quit.body') }
    case 'eject':
      return {
        title: tString('ui.toolAsk.eject.title'),
        body: tString(facts.wholeDisk ? 'ui.toolAsk.eject.bodyWholeDisk' : 'ui.toolAsk.eject.body', {
          name: facts.name,
        }),
      }
    case 'removeServer':
      return {
        title: tString('ui.toolAsk.removeServer.title'),
        body: tString('ui.toolAsk.removeServer.body', { name: facts.name }),
      }
    case 'indexing': {
      const words = INDEXING_WORDS[facts.action]
      return { title: tString(words.title), body: tString(words.body, { name: facts.name }) }
    }
  }
}
