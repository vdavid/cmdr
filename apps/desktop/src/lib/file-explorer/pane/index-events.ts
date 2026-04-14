import { isMacOS } from '$lib/shortcuts/key-capture'
import type { FilePaneAPI } from './types'

/** Ensures a path ends with '/' for correct prefix matching. */
export function ensureTrailingSlash(path: string): string {
    return path.endsWith('/') ? path : path + '/'
}

/**
 * Resolves well-known macOS symlinks to their canonical `/private/` targets.
 * The drive index stores canonical paths (scanner follows symlinks), but the
 * listing uses the raw navigation path. Without this, `index-dir-updated`
 * events for paths under `/tmp/`, `/var/`, or `/etc/` would never match.
 */
export function resolvePrivateSymlinks(path: string): string {
    if (!isMacOS()) return path
    for (const prefix of ['/tmp', '/var', '/etc']) {
        if (path === prefix || path.startsWith(prefix + '/')) {
            return '/private' + path
        }
    }
    return path
}

/** Returns true if any updated path is a descendant of `dir`. */
export function hasDescendantUpdate(paths: string[], dir: string): boolean {
    return paths.some((p) => {
        const withSlash = ensureTrailingSlash(p)
        return withSlash.startsWith(dir) && withSlash !== dir
    })
}

/** Throttled refresh: fires immediately on first relevant event, then skips for the cooldown period. */
export function throttledRefresh(
    shouldRefresh: boolean,
    throttleUntil: number,
    setThrottle: (v: number) => void,
    paneRef: FilePaneAPI | undefined,
    cooldownMs: number,
) {
    if (!shouldRefresh) return
    const now = Date.now()
    if (now < throttleUntil) return
    setThrottle(now + cooldownMs)
    paneRef?.refreshIndexSizes()
}

/**
 * Creates a handler for index directory update events.
 * Returns a function that checks which panes need refreshing and throttles appropriately.
 */
export function createIndexEventHandler(deps: {
    getLeftPath: () => string
    getRightPath: () => string
    getPaneRef: (pane: 'left' | 'right') => FilePaneAPI | undefined
}) {
    const cooldownMs = 2000
    let leftThrottleUntil = 0
    let rightThrottleUntil = 0

    return function handleIndexDirUpdated(paths: string[]) {
        const leftDir = ensureTrailingSlash(resolvePrivateSymlinks(deps.getLeftPath()))
        const rightDir = ensureTrailingSlash(resolvePrivateSymlinks(deps.getRightPath()))

        const refreshLeft = hasDescendantUpdate(paths, leftDir)
        const refreshRight = hasDescendantUpdate(paths, rightDir)

        throttledRefresh(refreshLeft, leftThrottleUntil, (v) => (leftThrottleUntil = v), deps.getPaneRef('left'), cooldownMs)
        throttledRefresh(refreshRight, rightThrottleUntil, (v) => (rightThrottleUntil = v), deps.getPaneRef('right'), cooldownMs)
    }
}
