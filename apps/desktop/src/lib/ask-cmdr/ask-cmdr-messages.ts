/**
 * What the rail renders: one item per thread line, and one line per tool call.
 *
 * Types only, so the state store (`ask-cmdr-trigger.svelte.ts`) and the pure history fold
 * (`ask-cmdr-history.ts`) can share them without either importing the other. The store
 * re-exports them, so `./ask-cmdr-trigger.svelte` stays the import path components use.
 */

import type {
  AskCmdrErrorKind,
  AttachmentRef,
  SkipBreakdown,
  WakeDigestFolderView,
  WakeDigestRollupView,
} from '$lib/tauri-commands'

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
  /** What a wake noticed, which opens every thread the agent started for itself. It sits
   * where a user bubble would, because that is the role it plays in the transcript.
   *
   * ⚠️ **Counts and paths, never a sentence.** The backend persists this as data precisely
   * so the words around it can be ours and can be translated; the English digest the model
   * reads never crosses IPC. */
  | {
      kind: 'wakeDigest'
      id: number | null
      folders: WakeDigestFolderView[]
      /** The folders the digest had no room to name, so the block can admit its own gaps. */
      rollups: WakeDigestRollupView[]
    }
  /** A timeline line marking that the thread's effective model changed between turns. */
  | { kind: 'modelChange'; model: string }
  /** A timeline line marking that older lookups left the model's context so this turn would
   * fit its budget: the reply was written with less than the whole chat in view. Live-stream
   * only — it describes one turn's assembly, so history doesn't replay it. */
  | { kind: 'contextTrimmed'; count: number }
  /** A finished rename batch, with the undo that reverses it. Live-stream only: undo
   * needs the batch's operation id, and a reopened thread would offer an Undo whose
   * batch may since have been undone elsewhere (the operation log is where past
   * batches are reversed from). */
  | {
      kind: 'renameApplied'
      /** The batch this line reports. */
      operationId: string
      /** Files this batch renamed. */
      fileCount: number
      /** Every still-undoable batch of this run, in the order they were APPLIED
       * (which is the order `undo_operations` needs to reverse them newest-first).
       * Non-empty only on the NEWEST such line, and only once a run has more than
       * one batch — so the job-wide undo appears once, at the bottom. */
      jobOperationIds: string[]
      /** Files across every batch in `jobOperationIds`. */
      jobFileCount: number
      undo: RenameUndoState
    }

/** Where a rename batch's undo stands. `partial` and `unavailable` are the honest
 * outcomes: undo never forces, so a file that changed since (or whose old name is
 * taken again) is left alone and said so. */
export type RenameUndoState =
  | { status: 'undoable' }
  | { status: 'undoing' }
  | { status: 'undone'; restored: number }
  | {
      status: 'partial'
      restored: number
      /** Files left alone: changed since the rename, or their old name is taken again. */
      skipped: number
      /** Batches that never ran an inverse at all (already undone, a volume gone).
       * Counted separately because a refused batch reports no per-file numbers, and
       * folding it into `skipped` would understate what was missed. */
      refusedBatches: number
      /** WHICH reason left which file alone, merged across the job's batches: one group
       * per reason, with its complete count and one example file name. Lets the line
       * name a file rather than a reason class. Empty when the backend recorded no
       * reasons, and then the line falls back to naming the class — the count is
       * reported either way. */
      skips: SkipBreakdown[]
    }
  /** Nothing was reversed: the batch was already undone, or a volume it needs is
   * disconnected. */
  | { status: 'unavailable' }
