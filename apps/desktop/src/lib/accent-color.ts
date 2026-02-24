/**
 * System accent color integration.
 *
 * Reads the macOS system accent color from the Rust backend on startup
 * and listens for live changes when the user updates their accent color
 * in System Settings. Sets `--color-accent` on the document root, which
 * drives `--color-accent-hover` and `--color-accent-subtle` via CSS
 * `color-mix()` derivations in app.css.
 */

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('accent-color')

let unlisten: UnlistenFn | undefined

function applyAccentColor(hex: string): void {
    document.documentElement.style.setProperty('--color-accent', hex)
    log.debug('Applied accent color: {hex}', { hex })
}

/**
 * Fetches the system accent color and applies it, then listens for changes.
 * Call once on app startup. If the backend command fails, the CSS fallback
 * in app.css (`#007aff` light / `#0a84ff` dark) stays in effect.
 */
export async function initAccentColor(): Promise<void> {
    try {
        const hex = await invoke<string>('get_accent_color')
        applyAccentColor(hex)
        log.info('System accent color loaded: {hex}', { hex })
    } catch (error) {
        log.warn('Could not read system accent color, using CSS fallback: {error}', { error })
    }

    try {
        unlisten = await listen<string>('accent-color-changed', (event) => {
            applyAccentColor(event.payload)
            log.info('System accent color changed: {hex}', { hex: event.payload })
        })
    } catch (error) {
        log.warn('Could not subscribe to accent color changes: {error}', { error })
    }
}

/** Cleans up the event listener. */
export function cleanupAccentColor(): void {
    unlisten?.()
    unlisten = undefined
    log.debug('Accent color listener cleaned up')
}
