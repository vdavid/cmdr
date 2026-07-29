/**
 * What the rail renders: one item per thread line, and one line per tool call.
 *
 * Types only, so the state store (`ask-cmdr-trigger.svelte.ts`) and the pure history fold
 * (`ask-cmdr-history.ts`) can share them without either importing the other. The store
 * re-exports them, so `./ask-cmdr-trigger.svelte` stays the import path components use.
 */

import type { AskCmdrErrorKind, AttachmentRef } from '$lib/tauri-commands'

/** One tool call the assistant made, as the collapsible "looked at X" line shows it. */
export interface RailToolCall {
  callId: string
  /** The wire tool name; the localized label is derived in `ask-cmdr-labels.ts`. */
  tool: string
  running: boolean
  ok: boolean
  /** A path pulled from the call arguments, shown as escaped plain text. `null` if none. */
  path: string | null
}

/** One rendered item in the thread. `attachments` on a user turn are the chips shown
 * under the sent message; history rows carry none (the refs rode into the envelope, not
 * stored blocks). */
export type RailMessage =
  | { kind: 'user'; id: number | null; text: string; attachments: AttachmentRef[] }
  | {
      kind: 'assistant'
      id: number | null
      text: string
      tools: RailToolCall[]
      thinking: boolean
      stalled?: boolean
      streaming: boolean
    }
  | {
      kind: 'error'
      errorKind: AskCmdrErrorKind
      /** The provider's own wording, shown as escaped plain text under the friendly
       * headline so the user sees what to fix. Display only; never branched on. */
      detail?: string
    }
  /** A timeline line marking that the thread's effective model changed between turns. */
  | { kind: 'modelChange'; model: string }
  /** A timeline line marking that older lookups left the model's context so this turn would
   * fit its budget: the reply was written with less than the whole chat in view. Live-stream
   * only — it describes one turn's assembly, so history doesn't replay it. */
  | { kind: 'contextTrimmed'; count: number }
