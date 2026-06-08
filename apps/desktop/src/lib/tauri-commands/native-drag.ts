// Native drag event listeners. Typed `on*` wrappers over the `tauri-specta`
// drag events: the macOS drag-overlay events (`drag-image-size`,
// `drag-modifiers`) and the drag-out-to-Finder session toasts
// (`drag-out-session-started`, `drag-out-session-complete`).

import { type UnlistenFn } from '@tauri-apps/api/event'
import {
  events,
  type DragImageSize,
  type DragModifiers,
  type SessionCompleteEvent,
  type SessionStartedEvent,
} from '$lib/ipc/bindings'

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
