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
  ProposalDecision,
  SkipBreakdown,
  WakeDigestFolderView,
  WakeDigestRollupView,
} from '#lib/tauri-commands/index.js'
import type { LineRef, ManagedAiRefusal, ToolOutcome, ToolSubject, TurnStats } from '#lib/ipc/bindings.js'

/** One tool call the assistant made, as its line shows it. The line's identity is its round's
 * `messageId` plus `callId`: call ids collide across rounds. */
export interface RailToolCall {
  callId: string
  /** The wire tool name; the localized label is derived in `ask-cmdr-labels.ts`. */
  tool: string
  running: boolean
  /** What the call looks at. `null` until the round resolves (live, only the name is known
   * while its arguments stream). */
  subject: ToolSubject | null
  /** What came back. `null` while running, and on a settled call no result was recorded for
   * (a stopped or crashed turn). Whether it got what it asked for is `outcomeOk`. */
  outcome: ToolOutcome | null
  /** The turn's earlier identical call this one repeats. */
  repeatOf: LineRef | null
}

/**
 * One model call of a turn: what it said, then the tools it called. A turn is one assistant
 * bubble made of rounds, so narration sits between the tool lines it explains.
 *
 * `messageId` is the round's persisted assistant row, `null` while it still streams. A round
 * with an id is SEALED: a later chunk opens a new round rather than writing into it. Its text
 * always renders before its calls, the order the stored row has whatever order the wire
 * interleaved them in.
 */
export interface Round {
  messageId: number | null
  text: string
  calls: RailToolCall[]
}

/** One assistant turn: its rounds, plus what the whole turn spent once it ended. */
export interface AssistantItem {
  kind: 'assistant'
  /** The turn's last persisted round, `null` until one lands. */
  id: number | null
  /** The opener row's id (the backend's turn identity), so a turn split across history pages
   * folds back into one bubble. `null` until the opener persists, and for rows before a
   * thread's first opener. */
  turnId: number | null
  rounds: Round[]
  thinking: boolean
  stalled: boolean
  /** The provider turned the call away as overloaded and the turn is waiting to try again:
   * `attempt` is the try about to start, of `attempts`, and `provider` the cloud provider's brand
   * name (absent for a custom endpoint). Absent while it isn't busy; the next chunk, tool line, or
   * the turn's end clears it. Live only, so a reload never shows it. */
  busy?: { attempt: number; attempts: number; provider?: string }
  streaming: boolean
  /** What the turn spent: `null` while it runs, and for a turn that ran no tool. */
  stats: TurnStats | null
}

/** One rendered item in the thread. `attachments` on a user turn are the chips shown
 * under the sent message; history rows carry none (the refs rode into the envelope, not
 * stored blocks). */
export type RailMessage =
  | { kind: 'user'; id: number | null; text: string; attachments: AttachmentRef[] }
  | AssistantItem
  | {
      kind: 'error'
      errorKind: AskCmdrErrorKind
      /** The provider's own wording, shown as escaped plain text under the friendly
       * headline so the user sees what to fix. Display only; never branched on. */
      detail?: string
      /** Which rule of the organization's policy refused, for `managedByOrganization`. */
      managed?: ManagedAiRefusal
      /** The cloud provider's brand name, for `requestTooLarge`, `modelNotInPlan`, and `providerBusy`, read when
       * the turn failed so a later provider switch can't rename it. Absent for a custom endpoint or a local server. */
      provider?: string
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
  /** What the user answered when the agent suggested something. Two transcript rows land
   * here on purpose: an `event` row carrying one decision as it happens, and the opener of
   * the follow-up turn a rejected sweep earns, carrying a whole sweep's worth.
   *
   * ⚠️ **Verbs, counts, and the group's own display text, never a sentence.** The English the
   * model reads is a prompt and never crosses IPC. */
  | { kind: 'proposalDecisions'; id: number | null; decisions: ProposalDecision[] }
  /** A timeline line marking that the thread's effective model changed between turns. */
  | { kind: 'modelChange'; model: string }
  /** A timeline line marking that the chat memory size changed between turns, so every reply
   * after it carries a different amount of the conversation. A NUMBER from the backend: the
   * rail owns the sentence around it. */
  | { kind: 'chatMemoryChange'; chatMemoryTokens: number }
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
      /** Renames the batch left out because they swap names with each other: a
       * batch that runs as a move (a rename that copies on S3) can't swap. */
      swapsSkipped: number
      undo: RenameUndoState
    }

/** Where a rename batch's undo stands. `partial` and `unavailable` are the honest
 * outcomes: undo never forces, so a file that changed since (or whose old name is
 * taken again) is left alone and said so. */
export type RenameUndoState =
  | { status: 'undoable' }
  /** The operation log was off when the batch ran, so it was never journaled and can't be undone. */
  | { status: 'notRecorded' }
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
