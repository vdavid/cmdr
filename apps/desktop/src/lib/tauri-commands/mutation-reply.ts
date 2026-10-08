/**
 * Waiting an instant mutation (new folder, new file, rename, paste-as-file) out
 * to its real end.
 *
 * The backend answers within a short deadline. Past it the reply is
 * `stillRunning` with a `pendingId`, and a settle event carrying that id says
 * how the work ended (`mutation-settled`, or `clipboard-paste-settled`, which
 * also names the file): it may well land seconds later, so this is never a
 * failure. Backend side: `src-tauri/src/file_system/write_operations/mutation_reply.rs`.
 */

import type { UnlistenFn } from '@tauri-apps/api/event'
import {
  events,
  type MutationError,
  type MutationReply,
  type PasteClipboardReply,
  type PastedClipboardFile,
} from '$lib/ipc/bindings'
import { throwMutationError } from '$lib/file-operations/mutation-error'

/** A command's typed result, as the bindings return it. */
type Answer<R> = { status: 'ok'; data: R } | { status: 'error'; error: MutationError }

/** How a late end arrives, whichever settle event carried it. */
type LateEnd<V> = { type: 'landed'; value: V } | { type: 'refused'; error: MutationError }

export interface MutationWaitOptions {
  /**
   * Called once when the volume hasn't answered within the backend's reply
   * deadline. The wait goes on; this is the moment to tell the person it's slow.
   */
  onStillRunning?: () => void
}

/**
 * Runs `call` and resolves with its value once the work landed, or throws its
 * typed refusal (`MutationFailure`), however long that takes.
 *
 * The listener goes up BEFORE the call: the backend settles from another task,
 * so the event can overtake the reply, and a listener added after it would wait
 * forever. Settles that arrive first wait in `early` until the reply names the id.
 */
async function awaitReply<V>(
  listen: (onEnd: (pendingId: string, end: LateEnd<V>) => void) => Promise<UnlistenFn>,
  call: () => Promise<Answer<{ type: 'done'; value: V } | { type: 'stillRunning'; pendingId: string }>>,
  options?: MutationWaitOptions,
): Promise<V> {
  const early = new Map<string, LateEnd<V>>()
  let waiting: { pendingId: string; resolve: (end: LateEnd<V>) => void } | null = null
  const unlisten = await listen((pendingId, end) => {
    if (waiting?.pendingId === pendingId) waiting.resolve(end)
    else early.set(pendingId, end)
  })
  try {
    const res = await call()
    if (res.status === 'error') throwMutationError(res.error)
    const reply = res.data
    if (reply.type === 'done') return reply.value
    options?.onStillRunning?.()
    const end =
      early.get(reply.pendingId) ??
      (await new Promise<LateEnd<V>>((resolve) => {
        waiting = { pendingId: reply.pendingId, resolve }
      }))
    if (end.type === 'refused') throwMutationError(end.error)
    return end.value
  } finally {
    unlisten()
  }
}

/** [`awaitReply`] for `create_directory`, `create_file`, and `rename_file`. */
export async function awaitMutation(
  call: () => Promise<Answer<MutationReply>>,
  options?: MutationWaitOptions,
): Promise<void> {
  await awaitReply<undefined>(
    (onEnd) =>
      events.mutationSettled.listen(({ payload: { pendingId, outcome } }) => {
        onEnd(pendingId, outcome.type === 'landed' ? { type: 'landed', value: undefined } : outcome)
      }),
    async () => {
      const res = await call()
      if (res.status === 'error') return res
      const reply = res.data
      return { status: 'ok', data: reply.type === 'done' ? { type: 'done', value: undefined } : reply }
    },
    options,
  )
}

/** [`awaitReply`] for `paste_clipboard_as_file`: the created file, or `null` for nothing pasteable. */
export async function awaitClipboardPaste(
  call: () => Promise<Answer<PasteClipboardReply>>,
  options?: MutationWaitOptions,
): Promise<PastedClipboardFile | null> {
  return awaitReply<PastedClipboardFile | null>(
    (onEnd) =>
      events.clipboardPasteSettled.listen(({ payload: { pendingId, outcome } }) => {
        onEnd(pendingId, outcome.type === 'landed' ? { type: 'landed', value: outcome.file } : outcome)
      }),
    async () => {
      const res = await call()
      if (res.status === 'error') return res
      const reply = res.data
      return { status: 'ok', data: reply.type === 'done' ? { type: 'done', value: reply.file } : reply }
    },
    options,
  )
}
