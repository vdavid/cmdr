/**
 * The placeholder kinds `MaskInput` draws an inline editor for: each kind's grammar (parse and format,
 * pure, in its own module), its editor component, and its words. The counter (`[C…]`) and the ranges
 * (`[N…]` `[E…]` `[P…]` `[G…]`, `[2-5]`); a date kind slots in as one more entry here.
 */
import type { Component } from 'svelte'
import type { MessageKey } from '#lib/intl/keys.gen.js'
import CounterTokenEditor from './CounterTokenEditor.svelte'
import RangeTokenEditor from './RangeTokenEditor.svelte'
import { counterKind, type Counter } from './counter-token'
import type { MaskTokenGrammar } from './mask-tokens'
import { rangeKind, type RangeToken } from './range-token'

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

const rangeTokenKind: MaskTokenKind<RangeToken> = {
  ...rangeKind,
  editor: RangeTokenEditor,
  markerLabel: 'multiRename.rangeEditor.marker',
  editorLabel: 'multiRename.rangeEditor.title',
}

export const MASK_TOKEN_KINDS = [counterTokenKind, rangeTokenKind] as const

/**
 * A kind with its value type let go, for `MaskInput`, which renders whichever kind a token is: TS can't
 * pair a union of kinds with their values in markup. Sound because every token's `value` is what its
 * own `kind` parsed (`findTokens`).
 */
export function anyKind(kind: (typeof MASK_TOKEN_KINDS)[number]): MaskTokenKind<unknown> {
  return kind as unknown as MaskTokenKind<unknown>
}
