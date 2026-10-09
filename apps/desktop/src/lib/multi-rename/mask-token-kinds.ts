/**
 * The placeholder kinds `MaskInput` draws an inline editor for: each kind's grammar (parse and format,
 * pure, in its own module), its editor component, and its words. Only the counter has one so far; a
 * range (`[N2-5]`) or a date kind slots in as one more entry here.
 */
import type { Component } from 'svelte'
import type { MessageKey } from '$lib/intl/keys.gen'
import CounterTokenEditor from './CounterTokenEditor.svelte'
import { counterKind, type Counter } from './counter-token'
import type { MaskTokenGrammar } from './mask-tokens'

/** What every token editor takes. The mask text stays the source of truth: `onChange` rewrites the token. */
export interface TokenEditorProps<V> {
  value: V
  onChange: (value: V) => void
  /** Leave the editor for the mask field (Enter, or ArrowUp from the first field). */
  onDone: () => void
}

export interface MaskTokenKind<V> extends MaskTokenGrammar<V> {
  editor: Component<TokenEditorProps<V>>
  /** The ▾ marker's accessible name. */
  markerLabel: MessageKey
  /** The editor popover's accessible name. */
  editorLabel: MessageKey
}

const counterTokenKind: MaskTokenKind<Counter> = {
  ...counterKind,
  editor: CounterTokenEditor,
  markerLabel: 'multiRename.counterEditor.marker',
  editorLabel: 'multiRename.counterEditor.title',
}

export const MASK_TOKEN_KINDS = [counterTokenKind] as const
