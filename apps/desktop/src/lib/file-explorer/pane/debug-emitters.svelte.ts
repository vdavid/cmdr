/**
 * Dev-only bridge that mirrors per-pane navigation history and closed-tab stacks
 * to the debug window via Tauri events. Two reactive `$effect`s, lifted out of
 * `DualPaneExplorer` so the component body isn't carrying dev instrumentation.
 *
 * Both effects no-op outside `import.meta.env.DEV` and in test mode, so there's
 * nothing to unit-test (the guard returns before any observable work) — hence the
 * coverage-allowlist entry. Created synchronously during component init (the
 * `initListingDiffSync` / `initPersistenceSubscriber` pattern): the factory needs
 * Svelte's effect-tracking context, so it must be called in the component body,
 * never in `onMount`.
 */

import { untrack } from 'svelte'
import type { NavigationHistory } from '../navigation/navigation-history'
import type { TabManager } from '../tabs/tab-state-manager.svelte'

export interface DebugEmittersDeps {
  getLeftHistory: () => NavigationHistory
  getRightHistory: () => NavigationHistory
  getLeftTabMgr: () => TabManager
  getRightTabMgr: () => TabManager
  getFocusedPane: () => 'left' | 'right'
}

/** Wires the two dev-only debug-window emitters. Call synchronously during init. */
export function initDebugEmitters(deps: DebugEmittersDeps): void {
  // Emit history state to debug window (dev mode only, skip in tests)
  $effect(() => {
    if (!import.meta.env.DEV || import.meta.env.MODE === 'test') return
    // Read the reactive values
    const left = deps.getLeftHistory()
    const right = deps.getRightHistory()
    const focused = deps.getFocusedPane()
    // Emit without tracking to avoid infinite loops
    untrack(() => {
      void import('@tauri-apps/api/event').then(({ emit }) => {
        void emit('debug-history', { left, right, focusedPane: focused })
      })
    })
  })

  // Emit closed-tab stacks to debug window (dev mode only, skip in tests)
  $effect(() => {
    if (!import.meta.env.DEV || import.meta.env.MODE === 'test') return
    // Snapshot reads every property, setting up reactivity on push/pop/mutate.
    // It also produces plain JSON so Tauri's event channel can serialize it;
    // raw `$state` proxies + nested NavigationHistory throw on structured-clone.
    const leftSnap = $state.snapshot(deps.getLeftTabMgr().closedStack)
    const rightSnap = $state.snapshot(deps.getRightTabMgr().closedStack)
    const focused = deps.getFocusedPane()
    untrack(() => {
      void import('@tauri-apps/api/event').then(({ emit }) => {
        void emit('debug-closed-tabs', { left: leftSnap, right: rightSnap, focusedPane: focused })
      })
    })
  })
}
