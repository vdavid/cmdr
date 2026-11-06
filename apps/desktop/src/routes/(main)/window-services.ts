/**
 * Everything the main window subscribes to for the life of the window, each start paired with
 * the teardown it hands to the page's mount scope.
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
 * ⚠️ **Every teardown lands in the page's `MountScope`** (`#lib/utils/mount-scope.js`), which
 * `onDestroy` disposes. A subscription that skips it outlives the page: HMR and a route change
 * remount it, and each remount stacks a duplicate. The scope is per mount, so a start still in
 * flight when its page unmounted registers into a scope that's already disposed, and the
 * subscription stops on the spot instead of leaking.
 */

import type { MountScope } from '#lib/utils/mount-scope.js'
import { startMainWindowVisibilityReport } from './main-window-visibility'
import { startAskCmdrTurnStream, stopAskCmdrTurnStream } from '#lib/ask-cmdr/ask-cmdr-turn-stream.svelte.js'
import { startWakeIndicator, stopWakeIndicator } from '#lib/ask-cmdr/wake-indicator.svelte.js'
import { startWakeToast, stopWakeToast } from '#lib/ask-cmdr/wake-toast.svelte.js'
import { startSecurityNotices, stopSecurityNotices } from '#lib/security-notices/security-notices.js'
import { startStateRecoveryNotices, stopStateRecoveryNotices } from '#lib/state-recovery/state-recovery.js'
import { startDragOutEventBridge } from '#lib/file-explorer/drag/drag-out-event-bridge.js'
import { startOsMountNoticeBridge } from '#lib/file-explorer/network/os-mount-notice-bridge.js'
import { startLeftoverNoticeBridge } from '#lib/file-operations/leftover-notice-bridge.js'
import { startOpenWithCopyBridge } from '#lib/file-explorer/open-with-copy-bridge.js'
import { initQuickLookListeners } from '#lib/file-explorer/quick-look/quick-look-state.svelte.js'
import {
  startOperationConflictHost,
  stopOperationConflictHost,
} from '#lib/file-operations/operation-conflict.svelte.js'
import { startSourcesGoneHost, stopSourcesGoneHost } from '#lib/file-operations/sources-gone.svelte.js'
import { startLockedSourcesHost, stopLockedSourcesHost } from '#lib/file-operations/locked-sources.svelte.js'
import {
  initOperationSessions,
  destroyOperationSessions,
} from '#lib/file-operations/operation-session/window-operation-sessions.svelte.js'
import {
  initMainWindowOperations,
  destroyMainWindowOperations,
} from '#lib/file-operations/queue/main-window-operations.svelte.js'
import { initSettledOperationsWatch, destroySettledOperationsWatch } from '#lib/file-operations/settled-operations.js'
import { startDownloadsEventBridge } from '#lib/downloads/event-bridge.svelte.js'
import { startGlobalShortcutBridge } from '#lib/downloads/global-shortcut-bridge.svelte.js'
import {
  initIndexState,
  destroyIndexState,
  initMediaEnrichState,
  destroyMediaEnrichState,
  initSpacePause,
  destroySpacePause,
} from '#lib/indexing/index.js'
import { startLowDiskSpaceEventBridge } from '#lib/low-disk-space/event-bridge.svelte.js'
import { startOpenTerminalMenuGate } from '#lib/open-terminal/menu-gate.svelte.js'
import { startSortMenuSync } from '#lib/file-explorer/pane/sort-menu-sync.svelte.js'
import { initSnapshotPurge, destroySnapshotPurge } from '#lib/search/snapshot-purge.js'
import { getSetting } from '#lib/settings/index.js'
import { drainPendingReveals } from '#lib/tauri-commands/index.js'
import { startRevealActivationNotice } from '#lib/reveal/reveal-activation-notice.js'
import { startRevealTabsDroppedNotice } from '#lib/reveal/reveal-tabs-dropped-notice.js'
import { startRevealNotShownNotice } from '#lib/reveal/reveal-not-shown-notice.js'
import { refreshCliCommandName } from '#lib/cli-command/toggle-cli-command.js'
import {
  startOperationFailureWatch,
  stopOperationFailureWatch,
} from '#lib/status-corner/operation-failure-watch.svelte.js'
import { startSuggestedOpsBadge, stopSuggestedOpsBadge } from '#lib/suggested-ops/suggested-ops-badge.svelte.js'
import type { CommandDispatchers, DialogsOnScreen } from './command-dispatch-context'
import type { ExplorerAPI } from './explorer-api'
import {
  type ListenerSetupContext,
  makeListenTauri,
  setupMenuListeners,
  setupDialogListeners,
  setupMouseNavListener,
  setupWindowFocusListener,
} from './listener-setup'
import { startMenuOperationGate } from './menu-operation-gate.svelte'
import { startMenuDialogGate } from './menu-dialog-gate.svelte'
import { setupMcpListeners } from './mcp-listeners'

/**
 * The seam between the main component (owns `$state`) and this module. Reads are getters so a
 * listener firing later sees the live value; writes are callbacks.
 */
export interface WindowServicesContext {
  /** Live read of the explorer handle (`undefined` until `DualPaneExplorer` mounts; HMR can swap it). */
  getExplorer: () => ExplorerAPI | undefined
  /**
   * One dispatcher per road (`command-dispatch-context.ts`). The menu and mouse listeners take
   * theirs, which absorb a rejection: a handler that rejects on a user gesture has already said
   * its piece in a toast. ⚠️ The MCP adapter takes `mcp`, the one that PROPAGATES it. A few
   * handlers reject on purpose so the adapter can report the real outcome back to the agent
   * (`select` names a file that isn't in the listing; `pane.refresh` when a re-read outlives its
   * wait). ❌ Never hand it a gesture dispatcher: an absorbed rejection turns every one of those
   * refusals into `ok: true`.
   */
  dispatchers: CommandDispatchers
  /** Write-only dialog setters (the component owns the `$state`). */
  dialogs: {
    setAboutWindow: (show: boolean) => void
  }
  /** Re-runs the "What's new" startup trigger; component-owned because it reads startup-modal `$state`. */
  maybeRunWhatsNew: (force: boolean) => Promise<void>
  /** Live read of what's on screen, so the native menu greys out what the dialog gate refuses. */
  getDialogsOnScreen: () => DialogsOnScreen
}

/**
 * Phase 1: the subscriptions that want to be up before anything awaits. All fire-and-forget —
 * nothing below can fail in a way the window has to know about. Each one's stop goes into
 * `scope`, ahead of its start, so a teardown landing while one is still connecting reaches it.
 */
export function startEarlyWindowServices(scope: MountScope): void {
  // Tell the backend whether anyone can see this window, so the idle work that only redraws it
  // (disk space, listing sizes) holds while hidden. Early: the first answer should beat the first poll.
  scope.add(startMainWindowVisibilityReport())
  // Grey out the File menu's operation items while a dialog is up or Ask Cmdr has focus.
  // Chrome only; every real refusal is elsewhere.
  scope.add(startMenuOperationGate())
  // Grey out the File menu's "Open terminal here" while the focused pane sits on a
  // phone, the network browser, or a search snapshot. Chrome only, same as above.
  scope.add(startOpenTerminalMenuGate())
  // Check View > Sort by's column and order on what the focused pane asked for.
  scope.add(startSortMenuSync())
  // Seed and subscribe the suggestions badge. The seed is not redundant with the subscription:
  // suggestions never expire, so a group proposed in an earlier session is already waiting
  // before anything emits.
  scope.add(stopSuggestedOpsBadge)
  void startSuggestedOpsBadge()
  // Subscribe to every Ask Cmdr turn, this window only. It is what keeps the rail rendering a
  // turn it wasn't watching from the start (a reload, or a wake), and what tells the session
  // list a wake just opened a thread.
  scope.add(stopAskCmdrTurnStream)
  void startAskCmdrTurnStream()
  // The status corner's wake indicator: whether a wake is thinking right now, and which gate is
  // in the way when the agent can't. Same reason as above, plus a gate can move before this
  // window is up (the API key is set in another window entirely).
  scope.add(stopWakeIndicator)
  void startWakeIndicator()
  // The one time the proactive agent interrupts: it staged something to review. Nothing at all
  // for a wake that stayed quiet, and nothing when `askCmdr.wakeToast` is off. Early, for the
  // same reason as the two above: a wake can fire from launch replay.
  scope.add(stopWakeToast)
  void startWakeToast()
  // Tell the person when Cmdr turned an access attempt away (a web page at the MCP server, a
  // refused server reply, a fake update). This window only, or the settings window would raise
  // its own copy. Early: an update check can refuse one right at launch.
  scope.add(stopSecurityNotices)
  void startSecurityNotices()
  // Tell the person once when Cmdr couldn't read one of its own state files and started it fresh.
  // Launch finds most of them before this window exists, so this drains the backend's queue.
  scope.add(stopStateRecoveryNotices)
  void startStateRecoveryNotices()
}

/**
 * One phase-2 start. It registers its own teardown in the scope: a start/stop pair registers
 * the stop BEFORE awaiting the start (a teardown mid-start still reaches it), and a start that
 * hands back its unlisten registers that AFTER (a disposed scope runs it on the spot).
 */
type WindowServiceStep = (scope: MountScope) => Promise<void> | void

/**
 * Phase 2: the listeners and stores that need the explorer handle or the command bus.
 *
 * ⚠️ Called AFTER the document-level key handlers are registered: outside Tauri (Playwright
 * smoke tests) the first `listen` rejects, and the handlers must already be in place.
 *
 * The steps run in order, each awaited, and none starts once `scope` is disposed: the page can
 * unmount while this is parked on an await, and a step started after that would hold its
 * subscriptions for good (a remount then stacked a second `execute-command` listener, running
 * every menu command twice). A new step goes in the list, where the runner gates it.
 */
export async function startWindowServices(scope: MountScope, ctx: WindowServicesContext): Promise<void> {
  const listenerCtx: ListenerSetupContext = {
    getExplorer: ctx.getExplorer,
    dispatchers: ctx.dispatchers,
    scope,
    dialogs: ctx.dialogs,
    maybeRunWhatsNew: ctx.maybeRunWhatsNew,
  }
  const steps: WindowServiceStep[] = [
    // Grey out every menu item the dialog gate refuses while something is open in front of the
    // panes. Chrome for the regular items; the dispatch core refuses the commands itself.
    (s) => {
      s.add(startMenuDialogGate(ctx.getDialogsOnScreen))
    },
    () => setupMenuListeners(listenerCtx),
    () => setupDialogListeners(listenerCtx),
    // macOS: the mouse's back / forward side buttons, which only AppKit sees.
    () => setupMouseNavListener(listenerCtx),
    () =>
      setupMcpListeners({
        getExplorer: ctx.getExplorer,
        // The MCP adapter dispatches through the same typed command bus as the keyboard / palette
        // / menu paths, so MCP events get the uniform preamble (log + breadcrumb + search-results
        // guard) — but through the strict dispatcher, so a refusal reaches the agent as one.
        dispatch: ctx.dispatchers.mcp,
        listenTauri: makeListenTauri(scope),
        isAiEnabled: () => getSetting('ai.provider') !== 'off',
      }),
    async (s) => {
      s.add(destroyIndexState)
      await initIndexState()
    },
    // Which drives are paused for disk space, for the folder-size warning (`#lib/indexing/space-pause.svelte.ts`).
    async (s) => {
      s.add(destroySpacePause)
      await initSpacePause()
    },
    // Image-enrichment progress joins the same top-right indicator, a second publisher;
    // listen-first-then-query, like initIndexState.
    async (s) => {
      s.add(destroyMediaEnrichState)
      await initMediaEnrichState()
    },
    // The main window's own view of the operation registry: the same store and the same two
    // app-wide streams the queue window subscribes to, so corner status can read live operations
    // with no new event or IPC.
    async (s) => {
      s.add(destroyMainWindowOperations)
      await initMainWindowOperations()
    },
    // This window's session registry: one session per operation, shared by every view of it.
    // Subscribed here, before anything can ask for a session, because its listeners are async and
    // an operation dispatched while they're being set up would go unheard.
    async (s) => {
      s.add(destroyOperationSessions)
      await initOperationSessions()
    },
    // Stored search snapshots drop rows for files an operation removed, from the per-path outcome
    // stream. Independent of any pane or dialog: a snapshot outlives both.
    async (s) => {
      s.add(destroySnapshotPurge)
      await initSnapshotPurge()
    },
    // Remembers which operations have finished tearing down, so a follow-up that reads an
    // operation's journal rows knows when they became readable. Armed here because the settle
    // for an operation routinely lands before anything thinks to wait for it.
    async (s) => {
      s.add(destroySettledOperationsWatch)
      await initSettledOperationsWatch()
    },
    // Watches that store for operations that stopped before they were done, and says so. After
    // the store, so the first snapshot has somewhere to land before anything reads it.
    (s) => {
      s.add(stopOperationFailureWatch)
      startOperationFailureWatch()
    },
    // The main window's answer to a conflict in an operation no progress dialog is showing.
    // After the store too: it pauses and resumes off the rows, and reads them to name which
    // operation is asking.
    async (s) => {
      s.add(stopOperationConflictHost)
      await startOperationConflictHost()
    },
    // The question a queued operation asks when some of its sources left while it waited. After
    // the store, like the conflict host: it names the operation from the rows and drops a
    // question whose operation left them.
    async (s) => {
      s.add(stopSourcesGoneHost)
      await startSourcesGoneHost()
    },
    // The question a move asks, before any byte moves, when some of what it moves is Locked.
    // After the store for the same reason.
    async (s) => {
      s.add(stopLockedSourcesHost)
      await startLockedSourcesHost()
    },
    () => setupWindowFocusListener(listenerCtx),
    // Native Quick Look (macOS) event wiring: `quick-look-closed` flips `isOpen` on the state
    // singleton; `quick-look-key` routes panel keystrokes back into the focused pane, and the
    // Quick Look key down the keyboard road (a panel keypress is a keypress).
    async (s) => {
      s.add(await initQuickLookListeners(ctx.getExplorer, ctx.dispatchers.keyboard))
    },
    // Downloads notifications event bridge: one `download-detected` listener that fans out to
    // the in-app toast and/or the macOS native notification per the current settings value.
    async (s) => {
      s.add(await startDownloadsEventBridge(ctx.getExplorer()))
    },
    // Global go-to-latest-download hotkey bridge (default ⌃⌥⌘J): one `global-shortcut-fired`
    // listener; routes through `goToLatestDownload` and shows the first-trigger warn toast when
    // `acknowledged === false`.
    async (s) => {
      s.add(await startGlobalShortcutBridge(ctx.getExplorer()))
    },
    // Low-disk-space warning bridge: one `low-disk-space` listener (the backend poller's
    // boot-volume hysteresis detector) dispatched to a persistent warn toast or a macOS
    // notification per the settings value.
    async (s) => {
      s.add(await startLowDiskSpaceEventBridge())
    },
    // OS-mount fallback notice: one `smb-fell-back-to-os-mount` listener turning the backend's
    // once-per-server signal into a persistent toast with a "Try connecting directly" button,
    // retired when the share goes direct, goes away, or has its direct connection switched off.
    async (s) => {
      s.add(await startOsMountNoticeBridge())
    },
    // Kept-leftovers notice: one `leftovers-kept` listener turning the leftover sweep's
    // "a drive came back holding something an unfinished transfer left, and Cmdr kept it"
    // into an info toast. Belongs to no operation, so it has no transfer toast to join.
    async (s) => {
      s.add(await startLeftoverNoticeBridge())
    },
    // "Open with" copy notices: `open-with-copy-refused` turns a file inside an archive that
    // couldn't be copied out for the chosen app (too big, locked, damaged) into a warning toast,
    // and `open-with-copy-launched` says the app got a read-only copy. The native menu launches
    // from Rust, so these events are all the frontend hears.
    async (s) => {
      s.add(await startOpenWithCopyBridge())
    },
    // Drag-out completion bridge: one `drag-out-session-started` + `drag-out-session-complete`
    // pair per drag session, turned into a single signs-of-life → completion toast (downloading
    // a phone/NAS file to Finder shows nothing on Finder's side; this is our feedback surface).
    async (s) => {
      s.add(await startDragOutEventBridge())
    },
    // The once-ever notice for the first reveal that actually lands here. ⚠️ Must be
    // subscribed BEFORE the drain below, or a cold-launch reveal — the very first one many
    // people ever see — delivers with nobody listening. See `#lib/reveal/CLAUDE.md`.
    async (s) => {
      s.add(await startRevealActivationNotice())
    },
    // Same ordering rule: a cold launch from "Open in Cmdr" with more folders than fit.
    async (s) => {
      s.add(await startRevealTabsDroppedNotice())
    },
    // Same ordering rule: a cold-launch reveal that can't show its file says so (#1020).
    async (s) => {
      s.add(await startRevealNotShownNotice())
    },
    // A "Show in Cmdr" that arrived before this window existed: a cold launch delivers the OS
    // event long before we mount, so the backend parks it and this drains it. ⚠️ Must stay
    // AFTER `setupMcpListeners`: the reveal is delivered over `mcp-nav-to-path`, so draining
    // earlier would emit into a window with no listener. Fire-and-forget: the backend answers
    // straight away and does the pane move on its own runtime. See `src-tauri/src/reveal/CLAUDE.md`.
    () => {
      void drainPendingReveals()
    },
    // The palette's "Install “cmdr” command" row names what it would do; it learns where the
    // command stands here and on every focus-gain (`listener-setup.ts`).
    () => {
      void refreshCliCommandName()
    },
  ]
  for (const step of steps) {
    if (scope.isDisposed()) return
    await step(scope)
  }
}
