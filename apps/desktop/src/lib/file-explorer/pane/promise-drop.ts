/**
 * Drops from other apps that hand over no files, only promises of them (an image
 * from Chrome or Safari, a Mail attachment): the pure half.
 *
 * wry's drop event arrives with EMPTY paths for these. The backend
 * (`src-tauri/src/drop_in/CLAUDE.md`) says on drag enter what the drag carries,
 * and on drop receives the files into a drop folder, reporting through
 * `promise-drop-started` / `-ready` / `-failed`. This module decides whether a
 * drag may light targets up, pairs wry's drop (which knows WHERE) with the
 * backend's events (which know WHAT), and words the Copy dialog's source line.
 * The controller (`drag-drop-controller.svelte.ts`) runs the actions.
 */

import { tString } from '#lib/intl/messages.svelte.js'
import { formatInteger } from '#lib/intl/number-format.js'
import type { DragPayloadKind, DropSource, PromiseDropFailure } from '#lib/tauri-commands/index.js'
import type { DropOutcome } from '../drag/drag-analytics'
import { lastName } from '#lib/path/grammar.js'

/**
 * Whether a drag carrying `kind` may light drop targets up. `null` (the native
 * side never said, which is every platform but macOS) keeps the old behavior.
 */
export function canAcceptDrag(kind: DragPayloadKind | null): boolean {
  return kind !== 'nothing'
}

/**
 * What an EXTERNAL drop with no paths was. A promise or image drop is waited
 * for; anything else had nothing usable, and ❌ never counts as a transfer.
 */
export function emptyDropOutcome(kind: DragPayloadKind | null): Extract<DropOutcome, 'promise' | 'nothingUsable'> {
  return kind === 'promises' || kind === 'imageData' ? 'promise' : 'nothingUsable'
}

/** The host of a web address, without `www.`; `null` for anything without one. */
export function hostOf(url: string | null | undefined): string | null {
  if (!url) return null
  try {
    const { protocol, hostname } = new URL(url)
    if (protocol !== 'http:' && protocol !== 'https:') return null
    return hostname.replace(/^www\./, '') || null
  } catch {
    return null
  }
}

function leafName(path: string): string {
  return lastName(path) || path
}

/**
 * The Copy dialog's source line for a promised drop: what arrived and where it
 * came from. ❌ Never the drop folder's path, which means nothing to anyone.
 */
export function dropSourceLabel(source: DropSource, paths: string[]): string {
  const appName = source.appName ?? null
  const host = hostOf(source.pageUrl)
  if (paths.length === 1) {
    const name = leafName(paths[0] ?? '')
    if (appName && host) return tString('fileExplorer.dropIn.sourceNameFromAppAndHost', { name, appName, host })
    if (appName) return tString('fileExplorer.dropIn.sourceNameFromApp', { name, appName })
    if (host) return tString('fileExplorer.dropIn.sourceNameFromHost', { name, host })
    return name
  }
  const count = { count: paths.length, countText: formatInteger(paths.length) }
  if (appName && host) return tString('fileExplorer.dropIn.sourceCountFromAppAndHost', { ...count, appName, host })
  if (appName) return tString('fileExplorer.dropIn.sourceCountFromApp', { ...count, appName })
  if (host) return tString('fileExplorer.dropIn.sourceCountFromHost', { ...count, host })
  return tString('fileExplorer.dropIn.sourceCount', count)
}

/** The signs-of-life line while a drop's files are on their way. */
export function receivingMessage(itemCount: number, appName: string | null): string {
  const count = { count: itemCount, countText: formatInteger(itemCount) }
  return appName
    ? tString('fileExplorer.dropIn.receivingFromApp', { ...count, appName })
    : tString('fileExplorer.dropIn.receiving', count)
}

/** Why a drop produced no file, in words. */
export function promiseDropFailureMessage(reason: PromiseDropFailure, appName: string | null): string {
  switch (reason) {
    case 'nothingReceived':
      return appName
        ? tString('fileExplorer.dropIn.nothingReceivedFromApp', { appName })
        : tString('fileExplorer.dropIn.nothingReceived')
    case 'timedOut':
      return tString('fileExplorer.dropIn.timedOut')
    case 'noDropFolder':
      return tString('fileExplorer.dropIn.noFolder')
  }
}

/** The warning beside the dialog when only some promised files arrived. */
export function someMissingMessage(failedCount: number): string {
  return tString('fileExplorer.dropIn.someMissing', { count: failedCount, countText: formatInteger(failedCount) })
}

/** Where a promised drop goes: the pane, and the folder row it landed on, if any. */
export interface PromiseDropTarget {
  pane: 'left' | 'right'
  folderPath?: string
}

/** What the controller does next for a promised drop. */
export type PromiseDropAction =
  /** Show signs of life: the files are on their way. */
  | { type: 'receiving'; dropId: string; itemCount: number; source: DropSource }
  /** The files are in: open the Copy dialog. */
  | {
      type: 'open'
      dropId: string
      target: PromiseDropTarget
      paths: string[]
      failedCount: number
      source: DropSource
    }
  /** Nobody wants these files: let the drop folder go. */
  | { type: 'release'; dropId: string }
  /** The drop produced nothing: say why. */
  | { type: 'failed'; dropId: string; reason: PromiseDropFailure; appName: string | null }

interface Started {
  dropId: string
  itemCount: number
  source: DropSource
}

interface Slot {
  started: Started
  /** `undefined`: wry's drop hasn't come yet. `null`: it came and hit no target. */
  target: PromiseDropTarget | null | undefined
  announced: boolean
  ready?: { paths: string[]; failedCount: number }
}

/**
 * Pairs wry's drop event with the backend's promise-drop events. The backend
 * emits `started` before wry's drop, and the files land later, but nothing here
 * relies on that order: each side waits for the other.
 */
export function createPromiseDropPairing() {
  const slots = new Map<string, Slot>()
  /** A drop that came before its `started` (`undefined`: none waiting). */
  let waitingTarget: { target: PromiseDropTarget | null } | undefined

  function settle(slot: Slot): PromiseDropAction[] {
    const { started } = slot
    if (slot.target === undefined) return []
    if (slot.target === null) {
      if (!slot.ready) return []
      slots.delete(started.dropId)
      return [{ type: 'release', dropId: started.dropId }]
    }
    const actions: PromiseDropAction[] = []
    if (!slot.announced) {
      slot.announced = true
      actions.push({ type: 'receiving', dropId: started.dropId, itemCount: started.itemCount, source: started.source })
    }
    if (slot.ready) {
      slots.delete(started.dropId)
      actions.push({
        type: 'open',
        dropId: started.dropId,
        target: slot.target,
        paths: slot.ready.paths,
        failedCount: slot.ready.failedCount,
        source: started.source,
      })
    }
    return actions
  }

  return {
    /** wry's drop of a promised drag: where it goes, or `null` for nowhere. */
    dropped(target: PromiseDropTarget | null): PromiseDropAction[] {
      const unplaced = [...slots.values()].reverse().find((s) => s.target === undefined)
      if (!unplaced) {
        waitingTarget = { target }
        return []
      }
      unplaced.target = target
      return settle(unplaced)
    },

    started(event: Started): PromiseDropAction[] {
      const slot: Slot = { started: event, target: waitingTarget?.target, announced: false }
      waitingTarget = undefined
      slots.set(event.dropId, slot)
      return settle(slot)
    },

    ready(event: { dropId: string; paths: string[]; failedCount: number }): PromiseDropAction[] {
      const slot = slots.get(event.dropId)
      if (!slot) return [{ type: 'release', dropId: event.dropId }]
      slot.ready = { paths: event.paths, failedCount: event.failedCount }
      return settle(slot)
    },

    failed(event: { dropId: string; reason: PromiseDropFailure }): PromiseDropAction[] {
      const slot = slots.get(event.dropId)
      let target: PromiseDropTarget | null | undefined
      let appName: string | null = null
      if (slot) {
        slots.delete(event.dropId)
        target = slot.target
        appName = slot.started.source.appName ?? null
      } else {
        // It never started (no drop folder): it's the drop that's waiting.
        target = waitingTarget?.target
        waitingTarget = undefined
      }
      if (!target) return []
      return [{ type: 'failed', dropId: event.dropId, reason: event.reason, appName }]
    },

    /**
     * A new drag entered. A drop still receiving keeps going, but one whose drop
     * event never came (or a drop that never got its `started`) is let go.
     */
    newDrag(): void {
      waitingTarget = undefined
      for (const slot of slots.values()) {
        if (slot.target === undefined) slot.target = null
      }
    },
  }
}
