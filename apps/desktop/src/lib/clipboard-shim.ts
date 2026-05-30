/**
 * E2E-only clipboard shim.
 *
 * Several features copy via `navigator.clipboard.writeText` straight from the
 * webview (file viewer, command-dispatch text copy, the error/crash reporters,
 * the logging panel). That path bypasses the Rust `NSPasteboard` mock, which
 * only intercepts the file-clipboard Tauri commands. So under E2E those writes
 * land on the real OS clipboard and clobber the developer's clipboard with test
 * data (the viewer copy test famously leaves 1024 'A's behind).
 *
 * When `CMDR_E2E_MODE=1`, this swaps `writeText` / `readText` for an in-memory
 * store, so the copy path still runs end to end (handler -> writeText, plus the
 * test's readText round-trip) without ever touching the OS clipboard. Strictly
 * additive: with the flag unset nothing is installed and production uses the
 * real clipboard.
 */

import { isE2eMode } from '$lib/tauri-commands'

let installed = false

/** Installs the in-memory clipboard shim when running under `CMDR_E2E_MODE=1`. */
export async function installClipboardShimIfE2e(): Promise<void> {
  if (installed) return
  if (!(await isE2eMode())) return
  installed = true

  let store = ''
  const writeText = (text: string): Promise<void> => {
    store = text
    return Promise.resolve()
  }
  const readText = (): Promise<string> => Promise.resolve(store)

  // Shadow the methods on the live Clipboard instance so every existing
  // `navigator.clipboard.writeText(...)` call site hits the in-memory store.
  // The Tauri webview is always a secure context, so `navigator.clipboard` is
  // present (the type system agrees: it's non-nullable here).
  Object.defineProperty(navigator.clipboard, 'writeText', { value: writeText, configurable: true, writable: true })
  Object.defineProperty(navigator.clipboard, 'readText', { value: readText, configurable: true, writable: true })
}

/** Test-only: clears the install latch so each test re-evaluates the guard. */
export function _resetClipboardShimForTests(): void {
  installed = false
}
