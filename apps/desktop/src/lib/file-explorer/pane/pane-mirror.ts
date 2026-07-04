/**
 * "Copy path from <source> to <target> pane": mirror the source pane's location
 * (volume + path + network state) into the target pane WITHOUT shifting keyboard
 * focus. Lifted out of `DualPaneExplorer`; the component keeps the one-line
 * `export function copyPathBetweenPanes` delegate.
 *
 * When the source pane is focused, the cursor refines the destination:
 * cursor-on-folder uses the folder's path; cursor-on-server (network browser)
 * sets the target's selected host; cursor-on-share (share browser) queues
 * auto-mount on the target. All navigation uses `source: 'mirror'`, which keeps
 * focus on the source pane (no focus shift, L1); `restoreFocus` re-anchors STORE
 * focus to wherever it was, so the user keeps working where they were.
 */

import { getCurrentEntry } from '../navigation/navigation-history'
import { pushHistoryEntry } from '../tabs/tab-state-manager.svelte'
import type { NavigationHistory } from '../navigation/navigation-history'
import type { NetworkHost } from '../types'
import type { NavigateIntent, NavigateResult } from './navigate'
import type { FilePaneAPI } from './types'

export interface PaneMirrorDeps {
    navigate: (intent: NavigateIntent) => NavigateResult
    getPaneRef: (pane: 'left' | 'right') => FilePaneAPI | undefined
    getPaneVolumeId: (pane: 'left' | 'right') => string
    getPanePath: (pane: 'left' | 'right') => string
    getPaneHistory: (pane: 'left' | 'right') => NavigationHistory
    setPaneHistory: (pane: 'left' | 'right', history: NavigationHistory) => void
    getFocusedPane: () => 'left' | 'right'
    setFocusedPane: (pane: 'left' | 'right') => void
}

export interface PaneMirror {
    copyPathBetweenPanes: (source: 'left' | 'right', target: 'left' | 'right') => void
}

export function createPaneMirror(deps: PaneMirrorDeps): PaneMirror {
    /** Restore focus to a pane after a target-pane state change so the user keeps working where they were. */
    function restoreFocus(pane: 'left' | 'right'): void {
        if (deps.getFocusedPane() !== pane) {
            deps.setFocusedPane(pane)
            // focusedPane persistence fires from the subscriber's focus effect.
        }
    }

    /** Mirror a {volumeId, path} state to a target pane without shifting focus. */
    function mirrorLocalStateToPane(target: 'left' | 'right', volumeId: string, path: string): void {
        const originalFocused = deps.getFocusedPane()
        // Keep the same-volume same-path no-op: routing it through `{ location }`
        // would `navigateToPath(samePath)` — a redundant listing reload with
        // cursor/selection churn. The `{ location }` arm subsumes the other two
        // branches (cross-volume → switch, same-volume different path → in-place).
        if (deps.getPaneVolumeId(target) === volumeId && deps.getPanePath(target) === path) {
            restoreFocus(originalFocused)
            return
        }
        // `source: 'mirror'` keeps focus on the source pane (no focus shift, L1).
        deps.navigate({ pane: target, to: { goTo: { volumeId, path } }, source: 'mirror' })
        restoreFocus(originalFocused)
    }

    /** Mirror a network state ({host, autoMountShare}) to a target pane without shifting focus. */
    function mirrorNetworkStateToPane(
        target: 'left' | 'right',
        host: NetworkHost | null,
        autoMountShare: string | undefined,
    ): void {
        const originalFocused = deps.getFocusedPane()
        const targetPaneRef = deps.getPaneRef(target)
        if (deps.getPaneVolumeId(target) !== 'network') {
            deps.navigate({
                pane: target,
                to: { selectVolume: { volumeId: 'network', path: 'smb://' } },
                source: 'mirror',
            })
        }
        targetPaneRef?.setNetworkHost(host)
        deps.setPaneHistory(
            target,
            pushHistoryEntry(deps.getPaneHistory(target), {
                volumeId: 'network',
                path: 'smb://',
                networkHost: host ?? undefined,
            }),
        )
        targetPaneRef?.setNetworkAutoMount(autoMountShare)
        restoreFocus(originalFocused)
    }

    function copyPathBetweenPanes(source: 'left' | 'right', target: 'left' | 'right'): void {
        if (source === target) return
        const sourcePaneRef = deps.getPaneRef(source)
        if (!sourcePaneRef) return

        const sourceVolumeId = deps.getPaneVolumeId(source)
        const sourcePath = deps.getPanePath(source)
        const sourceHistoryEntry = getCurrentEntry(deps.getPaneHistory(source))
        const sourceHost = sourceHistoryEntry.networkHost ?? null
        const sourceFocused = deps.getFocusedPane() === source

        // Normal listing on the source: cursor-on-folder refines the path.
        if (sourceVolumeId !== 'network') {
            let destPath = sourcePath
            if (sourceFocused) {
                const entry = sourcePaneRef.getCursorEntry()
                if (entry && entry.isDirectory && entry.name !== '..') {
                    destPath = entry.path
                }
            }
            mirrorLocalStateToPane(target, sourceVolumeId, destPath)
            return
        }

        // Source is on the network volume (host list or share list).
        let destHost: NetworkHost | null = sourceHost
        let destAutoMountShare: string | undefined
        if (sourceFocused) {
            const cursor = sourcePaneRef.getNetworkCursorEntry()
            if (cursor?.kind === 'host') {
                destHost = cursor.host
            } else if (cursor?.kind === 'share' && sourceHost) {
                destAutoMountShare = cursor.share.name
            }
        }
        mirrorNetworkStateToPane(target, destHost, destAutoMountShare)
    }

    return { copyPathBetweenPanes }
}
