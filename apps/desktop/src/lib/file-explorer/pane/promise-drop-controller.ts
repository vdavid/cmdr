/**
 * Runs drops from other apps that hand over promises, not files (`promise-drop.ts`
 * is the pure half): listens for what each drag carries and for the backend's
 * `promise-drop-*` events, shows signs of life while the files arrive, and hands
 * the received files to the drag-drop controller's normal Copy path.
 */

import {
  onDragPayloadKind,
  onPromiseDropFailed,
  onPromiseDropReady,
  onPromiseDropStarted,
  releasePromiseDrop,
  type DragPayloadKind,
  type UnlistenFn,
} from '#lib/tauri-commands/index.js'
import { addToast, dismissToast } from '#lib/ui/toast/index.js'
import {
  createPromiseDropPairing,
  dropSourceLabel,
  promiseDropFailureMessage,
  receivingMessage,
  someMissingMessage,
  type PromiseDropAction,
  type PromiseDropTarget,
} from './promise-drop'

/** A drop's files in hand, ready for the Copy dialog. */
export interface ReceivedDrop {
  paths: string[]
  target: PromiseDropTarget
  /** The drop folder's id, released if the dialog never starts a copy. */
  dropId: string
  /** The dialog's source line (`dropSourceLabel`), shown in place of a path. */
  sourceDisplay: string
}

/**
 * How long a drop may take before the "Getting the dropped file…" toast shows.
 * A browser image usually lands well within it, and a toast that flashes up and
 * vanishes as the dialog opens is noise.
 */
const RECEIVING_TOAST_DELAY_MS = 400

export function createPromiseDropController(deps: { openTransfer: (drop: ReceivedDrop) => void }) {
  const pairing = createPromiseDropPairing()
  let payloadKind: DragPayloadKind | null = null
  const receivingTimers = new Map<string, ReturnType<typeof setTimeout>>()
  const unlisteners: UnlistenFn[] = []

  const toastId = (dropId: string) => `promise-drop-${dropId}`

  function stopReceiving(dropId: string) {
    const timer = receivingTimers.get(dropId)
    if (timer !== undefined) clearTimeout(timer)
    receivingTimers.delete(dropId)
    dismissToast(toastId(dropId))
  }

  function run(actions: PromiseDropAction[]) {
    for (const action of actions) {
      switch (action.type) {
        case 'receiving':
          receivingTimers.set(
            action.dropId,
            setTimeout(() => {
              receivingTimers.delete(action.dropId)
              addToast(receivingMessage(action.itemCount, action.source.appName ?? null), {
                id: toastId(action.dropId),
                level: 'info',
                dismissal: 'persistent',
              })
            }, RECEIVING_TOAST_DELAY_MS),
          )
          break
        case 'open':
          stopReceiving(action.dropId)
          if (action.failedCount > 0) addToast(someMissingMessage(action.failedCount), { level: 'warn' })
          deps.openTransfer({
            paths: action.paths,
            target: action.target,
            dropId: action.dropId,
            sourceDisplay: dropSourceLabel(action.source, action.paths),
          })
          break
        case 'release':
          void releasePromiseDrop(action.dropId)
          break
        case 'failed':
          stopReceiving(action.dropId)
          addToast(promiseDropFailureMessage(action.reason, action.appName), { level: 'warn' })
          break
      }
    }
  }

  return {
    /** Subscribes to the backend's events. Called from the drag-drop controller's `init()`. */
    async init(): Promise<void> {
      unlisteners.push(
        await onDragPayloadKind((kind) => {
          // Fires on every drag enter, before wry's own enter event.
          payloadKind = kind
          pairing.newDrag()
        }),
        await onPromiseDropStarted((event) => {
          run(pairing.started(event))
        }),
        await onPromiseDropReady((event) => {
          run(pairing.ready(event))
        }),
        await onPromiseDropFailed((event) => {
          run(pairing.failed(event))
        }),
      )
    },

    cleanup(): void {
      for (const unlisten of unlisteners.splice(0)) unlisten()
      for (const dropId of [...receivingTimers.keys()]) stopReceiving(dropId)
    },

    /** What the current drag carries, or `null` before the native side said (or off macOS). */
    payloadKind(): DragPayloadKind | null {
      return payloadKind
    },

    /** The drag left or dropped: the next one says its own kind. */
    endDrag(): void {
      payloadKind = null
    },

    /** wry's drop of a promised drag: where it goes, or `null` when it hit no target. */
    dropped(target: PromiseDropTarget | null): void {
      run(pairing.dropped(target))
    },
  }
}
