/**
 * The Move dialog a rename chain hands its renames that copy on the server
 * (S3), with the two rules that keep it from getting in the way.
 *
 * - **One per chain.** The first rename whose save answers `confirm-move`
 *   claims it; the chain's others keep their names, said so in its toast.
 *   Confirming one starts a move that holds the progress slot, so a second
 *   dialog couldn't open behind it anyway.
 * - **Never over the editor.** A claimed rename whose save lands while an
 *   editor is open waits for it to close, since a dialog can't open over a
 *   name the user is typing (`../rename/DETAILS.md`).
 */

import type { RenameAsMoveRequest } from './rename-flow.svelte'

export function createChainMoveDialog(deps: {
  isEditorOpen: () => boolean
  open: (request: RenameAsMoveRequest) => void
}) {
  let claimed = false
  let deferred: RenameAsMoveRequest | null = null

  function offer() {
    if (deps.isEditorOpen() || deferred === null) return
    const request = deferred
    deferred = null
    deps.open(request)
  }

  return {
    /** A new chain gets its own dialog, unless the last one's still waits. */
    startChain(): void {
      claimed = deferred !== null
    },
    /** Claims the chain's one dialog; `false` when a rename already has it. */
    claim(): boolean {
      if (claimed) return false
      claimed = true
      return true
    },
    /** Opens `request`'s dialog now if no editor is open, else once it closes. */
    openWhenFree(request: RenameAsMoveRequest): void {
      deferred = request
      offer()
    },
    /**
     * The editor just closed. A waiting dialog opens a microtask later, so the
     * focus hand-back that follows a close lands first and the dialog keeps the
     * focus it takes.
     */
    editorClosed(): void {
      if (deferred !== null) queueMicrotask(offer)
    },
  }
}
