/**
 * Helpers for composing friendly-error markdown on the frontend.
 *
 * Two concerns:
 * 1. `esc(...)`: escape an untrusted runtime param (path, OS message, device
 *    name, free-form provider text) before it lands in a trusted template. This
 *    is the XSS boundary; see `markdown-escape.ts`.
 * 2. `expandSystemStrings(...)`: substitute `{system_settings}` and friends with
 *    the localized macOS pane labels, mirroring the Rust `system_strings::expand`.
 *    The labels are themselves trusted (they come from the OS loctable via our
 *    own backend), so they are NOT escaped.
 */

import { escapeMarkdown } from './markdown-escape'
import { systemStrings } from '$lib/system-strings.svelte'

/** Escape an untrusted runtime param for safe insertion into a markdown template. */
export function esc(value: string): string {
  return escapeMarkdown(value)
}

/**
 * Replace `{system_settings}`, `{privacy_and_security}`, `{full_disk_access}`,
 * `{files_and_folders}`, `{local_network}`, `{appearance}` with the live
 * localized labels. Mirrors `system_strings::expand` in Rust. Reads the reactive
 * `systemStrings` snapshot so callers get the localized values once hydrated.
 */
export function expandSystemStrings(input: string): string {
  return input
    .replaceAll('{system_settings}', systemStrings.systemSettings)
    .replaceAll('{privacy_and_security}', systemStrings.privacyAndSecurity)
    .replaceAll('{full_disk_access}', systemStrings.fullDiskAccess)
    .replaceAll('{files_and_folders}', systemStrings.filesAndFolders)
    .replaceAll('{local_network}', systemStrings.localNetwork)
    .replaceAll('{appearance}', systemStrings.appearance)
}
