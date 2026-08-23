/**
 * Everything the main window subscribes to for the life of the window, and the one place that
 * tears it all down again.
 *
 * Extracted from `+page.svelte` for the same reason `listener-setup.ts` and `startup-gates.ts`
 * were: the component owns `$state` and markup, and a list of thirty start/stop pairs is
 * neither. State crosses the boundary through [`WindowServicesContext`] — getters for reads,
 * callbacks for writes — so nothing here captures a stale reactive value.
 *
 * ## Two start phases, on purpose
 *
 * [`startEarlyWindowServices`] runs at the very top of `onMount`, before the licence read and
 * the onboarding gate get their awaits in. Everything in it is fire-and-forget and wants to be
 * listening as early as it can: a wake can fire from launch replay, and a turn event that
 * arrives before the subscription is up is simply lost.
 *
 * [`startWindowServices`] runs after the document-level key handlers are wired, because it can
 * throw outside Tauri (Playwright smoke tests) and the handlers must survive that. It needs the
 * explorer handle and the command bus, which the early phase deliberately does not.
 *
 * ⚠️ **Every unlisten lands in this module's own array**, which [`stopWindowServices`] drains.
 * HMR re-runs `onMount`, so a listener that skips the array stacks a duplicate on every reload.
 */

import type { UnlistenFn } from '@tauri-apps/api/event'
import { startAskCmdrTurnStream, stopAskCmdrTurnStream } from '$lib/ask-cmdr/ask-cmdr-turn-stream.svelte'
import { startDragOutEventBridge } from '$lib/file-explorer/drag/drag-out-event-bridge'
import { startOsMountNoticeBridge } from '$lib/file-explorer/network/os-mount-notice-bridge'
import { initQuickLookListeners } from '$lib/file-explorer/quick-look/quick-look-state.svelte'
import { startOperationConflictHost, stopOperationConflictHost } from '$lib/file-operations/operation-conflict.svelte'
import {
  initOperationSessions,
  destroyOperationSessions,
} from '$lib/file-operations/operation-session/window-operation-sessions.svelte'
import {
  initMainWindowOperations,
  destroyMainWindowOperations,
} from '$lib/file-operations/queue/main-window-operations.svelte'
import { initSettledOperationsWatch, destroySettledOperationsWatch } from '$lib/file-operations/settled-operations'
import { startDownloadsEventBridge } from '$lib/downloads/event-bridge.svelte'
import { startGlobalShortcutBridge } from '$lib/downloads/global-shortcut-bridge.svelte'
import { initIndexState, destroyIndexState, initMediaEnrichState, destroyMediaEnrichState } from '$lib/indexing/index'
import { startLowDiskSpaceEventBridge } from '$lib/low-disk-space/event-bridge.svelte'
import { initSnapshotPurge, destroySnapshotPurge } from '$lib/search/snapshot-purge'
import { getSetting } from '$lib/settings'
import {
  startOperationFailureWatch,
  stopOperationFailureWatch,
} from '$lib/status-corner/operation-failure-watch.svelte'
import { startSuggestedOpsBadge, stopSuggestedOpsBadge } from '$lib/suggested-ops/suggested-ops-badge.svelte'
import type { CommandDispatchArgs, CommandId } from '$lib/commands'
import type { ExplorerAPI } from './explorer-api'
import {
  type ListenerSetupContext,
  makeListenTauri,
  setupMenuListeners,
  setupDialogListeners,
  setupWindowFocusListener,
} from './listener-setup'
import { startMenuOperationGate } from './menu-operation-gate.svelte'
import { setupMcpListeners } from './mcp-listeners'

/**
 * The seam between the main component (owns `$state`) and this module. Reads are getters so a
 * listener firing later sees the live value; writes are callbacks.
 */
export interface WindowServicesContext {
  /** Live read of the explorer handle (`undefined` until `DualPaneExplorer` mounts; HMR can swap it). */
  getExplorer: () => ExplorerAPI | undefined
  /** Dispatch through the same typed command bus the keyboard / palette / menu paths use. */
  dispatch: <K extends CommandId>(commandId: K, ...args: CommandDispatchArgs<K>) => Promise<void>
  /** Write-only dialog setters (the component owns the `$state`). */
  dialogs: {
    setAboutWindow: (show: boolean) => void
  }
  /** Re-runs the "What's new" startup trigger; component-owned because it reads startup-modal `$state`. */
  maybeRunWhatsNew: (force: boolean) => Promise<void>
}

/**
 * Every menu / MCP / dialog / bridge unlisten, drained by {@link stopWindowServices}. Shared
 * with `listener-setup.ts` and `setupMcpListeners` through the context they're handed, so a
 * single array carries the whole window's teardown.
 */
const unlistenFns: UnlistenFn[] = []

/** Tears down the native-menu enabled-state sync (HMR safety). */
let stopMenuGate: (() => void) | null = null

/**
 * Phase 1: the subscriptions that want to be up before anything awaits. All fire-and-forget —
 * nothing below can fail in a way the window has to know about.
 */
export function startEarlyWindowServices(): void {
  // Grey out the File menu's operation items while a dialog is up or Ask Cmdr has focus.
  // Chrome only; every real refusal is elsewhere.
  stopMenuGate = startMenuOperationGate()
  // Seed and subscribe the suggestions badge. The seed is not redundant with the subscription:
  // suggestions never expire, so a group proposed in an earlier session is already waiting
  // before anything emits.
  void startSuggestedOpsBadge()
  // Subscribe to every Ask Cmdr turn, this window only. It is what keeps the rail rendering a
  // turn it wasn't watching from the start (a reload, or a wake), and what tells the session
  // list a wake just opened a thread.
  void startAskCmdrTurnStream()
}

/**
 * Phase 2: the listeners and stores that need the explorer handle or the command bus.
 *
 * ⚠️ Called AFTER the document-level key handlers are registered: outside Tauri (Playwright
 * smoke tests) the first `listen` rejects, and the handlers must already be in place.
 */
export async function startWindowServices(ctx: WindowServicesContext): Promise<void> {
  const listenerCtx: ListenerSetupContext = {
    getExplorer: ctx.getExplorer,
    dispatch: ctx.dispatch,
    unlistenFns,
    dialogs: ctx.dialogs,
    maybeRunWhatsNew: ctx.maybeRunWhatsNew,
  }
  await setupMenuListeners(listenerCtx)
  await setupDialogListeners(listenerCtx)
  await setupMcpListeners({
    getExplorer: ctx.getExplorer,
    // The MCP adapter dispatches through the same typed command bus as the keyboard / palette /
    // menu paths, so MCP events get the uniform preamble (log + breadcrumb + search-results guard).
    dispatch: ctx.dispatch,
    listenTauri: makeListenTauri(unlistenFns),
    isAiEnabled: () => getSetting('ai.provider') !== 'off',
  })
  await initIndexState()
  // Image-enrichment progress joins the same top-right indicator, a second publisher;
  // listen-first-then-query, like initIndexState.
  await initMediaEnrichState()
  // The main window's own view of the operation registry: the same store and the same two
  // app-wide streams the queue window subscribes to, so corner status can read live operations
  // with no new event or IPC.
  await initMainWindowOperations()
  // This window's session registry: one session per operation, shared by every view of it.
  // Subscribed here, before anything can ask for a session, because its listeners are async and
  // an operation dispatched while they're being set up would go unheard.
  await initOperationSessions()
  // Stored search snapshots drop rows for files an operation removed, from the per-path outcome
  // stream. Independent of any pane or dialog: a snapshot outlives both.
  await initSnapshotPurge()
  // Remembers which operations have finished tearing down, so a follow-up that reads an
  // operation's journal rows knows when they became readable. Armed here because the settle for
  // an operation routinely lands before anything thinks to wait for it.
  await initSettledOperationsWatch()
  // Watches that store for operations that stopped before they were done, and says so. After
  // the store, so the first snapshot has somewhere to land before anything reads it.
  startOperationFailureWatch()
  // The main window's answer to a conflict in an operation no progress dialog is showing. After
  // the store too: it pauses and resumes off the rows, and reads them to name which operation
  // is asking.
  await startOperationConflictHost()
  await setupWindowFocusListener(listenerCtx)
  // Native Quick Look (macOS) event wiring: `quick-look-closed` flips `isOpen` on the state
  // singleton; `quick-look-key` routes panel keystrokes back into the focused pane (and
  // intercepts Shift+Space to close).
  unlistenFns.push(await initQuickLookListeners(ctx.getExplorer))
  // Downloads notifications event bridge: one `download-detected` listener that fans out to the
  // in-app toast and/or the macOS native notification per the current settings value.
  unlistenFns.push(await startDownloadsEventBridge(ctx.getExplorer()))
  // Global go-to-latest-download hotkey bridge (default ⌃⌥⌘J): one `global-shortcut-fired`
  // listener; routes through `goToLatestDownload` and shows the first-trigger warn toast when
  // `acknowledged === false`.
  unlistenFns.push(await startGlobalShortcutBridge(ctx.getExplorer()))
  // Low-disk-space warning bridge: one `low-disk-space` listener (the backend poller's
  // boot-volume hysteresis detector) dispatched to a persistent warn toast or a macOS
  // notification per the settings value.
  unlistenFns.push(await startLowDiskSpaceEventBridge())
  // OS-mount fallback notice: one `smb-fell-back-to-os-mount` listener turning the backend's
  // once-per-server signal into a persistent toast with a "Try connecting directly" button,
  // retired when the share goes direct.
  unlistenFns.push(await startOsMountNoticeBridge())
  // Drag-out completion bridge: one `drag-out-session-started` + `drag-out-session-complete`
  // pair per drag session, turned into a single signs-of-life → completion toast (downloading a
  // phone/NAS file to Finder shows nothing on Finder's side; this is our feedback surface).
  unlistenFns.push(await startDragOutEventBridge())
}

/** Tear down everything both phases started. Safe to call when neither ran. */
export function stopWindowServices(): void {
  stopSuggestedOpsBadge()
  stopAskCmdrTurnStream()
  destroyIndexState()
  destroyMediaEnrichState()
  stopOperationFailureWatch()
  stopOperationConflictHost()
  destroyMainWindowOperations()
  destroyOperationSessions()
  destroySnapshotPurge()
  destroySettledOperationsWatch()
  stopMenuGate?.()
  stopMenuGate = null
  // Clean up every menu / MCP / dialog / window-focus listener (prevents duplicate listeners
  // after HMR). All of them register into this one array.
  for (const unlisten of unlistenFns) {
    unlisten()
  }
  unlistenFns.length = 0
}
