// Native drag event listeners. Typed `on*` wrappers over the `tauri-specta`
// drag events: the macOS drag-overlay events (`drag-image-size`,
// `drag-modifiers`), the drag-out-to-Finder session toasts
// (`drag-out-session-started`, `drag-out-session-complete`), and a drop from
// another app that carries file promises (`drag-payload-kind`,
// `promise-drop-*`, `src-tauri/src/drop_in/CLAUDE.md`).

import { type UnlistenFn } from '@tauri-apps/api/event'
import {
  commands,
  events,
  type DragPayloadKind,
  type PromiseDropFailed,
  type PromiseDropReady,
  type PromiseDropStarted,
  type DragImageSize,
  type DragModifiers,
  type SessionCompleteEvent,
  type SessionStartedEvent,
} from '#lib/ipc/bindings.js'

/** The OS drag image's pixel dimensions, read on drag enter. */
export function onDragImageSize(handler: (payload: DragImageSize) => void): Promise<UnlistenFn> {
  return events.dragImageSize.listen((event) => {
    handler(event.payload)
  })
}

/** Modifier-key state during a drag (alt / cmd / shift), emitted when it changes. */
export function onDragModifiers(handler: (payload: DragModifiers) => void): Promise<UnlistenFn> {
  return events.dragModifiers.listen((event) => {
    handler(event.payload)
  })
}

/** A drag-out-to-Finder session's first fulfillment began (signs-of-life toast). */
export function onDragOutSessionStarted(handler: (payload: SessionStartedEvent) => void): Promise<UnlistenFn> {
  return events.dragOutSessionStarted.listen((event) => {
    handler(event.payload)
  })
}

/** A drag-out-to-Finder session drained, with the folded per-item outcome counts. */
export function onDragOutSessionComplete(handler: (payload: SessionCompleteEvent) => void): Promise<UnlistenFn> {
  return events.dragOutSessionComplete.listen((event) => {
    handler(event.payload)
  })
}

/** What an incoming drag carries, read on drag enter (before wry's own enter event). */
export function onDragPayloadKind(handler: (kind: DragPayloadKind) => void): Promise<UnlistenFn> {
  return events.dragPayloadKind.listen((event) => {
    handler(event.payload.kind)
  })
}

/** A promise or image drop landed and the backend started receiving it. */
export function onPromiseDropStarted(handler: (payload: PromiseDropStarted) => void): Promise<UnlistenFn> {
  return events.promiseDropStarted.listen((event) => {
    handler(event.payload)
  })
}

/** A promised drop's files are on disk. */
export function onPromiseDropReady(handler: (payload: PromiseDropReady) => void): Promise<UnlistenFn> {
  return events.promiseDropReady.listen((event) => {
    handler(event.payload)
  })
}

/** A promised drop produced no file. */
export function onPromiseDropFailed(handler: (payload: PromiseDropFailed) => void): Promise<UnlistenFn> {
  return events.promiseDropFailed.listen((event) => {
    handler(event.payload)
  })
}

/** Lets an unclaimed drop folder go (dialog cancelled, no target, refused). Never throws. */
export async function releasePromiseDrop(dropId: string): Promise<void> {
  try {
    await commands.releasePromiseDrop(dropId)
  } catch {
    // The startup sweep takes whatever this misses.
  }
}
